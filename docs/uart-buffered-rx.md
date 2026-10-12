# Continuous IRQ-buffered UART receive

Stage83 main verification passed thirteen actual ARM HAL libraries, eleven ELF
links and five focused source/data gates. Independent main runtime and
metadata/source reviews accepted the frozen implementation. Three existing
native L010/L011 RCC LSE-constant warnings remain in each applicable row; all
other main rows had no warnings. Main ran no hardware generation; all 1,380
source files and 905 generated hardware files stayed unchanged during the matrix.
At main acceptance clean had not run; final-package completion requires a separate
final-source clean receipt for three libraries, three buffered ELFs and the five
focused gates. No hardware validation is claimed.

## Owners and scope

`BufferedUartRx<'d>` and `BufferedUart<'d>` are direct, interrupt-driven owners
using a consumed `&'static mut [u8]` receive ring. The RX owner directly owns
its RX pin and optional RTS pin; it does not wrap an ordinary `UartRx`.
The combined owner contains an ordinary `UartTx<'d, Async>` and a
`BufferedUartRx<'d>`. TX remains unbuffered and non-DMA.

The source/API scope covers all thirteen existing families: A030, F002, F003,
F020, F030, L010, L011, L012, L031, L052, L083, R031 and W031. All 54 current
selections retain their own UART versions, instances, clocks, typed interrupt
bindings, package-filtered routes and HAL pin exclusions. This is not a claim
that all selections or routes were compiled. Existing family/compatibility
aliases retain their current package policy; register similarity grants no
additional pin route. L011 retains its own source authority while using the
existing L010 UART register profile.

Collection starts during successful constructor activation, before the first
read is polled, and continues between reads and while the executor is delayed
if UART interrupts can run. It uses CPU reads of the one hardware receive data
register (RDR), not DMA. Reset/configuration discards prior UART state; no
pre-construction traffic is recovered.

## API and static storage

The constructors use the existing sealed pin roles and typed dedicated/shared
interrupt binding. Their argument order is:

```text
BufferedUartRx::new(peri, rx, irq, buffer, config)
BufferedUartRx::new_with_rts(peri, rx, rts, irq, buffer, config)
BufferedUart::new(peri, tx, rx, irq, buffer, config)
BufferedUart::new_with_rtscts(peri, tx, rx, rts, cts, irq, buffer, config)
```

All return `Result<Self, ConfigError>`. Existing baud/RX-pull validation, the
nonempty-ring check and optional CTS-pull validation complete before IRQ, RCC,
GPIO, ring or lifecycle changes. A failed preflight leaves a previously
forgotten collector unchanged, but the new constructor's supplied tokens and
static buffer are still consumed. The API does not return that storage on
error, Drop or reconstruction.

Capacity one and any larger valid slice are accepted. All slots are usable;
there is no power-of-two, DMA-region, alignment or transfer-count restriction.
The HAL does not zero the storage. Private indices distinguish valid data from
unused bytes. The ring reference is moved into per-instance protected state;
no allocation, duplicated mutable reference, pointer to a movable owner or
caller-memory lifetime promise is required. Peripheral and pin borrows can
still use `'d`; only the ISR destination must be static. Applications can use
the existing safe `cortex_m::singleton!` pattern with its result checked.

Both owners expose `actual_baudrate()` and an inherent async
`read(&mut self, buffer: &mut [u8]) -> Result<(), Error>` that fills the entire
caller slice unless an error occurs. Both implement `embedded_io::ErrorType`
and `embedded_io_async::Read`. A nonempty async trait read consumes exactly one
byte and returns `Ok(1)`, even when given a larger slice; use its returned count.
It does not wait to fill that slice.

On first poll, an empty inherent read returns `Ok(())`, and an empty trait read
returns `Ok(0)`. Neither consumes queued bytes or a pending error, registers a
waker or touches MMIO. An unpolled read future has no effect.

`BufferedUart` forwards async `write`/`flush`, `blocking_write`/`blocking_flush`,
`embedded_io_async::Write` and `embedded_io::Write` to its ordinary TX half.
`split()` returns owned `(UartTx<'d, Async>, BufferedUartRx<'d>)` halves;
`split_ref()` returns simultaneous disjoint mutable borrows of those halves.
There is no buffered TX owner or blocking buffered RX API.

