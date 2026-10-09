# CW32F020 UART, SPI, I2C and CRC compatibility evidence

## Decision and scope

The existing `uart::v1`, `spi::v1` and `i2c::v1` register backends have compatible
F020 clock/reset selectors and operational semantics for the limited HAL modes
listed below. Their register-shape equality is **not** the only evidence: this
review compared the F020 reference manual, SDK operations, and independently
numbered datasheet alternate-function tables.

- UART1/2/3: source-backed reuse for blocking and byte-interrupt-driven async,
  asynchronous full-duplex or owned TX-only/RX-only halves, eight data bits,
  no/even/odd parity, 1/1.5/2 stop bits, and PCLK-clocked 16/8/4 oversampling.
- SPI1/2: source-backed reuse for blocking full-duplex master, 4–16-bit frames,
  either bit order, modes 0–3, optional delayed sampling, and GPIO-controlled
  chip select. Use a conservative **12 MHz F020 ceiling**, in addition to PCLK/2,
  while the contradictory 12/16 MHz source statements remain unresolved.
- I2C1/2: source-backed reuse for blocking seven-bit master, write/read,
  repeated START, adjacent-operation merging, ACK/NACK, clock stretching,
  bounded polling, and arbitration-loss reporting, up to 1 MHz subject to
  divider and board electrical constraints.
- CRC: only the **eight CRC16 presets 0–7** are independently documented for
  F020. Do **not** enable the existing unrestricted ten-mode API/default
  `Crc32` unchanged. CRC32/MPEG2 modes 8/9 and the upper result half are
  SDK/SVD-only claims contradicted by the manual and datasheet.
- DMA is out of scope and unsupported in these F020 HAL paths. The F020 DMA
  controller has only two channels and must not inherit an x030 DMA backend.

This is source verification, not silicon validation. No hardware was exercised.
The generated PAC may retain a source-layout alias without that alias becoming
an approved HAL capability.

## Pinned sources

Page numbers below are **printed document pages**. In both PDFs, add one to
obtain the one-based PDF page number.

