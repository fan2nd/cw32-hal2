> Historical software-verification references: HAL tests and fixture harnesses were deleted on 2026-10-08. Counts and commands below apply to the dated source snapshot. See [current HAL layout and build scope](hal-production-layout.md).

# Stage 6: GPIO interrupts, CRC and window watchdogs

This checkpoint adds software-validated typed asynchronous GPIO across all 13
families, documented CRC presets across all 13, and polling window watchdogs on
the 11 families that contain that peripheral. L010/L011 have no WWDT driver,
token or PAC symbol. Nothing was flashed or executed on hardware.

## Implemented behavior

- GPIO IRQ ownership is per pin, with complete shared-vector bindings on L052
  and L083. Inactive banks are not touched, including clock-gated partners.
  Arming, interrupt completion, cancellation, retained-ready futures and drop
  preserve unrelated flags and enable bits. The ICR command uses its documented
  write-one no-op field width: 8 bits on F002/F003, 16 on the other families.
  Physical pins and serviced pending masks remain separate.
- L011/L012 interrupt support follows actual own-manual page images. Earlier
  missing-manual and reset-value-as-mask assumptions were corrected. The latest
  L011 manual also corroborates the existing clock, GPIO, IWDT and serial paths.
- CRC exposes only each family's documented presets and feed widths. F002/F003
  have four CRC16 presets; other families have eight; F030/A030 additionally have
  two CRC32 presets. Native halfword/word feeding remains x030/F020-only.
- WWDT retains exclusive ownership, validates counts/timing, uses a frozen PCLK
  model, and exposes explicit irreversible start/feed plus polling status. It
  has no IRQ, disable, reset or implicit Drop-stop API.

## Measured regression

The final functional matrix completed at 2026-10-08 08:48:37 UTC:

- 54 HAL feature selections, covering 13 family profiles
- 108 optimized Cortex-M0+ builds: `rt` and `rt,defmt`
- 9,900 unit, 24 IRQ, 1 API and 144 documentation test executions
- Zero compiler warning lines
- 878 build inputs unchanged across the run, including executable flags

Dependency-scoped contracts then passed 2,454 diagnostic-specific failures:
879 across five affected family suites, 141 CRC, 439 IWDT, 639 WWDT and 356 EXTI.
IWDT/WWDT/EXTI checks cover all 54 selections. EXTI positive constructors cover
1,528 safe package pins, with 108 ARM variants and 30 linked/inspected ELFs on 15
exact packages. Focused CRC/WWDT/EXTI model and RAM tests are retained separately.
Unchanged serial transaction-contract suites were not rerun wholesale; their
production sources remained unchanged, and all-feature host/ARM regression ran.

Independent source reviews found no unresolved defects in CRC, WWDT or the
corrected all-family EXTI implementation. This is source/model/build evidence,
not silicon, RF coexistence, low-power wake, oscillator or electrical validation.

## Data/PAC and reproducibility

`./d test` and `./d check` passed, including deterministic regeneration and all
54 PAC feature selections. Confirmed own-manual ISR access corrections cover
F020/F002/F003/L052/L083 GTIM, L012 ADC, and L011 GPIO. Exact IR equality after
correction merges two templates, reducing 135 canonical versions to 133. The
new GPIO and timer/ADC checks include 62 rejected write/modify programs.

The source-only archive excludes vendor SDKs, PDFs, captured HTML, toolchains
and build caches. `./d fetch-evidence` now restores 374 pinned evidence files.
A fully empty root was reconstructed and verified; all 20 explicit source replay
commands and all 18 acquisition safety tests passed. Real HTTPS transfers were
also checked. See [the acquisition guide](evidence-acquisition.md) for exact
requirements, no-overwrite behavior and fail-closed source changes.

Two A030 HTML pins were deliberately refreshed after inspection found only a
breadcrumb URL change. The common F030/A030 manual listing is asserted explicitly;
the empty SDK category is negative evidence, not proof of a separate A030 SDK.

## Formatting-only finalization

After functional validation, `cargo fmt --all` changed six Rust files. The
pre-format hashes are preserved, and independent AST/macro-token equivalence,
format checks and scoped post-format tests/links are recorded under
`verification-logs/stage6/formatting/`. The formatting equivalence proof and scoped checks passed: 44 generator tests,
747 HAL unit tests, 2 IRQ tests, 11 doctests, four ARM builds and five firmware
links. Final acceptance and artifact input hashes are recorded in
`verification-logs/stage6/summary.json`.

## Remaining scope

See [the machine-readable per-family/peripheral ledger](hal-coverage.json).
ADC and GTIM drivers remain x030-only in this checkpoint. Other missing areas
include ADC/timers on remaining variants, ATIM/BTIM/LPTIM/AWT, flash, RTC,
comparators, LCD, crypto/radio-specific peripherals, remaining AF/clock metadata,
and an Embassy time driver. Serial APIs remain basic blocking or UART IRQ-driven;
DMA integration is not safe to expose with borrowed buffers because bounded
abort/cancellation semantics are not established. The experimental unsafe x030
DMA Drop may wait indefinitely.

The separately prepared F020 ADC/GTIM batch is not part of this archive's claims.
