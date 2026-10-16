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

## Absolute-time execution cursor

`plan.cursor(start_us)` creates a `LinScheduleCursor` over the same immutable
entries. It consumes application-provided microsecond timestamps, with no timer
or UART dependency. `start_us` is a caller-selected time-base tick. Call
`start_slot(now_us)` at wakeup: it returns None before the scheduled start,
admits one slot within the declared jitter window, and rejects a later start.
Admission returns the frame definition, table index and absolute start/end.
It marks that slot active before returning; a second start is rejected.

The application executes its qualified header/response operation and then calls
`finish_slot(now_us)` only after the complete frame and required bus handling.
Completion by the slot boundary advances to the next entry, wrapping at table
end. The next deadline is the old slot end, not completion time; early completion
does not compress the table and late completion cannot shift it forward.
Unknown or delayed response handling must not silently be reported as success.

Start lateness, completion beyond slot end, nonmonotonic observations, timestamp
overflow and explicit `abort` latch the first fault. Further operations return
that fault. Calling start while active, or finish without an active slot, is an
operation error that preserves state. Transmission error/cancellation requires
`abort`; it does not retire hardware. There is no automatic skip, retry or burst
of missed slots. After application-controlled bus recovery, `restart(start_us)`
explicitly discards old cursor state and starts at entry zero. It may also
establish a new application clock epoch; it performs no physical recovery.

Hand-reviewed transition examples for the preceding 5 ms / 10 ms plan:

| Operation | Expected state |
| --- | --- |
| Cursor at 1,000; start at 999 | None, entry zero remains due at 1,000 |
| Start at 1,050 with 100 us jitter | Entry zero active, end 6,000 |
| Another start before finish | SlotActive, original slot remains active |
| Finish at 5,500 | Entry one due at 6,000, not 5,500 |
| Start entry one at 6,000; finish at 16,000 | Entry zero due at 16,000 |
| Start at deadline plus 101 | StartTooLate latched |
| Finish beyond admitted end | FinishTooLate latched |
| Observe a timestamp older than the preceding observation | ClockWentBackwards latched |

The first poll can precede the initial epoch; subsequent observations must be
nondecreasing. Zero jitter allows admission only at the exact scheduled start.
End-time addition is checked before activation, preventing wrapped deadlines.
These are source-review cases, not executed tests.

This cursor guards software ordering; it cannot prove physical Break timing.
Delay between admission and the actual falling edge must fit the application's
remaining timing budget, and finishing a UART header is not finishing a frame.
Integrating the cursor with actual UART transactions and
qualified wake/clock behavior remains pending. A dropped cursor does not stop
external hardware; retain or explicitly abort it when cancelling a transfer.

## Table changes at slot boundaries

`request_plan(&new_plan)` queues an already validated table. Network nominal
baud, time base and jitter must match the active cursor exactly; otherwise it
returns `IncompatibleTiming` without changing the active or pending table.
The latest compatible request replaces an earlier queued request. Requests
made during a fault return the retained error. `cancel_plan_change()` withdraws
only the pending request and cannot clear a fault.

The current slot, its returned frame and its end time remain unchanged. After
the application finishes that slot, the next successful `start_slot` commits
the new table at entry zero using the existing next boundary. Early polls leave
the request pending; an active slot prevents admission. Lateness and timestamp
overflow fail before committing the change. The same rule applies before the
cursor's first slot: the request can select the table at the original epoch.

For example, a slot ending at 6,000 us can queue a new table while active and
finish at 5,500 us. The new table still starts at 6,000 us, not 5,500 us; its
entry-zero duration determines the following boundary. A second compatible
request before that boundary wins. An incompatible request leaves the first
pending request intact. `restart` discards pending requests and restarts the
last admitted table after application-controlled recovery.

The cursor retains borrowed entries, so the selected tables cannot be mutated
or freed while referenced. A plan wrapper itself need not be retained merely
because its entry slice is queued. All referenced definitions must belong to
the same network contract; matching timing does not validate publisher roles,
diagnostic policy or consistency of frame layouts across application modes.
No UART settings change, no active transfer is interrupted, and no physical
bus recovery is inferred. Table-switch cases were source-reviewed only.
