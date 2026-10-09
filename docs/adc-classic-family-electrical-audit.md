# Classic ADC family electrical and control audit

Read-only own-source audit, 2026-10-08. Covers CW32F002/F003/L031/R031/W031/L052/L083 only. No implementation changes or silicon validation were performed by this audit. Page references below are **printed pages**; the corresponding PDF page number is printed page + 1. Sources are the locally retained official manuals, datasheets and own-family SDKs, individually hashed below. Matching register names alone were not treated as proof.

## Safe supply and reference policy

| Family | ADC supply range | Supply/ADC datasheet pages | Own RM timing table |
|---|---|---|---|
| F002 | VDD 1.65–5.5 V | DS1.2 pp30,43 | RM1.4 table19-3 p303 |
| F003 | VDD 1.65–5.5 V | DS1.9 pp31,44 | RM2.3 table20-3 p363 |
| L031 | VDDA=VDD 1.65–5.5 V | DS1.9 pp37,52 | RM1.6 table22-3 p433 |
| R031 | VDDA=VDD 2.2–3.6 V | DS1.2 pp41,59 | RM1.3 table22-3 p436 |
| W031 | VDDA=VDD 1.8–3.6 V in RF LDO mode; 2.0–3.6 V in RF DCDC mode | DS1.3 pp40,58 | RM1.4 table22-3 p436 |
| L052 | VDDA=VDD 1.65–5.5 V | DS1.3 pp42,57 | RM1.5 table23-3 p470 |
| L083 | VDDA=VDD 1.65–5.5 V | DS1.9 pp46,61 | RM2.0 table23-3 p475 |

These are operating limits, not absolute-maximum ratings. The tables apply to the named family/package variants in each own datasheet; no different ADC electrical band is assigned to a particular bonded package. Package-specific accessible pins still require separate route validation. W031's 1.8 V ADC acceptance does not certify the RF subsystem in DCDC mode; the board must meet its selected RF-mode limit. All L/R/W datasheet general-condition tables above explicitly require VDDA=VDD. Some introductory supply prose instead says VDDA>=VDD; the stricter equality condition is the safe documented operating policy.

For supply or external-reference conversion, independently confirmed maximum ADCCLK bands are:

- 1.65 <= supply < 1.8 V: 500 kHz; available only on F002/F003/L031/L052/L083.
- 1.8 <= supply < 2.0 V: 2 MHz; unavailable on R031, and restricted to RF LDO operation on W031.
- 2.0 <= supply < 2.4 V: 4 MHz; R031 starts at 2.2 V.
- 2.4 <= supply < 2.7 V: 12 MHz.
- 2.7 V through each family's maximum supply: 24 MHz.

Use the declared **guaranteed minimum** board supply, including tolerance, and compare PCLK against limit*divisor before division. The board must independently guarantee that its actual maximum supply stays below the family ceiling. The RM bands share endpoint labels; choosing the new band at its exact lower boundary follows the stated thresholds, with no rounding down of the clock beforehand.

F002 has only REF=2 (external ExRef) and REF=3 (VDD). Internal1V5/Internal2V5/Temperature/VrefInt must not be exposed. Its only internal input is VDD/3 at mux 13. ExRef remains out of the initial safe API until explicit reference-pin ownership is implemented.

For the other six families, Internal1V5 requires supply >= 1.8 V and permits 2 MHz below 2.0 V, 4 MHz from 2.0 V upward; on R031 its own table starts at 2.2 V and permits 4 MHz. Internal2V5 requires supply >= 2.8 V and permits 4 MHz. Family upper supply limits still apply. These reference-supply limits come from each own RM table above, not nominal reference value plus a guessed margin.

## Acquisition, rate and analog startup

All seven own manuals independently define CR0.CLK as PCLK / {1,2,4,8,16,32,64,128}; there is no APB doubling. CR0.SAM encodes acquisition {5,6,8,10} ADCCLK cycles, followed by 19 comparison cycles. Total conversion time is therefore 24/25/27/29 cycles. Sources: the timing-table pages above and immediately preceding pages (F002 p302, F003 p362, L031 p432, R031/W031 p435, L052 p469, L083 p474).

