# HAL capability facts through normal metadata

This architecture-only change removes the last four authored-data reads from
`embassy-cw32/build.rs`. The family GPIO pad union, CORDIC rational domains,
AES/TRNG geometry and RAM parity status capability now follow the existing
`cw32-data` → `cw32-data-gen` → chip JSON → `cw32-metapac-gen` → static metadata
pipeline. No HAL API, register access, peripheral scope or hardware behavior is
changed. There is no HAL adapter, register wrapper, path attribute or new cfg.

The schema changes are additive optional peripheral records. Missing and null
fields deserialize to `None` and remain omitted from serialized legacy data.
The independently authored schema implementation and its historical review
chain remain intact. Metapac uses the existing owned-generator/static-slice
representations and custom `Peripheral` debug emission, following the pinned
`stm32-data` organization at `37a22f31552ba1fd29b3ef192c4578b84abee6e1`.

## Facts and source boundaries

- `gpio.output_mask` is the union of output-capable pads in every reviewed
  package for that family. Package bonding and safe singleton projection stay
  unchanged. `gpio.pull_down_mask` preserves the own-source restrictions:
  no L010/L011 pull-down and only L012 PF3. The source review is
  `gpio-shared-inventory.json`, with original manual/SDK citations in
  `gpio-shared-implementation.md`. Neither value is derived from, or replaces,
  the existing serviced interrupt mask or R1W0 no-op command mask.
- `cordic.domains` contains exact inclusive signed Q1.31 bounds, rounded inward
  from the unchanged rational catalog. `l012-math-evidence.json` now records
  the nine already qualified rational domains alongside its unchanged manual
  URL, printed Rev1.4, hash and printed/PDF page references. The projection
  checks this table review, and the source validator independently checks the
  decimal intervals and open/closed endpoints in the pinned official text.
  EAU semantics and its numeric domains are unchanged.
- `aes` and `trng` record only 32-bit register-word geometry. Data generation
  validates the original mode, word order, missing IRQ/DMA/abort/health-status
  guarantees, clock/reset qualification and word-register layout against the
  existing own-L083 evidence before emitting geometry. No byte-order,
  cryptographic correctness or entropy guarantee is added.
- `ram_parity.enable_status` records availability of the read-only EN status.
  Existing own-family register, interrupt, clock/reset absence and diagnostics
  checks remain, with explicit catalog/evidence agreement added. No software
  parity switch, injection, initialization or reset operation is introduced.

All original authored catalogs, pinout tables, register YAML/JSON, official
source locks, source URLs, printed revisions and source hashes are unchanged.
Raw official sources remain external and are not included in this change.
The derived provenance index is refreshed for changed local transformation and
evidence hashes. The unused existing `serde_json` build dependency is retained
to avoid unrelated Cargo and example-lock changes; the HAL build script no
longer reads or parses any external authored data.

## Verification

The immutable baseline is Stage21. Removing only the five new optional fields
recovers all 54 old chip JSON values and all 29 old static metadata Rust files
exactly. Two full generation passes produce identical 399-file data and
477-file PAC trees. All source/data/schema/macro/PAC checks in the verification
receipt pass; no HAL tests, mocks or harnesses were added or run.

Normal optimized ARM builds were run on both snapshots: all 13 legacy families
with `rt` and `rt,defmt`, plus 29 existing-example build commands producing
32 firmware ELFs per side. Coverage includes per-family GPIO/comparator and
passive RAM examples, F030 blocking/async GPIO, L012 CORDIC/EAU and L083 AES/TRNG.
The 128 runtime-generated files, 55 cfg outputs and 4,994 constant declarations
are byte-for-byte identical. All 224 allocated ELF sections match in address,
size, flags, alignment and file-backed bytes (102,024 bytes per side).
Nonallocated ELF metadata can differ and is reported separately. NOBITS
sections are compared by their layout; no initialized-memory or hardware
execution claim is made.

Exact file hashes, build commands and logs, generated outputs, ELF sections,
reproduction details and schema review are recorded in
`hal-metadata-facts-verification.json` and the local handoff receipt packet.
There was no flashing, publication or external upload.
