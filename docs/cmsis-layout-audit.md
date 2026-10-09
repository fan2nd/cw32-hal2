# Reproducing the independent CMSIS layout audit

The recorded result is **4,339 shared register-address comparisons across 12
vendor SDK families, with zero mismatches**. It is reproducible independently of
the Rust generators and does not require generated PAC files.

## Files

- `tests/audit_cmsis_layouts.py`: independently written audit implementation.
- `tests/cmsis-layout-sources.json`: exact official SDK URLs, archive SHA-256,
  selected header/SVD SHA-256 and nested ZIP/PACK member paths. It also pins the
  expected comparison count for each family so an empty or partial parse fails.
- `docs/cmsis-layout-audit.json`: path-portable result, including every compared
  absolute address, unmatched register names and skipped peripheral views.

No vendor headers, SDK archives or SVD files are bundled in these files.

## Requirements

- Python 3.10 or newer, using only its standard library
- A host C11 compiler, such as GCC or Clang, whose resulting host executables can
  run locally (`cc` by default; override with `--cc clang`)
- The 12 exact official SDK ZIP archives listed in the input manifest

The source directory is a caller-selected **archive cache**, not an inferred
machine-specific path. Copy existing official archives into that directory, or
use the explicit download option below. The script verifies every archive and
selected member before compiling anything.

From the repository root, with an already populated external archive cache:

```sh
python3 tests/audit_cmsis_layouts.py \
  --sources /path/to/official-sdk-archives \
  --expect docs/cmsis-layout-audit.json
```

To acquire missing archives into the repository's ignored `sources/` directory
and then verify the saved result:

```sh
python3 tests/audit_cmsis_layouts.py \
  --sources sources \
  --download-missing \
  --expect docs/cmsis-layout-audit.json
```

To produce a separate result for review:

```sh
python3 tests/audit_cmsis_layouts.py \
  --sources /path/to/official-sdk-archives \
  --output /tmp/cw32-cmsis-layout-audit.json
```

`./d fetch-sources` obtains the SVDs needed by ordinary generation and may cache
SDK archives while doing so. However, an existing cached SVD can make that helper
skip its archive download. SVD files alone cannot reproduce this audit because
it also needs the pinned CMSIS headers. The audit's source manifest is the
complete acquisition contract; use its exact archives or `--download-missing`.

## Method and scope

1. Verify the selected official SDK archive and nested header/SVD members by SHA.
2. Extract plain CMSIS struct typedef declarations in memory. Replace CMSIS
   access qualifiers with corresponding C qualifiers and compile an independently
   written `offsetof` reporter in a temporary directory.
3. Run that host reporter. It only prints member offsets. It does not include or
   execute vendor startup/driver functions or access any MCU register addresses.
4. Parse SVD registers, including `derivedFrom` peripheral views, and compare
   `SVD peripheral base + register offset` against
   `header peripheral base + compiled offsetof` for shared register names.
5. Report one-sided names and skipped views explicitly. Enforce the pinned
   per-family comparison counts and exit nonzero for mismatches, invalid hashes,
   incomplete parses, compiler failures or a changed expected result.

This correctly recognizes that L031/R031/W031 timer SVD bases are 0x300 lower
than their CMSIS bases while the SVD member offsets are 0x300 higher. It also
avoids mistaking L010's stale IRMOD offset **comment** for its actual C layout.

This is a host ABI layout check of fixed-width integer register structs. It is
not a silicon test, a proof of vendor-header correctness, or a complete source
coverage claim. In particular it does **not** check bitfield positions, access
permissions, reset values, reserved-bit handling, atomicity or peripheral
behavior. Matching mistakes shared by the SVD and header remain possible.
Manual-backed corrections, such as the obsolete F030 RTC compensation field,
therefore remain necessary even when this audit passes.

The report compares pinned raw vendor sources. It is separate from generated
Rust PAC parity tests and from manual-backed corrections to the authoritative
register YAML.

## Source handling

The script reads archive members in memory and leaves vendor downloads only in
the caller-selected cache. Temporary C declarations and host executables are
removed after the run. No SVD redistribution permission is assumed; keep raw
vendor files out of distributable repository archives.
