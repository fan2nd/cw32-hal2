> Historical software-verification references: HAL tests and fixture harnesses cited below were deleted on 2026-10-08. Their reported results apply to the dated source snapshot only; source facts remain unchanged. See [current HAL layout and build scope](hal-production-layout.md).

# Classic GTIM counters and simple PWM

Source/software qualification, 2026-10-08. No device has been flashed or run.

## Supported boundary

The existing `timer::low_level::Timer<T: BasicInstance>` and
`timer::simple_pwm::SimplePwm<T: GeneralInstance4Channel>` APIs now cover classic
GTIM on F002/F003, L031/R031/W031, L052 and L083, in addition to F030/A030/F020.
F002/F003 have exactly one `GTIM` token; no `GTIM1` alias is fabricated. L031,
R031 and W031 have two instances, L052 three and L083 four. BTIM keeps its
independent three-counter implementation and never gains PWM capability.

Public drivers own their Embassy `Peri` and typed pins. Four-channel handles
borrow the PWM owner: a live channel prevents changing frequency or dropping the
owner. The public shape, package projection, generated tokens and version-selected
private register adapters follow the existing Embassy-stm32-based organization;
CW32 timing differences are explicit rather than mapped to invented STM32 modes.

Only continuous edge-aligned up-counting from internal PCLK, polling overflow and
simple push-pull PWM are implemented. Capture, external counting, encoder,
master/slave, cascade, hardware trigger, interrupt/async, DMA and global time-driver
APIs remain absent. L010/L011/L012 buffered GTIM is a separate IP family with its own
[counter/PWM implementation](buffered-gtim.md).

## Exact hardware policies

Every family was checked against its own manual and SDK. The pinned own-source
inventory and register/gate facts are in `classic-gtim-evidence.json`. Existing
register identity is preserved: L083's normalized GTIM version is
`cw32l031_v1`, while L052 uses its actual `cw32l052_v1` including additional
unexposed capture/XOR controls. This does not substitute L031 manual evidence
for L083's own documented behavior.

The L031/R031/W031 manuals anchor GTIM1/2 at0x40000700/0x40001300 with
ARR offset0 and ICR offset0x1c. Their SDK headers use the same compact bases; the normalized PAC structures
begin0x300 earlier and include a reserved prefix; ARR0x300/ICR0x31c give the identical absolute
addresses. The source verifier checks this equality explicitly; no pointer
cast or invented address correction is needed.

- F002/F003 use CR0.PRS[10:7], divisor `2^PRS`; their adapter never accesses the
  absent DMA or PSC registers. x030/F020 retain the original PRS/DMA adapter.
- L031/R031/W031/L052/L083 write PSC at 0x334 with `divisor - 1`. Their CR0
  reserved PRS/PRSSTATUS positions remain zero. The public API deliberately
  preserves the common power-of-two subset 1…32768. Other linear divisors and
  65536 are not advertised. The largest exposed total divisor is 2^31, so the
  existing u32 denominator remains exact and does not overflow.
- All classic variants latch prescalers at overflow or an EN rising edge. No EGR
  or software-update register is invented. ARR/CCR changes are immediate.
  Reconfiguration stops, resets CNT before reducing ARR, sets the prescaler,
  clears OV and restores running state. A stopped timer stays stopped.
- PCLK has no APB timer multiplier. Frequency selection uses exact u64 comparison
  and ceiling division over the exposed subset, never returning a faster-than-
  requested rational frequency. Invalid requests leave registers and states
  unchanged. Period is 1…65536 ticks, including ARR=65535.
- CMMR8/9 force low/high; F gives high while CNT<CCR and E gives high while
  CNT>=CCR. Active-low swaps active/inactive levels. Zero/full duty always use
  forced modes, so full duty65536 is never truncated into a 16-bit CCR.
  Disabling retains requested duty and actively drives inactive polarity.
