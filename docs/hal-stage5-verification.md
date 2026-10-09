> Historical software-verification references: HAL tests and fixture harnesses were deleted on 2026-10-08. Counts and commands below apply to the dated source snapshot. See [current HAL layout and build scope](hal-production-layout.md).

# Stage 5: all-family serial drivers with verified metadata corrections

## Full frozen driver run

`./ci/check-hal.sh` passed from 2026-10-08 07:06:23 to 07:24:32 UTC:
54 host selections, 108 ARM release builds, 7,625 host test executions,
85 contract suites, 2,365 expected compiler failures and 25 inspected ELFs.
No unexpected errors or compiler warnings were observed. All 1,196 tested input
files and executable flags remained unchanged during that full run.
Manifest SHA256: `91cce0ecbf1892911db557a42179e5dca8d5542ff72e473932516618a743bf93`.
See `verification-logs/stage5/hal-final-summary.json` and `hal-final-report.txt`.

## Verified post-run metadata delta

Before delivery, newly confirmed source errors were corrected:

- F020 FAULT31 is associated with SYSCTRL clock failures, not CRC. SYSCTRL's
  existing RCC association is preserved. Earlier stage4 metadata was incorrect;
  the polling-only CRC driver never used that association.
- Five GPIO ISR register maps were changed from RW to RO for F002/F003/L010/L052,
  covering 15 selections. Each own-family manual confirms read-only status.
  Reconstruction hashes prove no neighboring register, field, offset or width
  changed. L011 and disputed ICR masks remain unmodified.

This is intentionally a different final source snapshot from the full run.
Exactly 46 files changed and 3 were added, with zero deletions. All 91 protected
HAL/CI/example/Cargo/build/config inputs stayed unchanged. The final 1,199-file
source manifest SHA256 is
`2e27fa0dfe3a27eeeee6b1f67185951cf0e5efa9b7e3fc6a5b4f2703e1c6deca`.

Post-delta validation passed in 215 seconds, ending 07:52:18 UTC:
- All 19 affected host selections: 2,317 unit and 38 documentation executions
- All 19 affected ARM release builds with rt+defmt
- ARM rt+defmt checks for all 54 HAL selections
- F020 three-package typed contracts and three inspected ELF links
- Four compile-time FAULT owner positives and four rejected CRC-owner assertions
- Four GPIO access positives and 26 rejected write/modify attempts
- Complete data/PAC tests, exact regeneration and all 54 PAC selections

Exact changed/new file hashes, stable before/after manifests, protected-input
checks and results are in `verification-logs/stage5-delta/`.
No unrelated contract suite is claimed to have rerun after the isolated delta.

## Implementation scope and limits

All 13 families now have GPIO/HSI, independent watchdog, blocking/IRQ UART,
blocking master SPI and blocking seven-bit I2C. L083 shared UART handlers guard
inactive instances and never disable/unpend a partner. External bootloader/driver
owners must quiesce or service unowned sources before a shared vector is enabled.
L012 command I2C uses PCLK only, explicit electrical timing assumptions, bounded
drain/reset recovery and <=256-byte contiguous reads. Its NACK source is Unknown;
no asynchronous, bus-unwedge or disputed FIFO-depth capability is claimed.

The remaining 340 serial AF cells are source-backed and package-filtered.
New low-power SPI rejects unsupported HalfPeriod sampling delay and unavailable
pull-downs before MMIO. Independent source/lifetime/transaction reviews and
final reviewed hashes are recorded in the driver documents.

The code retains `<module>/mod.rs` organization and hardware-meaningful cfg names.
No firmware was flashed or executed; stack usage and electrical timing were not
measured. Full remaining per-family/peripheral scope is in `hal-coverage.json`.
