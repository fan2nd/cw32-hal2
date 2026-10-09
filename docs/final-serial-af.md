# Final serial AF metadata promotion

Status: reviewed metadata and generator integration. No silicon or electrical
validation is implied. The earlier read-only implementation audits remain
historical records; this document describes the subsequent metadata change.

## Scope and results

The five new serial-only sidecars promote **340 independently source-checked
pin/selector/function cells**:

| Family | Reviewed cells | SDK-only cells excluded | Documented digital selectors |
| --- | ---: | ---: | --- |
| F002 | 42 | 10 | 1–7 |
| F003 | 52 | 0 | 1–7 |
| L010 | 43 | 0 | 1–7 |
| L011 | 70 | 0 | 1–7 |
| L012 | 133 | 0 | 1–9 |

No timer, analog, comparator, clock or other AF candidate is promoted. The
original candidate files are unchanged and remain explicitly unreviewed inputs.
Each manifest points to its own `*-serial.json` sidecar. All promoted routes
carry `source_kind=sdk-and-datasheet`, the original SDK function, macro and exact
line, the actual PAC instance, GPIO register/field, and an exact datasheet cell.
No invented SDK evidence or unrecorded source correction is necessary here.

The complete source hashes, URLs, PDF/printed-page coordinates, bounding boxes,
selector declarations and exact-package joins are in
[`final-serial-af-evidence.json`](final-serial-af-evidence.json). The source-backed
review can be reproduced with:

```sh
python3 tests/verify_final_serial_af.py --sources /path/to/cw32-sources
python3 tests/test_final_serial_af.py
```

`--write --sources ...` re-extracts and rewrites only these five serial sidecars
and their evidence document. It does not regenerate the PAC, change manifests,
modify other AF reviews, or promote unrelated functions.

## Exact sources and independent checks

The original official PDFs, SDK GPIO headers and existing package sidecars are
SHA-256 pinned. Source-mode validation reads the PDFs anew, using actual numbered
column centers and pin-row coordinates, including wrapped debug rows and blank
cells. It checks each serial SDK macro's recorded source line, peripheral/field
assignment and selector value. This is not a sequential nonblank-column parser.

- F002 DS CN V1.2 Tables 5-3–5, printed p23 / PDF24.
- F003 DS CN V1.9 Tables 5-3–5, printed p24 / PDF25.
- L010 DS CN V1.3 Tables 5-3–4, printed p24 / PDF25.
- L011 DS CN V1.1 Tables 5-3–5, printed pp28–29 / PDF31–32.
- L012 DS CN V1.0 Tables 5-3–6, printed pp36–37 / PDF39–40.

All 246 low-power cells additionally match the previously independent,
rendered-page review in `read-only-low-power-serial-af-audit.json`, including
its complete bounding boxes. F002/F003 extraction agrees with the prior rendered
AF review in `f002-f003-serial-read-only-audit.md`.

The ten F002 SDK-only cells concern **PA3, PB7, PC3 and PC4**. F002's own current
UM CN V1.4 Table 8-2, printed p104 / PDF105, independently omits these pads; that
manual hash, table and complete documented pad set are recorded in the new
proof. These cells are removed before package filtering. F003 documents all ten,
but only its TSSOP24 part bonds those pads. They are absent from F003's two
20-pin projections and common-package-intersection family alias.

## Explicit naming and selector policy

| Family | SDK name | Datasheet name | Exact PAC name |
| --- | --- | --- | --- |
| F002/F003 | SPI | SPI | SPI |
| F002/F003 | I2C | I2C | I2C |
| L010 | SPI1 | SPI | SPI |
| L010 | I2C1 | I2C | I2C1 |
| L011 | SPI1 | SPI | SPI |
| L011 | I2C | I2C | I2C |
| L012 | SPI1/2/3, I2C1/2 | Same numbered names | Same numbered names |

UART TXD/RXD become TX/RX. SDK CS or NCS and datasheet CS become NSS. The
candidate L010/L011 `SPI` signals such as `1MOSI` are explicitly normalized from
source function identity rather than creating nonexistent PAC `SPI1` instances.

`cw32-data-gen/src/af/mod.rs` uses a closed set of reviewed family capabilities.
It does not infer a selector ceiling from field storage width, apply L012's
AF8/9 support to other families, or silently admit an unknown family. Newly
promoted profiles also require a matching source-backed capability declaration.
The generator separately checks the actual PAC field width and offset.

The same capability records the PDF-to-printed-page offset: L011 and L012 have
three front-matter pages, while the other reviewed sources use one. Existing
031 wrong-selector and wrong-coordinate negative fixtures remain unchanged.

## Package and safety boundaries

Metadata reports factual bonded routes, including debug-pad alternate functions.
Safe HAL pin exposure is a separate GPIO/build responsibility: no serial
constructor may automatically remap SWD, reset or oscillator functions.

| Exact package group | All safe serial cells | TX/RX/SCK/MISO/MOSI/SCL/SDA cells |
| --- | ---: | ---: |
| F002 QFN20/TSSOP20 | 36 | 29 |
| F003 QFN20/TSSOP20 | 36 | 29 |
| F003 TSSOP24 | 46 | 38 |
| L010 SOP16 | 30 | 26 |
| L010 QFN20/TSSOP20 | 38 | 31 |
| L011 QFN32/LQFP32 | 62 | 49 |
| L012 QFN48/LQFP48 | 125 | 99 |

Safe counts exclude F002/F003 PA2/PA5 SWD and PC5 reset, L010 PA7/PA8 SWD and
input-only PB7, and L011/L012 PA13/PA14 SWD. L012 PF3 is not treated as an
input-only pad; it simply has no serial route. Real oscillator-pad routes remain
factual metadata and require board/RCC compatibility before use. CTS/RTS/NSS
remain metadata unless a separately reviewed HAL API explicitly exposes them.

The 10-test independent metadata suite checks all 17 family/exact-package
projections, alias intersections, safe counts, singleton names, AF8/9 identity,
wrong direction/instance/selector, missing routes, invented SDK evidence and
unbonded pads. It also revalidates the old 031 radio/SWD restrictions and retains
negative AF8/9 fixtures for 031. Actual constructor and typed-pin compile tests
belong to the driver integration; this metadata review does not claim that a
route alone enables a working backend.

## Verification run on 2026-10-08

After `./d gen-all` completed, these checks passed:

- `cargo test --locked -p cw32-data-gen`: 32 unit and 7 integration tests,
  including all-reviewed-metadata generation without register-source changes.
- Source-mode verification of the new 340 cells and existing 675 serial cells
  against their own official PDFs and SDK headers.
- New serial metadata contracts: 10 tests covering all 17 affected chip profiles.
- Existing serial metadata contracts: 7 tests; physical pinouts: 12 tests;
  general metadata contracts: 19 tests.
- Independent generated/SVD parity for all 13 families and the data validator.
- PAC inventory: all 54 chip features, 1,845 peripheral instances, 1,513 interrupt
  entries and 135 register templates resolve.
- PAC access positive controls and all 27 forbidden-access compiler checks.
- Exact `cw32l012c8u6` PAC metadata: both Rust reference-resolution tests.

These are source, generator, host and compiler checks. No MMIO was executed on
silicon, and no serial electrical or throughput behavior was tested here.
