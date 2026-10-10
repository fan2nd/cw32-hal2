# Factory LSI system clock

This ordinary firmware selects `Sysclk::LSI` and exposes the resulting rate
bounds. Choose one exact part with `--no-default-features`:

- CW32F002F3P7, CW32F002F3U7, CW32F003F4P7, CW32F003F4U7 or CW32F003E4P7:
  system/bus bounds only. F002/F003 have no RTC and share the existing branch
  that uses no `LsiClock`, `CalendarClock` or calendar API. Generic F002/F003
  aliases are not qualified for this mode.
- CW32F020C6U7, CW32F030C8T7, CW32A030C8T7, CW32L031C8T6,
  CW32L031C8U6, CW32L031F8U6 or CW32R031C8U6: the existing calendar branch also
  acquires the same factory source through `LsiClock` and `CalendarClock::Lsi`,
  initializes an unset RTC and polls its date/time and source/divided bounds.
  Its demonstration epoch is 2026-10-09 00:00:00; replace it with the intended
  time. An already-running compatible calendar retains its date/time.

The default feature remains CW32F030C8T7. The firmware requires no GPIO wiring,
external oscillator, UART baud or other board peripherals. Support is init-only;
it does not provide runtime switching or sleep/wake recovery. Generic L031,
its other packages, generic R031 and every W031 are excluded from LSI SYSCLK.

## Board declarations and rate bounds

The example declares VDD 1.65–5.5 V and ambient temperature −40–85°C on the
three L031 parts. R031C8U6 explicitly declares 2.2–3.6 V and −40–85°C;
the other existing example families retain −40–105°C. These are
source-qualified limits, not measurements or proof about a particular
board. Qualify the board across its actual complete operating envelope and
adjust the declarations accordingly before hardware use. Default AHB/APB
prescalers remain /1. F002/F003/L031/R031 retain factory HSIOSC /6, nominal 8 MHz, on
successful initialization; selecting LSI does not replace its independent
qualification.

The nominal LSI rate is 32,800 Hz. Factory full-range bounds are 31,160–34,440 Hz
for F002/F020 and 31,816–33,784 Hz for F003/F030/A030 and exact L031/R031. F003's bounds come from its
own datasheet Rev1.9, PDF pages 32 and 38: factory ±3% over the declared full
temperature range, independently of the other families. L031 uses its own DS
Rev1.9, PDF pages 38 and 47, with ±3% only over −40–85°C; the 25°C-only ±1%
row and conditional low-dissipation 105°C extension do not expand this envelope.
R031 uses its own DS CN V1.2 PDF54/printed53 table7-23 under PDF42/printed41
table7-4: ±3% over −40–85°C and 2.2–3.6 V; its 25°C-only ±1% row does not
expand that range. VDDA=VDD, VDDRF and all ground connections must satisfy
the actual board requirements; no firmware RF-power-state check supplies this proof.
SYSCLK/HCLK/PCLK
bounds are rate-only; retained HSI keeps its own timing qualification. Exact
source bounds and divisors are retained through prescaling. For example, F003
AHB /128 and APB /8 would expose PCLK nominal/minimum/maximum 32/31/33 Hz while
retaining divisor 1024 internally; the example itself uses /1.

The calendar's nominal tick rate is 32800/32768 Hz, not a precision one-second
claim. Exact L031 and CW32R031C8U6 RTC LSI aliases are rate-only under every
SYSCLK, including HSI, HSE and LSE. Generic R031, every W031, other L031
packages and board-qualified LSE keep their prior
qualification. Strict cycle-duration helpers are not used. ADC timing
rejects these rate-only system clocks, and the fixed 1 MHz time driver rejects
selected LSI before singleton acquisition or RCC access. This crate enables
no time driver. AWT retains its independent HSIOSC timing qualification; F003's
ATIM/IR presence does not grant new peripheral modes or timing guarantees.

## Initialization and failure effects

Selecting the mode permits bounded whole-GPIOA/B/C inspection windows on
F002/F003, and whole-GPIOA/B/C/F windows on the classic and exact L031/R031 parts. Sampling, filters
and armed events may advance, including before failure. Configuration and
locks are preserved; software does not clear flags, but flags can change
naturally. Restoring gates cannot undo this progress. Existing exclusive
clock/memory handover and a continuously legal execution clock remain required;
a critical section does not freeze hardware. System/bus clock changes also
change downstream timing, including F003 ATIM/IR, despite retained configuration.
A cold source with retained LSI-selected consumers or ready observers is
rejected; an already-running factory-matching source is reused without
TRIM/WAIT writes. No ready flag is cleared to obtain admission or after a new
start.