All seven datasheet ADC-characteristic tables list 1 MSPS maximum sample rate, typical 24 MHz ADC clock, input-voltage range 0..VDD/VDDA, typical 9 pF sample/hold capacitance, and 100 kΩ maximum in the row described as input impedance (direct/buffer). The typical 24 MHz entry is not an unconditional clock permission; apply each RM voltage/reference band. Do not infer 12-bit accuracy at every clock/source impedance from that 100 kΩ row. External acquisition is a board/source obligation: select enough acquisition cycles or reduce clock, enable BUF for weak/high-impedance sources, and meet the documented analog input and reference full-scale limits. No universal microsecond acquisition minimum for all external inputs is specified in these sources.

Each own RM conversion-accuracy section requires BUF for weak external inputs and internal sources and directs these uses to single-channel single-shot operation. CR0.BUF's register description limits conversion rate to 200 kSPS whenever enabled. Thus enforce ADCCLK <= (sample_cycles+19)*200,000 for buffered reads, in addition to the supply/reference clock limit. Relevant accuracy pages: F002 p304, F003 p364, L031 p434, R031/W031 p437, L052 p471, L083 p476. F002's copied prose mentions TS/BGR even though its actual feature/channel/reference tables exclude them; retain BUF for VDD/3 and do not promote unavailable inputs.

| Family | Temperature startup/acquisition proof | BGR startup statement |
|---|---|---|
| F002 | No supported temperature channel | No supported BGR input/reference gate |
| F003 | DS1.9 table7-28 p46: <=45 us startup, >=5 us acquisition | RM2.3 p399: approximately 20 us |
| L031 | DS1.9 p54: <=45 us startup, >=5 us acquisition | RM1.6 p472: approximately 20 us |
| R031 | DS1.2 p61: <=45 us startup, >=5 us acquisition | RM1.3 p474: approximately 20 us |
| W031 | DS1.3 p60: <=45 us startup, >=5 us acquisition | RM1.4 p475: approximately 20 us |
| L052 | DS1.3 p59: <=45 us startup, >=5 us acquisition | RM1.5 p512: approximately 20 us |
| L083 | DS1.9 p63: <=45 us startup, >=5 us acquisition | RM2.0 p514: approximately 20 us |

Therefore a 50 nominal-us temperature startup wait and acquisition>=5 nominal us have independent own-family support for all six TS-capable families. Rewait after TS enable or follower reenable, and enforce acquisition with ADCCLK <= sample_cycles*200,000. Source startup and per-conversion acquisition are different requirements. Clock tolerance remains a board-clock guarantee; a nominal-time wait is not a proof against arbitrary oscillator error.

For every family EN 0->1 starts analog initialization and READY becomes 1 after approximately 40 us; conversion must await READY, with bounded polling. A fixed 40 us delay alone is insufficient. Internal1V5/2V5 selection is covered by the own ADC setup flow. The BGR 20 us value is only approximate, stated in comparator chapters, not a guaranteed datasheet maximum. A 25 nominal-us BGR guard is a conservative software margin rather than a characterized maximum. No additional hard acquisition minimum for the BGR 1.2 V or supply/3 channels was found; those inputs still require BUF and its 200 kSPS cap. READY remains mandatory.

## Control ownership, enabled writes and reset defaults

| Family | CR0 pages | CR1 page | EN+READY before later CR0 configuration | Temperature enabled-write example | START/ISR/ICR pages |
|---|---|---|---|---|---|
| F002 |326|327|307|N/A|331/332/333|
| F003 |387–388|389|367|384|393/394/395|
| L031 |458–459|460|437|455|465/466/467|
| R031 |460–461|462|440|457|467/468/469|
| W031 |461–462|463|440|458|468/469/470|
| L052 |496–497|498|474|493|505/506/507|
| L083 |500–501|502|479|497|507/508/509|

