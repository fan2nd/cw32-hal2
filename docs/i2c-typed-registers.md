# I2C typed PAC register cleanup

Date: 2026-10-08. Scope: classic CW32 I2C and the separate CW32L012 command/FIFO
controller. Public API, ownership, central RCC control, qualified `ClockBounds`,
electrical limits, timing selection and transaction policy are unchanged.

The private raw-word `Registers` traits and generic register engines are removed.
Each existing transaction engine owns the selected `pac::i2c::I2c` directly.
Control snapshots and writes use generated register values and field methods;
status and command encodings belong to authored register YAML and generated PAC
values. No register-layout mirror, substitute bus engine, MMIO mock, generic
register write adapter or HAL test framework is added.

## Source authority and version boundaries

The machine-readable source manifest is
[`i2c-typed-register-evidence.json`](i2c-typed-register-evidence.json). It pins
each own-family manual and SDK header by SHA-256, source URL, manual section,
printed/PDF page and PDF text-extraction hash. Register YAML remains authoritative;
normal data and PAC generators produce both register metadata and typed accessors.

- `i2c_v1`: F030/A030 use the joint x030 CN V2.5 manual; F020 is separately checked
  against its own CN V1.4 manual.
- `i2c_cw32f002_v1`: F002 CN V1.4 and F003 CN V2.3 independently agree.
- `i2c_cw32l031_v1`: L031 CN V1.6, R031 CN V1.3, W031 CN V1.4, L052 CN V1.5 and
  L083 CN V2.0 are independently checked.
- `i2c_cw32l010_v1` and `i2c_cw32l011_v1`: own L010 CN V1.2 and L011 CN V1.1
  manuals both define additional three-bit `SCLINSRC`/`SDAINSRC`. All complete
  control writes retain zero, selecting GPIO. Those fields are not fabricated
  on versions that lack them. L011 evidence now includes its own supplied manual.
- All twelve classic families document the same 27 distinct eight-bit STAT
  codes. The generated `vals::Status` is chiptool's typed eight-bit newtype with
  named constants, preserving every unspecified bit pattern. No reserved code
  is given an invented meaning or normalized into another code.
- `i2c_cw32l012_v1` is not a classic alias. Own CN V1.4 section 23.8.11, printed
  p552/PDF p578, documents MTDR command encodings 0 through 5. Generated
  `vals::Command` names those six commands; 6 and 7 stay unspecified. The HAL
  still uses only Transmit=0, Receive=1, Stop=2 and StartExpectAck=4. No new
  receive-discard or expected-NACK API is exposed.

Embassy `f16efeffe37581092ec184718e6fdb1620393214`,
`embassy-stm32/src/i2c/{mod,v1}.rs`, supplies ownership and direct-PAC organization
references only. No STM32 register behavior, errata, or path attributes are copied.

## Exact classic register-word correspondence

The following words describe source equivalence, not values measured on hardware.
Let `F` be 1 when the selected BRR is at most 9, otherwise 0; `B = 0x40 | F`.
The complete typed CR baseline uses `Cr::default()`, `set_en(true)` and
`set_flt(BRR <= 9)`. `Default` still means the zero word.

| Operation | Old word | Typed construction and unchanged order |
| --- | --- | --- |
| Configure | CR=0, BRREN=0, BRR=divider, BRREN=1, CR=B | Zero CR; BRREN.EN false; BRR.BRR; BRREN.EN true; complete baseline CR |
| START/repeated START | B \| 0x20 | Copy baseline and set STA; write once with SI clear; wait Start/RepeatedStart |
| Address | DR=(address<<1)\|read, CR=B | Write DR.DR first; baseline CR then clears STA and SI; wait address ACK |
| Transmit byte | DR=byte, CR=B | DR.DR then complete baseline; wait WriteDataAck |
| Receive with ACK | B \| 0x04 | Copy baseline and set AA; one CR write clears SI; wait ReadDataAck then read DR.DR |
| Receive with NACK | B | AA false, SI false; wait ReadDataNack then read DR.DR |
| STOP | B \| 0x10 | Copy baseline and set STO; STA/AA/SI clear in the same write; bounded wait for !STO && !SI |
| Arbitration release | B | Complete baseline only; no new START or STOP |
| Reset fallback | B & !0x40, B \| 0x08, B | EN false; baseline with SI true while re-enabling; baseline clears SI last |
| Drop | CR=0, BRREN=0 | Zero CR then BRREN.EN false, then the existing central RCC disable |

No CR read-modify-write was introduced: clearing SI releases the controller and
must not happen before DR, START/STOP and ACK decisions are ready. Reads still
sample CR once before each decision. STAT reads happen at the same conditions,
and DR is read only after its expected receive event.

The former `status == 0 || status >= 0x60` ownership fallback uses
`Status::BusError` and the generated `Status::SlaveWriteAddress.to_bits()` lower
boundary. Unspecified upper codes retain the exact conservative behavior; only
the source-backed arbitration values 0x38, 0x68, 0x78 and 0xB0 get Arbitration.
STAT 0xF8 alone is still not an idle indication. Adjacent read buffers still ACK
across their boundary and NACK only the last byte of the complete read run.

