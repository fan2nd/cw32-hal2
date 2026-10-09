# Authored CW32 clock facts

These per-family JSON files are reviewed input to `cw32-data-gen`, not generated
output and not a copy of STM32 clock topology. They cover every peripheral view
in each family profile. A source reference identifies the official artifact by
URL and SHA-256, plus a location and the specific claim it supports.

## Integration boundary

`status: supported` requires a verified bus clock, a representable kernel clock,
an enable field, and an established reset-field/absence decision. Only these
records become the unchanged upstream-shaped `Peripheral.rcc`. Partial records
retain established facts and explicit blockers; a missing kernel is never
silently replaced by the bus clock. The adapter resolves **all** field references
against the authoritative register IR, including references in partial records.
Repeated physical gates/resets require a named shared group and source evidence.

Bus-clock names follow the CW32 sources. For example, APBEN1 and APBEN2 are two
control registers and do not by themselves prove separate PCLK1 and PCLK2 clock
domains. Peripheral address ranges do not establish bus or kernel clocks.

## Facts the upstream Rcc shape cannot encode

- `rcc::Field` contains only a register and field, with no peripheral scope.
  Peripheral-local kernel muxes therefore remain scoped facts in these sidecars;
  they must not appear as if they were SYSCTRL muxes in generated metadata.
- `Rcc.reset` gives field identity, but has no assertion level. CW32 reset controls
  can be active-low. The sidecars preserve `reset_asserted_value`; a future CW32
  consumer must honor it instead of copying STM32's reset sequence.
- `Rcc.enable` cannot encode a write key. Where a family requires it,
  `enable_write_key` records the key field and **unshifted** field value (0x5A5A).
- The existing `StopMode` enum describes STM32 Stop1/Stop2/Standby policies, not the
  CW32 Sleep/DeepSleep hardware vocabulary. `Stop1` is explicitly the conservative
  build policy here, and not a verified low-power capability. No source claims
  that CW32 implements a mode named Stop1.
- One kernel field cannot describe multiple independently clocked master/slave
  engines or composite mux/divider behavior. Additional reviewed selectors may
  be retained under `related_fields`, with their own evidence and blockers.

The original `Rcc` shape remains unchanged. An optional sibling
`Peripheral.rcc_control` independently transports verified controller/bus identity,
readable and writable one-bit enable/reset fields, boolean active levels,
unshifted write keys and shared group identities. It is available even when the
kernel identity remains partial. No kernel clock is added to this record and no
bus frequency is substituted for an unresolved kernel. Absence means controls
have not been established. Sharing identities are hardware facts; lifetime,
acknowledgement and conservative preserve policies remain HAL decisions.

This optional JSON addition defaults to absent and is omitted when empty. Adding
a public Rust field is source-breaking for downstream `Peripheral` struct
literals, even though old JSON retains its original wire representation.

## Validation

`cw32-data-gen/src/clock.rs` rejects incomplete supported records, unknown field
references, non-single-bit gates/resets, unsupported local mux projection,
unacknowledged shared controls, missing evidence, mismatched family inventories,
and any attempt to label Stop1 as a verified CW32 hardware mode. Each source's
exact URL, artifact path and SHA-256 must match the canonical evidence-source
lock; this check does not require redistributing vendor artifacts.

`tests/test_clock_contracts.py` independently checks generated records against
these authored inputs and verifies that partial facts are absent from generated
Rcc metadata. Source artifacts remain external, consistent with vendor licensing.

`tests/test_rcc_control_metadata.py` checks the independent control projection for
all 54 chips while preserving all existing partial kernel statuses.
