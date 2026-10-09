# Independent RCC control metadata

The new optional `Peripheral.rcc_control` transports only verified hardware
control facts already present in the thirteen authored clock sidecars. It is
separate from the original complete `Peripheral.rcc` clock identity. UART and
CW32L012 I2C controls therefore reach the generated PAC without treating their
peripheral-local selectors as SYSCTRL fields or substituting the bus clock for an
unknown kernel clock.

## Contract and ownership

The typed record carries the controller, verified bus clock, one-bit enable and
optional reset references, boolean active levels, an optional unshifted write
key, and optional enable/reset sharing group names. Register references are
validated against curated IR and sanitized with the same chiptool operation as
the generated PAC. Registers must be both readable and writable; key values must
fit their field and accompany their enable in the same register.

A shared group describes hardware identity. It does not authorize resetting or
disabling a shared clock. HAL acknowledgement limits, dummy reads, ownership and
preserve policies remain in the central HAL implementation. No driver sequence,
clock frequency or electrical limit is introduced by this transport change.

The canonical evidence-source lock must contain every source's exact URL,
artifact path and SHA-256. The verification manifest records actual byte-hash
checks against all 153 distinct already-acquired external source artifacts.
There are no new vendor facts or source pins in this change.

## Compatibility and provenance

The original Rcc type and serialized fields are unchanged. Missing/null
rcc_control deserializes to None and None omits during serialization. Older JSON
therefore round-trips without additions. Adding a public field to Peripheral is
source-breaking for downstream Rust struct literals.

The new records, generator projection, validation and tests were authored for
this CW32 contract. No older upstream schema or macro implementation bodies were
copied. The existing independently reviewed schema replacement and historical
ancestry remain recorded in docs/upstream-file-provenance.json. This extension
has its own before/after hashes in docs/rcc-control-metadata.json; the parent
integration must record independent review and final combined hashes after other
concurrent typed metadata additions. This worker does not self-certify an
independent review or replace an earlier review hash with an unreviewed one.

## Verification

- All 13 profiles generated all 54 chips with 1,653 control records.
- 473 chip control records retain an absent complete Rcc identity.
- Every other peripheral JSON property is unchanged.
- Authored/generated register IR and PAC register/peripheral files are byte-identical.
- All 153 referenced source artifacts match the existing locked hashes.
- Schema, data-generator and metapac-generator suites pass, including missing
  kernel/local selector, asserted-low reset, key, sharing and source-lock checks.
- Independent Python checks compare every generated control against its authored
  record and ensure no partial identity becomes a complete kernel clock.
- Normal generated PAC metadata tests and Cortex-M0+ compile checks are used;
  there is no HAL test harness or firmware execution.

The machine-readable manifest includes every chip's control count and all source
identities. Runtime validation and the final combined HAL feature matrix belong
to the central RCC integration.
