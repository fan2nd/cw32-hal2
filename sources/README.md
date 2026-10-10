# Official sources and provenance

This directory separates external evidence from the project's curated hardware
description in [`cw32-data/`](../cw32-data/).

- [`evidence-sources.json`](evidence-sources.json) is the single authored authority
  for official download URLs, selected revisions, exact SHA-256 hashes and byte
  sizes, SDK member chains, text transforms, source IDs, upstream commit pins,
  acquisition facts and license notes. Unknown dates remain explicitly unknown;
  the selected pin is not a claim that it is the newest vendor release.
- [`catalog.json`](catalog.json) preserves the recorded official website product
  catalog and its source links. These catalog observations do not establish
  register compatibility or HAL support. Reviewed datasheet facts take precedence.
- [`layout-history.json`](layout-history.json) records authored path relocations
  so older source proofs remain interpretable without rewriting historical hashes.
- [`SOURCES.md`](SOURCES.md) is the YAML-to-original reading index, including
  selected document versions and recorded page/member locators. It preserves the
  distinction between direct citations, historical reviews and remaining gaps.
- [`REFERENCE-PACKAGE.md`](REFERENCE-PACKAGE.md) describes the delivered subset
  and how to fetch the complete originals. [`approved-sdk-members/`](approved-sdk-members/)
  contains 11 unchanged SDK chip headers with explicit Apache-2.0 notices, the
  official license, provenance README and checksums. Only those exact locked
  members are approved; their file-level license does not cover complete SDKs.
- `vendor/` is an ignored local cache for original SDK ZIPs, SVDs, manuals, HTML,
  extracted SDK members and derived PDF text. Its contents are not distributed.
- `../build/provenance/` contains generated `source-lock.json`,
  `vendor-sources.json`, `reference-index.json` and `SOURCE-CATALOG.md`. These
  compatibility and navigation views are excluded from source bundles and are
  regenerated from authored inputs; they are never a second source authority.

Curated YAML cites stable `source_ref` IDs from the lock: `vendor:<filename>` for
original artifacts, `member:<pinned path>` for SDK members, and `text:<pinned path>`
for PDF text. Existing local paths, hashes and URL fields remain compatibility
data and are checked against those IDs. Claim-specific page, section and table
attribution stays with the curated fact. JSON Pointer syntax in the generated
index addresses parsed YAML as well as JSON.

Install the pinned Python dependencies from `requirements-dev.txt`, then run:

```sh
# Recreate the four provenance views without fetching vendor files.
python3 cw32-data/tools/source_provenance.py --write
python3 cw32-data/tools/source_provenance.py

# Fetch only the exact SVD inputs used by the generator.
python3 cw32-data/tools/fetch_sources.py

# Acquire the 43 hardware PDF/SDK originals only, without derivatives or reports.
python3 cw32-data/tools/acquire_evidence.py --originals-only
python3 cw32-data/tools/acquire_evidence.py --originals-only --verify

# Acquire all hardware-required originals and selected derivatives.
python3 cw32-data/tools/acquire_evidence.py
python3 cw32-data/tools/source_provenance.py --sources sources/vendor

# Optional strict replay of all 45 selected originals, including historical HTML.
python3 cw32-data/tools/acquire_evidence.py --verify --include-discovery

# Separately observe current discovery pages without accepting or repinning them.
./d refresh-discovery
```

Default receipts explicitly report 43 required originals and two omitted discovery
records; `complete_manifest=false` distinguishes this hardware scope from an
all-record archival replay. See [discovery policy](../docs/a030-discovery-policy.json).

Hardware acquisition defaults to `sources/vendor/`; `--source-root` or `CW32_SOURCES`
can select another local cache. `--cache-root PATH --offline` can reconstruct from
existing hash-verified originals. Acquisition never silently accepts changed
bytes or replaces a pin. PDF text must match the recorded Poppler output exactly.

The lock retains historical source hashes, ambiguous printed versions, license
scope and acquisition reporting precision. Historical generated run receipts are
optional workspace records, not bootstrap dependencies; the small acquisition
facts they established are preserved in the lock. Vendor redistribution rights
are not assumed. See [`docs/source-provenance.md`](../docs/source-provenance.md)
for review and refresh steps, and [`docs/SOURCE-BUNDLE.md`](../docs/SOURCE-BUNDLE.md)
for distribution scope.

The lock also lists `project_audit_inputs`: explicitly retained, byte-pinned historical acceptance records that cannot be regenerated from the current checkout. These are distinct from disposable build receipts. Their presence and hashes are mandatory, and they make no legal-clearance claim.
