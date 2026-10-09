> Historical software-verification references: HAL tests and fixture harnesses cited below were deleted on 2026-10-08. Their reported results apply to the dated source snapshot only; source facts remain unchanged. See [current HAL layout and build scope](hal-production-layout.md).

# L011/L012 blocking RTC calendar

This batch adds a real, bounded calendar driver for CW32L011 and CW32L012,
including their four exact package features. It does not expand the existing
PAC correction batch. All other families keep their RTC HAL unavailable.

## Public API and ownership

The API follows the pinned Embassy `rtc::Rtc`, `RtcConfig`, `RtcError`,
`DateTime`, `DayOfWeek`, `DateTimeError`, `now` and `set_datetime` names and
DateTime constructor/accessor shape. Hardware-specific differences are explicit:

- `rcc::HsiOscClock::new(Peri<SYSCTRL>)` requires successfully frozen RCC and an
  enabled, stable HSI oscillator. The capability is neither Copy nor Clone.
- `Rtc::attach_preserving_state(Peri<RTC>, HsiOscClock, RtcConfig)` validates an
  already-running calendar. It only enables its APB configuration bus. It never
  writes RTC registers, KEY, event flags, prescalers, calibration or RESETFLAG.
- `Rtc::initialize_if_unset(..., DateTime)` explicitly initializes a stopped,
  unset DATE=0 calendar with no event-control/interrupt/output/compensation
  configuration. Inactive alarm-match and AWT reload registers are preserved. It
  selects nominal 96 MHz HSIOSC, PSC1=119, PSC2=399999, and 24-hour format.
- The clock capability holds SYSCTRL for the full driver lifetime. This is a
  capability for a shared frozen oscillator, not an exclusive physical clock:
  system HSI also depends on it. All safe RCC operations are one-time init or
  read-only clock queries; none disables/recalibrates HSI after init.
- Borrowed `Peri::reborrow()` permits a new attempt after dropping a failed
  constructor or a completed driver. Fully moved tokens remain consumed on
  constructor failure. RTC and SYSCTRL borrows cannot be reused while the
  driver is alive. There is no cloneable unsynchronized time-provider handle.
- No Drop implementation writes hardware or disables the oscillator/bus gate.
  `Peripherals::steal()` and direct PAC interference invalidate the contract.

`DateTime::from(year, month, day, weekday, hour, minute, second, microsecond)`
uses the upstream shape but accepts 2000–2099, Gregorian-valid days, matching
weekday and microsecond=0 only. Leap seconds and fractional setters are rejected.
Public `DayOfWeek::Sunday=7` matches Embassy, explicitly converted to CW32 zero.
Reads retain both 12/24-hour formats; midnight 0x12 and noon 0x32 in 12-hour
hardware encoding are converted correctly. Invalid reset DATE or invalid BCD
returns an error rather than a fabricated value. There is no century metadata:
2099→00 is not inferred to mean 2100 and can also fail weekday validation.

## RCC retention preflight

Both supported RCC backends enable APBEN2 bit1 with the 0x5A5A gate key and read
RTC SOURCE before any oscillator modification. They conservatively protect
HSIOSC whenever SOURCE=3, even when START=0, because AWT can still consume it.

A retained HSIOSC source requires HSI enabled, stable and already at the factory
trim. Otherwise `rcc::Error::RtcClockInUse` is returned before changing oscillator,
system clock or flash configuration. A failed gate enable returns
`RtcClockGateTimeout` before clock-tree writes. The bus gate may remain enabled
on either failure; RTC registers and reset causes remain unchanged.

For compatible retained HSIOSC, L011 omits stop/retrim and changes only DIV using
its documented live-divider procedure. Its existing temporary LSI/core-switch
sequence remains, with the original LSI enable/security controls restored on
success. L012 already has a matching-trim live-divider path and now explicitly
protects it. Every later timeout/error path preserves HSI enable and trim;
clock init can still fail and withhold frozen frequencies. This does not promise
recovery from asynchronous external-clock loss or direct register interference.

For SOURCE=LSE/HSE/LSI, RCC leaves the RTC source/calendar untouched, preserves
existing external enables and LSI parameters, and restores the software LSI
enable after any temporary bridge. The new RTC constructor returns
`IncompatibleClock` for these sources. It does not silently select HSIOSC to
attach. L010 shares a backend file but does not acquire this WAIT-only RTC API
or change its prior RCC transition algorithm.

## Prescalers and source tolerance

Both own datasheets Table 7-18 specify factory HSIOSC accuracy of ±2% at
−40…+85°C under their stated operating conditions. The nominal 96 MHz source
can therefore reach 97.92 MHz. The manual's RTCCLKD ≤1 MHz limit must be checked
against that maximum, independently of the nominal 2 Hz calendar product.

The default /120 then /400000 uses PSC1=119, PSC2=399999: RTCCLKD is nominally
800 kHz and at most 816 kHz at +2%; TICKCLK is nominally 2 Hz. Attachment rejects
/96 then /500000 even though its nominal product is exact, because RTCCLKD
could reach 1.02 MHz. It accepts other exact nominal products only when the
same worst-case intermediate limit holds. This preserves hardware margin;
it does not remove HSI clock drift. The nominal 2 Hz calendar can still run
1.96…2.04 Hz over that stated source range.

The initial frozen candidate used the nominal-only 1 MHz boundary. Independent
parent review caught the electrical-limit error before integration. This
revision supersedes manifest e7b7469f5dd3696c5cffc0d92ac5b27b93a83e05f1fd06d2aaa4b8106fe5be83;
the original manifest is retained for audit history, not current verification.

