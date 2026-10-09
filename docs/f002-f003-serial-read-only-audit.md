# F002/F003 serial implementation audit (read-only)

Status: **implementation plan only; not enabled or hardware validated**.
Audit date: 2026-10-08. Stage-4 code, generated data, tests and manifests were
left unchanged. This document is the only repository file authored by this audit.

## Result

The existing Embassy-shaped UART, SPI and I2C transfer engines can be reused for
CW32F002 and CW32F003, within their current mode boundaries. This conclusion is
based on the two families' own manuals and SDKs, not just equal SVD layouts.
Do not enable these families by changing only the top-level build predicate:

1. Add F002/F003 GPIO alternate-function hooks. The backend currently implements
   digital GPIO only and has no `alternate` or `alternate_with_type` methods.
2. Review and merge family-specific serial AF sidecars. Both current SDK
   candidate sidecars are intentionally unmerged; neither input manifest has
   `af_metadata`.
3. Generate actual `SPI` and `I2C` singleton names, rather than `SPI1` and
   `I2C1`. There are exactly two UARTs, one SPI and one I2C.
4. Compile out nonexistent UART3/SPI2/I2C2 gate/reset paths. Use the real
   SYSCTRL `SPI` field, not `SPI1`. Keep all unrelated/reserved bits unchanged.
5. Give SPI an absolute **12 MHz** ceiling and allow the documented **PCLK/2**
   divisor. The five-family L031 PCLK/4 restriction does not apply here.
6. Extend selected-PAC RAM tests, protocol tests and exact-package route/negative
   compile contracts before claiming implementation support.

Recommended first boundary: UART blocking plus existing per-byte IRQ async;
SPI blocking full-duplex master; I2C blocking seven-bit master. Keep DMA, UART
LSI/deep-sleep operation, SPI slave/async, and I2C async outside that first change.

## Primary sources and reproducibility

All source files below are in the separately acquired `cw32-sources` corpus.
Page numbers in this document are **printed** pages; PDF page number is printed
page + 1 for the cited pages. SDK paths are relative to each family's SDK root.

| Family | User manual | UART | SPI | I2C | Clock/reset | Datasheet AF |
| --- | --- | --- | --- | --- | --- | --- |
| F002 | CN V1.4 | ch15; register pp226-231; transmission/reception §§15.3.3.3-4 | ch16; feature p232; registers pp255-260 | ch17; BRR/filter pp267-268; state codes §17.4.10; registers pp290-294 | §§4.7.11-15, pp60-64 | CN V1.2, Tables 5-3/4/5, p23 |
| F003 | CN V2.3 | ch16; register pp285-290; transmission/reception §§16.3.3.3-4 | ch17; feature p291; registers pp314-319 | ch18; BRR/filter pp326-327; state codes §18.4.10; registers pp349-353 | §§4.7.11-15, pp62-66 | CN V1.9, Tables 5-3/4/5, p24 |

Source files and SHA-256:

| File | SHA-256 |
| --- | --- |
| CW32F002_UserManual_CN_V1.4.pdf | `e453b3f82d9d180a86cd5f7cb18d858d7f228afc8101d87c700ca9d5c02d5add` |
| CW32F003_UserManual_CN_V2.3.pdf | `0fa58dac223add7f2ac1ee714a7df7db4e414e0f193601b80dda96a948bfc738` |
| CW32F002_DataSheet_CN_V1.2.pdf | `6d0c5c37d069e5b4e33d394b0be6c53938868e9375e7b4bbbbdd40d62bb9d506` |
| CW32F003_DataSheet_CN_V1.9.pdf | `5fe15321b3963472c2629030767cf61a0ce36063815b54add74025b5b13b95dd` |
| CW32F002_StandardPeripheralLib_V1.2.zip | `108b6e1483669933e789c6868cf0a595ebba6bc4b77f78f5a45bb8bc96cbeffc` |
| CW32F003_StandardPeripheralLib_V1.7.zip | `fc2d753bfeaa0300d73b67f4c2d4f912cd065e6cb6d465935a1ad161bdd57333` |
| cw32f002/Libraries/inc/cw32f002.h | `399bd239233b4128480a3961f0885d29c2d40e1cede983a1343df958e6878d54` |
| cw32f003/Libraries/inc/cw32f003.h | `1f5a25d0c56de46e3333f98ad4135f0691da7d6fd7020b48cec83c5550a32a0f` |
| cw32f002/Libraries/inc/cw32f002_gpio.h | `12ab69693119a4fe4c731bb1545f5a9e6954f1e83ba6ed0c7da3d759a2d05b63` |
| cw32f003/Libraries/inc/cw32f003_gpio.h | `f4970bcdfb6da3403aa0d43e13ca1acfc82f3e3f19e2d2b557806ee456df5ce1` |

