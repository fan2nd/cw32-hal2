# Corrections since the delivered stage2 checkpoint

Status: corrected source passed the full integrated regression on 2026-10-08
at 05:30 UTC. See `hal-stage3-verification.md`. No hardware was exercised.

## Clock initialization: do not use stage2 on hardware

The delivered stage2 source writes factory HSI trim while the oscillator can
still be running. CW32x030 User Manual CN V2.5 §4.3.4 (p49), corroborated by its
English manual, explicitly prohibits changing oscillator parameters after
startup. The distinct §4.5.2 permission for live divider changes does not grant
permission to rewrite trim. F020 UM V1.4 §4.3.4 (p47) states the same restriction.

A successful host test or ARM link did not establish this missing hardware
precondition. Source-backed repairs are implemented: preserve a temporary LSI
clock, verify the system clock handover, stop HSI and verify its state before
writing trim, restart and verify HSI, then return and restore temporary state.
The reviews also cover failure paths, flash wait-state ordering, bus dividers,
entry from bootloaders and preservation of unrelated clock/security settings.

The later 031 and L010/L011 ports had the same issue and were repaired.
L012 already uses stopped-HSI calibration. F002/F003 passed independent review.
L052/L083 implement the restriction explicitly.
All repaired backends passed the final integrated regression. Family evidence
reports document voltage constraints, legal PLL transition paths and clock-
security limitations. Old result counts do not validate a later repair.

## F020 PAC corrections

- GPIO ISR/IDR read-only access is now enforced, using manual-backed corrections
  and exact-IR equality to reuse the common GPIO layout.
- The F020 CRC hardware supports eight CRC16 modes. Copied SDK CRC32 settings are
  not evidence of CRC32 support. A dedicated CRC16 register variant narrows the
  result field to 16 bits while retaining the documented bus-access alias.
- F020 SPI uses a conservative 12 MHz ceiling because official summary and
  detailed timing figures conflict (12 Mbit/s versus 16 MHz).

The authoritative register pool now has 135 distinct exact normalized layouts.
The previous delivered archive had 137. These counts measure deduplicated data,
not the number of hardware IP designs proven equivalent under all behavior.

## Additional implementations and metadata

The current working tree adds reviewed x030 ADC analog routes, blocking ADC,
blocking I2C, GTIM counter/PWM, F020 serial/CRC16 support and independently audited
family GPIO/HSI backends. See `hal-coverage.json` for exact scope. All remaining
peripheral and metadata gaps remain explicit; no all-peripheral completion is
claimed.
