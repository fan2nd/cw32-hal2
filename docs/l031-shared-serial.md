> Historical software-verification references: HAL tests and fixture harnesses cited below were deleted on 2026-10-08. Their reported results apply to the dated source snapshot only; source facts remain unchanged. See [current HAL layout and build scope](hal-production-layout.md).

# Shared L031 SPI/I2C controller HAL audit

## Implemented boundary

The `spi_cw32l031_v1` / `i2c_cw32l031_v1` backends use the existing Embassy
`Peri` ownership, sealed instance/pin traits and blocking embedded-hal APIs.
The protocol engines remain shared with the existing x030/F020 implementation.
This is a behavioral review against each family's manual and SDK, not an
inference that equal register layouts make arbitrary modes interchangeable.

- L031, R031 and W031: SPI1 and I2C1 only.
- L052 and L083: SPI1/SPI2 and I2C1/I2C2.
- The conservative `cw32l031` family alias has no safe SCL pad, so I2C1 cannot
  be constructed through the safe API there. Select an exact orderable-part
  feature: all exact parts have a complete safe route set. SWD is not remapped.
- SPI: full-duplex master, modes 0–3, MSB/LSB first, 4–16-bit words, optional
  delayed sampling, separately managed GPIO chip select. Ceiling is the lower
  of **12 MHz and PCLK/4** on all five new families.
- I2C: seven-bit master, adjacent-operation merging, repeated START, distinct
  address/data NACK, bounded poll-count waits, clock stretching, arbitration
  reporting and bounded controller cleanup. Up to **1 MHz**, additionally
  limited by the valid baud divisor and board electrical constraints.
- No async or DMA path is added. No controller IRQ is enabled. SPI polling
  remains unbounded as in the original API; I2C poll counts are not durations.
  No silicon or electrical validation is claimed.

The original x030 SPI PCLK/2 and 16 MHz limits and F020 PCLK/2 and 12 MHz limits
are unchanged. L010/L011/L012/F002/F003 controllers are outside this extension.

## Pinned primary evidence

Exact PDF/SDK URLs, SHA-256 digests and implementation-source digests are in
[`l031-shared-serial-evidence.json`](l031-shared-serial-evidence.json).
SDK headers, `Libraries/src/cw32*_spi.c`, `cw32*_i2c.c` and the independent
CMSIS layout audit were checked against the manual register descriptions.
Printed page numbers below are the document's own numbering.

| Family | User manual | SPI feature/register sections | I2C behavior/register sections | Clock gate/reset sections | Datasheet |
| --- | --- | --- | --- | --- | --- |
| L031 | CN V1.6 | §19.2 p359; §19.8 pp385–390 | §§20.4–20.7; divisor p397, CR pp421–422 | §§4.7.12–4.7.16, pp77–81 | CN V1.9 |
| R031 | CN V1.3 | §19.2 p362; §19.8 pp388–393 | §§20.4–20.7; divisor p400, CR pp424–425 | §§4.7.12–4.7.16, pp79–83 | CN V1.2 |
| W031 | CN V1.4 | §19.2 p362; §19.8 pp388–393 | §§20.4–20.7; divisor p400, CR pp424–425 | §§4.7.12–4.7.16, pp78–82 | CN V1.3 |
| L052 | CN V1.5 | §20.2 p396; §20.8 pp422–427 | §§21.4–21.7; divisor p434 | §§4.7.12–4.7.16 | CN V1.3 |
| L083 | CN V2.0 | §20.2 p401; §20.8 pp427–432 | §§21.4–21.7; divisor p439 | §§4.7.13–4.7.17 | CN V1.9 |

### Source contradictions and conservative choices

Every new manual's SPI feature list says maximum master speed is PCLK/4.
Each CR1.BR table nevertheless describes encoding 000 as PCLK/2. The HAL does
not select that encoding on these five families, even at a low PCLK where an
absolute 12 MHz cap alone would have allowed it. BR=001 through 110 implement
/4 through /128; BR=111 remains reserved. Requests exceeding PCLK/4 are rejected
before any MMIO, consistently with the original driver's checked-ceiling API.

