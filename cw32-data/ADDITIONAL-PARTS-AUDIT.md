# Additional CW32 ordering-code audit

Audit date: 2026-10-08. Source scope: the 13 downloaded current official family datasheets pinned by SHA-256 in `parts.json`. The current 37-part web catalog and `parts.json` were not changed.

## Result

The 13 MOQ tables explicitly document **40 full order codes**: the 37 catalog chip selections, two legacy renamed CW32F030 codes, and one shipment-packaging alias. They do **not** establish three additional distinct hardware variants. No other explicit full order code was found in those documents.

| Additional code | Classification | Existing PAC selection | Physical package | Main Flash / SRAM |
|---|---|---|---|---|
| CW32F030C8T6 | Legacy name retained in current ordering table | `cw32f030c8t7` | LQFP48 | 64 KiB / 8 KiB |
| CW32F030F6P6 | Legacy name retained in current ordering table | `cw32f030f6p7` | TSSOP20 | 32 KiB / 6 KiB |
| CW32F003F4P7TR | Tape-and-reel shipment alias | `cw32f003f4p7` | TSSOP20 | 20 KiB / 3 KiB |

All three mappings use main Flash base `0x00000000` and SRAM base `0x20000000`, as documented in the respective family address maps.

## Evidence and caveats

### CW32F030 legacy names

[CW32F030 datasheet Rev 1.9](https://www.whxy.com/uploads/files/20251229/CW32F030_DataSheet_CN_V1.9.pdf), SHA-256 `04ef91434320e5d05a3b6690fead0fedb31a7d9bad28c96b655a6e13a22e46b2`:

- Table 9-1, zero-based PDF page 75 (printed page 75), explicitly pairs C8T6 with C8T7 and F6P6 with F6P7 in the same MOQ rows.
- Table 10-1, PDF/printed page 76, says Rev 1.4, dated 2022-01-10, updated these names to C8T7 and F6P7 based on working-temperature range.
- Table 3-1, PDF/printed page 7, supplies the variant-specific memory and physical-package mapping. F6 has **6 KiB SRAM**, not the 8 KiB family maximum.
- Table 6-1, PDF/printed page 31, supplies the memory base addresses; its full-family maximum capacities must not override Table 3-1.
- Section 9, PDF/printed page 74, defines suffix 6 as -40 to +85 degrees Celsius. Sharing a PAC model with the replacement suffix-7 name does not confer its higher temperature rating on a legacy part.

The two names are documented legacy aliases, not proof of separate current products or present commercial orderability. The current ordering table retains them despite the explicit renaming history. No supplier or factory stock/acceptance check was performed.

### CW32F003 reel code

[CW32F003 datasheet Rev 1.9](https://www.whxy.com/uploads/files/20251226/CW32F003_DataSheet_CN_V1.9.pdf), SHA-256 `5fe15321b3963472c2629030767cf61a0ce36063815b54add74025b5b13b95dd`:

- Table 9-1, zero-based PDF/printed page 61, lists CW32F003F4P7TR in a separate Reel row: 4,000 pieces per reel and MOQ 4,000. The non-TR F4P7 is listed in a Tube row.
- The same page explicitly defines TR as tape-and-reel and P as TSSOP; F means 20 pins.
- Table 3-1, PDF/printed page 7, supplies F4 memory sizes and package choices. Table 6-1, PDF/printed page 26, gives Flash `0x00000000-0x00004FFF` and SRAM `0x20000000-0x20000BFF`.

TR changes shipment packaging only. It must not add a PAC feature or increase physical-chip coverage.

## All-family audit ledger

The machine-readable `additional-parts.json` contains all 13 source URLs, verified file hashes, explicit order-code lists, memory-source page indices, and ordering-table page indices. Table 9-1 pages inspected:

| Family | Zero-based PDF page | Printed page | Explicit codes |
|---|---:|---:|---:|
| CW32A030 | 62 | 62 | 1 |
| CW32F002 | 57 | 57 | 2 |
| CW32F003 | 61 | 61 | 4 |
| CW32F020 | 68 | 68 | 3 |
| CW32F030 | 75 | 75 | 7 |
| CW32L010 | 65 | 65 | 3 |
| CW32L011 | 70 | 68 | 2 |
| CW32L012 | 80 | 78 | 2 |
| CW32L031 | 77 | 77 | 6 |
| CW32L052 | 78 | 78 | 3 |
| CW32L083 | 85 | 85 | 5 |
| CW32R031 | 72 | 72 | 1 |
| CW32W031 | 70 | 70 | 1 |

F020 source handling: the cache-root file bearing the V1.3 filename is stale and has a different hash. This audit uses the `current-datasheets` copy with SHA-256 `1e330d800f10114c654b97cbd45b64d56a99c3cb39138d4ef20de3c12968dab0`, matching `parts.json`. Its ordering table is PDF page 68, not the stale copy's page 64.

## Integration contract

- Preserve the 37 catalog chip selections.
- `parts` contains the two full same-core-schema legacy records; `shipping_sku_aliases` separately contains the full shipment-alias record.
- Each record has `kind`, `current_status`, `alias_of`, `recommended_base_pac_feature`, `generate_chip_selection: false`, and `count_as_distinct_hardware: false`.
- The existing `feature` field points to the **base catalog feature** in this alias file. Consumers must not assume it equals the lowercase alias name or append these records as independent chip selections.
- A user-facing code resolver may accept the legacy/shipping names through `alias_of`, without generating duplicate hardware or register models.
- `catalog_source_url` is null because these exact additional names are absent from the current catalog snapshot. Datasheet source IDs carry their evidence.

All ordering tables were read as text, and the extra-code rows plus their memory/package, address-map, naming, and history evidence were visually checked. All 13 actual PDF hashes were checked against pinned sources. Code extraction from each full document matched the corresponding ordering-table codes, including the explicit TR suffix. Naming diagrams were not expanded into Cartesian products. No historical-product search, silicon testing, or current-availability claim is included.
