> Historical software-verification references: HAL tests and fixture harnesses cited below were deleted on 2026-10-08. Their reported results apply to the dated source snapshot only; source facts remain unchanged. See [current HAL layout and build scope](hal-production-layout.md).

# ADC and GTIM next-batch audit

Source-level, read-only audit, 2026-10-08. The audit phase produced this report and
`adc-timer-next-batch-evidence.json` without changing implementation. A subsequent
separately authorized access-only correction is documented in
`timer-adc-isr-access-corrections.md` and `gpio-isr-access-corrections.md`; it
regenerated the PAC without enabling ADC/timer drivers or promoting routes.
No hardware validation is claimed.

## Recommendation

1. **Port F020 first:** the existing bounded 12-bit ADC protocol and power-of-two
   GTIM counter/PWM algorithm are directly reusable after selecting the actual
   F020 PAC, correcting its GTIM ISR access, and adding independently qualified
   package routes. The older `f020-backend-compatibility.md` statement that both
   blocks have different layouts is too broad: the currently exercised ADC
   subset matches, and GTIM's only IR difference is the ISR access annotation.
2. **Then F002/F003:** share the classic GTIM protocol with a backend that never
   touches nonexistent DMA. ADC timing is similar, but F002 has a restricted
   reference/internal-channel capability and both families have a different
   analog pin map. They cannot inherit the entire x030 ADC public surface.
3. **Then L031/R031/W031, followed by L052/L083:** share the classic ADC state
   machine through real-PAC adapters and a separate integer-prescaler GTIM
   timing policy. Respect the R031 source-label/mux distinction, L052/L083 ADC
   pin changes, and reserved ADC CR0 bit 4.
4. **L010/L011/L012 are a separate implementation:** ADC is one-to-eight-slot
   sequence hardware without READY/OVW, and GTIM is a buffered PSC/ARR/CCR design
   with EGR, CCMR and CCER. L012 further changes ADC offsets/bit encodings and
   shares clock/reset and BGR resources between two converters. L011 now has its own
   acquired current CN V1.1 manual; use that with its own SDK and datasheet,
   never promote L010 manual statements into L011 guarantees.

All non-x030 generated chip metadata currently has zero ADC and GTIM routes.
Changing a module cfg alone cannot provide safe analog pins or PWM outputs.
Raw peripheral presence does not imply an implemented safe HAL capability.

## Evidence and review boundary

The companion JSON contains 162 SHA-256-pinned primary source files, exact
13-family instance/register inventories, current template hashes, clock/reset
register offsets and bits, explicit analog mux candidates, all chip/package
projections, and independently extracted PWM AF cells. Official URLs are retained
where the existing source manifests supply a matching hash. Source hashes are an
audit snapshot, not a generated-state checksum contract after later corrections.

Primary manuals examined: F002 CN1.4, F003 CN2.3, F020 CN1.4, L010 CN1.2, L011 CN1.1 (current 2026-06-02 copy), L012
CN1.4, L031 CN1.6, R031 CN1.3, W031 CN1.4, L052 CN1.5, L083 CN2.0. The existing
F030/A030 baseline uses x030 CN2.5/EN1.0 and its prior compatibility proof.

F020's file named `CW32F020_DataSheet_CN_V1.3.pdf` at the source root is actually
the older printed Rev1.2 document (SHA256
`9fe3f5cf054612faf3b94de3e0f4166886e7b7ab2a43ebced009a48270aaca91`).
The current printed Rev1.3 copy is under `current-datasheets/` (SHA256
`1e330d800f10114c654b97cbd45b64d56a99c3cb39138d4ef20de3c12968dab0`).
AF conclusions below use that current copy, tables5-3–5-6, printed pp26–27,
PDF pages27–28. Both rendered AF pages were inspected. The R031 table 22-5
mux/name distinction was also inspected in the rendered own-manual page439.

For other remaining families, the original PDFs were parsed by actual numbered
column and pin-row coordinates using the existing independent AF parsers,
restricted in memory to GTIM CH1–4. No verifier file was edited. These results
are useful next-batch evidence, not already-promoted canonical AF sidecars.
L012's non-PWM `GTIM3_TRGO` row has a layout ambiguity in the generic parser;
it was not silently accepted. Its41 CH1–4 cells pass the narrower parser.

## ADC: directly reusable classic protocol

### F020

