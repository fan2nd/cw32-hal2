# Independently authored chip schema and enum formatting

Date: 2026-10-08.

## Scope and history

This change replaces `cw32-data-serde/src/lib.rs` and
`cw32-data-macros/src/lib.rs`, their package manifests, and the macro test fixture
with project-authored implementations and tests. The chosen license for these
new project-authored files is MIT OR Apache-2.0.

The replaced files had ancestry in `embassy-rs/stm32-data` at commit
`37a22f31552ba1fd29b3ef192c4578b84abee6e1`. The audit did not find an applicable
license declaration for those originating schema/macro packages in the inspected
pinned source. The old SPDX/package labels were therefore insufficient evidence
of permission. This replacement does not assert a license for the originating
packages or erase their historical relationship to this workspace.

Exact predecessor and replacement hashes, upstream counterpart hashes and the
basis for the replacement are retained in
[`schema-reimplementation.json`](schema-reimplementation.json). Other
upstream-derived files and vendor data remain subject to their own provenance
reviews; this document is not a blanket redistribution clearance.

## Compatibility inputs and implementation boundary

The replacement author did not inspect the predecessor or upstream implementation
bodies for either library. Inputs were generated chip JSON, consumer construction
and use sites, public metadata models, existing tests, and a separately prepared
factual inventory of public types, fields, trait implementations, wire forms and
helper behavior. Copies of predecessors were hashed for audit and patch creation,
not used as implementation templates. The small old macro test was inspected as
a consumer fixture before its ancestry was identified; it too is replaced.

The new schema uses a shared project-authored record declaration mechanism with
explicit field attributes. It preserves public namespaces and field construction,
integer widths, field order, ordering traits, omission/default behavior, untagged
clock representation, and the existing permissive unknown-field behavior. It
performs no new hardware validation. In particular, required empty AFIO arrays
still serialize away but remain required when decoding, and ADC mux values remain
independent of signal names. The public `regex!` helper now uses a standard-library
`OnceLock` per call site and a hygienic exported Regex alias; the old thread-local
implementation dependency is removed.

The new `EnumDebug` derive validates input shape with explicit diagnostics and
emits qualified enum names using standard formatting builders. Unit and
single-payload tuple variants retain compact and alternate Debug formats, which
are inputs to generated Rust metadata. It also supports ordinary constrained
generics and recursive payloads. Unsupported named, zero-field tuple, multi-field
tuple, struct and union shapes receive errors rather than macro panics.

## Verification

The companion verification manifest records exact input, output and log hashes.
Checks include every curated chip's JSON round-trip, declaration/enum ordering,
missing fields, required collections, numeric bounds, defaults, optional values,
unknown fields, enum forms, lazy regex behavior, macro diagnostics, generic and
recursive enum cases, and compile-fail examples. Generator tests and independent
metadata contracts are rerun. Fresh complete chip data and formatted PAC trees
are compared byte-for-byte against the frozen curated baseline. Every generated
PAC selection is checked on Cortex-M0+ and runs host metadata tests; representative
HAL consumer configurations are compiled with runtime and defmt support.

These are software and compatibility checks. They do not validate hardware or
establish legal conclusions about upstream material. The pending typed-trigger
route extension is a separate narrow change; this replacement first preserves
the existing `Trigger { signal, source }` API and unchanged generated outputs.

## Later SPI metadata extension

The optional `Peripheral.spi` field and project-authored `Spi` record were added
and independently reviewed after the baseline above. See
[the narrow extension review](spi-metadata-refactor.md#schema-extension-review)
and its before/after hashes in `spi-metadata-refactor.json`. Historical JSON
defaults and omission are preserved; external Rust `Peripheral` struct literals
need the new `spi` field. The original replacement hashes remain historical
baseline evidence and do not purport to hash the extended file.
