# Continuous IRQ-buffered UART receive

Current implementation adds [`into_buffer`](uart-buffer-reclaim.md) for explicit
RX storage reclamation. It is pending software acceptance; the historical
results below do not validate it.

The Stage83 acceptance record below is historical. Subsequent API changes and
executed checks are recorded in [UART/SPI alignment](uart-spi-api-validation.md),
[BufRead validation](uart-bufread-validation.md), and
[borrowed-buffer validation](uart-borrowed-buffer-validation.md). These later
receipts do not replace the original hardware qualification boundary.

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
using either a consumed `&'static mut [u8]` receive ring (safe constructors)
or an ordinary `&'d mut [u8]` borrow (explicitly unsafe constructors). The RX owner directly owns
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

## API and buffer lifetime

The constructors use the existing sealed pin roles and typed dedicated/shared
interrupt binding. Their argument order is:

```text
BufferedUartRx::new(peri, rx, irq, buffer, config)
BufferedUartRx::new_with_rts(peri, rx, rts, irq, buffer, config)
BufferedUart::new(peri, tx, rx, irq, buffer, config)
BufferedUart::new_with_rtscts(peri, tx, rx, rts, cts, irq, buffer, config)
```

The existing safe constructor signatures remain unchanged. Four additional
unsafe entrypoints accept `&'d mut [u8]`, tying the buffer borrow to the RX
owner: `BufferedUartRx::new_borrowed`, `new_borrowed_with_rts`,
`BufferedUart::new_borrowed`, and `new_borrowed_with_rtscts`. They use the same
arguments and validation as their corresponding safe constructors.

Normal RX Drop stops its collector and detaches its pointer before the borrow
ends. A standalone receiver's caller can then reuse the original buffer.
No explicit buffer-return method is necessary. Constructor errors do not retain the supplied
borrowed buffer. After `split`, the RX half owns the ISR storage obligation;
dropping TX alone never releases it. The combined owner gives both halves the same `'d`, so a TX
half that remains in later use can conservatively keep the buffer borrow alive
even after RX Drop. Drop both halves before reuse in that case. `split_ref` and
`fill_buf` keep their ordinary exclusive borrow restrictions.

