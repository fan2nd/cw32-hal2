# Upstream divergence ledger

Baseline stm32-data revision: 37a22f31552ba1fd29b3ef192c4578b84abee6e1.
Baseline chiptool revision: be1bff3e9e1b27b090e69bd9ac753c66fdcce678.

- Mechanical crate/feature/environment-prefix renames STM32 -> CW32.
- `cw32-data-gen` replaces STM32-specific Cube database ingestion with a pinned
  official CW32 SVD source and a small reviewed input manifest. It calls the
  upstream SVD converter rather than substituting a Python Rust-code emitter.
- `cw32-data-serde::Chip.device_id` is optional: absence represents unknown,
  rather than fabricating a device identifier.
- CW32 DMA/clock/pin routing is independently curated from official CW32
  sources rather than a nonexistent Cube database. Unqualified fields stay absent.
- NVIC priority and F6 memory errata are explicit in the import configuration.
- `cw32-metapac-gen/src/lib.rs` and `data.rs` retain upstream generation/static
  metadata machinery. Binary defaults target `cw32-data/data` and `cw32-metapac`;
  the whole output directory is not unconditionally deleted.
- Generated crate documentation/package metadata describe only this import,
  rather than claiming upstream Embassy publication, version, or STM32 coverage.
- Production render still uses upstream `chiptool::generate::render`, shared
  `Reg<T, Access>`, typed fields, InterruptNumber and cortex-m-rt vectors.
- Generator source licenses are preserved. Vendor blobs with unclear license
  are external, hash-verified build inputs, not bundled source assets.
- Metadata register block references are sanitized through the same chiptool
  pass as the emitted block definitions, so uppercase vendor names resolve.
- Metadata-only register modules get an `_regs` suffix. F003's IR peripheral
  otherwise collides with the upstream static `metadata::ir` schema module.
- Whole-IR exact-equality deduplication maps 252 initial source versions to 135
  canonical curated YAML versions. Per-profile register_versions maps and
  register-reuse.yaml document every shared version; import candidates fail
  if any source assigned to one canonical version differs. Merely similar
  layouts are never merged.
- Data import trims vendor IRQ-name whitespace, supplements header-sourced
  missing IRQs and verifies expected old access before documented RO fixes.
- Post-import validation checks full byte spans, not only equal start offsets;
  any intentional alias must be enumerated in that source configuration.
- Generated host tests and compile-negative access fixtures validate metadata
  references, GPIO fields, CRC access widths and read/write type restrictions.

- Register YAML under `cw32-data/registers/` is now authoritative, as upstream.
  Normal generation loads authored YAML and validates provenance assertions;
  it does not replay register patches or rewrite authored source. The explicit
  import-registers command writes review candidates outside authored data.
- The generated static Metadata schema keeps the upstream shape and adds
  explicitly reviewed CW32 peripheral capabilities, electrical bounds and
  ownership/routing facts. Package identity, memory and qualified physical
  package-pin maps are projected into chip JSON. Extension history is recorded
  in the per-file provenance and evidence catalogs.
- Direct import output guards canonicalize relative, dot-dot and symlink paths;
  boundary tests verify authored source cannot be overwritten by candidates.
- RTC.COMPEN.FREQ in F030/A030 was removed from authored YAML because bits31:16
  are reserved in both reviewed common manuals. The candidate-import correction
  verifies the obsolete source field occupied only bits19:16 before removing it.

- F020 GPIO/GPIOC/GPIOF ISR and IDR are read-only in CW32F020 User Manual
  CN V1.4 sections9.6.14/9.6.18 (pp153/154). Six source-specific overrides
  correct the imported ReadWrite declarations. After these corrections the
  complete IR equals existing v1, so only those three F020 templates are reused.
  SYSCTRL, DMA, ADC and GTIM remain independently versioned.

- F020 CRC differs from the copied SDK/SVD claim: the current datasheet Rev1.3
  section4.4 p9 and manual Rev1.4 section10.6.3 p161 define only eight CRC16
  modes and reserve result bits31:16. A dedicated `crc_cw32f020_v1` keeps all
  DR8/16/32 input access widths and the 32-bit result bus-access alias, but narrows
  that alias's valid result field to 16 bits. An evidence-bearing manifest
  field-width override checks the original 32-bit field before candidate import;
  normal generation checks the curated correction without rewriting it.
  CR.MODE keeps its documented 4-bit layout; only encodings 0–7 are supported.
  This separates F020 from shared CRC v1 and raises canonical template count 134→135.
  See `docs/f020-serial-compatibility.md` and independent metadata-parity checks.

- Subsequent own-manual ISR access corrections for F020 and L083 GTIM make
  their complete corrected IR identical to existing `gtim_v1` and
  `gtim_cw32l031_v1`, respectively. Exact dedup reduces the canonical template
  count from 135 to 133 while retaining all 252 source-version identities.
  F002/F003 and L052 GTIM ISR, L012 ADC ISR, and L011 GPIO ISR also receive
  source-qualified RO corrections without changes to fields or register widths.
  See `docs/timer-adc-isr-access-corrections.md` and
  `docs/gpio-isr-access-corrections.md`. No HAL capability is inferred.

- Own-manual FLASH PAGELOCK reserved fields require two exact version splits: F002 from F003 and F020 from x030. This changes the current canonical count 133→135 while preserving all252 source-version identities; see `docs/flash-lock-access-corrections.md`.

- Peripheral pin records add optional `adc_mux: Option<u8>` in the JSON serde
  model and generated static metadata. R031 own RM CN V1.3 Table 22-5 (printed
  p439, PDF p440), own SDK `ADC_ExInputCH0` through `ADC_ExInputCH8`, and own
  datasheet Table 5-2 prove that source labels ADC_IN0–8 map to hardware mux
  values 4–12. For example, PA4 retains normalized signal `IN0` and carries
  `adc_mux: 4`; relabeling it `IN4` would erase the source identity. Other reviewed
  ADC families carry explicit mux values too. Digital pin records omit this
  optional JSON field and emit `None` in static metadata. Existing records
  deserialize with `None`; round-trip tests guard their unchanged serialized
  form. The HAL consumes the explicit mux rather than parsing a signal suffix.
  This does not change register layouts or import unrelated STM32 routing rules.
  Seven own-family analog sidecars preserve exact datasheet spellings separately
  (`ADC_AINn` on F002/F003/L052/L083, `ADC_INn` on L031/R031/W031), SDK macro
  encodings, manual cells, package positions and family-alias intersections.
  W031 SDK comments for channels8–12 contain the L052/L083 pin map; its own
  datasheet and manual independently agree on PB0/PB1/PB2/PB10/PB11 and take
  precedence over those five comments. See `tests/verify_classic_adc_routes.py`.

- All manually maintained hardware manifests, capability catalogs and route rules
  now use YAML. JSON remains the generated chip/IR format. Unlike the STM32
  Cube ingestion pipeline, CW32 requires explicit family SVD/source selection
  manifests. Vendor identities are maintained separately under `sources/`;
  downloaded originals and generated PAC/data/reports are ignored and excluded
  from source distributions. A separate root generator workspace builds before the generated PAC
  exists; the firmware workspace owns the HAL and PAC.
