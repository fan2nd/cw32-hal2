#![no_std]
use embassy_cw32::{self as hal, rcc, time::Hertz};

/// Demonstration board assertions, not measurements or a certified BOM.
/// Both modes require every cycle within this envelope throughout these conditions.
pub fn config(mode: rcc::LseMode) -> hal::Config {
    let board = rcc::OperatingConditions {
        min_supply_mv: 3_000,
        max_supply_mv: 3_600,
        min_temperature_c: -20,
        max_temperature_c: 70,
    };
    let mut config = hal::Config::default();
    config.rcc.operating_conditions = board;
    config.rcc.lse = Some(rcc::Lse {
        min_freq: Hertz(32_766),
        max_freq: Hertz(32_770),
        operating_conditions: board,
        mode,
        // Crystal board/load must qualify these settings. Bypass keeps explicit
        // parameters too, so later exact-source reuse checks the same tuple.
        drive: rcc::LseDrive::Strong,
        amplitude: rcc::LseAmplitude::Normal,
        wait: rcc::LseWait::Cycles16384,
        poll_budget: 20_000_000,
    });
    config.rcc.sys = rcc::Sysclk::LSE;
    // HSI /6, AHB /1 and APB /1 retain their existing defaults. Initialization
    // prepares detector LSI internally; there is no second source declaration.
    config
}

pub fn run(rtc: hal::rtc::Rtc<'_>) -> ! {
    loop {
        core::hint::black_box((
            hal::rcc::clocks().sys_bounds(),
            rtc.now().unwrap(),
            rtc.source_clock_bounds(),
            rtc.calendar_tick_bounds(),
        ));
    }
}

pub fn initial_time() -> hal::rtc::DateTime {
    // Demonstration epoch only. Replace it before using this as a clock product.
    hal::rtc::DateTime::from(2026, 10, 9, hal::rtc::DayOfWeek::Friday, 12, 0, 0, 0).unwrap()
}
