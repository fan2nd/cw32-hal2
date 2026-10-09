> Historical software-verification references: HAL tests and fixture harnesses were deleted on 2026-10-08. Counts and commands below apply to the dated source snapshot. See [current HAL layout and build scope](hal-production-layout.md).

# Stage 7: classic ADC, basic timers and corrected FLASH lock maps

This source checkpoint adds classic blocking ADC support to F002/F003,
L031/R031/W031 and L052/L083, extends ADC12/GTIM to F020, and adds polling BTIM1–3
on every family. The complete classic ADC scope is ten families and 44 chip/alias
selections. L010/L011 sequence ADC and L012 dual-ADC support are not in this snapshot.
No firmware was flashed or run on hardware.

## Implemented and constrained

ADC uses each actual PAC version, family electrical limits, reserved-bit policies
and package-qualified analog pins. R031's published logical input labels remain
separate from physical mux indices through the optional `adc_mux` metadata field.
F002 does not expose copied internal-reference/temperature channels. Radio-family
supply limits are checked separately. Operation remains blocking, software-triggered
single conversion; scan, DMA, hardware-triggered acquisition and owned external
references are unavailable. Validation errors preserve ADC state; channel setup may
already have configured a GPIO pin before a per-conversion timing error.

BTIM implements the existing low-level Timer through sealed BasicInstance, without
claiming GTIM compare/PWM capability. All three counters share a gate/reset group;
per-instance construction never asserts that reset, and Drop leaves the shared
clock enabled. Only the owned bank is configured/stopped. Incoming and HAL-owned
siblings are preserved. Linear-PSC hardware currently exposes the same documented
power-of-two divisor subset as classic timers.

Cascading, master/slave modes, TRGO and timer-triggered ADC remain unsupported.
The upstream-shaped `Peripheral.triggers` schema exists but has no curated routing
entries. No universal TriggerSource or guessed ITR mapping is exposed. A future
implementation must respect each destination instance's legal source/event mapping
and actual selector location. Raw PAC configuration must retain exclusive peripheral
ownership and must not be mixed with HAL methods that overwrite those controls.

## Measured combined regression

The frozen main-tree matrix completed at 2026-10-08 10:09:48 UTC:

- All 54 HAL feature selections across 13 families
- 108 optimized Cortex-M0+ builds, with `rt` and `rt,defmt`
- 11,411 unit + 24 interrupt-binding + 1 API + 144 documentation test executions
- All 896 scoped inputs unchanged during the run

Focused main integration checks additionally include:

- ADC: 44 ARM positive controls, 590 diagnostic-specific negative cases and
  21 newly supported exact-package ELF links; seven own-source route/electrical
  audits and round-trip/default-omission tests for explicit mux metadata
- BTIM: 2,514 host executions across 13 representative family selections,
  13 ARM positives, 78 negatives, and 13 inspected exact-part ELFs
- Existing GTIM/PWM: three positive controls and 39 negative cases
- F020 ADC/PWM: four-feature validation, qualified 9/11/13 analog and 17/32/46
  PWM routes by exact package, and three ADC/PWM/counter firmware links
- Updated legacy family API suites, retaining precise unavailable ADC or PWM
  controls rather than rejecting newly implemented modules

Separate independent reviews accepted ADC electrical/state/ownership behavior,
BTIM shared-resource sequencing and exact source-hash correspondence. These are
source, host-model, type-system, build and link checks. They do not establish
silicon timing, electrical accuracy, RF coexistence or low-power behavior.

## Final small validation changes

The full matrix reported the same unused-mut warning on three F002 test builds.
Only the cfg(test) reference-vector construction changed; its tested reference
values/order are identical. All three F002 full host suites and two non-F002
reference controls passed afterward: 611 executions, no warnings. Repository-wide
formatting is clean. Production, generated data and PAC bytes are unchanged.

CI additionally invokes the already qualified BTIM ELF suite after its matrix-only
exit. This validation-only extension does not alter the executed matrix branch.
The complete delta is recorded in `verification-logs/stage7-combined/final-delta.json`.

## Data/PAC and portable evidence

Full `./d test` and `./d check` passed, including deterministic regeneration and
all 54 PAC selections. Own-manual FLASH corrections remove reserved LOCK8/9 on
F002 and LOCK8–15 on F020, preserving F003 and x030's valid wider maps. Ten positive
and 76 negative compile controls cover the split. No WAIT, capacity, controller
mode or unrelated field changed. The ledger now has 135 exact canonical templates
for the same 252 source-version identities.

The acquisition lock now restores 398 files, including 24 added BTIM SDK members;
all 41 original URL/version/hash pins remain unchanged. Empty-root reconstruction,
48 BTIM source fingerprints and 18 acquisition safety tests pass. See
`evidence-acquisition-stage7-verification.json`. Vendor files, toolchains and build
caches are excluded from the source archive.

During integration, a stale copied-source timestamp and an accumulated incremental
cache were detected and corrected before accepted runs. Changed build inputs were
refreshed, and `./d` now disables incremental compilation for per-chip matrices.
An old global-template-count assertion was narrowed to its actual GTIM-merge scope;
BTIM cfg validation recognizes the declared hardware versions explicitly. Failed
attempts are retained separately and are not counted as accepted checks.

## Remaining scope

`hal-coverage.json` lists exact per-family/peripheral support. Remaining work includes
sequence/dual ADC, other GTIM/ATIM/LPTIM/AWT modes and variants, timer routing,
FLASH storage, RTC, comparators, LCD, crypto/radio functions, external/PLL clock
paths, the Embassy time driver and safe DMA cancellation/integration. Separately
prepared low-power ADC and FLASH candidates are excluded from this checkpoint.
The experimental unsafe x030 DMA may drain indefinitely on Drop.

Detailed reports live under `verification-logs/stage7-combined/`,
`verification-logs/stage8-adc-merge/`, `verification-logs/stage7-f020/` and
`verification-logs/btim-merged-verification.json`.
