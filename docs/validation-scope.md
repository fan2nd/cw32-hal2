# Current validation scope

## Commands

- `./d test`: generator/schema tests and current data/PAC contracts. Existing
  metadata tests can also consult pinned source fixtures; this is not an
  assertion that the command runs without source inputs.
- `./d audit-current`: verifies acquired official source bytes, current source
  references, official register/pin/clock facts, and their current projections.
  It also runs the existing source packager to validate actual current-file
  independent-review identities and distribution exclusions.
- `./d check`: regenerates the complete data and PAC, checks byte determinism
  against the current generated baseline, verifies authored register YAML remains
  unchanged, and verifies chip-feature selection/builds. Generated outputs need
  not be tracked or distributed.
- `./ci/check-hal.sh`: builds production ARM libraries and links real firmware.
  This does not execute firmware or establish hardware/runtime correctness.
- `./d lint`: optional Rust module-layout/style checks.

A failed command remains failed. Missing sources are not a successful audit.
The commands are separate scopes, not interchangeable pass labels. Production
validation requires all of the first four commands on the same frozen inputs.
HAL host/unit/model tests have not been restored.

## Historical review boundaries

The RTC and timer checkers no longer reverse each later metadata/schema change
in order to reproduce an earlier whole-file digest. Existing historical reports
remain unchanged, describe only their recorded snapshots, and are not reported
as passing again against current files. In particular retain:

- `docs/rtc-pac-corrections.json` and `docs/timer-command-evidence.json`;
- the existing HSE, VREF, HEX, schema and register-reuse review/addition records;
- `sources/layout-history.json`, source-lock previous snapshots and pin histories;
- original acquisition records and verification logs, including failed runs;
- `docs/upstream-file-provenance.json` and its independent-replacement reviews.

The last pre-simplification checker identities were:

- RTC: `ab0b5106462d2f61c0a25a82ed8b8c33d3a61498fd216fc28c0d38de0b7fd547`
- timer: `6759e960630a8de9c2ce46a4586363821f082e334cceffffc6e8fc62764b8a3f`

These identities locate dated checker inputs; they do not assert that either
checker passed on the latest snapshot. The preserved change patch identifies
removed historical reconstruction code. Reproducing a historical review needs
its actual old inputs, rather than successively undoing new features in current
files. The RTC optional `--compare-baseline` remains an explicit old-boundary
comparison and is not part of current validation.

## Retained checks and deliberately limited claims

RTC still verifies official PDF/SVD identities and field/access preconditions,
current corrected fields and canonical reuse identities, complete own-manual
register maps, generated chip/register selection, and real PAC contracts.
The shared `restore_classic_gtim_modes` helper remains for the separate
timer/ADC ISR checker; its existing scoped migration check is unchanged. This
short batch removes the RTC/timer-command whole-ledger/schema reconstruction,
not every historical check in the repository.

Timer still verifies official PDF/page bytes, ICR reset/W0C/reserved-bit policy,
CNT access, current authored overlays/reuse identities, generated projections,
and actual PAC compilation. Unrelated historical overlay hashes and schema
ancestry reconstruction no longer belong to the timer command.

ADC/RCC audits retain official voltage, frequency, oscillator tolerance and
Flash tables plus authored/generated metadata comparisons. Exact HAL source
strings are no longer tested. In particular these audits do not prove runtime
validation ordering, wait implementation, or clock-source selection. Those need
current production source review and, for execution claims, hardware evidence.

Current official source identity checks, current independent-replacement
identity/review requirements, copyright/licensing records, and distribution
exclusion checks are unchanged. This simplification grants no legal clearance,
changes no official source pins, and does not repin historical evidence.
