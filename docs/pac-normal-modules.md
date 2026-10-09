# PAC ordinary-module selection

The PAC generator declares shared, versioned IP implementations through ordinary
Rust modules. There are no module path attributes and no include-wrapper
substitutes for them.

## Organization

- `src/peripherals/mod.rs` contains cfg-gated `pub mod <kind>_<version>;`
  declarations; each implementation remains one flat sibling `.rs` file.
- `src/registers/mod.rs` uses the same normal declarations for the static register
  IR. The group is compiled only with the `metadata` feature.
- Each selected chip's `pac.rs` publicly reexports its chosen implementation under
  the existing public kind name, such as `spi`.
- Deduplicated chip metadata reexports its chosen register IR under the existing
  `<kind>_regs` name. Metadata types and register descriptions are unchanged.
- `chip_peripheral_versions.rs` is an ordinary build-script data module. It maps
  each chip selection to its concrete IP-version cfg predicates. It contains no
  peripheral implementation or register IR.
- `build.rs` retains the exactly-one-chip Cargo-feature check and emits only that
  chip's IP cfg predicates. It also declares every supported predicate to Rust's
  cfg checker. Enabling both `pac` and `metadata` does not compile other chips' IP
  versions.

The existing selected-chip PAC and deduplicated metadata `include!` mechanism is
retained. No new include is used to redirect module lookup. Shared versioned
sources remain deduplicated, and no register access or field behavior is changed.

## Verification of the module conversion

An isolated source snapshot was regenerated and checked on 2026-10-08:

- All 135 peripheral implementation leaf files were byte-identical before/after.
- All 135 static register-IR leaf files were byte-identical before/after.
- All 54 chip public peripheral alias maps were identical before/after.
- All 54 chip PAC source bodies were identical after removing module wiring and
  blank lines. Rustfmt groups the new public reexports, moving blank lines only.
- Actual generated build-script execution for all 54 chips, both with and without
  `rt`, emitted precisely each chip's required IP cfgs. Zero-chip and multi-chip
  configurations were rejected.
- Nine compiled Rust dependency manifests referenced precisely the selected
  chip's implementation leaves, without unselected versions.
- PAC and HAL ARM checks passed for F030, L012, and L083 with `rt,defmt`;
  PAC checks also enabled `metadata`.
- Separate metadata-only host and PAC-only ARM builds passed for L012.
- Three generator unit tests, 13 F030 PAC tests, three metadata tests, positive
  access controls and 27 negative register-access compilation tests passed.

`python3 tests/test_pac_module_selection.py` repeats the complete chip/module/alias
inventory and build-script selection checks. Generator unit tests cover ordinary
module declaration order and chip-specific build-table generation.

These are source and compiler checks. They do not execute firmware or validate
hardware. The final combined-tree full-feature matrix is a separate check.