Two variants extend currently exhaustive public enums on every UART backend:

- `ConfigError::RxBufferEmpty`: zero-capacity ring; displays
  `UART receive buffer is empty`.
- `Error::BufferOverflow`: a clean ISR-observed byte could not enter the full
  ring; displays `UART software receive buffer overflow` and maps to
  `embedded_io::ErrorKind::Other`.

Downstream exhaustive matches on both `ConfigError` and `Error` may need
updates. Existing hardware errors retain their meaning and mappings;
`BufferOverflow` is distinct from native `Error::Overrun`.

## Retained prefix and error recovery

The first observed discontinuity latches one error. Previously queued clean
bytes remain readable in order. A hardware-error observation omits its sampled
byte; a clean observation when the ring is full latches `BufferOverflow`
without overwriting queued bytes. Co-observed hardware errors retain the
existing ORE > PE > FE > NE priority. A hardware error takes precedence over
software fullness for that observation. Later faults do not replace an already
pending error or produce an exact loss count.

While an error is pending, the ISR continues draining and acknowledging
hardware but discards every later observed result. It appends nothing even
after the reader has freed ring slots. The reader drains the retained prefix
first; its next nonempty consumer step takes and clears the pending error and
returns it. Only later ISR observations can append again. Empty reads preserve
the prefix and error. No explicit restart or clear-error API is needed.

For example, a two-byte ring containing A,B receives clean C: A,B remain, and
overflow is pending. D and E observed before the error is consumed are
discarded. Reads return A then B, then `BufferOverflow`; later observations may
be retained. If an exact four-byte read copies A,B and then encounters that
error, it returns `Err` with the first two caller bytes already written and no
partial count. It does not put them back.

This boundary orders software observations, not physical arrival times or
packets. A byte already waiting in RDR when the reader consumes the error can
be observed and queued afterward. The reader does not drain RDR to establish a
wire-time boundary. Applications needing packet resynchronization must apply
their own protocol after the reported gap.

## Hardware and interrupt limits

All families have one RDR and can overwrite an unread frame. L010/L011/L012
report native ORE and NE; NE detection is limited to 16x/8x sampling, not 4x.
The other ten families have no documented ORE and can silently lose frames
before ISR service, even with space in the ring. Repeated flags can coalesce;
neither group supplies an exact count or chronology of losses. Software ring
overflow is detectable on every family, subject to the first-error policy.

Each active instance handles at most one receive observation per interrupt
dispatch. One status snapshot determines RDR eligibility, error selection and
acknowledgment. RDR is read only when that snapshot has RC, including when the
ring is full or an error is pending. The existing typed R1W0 no-op command
clears only observed receive flags; TC, CTS and auxiliary events are preserved.
Native RXIDLE stays disabled and is not consumed by the buffered path. The
critical section serializes software but cannot stop hardware reception or
prevent an already-set flag from reasserting before acknowledgment.

Buffered receive data/error interrupts remain armed between reads, at full
capacity and while an error is pending. Ordinary receivers without a ring keep
their one-shot interrupt path: their ISR leaves RDR and receive flags for task
context. RX and TX wakes occur outside the lifecycle critical section and ring
borrow. Each reader critical section copies at most one byte; waiter
registration precedes the empty-state check, and the first ready transition
wakes a waiting task. Further queued bytes do not require redundant RX wakes.

L083 retains the complete shared handlers for UART1/4, UART2/5 and UART3/6.
Each member has its own active guard before MMIO, bounded dispatch, ring and
error state. One owner does not disable/unpend the shared NVIC line or reset,
gate, drain or acknowledge its partner. Buffered and ordinary/staged-DMA
partners can coexist on different instances. An unowned partner previously
configured by external code must still be quiescent under the existing contract.

Hardware RTS reflects RC, not software ring occupancy. Reading and
acknowledging RDR can let a peer continue while the ring is full or discarding.
There is no software watermark or lossless backpressure. CTS can delay TX
indefinitely; canceling TX does not stop the independent buffered receiver.
Finite capacity does not guarantee throughput, interrupt latency, bounded
completion or lossless reception. Interrupt masking, saturation, clock policy,
peer behavior and in-flight frames remain relevant.

