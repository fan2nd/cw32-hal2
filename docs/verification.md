# Verified data/PAC checkpoint

Verified 2026-10-08 at 07:52 UTC with the toolchain in `toolchain.json`.

`./d test` and `./d check` completed with exit 0 after F020 FAULT ownership, confirmed GPIO ISR access, and031 UART TIMCNT corrections
and reviewed metadata enrichment. This includes 54 ARM selections (PAC,
metadata, runtime), 54 host metadata suites, three optional-defmt builds,
source/metadata contracts, compiler-negative register permissions and
byte-for-byte regeneration. All curated YAML hashes remained unchanged.
See `verification-logs/stage5-delta/data-test.log` and `stage5-delta/pac-matrix.log`.

The scope is 37 current catalog ordering codes across 13 family profiles.
The 54 selectable features also include 13 generic family profiles and four
F030 size aliases; they are not 54 distinct chips. Twelve SDK inputs and an
explicit common-manual A030/F030 relationship provide register sources. Three
additional documented order-code aliases add no distinct hardware features.

The canonical register pool contains 135 exactly normalized layouts after
manual-backed F020 GPIO access and CRC16 corrections. Full physical package
pins, reviewed DMA request topology and supported RCC identities are now
projected into upstream-shaped metadata. Source records retain partial and
unresolved cases rather than manufacturing associations.

## Remaining gaps

AF routing remains partial outside reviewed all-family serial and x030 analog
routes. Clock/reset records include 161 partial entries withheld from unsupported
projections. Flash geometry/device identifiers and some reset/clear semantics
remain absent. See source manifests and `cw32-data/coverage.json`.

HAL results are separate in `hal-coverage.json` and dated HAL reports. Earlier
clock tests were superseded by manual/electrical review; see
`stage3-corrections.md`. No physical board or electrical validation is claimed.