- ISR is read-only. Clearable ICR bits are0…6 and9 (mask0x27f), with R1W0
  semantics. Initial clear0x180 preserves reserved reset-one bits7/8; clearing
  OV writes0x3fe, preserving every other pending event. No ISR write/RMW is used.
- GTIM/GTIM1/2 gate/reset bits are APBEN1/APBRST1 bits1/1/2, and GTIM3/4 use
  APBEN2/APBRST2 bits10/11 where present. All are independently owned controls,
  reset-active-low. Critical-section RMW and readback preserve all neighbors.
  No shared BTIM or ADC gate/reset behavior is changed.

## Pins and electrical boundary

The new `*-pwm.json` sidecars are independent of unreviewed SDK-wide AF candidates.
Only own-PDF/SDK matching CH1…4 cells are qualified. Package bonding and family
alias intersections are applied before typed routes are generated. Debug, reset,
input-only, RF-reserved and oscillator-shared alternatives are withheld. Exact
source-cell counts and package projections are in `classic-pwm-route-evidence.json`.
F002's3, R031's7 and W031's4 SDK-only cells remain rejected. L052/L083 each withhold
8 documented oscillator alternatives until clock-pin ownership is designed.

The waveform counter rate is bounded by initialized PCLK and the family RCC's
source/electrical restrictions. A PCLK-valid frequency is not a pad-frequency,
current, load or edge-quality guarantee. Pins use the family's documented digital
AF, push-pull/no-pull configuration; the board must meet its own datasheet's VDD,
output-current, capacitive-load and rise/fall-time limits. No new voltage range,
external-load rating or calibrated oscillator accuracy is inferred from another
family. The library does not measure board voltage or output loading. The own-family
AC tables (F0027-23 p41; F0037-24 p42; L0317-27 p50; R0317-31 p57;
W0317-32 p56; L0527-26 p55; L0837-27 p59) independently list:

- VDDIO≥2.7 V, CL30 pF: fmax50 MHz, rise/fall maximum5 ns
- VDDIO≥2.7 V, CL50 pF: fmax30 MHz, rise/fall maximum8 ns
- 2.4 V≤VDDIO<2.7 V, CL50 pF: fmax20 MHz, rise/fall maximum12 ns

These tables explicitly use design/simulation data, not measurement. Their
frequency definition does not qualify arbitrary narrow PWM pulses. No output
speed guarantee is extrapolated below2.4 V, to another load or to arbitrary duty
cycles. These are application/board constraints; PCLK arithmetic alone cannot
check them. The verifier re-reads every own PDF's exact condition/value rows.

Outputs are inactive before AF connection. Frequency changes quiesce all outputs
and restart at zero; compare/polarity updates can truncate the current pulse.
Drop forces each polarity inactive, stops, disconnects owned pins, then gates its
independent timer clock. The code does not promise glitch-free transitions,
phase continuity, deterministic pad voltage after disconnect or safety-rated
motor/power-stage control. Such uses require independent protection and silicon
validation.

## Verification

`ci/check-classic-gtim-hal.sh` runs the own-source/register verifier, route
qualification, host production-engine/RAM-PAC tests, every qualified generic and
exact-package ARM type contract, and real exact-part ELF links with and without
`defmt`. Link inspection checks real vectors and FLASH/RAM bounds; no firmware is
executed. Host tests exercise actual production sequences, exact offsets,
no-DMA/reserved-PRS exclusions, both PRS and PSC EN-edge latches, all four compare fields,
endpoints, polarity, invalid inputs, stop/drop order and unrelated gate bits.
The RAM gate test proves final gate/reset values and unchanged neighbors; it does
not observe the intermediate active-low reset pulse. The two-write assertion/
release sequence is source-reviewed. No host model simulates pad edges or the
hardware overflow-latch transition.

The broader all-family host matrix covers BTIM/ADC/GPIO/serial regressions. Source
registers, vector identities and memory layouts are unchanged by route generation.
