> Historical Stage11 layout checkpoint: the later user correction replaced the blanket `module/mod.rs` rule with flat leaf files. The test deletions and production-equivalence results below remain valid for their recorded snapshot. See [current leaf layout](rust-leaf-layout.md).

# HAL production layout

Date: 2026-10-08. Reference: the checked-out `embassy-stm32` at Embassy commit
`f16efeffe37581092ec184718e6fdb1620393214`.

The HAL now contains production modules only. The requested `<module>/mod.rs`
convention is retained. Tests were deleted, not moved into another directory.
No replacement HAL test framework was introduced.

## What changed

- Deleted 42 Rust test-only files, including both integration-test roots, register
  models, shared test helpers and fixtures; also removed inline test modules and
  test-only constructors/fields/branches.
- Deleted five HAL `compile_fail` documentation fixtures and the build script's
  generated ADC pin-mux test. Runtime assertions and normal API documentation
  remain.
- Deleted 47 HAL-only Python compile/fixture/link engines, 11 dedicated HAL test
  pipelines and 26 stale bytecode files. Eleven historical copied HAL Rust
  files were removed from the active tree; historical reports/logs remain.
- Removed the HAL's `critical-section/std` dev dependency. The SPI source-evidence
  verifier still verifies original PDF identities/pages but no longer calls HAL
  tests. Data/PAC/generator/provenance and hardware-source fact checks remain.
- Consolidated the classic ADC driver at `adc/classic/mod.rs`, with its register
  adapter and variant helpers under `adc/classic/registers/`. This eliminates
  the cross-directory `classic_driver` → `classic` implementation split.
- RCC and I2C now select private `_version` modules behind their public facades,
  as in the upstream version-selection pattern. Public APIs and cfg selections
  are unchanged. Driver mode markers have their own `mode/mod.rs`.
- HAL CI now builds all declared ARM library selections and genuine firmware
  examples. It contains no HAL host tests or synthetic fixture generation.

The exact deletion inventory, before/after source hashes and module moves are in
[`hal-production-layout.json`](hal-production-layout.json).

## Mapping to pinned embassy-stm32

| CW32 production area | Upstream organization used as reference | Intentional CW32 differences |
| --- | --- | --- |
| `lib.rs` | `src/lib.rs` peripheral exports and initialization | CW32 cfgs, singleton generation and bounded HSI initialization |
| `adc/mod.rs`, `classic/`, `sequence/`, `l012/` | `adc/mod.rs` plus hardware-version modules | Real CW32 classic, one-slot sequence and dual-owner implementations; no fictitious STM32 v1/v2/v3 register mapping |
| `crc/mod.rs`, `backend/` | `crc/mod.rs` and version implementations | Shared CW32 fixed-preset adapter; only documented widths/polynomials |
| `dma/mod.rs` | `dma/mod.rs` and selected controllers | CW32 unsafe one-shot implementation; no DMA mux or ringbuffer claim |
| `exti/mod.rs`, `engine/`, `registers/`, `irq_groups/` | `exti/mod.rs`, `low_level.rs`, `blocking.rs` | GPIO-bank interrupts and CW32 clear semantics require distinct internals |
| `flash/mod.rs`, `backend/` | `flash/mod.rs`, `common.rs`, family implementations | Existing reserved-region API/adapter retained; no async, automatic partitioning or unsupported `NorFlash` promise |
| `gpio/mod.rs`, family backends | Upstream `gpio.rs` | Family directories retain CW32 pad, clock, mux and interrupt distinctions |
| `i2c/mod.rs`, `classic/`, `lpi2c/` | `i2c/mod.rs` and selected `_version` | CW32 state-code and L012 command/FIFO protocols; per-version configuration/timing remains with its actual hardware |
| `rcc/mod.rs`, family backends | `rcc/mod.rs` and selected `_version` | CW32 SYSCTRL, qualified HSI bounds, operating conditions and RTC oscillator policy; no STM32 RCC register fiction |
| `rtc/mod.rs`, `datetime/` | `rtc/mod.rs`, `datetime.rs` and versions | Supported CW32 blocking calendar subset only |
| `spi/mod.rs` | `spi/mod.rs` | CW32 blocking implementation; no unsupported ringbuffer or DMA modules |
| `timer/mod.rs`, `low_level/`, `simple_pwm/` | `timer/mod.rs`, `low_level.rs`, `simple_pwm.rs` | CW32 `btim`, classic and buffered adapters retained; no unsupported capture/encoder/time-driver modules |
| `usart/mod.rs`, `variant/`, `shared_irq/` | `usart/mod.rs` with optional capability submodules | CW32 register variants and shared vectors; no unsupported buffered/DMA API |
| `wdg/mod.rs`, `windowed/` | `wdg/mod.rs` | CW32 independent and window-watchdog modules and timing limits |
| `time/mod.rs`, `macros/mod.rs` | `time.rs`, `macros.rs` | File-backed directory spelling follows the explicit user convention |
| `mode/mod.rs` | Inline `mode` in upstream `lib.rs` | Intentionally file-backed here; marker semantics unchanged |

