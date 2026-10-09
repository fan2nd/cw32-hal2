> Historical software-verification references: HAL tests and fixture harnesses cited below were deleted on 2026-10-08. Their reported results apply to the dated source snapshot only; source facts remain unchanged. See [current HAL layout and build scope](hal-production-layout.md).

# CW32L011 own-manual corroboration

Reviewed 2026-10-08 against the newly acquired official [CW32L011 User Manual
CN V1.1](https://www.whxy.com/uploads/files/20260602/CW32L011_UserManual_CN_V1.1.pdf).
PDF SHA-256: `b245887e9037caf267f739e25bd3c354c2579d23315fc06389e055f371159f4f`.
Exact PDF/text provenance and section locators are in
`l011-manual-follow-up-evidence.json`.

This is a read-only correctness review of production drivers, with evidence,
documentation and source-audit updates. **No production code, hardware behavior,
electrical limits or package bonding was changed or hardware-tested.** CRC and
GPIO interrupt/ISR-access changes have their own independent reviews; they are
outside this report's acceptance scope.

## Result

No behavioral mismatch was found in the reviewed HSI/FLASH, basic GPIO, IWDT,
UART, SPI or classic-I2C contracts. The earlier implementation was based on
L011's own SDK/datasheet rather than an assumed L010 equivalence; its own manual
now independently corroborates the decisive behaviors below.

Two documentation qualifications changed:

1. Stopping HSI before changing oscillator parameters is explicitly required
   by L011 §4.3.4 p48, rather than just a conservative policy borrowed from L010.
   The existing implementation already follows this rule despite the SDK's
   live-TRIM sequence. §4.5.2 permits divider-only updates preserving TRIM;
   it does not authorize live oscillator calibration.
2. SPI SMP=1 is explicitly approximately **20 ns** (§17.7.1 p455). The previous
   'unquantified' description is obsolete. `SampleDelay::HalfPeriod` remains
   unsupported because it has different semantics; no driver change is needed.

A narrower GPIO documentation discrepancy was also corrected: §8.6.4/.5 p127
allocates full AF nibbles but defines only AF0–AF7. The SDK/PAC uses three bits
and the HAL preserves the fourth. The manual does not explicitly call that
fourth bit reserved, and does not define AF8–AF15. Preserving it and restricting
safe routes to reviewed AF0–AF7 needs no production change. The manual's GPIO
reset-table and interrupt-mask questions are tracked by the separate GPIO/EXTI
review; do not infer package bonding or reserved masks from broad PINy fields.

## Checked contracts and source locations

All page references are printed pages, not PDF viewer indices.

- RCC: §4.3.4 p48 states 96 MHz HSIOSC and default /24 = 4 MHz. §4.7.4 p68
  provides all 16 encodings, `[32,1,2,3,4,5,6,7,8,9,10,12,16,20,24,28]`,
  11-bit TRIM and the two calibration bytes at `0x001007c0..0x001007c1`.
  §4.7.1 p64 confirms HSI/HSE/LSI/LSE source encodings and AHB/APB dividers.
  §4.7.2 pp65–66 explicitly states hardware LSI requests, including IWDT, do
  not set software LSIEN. Restoring LSIEN=0 must not require STABLE=0.
- Clocks/resets: §§4.7.11–16 pp76–81 confirms keyed AHBEN/APBEN1/APBEN2,
  high-half key `0x5a5a`, unkeyed active-low peripheral resets, GPIO A/B/C
  bits4/5/6, SPI APB1 bit2, UART1/2/3 APB1 bits3/4/8, and IWDT/I2C APB2
  bits4/6. No whole-bank write or reset pulse is inferred beyond ownership.
- FLASH: §7.4 p101 confirms WAIT=0/1/2/3 at HCLK≤24/48/72/96 MHz, with the
  configuration gate enabled first. Supply-dependent maximum clocks remain
  governed by L011's datasheet electrical tables, not inferred from this table.
- Basic GPIO: §§8.3.2–4 pp119–120 and §§8.6.1–6 pp126–127 confirm DIR=1
  input, DIR=0 output, push-pull/open-drain, PUR and analog disconnect. The
  register inventory has no PDR or programmable SPEED; those APIs remain absent.
  AF0 selects GPIO and only AF1–AF7 are defined. Dedicated set/clear/toggle
  registers remain the output update path. Bonded/debug exclusions still use
  L011's own datasheet, not a broad manual field range.
- IWDT: §15.3 pp383–385 confirms initial count `0xfff`, LSI≈32.8 kHz,
  divisors4..512, `(ARR+1)*divisor/f`, start/reload/stop/unlock keys,
  WINR-write implicit reload, synchronization flags and PAUSE=0 continuing in
  DeepSleep. §15.4 p386 explicitly requires start before configuring CR/ARR
  and waiting for update/reload completion. §§15.6 pp388–390 confirms existing
  offsets/fields. The own datasheet's −10/+25% LSI bound is unchanged.
  This IWDT window option does not imply a separate WWDT peripheral on L011.
- UART: §16.8.1 pp421–422 confirms PCLK selection, integer/fraction 16× and
  integer-only 8×/4× sampling, and that parity occupies the last character bit.
  Eight data bits plus parity therefore uses CHLEN=1. §§16.8.11/.12 pp428–430
  confirms FE/PE/NE/ORE bits8/9/10/11, TXBUSY bit14 and ICR reset `0x1fff`,
  with R1W0 flags and reserved bit0 preserved by the existing mask policy.
- SPI: §§17.7.1–8 pp455–461 confirms all eight /2..256 BR encodings,
  approximately20 ns delayed sampling, CR1 writable only with CR2.EN=0,
  separate CR2.EN bit0, changed status/data offsets, BUSY/TXE/RXNE/error flags
  and R1W0 ICR reset `0xff`, including destructive FLUSH bit0. Disabled
  initialization/recovery and ordinary transfer policy remain unchanged.
- I2C: §§18.4.2/.3 pp468–469 confirms PCLK/(8*(BRR+1)), BRR1..255,
  and FLT=1 for BRR≤9. §18.4.12 pp484–486 confirms the existing master
  state codes (08/10/18/20/28/30/38/40/48/50/58), F8 idle and00 bus error.
  §18.7 pp493–496 confirms GPIO input-mux zero, software-cleared START,
  hardware-cleared STOP, and SI write-zero advancing the state machine only
  after settings are prepared. No new slave/multimaster/async API is inferred.

## Reproducible checks

- `python3 tests/audit_l011_manual_sources.py` checks the pinned own manual's
  decisive register tables and protocol statements for all seven domains.
- `python3 tests/audit_wdg_sources.py` now includes L011 own-manual evidence
  alongside its existing own SDK and electrical sources.
- `python3 tests/verify_remaining_uart_sources.py --sources /workspace/shared/cw32-sources`
  now checks L011's own R1W0 ICR table, removing the stale manual-gap exception.
- `tests/test_wdg_hal_contracts.py` now asserts missing WindowWatchdog only for
  L010/L011. The other 11 families' new WWDT support is verified separately
  by `tests/test_window_watchdog_contracts.py`.

Historical read-only audit documents continue to describe the source inventory
available at their original snapshot. This report closes the specific L011
manual-availability caveats above; it does not turn compilation or source
corroboration into silicon validation.
