> Historical software-verification references: HAL tests and fixture harnesses cited below were deleted on 2026-10-08. Their reported results apply to the dated source snapshot only; source facts remain unchanged. See [current HAL layout and build scope](hal-production-layout.md).

# L010/L011 one-slot sequence ADC

Bounded implementation and source review, 2026-10-08. No silicon execution.
L012, multi-slot scanning, asynchronous conversions, DMA, trigger routing,
watchdogs, calibration and calibrated physical-unit results remain unimplemented.

## Actual hardware-version boundary

`adc/mod.rs` is an upstream-style version facade. Existing classic driver code
is moved unchanged into `adc/classic_driver/mod.rs`, apart from relative module
paths to its existing classic PAC adapter and tests. The classic public API,
reference controls, ownership and state machine are unchanged.

`adc_cw32l010_v1` and `adc_cw32l011_v1` select `adc/sequence/mod.rs`, with a separate
PAC adapter in `sequence/registers/mod.rs` and separate production-engine tests.
Build-time assertions require the actual selected ADC register metadata version;
there is no classic pointer cast or fabricated global hardware compatibility flag.

This version exposes `Adc<'d,T,Blocking>`, fallible and infallible blocking
constructors, `blocking_read`, `set_config`, `timing`, fixed 12-bit resolution,
sealed generated channels, and lifetime-preserving owned/borrowed channel erasure.
Sampling enumerates the actual 16 durations and only four PCLK dividers. Reference
has only `Vdda` (VDD on L010, VDDA on L011). Temperature and BGR channels return
raw counts; there is no VDD/3 source, input follower or selectable internal
reference. `SampleTime::Cycles390` is used only as constructor/configuration
initial timing; each read explicitly selects its own acquisition duration.

## Own primary sources

All source files are in the external evidence cache; raw SDKs are not copied into
this source tree. Public source URLs and pin-coordinate proofs are in
[low-adc-routes.md](low-adc-routes.md) and the two qualified analog sidecars.

| Document | SHA-256 |
| --- | --- |
| L010 RM CN1.2 | b66ae2b2837cf22aede7f19312b82659ea10f96960bfe7965de8733bb72513fa |
| L010 DS CN1.3 | 6fbefd334a86a1fafbec8ead5b6bd44dc99dfe564b604f69ec1edaa4ec789b86 |
| L011 current RM CN1.1 (2026-06-02 URL) | b245887e9037caf267f739e25bd3c354c2579d23315fc06389e055f371159f4f |
| L011 DS CN1.1 | 0b7414049824881920fc38f829029e3ba0af88feb4536fb27351df0d60f688a5 |
| L010 SDK1.0.9 archive | 84dbbeb8b684d0435ef9f926df8b899ceeb1d7bfd7767145b1e4f7e22210c716 |
| L011 SDK1.0.3 archive | 76adfe39360eb1d05ef58c25f26a8c1f99f2bc2f8fef677214aaca85cffc679e |
| L010 own `Libraries/src/cw32l010_vc.c` | ff2b8564a6ea965f036b00660541dffb68597c8db014ad3e4a28f4dbf7fd87a3 |
| L011 own `Libraries/src/cw32l011_vc.c` | df0128f5a7edd3d58bd842c283ccd464f47117f5d29c4164d70d398ab05b4618 |

## Electrical bounds and discrepancies

The board must supply a guaranteed minimum voltage, including supply tolerance,
not merely its nominal voltage. L010 accepts 1.62–5.5V; L011 accepts 1.7–5.5V.
The board independently ensures maximum supply, input voltage and source
impedance ratings. The enforced acquisition minimum is only a floor: high
external source impedance may need a longer `SampleTime` or an external buffer,
as established by the own-datasheet input-impedance tables. L011 DS table7-4 printedp36 requires **VDDA=VDD**; this driver
requires tied rails, despite looser separate-rail language in RM §3.1. Consequently
the VDD-based acquisition rows and VDDA-based conversion-clock rows use the same
guaranteed minimum. Separate-rail operation is not qualified.

Own RM table20-3: L010 printedp505; L011 printedp507. Own DS table7-27:
L010 printedp48; L011 printedp53.

| Voltage lower bound | L010 max ADC clock / max samples/s / min acquisition | L011 max ADC clock / max samples/s / min acquisition |
| --- | --- | --- |
| Family minimum | 4MHz / 200k / 1.25us | 6MHz / 200k / 1us |
| 1.8V | 12MHz / 500k / 0.75us | 12MHz / 500k / 0.5us |
| 2.8V | 24MHz / 1M / 0.375us | 24MHz / 1M / 0.25us |
| 3.3V | 48MHz / 2M / 0.1875us | 48MHz / 1M / 0.125us |

L011's own datasheet lists fADC minimum4MHz, typical48MHz, maximum96MHz.
The own manual limits the upper band to48MHz. The implementation conservatively
intersects them: **4MHz ≤ ADCCLK ≤ the own manual's voltage-band cap**. It rejects
unattainable requests rather than silently going below4MHz or adopting96MHz.
L010's corresponding datasheet does not specify a minimum clock; none is invented.

For every candidate divider1/2/4/8, the engine compares exact integer-rational
PCLK/divider against the requested maximum, maximum ADC clock, conversion rate
`ADCCLK/(sample+15)`, and minimum source acquisition. Acquisition is represented in
integer picoseconds so0.1875us is not rounded down. Whole-hertz timing display is
rounded only after validation. No APB doubling applies. Clocks are the HAL's
nominal PCLK/HCLK; oscillator tolerance and board clock tolerance are not modeled.

