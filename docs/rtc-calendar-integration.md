> Historical software-verification references: HAL tests and fixture harnesses cited below were deleted on 2026-10-08. Their reported results apply to the dated source snapshot only; source facts remain unchanged. See [current HAL layout and build scope](hal-production-layout.md).

# Frozen RTC calendar integration

The main checkout was never edited by this worker. This isolated branch copies
its Stage 8 baseline. Copy only the files listed under `owned_files` in
`docs/rtc-calendar-frozen-manifest.json`, then apply `patches/rtc-shared.patch`.
Do not copy the isolated build.rs/lib.rs or other shared files wholesale: main
has concurrently integrated classic GTIM changes that must remain intact.

The shared patch is rebased additively on main's current classic-GTIM hooks and
passed `git apply --check` there. Its baseline/expected hashes and the separately
tested isolated shared-file hashes are recorded in
`patches/rtc-shared-baselines.json`. It includes:

- Hardware-version RTC cfgs and module exports, with no synthetic HAL cfg labels.
- The SYSCTRL-backed HSIOSC capability re-export.
- L011/L012 RCC retained-HSI preflight and production-model regression tests.
- RTC cfg-name recognition, official evidence verification in `./d test`, and
  ownership/API/link tests in the aggregate HAL runner.

`ci/check-rtc-hal.sh` is the focused runner. Re-run it after integration with
CW32_SOURCES and the configured Rust toolchain. The optional
CW32_EMBASSY_UPSTREAM points to the pinned Embassy checkout when its API source
rehash should also be enforced. Official hardware evidence checks remain
mandatory without an upstream checkout.

Software validation in the isolated checkout:

- L010 host regression: 183 tests passed.
- L011 full host suite: 208 tests passed, including 19 RTC tests.
- L012 full host suite: 218 tests passed, including the same 19 RTC tests.
- Six supported family/exact profiles: ARM release and defmt builds.
- 71 intended ARM compile failures: 60 ownership/API cases and 11 unsupported
  family aliases. Diagnostic codes and intended targets were checked.
- Four exact-package rt+defmt firmware links: L011 10,754 flash bytes, L012
  10,626 flash bytes; 24 static RAM bytes. Vectors/memory checked.
- Module layout, hardware cfg naming, formatting and 12 pinned source hashes,
  two own PAC/register contracts and pinned upstream API source rehash passed.
- The initial worker review's two nonblocking notes were addressed: explicit
  preserved inactive match/reload documentation and a restart-only failure test.
- Independent parent electrical review then found that the original nominal
  1 MHz intermediate clock could reach 1.02 MHz with permitted +2% HSI tolerance.
  This revision uses /120 then /400000 (800 kHz nominal, 816 kHz maximum), applies
  the 97.92 MHz source bound to attachment, and adds two tolerance regressions.
  Both Table 7-18 rows and manual ceiling pages are checked by the evidence test.
  The original manifest hash is retained in the new freeze's revision history;
  it is superseded and must not be treated as the current accepted candidate.
  Final parent delta review is separate from these software verification results.

Full logs are under `docs/verification-logs/rtc-*`. These are isolated software
results, not an assertion that a later combined timer/RTC tree has been tested.
No firmware was flashed/run, hardware touched, unknown software installed,
package published, commit pushed or branch merged.

For the parent coverage ledger, the bounded driver addition is
`rtc_calendar_blocking` on CW32L011/CW32L012 only. Suggested peripheral scope:
“Whole-second blocking calendar using frozen nominal HSIOSC; preserving-state
attach; explicit stop/write/restart setter; no alarms/async/low-power claims.”
All other RTC families remain PAC-only. The detailed behavior and board-work
boundary are in `rtc-blocking-calendar.md`.
