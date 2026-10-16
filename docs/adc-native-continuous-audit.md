# Native ADC continuous acquisition admission

L010/L011/L012 continuous operation cannot currently inherit the classic
`read_continuous` buffer contract. The hardware mode exists, but the inspected
own manuals do not provide the classic unread-result OVW signal. This is an
unresolved acquisition protocol, not a claim that the hardware lacks continuous
conversion or that ordinary single-shot APIs are affected.

Sources are the pinned official manuals in `sources/evidence-sources.json`:

| Family / revision | Continuous mode, printed / PDF | ISR, printed / PDF | ICR and RESULT0, printed / PDF |
| --- | --- | --- | --- |
| L010 CN1.2 | 507 / 508 | 522 / 523 | 523 / 524 |
| L011 CN1.1 | 509 / 510 | 524 / 525 | 525 / 526 |
| L012 CN1.4 | 579 / 605 | 597 / 623 | 597–598 / 623–624 |

L010/L011 §20.5.1 and L012 §25.5.1 specify CONT=1, repeated sequences, one
EOC per conversion, one EOS per sequence, and replacement of the corresponding
RESULT slot. They warn that software must read results in time. START=0 stops
and resets the sequence cursor. The result registers are RO. The documented
ISR/ICR fields are completion and analog-watchdog flags, with no overwrite
indicator or generation counter. L012 bit2 is reserved; its watchdog flags at
bits3/4 must not be repurposed as an overrun indication. ICR uses R1W0.

## Why the classic collector cannot simply be copied

For a single-slot continuous sequence, consider the clear-before-read order:

1. Conversion A completes, setting EOC and depositing A in RESULT0.
2. Software observes EOC and clears it.
3. Conversion B completes before software reads RESULT0, replacing A and
   setting EOC again.
4. Software reads B. The still-set EOC can immediately admit a second read of B
   before conversion C completes.

There is no documented OVW bit to reject that trace. Clearing after the read
instead can erase a new completion's notification. Reading EOC on both sides
does not supply a sample generation number. Equal values cannot identify a
duplicate because consecutive genuine conversions may return identical counts.
Masking CPU interrupts does not halt ADC hardware.

With multiple slots, EOS is a sticky event rather than an atomic snapshot
latch: later conversions can replace early slots while software reads later
ones. A scalar RO read does not prove that a whole returned array belongs to
one sequence. Stopping after observing EOS also needs a documented retention
boundary: it does not by itself prove that no slot was replaced before STOP.

## Evidence needed to implement the requested acquisition contract

- A documented result latch, generation/overwrite signal, or handshake that
  identifies which completion a read consumes, including simultaneous clear,
  read and conversion completion behavior; or
- A qualified minimum inter-conversion/sequence interval together with a
  guaranteed maximum polling/IRQ/executor service and bus-access interval,
  covering every enabled preemption and clock condition; or
- A qualified DMA request, queue, endpoint order and overrun protocol, including
  final-sample retirement and cancellation, sufficient to preserve sample order.

The current APIs impose no application-wide upper bound on interrupt or
executor latency. Choosing a slower ADC clock alone does not establish such a
bound. L012 DMA request and result-order qualification is already independently
pending. Repeated software single shots would change the requested hardware
operation and are not a substitute. A lossy latest-value observer would need
an explicitly different contract; this audit does not silently expose one as
ordered continuous acquisition.

No new native continuous API or register fields are added. Source review only;
no generation, compilation, test execution or hardware validation was run.