| Source | Identity and integrity |
| --- | --- |
| [F020 user manual](https://www.whxy.com/uploads/files/20240920/CW32F020_UserManual_CN_V1.4.pdf) | `CW32F020_UserManual_CN_V1.4.pdf`, printed Rev 1.4; SHA-256 `279521eaf7d32b241e614a9553044a4d99c1e7ca2566efa3b0fdd2b66c106eed` |
| [F020 datasheet](https://www.whxy.com/uploads/files/20251230/CW32F020_DataSheet_CN_V1.3.pdf) | `CW32F020_DataSheet_CN_V1.3.pdf`, printed Rev 1.3; SHA-256 `1e330d800f10114c654b97cbd45b64d56a99c3cb39138d4ef20de3c12968dab0` |
| [F020 SDK](https://www.whxy.com/uploads/files/20240115/CW32F020_StandardPeripheralLib_V1.2.zip) | Standard Peripheral Library V1.2; archive SHA-256 `1d77fece47a0c615c8ae51374ea946b17ab489042222f33e38d93e6969945b5d` |
| SDK GPIO source | `Libraries/inc/cw32f020_gpio.h`; SHA-256 `eccc3bb68452d2e3218397b2a795c4330e4a3d9b5a76481de128655e4602874d` |

The older official 2025-08-21 download of the same filename actually identifies
itself as Rev 1.2 (SHA-256 `9fe3f5cf054612faf3b94de3e0f4166886e7b7ab2a43ebced009a48270aaca91`).
It has the same 116 serial AF cells on pp24–25 and the same CRC/SPI statements;
it is retained only as corroborating evidence. The current Rev 1.3 above is the
primary datasheet for this audit.

Relevant SDK operational sources are `Libraries/inc/cw32f020_{uart,spi,i2c,crc}.h`
and `Libraries/src/cw32f020_{uart,spi,i2c,crc}.c` in that pinned archive. Do not
substitute the F030 SDK as evidence for F020 behavior.

## Peripheral identity, gate and reset review

Manual §2 address map, §§4.7.12–4.7.17, pp79–84, and the peripheral register-list
sections establish the following:

| Instance | Base address | Clock gate | Active-low reset | Clock selected by existing driver |
| --- | --- | --- | --- | --- |
| UART1 | `0x40013800` | APBEN2 bit9 | APBRST2 bit9 | CR2.SOURCE=0 → PCLK |
| UART2 | `0x40004400` | APBEN1 bit7 | APBRST1 bit7 | CR2.SOURCE=0 → PCLK |
| UART3 | `0x40004800` | APBEN1 bit8 | APBRST1 bit8 | CR2.SOURCE=0 → PCLK |
| SPI1 | `0x40013000` | APBEN2 bit8 | APBRST2 bit8 | PCLK |
| SPI2 | `0x40003800` | APBEN1 bit6 | APBRST1 bit6 | PCLK |
| I2C1 | `0x40005400` | APBEN1 bit11 | APBRST1 bit11 | PCLK |
| I2C2 | `0x40005800` | APBEN1 bit12 | APBRST1 bit12 | PCLK |
| CRC | `0x40023000` | AHBEN bit2 | AHBRST bit2 | HCLK |

APBEN1 is at SYSCTRL+0x38, APBEN2 +0x34, APBRST1 +0x48, APBRST2 +0x44.
UART clock gates enable configuration access; CR2 selects its separate UCLK.
SPI/I2C gate bits enable both configuration and working clocks. Existing
readback-after-gate-enable and masked read-modify-write of reset bits preserve
unrelated peripherals. Reset assertion is zero, release is one, unlike many
other MCU families. These selectors must use the F020 SYSCTRL PAC even where
the serial peripheral block reuses `v1`.

Manual interrupt table pp35–36 and SDK definitions assign UART1/2/3 IRQ27/28/29,
SPI1/2 IRQ25/26, I2C1/2 IRQ23/24. Only UART needs these IRQ hooks for the current
HAL; blocking SPI/I2C should not imply async support.

## UART operational review

Manual chapter17, especially §17.3.1 p267; §17.3.3 pp270 onward; and
§§17.9.1–17.9.9 pp293–298 supports the existing operations:

- CR1.OVER=0/1/2 uses `UCLK/(16*BRRI+BRRF)`, `UCLK/(8*BRRI)` or
  `UCLK/(4*BRRI)`. BRRI is 16 bits and BRRF four bits. Nonzero BRRF forces
  16-times sampling, so clearing BRRF in the 8/4-times modes is essential and
  already done by `configure`.
- CR2.SOURCE=0 selects PCLK; zeroing CR2 also disables DMA, flow control,
  inversion, single-wire mode and address recognition. The driver must not
  claim low-power LSI/LSE reception just because the hardware can do it.
- PARITY=0/2/3 corresponds to eight payload bits with none/even/odd parity.
  The manual's phrase “nine-bit data length” with parity includes the parity
  bit; Table17-2 explicitly shows eight data bits plus parity.
- STOP=0/1/2 encodes 1/1.5/2 stop bits. Special oversampling and custom ninth-bit
  parity are outside the current API.
- ISR.TXE bit0 indicates the transmit buffer is empty. TC bit1 is set per
  completed frame. TXBUSY bit8 covers the buffer **or** shift register being
  nonempty, so flushing on TXBUSY=0 is correct.
- RX completion/FE/PE are bits2/3/4. ICR is write-zero-to-clear, reset `0xff`,
  with TC/RC/FE/PE/CTS as the clearable fields. Existing `0xff & !mask` writes
  preserve the reset values of reserved low bits and do not clear unrelated
  flags. Do not use read-modify-write on this flag-clear register.
- No overrun flag is documented. Unbuffered interrupt-driven receive still
  requires timely service; executor/IRQ delays can lose bytes without a
  separately reported overrun error. UART blocking waits have no timeout.
- Table17-1 requires digital AF TX output and digital AF RX input, recommending
  pull-up for idle-high RX. The driver configuration follows this requirement.

## SPI operational review and speed conflict

Manual chapter18, particularly §§18.3.2–18.3.4, §18.3.8, §18.3.9 and
§18.8 pp326–331, confirms:

- CR1 WIDTH encodes frame bits minus one, legal 4–16 bits; BR encodes
  PCLK/2, /4, /8, /16, /32, /64, /128, with encoding7 reserved.
- CPOL/CPHA, LSBF and SMP semantics match the backend. SMP=1 delays master
  input sampling by half an SCK period.
- MSTR=1 and SSM=1 make the internal CS configuration output-controlled by SSI.
  SSI=1 is appropriate while the HAL leaves the physical peripheral CS pad
  unconfigured and the caller manages a separate GPIO CS.
- RXNE is bit1, TXE bit0, error flags UD/OV/SSERR/MODF are bits4–7, BUSY bit8.
  Reading DR clears RXNE. The driver must receive/drain a frame even during a
  write-only application operation because the engine is full duplex.
- ICR is write-zero-to-clear. Bit0 is **FLUSH**, clearing both transmit and shift
  buffers; a zero write deliberately clears everything. Use only for controlled
  initialization/error recovery, not routine status acknowledgement.
- Waiting for BUSY=0 before successful return or width changes matches the
  documented buffer/shift behavior. Blocking waits are unbounded if hardware
  stops making progress.

The datasheet is internally inconsistent: the communications summary p4 says
12 Mbit/s, but §7.3.18 Table7-38 p57 says master fSCK≤16 MHz and slave≤10 MHz.
The same table's sample timing conditions use PCLK=48 MHz and divide-by-four
(12 MHz). §4.15 p17 and the manual describe PCLK/2 as an available divider,
which by itself is **not** an electrical-frequency guarantee. The initial F020
HAL should cap requests at 12 MHz, preserve the independent PCLK/2 bound, and
explicitly record that this is conservative policy pending source clarification.
Do not silently relabel the F030/A030 16 MHz evidence as F020 evidence.

## I2C operational review

Manual §19.4.2 p338 gives `fSCL=PCLK/(8*(BRR+1))` with **BRR=1..255**.
The existing ceiling-based division and minimum divisor two preserve that range;
BRR=0 must remain excluded even if the SDK accepts it. §19.4.3 p339 requires
simple filtering (FLT=1) when master BRR≤9, advanced filtering otherwise, matching
`Timing::control`.

Manual §§19.4.8–19.4.10 pp342–356 and §19.7.3 pp362–363 establish:

- SI bit3 is cleared by writing zero and advances the state machine. Registers
  and ACK decision must be set before that write. SI holds SCL low until
  software handles the state, enabling clock stretching.
- STA bit5 requests START/repeated START and must be cleared by software.
  STO bit4 requests STOP and is cleared by hardware when completed.
- AA bit2 selects ACK/NACK; the final read byte must be NACKed before STOP.
- The master's success states are START08, repeated START10, address-write18,
  data-write28, address-read40, read-ACK50 and read-NACK58 (hex). Address NACKs
  are20/48, data NACK30, arbitration loss38. The driver's treatment of
  arbitration-followed-by-own-slave-address states68/78/B0 is conservative.
- StateF8 alone does not mean the bus is idle; it also appears between phases.
  Poll SI before interpreting STAT. State00 is a bus fault; STO+SI-clear resets
  its state without putting a STOP on the wire. If that fails, EN0→EN1 followed
  by SI-clear is the documented fallback, matching the driver's bounded cleanup.
- Arbitration loss releases ownership to another master. Do not issue STOP,
  retry a transaction automatically, or ACK subsequent slave addressing.
- SCL/SDA use digital open-drain AF. External pull-ups and correct rise-time
  design remain board responsibilities; weak internal pulls are not substitutes.

Datasheet §4.13 p16 and manual §19.2 p332 support 100 kHz/400 kHz/1 MHz operation
and seven-bit addressing. This does not imply ten-bit, slave, DMA or async HAL
support. Poll counts are not wall-clock deadlines, and cleanup cannot recover a
slave physically holding SCL/SDA low.

## CRC mismatch and safe subset

Both the datasheet §4.4 p9 and user manual §§10.2–10.3.1 pp156–157 list only:

| MODE | Preset | Polynomial | Initial | Reflected input/output | Final XOR |
| --- | --- | --- | --- | --- | --- |
| 0 | CRC16_IBM | 0x8005 | 0x0000 | yes/yes | 0x0000 |
| 1 | CRC16_MAXIM | 0x8005 | 0x0000 | yes/yes | 0xffff |
| 2 | CRC16_USB | 0x8005 | 0xffff | yes/yes | 0xffff |
| 3 | CRC16_MODBUS | 0x8005 | 0xffff | yes/yes | 0x0000 |
| 4 | CRC16_CCITT | 0x1021 | 0x0000 | yes/yes | 0x0000 |
| 5 | CRC16_CCITT_FALSE | 0x1021 | 0xffff | no/no | 0x0000 |
| 6 | CRC16_X25 | 0x1021 | 0xffff | yes/yes | 0xffff |
| 7 | CRC16_XMODEM | 0x1021 | 0x0000 | no/no | 0x0000 |

Manual §10.6.1 p161 only defines MODE0–7. §10.6.3 on the same page reserves
RESULT bits31:16 and exposes only the low16 result. SDK `cw32f020_crc.h` lines70–71
nevertheless defines CRC32_DEFAULT=8 and CRC32_MPEG2=9, and the `.c` file provides
32-bit-result functions. The header include guard even retains `CW32F03x` naming.
The latter is corroborating evidence of reused source, not proof of silicon
behavior. The independent product/manual restrictions take precedence in the HAL.

Safe reuse requires an F020-specific capability gate removing modes8/9, a CRC16
default, and result reads limited to the CRC16 result. The three **input** widths
8/16/32 remain valid and must not be confused with a 32-bit CRC result:
§10.3.2 p158 permits all three and specifies low-byte-first order. §10.4.1 p159
states writing MODE initializes the accumulator for the selected algorithm, so
existing write-mode reset behavior remains appropriate. Its second example
32-bit constant has a byte-order typo; follow the textual low-byte-first rule.

## Independently verified AF routes

Datasheet Tables5-3–5-6, pp26–27, were inspected as rendered PDF pages27–28 and
compared to exact SDK macros, including numbered selectors. All **116** serial
cells match; there are **zero** conflicting, SDK-only or datasheet-only serial
routes. This result makes no claim about non-serial candidate routes.

The complete machine-readable per-route comparison is
[`f020-serial-af-evidence.json`](f020-serial-af-evidence.json). The consumable,
serial-only sidecar is [`cw32f020-serial.json`](../cw32-data/af/cw32f020-serial.yaml),
with standard signal spelling TXD→TX, RXD→RX, CS→NSS and original names retained.
Set the F020 input's `af_metadata` to `cw32-data/af/cw32f020-serial.yaml`.
The original all-peripheral candidate sidecar is deliberately left unpromoted.

All rows below have match status. Each `AFn:function` is an independently checked
cell, not an inferred function group. CTS/RTS/CS are metadata only for the current
HAL, whose supported signal subset comprises **82** routes before package
filtering (UART27, SPI25, I2C30).

| Pin | Matched numbered serial cells | Datasheet table/page |
| --- | --- | --- |
| PA0 | AF1:UART3CTS, AF2:UART2CTS, AF5:SPI2MISO | 5-3/p26 |
| PA1 | AF1:UART3RTS, AF2:UART2RTS, AF3:I2C2SCL, AF5:SPI2MOSI | 5-3/p26 |
| PA2 | AF1:UART3TXD, AF2:UART2TXD, AF3:I2C2SDA, AF5:SPI2SCK | 5-3/p26 |
| PA3 | AF1:UART3RXD, AF2:UART2RXD, AF5:SPI2CS | 5-3/p26 |
| PA4 | AF2:UART2CTS, AF3:I2C2SCL, AF5:SPI1CS | 5-3/p26 |
| PA5 | AF2:UART2RTS, AF3:I2C2SDA, AF5:SPI1SCK | 5-3/p26 |
| PA6 | AF2:UART2TXD, AF5:SPI1MISO | 5-3/p26 |
| PA7 | AF2:UART2RXD, AF5:SPI1MOSI | 5-3/p26 |
| PA8 | AF2:UART1TXD | 5-3/p26 |
| PA9 | AF1:UART3TXD, AF2:UART1RXD, AF3:I2C1SCL, AF5:SPI1CS | 5-3/p26 |
| PA10 | AF1:UART3RXD, AF2:UART1CTS, AF3:I2C1SDA, AF5:SPI1SCK | 5-3/p26 |
| PA11 | AF1:UART3CTS, AF2:UART1RTS, AF3:I2C2SCL, AF5:SPI1MISO | 5-3/p26 |
| PA12 | AF1:UART3RTS, AF3:I2C2SDA, AF5:SPI1MOSI | 5-3/p26 |
| PA13 | AF2:I2C1SDA, AF3:UART1RXD, AF4:UART2RXD, AF5:I2C2SCL | 5-3/p26 |
| PA14 | AF1:UART3TXD, AF2:I2C1SCL, AF3:UART1TXD, AF4:UART2TXD, AF5:I2C2SDA | 5-3/p26 |
| PA15 | AF1:UART3RXD, AF3:UART1RXD, AF4:UART2RXD, AF5:SPI1CS | 5-3/p26 |
| PB0 | AF1:UART2RXD, AF2:UART1CTS, AF3:I2C2SCL | 5-4/p27 |
| PB1 | AF1:UART2TXD, AF2:UART1RTS, AF3:I2C2SDA | 5-4/p27 |
| PB2 | AF1:UART2CTS, AF2:UART1TXD | 5-4/p27 |
| PB3 | AF1:UART3RTS, AF3:UART1CTS, AF4:UART2TXD, AF5:SPI1SCK | 5-4/p27 |
| PB4 | AF1:UART3CTS, AF3:UART1RTS, AF4:UART2CTS, AF5:SPI1MISO | 5-4/p27 |
| PB5 | AF4:UART2RTS, AF5:SPI1MOSI | 5-4/p27 |
| PB6 | AF1:UART3TXD, AF3:I2C1SCL, AF5:SPI2MOSI | 5-4/p27 |
| PB7 | AF1:UART3RXD, AF3:I2C1SDA, AF5:SPI2MISO | 5-4/p27 |
| PB8 | AF1:I2C1SCL, AF3:UART1TXD, AF5:SPI2SCK | 5-4/p27 |
| PB9 | AF1:I2C1SDA, AF3:UART1RXD, AF5:SPI2CS | 5-4/p27 |
| PB10 | AF1:UART2RTS, AF2:UART1RXD, AF3:I2C1SCL, AF4:I2C2SCL, AF5:SPI2SCK | 5-4/p27 |
| PB11 | AF3:I2C1SDA, AF4:I2C2SDA | 5-4/p27 |
| PB12 | AF4:SPI2CS, AF5:SPI1CS | 5-4/p27 |
| PB13 | AF3:I2C2SCL, AF4:SPI2SCK, AF5:SPI1SCK | 5-4/p27 |
| PB14 | AF3:I2C2SDA, AF4:SPI2MISO, AF5:SPI1MISO | 5-4/p27 |
| PB15 | AF4:SPI2MOSI, AF5:SPI1MOSI | 5-4/p27 |
| PC13 | AF3:UART1CTS | 5-5/p27 |
| PC14 | AF3:UART1RTS, AF5:SPI2MISO | 5-5/p27 |
| PC15 | AF5:SPI2MOSI | 5-5/p27 |
| PF0 | AF3:I2C1SDA, AF5:SPI2SCK | 5-6/p27 |
| PF1 | AF3:I2C1SCL, AF5:SPI2CS | 5-6/p27 |
| PF6 | AF1:UART3CTS, AF2:I2C1SCL, AF4:UART2CTS, AF5:I2C2SCL | 5-6/p27 |
| PF7 | AF1:UART3RTS, AF2:I2C1SDA, AF4:UART2RTS, AF5:I2C2SDA | 5-6/p27 |

### Package and pin-use constraints

Intersect die-level routes with the exact part/package's verified pin inventory
before generating trait implementations. The family alias must not gain pins
that are absent from its common-package intersection. AF evidence alone does not
prove package bonding. PA13/PA14 are SWDIO/SWCLK at reset; assigning serial AF
relinquishes the corresponding debug function. PC14/PC15 and PF0/PF1 may serve
oscillators, so a board must not use their serial routes while those oscillator
functions are required. Pin conflicts and electrical design remain caller/board
responsibilities.

### Reproduction and checks

Run from the repository root:

```sh
python tests/verify_f020_serial_af.py
python tests/verify_f020_serial_af.py --sources /path/to/cw32-sources
```

The source-free check compares the independently transcribed 116-cell audit,
source-candidate digest, route identities, GPIO AFR fields, SDK evidence, signal
normalization and scope. The source-backed check additionally hashes the pinned
original PDF, extracts its tables afresh with `pdftotext -layout`, compares every
serial cell, hashes the SDK header and verifies every macro line and selector.
Both checks passed against the stated sources. They perform no generation,
flashing or hardware test. AF tables are not taken from an unverified sibling
family, and a change in source hash/table shape fails closed for renewed review.

Before claiming an integrated F020 HAL release, separately regenerate/package
filter metadata, compile positive and negative typed API cases, and validate
hardware timing and transfers on an F020 board. This evidence review did not
run those later integration/hardware stages.

## Implemented F020 register correction

`cw32-data/registers/crc_cw32f020_v1.yaml` now separates F020 from the shared
CRC template. DR8/DR16/DR32 retain 8/16/32-bit input transactions and fields.
RESULT16 remains a 16-bit read alias. RESULT32 remains a 32-bit read transaction
at offset0x0c, but its sole valid result field is corrected to bits15:0; its name
is retained as a bus-access alias, not a CRC32 algorithm claim. MODE remains the
manual's four-bit physical field and explicitly documents only encodings0–7.
The raw PAC is not a semantic mode validator; the HAL must still hide unsupported
mode choices and default to a CRC16 preset.

The F020 input selects the dedicated variant and carries an evidence-bearing
`field_width_overrides` record. Candidate import requires the exact old block,
fieldset, field, offset and width before changing the field. Normal generation
only asserts that the curated YAML already contains the correction; it never
patches authored YAML. Empty evidence, invalid spans, incorrect old width/offset,
and unmatched corrections fail closed. The independent XML-to-generated-data
parity auditor has its own implementation of the same narrowly scoped exception.
The shared CRC variant and other families are unchanged.

The reuse ledger separates this corrected F020 template (135 canonical templates
at this audit point). Authored descriptions explain the silicon restriction;
they intentionally differ from the original SDK's generic descriptions.
Focused Rust generator regressions passed for the candidate/curated rules,
failure cases and actual F020 input/result widths. The metadata-contract suite
also checks the dedicated F020 result field and confirms the shared variant still
has its original 32-bit result. Generated-tree integration is performed by the
parent task after these source changes; it is not claimed by this audit alone.
