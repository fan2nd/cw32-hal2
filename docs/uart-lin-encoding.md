# LIN response encoding

`LinIdentifier` accepts IDs 0..61 and computes or validates protected-byte parity.
`LinResponse` copies a payload into private nine-byte storage and appends a
classic or enhanced checksum. Payload length is 1..8; diagnostic IDs 60/61
require eight bytes and classic checksum. `from_received` validates a supplied
complete response, without collecting UART data. Invalid inputs never access
hardware. No heap allocation or mutable byte access is exposed.

`write_sync_identifier` queues 0x55/PID after an application-provided break and
delimiter. `write_response` queues the prepared payload/checksum; blocking forms
are also available. The caller must match the response to the transmitted header.
Flush and cancellation retain the existing transmitter contract. These helpers
add no interrupt sources, DMA path, receive state machine, retries or automatic
header tracking. Network payload layouts and scheduling remain application work.

Protocol basis: LIN Consortium **Specification Package 2.2A**, §2.3.1.3–2.3.1.5
pp31–32 (identifier/data/checksum), §2.3.3.4 p37 (diagnostic length), §2.3.3.5 p38
(reserved IDs), §2.8.3 p55 (carry example), available from the
[NXP-hosted specification](https://community.nxp.com/pwmxy87654/attachments/pwmxy87654/16-bit/18786/3/LIN_Specification_Package_2.2A.pdf).
This fixes the encoding target at 2.2A; it does not claim complete LIN/ISO
conformance. CW32 register behavior remains sourced by
[native LIN transmission](uart-native-lin-tx.md).

Local compilation/tests remain prohibited. PID parity, carry boundaries,
diagnostic restrictions, bad received checksums and copied-storage independence
remain pending acceptance. The example selects ID 0x12 with two data bytes on
a deliberately configured test network; it does not qualify transmission timing.

`LinFrameAssembler` consumes receive events for one explicitly configured ID,
payload length and checksum model. It requires Break, 0x55, validated PID, then
exactly the configured payload and checksum. Any Break abandons partial state
and starts a new header. A valid other ID skips the rest until another Break;
invalid sync/PID/checksum returns an error and waits for another Break. Completion
returns an owned `LinResponse`, then ignores trailing bytes until another Break.
Construction enforces payload/diagnostic constraints before any event is accepted.

The caller must reset on UART errors, known loss and expired deadlines. The
assembler has no clock and cannot validate physical break duration, event ordering,
frame timing, missing bytes or diagnostic transport segmentation. The receive
example monitors ID 0x12 with two enhanced-checksum payload bytes and resets on
UART errors. It supplies no timeout or bidirectional response. State transitions,
resynchronization, checksum failures and output independence are source-reviewed
but remain uncompiled and untested under the local execution restriction.
