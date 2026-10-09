> Historical software-verification references: HAL tests and fixture harnesses cited below were deleted on 2026-10-08. Their reported results apply to the dated source snapshot only; source facts remain unchanged. See [current HAL layout and build scope](hal-production-layout.md).

# CW32L012 blocking command/FIFO I2C

Implementation: `embassy-cw32/src/i2c/lpi2c/`, selected only by
`i2c_cw32l012_v1`. This is a separate command engine, not a register alias for
classic SI/STAT I2C. No async/DMA/slave/ten-bit support or silicon validation is
claimed. Source review, host modeling and target compilation are distinct gates.

## Source identity

- [CW32L012 UM CN V1.4](https://www.whxy.com/uploads/files/20260603/CW32L012_UserManual_CN_V1.4.pdf),
  SHA-256 `a9e54694a26f03c1f3e3041f40844900168ca2b8e6142f3328671b92e6b7a340`.
- [CW32L012 SDK V1.0.5](https://www.whxy.com/uploads/files/20260701/CW32L012_StandardPeripheralLib_V1.0.5.zip),
  SHA-256 `8b0a4c0ee865642d08f353aec98906c900a8b10f672120e649e6b1bf5e29c18f`.
  Relevant files: `Libraries/inc/cw32l012.h`, `cw32l012_lpi2c.h`,
  `Libraries/src/cw32l012_lpi2c.c`.
- [CW32L012 DS CN V1.0](https://www.whxy.com/uploads/files/20250717/CW32L012_DataSheet_CN_V1.0.pdf),
  SHA-256 `08a605fe8843d1eefa2b1115474b5a74df2833068f0ad6f35225e4f39377da76`.
- [NXP UM10204 Rev 7.0, Table 11](https://cache.nxp.com/docs/en/user-guide/UM10204.pdf?fsrch=1&pageNum=1&sr=3),
  printed p44, checked 2026-10-08. This supplies the I2C bus rise/data-valid
  limits and the stricter Standard/Fast/Fast+ bus minima, complementing the
  chip's own register equations and DS Table 7-38. Electrical compliance also
  depends on the board's fall times, voltage, load, pull-ups and noise behavior.

## Public contract and hooks

The dispatcher reexports `I2c<'d, M: Mode>`, `AnyI2c`, `Instance`, `SclPin`,
`SdaPin`, `Config`, `ConfigError`, `Error`, and embedded-hal `Operation`.
Constructors are available only on `I2c<'d, Blocking>` and accept typed `Peri`
instance/SCL/SDA tokens. It implements embedded-hal 1.0 seven-bit I2C only.
`pub(crate)` integration hooks are `Info`, `sealed`, `impl_instance!`,
`impl_pin!`, with the same signatures as the classic dispatcher.

`Config` keeps frequency, pin pull-ups and poll limit, and adds explicit
`scl_rise_time_ns`, `sda_rise_time_ns`, `scl_filter_cycles`,
`sda_filter_cycles`. Default rise bounds are **100 ns**, not a measurement.
Applications must increase them if their board is slower. Default filters are
zero; spike suppression is not promised when the digital filter is disabled.
No selectable clock source is public. Timing uses the frozen RCC PCLK envelope; nominal Hertz is only a reporting value.
See `docs/i2c-clock-bounds.md` for oscillator-endpoint validation and qualification.

A contiguous run of read operations must total **1..=256 bytes**. Adjacent read
buffers are scatter destinations for one receive command, so their boundaries
do not inject NACK or repeated START. Every run, address and empty-read check is
preflighted before even reading status, so a later oversized run cannot follow
partial writes. Separate read runs divided by a write may each contain 256
bytes. Empty writes retain their address phase; adjacent writes merge. A
transaction with no operations performs no MMIO.

## Source-to-implementation map

| Contract | Primary evidence | Implementation/verification |
|---|---|---|
| One-entry command/TX and RX FIFOs | UM §23.2.1 p524; SDK header line310 `FSL_FEATURE_I2C_FIFO_SIZEn(x)=1` | Model has one queued command and one receive byte; enqueue waits TXE; watermark zero |
| Commands 0=data, 1=receive count+1, 2=STOP, 4=START/address expecting ACK | UM §23.8.11 p552 | Exact command logs, address-only probes, repeated START and scatter-read tests |
| Last receive byte NACK unless a receive command is already queued | UM §23.4.1.4 p531; §23.4.4 p536 | Exactly one receive command per contiguous read; 256 allowed/257 preflight rejected |
| TXE means FIFO capacity, not ACK or wire completion | UM §§23.8.9, 23.8.15 pp552/555 | Model empties FIFO while address/STOP remain active; final success waits STOP + TXE + !MSTBUSY |
| RX readiness and atomic data/EMPTY observation | UM §23.8.12 p553; SDK source `LPI2C_MasterReceive` | RXNE sampled; MRDR.EMPTY checked before storing byte; no RXCNT read |
| NACK phase is not identified by the flag | UM §23.4.5 p541 | `Error::Nack` maps to embedded-hal `NoAcknowledge(Unknown)`; address/data faults produce identical error |
| Arbitration relinquishes ownership, blocks START until flag clear | UM §§23.4.2.2/23.4.5 pp533/541 | Highest-priority error; queued local work discarded; no cleanup STOP command |
| FIFO error requires TX reset before clear | UM §23.8.16 p556 | Ordered reset/clear model assertions; no implicit retry |
| MICR R1W0, reset 0x7f04 | UM §23.8.16 p556 | `0x7f04 & !(observed & 0x7f00)`; no RMW; reserved bit2 retained |
| FIFO resets R0W1 | UM §23.8.2 p547 | Command writes do not retain TXFIFORST/RXFIFORST; model checks readback |
| MEN=0 drains active work; no FIFO-service stalls; auto STOP | UM §23.4.4 p536 | Model retains active command after MEN clear and drains it; bounded cleanup below |
| RESET resets all master logic/registers except MCR0 | UM §23.4.1.1 p530; §23.8.2 p547 | Bounded held-reset fallback; reset readback and full reconfiguration before reuse |
| PINLOW cannot clear during continuing low condition | UM §23.4.5 p541 | Sticky-low model; no success claim or GPIO bus-unwedge pulses |
| PCLK source 0; GPIO INSEL; open-drain PINCFG=0 | UM §§23.8.1/2/4 pp546–549 | Complete initialization words verified through actual PAC against aligned RAM |
| I2C1/I2C2 APBEN2 bits6/11, key 0x5a5a; matching active-low APBRST2 | UM §§4.7.13/16 pp59/63; own SDK SYSCTRL definitions | Keyed gate readback bound, neighboring-bit preservation, owned reset and correct offsets |

No ACK decision is inferred from how far software got ahead in the command
FIFO. Error priority is ARBI, PINLOW, FIFO, NACK. Normal completion deliberately
does not require BUSBUSY=0 after STOP: another master may already own the bus.
Before a new transaction, BUSBUSY, MSTBUSY, TXE and physical SDA/SCL are checked.

## Timing solver and assumptions

UM Tables 23-1..3, printed pp534–535, define the four six-bit MCCR fields,
PRESCALE=0..7, the latency equations, minimum counts, filter ordering and bus
idle restrictions. `timing/mod.rs` uses u64 arithmetic and upward-rounded
required cycle counts. It searches all prescalers and low-count values, deriving
the smallest legal high count rather than rounding a baud-rate divisor down.

Rise times are upper bounds. Minimum latency assumes zero rise, so the fastest
possible rational frequency cannot exceed the request. Maximum latency includes
upward-rounded rise cycles and is used for internal safety inequalities.
Minimum timing and frequency constraints use the fastest qualified PCLK;
maximum data-valid time uses the slowest. The nominal whole-Hz accessor is not
a measurement or an upper bound. `get_current_frequency_bounds()` reports an
outward-rounded generated interval including oscillator and configured rise
bounds; stretching and FIFO service stalls can make the physical bus slower.
See `docs/i2c-clock-bounds.md` for the complete reporting contract. The
post-prescaler clock must be at least eight times the resulting bus rate. Timing registers are only written with MEN=0 after known idle/reset.

The minimum low/high/START-hold/START-setup, in ns, are respectively:

- Standard: 4700 / 4000 / 4000 / 4700
- Fast: 1300 / 600 / 625 / 600
- Fast+: 500 / 260 / 260 / 260

These are no less restrictive than either DS Table 7-38 or the bus-spec minima;
Fast START hold retains the chip datasheet's stricter 625 ns value. STOP uses
the same setup setting, at least as long as its requirement. The low-period
minimum also provides the required bus-free interval. Data setup minima are
250/100/50 ns; data-valid maxima are 3450/900/450 ns, conservatively budgeting
worst-case SDA rise within the maximum. Rise bounds may not exceed 1000/300/120
ns for the selected mode. DATAVD=1 is the shortest legal count, still subject to
its worst-latency upper bound. Filters are 0..15 raw PCLK cycles, with SDA>=SCL.
BUSIDLE=(CLKLO+SETHOLD+2)*2 and must exceed CLKHI+1.

This conservative solver can choose a slower rate or reject a configuration
that a vendor baud-only routine would accept. The UM's example 8 MHz/1 MHz row
is not copied blindly: its low/data-valid values conflict with a literal
application of the stated latency bound. No blanket 1 MHz-at-every-PCLK promise
is made. The tests check rational ceilings, every implemented waveform
inequality, field limits, source-clock/filter/rise boundaries and integer
extremes. These tests validate the algorithm against the equations, not the
physical device or analog signal assumptions.

## Bounded recovery and destruction

1. Disable MIER/MDER requests; there are no IRQ callbacks, DMA buffers or retained
   caller pointers.
2. Write TXFIFORST|RXFIFORST with MEN clear. This discards queued commands while
   an already active byte/receive command may still finish. The documented
   disabled-master behavior drains without waiting for FIFO service and emits
   STOP if it still owns the bus. No explicit STOP is enqueued during cleanup,
   including arbitration-loss cleanup.
3. Poll MSTBUSY clear and TXE set within `poll_limit`. Another master's BUSBUSY
   does not prevent our inactive-controller cleanup.
4. Once idle, flush again (active receive may have refilled RX) and clear only
   observed MICR flags. TX reset always precedes clearing FIFO. Re-enable an
   empty controller for later calls. A persistent PINLOW remains an error.
5. If idle never arrives, assert master RESET and retain it. Confirm RESET=1 and
   MEN=0 by bounded readback. Return the original transfer error; it cannot be
   represented as rollback of bytes already transmitted.
6. A later call first verifies the held reset and waits for both physical lines
   high, then restores the complete configuration before attempting START. A
   failed reset readback produces `RecoveryFailed` and no new command.
7. Drop gates the clock only after idle or held reset has actually been read
   back. If neither can be established, it leaves the clock on rather than
   claiming quiescence. Owned Flex pins disconnect on drop in either case.

No GPIO recovery pulses or automatic retransmission occur. Reset can stop local
logic but cannot make a slave or another master release the wires. Poll counts
bound each wait, not whole-transaction latency or wall-clock time.

## Deliberately unresolved source discrepancies

- UM pp552 RXWATER/RXCNT width is 12 bits; SDK/SVD/PAC say 2 bits. The driver
  writes the agreed value zero and uses RXNE/MRDR.EMPTY only. It does not patch
  the PAC, read the count, assume upper bits, or advertise deeper FIFOs.
- UM source3 says HSI, SDK says LSI. Neither source3 nor LSE is exposed; source0
  frozen PCLK is the only driver clock.
- The MCCR table's overlapping reserved row does not override its detailed four
  six-bit fields, corroborated by SDK and PAC.
- UM §23.4.4.2's prose mentions slave STAR.TXNACK during master receive and
  loosely describes the receive count. The master architecture/command table
  and own SDK receive routine instead agree on automatic final NACK and
  DATA=count-1. This driver follows those corroborated master semantics and
  never modifies STAR as a master workaround.
- SCR2 at +0x24 intentionally aliases MCR2. Initialization does not write SCR2
  separately and therefore cannot overwrite the prescaler.
- The SDK helper that clears FIFO before resetting it is not copied: the UM's
  explicit required ordering is followed.

## Validation commands and review gate

Run with the repository toolchain environment and `CARGO_INCREMENTAL=0`:

```sh
cargo test --offline -p embassy-cw32 --no-default-features \
  --features cw32l012c8t6 --lib i2c
python3 tests/test_l012_i2c_contracts.py
```

The dedicated ARM fixture covers both exact parts and the family alias, every
promoted I2C AF route including AF8, both instances, all blocking methods,
embedded-hal, reborrow-after-drop, and exact diagnostic failures for wrong
instance/direction, SWD, unbonded pins, retained ownership, async, ten-bit,
forged pin traits, guessed NACK phases and unverified oscillator selection.

Independent review must examine the literal UM timing equations and units,
zero-rise/maximum-rise bounds, receive command/NACK boundaries, late errors,
STOP completion, arbitration, MEN-drain/reset behavior, idle/reset readback
before gating, register/reset semantics and source conflicts. Host/compile
results do not remove the need for board-level electrical and silicon testing.

### Focused implementation verification, 2026-10-08

- 26 L012 command-I2C host tests passed with the normal generated capability
  cfgs. The 756-case timing grid matched exhaustive field search for both
  representability and fastest rational period: 108 Standard, 96 Fast and 96
  Fast+ configurations accepted; 456 rejected. All accepted cases satisfy the
  waveform assertions. An exact SDA-rise budget boundary test at 750 kHz PCLK
  accepts 783 ns and rejects 784 ns.
- Dedicated ARM contracts passed for `cw32l012`, `cw32l012c8t6` and
  `cw32l012c8u6`: 22 promoted AF routes and 12 exact negative diagnostics per
  selection. Both instances, public methods and ownership reuse were compiled.
- `cw32l012c8u6,rt,defmt` checked successfully for `thumbv6m-none-eabi`.
- The repository module-layout audit passed. These focused results do not claim
  an unrelated full-regression pass, final independent acceptance, device
  execution or measured electrical behavior.
