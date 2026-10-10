# Factory LSI system clock and calendar

This ordinary firmware selects `Sysclk::LSI`, acquires the same factory source
through `LsiClock` and `CalendarClock::Lsi`, and initializes an unset RTC before
polling calendar values and exact source/divided rate bounds. It uses no GPIO,
external oscillator, UART baud or other unnecessary board peripheral. The
example epoch is 2026-10-09 00:00:00; replace it with the intended time. An already
running compatible calendar retains its date/time.

Use one exact part: CW32F020C6U7, CW32F030C8T7 or CW32A030C8T7. Board VDD must be
1.65–5.5 V and ambient temperature −40–105°C; these are declarations, not
measurements. At nominal 32,800 Hz, the factory full-range bounds are
31,160–34,440 Hz for F020 and 31,816–33,784 Hz for F030/A030. The calendar's
nominal tick rate is 32800/32768 Hz, so it is not a precision one-second claim.
All shown LSI bounds are rate-only. No strict cycle-duration helper is called.

Selecting the mode permits bounded whole-GPIOA/B/C/F inspection windows. Sampling,
filters and armed events may advance, including before failure. Configuration
and flags are preserved; restoring gates cannot undo events. Follow the
[complete initialization and failure contract](../../docs/factory-lsi-sysclk.md).
A cold source with retained LSI-selected consumers is rejected; already-running
factory-matching LSI is reused. An error stops at a debugger breakpoint. A failed hardware init requires reset.

Build from the repository root, with the verified Rust 1.99.0 toolchain:

```sh
cargo build --release --locked --manifest-path examples/lsi-clock/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32f020c6u7,defmt
cargo build --release --locked --manifest-path examples/lsi-clock/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32f030c8t7,defmt
cargo build --release --locked --manifest-path examples/lsi-clock/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32a030c8t7,defmt
```

`build.rs` derives memory.x from that exact package's generated memory facts and
passes `-Tlink.x`. The defmt feature exercises HAL formatting implementations;
this example does not install or require a logging transport. It does not use
the fixed 1 MHz time driver, which cannot divide this LSI PCLK. Compilation and
ELF inspection establish source/link integration only, not hardware startup,
RTC operation, physical rate or runtime error-path execution.
