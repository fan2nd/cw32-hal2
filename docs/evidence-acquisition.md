# Reproducing the official-source audits

The source-only distribution deliberately excludes vendor SDKs, manuals,
datasheets and captured website pages. `./d fetch-sources` restores the selected
SVDs used by the generator. `./d fetch-evidence` restores the additional official
inputs needed by source-based tests and HAL audits.

## Quick start

Requirements: Python 3, Poppler `pdftotext`, and HTTPS access to `www.whxy.com` and, for the pinned I2C bus specification, `cache.nxp.com`.
The acquisition helper itself uses only the Python standard library. The audits
also use the repository's existing Python dependencies (`PyYAML` and `PyMuPDF`),
plus a host C compiler for the separate CMSIS layout audit. Install the pinned
Python audit dependencies with `python3 -m pip install -r requirements-dev.txt`.
Rust checks retain the toolchain requirements in the main README.

```sh
# Use an explicit location to avoid depending on another machine's source tree.
export CW32_SOURCES="$PWD/sources/evidence"
./d fetch-sources
./d fetch-evidence
./d test
```

`./d` and the aggregate HAL runner normalize the legacy `CW32_SOURCE_DIR` and
`CW32_SOURCE_ROOT` aliases to `CW32_SOURCES`. For direct test invocations, export
all three if exercising older scripts:

```sh
export CW32_SOURCE_DIR="$CW32_SOURCES" CW32_SOURCE_ROOT="$CW32_SOURCES"
```

The standalone helper defaults to `CW32_SOURCES`, or the ignored repository-local
`sources/evidence` directory when it is unset. `./d` also recognizes the historical
`/workspace/shared/cw32-sources` directory when present; an explicit
`CW32_SOURCES` always takes precedence.

## What is locked

[`sources/evidence-sources.json`](../sources/evidence-sources.json) pins:

- 12 SDK ZIP archives, with 366 exact C/header/SVD/PDSC/readme members. Nested PACK
  members are named explicitly. No complete SDK tree is blindly extracted.
- 28 manual/datasheet/specification PDFs, including separate historical and current F020
  datasheet revisions and the canonical L011 manual from 2026-06-02.
- 27 layout-preserving text companions generated from those PDFs.
- Two A030 official product-page snapshots used by the CRC source audit.
- Two local metadata documents required by DMA re-extraction. These contain
  only the already-pinned official URLs, document names and hashes.

Every downloaded original, extracted member and generated text has an exact
SHA-256 and byte length. Each original also names its official HTTPS URL and
its in-repository evidence references. The full materialized input set is
437 files, about 266 MB (254 MiB), excluding filesystem overhead. The manifest
contains metadata only, not vendor source code or document content.

The PDF text hashes were verified with Poppler **25.03.0** using
`pdftotext -layout -enc UTF-8`. A different version is acceptable only if its
output is byte-identical. A changed text hash is an error, never silently
normalized or repinned. A text mismatch prints the installed version; use the
verified version or deliberately review/re-pin the extraction with its audits.
The helper does not install software or execute vendor code.

## Offline recovery and verification

An optional cache supplies only original downloaded archives/PDFs/HTML. Each is
hashed before copying. Derived SDK files are re-extracted, and PDF text is
recreated rather than trusting preexisting cache derivatives.

```sh
python3 cw32-data/tools/acquire_evidence.py \
  --source-root /tmp/cw32-clean-inputs \
  --cache-root /path/to/previously-acquired-evidence \
  --offline

python3 cw32-data/tools/acquire_evidence.py \
  --source-root /tmp/cw32-clean-inputs --verify
```

Without `--offline`, missing cache originals are downloaded from their pinned
URLs. `--verify` performs no writes and no network requests. Omitting a cache
requires no prior vendor files at all:

```sh
python3 cw32-data/tools/acquire_evidence.py --source-root /tmp/cw32-online-inputs
```

To diagnose one source, use repeatable `--only` arguments naming its manifest
path. This also creates that source's derivatives, but does **not** claim a
complete input tree or generate the two complete-tree metadata documents:

```sh
python3 cw32-data/tools/acquire_evidence.py \
  --source-root /tmp/cw32-download-probe \
  --only CW32F002_StandardPeripheralLib_V1.2.zip
```

## Safety and source changes

- Existing files are verified, never overwritten. Human edits, corrupt caches,
  changed upstream bytes and wrong archive members cause an explicit failure.
