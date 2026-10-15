# Native single-wire UART examples

Select exactly one feature:

| Feature | UART | TXD | RXD |
| --- | --- | --- | --- |
| `cw32l010f8p6` | UART2 | PA3 | PA4 |
| `cw32l011k8t6` | UART3 | PA4 | PA5 |
| `cw32l012c8t6` / `cw32l012c8u6` | UART1 | PA2 | PA3 |

Only TXD is consumed; RXD remains free. Add an external pull-up and common
ground. Both examples send `0x55` and wait for one response byte, using eight
data bits and no parity. The peer must wait for the request to finish and
release the line after responding. Reads flush TX before receiving. No peer,
arbitration, timeout or turnaround guarantee is supplied.

L010/L011 changes have not been compiled or run. Board acceptance remains
pending. See [source and API scope](../../docs/uart-native-word-half-duplex.md).