Own manual CN1.4, SHA256
`279521eaf7d32b241e614a9553044a4d99c1e7ca2566efa3b0fdd2b66c106eed`:

- §§21.4.1–4 pp373–375: 12-bit result; acquisition5/6/8/10 ADCCLK cycles plus19
  comparison cycles. ADCCLK is PCLK divided by 1/2/4/8/16/32/64/128. EN analog
  startup is approximately 40 us; wait for READY. No APB clock doubling.
- Table21-3 p374: internal1.5 V reference requires VDDA≥1.8 V, ADCCLK≤2 MHz below
  2 V and≤4 MHz at/above 2 V; internal2.5 V requires VDDA≥2.8 V and≤4 MHz. VDDA/ExRef
  limits are0.5/2/4/12/24 MHz across the 1.65/1.8/2.0/2.4/2.7 V lower bounds.
  Retain exact rational comparison before clock division. The board declares
  its guaranteed minimum VDDA, not merely a nominal voltage.
- §21.4.3 and§21.13.1 p398: internal channels or high-impedance signals require
  BUF; BUF limits throughput to200 kSPS. Source settling and acquisition are
  separate constraints. The current datasheet TS characteristics require
  startup≤45 us and acquisition≥5 us. Keep conservative nominal waits, and do not
  advertise calibrated temperature or oscillator-tolerance guarantees.
- Table21-5 p377: PA0–7→mux 0–7, PB0/1/2→8/9/10, PB10/11→11/12;
  VDDA/3=13, temperature=14, nominal1.2 V BGR=15.
- §§21.13.1–2 pp398–400: CR0 EN0, BGREN4, TSEN5, REF7:6, CLK10:8,
  SAM12:11, BUF13; CR1 mux 3:0, DISCARD5, ALIGN6, DMAEN7. BIAS15:14 remains
  at its documented default; no invented calibration operation.
- §§21.13.8–12 pp404–406: START@0x08 bit 0 starts/stops conversion; with MODE0
  it clears on completion. EOC0 and OVW6 are read-only ISR flags at 0x3c;
  READY7 is read-only and has no ICR clear bit. ICR@0x38 is R1W0 bits 0–6.
  RESULT0@0x20 is read-only. Poll boundedly for EOC and stopped START, read the
  right-aligned12-bit result, clear EOC through ICR, and stop/power down on error.

The current x030 engine already follows this bounded protocol. It can be shared
through the real selected F020 PAC: reset/enable is APBEN2/APBRST2 bit 2; their
SYSCTRL offsets are0x34/0x44. Reset is active low. Preserve unrelated gates and
use readback barriers. F020's ADC base is 0x40012400.

The F020 SDK/SVD additionally contains RESULT140–143@0x40–0x4c and ENABLE14@0x88.
They are not evidence for exposing a 14-bit HAL resolution: the examined manual
and current datasheet describe the 12-bit path. Keep the imported raw registers;
exclude the extension from the HAL until independently documented. TRIGGER
bit 0 is not ATIM on F020; do not import x030 trigger routes or invent ATIM.
External-reference operation also stays excluded unless ExRef pin ownership is
explicitly designed. DMA and scan/accumulation modes stay disabled.

### F002 and F003

Own F002 RM1.4 §§19.4–19.5.1 pp302–307 and§19.12.1; F003 RM2.3
§§20.4–20.5.1 pp362–367. Acquisition, comparison cycles, READY/start/abort and
R1W0 event handling support the same bounded classic state machine. Both lack
the x030 ADC DMAEN field and timer DMA register; adapters must not touch them.

F002 is an important counterexample to register-name reuse. Its own CR0 reference
field documents only ExRef=2 and VDD=3; bits 5:4 are reserved. The converter offers
13 external channels plus VDD/3 at 13. Do not expose Temperature, VrefInt,
Internal1V5 or Internal2V5. Its SDK/SVD still contains TSEN/BGREN and temperature
trim symbols, and its BUF prose mentions unavailable internal sources. The
restricted feature list, reference table and channel table take precedence for
safe HAL capability; do not delete raw fields merely because they are copied.

F003 does support the internal1.5/2.5 V references and channels13/14/15, subject
to its own electrical limits and settling. Its factory temperature trim
addresses are0x001007C5/C6/C8, unlike F020/x030's0x00012609/0A/0C. Neither family
has a documented software ADC calibration command in the examined register
interface. Return raw temperature counts initially; a factory-trim utility is
separate future work and must validate erased/invalid trim values.