Verified public source locations already recorded in the local source catalog:
[F002 manual](https://www.whxy.com/uploads/files/20240920/CW32F002_UserManual_CN_V1.4.pdf),
[F002 datasheet](https://www.whxy.com/uploads/files/20251230/CW32F002_DataSheet_CN_V1.2.pdf),
[F003 datasheet](https://www.whxy.com/uploads/files/20251226/CW32F003_DataSheet_CN_V1.9.pdf),
[F002 SDK](https://www.whxy.com/uploads/files/20240115/CW32F002_StandardPeripheralLib_V1.2.zip),
[F003 SDK](https://www.whxy.com/uploads/files/20250606/CW32F003_StandardPeripheralLib_V1.7.zip).
No new online source retrieval was needed for this local-source audit.

Inspection covered each SDK's `Libraries/src/cw32f00x_uart.c`, `_spi.c`,
`_i2c.c`, their headers, generated `*_cw32f002_v1` peripheral/register modules,
selected chip PACs, the three current HAL `mod.rs` modules, GPIO backend,
build generation, AF candidates and package pinouts. Source text extraction was
cross-checked against rendered datasheet AF pages. An independent in-memory
PDF-coordinate extraction compared every serial AF cell to the SDK candidates
and joined the existing verified package pinouts. No code generator ran.

## Register compatibility and PAC correctness

Both input manifests already select `uart_cw32f002_v1`, `spi_cw32f002_v1` and
`i2c_cw32f002_v1`. Canonical YAML comparison, ignoring only descriptions, gives:

- UART versus `uart_v1`: same registers/offsets/field positions except that
  CR2 bits6/7 are reserved, not DMARX/DMATX.
- SPI versus `spi_v1`: same registers/offsets/field positions except that CR1
  bits16/17 are reserved, not DMARX/DMATX.
- I2C versus `i2c_v1`: identical register/field structure.

The two manuals' serial register-description sections were also compared after
normalizing section numbers and whitespace; the meaningful field semantics are
the same. The inspected generated PAC offsets, widths, read-only status/data
access and serial vector numbers agree with the manuals and SDK headers.
**No new serial PAC offset/width/vector defect was found in this audit.**
This is a scoped finding, not a correctness claim about unrelated peripherals.

PAC semantic caveat: generic `RW` and zero-valued register `Default` do not
encode R1W0, self-clearing or clear-and-advance behavior. For example,
`uart.icr().write(|v| v.set_tc(false))` starts from zero and would clear *all*
clearable events, not only TC. SPI ICR bit0 additionally flushes transmit data.
The existing HAL's explicit clear words handle this correctly. Preserve those
helpers; do not replace them with naive field writes. A future semantic metadata
improvement can document these effects independently of serial enablement.

All accessed MMIO registers use 32-bit words. Subword data widths are UART
TDR/RDR=9 bits, SPI DR=16 bits and I2C DR=8 bits.

| Peripheral | Offsets |
| --- | --- |
| UART | CR1 00, CR2 04, IER 08, BRRI 0C, BRRF 10, ISR 1C, ICR 20, RDR 24, TDR 28, ADDR 30, MASK 34 |
| SPI | CR1 00, IER 04, CR2 08, SSI 0C, ISR 10, ICR 14, DR 18 |
| I2C | BRREN 00, BRR 04, CR 08, DR 0C, ADDR0 10, STAT 14, ADDR1 20, ADDR2 24, MATCH 28 |

## Clocks, resets, keys and instance integration

| PAC singleton | Base | NVIC IRQ | Gate | Active-low reset |
| --- | --- | ---: | --- | --- |
| UART1 | 0x40013800 | 27 | APBEN2 (+34), bit9 | APBRST2 (+44), bit9 |
| UART2 | 0x40004400 | 28 | APBEN1 (+38), bit7 | APBRST1 (+48), bit7 |
| SPI | 0x40013000 | 25 | APBEN2 (+34), bit8 | APBRST2 (+44), bit8 |
| I2C | 0x40005400 | 23 | APBEN1 (+38), bit11 | APBRST1 (+48), bit11 |

Offsets in this table are hexadecimal relative to SYSCTRL. Gates are active
high and unkeyed. Reset writes must assert 0 and then release 1. Preserve the
rest of the word with critical-section masked RMW; use gate readback before
peripheral access. No SYSCTRL unlock, GPIO lock key or peripheral password is
needed. Do not copy clock-control keys into these gate/reset words.

UART's APB gate controls its configuration domain. Keep CR2.SOURCE=0, which
selects PCLK for UCLK on both families. The UART source table also documents
01=PCLK and 11=LSI; no new source selection is needed. SPI/I2C use PCLK directly.
Use `rcc::try_clocks().pclk`, not the nominal HSI frequency or HCLK. Current
F002/F003 RCC already freezes one PCLK domain; its default HSI output is 8 MHz.

Current UART `set_clock`/`reset` branches 0 and 1 are reusable. Branch 2 calls
`set_uart3`, unavailable on these SYSCTRL versions, and must be cfg-excluded
rather than relying on an unreachable runtime path. Current SPI helper calls
`set_spi1`; F002/F003 require `set_spi`. Its number-2 path must be cfg-excluded.
Current I2C bit11 path is reusable; bit12 must not be permitted. APBEN1 bit6
(SPI2), bit8 (UART3) and bit12 (I2C2) are reserved on these families.

`build.rs` currently iterates only SPI1/SPI2 and I2C1/I2C2. Add an explicitly
verified `(SPI, 1)` and `(I2C, 1)` selection for these families, retain their
actual peripheral and interrupt names, and assert all four serial register
versions before emitting capability cfgs. Do not rename their whole PACs to
match another family's naming. UART instance enumeration already checks
presence, but its compile/test references to UART3 still need family gating.

SDK pitfall: both `_i2c.c` files' `I2C_DeInit` (lines326-327) correctly pulse
0 then 1. Their `I2C_SoftwareResetCmd` (lines384-397) reverses the comments'
claimed reset polarity: the ENABLE branch writes 1 while calling that "under
reset." Follow the manuals and the correct DeInit sequence, not that helper.

## UART: reuse and boundaries

Reusable unchanged in principle: `Baud`, `calculate_baud`, `Access`,
`transmit`, `receive`, `on_interrupt`, `Wait`, owned `Uart`/split halves,
blocking methods and `embedded_io`/`embedded_io_async` adapters.

- Config supported by the current API remains 8 data bits, none/even/odd parity,
  one/1.5/two stop bits, asynchronous full duplex and separate RX/TX pins.
  PARITY encodings 0/2/3 and STOP encodings 0/1/2 match. Custom parity uses the
  ninth TDR/RDR bit but stays unexposed.
- OVER=0/1/2 means 16/8/4 samples. Baud=PCLK/(16*BRRI+BRRF),
  PCLK/(8*BRRI), or PCLK/(4*BRRI). BRRI is 16 bits; BRRF is 4 bits.
  Nonzero BRRF forces 16x sampling, so zero it for 8x/4x, as the current driver
  does. Keep nonzero integer-divider and rounding-error checks. SDK arithmetic
  is not a reason to replace the existing checked integer implementation.
- OVER=3 is special LSI sampling for 2400/4800/9600; it needs a separate clock,
  accuracy and low-power design. It is not part of existing `Config`.
- TXE bit0 reports an available transmit buffer; TDR writes clear it. TC bit1
  is raised after *each* frame, not necessarily the whole queued transmission.
  TXBUSY bit8 covers both transmit buffer and shifter. Flush must retain its
  TXBUSY test, using TC only as a wake event.
- RC bit2, FE bit3 and PE bit4 are the receive events. Reading RDR does not
  replace clearing RC through ICR. There is no overrun flag: an unread frame
  can be silently overwritten. Preserve error priority and the current
  unbuffered-receiver warning; source review is not a high-baud reliability test.
- IER permits TXE/TC/RC/FE/PE and CTS (bit6); it has no L031 TIMOV/BAUD/RXBRK.
  ICR reset is 0xFF and clearable mask is 0x5E. Use
  `0xFF & !(observed_mask & 0x5E)` to preserve unrelated events and reserved
  reset bits. Do not select the 0xFFF L031 clear helper.
- `configure` can retain CR2=0; comments should say reserved DMA bits remain
  zero on this IP. Only gates/reset selection and cfg wiring need changing.

IRQ async is feasible with the existing no-DMA design: type-correct UART1 or
UART2 binding, per-instance state, independent RX/TX wakers, register-before-arm
and recheck, one-shot IER masking, and no borrowed-buffer pointer in the ISR.
Keep split-half teardown so dropping one direction does not gate the other.
Cancellation may leave a transmitted or received prefix; RX status must remain
available for the next operation. RX throughput still depends on executor
latency. No DMA or deep-sleep communication is implied.

## SPI: reuse and boundaries

Reuse `Registers`, `Engine`, word wrappers, transfer/in-place loops,
configuration and `embedded_hal::spi::SpiBus` behavior after clock/name/limit
wiring. Public ownership and chip-select separation remain unchanged.

- Full-duplex master uses MODE=0, CR2.HDOE=0, MSTR=1, SSM=1 and SSI=1. GPIO
  device adapters own CS; no physical hardware-CS pin is configured.
- WIDTH=bits-1, valid data widths 4-16; CPHA/CPOL cover all four modes; LSBF
  and delayed SMP have the same meaning. CR1 bits16/17 are reserved and zero.
- BR=0..6 means /2, /4, /8, /16, /32, /64, /128; BR=7 is reserved. Both
  manuals explicitly list master PCLK/2 and slave PCLK/4. Set the family
  minimum prescaler bits to 0, not the L031 value 1.
- Datasheet feature summaries specify 12 Mbit/s SPI. Electrical tables allow
  16 MHz master (F002 Table7-33; F003 Table7-35), a
  conflict already familiar from other families. Use the conservative 12 MHz
  cap separately from the PCLK/2 cap. At PCLK=48 MHz, /2 is too fast and /4
  yields 12 MHz; at PCLK=8 MHz, /2 is legal and yields 4 MHz.
- TXE bit0 clears on DR write and sets when DR moves into the shifter. RXNE
  bit1 clears on DR read or ICR.RXNE=0. Always drain the received word even
  for writes. Successful completion requires TXE=1 and BUSY(bit8)=0.
- UD(bit4), OV(bit5), SSERR(bit6), MODF(bit7) match the current error enum.
  OV means unread RX data was overwritten. MODF clears EN in hardware; the
  engine's disable/clear/re-enable path preserves configured CR1 fields and
  returns an error without retry. Keep CS recovery with its external owner.
- ICR reset=0xFF; bits0-7 are R1W0. Bit0 FLUSH clears transmit buffer and
  shifter. Zeroing ICR is appropriate only for intentional disabled
  initialization/recovery, not a per-word status clear.

Do not expose async by merely implementing the trait around blocking loops.
An IRQ implementation would need per-instance wakers/state, event and error
mask ownership, cancellation cleanup, and final-idle handling. There is no
BUSY-falling interrupt: TXE can fire while the last word is still shifting.
A final bounded idle poll or a timed continuation must be deliberately designed
and tested. A master can naturally pause between words; no throughput or slave
latency guarantee follows. Current SPI polling is unbounded and should continue
to be documented as such unless a separately reviewed API changes it.

## I2C: reuse and boundaries

The current `Timing`, `Registers`, `Engine` and blocking `embedded_hal::i2c`
transaction implementation match both manuals. Only instance/cfg/GPIO/gate
integration is required for its existing feature boundary.

- BRR valid range is **1-255** and SCL=PCLK/[8*(BRR+1)]. Retain upward divisor
  rounding, the 1 MHz cap, the minimum divisor of 2, and rejection when even
  BRR=255 is too fast. BRREN.EN must be set for master operation.
- FLT=1 for BRR<=9; FLT=0 for BRR>9. These thresholds are explicitly stated in
  both families' input-filter sections. The SDK header's `IS_I2C_Baud_BRR`
  assertion lists only 1..7, despite the documented 1..255 register range;
  do not inherit that artificial seven-value restriction.
- CR positions: FLT0, AA2, SI3, STO4, STA5, EN6. SI is W0-to-clear-and-advance;
  W1 has no action. Set DR, AA and direction/START state before advancing SI.
  STA requires software clearing; STO clears when STOP finishes.
- Preserve the current START/repeated-START states 08/10, write-address
  ACK/NACK 18/20, data ACK/NACK 28/30, read-address ACK/NACK 40/48, and
  received ACK/NACK 50/58. Only the last byte of a contiguous read run is
  NACKed. Adjacent same-direction operation merging is reusable.
- F8 means no usable state, including between active phases; it is not proof
  that the bus is idle. Check lines and control state before starting.
- Arbitration states 38/68/78/B0 release mastership: report arbitration and
  never send STOP or retry. State00 bus error releases SDA/SCL; STO+SI=0
  clears its controller state without putting a STOP on the wires. If that
  fails, EN=0, EN=1, SI=0 is the documented fallback. Preserve bounded cleanup
  and partial-transfer semantics; do not automatically resend earlier bytes.
- AA remains clear outside active reads, and own-address/general-call state
  stays at reset. SDA/SCL need digital input plus open-drain output, with
  external pull-ups appropriate to actual voltage/capacitance/rise time.

Blocking construction must leave the NVIC interrupt masked; SI has no separate
peripheral IER. An async design cannot mask interrupts by clearing SI, because
that advances the protocol. It would need to mask NVIC/retain SI while waking
the task, then coordinate rearming without lost events. STOP completion can
clear STO without producing a new SI state, so a bounded timer/poll strategy is
also required. Define cancellation STOP/release behavior, arbitration ownership,
clock-stretch deadlines and post-cancel reuse before exposing an async trait.
Current poll counts are not elapsed-time deadlines.

## GPIO and AF evidence

F002/F003 use one AFRL at +0x18, with a **three-bit field in each four-bit
nibble**; the fourth bit is reserved. AF0 is GPIO; AF1-7 are valid selectors.
ANALOG is at +0x1C. There is no AFRH, SPEED or LOCK register. Reusing a F030
AFRH write at +0x1C would instead change analog configuration here.

Implement `alternate`/`alternate_with_type` in the existing
`gpio/f002_f003/mod.rs` using its `Ab`/`C` PAC dispatch. Validate AF1-7 before
MMIO; disconnect first; preserve other pins; configure pulls/open-drain and
clear ANALOG; update only `0x7 << (4*pin)` in AFRL; enable the required output
last. RX/MISO are inputs, TX/SCK/MOSI push-pull outputs, SDA/SCL open-drain
outputs with readback. Keep GPIO AHB gates at bits4/5/6. Do not write a lock
key to reserved +0x3C and do not introduce a speed option absent in hardware.

Preserve the existing safe-pad exclusions PA2/SWDIO, PA5/SWCLK and PC5/NRST.
SYSCTRL_CR2 controls debug/reset remapping and writing it locks RSTIO until
POR; ordinary serial constructors must not touch it. All first-stage buses
have complete non-debug route sets. For example, both 20-pin families support
UART1 PB1 TX + PA0 RX; UART2 PA1 TX + PA7 RX; SPI PA0 SCK + PA1 MOSI + PA4
MISO; I2C PB1 SCL + PA6 SDA. These are alternative constructor examples, not
simultaneously usable assignments: ownership prevents reusing the same pad.

### Exact serial table comparison

Both datasheet AF pages were rendered and visually reviewed. PDF-coordinate
extraction and SDK-sidecar comparison found:

| Family | Datasheet serial cells | SDK candidate serial cells | Missing SDK cells | SDK-only cells |
| --- | ---: | ---: | ---: | ---: |
| F002 | 42 | 52 | 0 | 10 |
| F003 | 52 | 52 | 0 | 0 |

No UART direction, SPI direction or I2C selector discrepancy was found on the
42 common documented cells. The **10 extra F002 SDK cells** are exactly:

| Pin | AF | SDK function | F003 status |
| --- | ---: | --- | --- |
| PA3 | 1 | UART2_TXD | documented, TSSOP24 only |
| PA3 | 2 | UART1_RXD | documented, TSSOP24 only |
| PB7 | 1 | UART2_RXD | documented, TSSOP24 only |
| PB7 | 2 | UART1_TXD | documented, TSSOP24 only |
| PB7 | 3 | SPI_SCK | documented, TSSOP24 only |
| PC3 | 1 | UART1_TXD | documented, TSSOP24 only |
| PC3 | 2 | SPI_CS | documented, TSSOP24 only |
| PC3 | 3 | SPI_MISO | documented, TSSOP24 only |
| PC4 | 1 | UART1_RXD | documented, TSSOP24 only |
| PC4 | 3 | SPI_MOSI | documented, TSSOP24 only |

F002's own manual Table8-2, p104, also omits these four pads. Exclude all ten
cells from its reviewed sidecar even though subsequent package filtering would
also remove them. Do not import the full F003 route set into F002.
For F003 they are legitimate only in TSSOP24, not QFN20/TSSOP20 or the
common-package-intersection family alias.

The common serial cells, expressed without blank-column inference:

| Pin | AF1 | AF2 | AF3 |
| --- | --- | --- | --- |
| PA0 | UART1_RXD | UART2_RTS | SPI_SCK |
| PA1 | UART2_TXD | - | SPI_MOSI |
| PA2 (debug) | UART1_RXD | UART2_TXD | I2C_SDA |
| PA4 | UART1_RXD | - | SPI_MISO |
| PA5 (debug) | UART1_TXD | UART2_RXD | I2C_SCL |
| PA6 | UART1_CTS | UART2_TXD | I2C_SDA |
| PA7 | UART1_RTS | UART2_RXD | - |
| PB0 | UART1_RXD | I2C_SDA | SPI_CS |
| PB1 | UART1_TXD | - | I2C_SCL |
| PB2 | UART1_TXD | UART2_CTS | SPI_CS |
| PB3 | UART2_RXD | I2C_SDA | - |
| PB4 | UART2_TXD | I2C_SCL | - |
| PB5 | UART1_RXD | I2C_SDA | - |
| PB6 | UART1_TXD | I2C_SCL | SPI_CS |
| PC0 | UART2_RXD | UART1_TXD | SPI_SCK |
| PC1 | UART2_TXD | - | SPI_MISO |
| PC2 | UART2_RXD | - | SPI_MOSI |

A dash here means no serial function in that cell, not necessarily a blank or
unimplemented nonserial AF. Normalize TXD/RXD to TX/RX and CS to NSS for the
existing metadata signal convention, but retain peripheral names **SPI/I2C**.
CTS/RTS/NSS can remain factual metadata while the initial HAL exposes only
TX/RX, SCK/MISO/MOSI and SCL/SDA.

Package joins after excluding PA2/PA5/PC5:

| Family/package | All safe serial cells | Current-driver signal cells | UART1 TX/RX | UART2 TX/RX | SPI data/clock | I2C |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| F002 QFN20 or TSSOP20 | 36 | 29 | 8 | 8 | 6 | 7 |
| F003 QFN20 or TSSOP20 | 36 | 29 | 8 | 8 | 6 | 7 |
| F003 TSSOP24 | 46 | 38 | 12 | 10 | 9 | 7 |

These are proposed verified-route counts, **not current generated HAL support**.

## Concrete next implementation and verification hooks

Keep public APIs in `usart/mod.rs`, `spi/mod.rs`, `i2c/mod.rs`. If a private
family module becomes useful, use `usart/f002_f003/mod.rs` (and equivalent
SPI/I2C paths), never new flat sibling `.rs` modules. Preserve `Peri`, sealed
instance/pin traits, typed interrupt bindings, split UART ownership, checked
configuration and embedded-hal/embedded-io behavior. Do not clone engines.

Suggested change order after the stage-4 freeze ends:

1. Add source-pinned F002/F003 serial AF audit and sidecars, preserving original
   candidates. Require exact PDF cell identity, source hash and SDK evidence;
   reject F002's ten SDK-only cells; wire only reviewed sidecars into manifests.
2. Add the two F002/F003 GPIO AF hooks and tests for 3-bit masks, analog/direction
   sequencing, open-drain readback, neighboring pins and untouched CR2/keys.
3. Add explicit family/IP capability selection, singleton names and absent
   instance cfgs. Retain 0xFF UART ICR and no DMA-register writes. Add SPI
   12 MHz/PCLK2 family constraints without changing other families' limits.
4. Extend existing `usart/tests/mod.rs` RAM tests and trait checks. Its current
   unconditional `instance::<UART3>()` must become availability-aware. Exercise
   both UART gate/reset locations, clear masks, every oversampling mode,
   split teardown, IRQ arrival races, flush with earlier TC, cancellation and
   no synthetic overrun detection.
5. Extend SPI/I2C RAM gate tests; their current second-instance exclusions cover
   only L031/R031/W031. Add F002/F003 to the absent-instance contracts and
   require rejection without touching APBEN1 bits6/12. Test SPI /2 at 8 MHz,
   12 MHz ceiling at 48 MHz, /128 minimum, all word widths and error recovery.
6. Reuse I2C modeled transactions including merged reads, repeated START,
   address/data NACK, arbitration, 00/F8 handling, stretched clocks,
   poll-limit boundaries, STOP completion and bounded cleanup.
7. Compile every exact package and both family aliases, all bonded signal
   traits and real constructors. Negative checks: F002 PA3/PB7/PC3/PC4,
   F003 20-pin extra pads, debug/reset pads, wrong AF/direction, UART3/SPI2/I2C2,
   token reuse, absent SPI/I2C async constructors, and nonexistent DMA support.
   Then run the complete existing regression matrix with the local toolchain.

No compilation or hardware tests were run for this read-only audit. Its checks
were source inspection, generated-model structural comparison, exact AF/PDF
comparison and package projection. Silicon loopback, logic-analyzer timing,
voltage/rise-time checks and sustained IRQ load tests remain separate work.
