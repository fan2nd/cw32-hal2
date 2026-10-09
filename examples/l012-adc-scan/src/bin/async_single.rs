//! ADC1 EOS-driven single reads with an independent blocking ADC2 neighbor.
#![no_std]
#![no_main]
use embassy_cw32::{
    self as hal,
    adc::{Adc, Common, Config, InterruptHandler, SampleTime},
    bind_interrupts, peripherals,
    time::Hertz,
};
use embassy_time::{Duration, Timer, with_timeout};

bind_interrupts!(struct Irqs {
    ADC1 => InterruptHandler<peripherals::ADC1>;
});

#[embassy_executor::main]
async fn main(_spawner: embassy_executor::Spawner) {
    // Change these board declarations before use. L012 VDDA must equal VDD.
    let mut system = hal::Config::default();
    system.rcc.operating_conditions.min_supply_mv = 3000;
    system.rcc.operating_conditions.max_supply_mv = 3600;
    system.rcc.hsi.div = hal::rcc::HsiDiv::Div8;
    let p = hal::init(system);
    let common = Common::try_new(p.BGR, 100_000).unwrap();
    let mut config = Config::default();
    config.vdda_mv = 3000;
    config.frequency = Hertz(24_000_000);
    // Config.timeout only bounds synchronous register polls, not the await.
    let mut adc = Adc::try_new_async(p.ADC1, &common, Irqs, config).unwrap();
    let mut adc2 = Adc::try_new(p.ADC2, &common, config).unwrap();
    let mut sibling = p.PA8;
    let mut input = p.PA0;
    loop {
        match with_timeout(
            Duration::from_millis(100),
            adc.read(&mut input, SampleTime::Cycles102),
        )
        .await
        {
            Ok(result) => {
                // HAL faults remain distinct from the caller's deadline.
                let sample = result.unwrap();
                core::hint::black_box((sample, adc.timing()));
            }
            Err(_) => {
                // Dropping the read masks EOS, requests START=0 and resets the
                // sequence. The next read reinitializes after a successful stop.
                // A terminal cleanup fault is reported by that next HAL call.
                core::hint::black_box("caller deadline elapsed");
            }
        }
        // ADC1 completion or cancellation leaves the independent ADC2 usable.
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