All five datasheet feature summaries say 12 Mbit/s SPI. Some electrical tables
say 16 MHz master or a 62.5 ns SCK period. The HAL retains the lower 12 MHz limit
as a separate constraint, rather than choosing the more permissive statement.
W031 also has an RF-interface SPI limit; that separate RF block is not exposed
by these SPI1 pin/instance traits.

## Clock, reset and register access

| Instance | Base | Gate | Reset |
| --- | --- | --- | --- |
| SPI1 | 0x40013000 | SYSCTRL+0x34 bit8 (APBEN2) | SYSCTRL+0x44 bit8 (APBRST2) |
| SPI2 (L052/L083) | 0x40003800 | SYSCTRL+0x38 bit6 (APBEN1) | SYSCTRL+0x48 bit6 (APBRST1) |
| I2C1 | 0x40005400 | SYSCTRL+0x38 bit11 (APBEN1) | SYSCTRL+0x48 bit11 (APBRST1) |
| I2C2 (L052/L083) | 0x40005800 | SYSCTRL+0x38 bit12 (APBEN1) | SYSCTRL+0x48 bit12 (APBRST1) |

These gate/reset words are **unkeyed** on all five families. Reset is asserted
by zero and released by one, matching each family's `SPI*_DeInit` and
`I2C*_DeInit`. Writes are masked read-modify-write inside a critical section;
a gate readback precedes configuration. No SYSCTRL key, GPIO key, local
peripheral clock selector or UART-style clock-source field is involved.
Both SPI and I2C kernels use frozen PCLK directly. L031/R031/W031 bit6 in
APBEN1/APBRST1 and bit12 in APBEN1/APBRST1 are reserved: the corresponding
second-instance branches are compiled out and rejected by internal helpers.

All accessed registers use 32-bit aligned bus words, including the 16-bit SPI
DR field and 8-bit I2C DR field. SPI offsets are CR1=0x00, IER=0x04,
CR2=0x08, SSI=0x0c, ISR=0x10, ICR=0x14, DR=0x18. I2C offsets are
BRREN=0x00, BRR=0x04, CR=0x08, DR=0x0c, ADDR0=0x10, STAT=0x14,
ADDR1=0x20, ADDR2=0x24 and MATCH=0x28. STAT/ISR are read-only.
Generated `v1` and `cw32l031_v1` register structures differ only in descriptive
text for the reviewed SPI/I2C blocks; this equality is additionally regression
checked without treating text equality as behavioral proof.

## SPI behavioral checks

- CR1: CPHA bit0, CPOL bit1, MSTR bit2, BR bits5:3, EN bit6, LSBF bit7,
  SMP bit8, SSM bit9, WIDTH bits13:10, MODE bits15:14. WIDTH is bits minus one;
  legal widths are 4–16. DMA bits16/17 and MISOHD bit18 remain disabled.
- Full duplex keeps MODE and CR2.HDOE zero. SSM=1 and SSI=1 hold the unused
  hardware select output inactive; no physical CS route is configured.
- ISR: TXE bit0, RXNE bit1, underrun bit4, overrun bit5, SSERR bit6, MODF bit7,
  BUSY bit8. DR reads consume RXNE. Successful operations drain every incoming
  frame and wait for TXE plus not BUSY before returning.
- ICR is R1W0. Clearing by zero also asserts FLUSH (bit0), which clears the
  transmit and shift buffers. The driver performs this while disabled during
  configuration/recovery and does not write the read-only ISR.
- On an error the existing engine disables the controller, clears its flags
  and buffers and re-enables it. Partially received buffers remain partially
  changed; no implicit retransmission is added.

## I2C behavioral checks

