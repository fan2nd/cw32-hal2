# Source provenance and guarded review

## Start here

- [Human-readable source catalog](../build/provenance/SOURCE-CATALOG.md)
- [Canonical external artifact/member lock](../sources/evidence-sources.json)
- [Generated source-reference index](../build/provenance/reference-index.json)
- [Factory-HSI/ADC facts and precise citations](adc-clock-source-bounds.json)

`evidence-sources.json` is the one editable URL/hash/member authority. The legacy
`source-lock.json` and `vendor-sources.json` are deterministic compatibility views,
not independent places to update pins. They remain readable by the existing
Rust/Python generation tools. The reference index and catalog are generated from
that same authority and the original evidence documents.

Evidence documents retain their hardware claims, section/page citations and
interpretations. The index links exact JSON pointers to those documents rather
than duplicating an independently maintained statement of each hardware fact.
The normalized clock fact document uses `source_ref` IDs; URL/hash/revision are
resolved from the canonical lock. Its family HSI fields use one common schema.
Classic and low-family ADC details retain distinct audit keys because their
voltage-boundary and reference-source conditions are not interchangeable.

## Identity and dates

Each original records its official URL, exact filename, byte size, SHA-256,
filename version, actual printed revision, cover date, revision-history date,
publication-date basis, acquisition date (or null), chip scope and license review.
A PDF page number is 1-based and is separate from the page printed in the footer.
Do not assume a constant offset: L011/L012 documents contain nonuniform front
matter. The final revision-history page is individually recorded.

A URL upload directory, ZIP member timestamp, file mtime or review date does not
establish acquisition. Three logged vendor re-downloads plus the NXP download have a recorded 2026-10-08
acquisition date; the other first-acquisition dates are unknown. Existing clean
cache-recovery receipts establish recovery, not initial acquisition.

The historical F020 filename ends in V1.3 but prints Rev 1.2 (2023-02-14).
The selected current Rev 1.3 PDF (2025-12-30) is a separate URL/path/hash. Existing
historical citations remain replayable and the clock-bound facts use the current
file. L011's old September 2025 and selected June 2026 manuals both print Rev 1.1
but have different hashes. The old snapshot's original URL was not recorded;
that gap is explicit rather than reconstructed from its filename.

## Upstream, local generator and environment identity

The lock records full Embassy, stm32-data, chiptool and svd-parser commits.
Validation checks all Git sources in Cargo.lock against those pins. The pinned
upstream license review is component-scoped: stm32-data has no verified root
license in the inspected checkout, while its stm32-metapac-gen package explicitly
declares MIT OR Apache-2.0. Do not extend that grant to other components. Cargo.lock
retains exact registry versions/checksums; requirements-dev.txt pins Python audit
dependencies. The index includes those locks, Rust/cargo/rustfmt/target details,
Python and PDF extraction/inspection versions, and hashes of generator/transform
files.

This project snapshot has no recorded local Git commit for the CW32 generators.
Their package version is 0.1.0; the index's complete Rust source/Cargo.toml hash
manifest provides the identity. A future committed release should additionally
record its verified project commit. Do not pretend an upstream chiptool commit
is the commit of this project's generator.

## Validate and regenerate

```sh
# Offline metadata, source-reference closure, dependency pins and view freshness.
./d provenance
python3 tests/test_source_provenance.py

# Deliberate regeneration of navigation/compatibility views only.
./d provenance --write

# Re-acquire fixed-version inputs locally; never distribute this directory.
python3 cw32-data/tools/acquire_evidence.py \
  --source-root /tmp/cw32-evidence --cache-root /path/to/cache --offline
./d provenance --sources /tmp/cw32-evidence
python3 cw32-data/tools/acquire_evidence.py --source-root /tmp/cw32-evidence --verify
```

View generation does not download, change source pins, edit authored registers,
change HAL/runtime code, or accept new vendor content. Missing evidence paths,
unresolved source IDs, unknown URL/hash pairs, unknown external file hashes,
invalid PDF page references, dependency-pin drift and stale views fail closed.
The scanner covers source metadata JSON, top-level evidence JSON and source-test
manifests. Narrative-only claims still require human review: this validator does
not prove that a cited table supports its author's interpretation.

