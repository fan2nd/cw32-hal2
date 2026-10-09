//! ADC1 mixed slots, cancellation/reuse, and an independent blocking ADC2.
#![no_std]
#![no_main]
use embassy_cw32::{
    self as hal,
    adc::{Adc, AdcChannel, Common, Config, InterruptHandler, SampleTime},
    bind_interrupts, peripherals,
    time::Hertz,
};
use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Timer, with_timeout};

bind_interrupts!(struct Irqs {
    ADC1 => InterruptHandler<peripherals::ADC1>;
});

#[embassy_executor::main]
async fn main(_spawner: embassy_executor::Spawner) {
    // Example board: actual supply 3.0–3.6 V, ambient -40..85 C, HSI /8.
    // L012 VDDA must equal VDD. PA0/PA1 must remain within the supply rails.
    let mut system = hal::Config::default();
    system.rcc.operating_conditions.min_supply_mv = 3000;
    system.rcc.operating_conditions.max_supply_mv = 3600;
    system.rcc.hsi.div = hal::rcc::HsiDiv::Div8;
    let p = hal::init(system);
    let common = Common::try_new(p.BGR, 100_000).unwrap();
    let mut config = Config::default();
    config.vdda_mv = 3000;
    config.frequency = Hertz(24_000_000);
    let mut adc = Adc::try_new_async(p.ADC1, &common, Irqs, config).unwrap();
    let mut adc2 = Adc::try_new(p.ADC2, &common, config).unwrap();
    let mut sibling = p.PA8;
    let mut first = p.PA0;
    let mut second = p.PA1;
    let temperature = adc.enable_temperature().degrade_adc();
    let bandgap = adc.enable_vrefint().degrade_adc();
    loop {
        {
            let a = first.reborrow_adc();
            let b = second.reborrow_adc();
            let sequence = [
                (&a, SampleTime::Cycles54),
                (&b, SampleTime::Cycles102),
                (&temperature, SampleTime::Cycles518),
                (&bandgap, SampleTime::Cycles518),
                (&a, SampleTime::Cycles518),
                (&b, SampleTime::Cycles54),
                (&temperature, SampleTime::Cycles518),
                (&bandgap, SampleTime::Cycles518),
            ];
            let mut samples = [0xffff; 8];
            // A deliberately short caller deadline demonstrates ordinary select
            // composition. Either outcome is valid; no IRQ latency is promised.
            let timing = match select(
                adc.read_sequence(&sequence, &mut samples),
                Timer::after_micros(50),
            )
            .await
            {
                Either::First(result) => Some(result.unwrap()),
                Either::Second(()) => None,
            };
            // The losing future has been dropped. A canceled scan leaves every
            // output element unchanged; completed results retain caller order.
            core::hint::black_box((samples, timing));

            // Reuse the same owner, channels and output after completion or
            // successful logical cancellation. The HAL reestablishes fresh state.
            match with_timeout(
                Duration::from_millis(100),
                adc.read_sequence(&sequence, &mut samples),
            )
            .await
            {
                Ok(result) => {
                    let timing = result.unwrap();
                    core::hint::black_box((samples, timing));
                }
                Err(_) => {
                    core::hint::black_box("caller deadline elapsed");
                }
            }
        }
        // Pin/sequence borrows ended. Single conversion uses one slot again.
        let result = with_timeout(
            Duration::from_millis(100),
            adc.read(&mut first, SampleTime::Cycles102),
        )
        .await;
        let _ = core::hint::black_box(result);
        let neighbor = adc2
            .blocking_read(&mut sibling, SampleTime::Cycles262)
            .unwrap();
        core::hint::black_box(neighbor);
        Timer::after_millis(10).await;
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
