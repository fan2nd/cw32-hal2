# Independent technical review of schema/proc-macro replacement

Reviewed on 2026-10-08. This is a compatibility and provenance review, not a legal
clearance opinion about the upstream repository or originality under law.

The independently authored baseline patch has SHA-256
`908ccc6eb476bb7421a9d312c627a97b90df076e6e6b211216537ab362b1e152`.
The reviewer reconstructed all 16 resulting file hashes by replay, checked all
recorded predecessor hashes, and compared the factual schema/default/enum and
macro behavior with the stated contract. No implementation mismatch was found.
The complete 337 generated-data and 471 PAC-file inventories are byte-identical.
Recorded all-feature evidence covers 54 metadata selections (162 tests), 54 ARM
PAC checks and three HAL checks. A fresh independent rerun passed all 17 new
cases: eight schema, two macro-unit, three macro integration and four compile-fail
documentation tests.

Integration conditions:

- The source-evidence JSON pointer resolves after the provenance lock is merged.
- Previous source bodies, saved bases, and replacement patch deletion hunks are
  not part of the released source archive. Factual origin and hashes are retained.
- Changes after this baseline, including optional trigger model fields, require
  explicit hash/provenance updates and their own compatibility checks.

The five new source/manifest/test files are recorded by exact current hashes in
`docs/upstream-file-provenance.json`. The unresolved upstream license finding
remains recorded; replacement does not retroactively license its predecessor.

## Later SPI metadata extension

The optional `Peripheral.spi` field and project-authored `Spi` record were added
and independently reviewed after the baseline above. See
[the narrow extension review](spi-metadata-refactor.md#schema-extension-review)
and its before/after hashes in `spi-metadata-refactor.json`. Historical JSON
defaults and omission are preserved; external Rust `Peripheral` struct literals
need the new `spi` field. The original replacement hashes remain historical
baseline evidence and do not purport to hash the extended file.

## YAML source-format fixture dependency

The independent 2026-10-09 review accepts the sole additional
`serde_yaml.workspace = true` line under `cw32-data-serde` dev-dependencies.
Deleting that line reconstructs accepted manifest SHA-256
`09466cbde9ae925db9a1972454d32dea837c707eb0c6443dcf182e77ec8ef224`
exactly; the updated manifest is
`8ac1cc060596cb9b1fa7f8acaf41affe2f63928ec5ca56ac58204f5ce747682f`.
The existing strict field-access schema fixture now reads the authored YAML and
retains its JSON round-trip and rejected-value checks. No predecessor body or
production schema change is introduced. This factual identity review does not
resolve rights in external predecessor material or constitute legal clearance.
