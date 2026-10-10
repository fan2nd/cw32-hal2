#![no_std]
#![no_main]

#[cfg(any(
    feature = "cw32f020c6u7",
    feature = "cw32f030c8t7",
    feature = "cw32a030c8t7",
    feature = "cw32l031c8t6",
    feature = "cw32l031c8u6",
    feature = "cw32l031f8u6",
    feature = "cw32r031c8u6",
    feature = "cw32w031r8u6",
    feature = "cw32l052c8t6",
    feature = "cw32l052r8s6",
    feature = "cw32l052r8t6"
))]
use embassy_cw32::rtc::{DateTime, DayOfWeek, Rtc};
use embassy_cw32::{self as hal, rcc};

#[cortex_m_rt::entry]
fn main() -> ! {
    // W031 requires 2.0..3.6 V; R031 retains 2.2..3.6 V, both at -40..85 C.
    // L031 and exact L052 declare 1.65..5.5 V and -40..85 C.
    // Other prior example parts retain 1.65..5.5 V and -40..105 C.
    // These board declarations are not measurements. Qualify the actual board
    // across its complete operating envelope. No external crystal or pin is used.
    let mut config = hal::Config::default();
    config.rcc.operating_conditions = rcc::OperatingConditions {
        min_supply_mv: if cfg!(feature = "cw32r031c8u6") {
            2_200
        } else if cfg!(feature = "cw32w031r8u6") {
            2_000
        } else {
            1_650
        },
        max_supply_mv: if cfg!(any(feature = "cw32r031c8u6", feature = "cw32w031r8u6")) {
            3_600
        } else {
            5_500
        },
        min_temperature_c: -40,
        max_temperature_c: if cfg!(any(
            feature = "cw32l031c8t6",
            feature = "cw32l031c8u6",
            feature = "cw32l031f8u6",
            feature = "cw32r031c8u6",
            feature = "cw32w031r8u6",
            feature = "cw32l052c8t6",
            feature = "cw32l052r8s6",
            feature = "cw32l052r8t6"
        )) {
            85
        } else {
            105
        },
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
    feature = "cw32a030c8t7",
    feature = "cw32l031c8t6",
    feature = "cw32l031c8u6",
    feature = "cw32l031f8u6",
    feature = "cw32r031c8u6",
    feature = "cw32w031r8u6",
    feature = "cw32l052c8t6",
    feature = "cw32l052r8s6",
    feature = "cw32l052r8t6"
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