## Synchronization and failure semantics

`RtcConfig::timeout` is a register-poll budget, and `read_retries` is a snapshot
attempt budget. Neither is a time duration; neither depends on an Embassy time
driver. Zero budgets are rejected before RTC MMIO.

`now()` checks source/readiness, prescaler suitability and disabled compensation,
waits for WAIT=0, then performs a short interrupt-masked T-D-T-D-T attempt. It
requires all times equal, both dates equal and final WAIT=0. This combines the
manual's fast repeated-register-read exception with complete-pair consistency,
without unlocking or stopping the counter. It retries observed rollovers.
The own manuals provide no full-calendar latch, so clock-domain behavior at a
silicon rollover must still be tested on boards. NMI/debugger stalls are outside
the bounded normal-execution contract. These tests do not turn a software
consistency check into an undocumented atomic hardware snapshot guarantee.

`set_datetime()` is an explicit time jump. After WAIT=0 it unlocks, clears START,
verifies stopped state, drains WAIT, writes DATE, waits, writes TIME, waits and
checks both stopped values. It restarts only after verification and relocks via
a scope guard on every unlocked exit. Stopping avoids a date/time pair crossing
midnight during writes. The current 12/24-hour format and all event registers
are preserved. TIME clears subseconds. Calendar events and TICKCLK-derived AWT
can be affected by the explicit jump and stop interval.

A failed set is not rolled back. If a failure happens after stop, the RTC may
remain stopped or partially updated. The error is returned and `now()` reports
NotRunning while stopped; another explicit set can retry. A failure before the
unlock leaves lock state and calendar unchanged. Cold initialization failures
likewise can leave a partially configured stopped RTC. No error path claims
that time continued normally.

## Verified boundary

Production-function host tests cover all 36,525 supported dates, weekday
validation, both hour formats, malformed BCD, leap/month/year boundaries,
midnight at every modeled read position, bounded unstable/stuck WAIT reads,
write settlement and failure relocking, rejected cold-init takeovers, exact
nominal prescaler products, positive-HSI-tolerance rejection on attachment,
and real generated PAC offsets in RAM.

Both full RCC/driver host suites run, plus L010 regression tests. Retained-HSI
models inject each numbered clock-tree write failure and verify no HSI disable
or trim change; they separately track the keyed RTC gate preflight. ARM contracts
build the six family/exact profiles with and without defmt, test ownership and
unsupported APIs, and reject this RTC module on the other eleven family aliases.
Four exact-package rt+defmt firmware images link and their vectors/memory are
inspected. None of these images is executed on hardware.

Commands (use the configured Rust toolchain and `CARGO_INCREMENTAL=0`):

```sh
python tests/verify_rtc_calendar_evidence.py
cargo test -p embassy-cw32 --no-default-features --features cw32l011 --lib
cargo test -p embassy-cw32 --no-default-features --features cw32l012 --lib
cargo test -p embassy-cw32 --no-default-features --features cw32l010 --lib
python tests/test_rtc_calendar_contracts.py
python tests/test_rtc_calendar_links.py
python tests/test_module_layout.py
```

Still required: board verification of reset-preserved clock behavior, exact
stop/start/write settlement, midnight/leap-day rollover, missing/stuck clocks,
actual RC drift and the required timing margin of the fast read sequence.
HSIOSC readiness is not frequency measurement or a low-power guarantee.
No LSE/LSI/HSE startup API, alarm (including contradictory ALARMB polarity),
timestamp, calibration, async, Embassy time driver, battery/VBAT retention or
sleep/wake behavior is claimed.

## Sources

The [machine-readable evidence](rtc-blocking-calendar.json) pins the rehashed
own manuals, datasheets, SDK files, existing PAC templates and upstream API
sources. Page numbers are one-based physical PDF pages.

- [L011 UM CN V1.1, June 2026 bytes](https://www.whxy.com/uploads/files/20260602/CW32L011_UserManual_CN_V1.1.pdf):
  §4.5.2 p61 permits live DIV changes with TRIM unchanged; §10.3.2 p140 gives
  HSIOSC source and two-stage divider limits; §10.3.6 p143 gives WAIT-only and
  fast equal-read access; §§10.5.2–10.5.7 pp151–155 define START/H24/SOURCE/BCD.
- [L012 UM CN V1.4](https://www.whxy.com/uploads/files/20260603/CW32L012_UserManual_CN_V1.4.pdf):
  §4.5.2 gives the live HSI divider path; §13.3.2 gives PSC constraints;
  §13.3.6 p194 gives WAIT-only access; §§13.5.2–13.5.7 pp201–205 define registers.
- L011 [datasheet CN V1.1](https://www.whxy.com/uploads/files/20250724/CW32L011_DataSheet_CN_V1.1.pdf)
  Table 7-18 physical p51 and L012 [datasheet CN V1.0](https://www.whxy.com/uploads/files/20250717/CW32L012_DataSheet_CN_V1.0.pdf)
  Table 7-18 physical p58 specify ±2% HSIOSC at −40…+85°C. Exact verified URLs
  and hashes are recorded in the machine-readable evidence.
- [Pinned Embassy RTC](https://github.com/embassy-rs/embassy/tree/f16efeffe37581092ec184718e6fdb1620393214/embassy-stm32/src/rtc):
  upstream API names/DateTime shape, not STM32 register behavior.
- [Prior full audit](rtc-next-batch-audit.md) and [PAC correction provenance](rtc-pac-corrections.json):
  family differences, corrected DATE widths, alarm contradiction and deferred scope.
