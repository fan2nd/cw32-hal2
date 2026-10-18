# UART buffer reclamation

`BufferedUartRx::into_buffer(self) -> &'d mut [u8]` consumes RX, runs normal
shutdown to detach ISR storage, then returns the current buffer. The interface
has compile-time lifetime checks and release example builds; see
[software acceptance](validation-scope.md). Hardware shutdown remains unverified.

- Returned storage has the owner's lifetime; only a static owner returns a
  static slice. Unsafe borrowed constructors retain their no-leak obligations.
- Live read futures or `fill_buf` borrows prevent consuming the owner.
- For combined UART, call `split` first. TX remains usable; pins and peripheral
  tokens are not returned, and TX is not flushed.
- Unread data and errors are discarded. Bytes are neither cleared nor reordered.
- Constructor errors still consume static storage. Ordinary Drop does not return
  it. DMA storage is separate.

`try_replace_buffer(&mut self, &'static mut [u8])` switches a running RX owner
to a new ring and returns the old storage with the owner's lifetime. Combined
`BufferedUart` also exposes this method. The same software acceptance scope applies.

- Empty input returns `BufferReplaceError::Empty`. Queued bytes or a pending
  error return `PendingReceive`; native active idle capture returns
  `IdleCaptureActive` (stop capture first). All errors return the supplied static slice
  and leave the old ring unchanged; drain data/errors before retrying.
- The swap uses the ISR's critical section without copying bytes or changing
  UART registers, pins, TX or shared-vector ownership. Live read/fill_buf borrows
  exclude replacement. The new storage must be static to remain safe if forgotten.
- Bytes pending in hardware can enter the new ring. Replacement is not a packet
  boundary or lossless handover guarantee; continuous traffic can prevent success.
