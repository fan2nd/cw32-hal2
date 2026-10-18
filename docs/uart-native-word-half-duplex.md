# Native UART word and single-wire modes

L010/L011/L012 ordinary PIO owners support nine data bits without parity through
`read_u16`/`write_u16` and their blocking counterparts. Bits above the configured
eight/nine-bit payload are masked. Byte APIs reject nine-bit owners before
activity, including empty calls. Buffered/DMA construction remains byte-only.

`HalfDuplexUart` owns a bidirectional TXD pin configured directly as open-drain
AF. An external pull-up and application turn-taking are required. Hardware
returns to receive after TX finishes; reads flush TX first. The owner cannot
split because RX depends on TXD. Cancellation keeps ownership and may leave a
partial transfer; Drop may truncate output. No collision detection, peer timing,
DMA, buffering, flow control or runtime mode conversion is exposed.

The L010/L011 extension reuses the existing frame setup, native word access,
observed-error acknowledgment, wait cancellation and GPIO policies. It changes
API availability, not the receive algorithm. The source basis is each family's
own manual, independently checked against its pinned SHA-256 in
`sources/evidence-sources.json`:

| Manual | Frame | Single-wire/pins | TDR/RDR | Programming |
| --- | --- | --- | --- | --- |
| L010 CN1.2 | §16.3.3.1 p393 | §16.3.1 p390; §16.3.3.5 p400 | §16.8.8–9 p424 | §16.6.3 pp416–417 |
| L011 CN1.1 | §16.3.3.1 p395 | §16.3.1 p392; §16.3.3.5 p402 | §16.8.8–9 p426 | §16.6.3 pp418–419 |

Page numbers are printed; PDF pages are +1. Both table16-2 entries define
CHLEN=1/PARITYEN=0 as nine payload bits; both data registers expose bits8:0.
Both manuals require SIGNAL=1, TXEN=RXEN=1, open-drain TXD and an external
pull-up. They explicitly describe automatic return to RX. ORE remains a data
loss error; new frames can overwrite unread RDR. No lossless guarantee follows.

L012 retains its [word](uart-l012-nine-bit.md) and
[single-wire](uart-l012-half-duplex.md) source contracts. Non-native families
remain excluded. The expanded examples use existing qualified package routes.
L010/L011 release examples pass; see [software acceptance](validation-scope.md).
Bit8 signaling, error injection, cancellation timing and electrical behavior
still require hardware acceptance.
