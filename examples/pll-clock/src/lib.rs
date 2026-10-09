#![no_std]
use embassy_cw32::{self as hal, rcc};

/// Board declarations in README.md must hold for this firmware's lifetime.
pub fn config() -> hal::Config {
    let mut config = hal::Config::default();
    config.rcc.operating_conditions = rcc::OperatingConditions {
        min_supply_mv: if cfg!(feature = "low-voltage") {
            1_650
        } else {
            3_000
        },
        max_supply_mv: if cfg!(feature = "low-voltage") {
            1_790
        } else {
            3_600
        },
        min_temperature_c: -20,
        max_temperature_c: 70,
    };
    config.rcc.hsi.div = if cfg!(feature = "fractional") {
        rcc::HsiDiv::Div10
    } else {
        rcc::HsiDiv::Div6
    };
    config.rcc.pll = Some(rcc::Pll {
        src: rcc::PllSource::HSI,
        mul: if cfg!(feature = "fractional") {
            rcc::PllMul::Mul12
        } else {
            rcc::PllMul::Mul7
        },
    });
    config.rcc.sys = rcc::Sysclk::PLL;
    if cfg!(feature = "low-voltage") {
        config.rcc.ahb_pre = rcc::AHBPrescaler::Div4;
    }
    // Long startup-cycle wait is selected internally; this is a poll budget,
    // not a microsecond guarantee across incoming and bridge clock choices.
    config.rcc.timeout = 2_000_000;
    config
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    cortex_m::interrupt::disable();
    loop {
        cortex_m::asm::wfi();
    }
}
