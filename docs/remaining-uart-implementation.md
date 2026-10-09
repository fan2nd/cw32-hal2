> Historical software-verification references: HAL tests and fixture harnesses cited below were deleted on 2026-10-08. Their reported results apply to the dated source snapshot only; source facts remain unchanged. See [current HAL layout and build scope](hal-production-layout.md).

# Remaining UART variants

Implemented 2026-10-08. Source-reviewed and tested with generated-PAC aligned
RAM, deterministic protocol/IRQ models, and Cortex-M0+ compile contracts.
**No board, electrical, DMA-throughput or silicon validation is claimed.**

## Supported boundary

UART blocking and per-byte interrupt async are enabled for F002/F003,
L010/L011/L012, L052 and L083, using the same owned `Peri`, sealed TX/RX pins,
split halves and `embedded_io`/`embedded_io_async` APIs as the existing drivers.
There is no software RX queue, DMA or retained caller-buffer pointer in an ISR.
The executor must service every frame promptly. Cancellation masks only the
operation's event bits and retains unread RX flags/data; a completed RX/TX prefix
is not undone. Dropping a TX half can truncate queued data; flush first.

Data format is eight data bits, none/even/odd parity, one/1.5/two stop bits,
normal asynchronous full duplex. PCLK is the actual frozen clock. Baud selection
uses the existing checked rounded integer/fraction solver for 16/8/4 sampling;
8/4 sampling always clears BRRF. LSI sampling, deep-sleep communication, LIN,
RS485 automatic direction, loopback, synchronous mode, inversion, flow control,
automatic baud, address matching and nine-bit data remain outside this API.

Each version has its own actual register predicate. There are no `hal_*` or
implementation-label predicates. L011 explicitly selects `uart_cw32l010_v1`
because that is its metadata register version, while its own clock topology is
selected by `cw32l011`. Register policy and gate topology live in
`usart/variant/mod.rs`; shared-vector dispatch lives in `usart/shared_irq/mod.rs`.
Every file-backed module follows `module/mod.rs`.

## Primary source basis

The earlier read-only audits remain historical plans:
`f002-f003-serial-read-only-audit.md` and `read-only-low-power-serial-audit.md`.
The new `remaining-uart-evidence.json` pins each family's manual (when available),
CMSIS header, UART header and UART implementation by SHA-256 and official URL.
`tests/verify_remaining_uart_sources.py --sources /workspace/shared/cw32-sources`
rechecks original hashes, each manual's own R1W0 table, SDK flags, UART base
addresses/vectors, selected PAC fields/offsets and all package metadata.

| Family | Own authority and relevant sections |
| --- | --- |
| F002 | UM CN V1.4 ch15, §§15.3.3.3–4 and 15.8.9; SYSCTRL §§4.7.11–15; SDK V1.2 |
| F003 | UM CN V2.3 ch16, §§16.3.3.3–4 and 16.8.9; SYSCTRL §§4.7.11–15; SDK V1.7 |
| L010 | UM CN V1.2 §§16.3, 16.8.1, 16.8.12; SYSCTRL §§4.7.12–16; SDK V1.0.9 |
| L011 | Own UM CN V1.1 §16.8.1 pp421–422 parity/clock; §16.8.11/.12 pp428–430 status/R1W0; SYSCTRL §§4.7.12/.15 pp77/80; own SDK V1.0.3 and DS V1.1 AF tables |
| L012 | UM CN V1.4 §§21.3, 21.9.1, 21.9.12; SYSCTRL §4.7; SDK V1.0.5 |
| L052 | UM CN V1.5 §19.9, ICR §19.9.11; SDK V1.4 |
| L083 | UM CN V2.0 §19.9, ICR §19.9.9; SDK V2.2 `cw32l083.h` vector/base tables |

**The L011 manual evidence gap is closed by its own CN V1.1 manual**, acquired
and checked on 2026-10-08. It corroborates the current UART register/ICR/parity
policy; no driver change was required. This is source review, not hardware
validation. See `l011-manual-follow-up-audit.md`.
Reviewed AF promotion/package/debug exclusions are independently verified by the
serial AF source audit and are not inferred from register compatibility.

## Register policies and errors

All versions use CR1/CR2/IER/BRRI/BRRF at 0/4/8/12/16, ISR at 0x1c,
ICR at 0x20, RDR at 0x24 and TDR at 0x28. Accesses are 32-bit.

| UART version | FE / PE | NE / ORE | TXBUSY | ICR reset / clearable |
| --- | --- | --- | --- | --- |
| F002 (also F003), L083 | 3 / 4 | absent | 8 | 0xff / 0x5e |
| L052 | 3 / 4 | absent | 8 | 0xfff / 0xe5e |
| L010 (also L011), L012 | 8 / 9 | 10 / 11 | 14 | 0x1fff / 0x1ffe |

Every live ICR acknowledgement writes `reset & !(observed & clearable)`, never
RMW or generic zero. TXE/TC/RC are bits0/1/2 throughout. Auxiliary status flags
remain untouched during transfers. L052 reserved bit0 is not an event, and its
reserved bit8 must remain one in ICR even though ISR bit8 is TXBUSY. On L083 TC
is per-frame; all flush paths use TXBUSY to include queued data and the shifter.

L010/L011/L012 eight-data-bit parity needs CHLEN=1 and PARITYEN=1; even sets
PARITY=0 and odd sets PARITY=1. None clears all three. CR3 is zeroed to disable
LIN/RS485, and CR2=0 deliberately selects GPIO RX and disables timer/swap/loopback/
ADC options and any implemented DMA. L012 rejects `rx_pull=Pull::Down` as
`ConfigError::UnsupportedRxPull` before RCC lookup, clock/reset or pin access:
its only pull-down pad PF3 has no reviewed UART route.

