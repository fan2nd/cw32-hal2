# UART buffer reclamation

`BufferedUartRx::into_buffer(self) -> &'d mut [u8]` consumes RX, runs normal
shutdown to detach ISR storage, then returns the original buffer. The interface
is implemented but has not been compiled or tested.

- Returned storage has the owner's lifetime; only a static owner returns a
  static slice. Unsafe borrowed constructors retain their no-leak obligations.
- Live read futures or `fill_buf` borrows prevent consuming the owner.
- For combined UART, call `split` first. TX remains usable; pins and peripheral
  tokens are not returned, and TX is not flushed.
- Unread data and errors are discarded. Bytes are neither cleared nor reordered.
- Constructor errors still consume static storage. Ordinary Drop does not return
  it. DMA storage and running buffer replacement are separate concerns.