- Publication is atomic and no-clobber. Interrupted/failed work leaves any
  already-verified files usable; rerunning safely continues the remaining work.
  Temporary partial files are removed by the failing invocation.
- Cache originals are copied, not hard-linked, so editing a recovered source
  cannot mutate a separate cache entry.
- ZIP/PACK paths are checked for traversal, absolute/drive paths, backslashes,
  symlinks, special files, duplicates and encryption. Only named members are
  read, with bounded nested-member sizes; `extractall` is never called.
- Source/cache paths containing symlinks are refused. HTTPS downloads and
  redirects must remain on the narrowly allowed official `www.whxy.com` or `cache.nxp.com` hosts. Exact URL/hash pins still apply.
- There is no `--force`, latest-version discovery, unpinned fallback, or automatic
  manifest update. A changed source must be investigated and deliberately
  reviewed before both evidence and acquisition pins are updated.

On 2026-10-08 the A030 page snapshots required a deliberate pin refresh: only
breadcrumb `/index.php/` placement changed. The product name and shared
CW32F030/CW32A030 manual links were unchanged. The SDK category was empty in
both snapshots and is negative evidence, not an independent A030 SDK. Previous
hashes and the review reason are retained in the acquisition manifest and CRC
evidence. Future page changes still fail closed.

Files remain local ignored inputs. No permission to redistribute official SDKs,
PDFs or HTML is assumed. Do not include the acquired tree in source releases.

## Explicit source replays

`./d test` exercises the source inputs through the normalized environment and
explicit `--sources`/`--manual-dir` options. The helper does not run tests itself.
Additional useful direct replays include:

```sh
python3 tests/audit_wdg_sources.py
python3 tests/audit_crc_sources.py
python3 tests/audit_window_watchdog_sources.py
python3 tests/audit_l011_manual_sources.py
python3 tests/check_gpio_irq_sources.py
python3 cw32-data/dma/build_verified.py --check --source-root "$CW32_SOURCES"
python3 tests/verify_l031_r031_w031_uart_sources.py --sources "$CW32_SOURCES"
python3 tests/verify_remaining_uart_sources.py --sources "$CW32_SOURCES"
python3 tests/verify_remaining_serial_af.py --sources "$CW32_SOURCES"
python3 tests/verify_final_serial_af.py --sources "$CW32_SOURCES"
python3 tests/verify_f020_serial_af.py --sources "$CW32_SOURCES"
python3 tests/verify_x030_af_tables.py --sources "$CW32_SOURCES"
python3 tests/audit_cmsis_layouts.py --sources "$CW32_SOURCES" \
  --expect docs/cmsis-layout-audit.json
python3 cw32-data/tools/extract_pinouts.py "$CW32_SOURCES" --check
```

These are source, metadata and host-layout checks. They do not execute firmware,
validate silicon, or establish electrical/timing behavior.

## Recovery validation

The acquisition helper has dedicated offline security and recovery tests:

```sh
python3 tests/test_evidence_acquisition.py
```

The dated clean-root recovery, real official HTTPS download probes and explicit
source replays are recorded in
[`evidence-acquisition-verification.json`](evidence-acquisition-verification.json).
A successful cache recovery is distinguished there from network downloads; it
is not described as an all-network download of every original.

### Stage 7 BTIM source closure

The current lock adds 24 BTIM driver/header members from the same 12 previously
pinned SDK archives. All 48 BTIM PDF/text/SDK source pins are covered. No original
URLs, versions or hashes changed. The Stage 6 receipt above is retained unchanged;
current-lock empty-root recovery and source-hash results are recorded separately
in [`evidence-acquisition-stage7-verification.json`](evidence-acquisition-stage7-verification.json).

### Provenance closure and navigation

The [source catalog](../build/provenance/SOURCE-CATALOG.md) and
[reference index](../build/provenance/reference-index.json) are generated from this lock.
The [refresh process](source-provenance.md) distinguishes filename from printed
version, cover/revision/publication from acquisition dates, and current from
superseded hashes. This closure adds 38 already-cited SDK members (12 PDSC and
26 FLASH/RTC files) from unchanged archive pins. See
[source-provenance-verification.json](source-provenance-verification.json) for
this lock's own recovery and negative-test results; earlier receipts remain
historical evidence of their recorded manifest hashes.
