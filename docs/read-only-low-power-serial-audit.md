# Read-only audit: remaining low-power serial backends

Audit date: 2026-10-08. **Preimplementation plan only.** Stage-4 driver,
metadata, generator, PAC and test sources were frozen. This audit changed only
this document and `read-only-low-power-serial-af-audit.json`. No build,
regeneration, test run, firmware execution, hardware validation or publication is
claimed. Existing stage-4 results remain separate from this future-work audit.

Scope: UART/SPI/I2C for L010/L011/L012, and UART for L052/L083. The current
`usart/mod.rs`, `spi/mod.rs`, `i2c/mod.rs`, GPIO backends, generated chip metadata,
canonical register YAML, each family's SDK and the available official manuals
and datasheets were read. L011 has its own SDK/datasheet evidence; **no L011
reference manual was available in the supplied source inventory**, and the L010
manual must not silently become its hardware authority.

## Executive findings and recommended order

1. L052 UART is the smallest extension: existing 031 UART transfer/baud logic
   matches, but select the L052 ICR policy by UART register version, not RCC cfg.
2. L010/L011 SPI and classic I2C can reuse protocol engines behind dedicated
   register/gate/GPIO adapters. They cannot reuse existing initialization code.
3. L010/L011/L012 UART can share ownership, byte-transfer and wakeup structure,
   but needs different masks, parity/word-length programming and error handling.
4. L083 UART has a **separate shared-NVIC lifecycle blocker**, even for a new
   blocking instance while its IRQ-sharing partner is async. See below.
5. L012 SPI reuses the full-duplex exchange engine with a new divider solver.
6. L012 I2C requires a **new command/FIFO engine**, timing solver and cancellation
   protocol. It is not an instance variant of the current SI/STAT machine.

No conclusive new PAC offset defect was established. Two L012 source conflicts
need explicit tracking: FIFO receive field widths, and I2C clock selector 3.
Neither is grounds to patch the PAC blindly. Details are below.

## Primary sources

Files are under `/workspace/shared/cw32-sources`; SDK paths below are relative to
the corresponding unpacked family directory. The documentary AF JSON pins the
three datasheet PDFs, candidate sidecars, GPIO SDK headers and pinout hashes.