- BRR is 8 bits; **1–255**, not 0–255, is valid for master operation.
  SCL=PCLK/[8×(BRR+1)]. The driver rounds the divisor upward with u64 math,
  never exceeding the requested ceiling. BRREN.EN is enabled for master use.
- CR bits: FLT=0, AA=2, SI=3, STO=4, STA=5, EN=6; reserved bit1 stays clear.
  FLT=1 selects the simple filter when BRR≤9; FLT=0 above that threshold.
- SI is W0-to-clear-and-advance; W1 has no action. Registers and DR are set
  before SI clears. STA needs software clearing; STO clears in hardware after
  STOP. AA is disabled except when another byte of the current read run is
  required, including across adjacent read buffers.
- Master states are START=08, repeated START=10, write address ACK/NACK=18/20,
  transmit ACK/NACK=28/30, read address ACK/NACK=40/48, received ACK/NACK=50/58.
  F8 alone is not an idle proof: it also occurs between protocol states.
- Arbitration states 38/68/78/B0 cause arbitration error and no STOP or retry.
  Bus-error state 00 uses STO to clear state without sending an on-wire STOP.
  A stuck cleanup falls back to EN=0, EN=1, SI=0 as described in the manuals'
  state-code section. It cannot force an external device to release SDA/SCL.
- Own addresses are never written by master configuration; reset leaves
  general-call disabled. SDA/SCL use digital input plus open-drain output,
  and optional weak pull-ups supplement, rather than replace, external pull-ups.

## Integration and verification

Build selection must independently approve these two register versions, emit
`hal_spi`, `hal_i2c`, `hal_af` and the corresponding IP selectors, and generate
only present `impl_instance!` entries and package-bonded `impl_pin!` entries.
SPI requires `Flex::set_as_af`; I2C requires `Flex::set_as_af_open_drain`.
Those family GPIO hooks must retain their real lock semantics and must not
write an invented SPEED register. Pin-route provenance is a separate AF audit.

- `src/spi/tests/mod.rs` and `src/i2c/tests/mod.rs` run the production protocol
  engines against deterministic models and the selected PAC against RAM.
  They test configuration offsets/bits, data access, flags, recovery, every
  prescaler/BRR choice, family limits, neighboring gate/reset preservation and
  absent second-instance rejection.
- `tests/test_l031_shared_serial_hal_contracts.py` compiles all 21 feature
  selections, every safe bonded signal trait and one real constructor per
  present instance with a complete safe route set. The L031 family alias also
  rejects every safe pad as SCL. Negative controls verify wrong routes, retained token
  borrows, absent second instances and absence of an async constructor.
- `tests/test_l031_shared_serial_sources.py` checks independent expected
  layouts/fields, common-IP structural equality and pinned source claims.

Use the repository-local Rust toolchain and `CARGO_INCREMENTAL=0`. Host tests
model controller behavior; RAM verifies resulting writes but not electrical
edges or the transient assertion of a real reset pulse. Target checks do not
flash or execute firmware. Hardware smoke testing remains required before a
production stability claim.

### Recorded verification result

The focused verification run passed:

- 372 SPI/I2C host unit-test executions across L031, R031, W031, L052, L083,
  F030, A030 and F020 family profiles, plus `rt,defmt` Cortex-M0+ checks for
  those eight profiles.
- All 21 new family/exact-part compile-contract selections: 770 bonded signal
  checks, 61 real constructor controls, and 153 specifically diagnosed intended
  failures. The conservative L031 alias contributes the sole incomplete I2C
  route set and is explicitly checked rather than bypassed.
- Independent layout/field/gate assertions and replay of 30 pinned official
  source hashes, including all five manuals, datasheets, SDK archives, CMSIS
  headers and both driver implementation files per family.

Logs: `verification-logs/l031-shared-serial-representative.log`,
`verification-logs/l031-shared-serial-contracts.log`, and
`verification-logs/l031-shared-serial-sources.log`. Earlier representative
compilation during AF integration emitted temporary unused-pin-macro warnings;
the final route compile run includes the generated AF implementations.
