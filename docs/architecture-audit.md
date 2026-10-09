> Historical software-verification references: HAL tests and fixture harnesses cited below were deleted on 2026-10-08. Their reported results apply to the dated source snapshot only; source facts remain unchanged. See [current HAL layout and build scope](hal-production-layout.md).

# Data/PAC architecture audit

Audit date: 2026-10-08. Scope is data and PAC, not completion of a HAL. The
pinned upstream checkouts were inspected directly; a similar directory name or
successful compile alone is not evidence of architectural or silicon parity.

## Reference contract

- [stm32-data README](https://github.com/embassy-rs/stm32-data/blob/37a22f31552ba1fd29b3ef192c4578b84abee6e1/README.md)
  and [register loader](https://github.com/embassy-rs/stm32-data/blob/37a22f31552ba1fd29b3ef192c4578b84abee6e1/stm32-data-gen/src/registers.rs):
  initially extract SVDs, then commit and manually maintain normalized versioned
  register YAML; normal generation reads YAML and writes intermediate JSON.
  Upstream explicitly avoids replaying register patches over SVDs on every build.
- [stm32-metapac generator](https://github.com/embassy-rs/stm32-data/blob/37a22f31552ba1fd29b3ef192c4578b84abee6e1/stm32-metapac-gen/src/lib.rs):
  consume chip/register JSON, construct chiptool IR, expand inheritance,
  sort/sanitize names, render reusable register modules and per-chip PACs.
- [chiptool](https://github.com/embassy-rs/chiptool/tree/be1bff3e9e1b27b090e69bd9ac753c66fdcce678):
  actual IR, transforms, validator, typed access model and Rust renderer.
- [embassy-stm32 build script](https://github.com/embassy-rs/embassy/blob/f16efeffe37581092ec184718e6fdb1620393214/embassy-stm32/build.rs):
  consume selected-chip static metadata and peripheral kind/version information.
  Copying its metadata consumer cannot supply missing CW32 topology or drivers.

## Actual matches

1. The Rust data/PAC crate split is real. `cw32-data-serde` follows the upstream
   chip/core/peripheral/register-reference schema; the metapac generator is an
   attributed upstream adaptation, not a handwritten Rust register emitter.
2. Peripheral instances refer to `(kind, version, block)` register definitions.
   SVD `derivedFrom` instances share a block; GPIOC and GPIOF remain distinct where
   the source has different layouts. A030/F030 reuse is an explicit documented
   source relationship, not a same-name heuristic.
3. Per-chip Cargo features, selected-chip build environment, PAC modules,
   `device.x`, shared `Reg<T, Access>`, typed fields, static register IR,
   `ALL_CHIPS` and `ALL_PERIPHERAL_VERSIONS` follow upstream machinery. Exactly one
   chip feature must be selected; no implicit chip is supplied.
4. Source URLs/hashes, separate part catalog and source-correction evidence make
   imports reviewable. Independent XML comparisons and compiler-negative access
   fixtures complement checks of generated metadata reference closure.

## Register-authoring boundary

Normal generation now reads the checked-in `cw32-data/registers/*.yaml` as
its register source of truth, matching upstream's authoring direction. Register
YAML/JSON under `cw32-data/data/registers` is generated output. `./d import-registers`
is a separate explicit vendor refresh: it writes candidates under
`build/register-candidates` for review and never promotes them into authored YAML.

Reviewed access corrections and the obsolete RTC field removal remain in manifests
as provenance assertions and candidate-import corrections. Normal generation does
not replay them over SVD register definitions. Public-API tests demonstrate that
manually reviewed register offsets, field names and bit positions survive normal
generation, including edits beyond the current correction list. A source-parity
audit may deliberately fail for a new YAML correction until its evidence and exact
comparison expectation are updated; that is different from rejecting a valid
curated IR or silently discarding the change.

Candidate imports resolve paths before rejecting curated destinations, including
relative, `..` and symlink aliases. Isolated fixtures verify refresh cannot replace
authored YAML. Regenerated candidate YAML currently equals all 135 authored files,
and source-candidate generation rejects attempts to share different complete IRs
under one version while accepting distinct versions.

## Deliberate CW32 differences

- CW32 official SVDs, CMSIS evidence and datasheets replace STM32Cube ingestion.
  There is no reason to transplant STM32 RCC, AF or DMA wiring into CW32 data.
- The intermediate chip JSON lives under `cw32-data/data/chips`, not upstream's
  `build/data/chips`; generated artifacts are retained in this workspace.
- `Chip.device_id` is optional and omitted when unknown, rather than inventing 0.
- Vendor uppercase block references are sanitized with the same transform used
  for emitted IR. Metadata-only modules use an `_regs` suffix, avoiding the F003
  `ir` peripheral's collision with the static metadata `ir` schema namespace.
- Overlap checks allow only enumerated register aliases and validate byte spans;
  this is stricter than the pinned upstream register loader's permissive overlap
  settings. Unsupported nested peripheral inheritance and register arrays must
  not silently pass the current importer/auditor.
- Per-profile `register_versions` maps each kind to a canonical version. The
  `register-reuse.yaml` ledger reduces 252 source versions to 137 by exact complete
  IR equality, including descriptions, access, offsets, widths and fields. This
  is conservative deduplication, not a proof of complete semantic normalization.
  Canonical names retain an originating family/version label. The F030/A030 source
  relationship additionally has reviewed shared-reference-manual evidence.

## Incomplete data and remaining normalization

- The snapshot contains 54 generated chip features: 37 exact current-catalog
  parts, 13 generic family profiles and four F030 size aliases. This is not every
  historical ordering or shipping code. `additional-parts.yaml` separately
  records two documented F030 legacy name aliases and one F003 tape-and-reel SKU,
  mapping them to existing PAC features without counting new hardware or upgrading
  the legacy temperature rating. Generic profiles make no memory-size assertion.
- The 135 canonical register versions contain 1,678 register items and 10,122 fields
  after the reviewed RTC and F020 GPIO/CRC corrections. They remain mostly vendor-shaped: there are
  currently no register/field arrays or enum definitions. Repeated-register/field cleanup, consistent semantic naming,
  enum/documentation curation and cross-family compatibility review are unfinished.
- Static `Metadata` fields/types match the pinned upstream shape; the temporary
  additive `packages` field was removed. Chip JSON still carries exact catalog
  package labels and Flash/SRAM sizes. All 37 exact current packages now have
  source-backed physical pad positions/signals (1,482 positions). Exact-part
  core pins are package-filtered; generic aliases use conservative common sets.
- Peripheral IRQ associations preserve the source, including shared lines such
  as F030 DMACH23 for channels 2/3, FLASHRAM for FLASH/RAM and WDT for IWDT/WWDT.
  They use a generic `GLOBAL` signal, not upstream's richer per-signal topology.
  Header-sourced supplemental IRQs do not invent peripheral associations.
- Reviewed DMA topology now projects 37 channels and 371 requests across all
  13 family profiles, including four with no DMA. RCC projects 273 supported
  records while 161 partial clock records remain explicit sidecar gaps.
  AF routing is populated only for reviewed x030/F020 serial and x030 analog
  signals; remaining SDK candidates are not treated as verified mappings.
  Trigger routing, flash geometry and device identification remain incomplete.
- Register reset values and read-clear/write-one-clear semantics are absent from
  the pinned chiptool IR. SVD parity preserves source errors as well as facts.
  Current Chinese CW32x030 manual V2.5 supersedes the old English manual: its
  revisions removed RTC high-frequency 1 Hz compensation and changed SPI BUSY
  behavior wording. The obsolete `RTC.COMPEN.FREQ` field has now been removed from
  authored F030/A030 data with explicit bit-range/evidence checks; the broader
  behavioral differences are not fully reconciled field-level hardware validation.
- Zero chips are hardware-validated. A generated or compiling PAC does not prove
  register correctness, electrical behavior, package routing or HAL completeness.

## Contract checks

`python3 tests/test_metadata_contracts.py` passes 19 independent tests covering
all generated per-instance source references and IRQ associations, exact catalog
memory/package labels, shared IRQ ownership, documented source reuse, F030
package differences, and exact reviewed topology projections. Negative
fixtures reject valid-but-wrong block/family references, a wrong-but-resolvable
shared IRQ, an incorrect source-alias edge, dropped shared corrections, and
invented DMA routes or physical pin positions. The suite also checks exact static
metadata shape, canonical IR hashes/reference mappings, and non-inflating order
aliases. All 19 passed after authoritative-YAML migration, 135-version reuse and
the manual-backed register/topology corrections. The independent strict source-parity audit passed all 13
profiles, including canonical version references and the documented field removal.

`cargo test -p cw32-data-gen --test generation_boundaries` passes seven isolated
public-API tests for authored-change preservation, candidate refresh isolation,
relative/`..`/symlink output protection, incompatible-version rejection and valid
separate versions. These fixtures are synthetic contracts, not extra chip evidence.

Other coverage lives in `tests/audit_generated_parity.py`,
`tests/validate_pac_inventory.py`, the generated Rust metadata tests, access
compile tests and `./d check`. Check results describe exactly what ran; this audit
does not promote planned or partial checks to a completed full matrix.

## Requested module-directory organization

All project-owned file-backed Rust modules now use `<module>/mod.rs`, including
HAL backends and their tests, generator helper modules, generated PAC peripheral
and register modules, per-chip PAC/metadata modules and shared metadata modules.
The PAC generator and its templates produce this layout directly; regeneration
never relies on hand-edited outputs. Crate entrypoints (`lib.rs`, `main.rs`,
`build.rs`, integration-test crates) retain Cargo's standard names. Small inline
sealed/test namespaces remain inline. Vendored upstream implementation files are
not changed. This filesystem organization is an explicit user-requested
difference from some flat upstream files; public metadata shapes/APIs are not
changed by the move. `tests/test_module_layout.py` prevents regressions and
checks all explicit module-path attributes resolve to `mod.rs`.

## Hardware-meaningful cfg predicates

Custom cfg predicates now name concrete chip families, peripheral IP versions,
or explicit capabilities. Examples are `cw32l012`, `uart_cw32l031_v1`,
`rcc_cw32l011_v1`, `gpio_af`, `gpio_exti` and `crc_32bit`. Shared source files
select documented version unions rather than generic implementation labels.
The former HAL-prefixed predicates are removed from emitted cfg/check-cfg and
all active source consumers. `tests/test_cfg_names.py` validates the declared
set and rejects legacy labels. Naming and module-layout preflights run at the
start of the integrated HAL CI script. Public crate/API names are unaffected.
