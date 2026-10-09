# CW32L011 typed timer cascade and ADC trigger

CW32L011 and its two exact packages, CW32L011K8T6 and CW32L011K8U6,
now expose the bounded `Btim1Update`, `Btim2Cascade` and `Btim1Adc<P>`
owners already used by L010. Each route is qualified from the L011 manual,
SDK and CMSIS header. Layout sharing alone does not enable a family.

The implemented paths are exactly BTIM1 UPDATE through CR2.MMS=2 to
BTIM2.SMCR.TRGISRC=8, and the same source to ADC.TRIGGER.BTIM1TRGO bit 13.
BTIM2 uses SMS=3, rising-edge counting, no input filter, no reset source,
and no destination prescaling. The ADC uses an independent enable bit and
one finite, noncontinuous sequence containing only slot zero. Additional
BTIM instances, multi-slot hardware scans, GTIM/ATIM sources, fanout and
classic IRQ-based ADC triggers remain outside this implementation.

## Own-family evidence

The selected [L011 manual Rev 1.1](https://www.whxy.com/uploads/files/20260602/CW32L011_UserManual_CN_V1.1.pdf)
has a June 2026 cover and September 19, 2025 revision-history date, 564 PDF
pages and SHA-256 `b245887e9037caf267f739e25bd3c354c2579d23315fc06389e055f371159f4f`.
The earlier September 2025 cover PDF has different bytes and is not this
qualification's source. The [official SDK V1.0.3](https://www.whxy.com/uploads/files/20251016/CW32L011_StandardPeripheralLib_V1.0.3.zip)
agrees on the selected producer, destination, mode and bit masks.

One-based PDF pages and sections:

- 163–164, §11.3.1.1: enable latency and one-shot terminal overflow clearing EN.
- 167–168, §11.3.2.1–2: internal clock and external rising-edge counter modes.
- 181, §11.9.2: MMS=2 selects UPDATE; MMS=0 still forwards software UG.
- 182–183, §11.9.3: TRGISRC=8 selects BTIM1 TRGO; SMS=3 selects external
  counting. Self-feedback is forbidden. RSTISRC is separate and stays disabled.
- 511, §20.5.2: either a software or external start executes one sequence with
  CONT=0; EOC marks each result, EOS marks sequence end and START clears.
- 513, §20.6: external trigger sources can start ADC conversion.
- 518–519, §20.11.1–2: ENS=0 selects one slot; START=0 stops conversion and
  resets sequence position, without a documented abort/drain latency bound.
- 523, §20.11.7: bit 13 independently enables BTIM1 TRGO; visually checked.
- 525–526, §20.11.9–10: EOC/EOS status and W0C flag clearing.

Exact source identifiers, member hashes, runtime references and exclusions live
in `cw32-data/triggers/cw32l011.yaml`; `trigger-routing-l011-evidence.json`
records the manual extraction hashes and upstream API review. No vendor PDF,
SDK or extracted page text is redistributed.

All four L011 findings in the broader routing audit were assessed. Unresolved
UARTx source identity at BTIM code 5, the stale SDK AWT constant at code 0,
SDK omissions for ADC bits 19–24, and contradictory GTIM TI2–4 pad labels
remain excluded. None changes BTIM1 UPDATE, BTIM2 code 8, or ADC bit 13.
The omitted ADC macros are not treated as contradictory documentation of bit 13.

## Generated constraints and ownership

The source generator pins each family's authored sidecar separately and checks
its source event, exact destination, register identities, field access/width,
encodings and complete two-route set. L011 BTIM uses the reviewed
`cw32l010_v1` register layout while its ADC must be `cw32l011_v1`.
The HAL build script separately qualifies both family identities before
emitting destination-local `Btim2TriggerInput` and ADC `TriggerInput` types.
There is no universal `TriggerSource` enum and no interpretation of ADC bits
as a mutually exclusive EXTSEL selector.

Ownership and register ordering use the existing [bounded route contract](trigger-routing-l010.md).
The source, destination and ADC pin are static owned tokens. They remain
owned even if a route is forgotten. Source reconfiguration occurs only while
the destination is detached. Cascade stop disables BTIM2 before selecting
SMS=0, disconnects it, and then stops BTIM1. The shared BTIM gate stays enabled
and group reset is never asserted.

ADC arm detaches first, prepares the stopped source, clears stale ADC state,
enables the qualified bit and starts BTIM1 in one-shot mode. It never writes
ADC START=1. Completion requires source EN clear, EOC/EOS set and ADC START
clear. Disarm, timeout and drop detach before stopping the source or ADC;
disarm/timeout are terminal for that owner. The existing ADC shared bandgap,
central RCC gate policy, supply/acquisition validation and ClockBounds remain
in force. No mutable PAC or peripheral token is returned by these owners.
Public direct PAC accesses must not concurrently change their registers,
pins or clocks; their availability is not claimed to be safe ownership.

## Verification limits

The source replay checks each family's own pinned PDF and headers, then verifies
14 bindings across seven exact/alias chip selections. Normal ARM HAL checks
and actual `cascade`/`triggered_conversion` ELF links cover the selected L011
packages with an L010 regression check. No HAL tests, mocks or model harnesses
are added. These programs have not been flashed or run on silicon.

Timer intervals are nominal PCLK divisors; ADC ClockBounds exclude trigger and
startup latency. `wait(budget)` bounds polling iterations, not elapsed time.
There is no promise of atomic cancellation, measured conversion latency,
maximum lossless trigger rate or periodic lossless acquisition.