This follows Embassy's borrowed-storage/Drop usage but deliberately does not
copy its safe nonstatic constructor signature. The reference implementation is
[embassy-stm32 buffered UART at the pinned revision](https://github.com/embassy-rs/embassy/blob/ad0d0dbd2dd4339ec17867e01d380323f52d6591/embassy-stm32/src/usart/buffered.rs).
Safe `mem::forget` can bypass Drop, including by forgetting a future containing
the receiver, while this driver continues receiving through a raw ISR pointer.
A lifetime marker alone cannot keep the caller's bytes alive or exclusive in
that case. Therefore borrowed construction is unsafe: the caller must ensure
that backing storage remains allocated, stationary, and exclusively reserved
until reception is stopped and its ISR ring is detached (normally by RX Drop).
Leaking RX does not stop it: keep the storage valid and reserved until shutdown,
or forever if shutdown never occurs. Normal cancellation is permitted when it
actually drops the RX owner; cancellation of a read alone does not stop collection.
Panic handling that resumes other execution must preserve the same storage
obligation. Keep the safe static API when these guarantees cannot be established.

All return `Result<Self, ConfigError>`. For both API forms, baud/RX-pull
validation, the nonempty-ring check and optional CTS-pull validation complete before IRQ, RCC,
GPIO, ring or lifecycle changes. A failed preflight leaves a previously
forgotten collector unchanged. Safe static constructors still consume their
supplied tokens and static buffer, without returning that storage on error,
Drop or reconstruction. Borrowed constructor errors do not retain the supplied
buffer borrow once the returned error/result is released.

Capacity one and any larger valid slice are accepted. All slots are usable;
there is no power-of-two, DMA-region, alignment or transfer-count restriction.
The HAL does not zero the storage. Private indices distinguish valid data from
unused bytes. The slice becomes a private raw storage descriptor in per-instance
protected state. The RX owner carries the borrowed-buffer lifetime marker. No allocation-wide mutable reference is retained or recreated;
ISR writes target only free bytes, disjoint from a borrowed occupied prefix.
No allocation, duplicated mutable reference or pointer to a movable owner is
required. Safe constructors need no caller-memory lifetime promise; borrowed
constructors require the safety contract above. Peripheral and pin borrows use
`'d` too. Applications can use
the existing safe `cortex_m::singleton!` pattern with its result checked.

Both owners expose `actual_baudrate()` and these receive methods:

- `read(&mut self, buffer: &mut [u8]) -> Result<usize, Error>` asynchronously
  waits for at least one byte for a nonempty slice and copies the available
  clean prefix, up to the slice length and a fixed 64-byte batch limit. It can
  copy across the ring wrap in one
  critical section. Use the returned count, not the caller buffer length.
- `read_exact(&mut self, buffer: &mut [u8]) -> Result<(), Error>` loops over
  partial reads to fill the caller slice. Cancellation/errors may leave a
  consumed prefix and do not return its length.
- `blocking_read(&mut self, buffer: &mut [u8]) -> Result<usize, Error>` copies
  queued data without waiting, following the pinned Embassy buffered API's
  naming. Zero means temporarily empty, not end-of-stream.
- `read_ready(&mut self) -> Result<bool, Error>` reports whether a nonempty
  read can immediately return data or a pending error, without consuming either.

- `fill_buf(&mut self) -> Result<&[u8], Error>` asynchronously waits for an
  occupied contiguous prefix and borrows it without copying or consuming it.
  A wrap may require a second call after consuming the first segment. Queued
  clean bytes precede pending errors, as with copying reads.
- `consume(&mut self, amount: usize)` releases bytes from that last exposed
  prefix. It panics if the amount exceeds the remaining exposed length, rather
  than consuming newly arrived data. Nonempty copying reads invalidate that
  allowance; empty reads and `consume(0)` have no effect.

Both implement `embedded_io::ErrorType`, `embedded_io_async::Read`,
`embedded_io_async::BufRead` and
`embedded_io::ReadReady` with the same semantics as their inherent methods.
No synchronous `embedded_io::Read` is exposed: the nonwaiting inherent method's
zero-for-empty result must not be mistaken for the trait's EOF contract.

Empty reads return zero (`read_exact`: unit), consuming neither bytes nor
errors, registering no waker and touching no MMIO. An unpolled future has no
effect. A partial async read never consumes data before returning `Ready`, so
cancelling a pending read loses no queued bytes. Cancelling `read_exact` can
lose an already-copied prefix, as documented above.

`BufferedUart` forwards async `write`/`flush`, `blocking_write`/`blocking_flush`,
`embedded_io_async::Write` and `embedded_io::Write` to its ordinary TX half.
`split()` returns owned `(UartTx<'d, Async>, BufferedUartRx<'d>)` halves;
`split_ref()` returns simultaneous disjoint mutable borrows of those halves.
There is no buffered TX owner.

The slice returned by `fill_buf` is tied to an exclusive borrow of the RX owner.
Safe code cannot consume, read, drop or reconstruct that owner while using the
slice. IRQ reception may write only free slots; occupied slots remain reserved
until consumed, including at overflow or an error barrier. Holding a prefix for
too long can therefore cause overflow, not mutation of that prefix. Use split
TX/RX owners to transmit while holding an RX slice. Cancelling a pending
`fill_buf` consumes nothing. Both storage modes keep this aliasing contract;
this software API does not claim hardware validation.

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
discarded. A read with room for both returns A,B, then the next read returns
`BufferOverflow`; later observations may
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
borrow. Each reader critical section copies at most 64 bytes (also bounded by the
queued length and caller slice length), across at most two contiguous ring
segments. This caps copy work independently of ring capacity, not elapsed
interrupt latency or losslessness. Waiter
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
bytes. A Pending partial read (inherent or trait) has consumed no byte or error. Background
collection continues independently in either case. No read future or read-output slice is stored in the ISR state. A borrowed
constructor does store the receive-ring pointer under its unsafe contract.

Canceling or forgetting a pending read does not mask the collector. It can
leave waiter interest and a cloned task waker; the next ready observation may
issue one stale wake. A later poll registers its own waker and checks retained
state. This does not promise an unchanged physical stream during cancellation.

Dropping buffered RX masks its receive group, disables RX/RTS and detaches the
ring under the lifecycle critical section before releasing RX/RTS pins. It
discards queued bytes, any error and the stored raw descriptor. A borrowed
ring becomes reusable when the RX borrow ends; consumed static storage is not
returned. A live TX half keeps its pins, CTS, enables, clock and
interrupt support. Dropping TX likewise preserves live buffered RX. The last
owner follows the existing clock/interrupt policy; either split-half drop
order is supported.

Forgetting a safely constructed owner can leave pins, clock, interrupts and its static ring live
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
packet framing, live ring replacement, consumed-static-storage reclamation,
mode conversion, DMA API, synchronous `embedded_io::Read` or synchronous
`embedded_io::BufRead` API. Async `embedded_io_async::BufRead` and zero-copy
`fill_buf`/`consume` are supported. Ordinary borrowed-buffer reuse after RX Drop
is supported under the unsafe constructor contract above; it is not DMA
reclamation or return of a consumed static buffer.
The existing [native finite idle API](uart-idle.md) remains on ordinary
`UartRx<Async>`/`Uart<Async>` for L010/L011/L012 with its Stage82 window, error
and cancellation semantics. Existing [staged DMA](uart-dma-rx.md) ownership,
retention and quarantine remain separate. No combined buffered RX/DMA TX
constructor is added. RF and low-power UART remain deferred.

The [buffered application crate](../examples/uart-buffered/README.md) has six
firmware binaries: `receive`, `split`, `rx_only`, `flow_control`,
`shared_vector` and the synchronous `borrowed` lifetime example. Its selected
routes use current exact-package authority. The
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
