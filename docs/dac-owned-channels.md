# Independently owned DAC outputs

`Dac::into_owned_channels` consumes an initialized direct-loading L012 DAC and
returns optional `OwnedDacChannel` values for the enabled outputs. A single-output
constructor returns one owner and one None. The operation moves pins without
MMIO; the original whole-DAC destructor is suppressed so outputs keep their
current values. Both channel types carry the original peripheral lifetime.

Each channel can independently set/read its existing dedicated data registers.
Dropping one disables only its own CxOUT and ENx bits, then releases its pin.
Shared CR0/CR1 read-modify-writes use a critical section to preserve concurrent
sibling teardown. Gates, BGR and DMA/IRQ controls remain unchanged; the ADC2
quiet-peer invariant is preserved. Forgotten channels remain enabled and retain
no caller-memory pointer. The other channel is still usable/droppable.

These owners cannot select software/hardware trigger modes, submit dual-channel
loads, re-enable a dropped channel or reconstruct the peripheral. They have no
public constructor and cannot outlive a borrowed peripheral/pin. Existing
borrowed `split` remains available when the whole owner should be retained.

The [DAC source contract](dac-opa.md) and dedicated channel registers already
qualify these operations. No analog-startup, settling, accuracy or new clock
claim is added. The new example drops channel 1 and continues channel 2; no
local compilation or tests have been run. Lifetime, interleaved Drop and ADC2
coexistence acceptance remain pending.