Both families' external mux 0–12 map is:
PB2, PA1, PA4, PA6, PA7, PC0, PC1, PC2, PB0, PB1, PB6, PB5, PB3.
They are not PA0–7 plus PB0/1/2/10/11. The single timer token is GTIM, not GTIM1.

### L031/R031/W031 and L052/L083

The classic timing/status protocol remains a reusable algorithm, not a PAC
layout alias. The 8-result layout moves RESULTACC to0x40 and IER/ICR/ISR to
0x44/0x48/0x4c; SQR0/1 are0x50/0x54. CR0 bit 4 is reserved, so the current engine's
BGREN writes are forbidden. CR1 DMA request bits are14/15, not bit 7. A full
zeroed initial CR1 is suitable for the single-shot scope, but any later adapter
must use the real bit positions. Keep shared analog/BGR ownership constraints
explicit; no driver should disable a shared analog source another driver owns.

Own references: L031 RM1.6 §§22.4–5 pp432–436 and§22.13.1 p458; R031 RM1.3
§22.5.1 p439/§22.13.2 p462; W031 RM1.4 §22.5.1 p439; L052 RM1.5
§§23.4–5 pp469–473; L083 RM2.0 §§23.4–5 pp474–478. Each has PCLK/1–128,
5/6/8/10+19-cycle timing, READY startup, single-shot START completion, OVW and
R1W0 ICR. The cited clock-limit tables have the classic voltage/reference bands;
validate actual family/package voltage requirements too.

- L031/W031 use the x030 external mux map, subject to own package/radio bonding.
- R031 uses source labels AIN0–8 / ADC_IN0–8 for PA4–7, PB0/1/2/10/11, but the
  hardware mux values are4–12. Both own RM tables and own SDK adc.h193–219
  explicitly agree. Datasheet prose calls TS ADC_IN10, while hardware TS mux is 14.
  Store source signal identity and hardware mux separately. The current
  build.rs `strip_prefix("IN").parse()` must not be generalized to R031.
- L052/L083 mux 0–7 are PA0–7; mux 8/9 are PC4/5; mux 10/11/12 are PB0/1/2.
  Do not substitute L031 routes. Internal mux 13/14/15 retain their roles.
- L052 adds ALTR@0x58 and common RESULT@0x5c plus another conversion mode. These
  are outside the initial single-shot RESULT0 path and must remain unexposed.
- SDK factory TS trims for this group are0x00100A09/0A/0C, not x030 addresses.
  Raw internal channels are a bounded first step; calibrated engineering units
  need a separate own-datasheet accuracy/trim review.

### L010/L011/L012: separate single-slot sequence engine

L010 own RM1.2 ch20, especially§§20.4–5 pp503–506 and§§20.12.1–4 pp516–518:

- 12-bit, VDD reference only; PCLK/1/2/4/8. There is no selectable internal1.5/2.5 V
 reference, no VDD/3 channel and no READY or OVW status flag.
- Configure CONT=0, ENS=0, slot0 source in SQRCFR and slot0 sample duration in
 SAMPLE. Wait approximately 1 us after enabling external conversion. TS/BGR need
 approximately 30 us startup and at least 40 us acquisition. These are not x030's
 TS acquisition5 us or BGREN25 us policies.
- Acquisition encodings 0–15 mean6,7,9,12,18,24,30,42,54,70,102,134,166,198,262,390
 cycles, followed by 15 comparison cycles. Supply/clock table 20-3 has 4/12/24/48 MHz
 bands at 1.62/1.8/2.8/3.3 V. Enforce sample-rate and source-acquisition constraints
 as well as ADCCLK limits; a maximum clock alone is insufficient.
- START=0 aborts and resets the sequence slot. For one-slot single shot, await
 EOS/EOC and START cleared with bounded polling, then read RESULT0. ICR has
 EOC/EOS/AWDL/AWDH bits 0/1/2/3, R1W0, while ISR is read-only.
- CR@0: EN0/BGREN1/TSEN2/CONT3/CLK5:4/ENS8:6; START0x08,
 SAMPLE0x28, SQRCFR0x2c, RESULT0x40, IER0x74, ICR0x78, ISR0x7c.

