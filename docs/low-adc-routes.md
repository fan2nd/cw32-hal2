# L010/L011 ADC analog-route qualification

Source review: 2026-10-08. This review qualifies external analog pin routes and
records the identities of the two internal sources. It does not establish ADC
timing, analog accuracy, electrical safety or silicon behavior. No hardware was
executed. L012 and every timer remain outside this route change.

## Own-family evidence

Each `cw32-data/af/cw32l01x-analog.json` pins its own datasheet, reference manual,
SDK archive, ADC header and existing physical-pinout sidecar by SHA-256. The
importer's `low_adc/profiles/mod.rs` independently locks those filenames and
hashes. It does not select sources by a guessed compatible-family name.

| Family | Pin/source-name authority | Mux authority | Own SDK authority |
| --- | --- | --- | --- |
| L010 | [DS CN V1.3](https://www.whxy.com/uploads/files/20260416/CW32L010_DataSheet_CN_V1.3.pdf), table 5-2, printed pp23–24 / PDF pp24–25 | [RM CN V1.2](https://www.whxy.com/uploads/files/20260626/CW32L010_UserManual_CN_V1.2.pdf), table 20-4, printed p506 / PDF p507 | [SDK V1.0.9](https://www.whxy.com/uploads/files/20260806/CW32L010_StandardPeripheralLib_V1.0.9.zip), `Libraries/inc/cw32l010_adc.h`, comments169–182, selectors186–199, TS/BGR200–201 |
| L011 | [DS CN V1.1](https://www.whxy.com/uploads/files/20250724/CW32L011_DataSheet_CN_V1.1.pdf), table 5-2, printed pp26–27 / PDF pp29–30 | [RM CN V1.1, current 2026-06-02 copy](https://www.whxy.com/uploads/files/20260602/CW32L011_UserManual_CN_V1.1.pdf), table 20-4, printed p508 / PDF p509 | [SDK V1.0.3](https://www.whxy.com/uploads/files/20251016/CW32L011_StandardPeripheralLib_V1.0.3.zip), `Libraries/inc/cw32l011_adc.h`, comments189–202, selectors206–219, TS/BGR220–221 |

The L011 SDK's comment header at line188 explicitly names two columns, `L010`
and `L011`. Only the **right-hand L011 column** qualifies L011 pins. The comment
column is saved as zero-based `sdk_pin_comment_column=1`; L010 uses0. Reading the
first match on each line would silently import the wrong map for channels7–13.
Both L011's own manual and its own datasheet independently confirm the right-hand
map. The source verifier checks the column header, number of matches and selected
column on every row.

L011's datasheet has **three** front-matter pages, but its reference manual has
**one**. The two independent offsets are retained in every evidence cell. L010
uses one front-matter page in both documents. All six cited PDF pages were
rendered and visually reviewed, as well as parsed directly.

## Explicit pin and package map

The logical signal `INn`, own-source name `ADC_INn`, and hardware selector `mux`
are independently stored fields. These two families happen to have matching
signal indices and mux values. They are not inferred from signal spelling, and
the ADC's one-to-eight sequence slot numbers do not become external pin numbers.
Analog routes have `af: null`; no digital AF selection is fabricated.

### CW32L010

| Signal / mux | Pin | SOP16 position | TSSOP20 position | QFN20 position | Datasheet PDF page |
| --- | --- | --- | --- | --- | --- |
| IN0 / 0 | PA0 | 4 | 5 | 2 | 24 |
| IN1 / 1 | PA1 | 5 | 6 | 3 | 24 |
| IN2 / 2 | PA2 | absent | 10 | 7 | 24 |
| IN3 / 3 | PA3 | 11 | 13 | 10 | 24 |
| IN4 / 4 | PA4 | 12 | 14 | 11 | 24 |
| IN5 / 5 | PA5 | 13 | 15 | 12 | 24 |
| IN6 / 6 | PA6 | 14 | 16 | 13 | 24 |
| IN7 / 7 | PB0 | 9 | 11 | 8 | 24 |
| IN8 / 8 | PB1 | 10 | 12 | 9 | 24 |
| IN9 / 9 | PB2 | absent | 19 | 16 | 25 |
| IN10 / 10 | PB3 | absent | 20 | 17 | 25 |
| IN11 / 11 | PB4 | absent | 1 | 18 | 24 |
| IN12 / 12 | PB5 | 1 | 2 | 19 | 24 |
| IN13 / 13 | PB6 | 2 | 3 | 20 | 24 |

The exact SOP16 `CW32L010Y8M6` projection has10 routes. `CW32L010F8P6`
(TSSOP20) and `CW32L010F8U6` (QFN20) each have14. The family alias has the
**common-package intersection of10**, never the largest-package superset.
PA7/PA8 retain SWD, and input-only PB7/NRST is not an analog route.
PA0/PA1 also name OSC_IN/OSC_OUT and PB0/PB1 name OSC32_OUT/OSC32_IN respectively;
those aliases are preserved as evidence. An ADC pin owner must not coexist with
an active oscillator using the same pad.

### CW32L011

| Signal / mux | Pin | LQFP32 and QFN32 position | Datasheet PDF page |
| --- | --- | --- | --- |
| IN0 / 0 | PA0 | 6 | 29 |
| IN1 / 1 | PA1 | 7 | 29 |
| IN2 / 2 | PA2 | 8 | 29 |
| IN3 / 3 | PA3 | 9 | 29 |
| IN4 / 4 | PA4 | 10 | 29 |
| IN5 / 5 | PA5 | 11 | 30 |
| IN6 / 6 | PA6 | 12 | 30 |
| IN7 / 7 | PA7 | 13 | 30 |
| IN8 / 8 | PB0 | 14 | 30 |
| IN9 / 9 | PB1 | 15 | 30 |
| IN10 / 10 | PA8 | 18 | 30 |
| IN11 / 11 | PA9 | 19 | 30 |
| IN12 / 12 | PA10 | 20 | 30 |
| IN13 / 13 | PA11 | 21 | 30 |

`CW32L011K8T6` (LQFP32), `CW32L011K8U6` (QFN32) and their family intersection
all have14 routes. PA13/PA14 remain SWD pins. The L010 PB2–PB6 assignments are
not valid L011 ADC routes.

## Internal sources are not pins

Both own manuals' table20-4 explicitly gives TS=1110 (14) and the internal1.2V
BGR source=1111 (15), with `-` in the GPIO column. Each own SDK independently
supplies `ADC_InputTs=0x0000000E` and `ADC_InputVref1P2=0x0000000F`.
`internal_sources` records these facts with exact macro line and manual-cell
provenance, but never projects them into GPIO pin metadata. There is no VDD/3
source on these converters. Internal-source timing and safe HAL ownership need
the separate driver review; these route records do not enable them by themselves.

## Importer and independent verification

The importer accepts only the two reviewed profiles and exact ADC register
versions. Before changing the core it checks source identities, source/selector
pairs, own-family SDK comment column, both PDF page offsets, package positions,
safe I/O classification and oscillator aliases. It requires exact physical
package pins or their common intersection and an empty ADC route list. Validation
failure leaves the core unchanged. Internal sources are validated separately,
then only the14 external-source candidates are package-filtered into `adc_mux`.

`tests/verify_low_adc_routes.py --sources /path/to/cw32-sources --sidecars-only`
rehashes the primary files and independently:

1. Parses each own-manual table20-4 through Poppler, including internal14/15.
2. Reads actual ADC names **inside the rightmost analog-function PDF cells**,
   rather than searching a row's digital functions or trusting curated metadata.
3. Reads physical pin/package coordinates from the same grid rows and compares
   the pre-existing pinout extraction, which also cross-checks Poppler text.
4. Parses each SDK selector as an exact hexadecimal macro value and checks its
   own-family pin-comment column.
5. Compares the extracted source identities, pages and package positions with
   every route in the sidecars.

Without `--sidecars-only`, the verifier also checks both input-manifest links and
all seven generated selections, exact10/14 package counts, alias intersection,
forbidden pin omissions, no AF selector, and no internal-source pin projection.

Generator unit tests cover all seven projections and negative mutations of pin,
logical/source signal, mux, AF, ADC instance, SDK macro/line/comment column,
source hash/identity, manual/pin provenance, package position, oscillator aliases,
route duplication/omission, internal source changes, family map interchange,
L011 front-matter offset, wrong register identity, invalid physical pin sets and
repeated projection. Generated/PAC validation after regeneration is separate.

## Integration boundary

Set `analog_metadata` in `cw32-data/inputs/cw32l010.yaml` and `cw32l011.json` to
`cw32-data/af/cw32l010-analog.yaml` and `cw32-data/af/cw32l011-analog.yaml`
respectively. Both already use `common-package-intersection`. Dispatch is in
`cw32-data-gen/src/af/mod.rs`; low-family import code and its source locks/tests
are isolated under `cw32-data-gen/src/af/low_adc/`.

Regenerate data/PAC through the normal checked generation path, then run:

```sh
cargo test --locked -p cw32-data-gen af::low_adc
python3 tests/verify_low_adc_routes.py --sources "$CW32_SOURCES"
```

This route change did not edit the HAL, physical pinouts, register templates,
whole input-hash manifests or generated outputs, and did not run generation.

Verification for this route change: all seven focused `af::low_adc` generator
unit tests passed, the source-enabled sidecar verifier passed for both families,
and the module-layout check passed. Full generated-output verification is a
separate post-generation gate.
