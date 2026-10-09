> Historical software-verification references: HAL tests and fixture harnesses cited below were deleted on 2026-10-08. Their reported results apply to the dated source snapshot only; source facts remain unchanged. See [current HAL layout and build scope](hal-production-layout.md).

# HAL stage 3: corrected clocks and all-family base support

Final `./ci/check-hal.sh` passed on 2026-10-08 at 05:30 UTC, exit 0, in 490 seconds.
All 1,158 hashed code/data/test/CI inputs were unchanged during the run. Manifest
SHA256: `1ae4ff3af7656b75094f6745ecbbcf89bc33bb8c46f581823400ea1e65233418`.
See `verification-logs/` for full logs and before/after manifests.

## Passed software verification

- 54 host chip selections; 108 ARM release library builds (rt and rt+defmt)
- 3,383 unit, 24 IRQ, 1 UART API and 106 documentation test executions across
  the feature matrix. Repeated feature executions are not distinct hardware tests.
- All typed peripheral and package compile-contract suites
- 25 fully linked and inspected firmware ELF configurations
- Separate data/PAC regeneration and all 54 PAC metadata/ARM selections passed

Per-feature unit tests: F030/A030 164, F020 105, F002/F003 24, L010/L011 28,
L012 22, L031/R031/W031 27, L052 25 and L083 28.

## Scope

All 13 current family profiles now have GPIO and HSI clock backends. F030/A030
also implement asynchronous GPIO edges, CRC16/32, blocking/interrupt UART,
blocking SPI/I2C, blocking ADC, GTIM counter/PWM and experimental unsafe DMA.
F020 additionally implements CRC16, blocking/interrupt UART and blocking SPI/I2C.
Most other peripherals remain PAC-only; see `hal-coverage.json`.

## Corrections and operating requirements

This checkpoint supersedes stage2 for hardware use. HSI trim is changed only with
verified safe-clock handover and stopped HSI. Bus staging prevents transient
low-voltage overclocking. Legal PLL transitions, FLASH wait-state ordering,
mandatory control readbacks, final divider/trim/stability checks and LSI
software-state restoration are covered by source reviews and protocol tests.

The incoming clock/voltage state must be legal and its active source must remain
available during initialization. Arbitrary asynchronous external-clock loss can
race read/modify/write operations; fault-tolerant recovery is not guaranteed.
The board must satisfy final clock/voltage limits, including applicable 24 MHz
limits below 1.8 V. The HAL does not measure supply voltage. Failed initialization
may require reset before retrying. See `clock-init-independent-review.md` and
`stage3-corrections.md` for evidence and exact limits.

## Linked examples

F030 blocking: 4,836 FLASH / 24 static RAM bytes; async GPIO: 3,204 / 584.
F020 F6U7: 10,306 / 40; K6U7 and C6U7: 10,338 / 40.
L012 exact parts: 4,472 / 24 (4,482 with defmt).
L052 exact parts: 4,524 / 24 (4,546 with defmt).
L083 exact parts: 5,108 / 24 (5,122 with defmt).
These figures exclude runtime stack use. ELF checks verify memory placement and
vectors; they do not execute firmware or establish electrical correctness.

No board was flashed. DMA has no safe borrowed-buffer wrapper; experimental
raw transfer Drop may wait indefinitely for terminal hardware completion.
Clippy was unavailable; no clippy or physical-device result is claimed.
