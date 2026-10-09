> Historical software-verification references: HAL tests and fixture harnesses cited below were deleted on 2026-10-08. Their reported results apply to the dated source snapshot only; source facts remain unchanged. See [current HAL layout and build scope](hal-production-layout.md).

# Remaining SPI and classic I2C adapters

Implementation scope: blocking full-duplex SPI master on F002/F003,
L010/L011/L012, and blocking seven-bit state-code I2C master on F002/F003 and
L010/L011. This is a source-reviewed software implementation with host models
and selected-PAC RAM tests, **not on-hardware or electrical validation**. L012
command/FIFO I2C is a separate backend and is not covered here. There is no new
SPI/I2C async, DMA, slave, low-power wake, or multi-master scheduling API.

The original Embassy-style `Peri` ownership, blocking methods, embedded-hal 1.0
bus traits, SPI word wrappers, I2C transaction merging, error categories and
bounded I2C polling/recovery are preserved. SPI completion still drains every
received frame and waits for TXE with BUSY clear. As before, SPI has no timeout
API; a device/clock fault can leave a blocking call waiting indefinitely.

## Evidence and family boundaries

The source inventories, pinned documents, SDK versions and AF reviews are in:

- [F002/F003 audit](f002-f003-serial-read-only-audit.md).
- [L010/L011/L012 audit](read-only-low-power-serial-audit.md).
- [Existing L031 shared-controller audit](l031-shared-serial.md).

The original audits describe a preimplementation snapshot; their statements
that code was unchanged at audit time are historical, not claims that this
implementation is still absent.

This implementation additionally checked the original local source text and
SDK headers/functions for the decisive fields, independently of the PAC API:

- F002 UM CN V1.4 chapter 16 and F003 UM CN V2.3 chapter 17 retain the classic
  SPI offsets, CR1.EN, BR=0..6 and flag meanings. Both have a real singleton
  `SPI`, plus a real singleton `I2C`, rather than numbered peripheral tokens.
- L010 UM CN V1.2 §§4.7.12–16, pp79–83: APBEN1.SPI bit2 and APBEN2.I2C bit6;
  APBEN keys are 0x5a5a in bits31:16, APBRST is unkeyed and active-low.
- L010 UM §17.7.1 pp453–454: BR[6:4]=0..7 gives /2 through /256;
  CR1.SMP=1 delays approximately 20 ns. §17.7.2 places EN in CR2 bit0.
- L011's own SDK `Libraries/inc/cw32l011.h`, lines5731–5746 and 6009/6032,
  gives BR[6:4], CR2.EN bit0, I2C gate bit6 and SPI gate bit2. Its own
  `cw32l011_spi.h`, lines149–164, lists all eight /2..256 prescalers.
  `cw32l011_sysctrl.c:705–738` confirms keyed APBEN1/APBEN2 writes;
  `cw32l011_i2c.c:328–332` confirms active-low reset on APBRST2.
  Its newly acquired own UM CN V1.1 §§4.7.12–16 pp77–81 and §17.7 pp455–461
  independently corroborate these routes and register policies. SMP=1 is
  explicitly approximately 20 ns (p455), so it is no longer unquantified; it
  remains unsupported by the existing HalfPeriod API because those semantics
  differ. No production SPI change is required.
- L012 UM CN V1.4 §22.7.1 pp516–517: BR[30:24]=N gives
  PCLK/(2*(N+1)), N=0..127; bits6:4 are reserved; CR1 must only change with
  CR2.EN=0; SMP=1 is approximately 20 ns. Its own SDK `cw32l012.h`
  lines7972–8001 and 8253–8273 independently corroborate BR/EN and
  SPI1/2/3 gate bits2/13/14.
- L010 UM §§18.4/18.7 and L011's own UM V1.1 §§18.4.2/.3 pp468–469,
  §18.4.12 pp484–486 and §18.7 pp493–496 corroborate the classic BRR/SI/STAT
  algorithm alongside L011's own I2C SDK. The new input mux fields SCLINSRC[10:8]/SDAINSRC[13:11] select
  GPIO with zero; full control-word initialization and every subsequent engine
  control write keep them zero. Comparator routing is not exposed.

Register layout is selected by actual SPI IP-version predicates; physical clock
routes use actual chip-family predicates. No numeric-address alias is used to
pretend the low-power SPI block is the classic SPI block. The generated typed
PAC selects the real register offsets. No PAC layout patch was needed here.

## Deliberate hardware policies

| Family | SPI divider | Frequency ceiling | Clock route |
| --- | --- | --- | --- |
| F002/F003 | powers of two /2..128, BR=7 reserved | 12 MHz | unkeyed APBEN2/APBRST2 bit8 |
| L010/L011 | powers of two /2..256 | 24 MHz | keyed APBEN1 bit2; unkeyed APBRST1 bit2 |
| L012 | every even divider /2..256 | 24 MHz | keyed APBEN1 bits2/13/14; unkeyed APBRST1 same bits |