L011's own SDK has the same exercised CR/sample/result layout and sample list,
plus extra trigger sources; this does not establish identical analog limits.
Own DS1.1 table 7-27 p53 lists VDD1.7–5.5 V, ADC clock 4–96 MHz, max sample rate 1 MHz,
voltage-dependent minimum acquisition1/0.5/0.25/0.125 us, and 21–405-cycle total
conversion. TS startup max 40 us is on p54. Own SDK examples request≥40 us internal
acquisition. Do not claim 2 MSPS or import L010's lower-voltage clock table. The newly acquired own RM CN V1.1 table 20-3 p507 supplies the voltage bands:
6/12/24/48 MHz at 1.7/1.8/2.8/3.3 V, and at most 1 MSPS. This is stricter than
the datasheet 96 MHz maximum; use the manual cap until that discrepancy is
resolved. Sections 20.11.1–3 pp517–518 establish approximately 1 us external
startup, approximately 30 us TS/BGR startup, and at least 40 us internal
acquisition. Use the datasheet maximum 40 us TS startup rather than the typical
manual delay when choosing a conservative wait.
L010 mux 0–6=PA0–6, mux 7–13=PB0–6; L011 mux 0–7=PA0–7,
mux 8/9=PB0/1 andmux10–13=PA8–11. Both use TS14/BGR15.

L012 own RM1.4 §§25.4–5 pp575–578 and§25.12 changes this further:

- ADC1 and ADC2 are independent register instances but share SYSCTRL ADC clock
 and active-low reset. Resetting one or dropping its gate must not disrupt the
 other. Start with explicit common ownership or a reviewed shared gate/reset
 guard; two naïvely independent `Adc<T>` constructors/drops are incorrect.
- CR EN0/CONT1/CLK3:2/ENS6:4/SLAVE7; START0x04, SAMPLE0x18,
 SQRCFR0x20, RESULT0x30, IER0x70, ICR0x74, ISR0x78. The manual gives CR reset
0x100 with bits 31:8 reserved: retain documented reserved defaults rather than
copying a blanket CR=0 initialization. ICR flags are0/1/3/4, not 0–3; its reset
value 0x0f is inconsistent with that flag map. Treat this as an explicit register
protocol review item, not a reason to reuse L010 masks.
- Sample encoding 15 is 518 cycles, not 390; the comparison stage remains15 cycles.
 Own table 25-3 gives6/12/24/48 MHz at 1.7/1.8/2.8/3.3 V but calls the last band's
 rate 1 MSPS although the 21-cycle minimum would exceed2 MSPS. Bound the initial
 API by both the listed rate and clock, and record the contradiction.
- BGR and TS control moved to shared BGR_CR (ch26), with roughly30 us startup;
 internal acquisition≥40 us. No BGR/TS bits may be written in ADCx_CR.
- ADC1 mux 0–9: PA0–7, PB0/1. ADC2 mux 0–9: PA5/6/7, PB0/1,
 PA8/9/10/11/12. Both mux 10=PB10,11=PB2,12=internal DAC_OUT2,
13=internal DAC_OUT1,14=TS,15=BGR. Initial pin-only reads do not imply safe DAC,
 cross-ADC synchronization, SLAVE or shared internal-source support.

All these ADC variants have software start/stop, not a documented ADC calibration
command. L010/L011/L012 factory TS addresses in their SDKs are0x001007CD/CE;
L011/L012 additionally list BGR trim0x001007D2. Do not treat an OPA CAL register
as ADC calibration. Leave physical-unit conversion and unsupported trims separate.

## GTIM families and precise arithmetic

Every GTIM in this audit has a 16-bit CNT/ARR/CCR and four CH outputs. None of the
examined own-manual timer modes authorizes STM32-style APB doubling: the internal
source is PCLK itself. Keep timer kernel frequency explicit even when PCLK≠HCLK.
The different prescaler/update mechanisms require distinct hardware capabilities.

### Classic power-of-two: F030/A030/F020/F002/F003

F020 RM§14.3.1.1–2 p215; F002/F003 own RM§12.3.1.1–2:
CR0.PRS[10:7]=0–15 divides by 2^PRS, and a changed prescaler latches at overflow
or EN0→1. ARR writes take effect immediately. Stop before reconfiguration,
reset CNT before lowering ARR, set the prescaler and clear only relevant flags,
then restart. Period=ARR+1; use a wide representation for65536.

