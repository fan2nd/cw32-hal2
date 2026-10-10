#![no_std]
#![no_main]

use embassy_cw32::{
    self as hal, rcc,
    rtc::{DateTime, DayOfWeek, Rtc},
};

#[cortex_m_rt::entry]
fn main() -> ! {
    // This firmware requires 1.65..5.5 V and ambient -40..105 C. These are
    // board conditions, not measurements. No external crystal or pin is used.
    let mut config = hal::Config::default();
    config.rcc.operating_conditions = rcc::OperatingConditions {
        min_supply_mv: 1_650,
        max_supply_mv: 5_500,
        min_temperature_c: -40,
        max_temperature_c: 105,
    };
    config.rcc.sys = rcc::Sysclk::LSI;
    let p = hal::try_init(config).unwrap_or_else(|error| stop(error));
    let lsi = rcc::LsiClock::new(p.SYSCTRL, 100_000).unwrap_or_else(|error| stop(error));
    core::hint::black_box((rcc::clocks().sys_bounds(), lsi.bounds()));
    let clock = rcc::CalendarClock::Lsi(lsi);
    core::hint::black_box(clock.bounds());
    // An unset calendar is provisioned with an explicit demonstration epoch.
    // Replace it with the intended time before using this as a clock product.
    // An already-running compatible calendar retains its existing date/time.
    let epoch = DateTime::from(2026, 10, 9, DayOfWeek::Friday, 0, 0, 0, 0).unwrap();
    let rtc = Rtc::initialize_if_unset(p.RTC, clock, Default::default(), epoch)
        .unwrap_or_else(|error| stop(error));
    loop {
        // Bounds remain rate-only through the calendar's exact 32768 divisor.
        // No strict minimum/maximum-duration helper or 1 MHz time driver is used.
        core::hint::black_box((
            rtc.now().unwrap_or_else(|error| stop(error)),
            rtc.source_clock_bounds(),
            rtc.calendar_tick_bounds(),
        ));
    }
}

fn stop<E>(error: E) -> ! {
    core::hint::black_box(error);
    loop {
        cortex_m::asm::bkpt();
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {
        cortex_m::asm::bkpt();
    }
}
