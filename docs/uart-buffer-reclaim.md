# UART receive-buffer reclamation

Implemented on 2026-10-15; pending software acceptance. No local compiler,
test, validation script, firmware or hardware run was performed for this change.
Earlier buffered-UART acceptance records do not validate this new method.

`BufferedUartRx::into_buffer(self) -> &'d mut [u8]` stops collection and returns
the entire original allocation. It snapshots only the raw pointer and length
under the existing critical section, then consumes the RX owner through its
normal Drop before constructing the returned mutable slice. Drop masks RX
interrupt sources, disables RX and RTS, detaches the ring from ISR state and
handles the existing shared lifecycle and clock ownership. If teardown panics,
no buffer reference is returned. No new shutdown sequence is introduced.

The consuming signature prevents a live `fill_buf` slice or read future from
being used across reclamation. A combined `BufferedUart` is first split; its
ordinary TX half can remain live and does not access RX storage. Pin and
peripheral tokens are not returned. The method neither flushes nor stops TX.

The returned lifetime is the RX owner's `'d`, not unconditionally `'static`.
A `BufferedUartRx<'static>` returns a static mutable slice suitable for another
safe static-buffer constructor. Shorter peripheral/pin borrows can shorten the
returned lifetime even if the original buffer was static. Unsafe borrowed
constructors retain their existing storage-validity/no-leak obligations until
shutdown; this method does not make them safe to forget.

Unread bytes and pending errors are discarded as ring state. Storage bytes are
not zeroed or reordered, and the returned slice is not a receive-data result.
Read any required clean prefix before consuming the receiver. Constructor errors
still consume static storage; ordinary Drop does not return static storage.
DMA staging, DMA cancellation, idle markers and running buffer replacement are
outside this change.

When local validation is authorized again, acceptance needs public-API lifetime
checks for static and borrowed storage, rejection of live-prefix/future reuse,
RX-only and split-TX teardown review, shared IRQ behavior and the existing
buffered-UART regressions. These are pending work, not executed results.