Every CR0 resets to 0. BIAS15:14 is RW but explicitly must remain at its default 0; upper31:16 reserved bits must also remain 0. EN0, MODE3:1, REF7:6, CLK10:8, SAM12:11 and BUF13 are present. F002 reserves5:4. F003 has TSEN5 and BGREN4. L031/R031/W031/L052/L083 have TSEN5 but reserve bit4. Do not even transiently write BGREN4 on the latter group. Single-shot mode leaves MODE=0; no automatic-stop, accumulation, DMA, watchdog or external trigger is needed.

For F003, the documented ADC BGREN is also used by comparators selecting BGR and temperature sources (RMp399 andp407); ADC ownership must exclude concurrent raw comparator use of its analog sources. Its SDK sets BGREN for BGR 1.2 V channels and TSEN for temperature. Setting BGREN alongside TSEN or an internal reference is supported by the real CR0 field, but do not copy that write to another family.

For the five L/R/W families, each own SDK ADC initialization touches BUF/SAM/CLK/REF/TSEN/MODE; internal-channel selection sets BUF for 13/14/15 and TSEN only for 14. Their own headers/manuals have no separate ADC BGREN software gate. The safe adapter follows that documented sequence: EN/READY plus REF/BUF/TSEN, with bit4 permanently 0. Do not invent a shared BGR register or claim the BGR is always powered. The comparator chapters still say to enable internal BGR but do not identify a separate gate; that incomplete prose does not authorize reserved-bit writes. Comparators may depend on ADC-selected reference/temperature, so the ADC must not promise safe simultaneous independent analog ownership. A future comparator integration needs coordinated ownership before ADC reset, reconfiguration or shutdown.

The setup examples explicitly set EN=1, wait for READY, then configure MODE/REF/SAM/CLK and later TSEN/BUF. This is affirmative evidence that such **idle, enabled** CR0 writes are allowed. It does not authorize changing conversion configuration while START=1. Stop conversion/external triggering first; then set channel/timing, settle sources as required, verify READY, and start the new conversion.

CR1@0x04 resets 0 in every family. CHMUX3:0, DISCARD5, ALIGN6, WDTCH11:8 and WDTALL13 exist; bit4, bit7 and bit12 are reserved. F002/F003 additionally reserve31:14, with no DMA bit. The five L/R/W families instead have DMASOFEN14 and DMASOCEN15, reserving31:16. A zero initial CR1 and later writes of only the valid 4-bit mux keep right alignment, overwrite policy, watchdog and all DMA requests disabled. F002 accepts mux 0..13; the other six accept0..15. CR2@0x10 also resets 0; its no-accumulation single-shot initial value is 0.

## Abort, stale results and reset proof

START@0x08 bit0 writes 0 to stop and 1 to start; AUTOSTOP1 must remain 0 for this engine. MODE 0's own channel sections say completion stores RESULT0, sets EOC and clears START automatically. Require EOC **and** stopped START before accepting a result; EOC alone is not adequate against incomplete/stale state.

On cancellation, timeout, overrun or drop: write START=0, disable event/trigger requests, disable EN and this driver's analog controls, drain RESULT0 and clear actual event flags. Power-down/reinitialization is a conservative recovery policy; the manuals do not specify a separate abort-completion flag or guaranteed abort latency. Do not label an interrupted result valid. For a later read, reinitialize and wait for fresh READY. Before a new conversion, drain unread RESULT0 and clear stale EOC/OVW. An OVW sample is discarded, not silently returned.

ISR is read-only: EOC0, OVW6, READY7. ICR is R1W0 bits0..6, reset 0x7f, reserved31:7 reset 0; clearing flags uses 0x7f & ~flags, never writes READY or ISR. F002/F003 IER/ICR/ISR offsets are 0x34/0x38/0x3c; the five L/R/W families use 0x44/0x48/0x4c. RESULT0@0x20 is read-only in all seven. L052 additionally has RESULT@0x5c but still stores single-channel results in RESULT0 (RMp471); no common-result or extra result register is needed by the bounded engine.

