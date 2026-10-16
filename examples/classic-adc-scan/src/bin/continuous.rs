//! F030C8T7: low-impedance PA0 input in 0..VDDA; board supply at least 3.3 V.
#![no_std]
#![no_main]
use cortex_m_rt::entry;
use embassy_cw32::{self as hal, adc::{Adc, Config, SampleTime}, time::Hertz};

#[entry]
fn main() -> ! {
    let p = hal::init(Default::default());
    let mut input = p.PA0;
    let mut config = Config::default();
    config.vdda_mv = 3300;
    config.frequency = Hertz(125_000);
    let mut adc = Adc::new(p.ADC, config).unwrap();
    let mut samples = [0; 32];
    match adc.blocking_read_continuous(&mut input, SampleTime::Cycles10, &mut samples) {
        Ok(timing) => { core::hint::black_box((samples, timing)); }
        // Partial contents are deliberately not consumed on failure.
        Err(error) => { core::hint::black_box(error); }
    }
    drop(adc);
    loop { core::hint::spin_loop(); }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop { core::hint::spin_loop(); }
}