Sample encodings0..15 are6,7,9,12,18,24,30,42,54,70,102,134,166,198,262,390 cycles,
followed by15 comparison cycles (each own RM table20-2). TS14 and BGR15 each need
at least40us acquisition (L010 §20.12.3 p517; L011 §20.11.3 p518). Startup waits
cannot substitute for acquisition. At PCLK96MHz even divider8/sample390 cannot
satisfy40us; internal reads return a timing error until the board lowers PCLK.

RM EN startup is approximately1us. Each datasheet also lists `tSTAB=15/fADC`,
whose "stabilization" label is ambiguous alongside the manual's15-cycle comparison
stage. The implementation conservatively waits **max(2 nominal us,15 ADCCLK
cycles)** after EN; it does not claim to have resolved the source discrepancy.
TS/BGR startup is about30us in the manuals, while both datasheets give maximum
TS startup40us (L010 p50, L011 p54). First TS use and each BGR read wait50 nominal
us. BGR's manual value is approximate, not a guaranteed worst-case maximum;
analog accuracy/settling still require board/silicon validation.

## Protocol and shutdown

L010 §§20.12.1–11 pp516–523, L011 §§20.11.1–11 pp517–525 independently establish:
CR0x00 EN0/BGREN1/TSEN2/CONT3/CLK5:4/ENS8:6; START0x08; TRIGGER0x18;
AWDCR0x20; SAMPLE0x28; SQRCFR0x2c; RESULT0x40; IER0x74; ICR0x78; ISR0x7c.
No READY or overwrite status is present. RESULTS are right-aligned12-bit counts
in16-bit read-only fields. ISR is read-only. ICR low nibble is R1W0:
EOC0/EOS1/AWDL2/AWDH3, reset0x0f. Reserved upper bits remain0.

Initialization and reconfiguration stop START first, disable EN/TS while retaining
shared BGREN, disable trigger/interrupt/watchdog channels, configure only slot0,
drain stale RESULT0 and clear actual flags through ICR. EN is then set and analog
startup is delayed. CONT=0/ENS=0 are enforced on every CR write. Unused SAMPLE/SQRCFR slot fields are zeroed but not converted. No unused result,
threshold or reserved register is touched.

A read validates timing before ADC writes, stops/resets sequence position via
START=0, drains old result and clears old flags, writes slot0 mux/sample, programs
the real divider, settles newly enabled internal sources, and starts once.
Success requires both EOC and EOS plus hardware-cleared START within a finite
budget. It masks the result to12 bits and clears EOC/EOS with ICR=0x0c. Missing
flags, READY-looking unrelated bits and stuck START time out; the driver stops
conversion and shuts down EN/TS. A later read reinitializes. Configuration/timing
rejection preserves the previous ADC state. GPIO setup may already have occurred
before per-read timing rejection, matching the public documentation.

## Shared ADC/BGR/VC ownership

This is a real shared resource, not an optional precaution. Both own VC drivers
(`cw32l010_vc.c` lines266–272 and `cw32l011_vc.c` lines265–271) enable APBEN1.ADC
and ADC_CR.BGREN when comparator negative input is the1.2V bandgap. Own VC manuals
confirm that dependency (L010p528, L011p530).

Therefore this driver never asserts ADC reset, never clears BGREN once set, and
never disables an ADC gate that was enabled before construction. CR writes
atomically merge the live BGREN bit under a critical section, so an incoming or
mid-lifetime comparator owner sees no BGR-low transition. Drop/reconfigure/error
clear only conversion/temperature ownership. If BGREN remains enabled, the keyed
APB1 gate also remains enabled. APBEN1 writes use KEY=0x5a5a in31:16 and change
only bit0 in the lower half; APBRST1 is never written. VC gate/reset bit1 and all
other owners are preserved. No L012 shared-ADC protocol is inferred from this.

The deliberate cost is retained BGR power and potentially retained APB clock after
ADC drop, including when this ADC was the first BGR user. A future common analog
resource owner/refcount may reclaim these only after all dependents release them.
Raw users must not concurrently alter this driver's ADC conversion configuration
or disable BGREN. The classic backend retains its previous distinct ownership
contract and has not been silently changed.

## GPIO and route ownership

Every pin implementation comes from selected-package metadata with explicit
`adc_mux`; logical source labels are never parsed into mux values. Own PDF/SDK
proofs are detailed in [low-adc-routes.md](low-adc-routes.md). L010 package/alias
counts10/14/14/10 and L011 counts14/14/14 are independently asserted. In particular
L011 PA8..11 are mux10..13, not the L010 PB3..6 assignments.

Borrowed channels retain exclusive pin ownership. Their setup uses the existing
GPIO `Flex` path: enable/unlock port, disable digital input/output path, disable
weak pull-up/open-drain, retain output latch, and leave analog state after its
borrow ends. No digital AF is selected. Existing family GPIO RAM/model tests cover
neighbor preservation and disconnect. Debug/reset exclusions remain enforced.
Oscillator aliases are recorded; only the HAL's HSI clock path is exposed, and
raw oscillator users must not configure those pads concurrently.

## Verification scope

`ci/check-low-adc.sh` runs source/route verification, all seven host feature suites,
seven positive/negative Cortex-M0+ contract suites and five exact-package
`rt,defmt` release ELF links. Additional classic-regression/source-parity checks
and final counts are recorded in `verification-logs/low-adc/summary.json`.
Tests exercise the production state machine through a status/command model and
the actual selected PAC through aligned RAM. No analog accuracy, oscillator
margin, electrical safety, silicon timing or running-firmware validation is claimed.