## Cancellation, Drop and forgotten owners

An exact read may copy a prefix before returning Pending. Canceling or
forgetting it preserves that written caller prefix and does not restore popped
bytes. A Pending one-byte trait read has consumed no byte or error. Background
collection continues independently in either case. No read future, caller
slice or stack-buffer pointer is stored in the ISR state.

Canceling or forgetting a pending read does not mask the collector. It can
leave waiter interest and a cloned task waker; the next ready observation may
issue one stale wake. A later poll registers its own waker and checks retained
state. This does not promise an unchanged physical stream during cancellation.

Dropping buffered RX masks its receive group, disables RX/RTS and detaches the
ring under the lifecycle critical section before releasing RX/RTS pins. It
discards queued bytes, any error and the stored static reference without
returning the storage. A live TX half keeps its pins, CTS, enables, clock and
interrupt support. Dropping TX likewise preserves live buffered RX. The last
owner follows the existing clock/interrupt policy; either split-half drop
order is supported.

Forgetting an owner can leave pins, clock, interrupts and the static ring live
indefinitely. The storage stays valid; an unconsumed full prefix and first error
can persist while subsequent observations are discarded. Forgetting is not
resource recovery or graceful shutdown.

Successful reconstruction through any existing constructor class retires a
stale ring before reset/configuration and new ownership. This also covers safe
reacquisition after forgetting an owner made from reborrowed tokens. Ordinary
blocking/async, RX-only, TX-only, full, RTS/CTS and admitted DMA constructors
share that boundary. Failed preflight preserves the old collector. Old storage
is not returned, and buffered bytes do not survive successful reconstruction.

## Separate modes and verification boundary

There is no buffered `read_until_idle`, idle marker queue, numeric timeout,
packet framing, ring replacement/reclamation, zero-copy borrow, mode conversion,
DMA API, `embedded_io::Read`, `BufRead`, `ReadReady` or try-read API.
The existing [native finite idle API](uart-idle.md) remains on ordinary
`UartRx<Async>`/`Uart<Async>` for L010/L011/L012 with its Stage82 window, error
and cancellation semantics. Existing [staged DMA](uart-dma-rx.md) ownership,
retention and quarantine remain separate. No combined buffered RX/DMA TX
constructor is added. RF and low-power UART remain deferred.

The [buffered application crate](../examples/uart-buffered/README.md) has five
ordinary firmware binaries: `receive`, `split`, `rx_only`, `flow_control` and
`shared_vector`. Its selected routes use current exact-package authority. The
existing [flow-control examples](../examples/uart-flow-control/README.md) remain
ordinary unbuffered applications; their earlier results are not buffered results.

Current source evidence is recorded in
[`uart-buffered-rx-evidence.json`](uart-buffered-rx-evidence.json).
Independent main runtime and metadata/source reviews accepted the frozen
implementation and its retained actual-artifact receipts. The finite acceptance
scope is:

- Main passed: thirteen release ARM HAL libraries, one exact selection per family;
  eleven ELF links comprising nine selected buffered applications, the existing
  L012 native-idle `receive` and L083 staged-DMA `shared_vector` regressions.
  All 24 commands exited successfully. Three existing RCC LSE-constant warnings
  appear in each L010/L011 library and buffered-ELF row; the other 20 rows have none.
- Final-package clean requires one fresh final-source package and separate target
  directory, with F002F3P7, L012C8T6 and L083RCT6 libraries and buffered F002
  `receive`, L012 `split` and L083 `shared_vector` ELFs. L012 library rows include
  `defmt`. At main acceptance clean had not run; final-package completion requires
  a separate final-source clean receipt.
- Main passed all five separately named focused gates: buffered source
  verification, native-idle source verification, source provenance,
  functional-coverage consistency and module layout. Clean requires these same
  five focused gates.

Main ran no hardware generation; its 1,380 source files and all 905 generated
hardware files stayed unchanged. Clean allows one necessary bootstrap only if
its source package omits generated files; all 905 generated hardware files must
then match the accepted main projection. No cumulative script,
extra matrix, synthetic HAL test, temporal model, probe or hardware execution
is part of this acceptance. Builds and source/data checks cannot establish
silicon loss rate, interrupt saturation, RTS timing or throughput.
