# L012 UART single-wire request/response initiators

Build either exact package using the release profile:

    cargo build --release --target thumbv6m-none-eabi --features cw32l012c8t6 --bins
    cargo build --release --target thumbv6m-none-eabi --features cw32l012c8u6 --bins

The `blocking` and `async_turns` binaries use UART1 PA2 as a bidirectional
open-drain line, 115200 baud, eight data bits, no parity, one stop bit.
Connect PA2 to a compatible single-wire peer with common ground and an external
pull-up appropriate to the voltage, line capacitance and desired baud. RXD PA3
is unused. Do not connect the board's TXD to its RXD as a loopback test.

Each initiator sends 0x55, then reads exactly one response byte. The peer must
wait until the request's final stop bit, send one byte, and release the bus.
The read implicitly flushes local queued TX first. The peer is not supplied,
no timeout/recovery is implemented, and the code may wait indefinitely.
Applications must establish turn-taking; this driver does not arbitrate or
provide collision detection. No board, pull-up, peer-timing or signal-integrity
test is implied by a successful build.

See ../../docs/uart-l012-half-duplex.md for lifetime, cancellation, destructive
Drop, nine-bit and supported-empty-read contracts.
