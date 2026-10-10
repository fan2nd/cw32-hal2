# Factory LSI system clock

This ordinary firmware selects `Sysclk::LSI` and exposes the resulting rate
bounds. Choose one exact part with `--no-default-features`:

- CW32F002F3P7 or CW32F002F3U7: system/bus bounds only. F002 has no RTC and this
  branch uses no `LsiClock`, `CalendarClock` or calendar API.
- CW32F020C6U7, CW32F030C8T7 or CW32A030C8T7: the existing calendar branch also
  acquires the same factory source through `LsiClock` and `CalendarClock::Lsi`,
  initializes an unset RTC and polls its date/time and source/divided bounds.
  Its demonstration epoch is 2026-10-09 00:00:00; replace it with the intended
  time. An already-running compatible calendar retains its date/time.

The default feature remains CW32F030C8T7. Neither branch uses GPIO, an external
oscillator, UART baud or other unnecessary board peripherals.

## Board declarations and rate bounds

The example declares VDD 1.65–5.5 V and ambient temperature −40–105°C. These
are source-qualified limits, not measurements or proof about a particular
board. Qualify the board across its actual complete operating envelope and
adjust the declarations accordingly before hardware use. Default AHB/APB
prescalers remain /1. F002 retains factory HSIOSC /6, nominal 8 MHz, on
successful initialization; selecting LSI does not replace its independent
qualification.

The nominal LSI rate is 32,800 Hz. Factory full-range bounds are 31,160–34,440 Hz
for F002/F020 and 31,816–33,784 Hz for F030/A030. F002's SYSCLK/HCLK/PCLK bounds
are rate-only; retained HSI keeps its own timing qualification. The classic
calendar's nominal tick rate is 32800/32768 Hz, not a precision one-second
claim. Strict cycle-duration helpers are not used. ADC timing rejects these
rate-only system clocks, and the fixed 1 MHz time driver rejects selected LSI
before singleton acquisition or RCC access. This crate enables no time driver.

## Initialization and failure effects

Selecting the mode permits bounded whole-GPIOA/B/C inspection windows on F002,
and whole-GPIOA/B/C/F windows on the classic parts. Sampling, filters and armed
events may advance, including before failure. Configuration and locks are
preserved; software does not clear flags, but flags can change naturally.
Restoring gates cannot undo this progress. A cold source with retained
LSI-selected consumers or ready observers is rejected; an already-running
factory-matching source is reused without TRIM/WAIT writes. No ready flag is
cleared to obtain admission or after a new start.

An error stops at a debugger breakpoint. A failed hardware initialization can
leave attempted TRIM, an enabled inspection gate, conservative Flash/bus
guards or a permanent LSI request. HSI-calibration failure can leave execution
on the LSI bridge with HSI stopped or incompletely restarted. No hardware error
promises rollback; reset before retrying. Read the complete
[F002 contract](../../docs/f002-factory-lsi-sysclk.md) or
[classic contract](../../docs/factory-lsi-sysclk.md) before hardware use.

## Ordinary firmware builds

From the repository root:

```sh
cargo build --release --locked --manifest-path examples/lsi-clock/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32f002f3p7,defmt
cargo build --release --locked --manifest-path examples/lsi-clock/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32f002f3u7,defmt
cargo build --release --locked --manifest-path examples/lsi-clock/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32f020c6u7,defmt
cargo build --release --locked --manifest-path examples/lsi-clock/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32f030c8t7,defmt
cargo build --release --locked --manifest-path examples/lsi-clock/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32a030c8t7,defmt
```

`build.rs` derives `memory.x` from the exact package's generated memory facts
and passes `-Tlink.x`. F002's own datasheet Rev1.2 tables 3-1 and 6-1 specify
16 KiB Flash at 0x00000000 and 2 KiB SRAM at 0x20000000 for both packages; the
build script asserts these limits. Release uses size optimization, LTO and one
codegen unit. A Flash overflow must be resolved within the real capacity.

The `defmt` feature exercises HAL formatting implementations without requiring
a logging transport. These commands are a build recipe, not evidence that they
have run. Compilation and ELF inspection establish source/link integration
only. No hardware startup, electrical rate, RTC operation or runtime error-path
validation is claimed.