F020 RM§14.3.4 pp226–228 explicitly defines CMMR mode 8=forced low,
9=forced high, E=high whenCNT≥CCR, F=high whenCNT<CCR. It has immediate compare
updates and no ARR/CCR preload/UG interface. The existing active-high policy is
modeF with CCR=duty ticks; active-low usesE. At duty0 and full duty use forced
inactive/active modes, preserving exact0%/100% even whenARR=65535 andperiod=65536
cannot fit CCR. Retain behavior on disable/drop and both polarities. Do not
promise glitch-free mid-period duty updates for this hardware.

Offsets: ARR0x300, CNT0x304, CMMR0x308, ETR0x30c, CR00x310,
IER0x314, ISR0x318, ICR0x31c, CCR1–4 0x320–0x32c, CR10x330;
DMA0x340 exists in x030/F020 but not F002/F003. ICR is R1W0, reset0x3ff,
clearable flag mask0x27f; reserved7/8 remain at reset1, and DIR10 is read-only.
F002/F003's single `GTIM` must never be manufactured into four timer tokens.

### Classic integer-prescaler: L031/R031/W031/L052/L083

Own L031 RM§14.3.1.1 p210, R031/W031 same section, L052 RM§15.3.1.1 p243,
L083 RM§15.3.1.1 p255: PSC@0x334 is 16-bit, with divisorPSC+1 for PCLK and divisorPSC
for TRS (PSC>0). A changed PSC latches at overflow or EN0→1. CR0 no longer has
PRS/PRSSTATUS; those bits must not receive the x030 prescaler encoding. ARR and
compare output model remain classic/immediate. The first supported source must
be PCLK; external/TRS counting requires different arithmetic and its own API.

Use `(PSC+1)*(ARR+1)` in 64-bit arithmetic: its maximum is 2^32, which overflows
u32. Select the smallest divider that permits a 16-bit period at/below the request,
with rational rounding and explicit unattainable-frequency errors. Retain forced
endpoints, pin ownership, and stop/reset/restart sequencing. L052's extra
TI1XOR/IC1RST/IC2RST/IC3RST fields do not enable hall/capture capabilities.

### Buffered design: L010/L011/L012

L010 RM§13.3.1.1–3 pp208–209,§13.3.4 pp234–236,§13.9.6 p267;
L012 RM§16.3.1.1–3 pp237–238 and corresponding compare/update sections:

- PSC is a 16-bit programmable divider1–65536 and always buffered until an update
 event. ARR buffering is selected by CR1.ARPE; CCR buffering by CCMR.OCyPE.
- Registers start at 0: CR1, CR2, SMCR, IER, ISR, EGR, CCMR1/2CAP/CMP,
 CCER, CNT0x24, PSC0x28, ARR0x2c, CCR1–4 0x34–0x40; separate ICR0x70.
- For a bounded first counter/PWM backend, choose internalPCLK, up-counting,
 edge alignment. Stop/disconnect outputs, programPSC/ARR/CNT/CCR, generate
 EGR.UG to latch, clear only actual update flags through ICR, then enable outputs
 and counter. A model must distinguish shadow and active registers.
- CCMR mode 4/5 forces OCREF low/high,6/7 selects PWM1/2; CCER.CCyP controls final
 polarity and CCyE output enable. These are not classic modes 8/9/E/F. Preserve
 the unrelated channel packed into the same CCMR register. Full-width100% needs
 forced-active handling ifperiod65536 exceeds CCR width.
- L010 ICR reset/clearable mask is 0x00f01e5f, R1W0, not a write-to-ISR convention.
 Disabling a CCER output alone does not establish its external inactive voltage;
 design and test the force-inactive/pin-disconnect sequence explicitly.
- L011 own SDK independently shows PSC/ARR, ARPE, OC preload, CCER polarity and
 output-enable programming, and its own datasheet lists both GTIM1/2 as 16-bit
 four-channel units. Its newly acquired own RM CN V1.1 chapter 13 independently confirms these
  timer mechanisms. Its current diagrams supersede the older same-named copy.

No initial implementation should expose down/center-aligned modes, captures,
encoders, retriggerable/asymmetric PWM, synchronization, trigger generation,
external clock, DMA, or an Embassy time driver merely because the raw fields exist.

## Clock/reset and ownership

The JSON joins own-family clock evidence to actual selected SYSCTRL field bits.
All these gates arePCLK and resets active low. A compact summary:

