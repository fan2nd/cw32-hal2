> Historical software-verification references: HAL tests and fixture harnesses cited below were deleted on 2026-10-08. Their reported results apply to the dated source snapshot only; source facts remain unchanged. See [current HAL layout and build scope](hal-production-layout.md).

# Independent watchdog implementation evidence

This is a source-reviewed, host-tested implementation, **not silicon validation**.
The module follows the pinned Embassy `IndependentWatchdog<'d, T>` ownership and
`new(Peri, timeout_us)`, `unleash()`, `pet()` interface. The upstream reference is
[embassy-stm32/wdg](https://github.com/embassy-rs/embassy/blob/f16efeffe37581092ec184718e6fdb1620393214/embassy-stm32/src/wdg/mod.rs).
The implementation is original Rust using the generated CW32 PAC.

## Hardware-specific differences

1. Every reviewed CW32 manual/SDK starts IWDT **before** writing CR/ARR/WINR.
   `new` therefore only validates and saves settings. `unleash` performs the
   hardware sequence. This preserves the caller-visible delayed-start boundary.
2. The prescaler is 4, 8, 16, 32, 64, 128, 256, or 512; the counter is 12 bits.
   All eight encodings are supported, including DIV256 omitted accidentally
   from some SDK `IS_IWDT_PRESCALER` validation macros.
3. The sources document stop keys 0x5A5A then 0xA5A5. This is not an irreversible
   STM32 start. Nevertheless this API intentionally exposes no stop/disable and
   implements no `Drop`: dropping a watchdog never changes its hardware state.
4. 0x5555 unlocks configuration. Any other KR value locks it. We use 0x6666 to
   relock explicitly on every exit after START, including failure exits.
5. Overflow selects reset (ACTION=0); interrupts are disabled (IE=0), and
   DeepSleep counting continues (PAUSE=0). Existing debug-freeze controls and
   reset-cause flags are not changed. This document covers IWDT; the separate
  polling WWDT implementation is described in `window-watchdog-evidence.md`.

## Clock audit across every supported family

Electrical tables override generic SDK timing macros and illustrative 10 kHz
manual calculations. Source paths and SHA-256 hashes are in
`watchdog-evidence.json`. The following wide-temperature accuracy figures apply
under each datasheet's stated supply and factory-calibration conditions.

| Family | Clock | Typical Hz | Accuracy | Calculated low/high Hz | Temperature °C | Gate | PAC version |
|---|---|---:|---|---|---|---|---|
| F002 | RC10K | 8000 | ±50% | 4000 / 12000 | −40..105 | APBEN1.5 | v1 |
| F003 | RC10K | 8000 | ±50% | 4000 / 12000 | −40..105 | APBEN1.5 | v1 |
| F020 | RC10K | 8000 | ±50% | 4000 / 12000 | −40..105 | APBEN1.5 | v1 |
| F030 | RC10K | 8000 | ±50% | 4000 / 12000 | −40..105 | APBEN1.5 | v1 |
| A030 | RC10K | 8000 | ±50% | 4000 / 12000 | −40..105 | APBEN1.5 | v1 |
| L010 | LSI | 32800 | ±3% | 31816 / 33784 | −40..85 | APBEN2.4 keyed | v1 |
| L011 | LSI | 32800 | −10/+25% | 29520 / 41000 | −40..85 | APBEN2.4 keyed | v1 |
| L012 | RC10K | 10500 | ±25% | 7875 / 13125 | −40..85 | APBEN2.4 keyed | v1 |
| L031 | RC10K | 8000 | ±50% | 4000 / 12000 | −40..85 | APBEN1.5 | cw32l031_v1 |
| R031 | RC10K | 8000 | ±50% | 4000 / 12000 | −40..85 | APBEN1.5 | cw32l031_v1 |
| W031 | RC10K | 8000 | ±50% | 4000 / 12000 | −40..85 | APBEN1.5 | cw32l031_v1 |
| L052 | RC10K | 8500 | ±50% | 4250 / 12750 | −40..85 | APBEN1.5 | cw32l031_v1 |
| L083 | RC10K | 8500 | ±50% | 4250 / 12750 | −40..85 | APBEN1.5 | cw32l031_v1 |

Datasheet clock-table locators: F002 V1.2 table 7-15; F003 V1.9 table 7-16;
F020 **current** V1.3 table 7-19 (the earlier root-path download's body says V1.2,
so this review uses `current-datasheets/CW32F020_DataSheet_CN_V1.3.*`);
F030 V1.9 table 7-19; A030 V1.1 table 7-18; L010 V1.3 table 7-19;
L011 V1.1 table 7-19 (its revision updates the LSI characteristics);
L012 V1.0 table 7-20; L031 V1.9 table 7-20; R031 V1.2 table 7-24;
W031 V1.3 table 7-25; L052 V1.3 and L083 V1.9 table 7-19.

L010/L011 headers still define `IWDT_FREQ=10000`, but both own-family manuals
§15.3.7 (L011 V1.1 p385), `IWDT_SetPeriod` and reset-example readmes use
LSI at 32.8 kHz. Those SDK helpers also explicitly enable LSI before starting
IWDT. The HAL preserves LSI TRIM/WAITCYCLE, enables LSI with the keyed CR1
write, waits for STABLE, and leaves it enabled. It does not trim/calibrate a
shared oscillator or borrow the L010 tolerance for L011. The ordinary RCC
initialization's temporary LSI enable is insufficient because it restores the
entry enable state. The newly acquired L011 User Manual CN V1.1 independently
confirms initial 0xFFF count, keys/locking and update flags (§15.3 pp383–385),
start-before-configuration and reload completion (§15.4 p386), and registers
(§15.6 pp388–390). Its own datasheet still supplies electrical clock tolerance.
See `l011-manual-follow-up-audit.md`; no production IWDT change was required.

The counter period is `(ARR + 1) * divisor / oscillator_hz`. Arithmetic uses
u64 products and ceiling division **before** narrowing. The smallest divider
that fits 1..4096 ticks is selected. Zero and values above the selected
family's maximum fastest-clock period are rejected. For example a requested
1 s on an 8 kHz-typical part produces an approximately 1.5 s typical period,
with the fastest-clock estimate at least 1 s. This conservatism is intentional.
The `Timing` API exposes outward-rounded oscillator-bound estimates.

These estimates are not a hard wall-time guarantee from `unleash`/`pet` to
system reset. Asynchronous clock-domain latency, counter phase, reset
propagation, debugger state, firmware latency and application changes to LSI
can affect observed behavior. Feed with additional system-specific margin.

## Startup, synchronization and failure contract

- Enable the configuration gate without resetting the peripheral. APBEN2 writes
  on L010/L011/L012 require 0x5A5A in the high halfword; preserve other enables.
- Before START, refuse RUN=1, busy state, or CR/ARR/WINR different from their
  reset values 0/0xFFF/0xFFF. There is no bootloader/watchdog takeover mode.
- START is 0xCCCC. Manuals say it begins at counter 0xFFF. At reset divider 4,
  the shortest initial oscillator-only window is `16384 / max_hz` seconds:
  roughly 1.365 s (8 kHz group), 1.285 s (8.5 kHz), 1.248 s (L012),
  0.485 s (L010), and 0.400 s (L011). The caller must complete startup within
  that window. The per-wait poll budget is not a real-time bound.
- Wait for RUN and idle synchronization flags, unlock, then write CR before
  ARR. A divisor increase cannot shorten the initial counter window.
- Leave the checked default WINR untouched: writing WINR implicitly reloads
  the counter and can prematurely load a very short ARR during setup.
- Wait for configuration completion and compare CR/ARR/WINR readback, lock,
  then issue 0xAAAA and wait for RELOAD clear. Never claim success merely
  because a loop exhausted its counter.
- Every polling wait is bounded. If an error occurs after START, mark that
  instance's startup attempt as failed, relock protection, and do not stop or
  keep feeding a partially configured watchdog. Hardware may still reset the
  MCU; reset is the recovery path. Pre-START errors are retryable.
- Repeated successful `unleash` calls refresh without changing configuration.
  A premature `pet` errors. Refresh checks RUN and waits both before and after
  its key write. It never clears OV or the MCU's IWDT reset-cause flag.

The hardware may reset before software returns any timeout error, particularly
with a slow CPU, stalled bus, short configured interval or a long interrupt.
This is fail-safe behavior, not an assurance the startup routine is real-time.

## Manual/SDK protocol locators

F002 manual V1.4 chapter 13; F003 V2.3 chapter 14; F020 V1.4 chapter 15;
F030/A030 shared x030 V2.5 chapter 16; L010 V1.2 chapter 15; L012 V1.4 chapter
19; L031 V1.6, R031 V1.3 and W031 V1.4 chapter 16; L052 V1.5 and L083 V2.0
chapter 17. Within each chapter, sections .3.2 through .3.7 describe counting,
window reload, keys, status and timing; .4.1/.4.3 give configuration/feed
sequences; .6 gives reset values and fields. Each family's own SDK header and
`IWDT_Init` are separately hashed. A030 uses its documented shared F030 SDK
and shared x030 manual, plus its separate electrical datasheet.

## Verification

- `cargo test -p embassy-cw32 --no-default-features --features <chip> wdg::`
  runs 18 host tests: frequency profiles, divider boundaries/overflow, outward
  rounding, scripted start/lock/feed protocol, every modeled failure phase,
  refused takeover, final-poll boundary, no-MMIO constructor/drop behavior, and RAM-backed generated PAC accesses.
- `python3 tests/test_wdg_hal_contracts.py` checks one positive ARM program and
  nine intentional compile failures; `CW32_WDG_TEST_CHIP` selects another part.
- `python3 tests/audit_wdg_sources.py` checks the recorded 13-family source
  hashes, SDK constants/field masks, electrical table text, and all chip
  metadata against the reviewed watchdog register/clock/gate contract.
- Source audits and models do not establish reset propagation, oscillator
  extrema, low-power/debug behavior, bus synchronization on silicon or errata.
  Real-board timeout/reset and failed-start tests remain necessary.