This is organizational alignment, not a claim of feature parity. Upstream often
uses flat `.rs` files; all owned CW32 file-backed modules intentionally continue
to use `<module>/mod.rs`. Small sealed/type namespaces remain inline. Existing
production register abstractions remain because runtime drivers use them.

## Production equivalence

A temporary external Rust AST tool compared the Stage10 source snapshot with
this tree using `syn 2.0.119`. It evaluated `test=false`, removed only test-only
nodes, preserved all remaining hardware cfg branches, expanded file modules,
and compared normalized non-documentation tokens. Only declared private RCC/I2C
facade names and resolved module paths were normalized.

The expanded production tree matches exactly; its canonical SHA-256 is
`49a2a1f2c7a589f1f9a14688ae66f9be8c5737fd049f647e1be67f4e98869277`.
The five cfg-attribute-selected timer adapters were also compared separately.
The build script matches after removing only its generated test-emission block.
The comparator and temporary source copies are outside the project, not CI.

Three mixed `cfg(any(test, ...))` attributes in RCC clock bounds now retain the
L010/L011/L012 hardware predicates without `test`. Flash's RAM-emulation field
and branches were removed while its ARM register/array operations stayed intact.
All 112 baseline HAL Rust files matched the intact Stage10 archive before edits.
The resulting HAL has 71 Rust files, including `build.rs` and `lib.rs`, and
661,874 source bytes, down from 1,278,451. File count includes the newly extracted
production mode module.

## Verification and historical boundary

Current command: `./ci/check-hal.sh`. It builds all 54 declared selections with
`rt` and `rt,defmt` for `thumbv6m-none-eabi`, then links the existing F030 blocking
and asynchronous GPIO firmware examples. No firmware is flashed or executed.
All 108 ARM builds and both examples passed on the final source with zero
warnings. All 895 recorded input files remained unchanged (manifest SHA-256
`9fa90d5eeff9ce41cc6dc551fa1c3a5ed95bbe533ab4f349563f4214393bb929`).
The final frozen command log and full before/after input manifests are under
`verification-logs/hal-production-layout/final-arm-and-examples/`.

Source-fact checks retained their original evidence scope: ADC (25 original
source hashes), I2C (27 PDFs), RCC (25 original sources), and SPI (26 family
manual/datasheet identities and page extractions) passed after cleanup. The
repository module-layout check and 17 provenance tests passed as well.

Earlier reports, counts, commands and removed paths are historical evidence for
their dated checkpoints. They do not mean the deleted tests remain available or
that current code has been re-tested by them. Their reviewed source facts have
not been rewritten. Recovery remains the intact
`embassy-cw32-stage10-2026-10-08.zip`; no archives or Library versions were deleted.
Hardware validation and the previously documented implementation limitations
remain outstanding. Nothing was published.