| Families | ADC gate/reset | GTIM gate/reset |
| --- | --- | --- |
| F020/x030 | APB2 bit 2 | GTIM1/2 APB1 bits 1/2; GTIM3/4 APB2 bits 10/11 |
| F002/F003 | APB2 bit 2 | GTIM APB1 bit 1 |
| L031/R031/W031 | APB2 bit 2 | GTIM1/2 APB1 bits 1/2 |
| L052 | APB2 bit 2 | GTIM1/2 APB1 bits 1/2; GTIM3 APB2 bit 10 |
| L083 | APB2 bit 2 | GTIM1/2 APB1 bits 1/2; GTIM3/4 APB2 bits 10/11 |
| L010 | APB1 bit 0 | GTIM1 APB1 bit 6 |
| L011 | APB1 bit 0 | GTIM1/2 APB1 bits 6/7 |
| L012 | sharedADC1/2 APB1 bit 0 | GTIM1/2 APB1 bits 6/7; GTIM3/4 bits 11/12 |

APBEN1/APBRST1 offsets0x38/0x48; APBEN2/APBRST2 offsets0x34/0x44.
Use the real family PAC and field capabilities, not a raw x030SYSCTRL pointer cast.
Clock initialization, supply assumptions and conservative initialization failure
behavior remain prerequisites. Avoid resetting/disabling a sibling peripheral
through shared resources, especially L012ADC and shared BGR/temperature hardware.

ATIM, BTIM, LPTIM and HALLTIM remain separate kinds. The inventory records their
actual presence: F002 has no ATIM, F003 does; F020 has no ATIM despite copied AF
cells in its datasheet; L010/L011/L012 have their own advanced/buffered timers;
L012 alone in this set exposesHALLTIM; L010/L011/L012/L052/L083 expose differing
LPTIM instances. A BTIM toggle output is not a GTIM compare PWM channel. Matching
ATIM raw versions do not authorize complementary outputs, break/dead-time or
motor-control APIs.

## Route audit and package limits

Independently compared CH1–4 cells, before package/safety filtering:

| Family | Matching own-PDF/SDK PWM cells | SDK-only PWM cells withheld |
| --- | ---: | ---: |
| F002 |15|3: PB7, PC3, PC4 |
| F003 |18|0|
| F020 |46|0|
| L010 |9|0|
| L011 |19|0|
| L012 |41|0|
| L031 |20|0|
| R031 |13|7|
| W031 |16|4|
| L052 |56|0|
| L083 |97|0|

F002's broader GTIM comparison also finds SDK-only ETR/TOG routes onPA3/PC3/PC4;
its own datasheet and own RM table 8-2 omit these pads. F003's24-pin part really
has the additional routes;20-pin parts do not. R031 SDK-only PWM rows arePA0–3,
PB8/9; W031's arePA15/PB3/PB4/PB5. Preserve own-PDF omissions and package/radio
restrictions instead of inheriting L031 route tables.

L010 datasheet usesGTIM while its PAC namesGTIM1. Most SDK macros sayGTIM1CHn,
but PA3 AF6 saysGTIMCH4. This is a coordinate- and family-qualified correspondence,
not an unrestricted renaming rule. None of these source labels authorizes a
fabricated timer instance.

F020's all 73 GTIM cells (46PWM,27other) agree with its current datasheet and own
SDK. Its three exact packages yield9/11/13 ADC pin candidates and 17/32/46 PWM
routes for QFN20/QFN32/QFN48. The family alias uses the common-package intersection,
not the largest die. All package projections in the JSON are candidates until
merged through reviewed manifests and generator checks. Existing SWD/reset/
input-only exclusions remain in force; do not automatically reconfigure debug,
reset, oscillator or radio pins. Analog routes have no digital AF selector.

## Confirmed access corrections found

At audit start these canonical register items lacked `access: Read`:

| Template / item | Own-manual evidence |
| --- | --- |
| gtim_cw32f020_v1 / GTIM.ISR@0x318 | F0201.4 §14.8.12 p246 |
| gtim_cw32f002_v1 / GTIM.ISR@0x318 | F0021.4 §12.7.12 p182 and F0032.3 §12.7.12 p184 |
| gtim_cw32l052_v1 / GTIM.ISR@0x318 | L0521.5 §15.8.13 p277 |
| gtim_cw32l083_v1 / GTIM.ISR@0x318 | L0832.0 §15.8.13 p289 |
| adc_cw32l012_v1 / ADC.ISR@0x78 | L0121.4 §25.12.9 p597 |

