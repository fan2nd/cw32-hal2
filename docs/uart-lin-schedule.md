# LIN static schedule budgets

`LinScheduleTiming`, `LinScheduleEntry` and `LinSchedulePlan` validate a borrowed
schedule table without accessing hardware, allocating memory or starting a
timer. They cover arithmetic admission only, not execution of the schedule.

The protocol basis is LIN Consortium Specification Package 2.2A,
[§2.3.2 pp32–33 and §2.4 pp39–40](https://community.nxp.com/pwmxy87654/attachments/pwmxy87654/16-bit/18786/3/LIN_Specification_Package_2.2A.pdf).
Nominal header length is 34 bits; nominal response length is ten times payload
length plus one checksum byte. Maximum frame time applies a 7/5 multiplier.
Slots are integer multiples of a common time base, strictly longer than jitter
plus maximum frame time. The network nominal baud is used, including the
specification's existing treatment of bit tolerances. These formulas are not
receiver rejection deadlines. Diagnostic and event-triggered schedule policies
are separate from this arithmetic.

For integer microsecond inputs, admission uses:

```text
bits = 34 + 10 * (payload_length + 1)
minimum_slot_us = jitter_us + floor(bits * 7 * 1_000_000 / (nominal_baud * 5)) + 1
slot_us = time_base_us * time_base_ticks
accept when slot_us >= minimum_slot_us
```

The final `+1` enforces strict inequality even when the rational frame duration
is an exact integer. It is not ordinary ceiling division. No intermediate
floating-point rounding or per-field duration rounding occurs. Payload length
comes from the already validated `LinFrameDefinition` (1..8); all products fit
u64 for accepted u32 timing inputs, and cycle accumulation uses checked addition.

Construction rejects zero baud, zero time base, empty tables, zero/short slots
and cycle overflow. The offending slot index is returned for short entries.
Repeated IDs are allowed because a frame can occupy multiple table positions.
The borrowed immutable entries retain their validated durations. `slots()`
returns definitions and allocated durations in order, and `cycle_us()` returns
the checked sum. These accessors do not advance clocks or transmit bytes.

Example application setup:

```rust,ignore
let short = LinFrameDefinition::new(LinIdentifier::new(0x12)?, 2, LinChecksum::Enhanced)?;
let long = LinFrameDefinition::new(LinIdentifier::new(0x13)?, 8, LinChecksum::Enhanced)?;
let timing = LinScheduleTiming::new(19_200, 5_000, 100)?;
let entries = [
    LinScheduleEntry { frame: short, time_base_ticks: 1 },
    LinScheduleEntry { frame: long, time_base_ticks: 2 },
];
let plan = LinSchedulePlan::new(timing, &entries)?;
```

Arithmetic expectations for that example are minimum durations 4,767 and
9,142 microseconds, with allocated durations 5,000 and 10,000 microseconds and
a 15,000-microsecond cycle. These are hand-reviewed expectations, not executed
test results. At nominal 20,000 baud, an eight-byte frame with 100 microseconds
jitter requires at least 8,781 integer microseconds: equality at 8,780 must fail.

The caller must qualify actual baud, oscillator tolerance, transceiver behavior,
header formation, response/inter-byte spaces, scheduler latency and timer drift.
Using `actual_baudrate()` as if it were the network nominal rate is not implied.
A validated table does not certify a configured baud as physically supported.
The planner does not switch tables, implement diagnostics, arbitrate publishers,
retry, catch up missed slots or establish a wake source. In particular, current
CW32 `send_idle` emits a full idle frame, so it cannot be assumed to implement
the nominal one-bit delimiter without checking the complete header budget.

No compilation, tests, dependency resolution or board execution was performed.
