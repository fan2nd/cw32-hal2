# Reviewed CW32 DMA metadata

These factual sidecars cover all 13 current family profiles. They are inputs for
chip metadata, not a HAL implementation. There are 37 physical DMA channels and
371 hardware request selections across the nine DMA-equipped profiles. F002,
F003, L010 and L011 have no DMA controller in their reviewed datasheet memory
maps, CMSIS headers and independently imported peripheral inventories.

| Profile | Channels | Hardware requests | NVIC sharing |
| --- | ---: | ---: | --- |
| A030 | 5 | 43 | 1 / 2+3 / 4+5 |
| F020 | 2 | 41 | 1 / 2 |
| F030 | 5 | 43 | 1 / 2+3 / 4+5 |
| L012 | 4 | 64 | 1+2 / 3+4 |
| L031 | 4 | 30 | 1 / 2+3 / 4 |
| L052 | 4 | 39 | 1 / 2+3 / 4 |
| L083 | 5 | 51 | 1 / 2+3 / 4+5 |
| R031 | 4 | 30 | 1 / 2+3 / 4 |
| W031 | 4 | 30 | 1 / 2+3 / 4 |

## Upstream-compatible projections

Each `cw32*.json` file contains:

- `core_dma_channels`: exact `Core.dma_channels` entries. `DMA_CH1` has
  `dma: "DMA"` and the upstream zero-based `channel: 0`; the one-based hardware
  ordinal and `DMACHANNEL1` register instance are retained in `channel_details`.
- `peripheral_dma_channels`: map from the existing generated peripheral name to
  exact `Peripheral.dma_channels` entries. Every route has a signal, controller
  and unshifted request number. No new serde fields are required.
- `channel_details`: physical ordinal, register instance, exact CMSIS/generated
  interrupt spelling and number, and evidence references.
- `requests`: the reviewed projection plus all allowed channel names, original
  SDK symbol/line/raw encoding, and the matching manual binary value/page/row.
- `sources`, `evidence`, `excluded_sdk_requests`, `source_discrepancies`,
  `unknowns`: factual provenance and explicit limits, kept outside chip serde.

The channel-local `TRIG.HARDSRC[7:2]` selects the request. These controllers have
no separate DMAMUX. Each family's manual section 8.8.4 gives one shared request
table for all physical channels, explicitly `y = 1~N`. This proves that a
`dma: "DMA"` route without a fixed `channel` is appropriate. It does not prove
that assigning the same request to several channels simultaneously is useful or
safe; peripheral enable configuration and DMA arbitration still apply.

## Important family differences

- F020 has exactly two channels. Its SDK `IS_DMA_ALL_PERIPH` and stale CMSIS
  aliases also accept channels 3–5. The manual's count and register range
  override those aliases. F020 legitimately has UART1–3, SPI1–2, BTIM1–3 and
  GTIM1–4 request sources; it has no ATIM request IDs 17/18.
- L052's SDK copied twelve request defines for GTIM4 and UART4–6, IDs 37–48.
  Those sources are absent from its manual HARDSRC table and its peripheral
  inventory. All twelve are recorded under `excluded_sdk_requests`, not emitted.
- L012 has a distinct 64-entry request table. ID 10 is SPI3 RX, unlike the ADC
  request on the older profiles. ADC1 SEQ is 12; ATIM CH1 is 50, unlike ADC SINGLE
  on L031/L052/L083/R031/W031. Values must not be borrowed across families.
- L012's SDK `LPI2C1/2` names correspond to the manual/generated `I2C1/2`.
  Manual `DAC1/2` denote channels of the single generated `DAC` peripheral.
- A030/F030 use the manual explicitly labeled CW32F030/CW32A030. A030's F030 SDK
  corroboration is shared-source evidence, not an independent A030 SDK.

## Signal normalization

`TX`/`RX` identify UART, SPI and I2C buffer events. Timer signals use `UP`, `TRIG`
and `CH1`…`CH6`. The older ATIM's two hardware selections are preserved as
`CH1A2A3A4_UP` and `CH1B2B3B_UP`; they are **not** split into invented independent
per-channel request sources. L012 has independent ATIM UP/CH1…CH6/COM/TRIG
selections. ADC uses `COMPLETE` where the older manual only says conversion
complete, and `SEQ`/`SINGLE` where it explicitly distinguishes them. DAC uses
`CH1_DHR_UNDERRUN`/`CH2_DHR_UNDERRUN`; LCD uses `FRAME`; CORDIC uses `IDLE`/`EOC`.

L012 request 18 is labeled only `HALLTIM` in the request table. `EVENT` preserves
that generic source instead of inventing a narrower event. This detail is the
only recorded semantic unknown; every documented HARDSRC selection is covered.
These strings are data labels accepted by upstream serde; they do not promise
compatibility with a future driver's trait naming.

## Provenance and verification

Each source has a SHA-256 and official vendor document/archive URL. Header paths
are relative to an external vendor source cache. Manual evidence records both
zero-based PDF page indices and printed page labels; notably L012 printed page
121 is PDF page 147 (zero-based index 146). Manual table values and SDK enum or
macro values are independently compared. Request values are always unshifted
six-bit HARDSRC values; most older SDK macros are already shifted left by two.

The L012 two-column request table and the L052 table were additionally rendered
and visually reviewed. All emitted channel, controller, peripheral and IRQ
references, HARDSRC fields, and global TC/TE status/clear fields are checked
against generated PAC data for every generated chip. There has been no silicon
validation. Completeness means the cited manual table is completely represented,
not that undocumented behavior, electrical operation, or a DMA driver is tested.

Re-extract from the original downloaded vendor source tree (the script never
fetches or modifies sources):

```sh
python cw32-data/dma/build_verified.py --source-root ../cw32-sources --check
```

Omit `--check` only when deliberately updating reviewed facts. The script pins
reviewed table page selections, channel counts and request sets, rejects new
unreviewed symbols, validates normalized source identities, and compares SDK
values with the manuals. It does not infer request values from register aliases.

Run independent sidecar and generated-reference contracts:

```sh
python -m unittest discover -s tests -p test_dma_metadata.py -v
```

The source re-extraction test is skipped when the vendor cache is unavailable;
that skip must not be reported as a successful source audit. All other checks
work from repository data. `validate_dma_projection(profile, core)` is also
provided for the main metadata-import contract tests once these projections are
consumed by the importer. No source PDFs, SDK code, or generated HAL are vendored
by this change.