All seven own SYSCTRL chapters independently assign ADC gate to APBEN2@0x34 bit2 (1 enables configuration and operating clocks), and ADC reset to APBRST2@0x44 bit2 (0 holds reset,1 releases). Use the actual selected PAC, masked critical-section updates and readback; preserve every unrelated gate/reset bit. Own page pairs (gate/reset): F002 61/64, F003 63/66, L031 78/81, R031 80/83, W031 79/82, L052 82/85, L083 88/92. The registers' whole reset values differ across families, so writing a copied full SYSCTRL word is not justified.

## Source contradictions and remaining limits

- F002 SDK contains internal reference constants, TSEN/BGREN operations and TS/BGR channels that conflict with its restricted own RM CR0/reference/channel tables. Its BUF prose also repeats unavailable TS/BGR names. Follow the restrictive family feature, channel and permission tables.
- W031 own SDK `Libraries/inc/cw32w031_adc.h` channel comments at lines194–198 claim mux8=PC04,9=PC05,10=PB00,11=PB01,12=PB02. These are incompatible with its own RM table22-5 p439 and DS table5-2 pp26–27, which agree on mux8=PB00,9=PB01,10=PB02,11=PB10,12=PB11. The numerical SDK constants 0..12 alone do not validate those copied GPIO comments. Use the own-PDF mapping and independent package filtering.
- R031 is a different family with its own source-label/mux distinction; do not substitute W031/L031 channel names when projecting its pin routes. This report does not replace the separate coordinate/package route audit.
- L/R/W comparator BGR-enable prose lacks a named gate while their ADC CR0 explicitly reserves bit4. Follow the register permission table and own SDK; do not infer a hidden control.
- The BGR approximately 20 us and ADC approximately 40 us statements are not tested maximum-time guarantees. READY and bounded error paths are required; analog accuracy and transient settling still require hardware measurements.
- Host/RAM tests can establish masks, offsets, sequencing, bounded waits and exact arithmetic; they cannot establish analog accuracy, source drive, supply integrity or actual source startup at electrical corners.

## Independent implementation review

The new `adc/mod.rs` and `adc/classic` policies were checked against this evidence. The family supply bounds, F002 capability restrictions, eight-result BGREN=0, TSEN5, temperature50 us startup plus5 us acquisition, BUF200 kSPS cap, bounded READY/EOC/START handling, mux-only CR1 writes and R1W0 event clearing match the audited subset. No implementation-level electrical/control mismatch was found in that review. This does not replace the implementation's tests or hardware validation.

Public module documentation must retain VDDA=VDD as a board requirement for L031/R031/W031/L052/L083 and extend the exclusive analog-source warning to **all** supported classic families: their comparators can depend on ADC-selected reference/temperature even where no separate BGREN gate is written. Describing the eight-result EN/READY sequence as the documented, SDK-compatible setup is supported; claiming an undocumented independent BGR power-control mechanism is not.

## Exact primary-source hashes

SHA-256 hashes below pin the inspected evidence, not generated implementation output. PDF filenames and SDK paths are relative to the retained `cw32-sources` source directory. All28 entries were rehashed and verified after report editing.

### F002

- `CW32F002_UserManual_CN_V1.4.pdf`: `e453b3f82d9d180a86cd5f7cb18d858d7f228afc8101d87c700ca9d5c02d5add`
- `CW32F002_DataSheet_CN_V1.2.pdf`: `6d0c5c37d069e5b4e33d394b0be6c53938868e9375e7b4bbbbdd40d62bb9d506`
- `cw32f002/Libraries/src/cw32f002_adc.c`: `bfba4bf13b286e2e650c0fc8455eb17c18f4e872c98820d15db5acbbdb3ed9c2`
- `cw32f002/Libraries/inc/cw32f002_adc.h`: `6659fbac3a3d2df0436fae42c7be19e3841d581ad5377fbd4cf0ec3b54b236e1`

### F003

- `CW32F003_UserManual_CN_V2.3.pdf`: `0fa58dac223add7f2ac1ee714a7df7db4e414e0f193601b80dda96a948bfc738`
- `CW32F003_DataSheet_CN_V1.9.pdf`: `5fe15321b3963472c2629030767cf61a0ce36063815b54add74025b5b13b95dd`
- `cw32f003/Libraries/src/cw32f003_adc.c`: `683164924a334a82961527fcfd57ec288fef292701dd2e9ce13eeaab56807574`
- `cw32f003/Libraries/inc/cw32f003_adc.h`: `d76e3217e53a06a1671a60b4749a577aaad1c8f5520b15505a49d9a66fa0ca1c`