The 12 MHz F002/F003 limit uses the lower datasheet summary limit despite the
16 MHz electrical-table conflict. The new low-power 24 MHz cap remains separate
from the PCLK/2 bound. The old L031-derived PCLK/4 + 12 MHz policy is unchanged;
F030/A030 and F020 also retain their previous limits. Every solver compares
rational frequencies with widened arithmetic, rejects out-of-range requests,
and chooses the fastest supported divisor not exceeding the requested ceiling.
The result does not certify voltage, loading, rise/fall time or board timing.

L010/L011/L012 initialize CR2/CR3 explicitly, including zero ADC/DMA-trigger and
half-duplex controls, program CR1 only while CR2.EN=0, use GPIO-managed chip
select through the existing software SSI policy, and leave FLTEN/GAP disabled.
Register configuration and transfer width changes use the same separate enable
adapter. The flag/data offsets move to IER +0x10, ISR +0x14, ICR +0x18 and
DR +0x1c. All supported SPI ICRs are R1W0 with bit0 FLUSH; intentional whole-zero
clear remains restricted to disabled initialization/error recovery.

`SampleDelay::HalfPeriod` retains its existing meaning on older IP. New
low-power IP accepts only `SampleDelay::None`; requesting `HalfPeriod` returns
`ConfigError::UnsupportedSampleDelay` before MMIO. It is never translated into
20 ns. L010/L011 provide no `Pull::Down`; L012 only supports pull-down on PF3,
which has no serial route. Idle-low SCK therefore uses no weak pull on these
three families. L012 SPI also rejects MISO `Pull::Down` with
`ConfigError::UnsupportedInputPull` before MMIO. Idle-high SCK uses pull-up.

Classic I2C uses unkeyed APBEN1/APBRST1 bit11 for F002/F003 and keyed APBEN2 /
unkeyed APBRST2 bit6 for L010/L011. It retains the 1 MHz ceiling, BRR=1..255,
PCLK/(8*(BRR+1)) solver, and FLT=1 for BRR<=9. GPIO is open-drain on both lines;
external pull-ups and board rise-time design remain required.

All gate/reset updates are made under a critical section, preserve neighboring
bits, discard stale upper key bits before writing 0x5a5a on keyed gates, and
assert reset low then release high. A bounded 100,000-poll gate readback prevents
an infinite enable wait or access to a clock-gated block; failure panics, as in
the existing infallible GPIO gate policy. There is no new reset of a shared GPIO
port. Nonexistent second instances are rejected before a register write.

Pins remain selected from reviewed package-bonded routes. Drivers do not change
debug, reset, or oscillator remaps. Users must reconcile a chosen oscillator-pin
route with their board and RCC configuration. L012 SPI2/SPI3 share an IRQ, but
these blocking drivers neither enable nor change its NVIC state.

## Verification

`embassy-cw32/src/spi/tests/mod.rs` runs the real transfer engine through the
existing deterministic host model on every selected chip. The expanded
selected-PAC RAM checks cover every source-supported divider encoding, all data
widths/modes, changed offsets, CR2.EN, disabled CR3/trigger/filter controls,
source-supported frequency ceilings, /6 and /10 L012 rates, rational boundary
rounding, unsupported sampling/pull options, gate keys, preservation and missing
instance rejection. The original SPI transfer/error/idle tests are reused
unchanged except for family-accurate pull expectations.

`embassy-cw32/src/i2c/classic/tests/mod.rs` retains the real SI/STAT protocol
model for address/data NACK, repeated START, contiguous reads, arbitration,
stale bus errors, bounded waits/cleanup, and no implicit retry. RAM tests cover
the moved keyed gates and explicit zero GPIO-input mux initialization. RAM can
check final reset words and preservation, but does not simulate the hardware
reset pulse or peripheral side effects.

`tests/test_remaining_spi_i2c_contracts.py` contains independently hard-coded
source-reviewed routes across all 12 newly supported exact parts, verifies real
singleton names and all three L012 SPI instances (including AF9 SPI2 routes),
and checks blocking traits, word widths, reuse after drop, retained pin/peripheral
borrows, wrong signal/instance routes, absent instances and absence of fake
async constructors. These are Cortex-M0+ compile-only checks, not firmware runs.

### Implementation verification run (2026-10-08)

- New typed suite: all 12 exact parts passed their positive controls, with all
  112 targeted negative contracts failing for the expected diagnostic.
- Selected host suites passed for F030C8T7, F002F3P7, F003E4P7, L010Y8M6,
  L011K8T6, L012C8T6, L031C8T6, L052C8T6 and L083MCT6, including the expanded
  SPI/classic-I2C tests. Other workers were adding unrelated tests concurrently;
  counts are intentionally not frozen in this document.
- Original F030 SPI and I2C typed suites passed, including both instances,
  wrong-route/ownership contracts, and the I2C TSSOP20 bonding check.
- Cortex-M0+ L012C8T6 with `defmt` compiled. Broader package/feature matrix and
  independent final manual review are tracked by the integration task.
