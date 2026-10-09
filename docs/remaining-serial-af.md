# L031/R031/W031/L052/L083 serial pin routing

These five reviewed sidecars are connected through `af_metadata` in each input
manifest. They cover **675 UART/SPI/I2C pin/selector/function cells** from the
individual official datasheets, including UART flow-control and SPI chip-select
data. They do not promote other functions from the SDK candidate sidecars.

| Family | Reviewed serial cells | SDK-only cells excluded | Datasheet AF tables | Printed / PDF pages |
| --- | ---: | ---: | --- | --- |
| CW32L031 | 108 | 0 | 5-3 to 5-6 | 28-29 / 29-30 |
| CW32R031 | 86 | 21 | 5-3 to 5-6 | 31-32 / 32-33 |
| CW32W031 | 90 | 18 | 5-3 to 5-6 | 30-31 / 31-32 |
| CW32L052 | 158 | 0 | 5-3 to 5-7 | 30-33 / 31-34 |
| CW32L083 | 233 | 0 | 5-3 to 5-8 | 33-37 / 34-38 |

## Authority and reproducible evidence

The official family datasheet is the hardware authority. An SDK macro supplies
corroboration and the field assignment, but its absence does not invalidate a
clearly documented route. `remaining-serial-af-evidence.json` records the PDF
SHA-256, page, table, pin, selector and exact function bounding box for every
cell. It also pins the SDK header/candidate JSON and physical-pinout JSON hashes,
records all corrections/exclusions, and joins every exact package independently.

The source audit compares two independent extraction paths: physical coordinates
from the PDF and character columns from Poppler `pdftotext -layout`. Both retain
blank columns. All 15 datasheet AF pages were rendered and inspected. The three
reference-manual pages resolving SDK defects were also rendered and inspected.
A changed source hash, table layout or unmatched cell fails the audit rather
than falling back to sibling-family routing.

Official datasheets:

- [CW32L031 CN V1.9](https://www.whxy.com/uploads/files/20251229/CW32L031_DataSheet_CN_V1.9.pdf)
- [CW32R031 CN V1.2](https://www.whxy.com/uploads/files/20251230/CW32R031_DataSheet_CN_V1.2.pdf)
- [CW32W031 CN V1.3](https://www.whxy.com/uploads/files/20251230/CW32W031_DataSheet_CN_V1.3.pdf)
- [CW32L052 CN V1.3](https://www.whxy.com/uploads/files/20251229/CW32L052_DataSheet_CN_V1.3.pdf)
- [CW32L083 CN V1.9](https://www.whxy.com/uploads/files/20251229/CW32L083_DataSheet_CN_V1.9.pdf)

## Explicit SDK defects and family differences

1. **L031/R031/W031 PA10 AF6 is UART3 TX.** Each SDK calls its macro
   `PA10_AFx_SUART3TXD`. Each family datasheet Table 5-3 and its own reference
   manual Table 9-2 agree on `UART3_TXD`. The correction is limited to these
   three exact coordinates; the original macro and spelling remain recorded.
2. **R031 PA15 AF4 is UART2 RX.** SDK V1.1 says `UART2TXD`, but both the
   R031 datasheet and [R031 manual CN V1.3](https://www.whxy.com/uploads/files/20240920/CW32R031_UserManual_CN_V1.3.pdf),
   Table 9-2, printed p141/PDF p142, say `UART2_RXD`. The reviewed route is RX,
   with the SDK conflict preserved. It is not generalized to L052/L083: their
   PA15 AF4 cell is UART2 TX. W031 PA15 is not a bonded external pad.
3. **W031 PA2 AF5 is UART3 RX.** It appears in the datasheet and
   [W031 manual CN V1.4](https://www.whxy.com/uploads/files/20240920/CW32W031_UserManual_CN_V1.4.pdf),
   Table 9-2, printed p141/PDF p142, but the SDK omits the macro. This route has
   `source_kind: datasheet` and null SDK evidence; no macro or line is invented.
4. The L031/R031/W031 documents call their sole serial instances `SPI` and
   `I2C`; the PAC/SDK names are `SPI1` and `I2C1`. Only those three families use
   this instance-name normalization. Signal normalization is `TXD` to `TX`,
   `RXD` to `RX`, and `CS` to `NSS`.
5. L083 has six UART instances and different cells from L052 despite shared
   register layouts. For example PA2 AF2 is UART6 TX on L083 and UART2 TX on
   L052. L031 I2C1 SCL on PB6 uses AF3; L052/L083 use AF4.

The L031 PA10 correction is additionally backed by
[L031 manual CN V1.6](https://www.whxy.com/uploads/files/20240920/CW32L031_UserManual_CN_V1.6.pdf),
Table 9-2, printed p139/PDF p140. The exact manual hashes and function boxes are
included alongside each correction.

## Package and safe-HAL boundaries

Generated PAC metadata includes only the selected package's bonded pins, or the
common package intersection for a family alias. Debug pad routes remain factual
metadata, while the HAL filters PA13/SWDIO and PA14/SWCLK. Input-only PF3 has no
serial route and is not exported as a safe HAL pad. Oscillator pins can be used
for serial functions only when the external circuit and RCC configuration allow
GPIO use.

Radio-internal pins never become external serial routes:

- R031 PA0-PA3 are the internal RF software-SPI connection, datasheet section
  4.4.5/Table 4-4. The SDK's extra PB8/PB9/PF6/PF7 routes are also absent from
  the R031 AF/package tables and are excluded.
- W031 PB3/PB4/PB5/PB6/PB13 form its internal RF interface, section 4.4.4/Table
  4-2. Those pads and unbonded PA15 are excluded even though SDK macros exist.
  That section also says external SPI is unavailable during RF operation. The
  external SPI routes do not authorize simultaneous SPI/RF use, and no RF driver
  is implemented here.

Exact-package route counts after both package and safe-pad filtering:

| Part | All reviewed safe serial cells | TX/RX, SCK/MISO/MOSI, SCL/SDA cells |
| --- | ---: | ---: |
| L031F8P6 | 34 | 25 |
| L031F8U6 | 36 | 25 |
| L031K8V6, L031K8U6 | 69 | 48 |
| L031C8U6, L031C8T6 | 101 | 71 |
| R031C8U6 | 79 | 57 |
| W031R8U6 | 83 | 60 |
| L052C8T6 | 112 | 79 |
| L052R8S6, L052R8T6 | 149 | 111 |
| L083RBT6, L083RCS6, L083RCT6 | 149 | 111 |
| L083MCT6 | 183 | 138 |
| L083VCT6 | 216 | 164 |

CTS/RTS/NSS are retained as reviewed metadata; typed support depends on the
corresponding HAL driver. No blank AF cell is inferred. **AF0 is GPIO; only AF1-7
are admitted. Codes 8-15 are unsupported** even though AFR fields occupy four
bits. None of this constitutes silicon testing or a new DMA compatibility claim.

## Checks

```sh
python tests/verify_remaining_serial_af.py
python tests/verify_remaining_serial_af.py --sources /path/to/official-sources
python tests/test_remaining_serial_af.py
cargo test --locked -p cw32-data-gen af::tests
```

`--write --sources ...` rebuilds only the reviewed AF sidecars and source audit;
`./d gen-all` subsequently regenerates package-filtered metadata and PAC output.
The original SDK candidate JSON is preserved unchanged.

The contracts cover exact source coordinates, all generated family/package
projections, selector rejection, SDK omission without fabricated evidence,
family-specific instance/direction/selector changes, radio/unbonded pads,
incorrect TX/RX, missing routes and altered SDK macro/line evidence.
