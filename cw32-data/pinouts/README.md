# Official physical package pin maps

These 13 family files cover every one of the 37 exact current parts in
`../parts.json`. They describe physical package positions, bonded GPIO pads,
power/ground, reset, debug, BOOT, oscillator, RF and regulator signal names.
They do not supply alternate-function numbers, electrical configuration, or
silicon-validation claims.

## Data contract

- `packages[].name` is the exact orderable part; `package` preserves the catalog
  package designation, including body-size distinctions.
- `packages[].pins` uses the upstream chip schema's
  `{ "position": "...", "signals": ["..."] }` entries. Numeric positions remain
  strings. Vendor-numbered exposed pad `"0"` is preserved.
- `table_rows` retains every reviewed source-table row, package-column values,
  original pin spelling/type, normalized signals, I/O structure, and zero-based
  PDF page index. An unbonded source-table dash is `null` here and is never
  emitted as a physical pin.
- GPIO spelling is normalized from `PA00` to `PA0`. Other labels retain the
  datasheet spelling, including `Vcore`, `VSSRF` and RF `GPIO10`/`GPIO11`.
- `gpio_pins` is the output-capable MCU GPIO set according to the family-specific
  pin-type table. `input_only_pins` is the input-only MCU GPIO set. `debug_pins`
  identifies the GPIO pads carrying SWD. These are capability facts, not an
  instruction to disable debug/reset or bypass safe HAL ownership.
- A selected exact chip's core pin names can be derived from GPIO-shaped signals
  in its physical package. Generic family or memory-size aliases still require
  an explicitly documented die-level set or package intersection.

## Reproduction and validation

`source` records the official URL and SHA-256 from `../parts.json`. Every table
page is identified, and rendered visual checks are recorded separately. The
extractor reads the PDF's ruled table using `pdfplumber`, independently extracts
all physical-position columns using Poppler `pdftotext`, and requires exact
agreement before producing any map. It also requires full contiguous lead
positions, the expressly documented additional pad set, unique positions,
known package columns and exact source hashes.

Run from the repository root (Python `pdfplumber` and Poppler are required):

```sh
python cw32-data/tools/extract_pinouts.py /path/to/official-pdfs --check
python -m unittest discover -s tests -p test_physical_pinouts.py -v
```

Without `--check`, the first command regenerates the factual JSON. A single
family can be selected with `--family CW32F030`. The source resolver also checks
`current-datasheets/` under the supplied directory, because an older F020 PDF
was distributed under the same filename; only the SHA-256-locked current bytes
are accepted. No network fetches occur.

The offline tests cover all 37 parts, every package position, source identity,
source-table conservation, aliases, capability classifications, body-size
variants and the exceptional cases below. Rebuilding from PDFs additionally
checks both independent extraction paths; offline shape checks alone are not
claimed to re-read a datasheet.

## Important family/package distinctions

- F020 and F030 QFN32 have 32 perimeter leads **plus VSS pad 0**. Renumbering the
  pad to 33, omitting it or rejecting position 0 loses source information.
- F030 Figure 5-4 (PDF index 21) mistakenly labels QFN32 lead 31 `PB07`, repeating
  lead 30. Table 5-2 (PDF index 27) identifies lead 31 as `PF03/BOOT`; the complete
  pin-definition table is followed and the conflict is explicitly recorded.
- F030 LQFP32 lacks `PB2` and `PB8`, which QFN32 exposes. F6P/TSSOP20 has `PA9`
  and `PA10`; F8V/QFN20 instead has `PA11` and `PA12`.
- L012 `PF3/BOOT` is explicitly **I/O**, with output functions listed in the
  current datasheet. Other reviewed families that expose PF3 classify it as
  input-only. The restriction must not be copied between families.
- L010 `PB7/NRST` stays input-only after reset is disabled. F002/F003 `PC5/NRST`
  is documented I/O. Their SWD pads also differ: L010 uses PA7/PA8, F002/F003
  use PA2/PA5, and F030 uses PA13/PA14.
- L031 QFN20 and TSSOP20 differ substantially; the two QFN32 body sizes share
  the one explicitly documented QFN32 column.
- L083 LQFP100 is **not** a GPIO superset of smaller packages. `PF4`, `PF5` and
  `PF7` are present in LQFP80/LQFP64 but absent in LQFP100. Its source table has
  103 rows for 100/80/64 physical leads because some rows are package-exclusive.
- R031/W031 RF and antenna/regulator pins remain distinct from MCU GPIO.
  W031 GPIO10/11 have separate input/output descriptions in the source cell;
  their original `I; O` type is retained alongside the normalized I/O type.
- Unnumbered thermal pads receive neither invented positions nor assumed
  ground labels. Only explicitly numbered positions are included.

## Scope of review

This snapshot contains 615 source-table rows and 1,482 exact-part physical-pin
entries, including the two numbered exposed-pad entries. It was checked against
current official datasheets and visually reviewed at the recorded pages. It is
not a replacement for the manufacturer's electrical specifications or board
layout guidance and has not been tested on silicon.

## Generated package and core metadata

Every active input manifest explicitly opts in with `pinout_metadata` and
`alias_pin_policy: "common-package-intersection"`. During generation, exact part
package names and memory are checked against the parts catalog; physical maps
are rechecked against every reviewed table row and capability set before they
are copied into `Chip.packages[].pins`.

`Core.pins` on an exact part contains every GPIO-shaped physical pin label,
including input-only GPIO pads. Capability restrictions are retained in these
sidecars and must be respected by HAL consumers. The pin list itself does not
claim that every pad supports output, debug removal, or a safe board function.

For a package-less family alias, `Core.pins` is the intersection of GPIO labels
across every reviewed family package. For a package-less size alias, the
intersection is restricted to packages with that exact `family_part`. This
keeps `CW32F030K8` from claiming QFN32-only PB2/PB8 and does not attach any
physical package to the alias. This conservative policy can omit pins available
on a user's particular part; selecting the exact part exposes its reviewed set.
No package fields are added to upstream static `Metadata`.