The release packager rejects any file whose bytes match a locked vendor
original, member, derived text or retained historical snapshot, even if renamed.
The existing ignored `sources/`, `build/` and target exclusions still apply.
This byte guard is not a general copyright detector; reviewers must also exclude
new copied page images, long extracts and other unreviewed vendor material.

## Update and correction review

1. Identify an actual need for a source change; never follow a mutable "latest"
   link or silently replace an existing pin. Fetch candidates into a separate
   ignored directory. Preserve the selected original and its hash.
2. Inspect the official artifact itself: cover/footers, revision history, chip
   scope, relevant pages/sections, SDK member paths and notices. Record differing
   filename/printed versions and cover/history dates. Record acquisition only
   when a download receipt is available.
3. Review changed bytes and the affected hardware claims. A newer manual is not
   automatic proof that a vendor SVD or previous reviewed correction is wrong.
   Keep conflicting statements and the driver boundary visible. Do not borrow
   another family's oscillator accuracy from register layout reuse.
4. Add the candidate with a distinct path/ID and hash; mark the relationship to
   the old pin explicitly. Update dependent evidence only after review. Acquire
   archive members by exact nested chain and SHA-256. Do not overwrite the source
   tree or repin to make a failing test green.
5. Run `./d import-registers` to produce candidates under
   `build/register-candidates/`. Compare them with authored
   `cw32-data/registers/*.yaml`. Never treat imported output as permission to
   overwrite curated definitions.
6. A curated fix must identify the exact source hash, old value, proposed value,
   register/field, section/page basis and rationale. Preserve the import guards
   that reject a changed source hash or changed expected old value. Update
   per-family input corrections, version mapping and regression tests together.
   Conflicting unresolved facts remain in `docs/known-source-conflicts.json`;
   project corrections are not labeled vendor-issued silicon errata.
7. Regenerate views with `./d provenance --write`, review the diff and rerun
   metadata/source, generation-parity and affected driver tests. Run full
   `./d test` and applicable ARM checks before an integrated release. Preserve
   historical verification receipts with their original manifest hashes; create
   a new receipt for the newly reviewed lock.
8. Recheck raw redistribution permission before packaging. The current policy
   excludes all vendor original/derived inputs. Record precise component scope if
   permission is later established; a CMSIS header's Apache-2.0 notice never
   grants permission for an entire SDK, SVD or manual.

## Current closure review

The initial provenance closure found 38 already-cited SDK members absent from
the common acquisition lock: 12 DFP/PDSC device descriptions and 26 FLASH/RTC
implementation/header files. Every addition was matched by its existing evidence
SHA-256 inside its already-pinned official archive, including nested PACK files.
No original URL/hash/revision changed. Source recovery now includes those members.
There are still no hardware-in-loop results, and correct source attribution does
not establish oscillator performance or silicon errata compliance.

## Historical upstream license gap and reviewed replacements

The targeted [per-file ancestry review](upstream-file-provenance.json) records
that the former macro implementation reproduced its upstream body and the former
schema implementation retained substantial upstream source. The originating
packages have no verified license in the pinned tree; the earlier project
SPDX/package assertion alone did not establish redistribution permission.

The five live implementation/manifest/test files have now been independently
reimplemented from factual API and serialization contracts and technically
reviewed. [Review evidence](schema-reimplementation-review.md), original hashes,
replacement hashes and byte-identical generation results are retained. This is
not a legal clearance opinion and does not retroactively license the old code.
The separately licensed stm32-metapac-gen package is not used to justify a
repository-wide license claim.

The release guard requires exact reviewed replacement bytes. Historical source
snapshots and patches containing deleted predecessor bodies are excluded from
the source archive. Raw upstream repositories and vendor inputs are not bundled.

## I2C bus specification

NXP UM10204 is pinned as printed Rev 7.0, 1 October 2021, with Table 11 on
printed/PDF page 44. Its official download URL has a mutable filename, so the
SHA-256 is mandatory. The 2026-10-08 download is recorded separately from the
2021 publication date. Only the factual citation and fixed-byte acquisition pin
are included; raw PDF and extracts are not redistributable project assets.
