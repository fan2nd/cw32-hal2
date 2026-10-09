# UART typed PAC operations

The UART HAL now follows the direct generated-register style of pinned
Embassy STM32 `embassy-stm32/src/usart/mod.rs` at commit
`f16efeffe37581092ec184718e6fdb1620393214`: register fields and hardware encodings
belong to the PAC, while the HAL retains ownership, futures and protocol policy.
The private `Access` adapter and every UART hardware-bit constant are removed.
`WaitEvent::{TxReady, TxComplete, Receive}` names software wait conditions and
has no numeric hardware representation. Configuration is a private helper in the flat `usart.rs` module;
the fixed L083 vector grouping is part of the same module.

## Authored data and command values

The seven authored UART register YAMLs bind `OVER`, `STOP`, `PARITY` and
`SOURCE` to generated enums. Every exposed value is checked against the relevant
family's own manual in `uart-typed-register-evidence.json`; A030's documented
shared F030/A030 manual is explicitly identified. F002/F003 do not acquire an
LSE source variant: their own SOURCE table omits encoding 10. L052 retains the
vendor IR's `SORCE` spelling; this change does not invent an accessor alias.

L011 and L012 SDK stop-bit macros reverse the 1.5- and 2-bit labels. Their own
manuals explicitly assign STOP 01 to 1.5 and 10 to 2. The PAC follows those
manuals and preserves the existing HAL behavior. Low-power PARITY is a
one-bit even/odd selector with separate PARITYEN and CHLEN; it is not treated
as the classic two-bit none/custom/even/odd field.

Chiptool IR does not represent command write seeds. The reviewed
`cw32-data/register-writes.yaml` sidecar records hardware reset, explicit no-op,
exact R1W0 command field names and evidence separately. Data generation checks
writable register identity, scalar field widths, seed transaction width, command
membership and no-op ones. It projects only the relevant version into
`data/register-writes/`. PAC generation uses chiptool's sanitizer to emit
`Icr::reset_value()` and `Icr::write_noop()`. These are distinct functions even
where their documented values coincide. Existing zero `Default` is unchanged.

| Selected UART register version | Reset / no-op | R1W0 command domain |
| --- | --- | --- |
| v1, cw32f002_v1, cw32l083_v1 | 0x00ff | 0x005e |
| cw32l031_v1, cw32l052_v1 | 0x0fff | 0x0e5e |
| cw32l010_v1, cw32l012_v1 | 0x1fff | 0x1ffe |

These numbers occur in source-backed data and PAC verification, never in HAL
flag definitions. The HAL constructs a typed no-op command, changes only the
intended fields to false, and writes the typed value once. It never reads or
modifies ICR. In particular, L031/L052 reserved ICR bit8 stays one even though
ISR bit8 means TXBUSY. Initialization explicitly selects every implemented
R1W0 command and keeps all reserved bits at their documented values. Live
transfers never acknowledge auxiliary events or an unobserved receive error.

## Exact operation correspondence

| Previous operation | Direct generated PAC operation and preserved effect |
| --- | --- |
| status word & TXE | `isr().read().txe()`; same single word read |
| status word & TXBUSY | `isr().read().txbusy()`; flush still includes buffer and shifter |
| status word & RX_EVENTS | typed RC/FE/PE and, only on low variants, NE/ORE predicates |
| IER word & ISR word | pairwise typed status/enable predicates from the same two snapshots; no flag-position equivalence assumption |
| clear TC mask | no-op ICR with TC=false, one word write before TDR or flush arming |
| clear observed receive mask | sampled typed flags choose false commands; RDR read only when sampled RC=true, before acknowledgement |
| initialize clearable mask | each implemented ICR command setter is false; same reserved word bits remain one |
| disable pending RX/TX groups | mutate the sampled typed IER value and write it once, preserving every foreign enable |
| arm/disarm or cancel event mask | IER.modify sets only the wait condition's typed fields; same critical section and read/write order |
| clear CR1 direction bit | typed TXEN/RXEN setter; final owner check reads both fields once before clock gating |
| enable both CR1 direction bits | TXEN=true and RXEN=true in the same modify |
| raw zero IER/CR2/CR3 init | explicit setters on a zero-based PAC write; same implemented fields, zero reserved bits and register order |
| numeric OVER/PARITY/STOP | generated typed enum setters with the same source-reviewed encodings |

UART ISR/IER/ICR/CR1/BRRI/BRRF/RDR/TDR remain 32-bit bus accesses, including
RDR's byte result and TDR's nine-bit data field. No byte/halfword pointer cast or
transaction-width alias is introduced. Receive error order stays overrun,
parity, framing, noise. The arming sequence remains waker registration, optional
TC acknowledgement after TXBUSY check, IER arm, ISR recheck, and disarm if ready.
Cancellation leaves unread data and flags intact.

The central `RccInfo` patch is retained unchanged. L083 active guards still
precede any local MMIO; construction/drop do not disable or unpend the paired
NVIC vector. Only the last local split half marks its own state inactive and
gates its clock, under the same critical section. Unowned shared-vector partners
remain untouched, including external-owner pending interrupts.

## Verification scope

- `tests/verify_uart_typed_registers.py --sources <official-source-directory>`
  checks SHA-256-pinned manuals and SDKs, every enum value, each R1W0 field,
  reset/no-op values and generated projection across 13 families.
- Data-generator tests reject missing evidence, unknown command fields, a
  zero no-op seed and a seed outside the register transaction width.
- Generated PAC `uart_fields` tests exercise field value types and every
  subset of each family's documented command domain, including reserved bits.
  They do not instantiate the HAL or dereference MMIO.
- Normal Cortex-M0+ crate checks cover all 13 families with `rt,defmt`.
  Existing genuine UART examples are linked separately. No HAL test/harness,
  compile-fail test, hardware execution, electrical or latency claim is added.

The same command-seed mechanism also carries independently source-reviewed SPI
facts supplied by the SPI workstream; see `spi-register-write-evidence.json`.