Receive error priority is **overrun, parity, framing, noise**. RDR is read only
if RC was observed, then only the observed RC/error flags are acknowledged.
An error discards that frame and permits the next operation to recover. An error
arriving after the initial status snapshot is retained for the next operation.
L010/L012 manuals say noise detection is inactive at fourfold (and specialized
low-speed) sampling. F002/F003/L052/L083 have no overrun flag and may silently
lose an unread frame; host tests cannot establish high-baud reliability.

## Gates and reset

| Family | UART instances: APB register / bit |
| --- | --- |
| F002/F003 | UART1 APBEN2/9; UART2 APBEN1/7 |
| L010 | UART1 APBEN1/3; UART2 APBEN1/4 |
| L011/L012 | UART1 APBEN1/3; UART2 APBEN1/4; UART3 APBEN1/8 |
| L052 | UART1 APBEN2/9; UART2 APBEN1/7; UART3 APBEN1/8 |
| L083 | UART1 APBEN2/9; UART2–5 APBEN1/7,8,9,10; UART6 APBEN2/1 |

L010/L011/L012 APBEN writes use upper key 0x5a5a and preserve all unowned low
bits. Others preserve the full unkeyed word. Gate acknowledgement is bounded
(100,000 reads) before UART MMIO. Timeout panics without marking the instance
active. APBRST is unkeyed and active-low throughout: owned bit 0, then 1, under
the critical section, preserving unrelated lines. No debug/reset/oscillator
routing is changed. Unsupported UART indices are unreachable before MMIO.

## L083 shared-vector ownership and boot boundary

IRQ27 is UART1+UART4, IRQ28 UART2+UART5 and IRQ29 UART3+UART6. Each `Instance`
has an associated required `InterruptHandler`. Nonshared devices retain
`InterruptHandler<UARTn>`. L083 requires the complete fixed vector group, e.g.:

```rust
hal::bind_interrupts!(struct Irqs {
    UART1_UART4 => hal::usart::SharedInterruptHandler<hal::interrupt::typelevel::UART1_UART4>;
});
```

That one binding serves both instances. A single-instance handler cannot satisfy
a shared UART constructor, and users cannot choose an incomplete group.
The group handler checks each instance's critical-section-protected active flag
**before reading IER, ISR or any other UART register**. It dispatches only active
instances. No caller buffer pointer is installed in the ISR.

Construction, including blocking construction, does not disable or unpend the
shared vector. Initialization marks the local state inactive, enables/readbacks
the local gate, pulses only its reset, masks/configures only its UART, then marks
it active under the same critical section. An active partner's IER/status/clock
are untouched. Either split half keeps the instance active. The last half masks
its direction, clears its enable, marks the instance inactive and gates only its
clock, all under one critical section. On L083 `Info` contains **no NVIC-disable
callback**, and the NVIC line stays enabled after first use, even after both
owners are dropped. An already-pending vector then performs no inactive MMIO.

**Boot/external-owner assumption:** the HAL does not own an unconstructed
partner's state. An inactive guard does not imply that a bootloader/external
UART has IER=0. Before enabling the shared vector, that external owner must
quiesce its source or provide compatible servicing. The HAL deliberately does
not read, clear flags, reset, gate or otherwise seize an unowned partner to
establish this. Otherwise a still-asserted unowned source can repeatedly pend
the line. HAL-owned teardown does establish masked local event sources and an
inactive guard; reset-startup normally supplies the initial quiescent state.

## Reproducible acceptance checks

- Existing UART baud, cancellation, one-shot ISR, error ordering and flush-race
  tests run against each selected new PAC; dedicated variant tests hard-code
  documented register words, masks, keyed gates and every supported instance.
- All 27 parity/stop/oversampling combinations are checked against RAM. The low
  power error test covers every NE/ORE/PE/FE combination both with and without RC,
  auxiliary flag preservation, error-only wakeups and arrival during arming.
- L083 tests cover all three pairs, both owner-drop orders, both half-drop orders,
  pending partner events during construction/drop, unconstructed partners,
  cancellation, immediate reuse and both-inactive zero-MMIO dispatch. The mock
  asserts the actual PAC SYSCTRL gate before each register access and verifies
  unowned bootloader IER/status are preserved rather than forcibly quiesced.
- `tests/test_remaining_uart_contracts.py` compiles blocking/async/halves and
  ownership recovery for every remaining exact part and family alias. Negative
  controls check reversed pins, wrong/missing IRQ, partial shared handler,
  nonexistent UART3, retained peripheral borrow, split borrow and exclusive
  receive futures, with expected Rust diagnostics after positive controls.
- Full project regression and independent shared-IRQ/source review remain the
  parent integration gate. Compile/RAM/model results never substitute for board
  tests, real interrupt-latency measurement or electrical qualification.

### Implementation verification run (2026-10-08)

- Source checker passed all seven families and 27 exact-part/family features,
  including original PDF/SDK hashes and own-manual R1W0 tables where available.
- The complete Cortex-M0+ UART contract matrix passed 27 positive feature
  controls and 179 intended negative controls. L083 positive controls include
  all six UARTs and one shared binding serving either partner; partial handlers
  fail compilation.
- After final formatting, selected-PAC/engine tests passed F030 (25), F020 (25),
  L031 (32), F002 (25), F003 (25), L010 (26), L011 (26), L012 (27), L052 (25)
  and L083 (27): 263 test executions in that representative UART matrix.
- Module-layout and cfg-naming checks passed. The integration task owns the full
  crate/feature regression, final independent-review correspondence and archive.