Exact L031/R031 additionally preserve configurable CCS/LSELOCK and inherited
HSE/LSE source/pad owners. Cold admission checks nine gates and eleven selectors,
including RTC and UART1/2/3 regardless of their local enable state. GPIOB FILTER
and PB11 AF share one inspection window. F8U6 has no bonded PB11; inspecting
its register is conservative policy. FILTER7 is documented AWT overflow and
is conservatively rejected; MCO7 is separately undocumented. Matching cold
trim does not bypass admission. The retained configured HSI tree must remain
legal with the final dividers and determines the conservative Flash wait along
with LSI. L031F8U6 still cannot request HSE.

On R031, RFCLK has its independent dedicated 16 MHz oscillator. Selected LSI
can nevertheless affect RF host traffic through PCLK, the GPIOA working gate
and PA00..PA03. Finish/quiet host transfers and permit the whole-bank interval;
restoration or failure cannot promise unchanged RF signals or packets. Keep
inherited RF XTAL_OCLK available if it feeds HSE bypass. These are functional
limits of safe initialization, with no hidden Rust memory-safety precondition.
The initializer performs no RF register access, SPI command, RF reset, power
change, event clear or RF interrupt inspection.

An error stops at a debugger breakpoint. A failed hardware initialization can
leave attempted TRIM, an enabled inspection gate, conservative Flash/bus
guards or a permanent LSI request. HSI-calibration failure can leave execution
on the LSI bridge with HSI stopped or incompletely restarted. A partial HEX
configuration may also remain. RCC failure publishes no clocks or peripheral
tokens. No hardware error promises rollback; reset before retrying. Ready
status is a startup latch, not running clock-loss detection, and poll budgets
are not microseconds. Loss of the execution clock may prevent a return. Read
the complete [F002 contract](../../docs/f002-factory-lsi-sysclk.md),
[F003 contract](../../docs/f003-factory-lsi-sysclk.md),
[L031 contract](../../docs/l031-factory-lsi-sysclk.md),
[R031 contract](../../docs/qualified-r031-lsi-sysclk.md), or
[classic contract](../../docs/factory-lsi-sysclk.md) before hardware use.

## Ordinary firmware builds

From the repository root:

```sh
cargo build --release --locked --manifest-path examples/lsi-clock/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32f002f3p7,defmt
cargo build --release --locked --manifest-path examples/lsi-clock/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32f002f3u7,defmt
cargo build --release --locked --manifest-path examples/lsi-clock/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32f003f4p7
cargo build --release --locked --manifest-path examples/lsi-clock/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32f003f4u7,defmt
cargo build --release --locked --manifest-path examples/lsi-clock/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32f003e4p7,defmt
cargo build --release --locked --manifest-path examples/lsi-clock/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32f020c6u7,defmt
cargo build --release --locked --manifest-path examples/lsi-clock/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32f030c8t7,defmt
cargo build --release --locked --manifest-path examples/lsi-clock/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32a030c8t7,defmt
cargo build --release --locked --manifest-path examples/lsi-clock/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32l031c8t6,defmt
cargo build --release --locked --manifest-path examples/lsi-clock/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32l031c8u6,defmt
cargo build --release --locked --manifest-path examples/lsi-clock/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32l031f8u6,defmt
cargo build --release --locked --manifest-path examples/lsi-clock/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32r031c8u6,defmt --bin cw32-lsi-clock-example
```

`build.rs` derives `memory.x` from the exact package's generated memory facts
and passes `-Tlink.x`. F002's own datasheet Rev1.2 tables 3-1 and 6-1 specify
16 KiB Flash at 0x00000000 and 2 KiB SRAM at 0x20000000 for both packages. F003's
own datasheet Rev1.9 PDF pages 5, 8, 27 and 62 specify 20 KiB Flash at
0x00000000 and 3 KiB SRAM at 0x20000000 for all three exact packages. The F4
packages also match their DFP device entries; E4P7 is supported by the datasheet
without a matching DFP device. L031's own DS Rev1.9 PDF pages 10, 32 and 77–78
specify 64 KiB Flash at 0x00000000 and 8 KiB SRAM at 0x20000000 for the three
exact packages. R031C8U6 uses its own DS CN V1.2 PDF11/35 and own PDSC
for the same exact 64 KiB Flash / 8 KiB SRAM at those bases. The build script
asserts each package's limits and RTC presence, while
preserving F002/F003's own limits and RTC absence.
Release uses size optimization, LTO and one codegen
unit. A Flash overflow must be resolved within the real capacity.

The `defmt` feature exercises HAL formatting implementations without requiring
a logging transport. These commands are a build recipe, not evidence that they
have run. Compilation and ELF inspection establish source/link integration
only; actual load segments, Flash/RAM use, remaining stack space, entry and
vectors must be inspected before claiming a binary fits. No hardware startup,
electrical rate, RTC operation, real-time behavior or runtime error-path
validation is claimed.