## Exact L012 register-word correspondence

All ordinary registers continue to start at zero for the same deliberately
selected disabled/normal/PCLK/GPIO/open-drain modes. Zero writes use generated
values; nonzero fields use setters. No `Reg` API or `Default` implementation
changes were required.

| Register/action | Unchanged complete word |
| --- | --- |
| Initial MCR0, MIER, MDER, SCR0, SIER, SDER, INSEL, MCR1 | 0, in this order |
| MCR2 | PRESCALE; all other fields zero |
| MCR3 | BUSIDLE \| (FLTSCL<<16) \| (FLTSDA<<24) |
| MCR4 and MMATCH | 0 |
| MCCR | CLKLO \| (CLKHI<<8) \| (SETHOLD<<16) \| (DATAVD<<24) |
| MFIFOCR | 0 |
| FIFO reset/disabled drain | MCR0=0x300; MEN and RESET clear |
| Enable empty controller | MCR0=1 |
| Hold master reset | MCR0=2 |
| Transmit | MTDR=byte |
| Receive N bytes | MTDR=0x100 \| (N-1), N in 1..=256 |
| STOP | MTDR=0x200 |
| START/address | MTDR=0x400 \| (address<<1) \| read |

Timing's redundant raw-word packers are deleted; its already-bounded numeric
outputs are assigned to the corresponding PAC fields at configuration time.
The solver arithmetic and returned frequencies are unchanged.

### MICR command seed and reserved-bit policy

Own manual section 23.8.16, printed p556/PDF p582, gives MICR reset `0x7f04`,
R1W0 fields at bits 8..14, and reserved bits 7..0/31..15 to retain at default.
Therefore zero-default `Reg::write` would incorrectly clear every event and
write reserved bit 2 low. An RMW would also be inappropriate.

The existing `cw32-data/register-writes.yaml` mechanism now authors MICR's
`reset_value=write_noop=0x7f04`. The unchanged generators validate/project the
fact and emit `regs::Micr::write_noop()`. Each command starts with that generated
value, changes only intended fields, and calls the existing `write_value()`.
Zero `Default` is explicitly unchanged. There is no HAL-local seed/mask constant.

| Site | Former word, exactly preserved | PAC fields set false |
| --- | --- | --- |
| Configuration | 0x7f04 & !0x7f00 = 0x0004 | All seven command fields |
| Pre-START stale completion cleanup | 0x7f04 & !(status & 0x4300) | Only observed STOP, PACKET, MATCH |
| Successful final STOP cleanup | 0x7f04 & !(status & 0x0300) | Only observed STOP, PACKET |
| Quiesce after second FIFO reset | 0x7f04 & !(status & 0x7f00) | Each observed event, independently |

Unobserved events remain one (no action), reserved bit 2 remains one and all
other reserved bits remain zero. Configuration writes MCR0 FIFO resets before
MICR's FIFO clear. Recovery disables requests, resets FIFOs/clears MEN, polls
!MSTBUSY && TXE, resets FIFOs again, then clears observed flags. A timeout instead
holds RESET and confirms RESET && !MEN with the existing polling bound. Recovery
never queues an explicit STOP after arbitration loss. Later reuse still verifies
held reset and high lines before full reconfiguration. Drop gates the peripheral
only after observed idle or confirmed held reset.

The read engine still enqueues exactly one Receive command per adjacent read
run, giving the controller one final NACK. MRDR DATA and EMPTY still come from
one atomic register sample. Error priority stays arbitration, pin-low, FIFO,
NACK. Success still requires STOP && TXE && !MSTBUSY, allowing another master to
own BUSBUSY. No ACK result is inferred from FIFO enqueue progress.

### Deliberately unchanged L012 discrepancies

The RXWATER/RXCNT width disagreement remains unresolved; the driver writes only
the agreed zero watermark and does not read RXCNT. CLKSRC=3 remains disputed
(own manual HSI versus SDK LSI) and is not exposed or assigned a fabricated enum.
SCR2's address alias with MCR2 is not separately initialized. The SDK's
clear-before-FIFO-reset ordering is not copied. The manual's explicit reset-first
requirement is retained.

## Verification boundary

`tests/verify_i2c_typed_registers.py` independently opens all own manuals, verifies
hash-pinned pages/field meanings and checks generated enum/command metadata and
MICR's explicit seed. It is a source/PAC validator, not a HAL register model.
The existing I2C clock-envelope validator now checks the established central
`bus_clock_bounds::<T>()` call instead of its obsolete pre-central-RCC spelling.

Ordinary `cargo check --offline --locked -p embassy-cw32 --no-default-features
--features <family>,rt,defmt --target thumbv6m-none-eabi` passes for all 13 family
selections: F030, A030, F002, F003, F020, L010, L011, L012, L031, R031, W031,
L052 and L083. Logs are under `verification-logs/i2c-typed-registers/`. Source/PAC,
clock-envelope, generated parity, module-layout, data and PAC-access checks are
recorded there separately. There is no genuine existing I2C firmware example in
this checkout, so none was invented. The parent owns the final complete matrix.
No firmware was executed, flashed or pushed; wire/electrical behavior is not
claimed tested by these builds.