All defined ISR fields areRO; flag clears belong to separateR1W0ICR registers.
F002/F003 share one template but were checked against both own manuals. These
findings were handed to the parent for any separately authorized correction;
these corrections were subsequently implemented with exact source proof and
regression checks, as documented in the two correction reports. RawSVD/header access is not
sufficient to override explicit own-manual permission tables.

## Concrete implementation and verification plan

### First batch: F020 only

1. Add narrowly qualified analog and GTIM-PWM route sidecars with per-cell source
   hashes, own-package joins and source signal versus mux fields. Keep all other
   AF kinds unpromoted. Test exact9/11/13 and 17/32/46 package counts, alias
   intersection, wrong channel/AF/peripheral, unbonded/SWD pins, and absence ofATIM.
2. Preserve real register version identity. Introduce capability-selected shared
   logic/adapters; do not label the entire F020 asx030. Build cfgs should name
   reviewed hardware capabilities and assert exact metadata versions. The
   access correction now deduplicates F020 GTIM to `v1`; family-specific
   clock/pin capability still needs explicit review.
   `adc_cw32f020_v1` and the now-exact `gtim_v1` schema may select shared logic only after
   these independent checks. A generic `adc` or `timer` cfg must not mean all
   present raw hardware is supported.
3. Keep latest module layout: `adc/mod.rs`, `adc/<backend>/mod.rs`,
   `adc/tests/mod.rs`, `timer/mod.rs`, `timer/<backend>/mod.rs`,
   `timer/low_level/mod.rs`, `timer/simple_pwm/mod.rs`, `timer/tests/mod.rs`.
   Do not reintroduce sibling `adc.rs`/`timer.rs` or cfgs describing unrelated
   software choices. Reuse public Embassy ownership/channel erasure shape where
   capabilities agree; gate unavailable references/internal channels explicitly.
4. Extend the actual existing engine-model tests: bounded startup/read/abort,
   stale-result drain, overrun, EOC withoutSTART-stop, timeout reinitialization,
   temperature source switching/settling, exact voltage boundaries and rational
   timing. RAM-PAC tests must assertF020 offsets, reserved defaults and clock
   neighbor preservation. ExcludeENABLE14 and ATIM trigger operations.
5. Extend current timer model tests for F020: no APB doubling, all 16PRS encodings,
   period 1/65536, invalid frequencies without partial writes, CNT reset before
   loweringARR, prescaler latch on restart, R1W0 clear masks, all 4channels,
   forced0/100% for both polarities, frequency-change duty preservation, and
   inactive-before-disconnect drop ordering. Keep the actualF020PAC in tests.
6. Compile positive typed ADC/pin/PWM ownership examples and negative wrong-pin,
   wrong-channel, duplicate owner, invalid borrowed-channel lifetime, unsupported
   ADC14/DMA/ATIM/async/capture cases for all three exact parts and alias. Run
   host tests and Cortex-M0+ `rt,defmt` release/link checks, plusx030 regressions,
   generated/source parity and all package/metadata contracts. Existing tests
   passing without these new assertions is not implementation validation.

### Later batches

- F002/F003: prove no DMA-register access, singleGTIM token, F002 forbidden
  reference/channel compile cases, both own maps and reservedADC bits.
- Integer-PSC group: testPSC0/65535, divider65536, total divisor 2^32 in 64-bit,
  no CR0PRS writes, current prescaler activation, own pin mappings/R031 alias
  separation, ADC ISR relocation and no CR0bit4 writes.
- Buffered timers: build a shadow-register model, verifyingPSC/ARR/CCR update
  events, explicitUG ordering, UIF clearing, preload choices, packedCCMR
  preservation, final pin polarity, endpoints and quiescent output transitions.
- Sequence ADCs: no READY/OVW assumptions; one-slotENS=0, sample encodings,
  source acquisition plus analog startup, EOS/START completion, abort-to-slot0,
  R1W0 variant masks, ADC1/ADC2 shared-reset/gate lifecycle, BGR ownership and
  preservation ofL012 reserved defaults. Gate internal channels until own-source
  settling/clock bounds are sufficient. Keep conversions blocking and bounded.

Hardware validation remains necessary before claiming silicon correctness:
known-voltage ADC channels at supply/clock corners and high source impedance;
internal-source settling; frequency/duty/polarity including0/100% and ARR65535;
nonunityAPB divider; reconfiguration/drop edges; shared-resource interference.
Host/RAM/compile tests cannot establish analog accuracy, electrical safety,
glitch behavior or clock tolerance.
