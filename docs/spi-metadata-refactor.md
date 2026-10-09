# Source-owned SPI limits and direct peripheral operations

The 2026-10-08 refactor changes where existing verified SPI facts are stored and
how the HAL consumes them. It does not change the qualified clock envelope,
conservative electrical choices, register encodings, or supported public SPI API.

## Data path

`cw32-data/spi.yaml` explicitly lists each family's peripheral instances and their
maximum master frequency and minimum divisor. Each profile points to its own
family entry in `docs/spi-clock-source-policy.json`, whose pinned datasheet/manual
sources and extracted-page hashes remain unchanged. The generator rejects a
missing SPI instance, an extra/non-SPI instance, a cross-family source pointer,
or limits that disagree with the reviewed source policy. Register aliases do not
supply electrical limits.

The data generator projects `Peripheral.spi`; the metadata generator preserves
it as `PeripheralSpi`. The HAL build script passes these values to the existing
instance macro, implementing sealed `Instance::MAX_FREQUENCY` and
`Instance::MIN_DIVISOR`. The existing type-erased `Info` stores those constants
for reconfiguration. There is no family-frequency table in the HAL or build script.

The pinned Embassy SPI driver uses ownership, per-instance metadata and generated
peripheral implementations. Its build script also generates sealed RCC instance
implementations from metadata (`embassy-stm32/build.rs`, `RCC_INFO` construction).
That architecture informs this refactor; the pinned SPI driver does not itself
provide this CW32 electrical-limit schema. No upstream schema implementation body
was copied for this extension.

## Driver structure and unchanged behavior

- `Spi` directly owns the word-width state and executes transfer loops through
  the PAC. The private `Registers` trait and generic `Engine` were removed.
- `Info` owns configuration, source-limit validation, peripheral enable and clear
  operations. `Config` owns its SCK pull choice; `Prescaler` owns real divider
  encodings. There are no module-level helper functions in `spi.rs`.
- Existing RCC metadata generates typed gate/reset accessors in sealed instance
  implementations. The old family/instance-number bit-mask lookup is gone.
- Low-power clock gates still write KEY=0x5a5a and preserve every other low-half
  field. Resets remain unkeyed active-low; clock-enable polling and the 100,000
  iteration limit are unchanged.
- Actual CR1/CR2 enable placement, CR3 initialization, SMP semantics, GPIO pulls,
  maximum encoded divider and L012 linear-even divider retain register-version
  conditionals.

For all 13 families, the old and new caps and minimum divisors are identical:
x030 16 MHz; F002/F003/F020 and L031-derived families 12 MHz;
L010/L011/L012 24 MHz. L031-derived minimum divisor is 4; all others use 2.
The ordered eligible-divisor sequence is unchanged. The nominal request-range
multiplication, exact `ClockBounds::divided_by` propagation and both
`maximum_exceeds` ceiling checks are unchanged. Moving the starting-divider
restriction to a minimum-divisor comparison therefore produces the same first
successful divider and the same error outcomes for every clock/request pair.

The machine-readable companion records each family's old/new limits and every
RCC gate/reset field's equivalence to the previous literal bit masks. It also
records that stripping only the added SPI metadata from all 54 generated chip
selections restores their previous JSON exactly. All register IR and generated
PAC peripheral files are byte-identical.

## Schema extension review

`chip::core::Peripheral` adds `spi: Option<peripheral::Spi>`, with an explicit
Serde default and omission when `None`. Historical JSON without this field still
decodes; serializing `None` emits no new field. Present values preserve u32
maximum-frequency and u16 minimum-divisor widths; both fields are required.
This is an additive wire-format extension, not complete Rust source compatibility:
external Rust struct literals for `Peripheral` must add `spi: None` or a value.
Generated PAC `Peripheral` struct literals likewise acquire the field.

The new schema record and field were authored within the independently rewritten
schema. An independent read-only review checked this narrow extension, its
metadata transport, all 13 max/min pairs, and the divider scan equivalence; it
found no substantive issue. That review does not re-audit original datasheet
claims or resolve historical upstream licensing. The original replacement hashes
and technical review remain recorded; the extension's before/after hashes are in
`spi-metadata-refactor.json` and the provenance inventory.

## Verification

Passed on the isolated source tree:

- Existing schema, macro and generator checks, with optional-SPI wire-format
  coverage added to the schema suite
- Generated-data validation, all-profile metadata contracts and independent
  register/SVD parity checks
- 54 chip selections and 86 SPI metadata instances against the authored facts
- All 26 pinned own-family SPI datasheet/manual PDF identities and exact page
  extraction hashes
- Normal `thumbv6m-none-eabi` library builds for all 13 families with `rt,defmt`
- Linked the existing F030 blocking GPIO/UART/SPI example in release mode

An initial build matrix ran out of disk space; only this task's regenerable build
artifacts were removed. The complete matrix then passed with debug information
disabled for library builds. No HAL tests, mock register engine, compile-fail HAL
harness, firmware execution, or hardware validation was added or performed.
