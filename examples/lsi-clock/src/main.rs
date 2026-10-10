#![no_std]
#![no_main]

#[cfg(any(
    feature = "cw32f020c6u7",
    feature = "cw32f030c8t7",
    feature = "cw32a030c8t7"
))]
use embassy_cw32::rtc::{DateTime, DayOfWeek, Rtc};
use embassy_cw32::{self as hal, rcc};

#[cortex_m_rt::entry]
fn main() -> ! {
    // This firmware requires 1.65..5.5 V and ambient -40..105 C. These are
    // example board declarations, not measurements. Qualify the actual board
    // across its complete operating envelope. No external crystal or pin is used.
    let mut config = hal::Config::default();
    config.rcc.operating_conditions = rcc::OperatingConditions {
        min_supply_mv: 1_650,
        max_supply_mv: 5_500,
        min_temperature_c: -40,
        max_temperature_c: 105,
    };
    config.rcc.sys = rcc::Sysclk::LSI;
    let p = hal::try_init(config).unwrap_or_else(|error| stop(error));
    run(p)
}

#[cfg(any(
    feature = "cw32f002f3p7",
    feature = "cw32f002f3u7",
    feature = "cw32f003f4p7",
    feature = "cw32f003f4u7",
    feature = "cw32f003e4p7"
))]
fn run(_p: hal::Peripherals) -> ! {
    // F002/F003 have no RTC. Successful init exposes factory LSI rate bounds for
    // SYSCLK and both buses, while HSI keeps its independent qualification.
    let clocks = rcc::clocks();
    loop {
        core::hint::black_box((
            clocks.sys_bounds(),
            clocks.hclk_bounds(),
            clocks.pclk_bounds(),
            clocks.hsi_bounds(),
        ));
    }
}

#[cfg(any(
    feature = "cw32f020c6u7",
    feature = "cw32f030c8t7",
    feature = "cw32a030c8t7"
))]
fn run(p: hal::Peripherals) -> ! {
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