- [L010 UM CN V1.2](https://www.whxy.com/uploads/files/20260626/CW32L010_UserManual_CN_V1.2.pdf):
  §§4.7.12–16, 16.3/16.8, 17.3/17.7, 18.4/18.7.
- [L010 SDK V1.0.9](https://www.whxy.com/uploads/files/20260806/CW32L010_StandardPeripheralLib_V1.0.9.zip),
  under `CW32L010_StandardPeripheralLib_V1.0.9/Libraries`.
- [L010 DS CN V1.3](https://www.whxy.com/uploads/files/20260416/CW32L010_DataSheet_CN_V1.3.pdf),
  Tables 5-2 to 5-4, §4.16, Table 7-35.
- [L011 SDK V1.0.3](https://www.whxy.com/uploads/files/20251016/CW32L011_StandardPeripheralLib_V1.0.3.zip),
  `Libraries/inc/cw32l011.h`, `cw32l011_uart.h`, `cw32l011_spi.h`,
  `cw32l011_i2c.h` and matching sources.
- [L011 DS CN V1.1](https://www.whxy.com/uploads/files/20250724/CW32L011_DataSheet_CN_V1.1.pdf),
  Tables 5-2 to 5-5, §4.16 and SPI characteristics.
- [L012 UM CN V1.4](https://www.whxy.com/uploads/files/20260603/CW32L012_UserManual_CN_V1.4.pdf):
  §§4.7, 21.3/21.9, 22.3/22.7, 23.4/23.8.
- [L012 SDK V1.0.5](https://www.whxy.com/uploads/files/20260701/CW32L012_StandardPeripheralLib_V1.0.5.zip),
  `Libraries/inc/cw32l012.h`, serial headers/sources; the I2C source is named
  **`cw32l012_lpi2c.c`**, not `cw32l012_i2c.c`.
- [L012 DS CN V1.0](https://www.whxy.com/uploads/files/20250717/CW32L012_DataSheet_CN_V1.0.pdf),
  Tables 5-2 to 5-6 and SPI/I2C features and electrical characteristics.
- [L052 UM CN V1.5](https://www.whxy.com/uploads/files/20240920/CW32L052_UserManual_CN_V1.5.pdf),
  §19.9, compared with its V1.4 SDK.
- [L083 UM CN V2.0](https://www.whxy.com/uploads/files/20240920/CW32L083_UserManual_CN_V2.0.pdf),
  §19.9, compared with its V2.2 SDK and generated IRQ metadata.

The L052/L083 manual URLs are also present in the supplied official manual
catalog HTML at lines636/831 and the existing source-evidence documents.

## UART: reusable engine and distinct register policies

All audited UARTs retain the basic offsets CR1 +0x00, CR2 +0x04, IER +0x08,
BRRI +0x0c, BRRF +0x10, ISR +0x1c, ICR +0x20, RDR +0x24 and TDR +0x28.
That resemblance does **not** make control words or status bits interchangeable.

| Policy | L010/L011/L012 | L052 | L083 |
|---|---|---|---|
| TXE / TC / RC | bits 0 / 1 / 2 | same | same |
| FE / PE | bits 8 / 9 | bits 3 / 4 | bits 3 / 4 |
| Noise / overrun | bits 10 / 11 | absent | absent |
| TXBUSY | bit 14 | bit 8 | bit 8 |
| RXIDLE / RXBRK / BAUD / TIMOV | 3 / 4 / 5 / 6 | no RXIDLE; 11 / 10 / 9 | absent |
| CTS / RXMATCH | 7 / 12 | CTS=6; no RXMATCH IRQ | CTS=6; no RXMATCH IRQ |
| ICR reset / clearable | 0x1fff / 0x1ffe | 0x0fff / 0x0e5e | 0x00ff / 0x005e |
| PCLK source | CR1[13:12]=0 (L010/L012 also document 1) | CR2[9:8]=0 | CR2[9:8]=0 |
| Parity | PARITY bit2, PARITYEN bit3, CHLEN bit6 | PARITY[3:2] | PARITY[3:2] |
| Extras to disable | CR3 RS485/LIN; CR2 timer, swap, ADC, loopback, local RX source | CR1 LINEN/BRKLEN; CR2 timer | no LIN/timer registers |

L010 and L012 UM ICR descriptions are §16.8.12 p428/PDF429 and §21.9.12
p489/PDF515. L052 §19.9.11 is p394/PDF395; L083 §19.9.9 p400/PDF401.
All are **R1W0**, not W1C. Reserved reset-one bits must remain one. Use an
explicit reset-preserving `reset & !(observed & clearable)` write; never generic
zero-to-clear-all on a live UART and never read-modify-write ICR. L052's generated
`RFU` bit-0 accessor is reserved, not a new event. Its `SORCE`, `LINEN`, `BRKLEN`
spellings differ from the 031 PAC but the corresponding fields remain distinct
source-backed evidence, not name-based compatibility.

### Parity and data length

For the public contract of eight data bits:

- None: CHLEN=0, PARITYEN=0, PARITY=0.
- Even: CHLEN=1, PARITYEN=1, PARITY=0.
- Odd: CHLEN=1, PARITYEN=1, PARITY=1.

CHLEN counts the parity bit. L010 §16.8.1 pp419–420 and L012 §21.9.1
pp480–481 explicitly say 8-bit CHLEN automatically clears PARITYEN.
L011 SDK `cw32l011_uart.c:164–187` and its parity constants corroborate this
family's own policy. The current `set_parity(2/3)` with CHLEN left zero would not
preserve the existing 8E/8O API. Reset CR3 and new CR2 features deliberately before
connecting pins; plain RX uses RXSRC=0, LOOP=0, SWAP=0. L012 alone adds CR2 DMA
bits6/7; these stay zero for the initial interrupt-per-byte port.

### Baud, errors and cancellation

The existing integer rounding/tolerance engine is reusable for OVER 0/1/2:
PCLK/(16*BRRI+BRRF), PCLK/(8*BRRI), PCLK/(4*BRRI), with 16-bit BRRI.
BRRF must be zero for OVER 1/2: L010/L012 manuals say nonzero BRRF forces 16x
sampling. Keep OVER=3 specialized low-speed sampling out of the basic API.
Use the frozen actual PCLK, not SYSCLK or an oscillator nominal rate. L012 SDK
names PCLK as SOURCE=1, while its manual permits both 0 and 1; this is not a
conflict for a deliberate SOURCE=0 PCLK implementation.

New RX error handling must observe NE and ORE and expose noise/overrun errors;
the existing two-error enum and claim that hardware has no overrun flag do not
apply. Overrun replaces unread RDR with newer data, so data loss must not be
reported as success. Read RDR when RC was observed, then clear only the observed
RC/error flags. Define and test priority when multiple errors arrive together.
L010/L012 NE is documented inactive at OVER=2/3; do not promise noise detection
in those modes. L052/L083 retain the older no-overrun-status limitation.

L010/L012 TC means both TDR and shift register are empty; L052/L083 TC is a
per-frame flag. Continue using TXBUSY for flush in all cases with each policy's
mask. Existing register-before-arm/recheck and one-shot RX/TX wakeup structure is
reusable, with new masks. Cancellation must mask only that direction's IER bits,
retain unread RDR and flags, never retain a caller-buffer pointer in the ISR,
and document partial receive/transmit progress. A cancelled write may already
have queued a byte; it does not imply a wire-level rollback.

## L083 shared IRQ: independent implementation blocker

SDK `cw32l083.h:93–95`, startup vectors and generated chip metadata agree:
UART1+UART4 share IRQ27; UART2+UART5 share IRQ28; UART3+UART6 share IRQ29.
A UART register-policy flag is insufficient to solve lifetime and IRQ binding.

Upstream-style plan:

1. Retain typed `InterruptHandler<UARTn>` and require `bind_interrupts!` fan-out
   for both typed handlers on the shared vector. For example the single
   UART1_UART4 vector must call the UART1 and UART4 handlers. Make the binding
   contract explicit for both async constructors; alternatively provide a typed
   group handler that guarantees both dispatches rather than allowing a partial
   binding to compile.
2. Each handler first checks a synchronized **instance-active** state. It must
   not read a partner's IER/ISR while that module is clock-gated. During register
   initialization the instance remains inactive; activate only after gate,
   reset and register setup have completed.
3. **Constructors, including blocking constructors, must not disable or unpend a
   shared vector while the partner may be live.** Replace current `prepare`'s
   unconditional disable/unpend. Configure the local IER=0 under the existing
   critical-section boundary, preserving partner pending/enable state.
4. Review one of two explicit lifetime policies: critical-section-protected
   group reference accounting with NVIC disabled only when the last active
   async owner is gone; or leave NVIC enabled after first use, with all inactive
   local IERs masked and inactive handlers guarded. The latter avoids shared
   unpend entirely. Do not combine either with the current unconditional
   `Info.disable_irq` callback on last-half drop.
5. Split TX/RX halves count as one active instance, not two independent UARTs.
   Final-half destruction first masks local IER, makes the instance inactive,
   then gates only its clock. Preserve the partner's flag, waker, gate and NVIC
   state throughout. Async future cancellation never changes group lifetime.
6. Tests must interleave simultaneous UART1/4 (then 2/5, 3/6), events arriving
   during arming, one partner not constructed, one dropped, blocking construction
   beside active async, split halves, all half-drop orders, and immediate reuse.
   Prove no access to an inactive clock-gated block and no lost partner wakeup.

L010/L011/L012 and L052 UARTs have individual IRQs 27/28/29 (L010 only 27/28).
The L012 **SPI2/SPI3** vector is also shared (`SPI23`, IRQ26); the same review
will be necessary before adding async SPI, although the existing SPI engine is
blocking-only.

## Clock gates, resets and instance names

The gate field also identifies the corresponding reset field in APBRSTn.
All resets below are active-low: clear the owned bit, then set it; preserve all
unrelated lines. L010/L011/L012 APBEN writes require 0x5a5a in bits31:16 and
preserved low bits; APBRST writes are unkeyed. L052/L083 APBEN/APBRST are
unkeyed. Use bounded gate readback before peripheral access, matching the
existing dedicated GPIO policy for keyed families.

| Family / PAC instance | Gate register / bit |
|---|---|
| L010 UART1/UART2 | APBEN1 / 3,4 |
| L011/L012 UART1/UART2/UART3 | APBEN1 / 3,4,8 |
| L010/L011 SPI | APBEN1 / 2 (PAC field is SPI1) |
| L012 SPI1/SPI2/SPI3 | APBEN1 / 2,13,14 |
| L010 I2C1; L011 I2C | APBEN2 / 6 (PAC field is I2C1) |
| L012 I2C1/I2C2 | APBEN2 / 6,11 |
| L052 UART1; UART2/UART3 | APBEN2 / 9; APBEN1 / 7,8 |
| L083 UART1/UART6 | APBEN2 / 9,1 |
| L083 UART2/UART3/UART4/UART5 | APBEN1 / 7,8,9,10 |

L010 UM §§4.7.12–16 pp79–83, L011 SDK UART init/deinit and SYSCTRL routines,
L012 own SYSCTRL register descriptions/header, and each L052/L083 SDK verify
these routes. Current UART RCC switches cover only three old-style instances;
current SPI/I2C RCC switches target different registers/bits and assume no key.
Do not accidentally reset GPIO ports shared by other drivers.

## SPI: common transfer algorithm, different hardware adapter

L010/L011 and L012 move CR2 to +0x04, add CR3 +0x08, move IER/ISR/ICR/DR to
+0x10/+0x14/+0x18/+0x1c; SSI remains +0x0c. EN is CR2 bit0, not CR1 bit6.
CR1: MSTR0, SSM1, CPHA2, CPOL3, LSBF7, WIDTH[11:8], GAP[15:12],
MODE[17:16], FLTEN18, MISOHD19, SMP20. HDOE is in CR3 bit0. Clear DMA/ADC
triggers and half-duplex controls rather than writing the old CR2 interpretation.
CR1 may be changed **only while EN=0**. The typed PAC already represents these
layouts; add a family-specific `enable` adapter instead of raw offset aliases.

ISR flag positions and semantics are reusable: TXE0, RXNE1, UD4, OV5, SSERR6,
MODF7, BUSY8. The current one-word full-duplex exchange, width encoding 4–16,
error mapping and drain-before-completion are reusable. ICR reset 0xff is R1W0;
bit0 FLUSH clears TX buffer and shift register. Whole zero is appropriate only
for intentional disabled-controller flush/reset. A future IRQ handler must not
clear with zero while sending; preserve FLUSH and unrelated flags.

Divider policies:

- L010/L011: CR1.BR[6:4], 0..7 -> PCLK/2, /4, /8, /16, /32, /64, /128, /256.
- L012: CR1.BR[30:24], n=0..127 -> PCLK/(2*(n+1)); /6, /10 and all even divisors
  are real options. Bits6:4 are reserved. Use ceil(PCLK/(2*requested))-1 with
  range/electrical-ceiling checks rather than the powers-of-two solver.
- Datasheets advertise 24 Mbit/s for each of these three families. Do not use
  PCLK=96 MHz to silently permit 48 MHz SPI. L010/L011 electrical tables give
  41.6 ns minimum master SCK period at PCLK=48 MHz. Apply a conservative 24 MHz
  ceiling and the board's voltage/load/timing restrictions; do not infer
  validated electrical performance from host calculations.

**SampleDelay is a semantic API blocker.** L010 §17.7.1 p453 and L012 §22.7.1
p516 specify SMP=1 as approximately 20 ns delayed sampling. The current
`SampleDelay::HalfPeriod` promises half an SCK period, which is true for the old
backends but not these. Add accurately named per-capability options, or initially
support only `None`. L011 SDK says delayed sampling without quantifying it; a
matching layout is not evidence for either 20 ns or half a period.

FLTEN should initially remain zero: L010 says use only fSCK<PCLK/8, whereas L012
says fSCK<=PCLK/16. Preserve GAP=0 unless the API deliberately exposes it.

## GPIO prerequisites and reviewed AF cells

Dedicated GPIO backends currently have no serial alternate hooks. Add them
without introducing LOCK/SPEED/PDR accesses unsupported by that family:

- L010/L011: three writable AF bits inside each nibble; retain reserved fourth
  bits and only AFRL for L010 GPIOB. Valid serial selectors are 1..7.
- L012: full four-bit nibble; valid documented selectors extend through **9**.
  SPI2/SPI3 and I2C2 rely heavily on AF8/AF9. Do not apply the L052/L083 AF<8
  restriction universally.
- UART TX / SPI SCK/MOSI are push-pull outputs; RX/MISO digital inputs; I2C
  uses open-drain bidirectional signals. Disconnect before changing AF/type,
  establish pulls, clear ANALOG and enable the output driver last.
- L010/L011 support no pull-down. L012 has pull-down **only on PF3**, and PF3
  has no serial route. Current SPI `sck_pull(CPOL=0)` requests Pull::Down:
  this would fail compilation for L010/L011 or panic for every L012 serial SCK.
  Select no weak pull for actively driven idle-low SCK on these variants; keep
  peripheral idle configuration before connecting the AF. Do not fake PDR.

The companion JSON has exact bounding boxes and SDK macro/line evidence for
**43 L010 + 70 L011 + 133 L012 = 246** serial pin/selector/function cells.
All match their own datasheets after explicit naming normalization. No actual
TX/RX, signal or selector contradiction was found in these serial cells.
L010 AF tables are printed p24/PDF25; L011 p28–29/PDF31–32; L012
p36–37/PDF39–40. Blank columns remain blank; all five pages were rendered and
visually inspected. These documentary counts are not a metadata promotion.

Necessary normalization is family-specific and uses the **actual PAC instance**:

- L010 SDK `SPI1...` and datasheet `SPI_...` -> PAC `SPI`; L010 I2C -> `I2C1`.
- L011 SDK `SPI1...` and datasheet `SPI_...` -> PAC `SPI`; I2C -> PAC `I2C`.
- L012 retains `SPI1/2/3`, `I2C1/2`, `UART1/2/3`.
- `TXD/RXD` -> typed `TX/RX`; SDK `NCS` and datasheet `CS` -> `NSS`.
- Candidate L010/L011 SPI already says peripheral=`SPI`, but has
  signal=`1SCK/1MISO/1MOSI/1NCS`; normalize the signal rather than inventing SPI1.

Safe serial-cell counts after package bonding and SWD exclusion:

| Exact part | All serial cells | TX/RX/SCK/MISO/MOSI/SCL/SDA |
|---|---:|---:|
| L010Y8M6 | 30 | 26 |
| L010F8P6/F8U6 | 38 each | 31 each |
| L011K8T6/K8U6 | 62 each | 49 each |
| L012C8T6/C8U6 | 125 each | 99 each |

Pin-special restrictions:

- L010 SWD is PA7/PA8, not PA13/PA14. PB7/NRST remains input-only after remap
  and has no serial route. PA0/PA1 are oscillator pins; resolve board and RCC
  use before serial connection. The 16-pin package lacks several 20-pin routes.
- L011 PA13/PA14 remain excluded SWD pads. PB2 and PC0–PC12 are not bonded
  GPIO. Oscillator pins are PC13/PB7; PB7 also has real UART/I2C routes.
  PC14/PC15 have real serial routes but still require board-function review.
- L012 PA13/PA14 are SWD; PF0/PF1 are oscillator pins. PF3/BOOT is bidirectional
  on this family (not the input-only PF3 rule from other families) but has no
  UART/SPI/I2C AF route. NRST is a dedicated input.
- AF ownership must not automatically change debug/reset/oscillator routing.
  Preserve exact-package joins and family-alias intersections.

L052/L083 UART routes are already part of the separately reviewed 675-cell
serial AF sidecars; keep their existing package/debug restrictions. L083 PA2 AF2
is UART6 TX, while L052 PA2 AF2 is UART2 TX. Do not reuse one family pin map just
because another family's UART register fields match.

## Classic I2C: L010/L011

The current CR/SI/STAT engine is reusable: offsets BRREN0, BRR4, CR8, DR0xc,
ADDR0=0x10, STAT=0x14 and alternate own addresses/match remain unchanged;
SI3 advances the state machine when written zero, AA2 controls ACK, STA5 must
be cleared by software and STO4 clears after completion. Error/status values
0x08/10/18/20/28/30/38/40/48/50/58/F8 retain their meanings. L010 §18.4.12
also documents 0x00 recovery via STO without a wire STOP, then EN reset if
needed, matching the current bounded abort structure. L011 own SDK state-machine
examples and driver corroborate the protocol; do not claim missing manual proof.

BRR is 1..255, fSCL=PCLK/(8*(BRR+1)); max advertised bus rate 1 MHz.
FLT=1 for BRR<=9, otherwise 0. The current conservative ceiling solver matches.
L010/L011 add CR.SCLINSRC[10:8], SDAINSRC[13:11]: 0 is GPIO, 1/2 are VC1/VC2.
Explicitly select GPIO before bus activity. If future APIs allow comparator
routes, carry mux values in the engine base control word through **every**
START/ACK/STOP/recovery write; current full-word writes otherwise erase them.

There is no per-event IER in this classic engine. A future async port would use
its dedicated NVIC line as the one-shot mask and preserve SI until the task has
read state/prepared data. Cancellation requires bounded owned-bus STOP/recovery;
a lost-arbitration path must never emit STOP into another master's transaction.
Current blocking-only engine is preferable for the first extension.

## L012 I2C: new engine and unresolved evidence

### Architecture and timing

The PAC exposes independent master/slave banks and a one-entry command/TX FIFO
and one-entry RX FIFO (UM §23.2.1 p524; SDK `FSL_FEATURE_I2C_FIFO_SIZEn(x)=1`).
Master control starts at +0x10, ISR +0x14, IER +0x18, MICR +0x0c; command/data
is MTDR +0x60 and receive data MRDR +0x70. INSEL +0x08 selects GPIO/comparator
inputs. Set both input selectors to zero, PINCFG=0 open-drain and leave the slave
bank disabled for a master-only port. The SCR2/MCR2 alias at +0x24 is **intentional**
(UM §23.8.19 p559); do not "repair" SCR2 to +0x120 or +0x124.

Use CLKSRC=0 frozen PCLK initially. MCCR has 6-bit CLKLO, CLKHI, SETHOLD and
DATAVD fields plus MCR2 PRESCALE=0..7. The timing equation is
( CLKHI + CLKLO + 2 + SCL_LATENCY ) * 2^PRESCALE / TCLK; the classic /8/BRR
formula is inapplicable. UM §23.4.3 pp534–535 has separate minimum low/high,
setup/hold, data-valid and rise/filter latency constraints, including CLKLO>=3,
CLKHI>=1, SETHOLD>=2, DATAVD>=1 and DATAVD<=CLKLO-SDA_LATENCY-1. Functional
clock after prescaling must be at least 8x bus speed. Program timings only while
MEN=0. Implement checked integer timing selection and an explicit rise/filter
assumption/configuration; a correct average frequency alone is insufficient.

### Protocol, flags, errors, cancellation

MTDR.CMD is 0=data, 1=receive DATA+1 bytes (1..256), 2=STOP, 3=receive/discard,
4=(repeated) START/address expecting ACK, 5=START expecting NACK. Use 0/1/2/4
for initial seven-bit master transactions. TXE means command FIFO available,
**not** address or byte acknowledged and not wire idle. MRDR.EMPTY and RXNE
are receive readiness; success needs STOP and completion/master-idle checks.

MICR is R1W0, reset **0x7f04**; clearable bits14:8 are MATCH, PINLOW, FIFO,
ARBI, NACK, STOP, PACKET. Preserve reserved bit2's reset-one state and avoid
ICR read-modify-write. TXE/RXNE are cleared by FIFO writes/reads, not MICR.
MCR0 FIFO-reset commands are R0W1, not normal retained enables. Reset TX FIFO
**before** clearing MICR.FIFO, per §23.8.16 p556. SDK error cleanup uses the
opposite order in one helper, so do not copy it as proof of safe recovery.

- ARBI releases mastership and blocks new START until cleared. Flush queued
  local work without putting STOP onto the winner's bus.
- NACK blocks a new START until cleared and may trigger an automatic STOP.
  Do not infer AddressNack versus DataNack merely from software enqueue position;
  prefer an unknown-source NACK unless completion evidence can distinguish it.
- PINLOW cannot clear while the low condition persists. Recovery must stay
  bounded and never claim the slave was forced to release the bus.
- FIFO protocol errors need the ordered reset/clear sequence and no implicit
  retransmission of the user's bytes.

Read commands NACK their last byte unless another receive command is already
queued. Adjacent read operations and >256-byte reads require prequeued continuation
or an explicitly bounded, preflighted API restriction; silently splitting into
independent read commands introduces an unwanted NACK. SDK MasterReceive rejects
>256 bytes with its one-entry FIFO. Test this boundary rather than blindly
copying its unreachable multi-command comment/loop.

**MEN=0 is not an immediate abort.** UM §23.4.4 p536 says it continues draining
queued commands until STOP, then automatically stops when TX FIFO is empty;
it no longer stalls for FIFO servicing. A cancel guard must mask MIER/MDER,
prevent stale commands or retained buffer access, distinguish owned bus versus
arbitration loss, perform ordered bounded cleanup and ensure the next transaction
cannot resume stale work. If asynchronous Drop cannot complete a bounded safe
bus cleanup, retain explicit recovery state and finish it before the next START.
Do not gate clocks while activity is merely assumed stopped. First implement
and model the blocking command engine, then add IRQ/waker cancellation.

### Unresolved source conflicts; no PAC patch in this audit

1. **Receive FIFO field widths.** UM §§23.8.9–10 p552/PDF578 prints RXWATER and
   RXCNT as bits27:16 (12 bits). SDK header `cw32l012.h:3027,3037,7290–7296`,
   SVD and current canonical YAML use bits17:16 (2 bits). The same manual says
   the FIFO has one entry, and the SDK explicitly sizes it as one. This makes a
   manual typo plausible but does not establish it. Keep current PAC widths
   under a documented open discrepancy; an initial driver can use watermark=0,
   RXNE and MRDR.EMPTY without depending on disputed upper bits. Do not advertise
   a deeper FIFO or widen fields absent evidence.
2. **I2C clock-source 3.** UM clock diagram and MCR0/SCR0 tables name HSI;
   `cw32l012_lpi2c.h:96,121` names LSI. Do not expose this choice or fabricate
   its frequency from the enum name. PCLK=0 is agreed and sufficient initially.
3. The MCCR table's `31:20 RFU` row overlaps its own documented DATAVD/SETHOLD
   fields; its detailed field rows, SDK and PAC agree on four six-bit timing
   fields. Record this editorial inconsistency, rather than deleting real fields.

## Suggested upstream-style code organization and acceptance work

Keep every future module under `module/mod.rs` as requested. Suggested private
boundaries are `usart/variant/mod.rs`, `spi/variant/mod.rs`,
`i2c/classic/mod.rs`, and `i2c/l012/mod.rs`, with public APIs reexported from
existing `usart/mod.rs`, `spi/mod.rs`, `i2c/mod.rs`. A variant boundary owns masks,
control encoding, gate/reset routes and IRQ lifetime capabilities; it should not
copy whole public drivers. The L012 I2C command engine is deliberately separate.

Use meaningful capability predicates (`uart`, `spi`, `i2c`), actual peripheral
version predicates (`uart_cw32l010_v1`, `uart_cw32l012_v1`,
`uart_cw32l052_v1`, `uart_cw32l083_v1`, `spi_cw32l010_v1`,
`spi_cw32l012_v1`, `i2c_cw32l012_v1`) and concrete `cw32*` family predicates
where gates/pin-selector limits truly differ. Do not hide compatibility behind
an unrelated RCC predicate. Keep per-family AF limits in reviewed metadata or
the owning GPIO backend rather than one universal selector test.

When the freeze is lifted, the staged acceptance sequence should be:

1. Promote only the 246 verified serial cells through evidence-bearing sidecars,
   explicit per-family naming and package joins; leave other candidate functions
   unpromoted. Add per-family AF selector limits and negative tests for 8/9 on
   L010/L011 and >9 on L012, plus wrong instance/direction/unbonded/SWD routes.
2. Add GPIO AF hooks with preservation/order tests, including L010 GPIOB's
   different block, unsupported pull-down and L012 AF8/9. Keep debug/oscillator
   remap untouched. Prove no write before invalid pull/selector rejection.
3. Add register adapters and test actual generated PAC against aligned RAM:
   every offset, reset/gate bit/key, adjacent-field preservation, ICR word,
   initial CR1/CR2/CR3, parity combinations and both SPI divider schemes.
4. Reuse deterministic engines with family-specific error/race tests. For UART,
   retain original engine tests and add RC+NE/ORE, error-only wakes, simultaneous
   errors, aux-flag preservation, cancellation and all oversampling edge cases.
5. Treat L083 group-NVIC tests as a release gate, not optional regression cases.
6. For L012 I2C, model FIFO depth/read-empty behavior, R1W0/R0W1 writes,
   command ordering, long/adjacent reads, NACK phase ambiguity, arbitration,
   stuck-low timeout, drain-after-MEN-clear, cancellation and reuse after error.
   Timing tests must enforce waveform constraints and range/overflow boundaries.
7. Then run all exact-part/family host and target compile matrices and the full
   existing regression suite against the final source. Compile-only success
   must not be described as silicon or electrical validation.