### L031

- `CW32L031_UserManual_CN_V1.6.pdf`: `4288cfd97b56385059c5a283f69972047af4773ef8bbc4d8b51d8155fb17a760`
- `CW32L031_DataSheet_CN_V1.9.pdf`: `90525f4085d00e9d586a991c24f2e4398d92a41e6cc1402c413e963423a35855`
- `cw32l031/Libraries/src/cw32l031_adc.c`: `11b87264ec9a6203e2e9cc0cc168c91adebfbfb9478e2725817c9af6f751d202`
- `cw32l031/Libraries/inc/cw32l031_adc.h`: `990b97a7433129327fafd19f01dbe03a47b8f0b00b836b077d018ca9f94201a7`

### R031

- `CW32R031_UserManual_CN_V1.3.pdf`: `fbee9b6942be9fa09f00c946705644d5356c4249f3e2280e3dfe5f0cb342eddb`
- `CW32R031_DataSheet_CN_V1.2.pdf`: `88759314fa4cf8b6caf7098df4829489179e27aa3752a13de85ce955b29c5805`
- `cw32r031/Libraries/src/cw32r031_adc.c`: `b6ed925f684a06ec95c9942c87f51701b2fdfb87ce9af7dd78c5fedafb8d8e36`
- `cw32r031/Libraries/inc/cw32r031_adc.h`: `d9a2e9c4555b7b90c93808339fe60cf802cfc4a3ea14f83a3ec586c2712aa733`

### W031

- `CW32W031_UserManual_CN_V1.4.pdf`: `b6973677946a9332b0e5b3e954119768aa40469d44140a73e18648e9419bedc9`
- `CW32W031_DataSheet_CN_V1.3.pdf`: `45ec43e6370956d09f9b9aa4c0f6c83fb0661f576d2d2bf64e0a6d7c4e203d3c`
- `cw32w031/Libraries/src/cw32w031_adc.c`: `4549b210fce024feff5373b3dd755e55fc38a007ec33a1c06d840345ffd9a0ca`
- `cw32w031/Libraries/inc/cw32w031_adc.h`: `ddd4f253101a98cf7883da931953446ab499f6df00244db47d41d72f39092dd8`

### L052

- `CW32L052_UserManual_CN_V1.5.pdf`: `4bac53df4db69a3b76c833cb14dd45b0e5c7f9884506c83a5cda6ea00a859f41`
- `CW32L052_DataSheet_CN_V1.3.pdf`: `f03e2c3545b52942b576f669e3de69e1962631834ef6a01e51b45996fbeb3e7f`
- `cw32l052/CW32L052_StandardPeripheralLib_V1.4/Libraries/src/cw32l052_adc.c`: `72d4af906902d6c63d6c51d39659621f1d07b7e38d20ca2a3640b3eadf82af6e`
- `cw32l052/CW32L052_StandardPeripheralLib_V1.4/Libraries/inc/cw32l052_adc.h`: `f14df878f5cb5ddb1bba79c816a4c8db404e54e535fac11a99bc15b30f1e2ce7`

### L083

- `CW32L083_UserManual_CN_V2.0.pdf`: `9930bf1755f3bbf8933163c2d0da57fd9a4f3250a358a4c0bfc75ed4eda3a0a3`
- `CW32L083_DataSheet_CN_V1.9.pdf`: `852f772e9174cb76bf0f475f31f1e275254f8fe176bd3e7ad60d00b41db9509e`
- `cw32l083/Libraries/src/cw32l083_adc.c`: `cf0046a968611133d690af17e2826551244980be3faf7ab675e6612f47249130`
- `cw32l083/Libraries/inc/cw32l083_adc.h`: `43591f8fc96a509c311150f16d70a62e8d8b9a2faec364ce77480dbd41823ecf`

