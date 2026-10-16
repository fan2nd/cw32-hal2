# Native LIN paired ownership

`LinUart` consumes one qualified UART plus TX/RX pins. Scope is L010/L011 UART1
and L012 UART1/2, using the same fixed-baud 8N1 validation as the independent
owners. It prepares the peripheral once, constructs both direction guards while
disabled, programs LIN/BRKL, then enables TX and RX together. No ordinary-UART
receive window or second reset is introduced. The async constructor enables the
bound IRQ only after both owners are active.

`transmitter`/`receiver` borrow a direction; `split_ref` permits concurrent polling;
`split` transfers both LIN-specific owners. Neither exposes ordinary UART or a
mode change. Each half retains its existing cancellation contract. Its Drop masks
only its requests, disables its direction and disconnects its pin. Existing UART
lifecycle accounting keeps the clock and IRQ usable for the other live half.
Forgetting a half retains its lifecycle ownership and can keep the clock enabled.
A queued TX may be truncated on Drop. There is no combined atomic transaction.

RX remains enabled during transmission. Echo may cause bytes, break events or
overrun; writes do not drain it and reads do not suppress it. Concurrent polling
can reduce service gaps but cannot guarantee ordering or lossless reception.
External transceiver controls, arbitration, collision handling, response selection,
oscillator tolerance and deadlines belong to the application. UART flush proves
neither external bus release nor LIN response timing.

`examples/uart-lin/src/bin/respond.rs` uses UART1 PA1/PA0 on L010F8P6 and PA2/PA3
on L011K8T6/L012C8T6/L012C8U6. A deliberately configured test network must assign
ID 0x12, two payload bytes and enhanced checksum exclusively to this publisher.
The example waits for Break/sync/validated PID, writes its response, then waits
for another Break. It does not compare echoed bytes, detect collisions or enforce
header/response deadlines; delayed header bytes may therefore still cause a reply.
It is not a production scheduled responder. UART errors abandon header state.

Hardware evidence is the same own-manual LIN format, receive and send chapters
listed in [TX](uart-native-lin-tx.md) and [RX](uart-native-lin-rx.md); no new register
encoding is introduced. Ownership/interrupt behavior was inspected against
`Info::prepare`, `Info::shutdown`, `UartTx::from_pin`, `UartRx::from_pin` and the
one-shot IRQ dispatcher. Compilation, tests, paired cancellation/Drop acceptance
and board timing remain pending under the local execution restriction.
