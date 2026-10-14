# L012 nine-bit ordinary UART examples

Select exactly one of `cw32l012c8t6` / `cw32l012c8u6`. UART1 uses PA2 TX and PA3
RX; connect them for loopback. Both packages use their metadata-derived memory
map. A normal eight-bit USB-UART adapter is not a suitable nine-bit peer.

`async_split` exercises independent TX/RX ownership and u16 containers concurrently;
`blocking` sends and receives one frame at a time. Config requests nine data bits,
no parity, one stop bit and default115200 baud. Ordinary receive has one hardware
buffer, not a software ring; delayed servicing can overrun. Each successful write
is explicitly flushed. No timeout or retry is supplied.

These are buildable firmware examples, not recorded board test results.
See [the qualification and API contract](../../docs/uart-l012-nine-bit.md).
