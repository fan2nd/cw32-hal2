# CW32L012 DAC/OPA own-source review

Reviewed 2026-10-08. Read-only review of the locked CW32L012 CN UserManual V1.4, CN DataSheet V1.0, corroborating EN UserManual V1.0, and SDK V1.0.5. No code edits or HAL tests performed. `source-hashes.json` records verified official artifact identities plus every inspected DAC/OPA SDK member; all selected local members were compared byte-for-byte with the locked ZIP.

## Conclusion for the proposed narrow APIs

Source support is sufficient for direct, unbuffered DAC pin output with 8-bit or 12-bit DHR writes, and externally pinned OPA follower, internal PGA, and external-feedback operation. Restrict to one positive input; for PGA/follower leave both external negative-input switches disconnected. Standalone requires one positive input, one negative input, and output, with a caller-provided external feedback network. Fixed BIAS=7 (8 microamps bias) avoids unsupported lower-bias settling promises. Do not expose LPMODE or calibration in this initial slice.

The proposed counterpart-conflict checks are appropriate: treat matching OPA EN=1 as potentially driving the shared pad in every mode; treat DAC as potentially driving its pad when both channel EN and CR1.CxOUT are set. Perform this check before configuring the output GPIO or enabling the new output. A disabled configuration-clock gate is not evidence that analog output is off. Acquire the counterpart gate without resetting or changing its channel configuration; do not undo a gate inherited from another owner.

Constructor return and DHR-write return must not promise electrically settled output. OPA EN auto-starts BGR, but the manual's recommended programming sequence explicitly pre-enables and stabilizes BGR before setting OPA EN. A caller-responsible settling contract must identify BGR startup, OPA startup, load-dependent output settling, and the absence of a guaranteed maximum timing bound in the selected datasheet.

## Authoritative source identities

CN manual V1.4: https://www.whxy.com/uploads/files/20260603/CW32L012_UserManual_CN_V1.4.pdf
- PDF SHA-256: a9e54694a26f03c1f3e3041f40844900168ca2b8e6142f3328671b92e6b7a340
- Extracted text SHA-256: 57f16772d6cfdb8055f8f2f91b5fed7257a963aa36c648293eb23762a6757505
- Printed pages cited below map to one-based PDF pages by adding 26.

CN datasheet V1.0: https://www.whxy.com/uploads/files/20250717/CW32L012_DataSheet_CN_V1.0.pdf
- PDF SHA-256: 08a605fe8843d1eefa2b1115474b5a74df2833068f0ad6f35225e4f39377da76
- Extracted text SHA-256: 00ba7e2c9d26a8ce5a4eaa4cc2e7790773c427b13a4d491838622041abfa8b0a
- Printed pages cited below map to one-based PDF pages by adding 3.

SDK V1.0.5: https://www.whxy.com/uploads/files/20260701/CW32L012_StandardPeripheralLib_V1.0.5.zip
- ZIP SHA-256: 8b0a4c0ee865642d08f353aec98906c900a8b10f672120e649e6b1bf5e29c18f
- Libraries/inc/cw32l012.h: 3758779c7b9e60fda1aa80dfd2848b6986074d91a6763076fa6777d55b1ad000
- Libraries/inc/cw32l012_dac.h: 272c2a90583f3f98bc1aa0e0fbd06a9b0b3951aacf2bd14525eb321d4ca2f709
- Libraries/src/cw32l012_dac.c: ceabb6c1c8b0a6264c40383d9525b3d01f0bd8f528b6b0f3c93f60646cda7ed9
- Libraries/inc/cw32l012_opa.h: 88177b95a1a8d60ff273e17d688a2be002e73b6466724598cd5316fbe7361406
- Libraries/src/cw32l012_opa.c: e96839a43436f4f3e093449a829bc2b563a83326dba567980755506f380b4aba

Corroborating EN manual V1.0: https://www.whxy.com/uploads/files/20260603/CW32L012_UserManual_EN_V1.0.pdf
- Current PDF SHA-256: f56d5ed899dd090b469fac6a09084aab9f5068ae3b56cf7562b83997bd2f1088
- Current extracted text SHA-256: 4b4413bdc5c57d7c0f23b5c949a66e10997b0adf6b087cea8d08b125b5320737
- Current June 2026 cover: OPA CR printed p709 = one-based PDF p735.
- The previously reviewed January-cover snapshot is unavailable at the same URL. Its exact identities and the limited source migration are retained in `docs/l012-english-source-update.json`; the earlier review is historical evidence, not a claim that the old bytes were recovered.

All identities above match `sources/evidence-sources.json` in the reviewed snapshot, except DAC headers/driver and OPA header are new member-level evidence to add there. Their containing SDK archive is already locked. Additional reviewed examples and full member identities are in `source-hashes.json`.

## DAC findings

1. CN manual sections 26.1-26.2, pp600-601: two independent DAC channels, 8/12-bit operation, VDDA supply/reference. No selectable BGR-reference mode is described.
2. Section 26.4, p603: WAVE=0 selects DHR data; source changes do not automatically clear DOR. Explicitly initialize DHR before connecting the pad rather than relying on inherited DOR/reset state.
3. Section 26.5, p604 and register sections 26.10.3-26.10.11, pp612-614: single 12-bit right-aligned data in bits11:0, 12-bit left-aligned bits15:4, 8-bit right-aligned bits7:0; packed dual-channel registers also exist. Narrow implementation should validate 12-bit values before MMIO and use the corresponding typed 8-bit DHR API.
4. Section 26.6, p605: TEN=0 directly loads DHR into DOR. The more precise CR0 description, section 26.10.1 pp610-611, specifies one dac_pclk cycle after DHR write. There is no need for SWTRGR, a timer, waveform generator, DMA, or ADC configuration for direct mode.
5. Section 26.10.1, pp609-611: initialize DMAEN=0, DMAUDRIE=0, WAVE=0, TEN=0. Both EN and CR1 output enable are required for external output. SDK DAC_Init first disables the selected channel EN before reconfiguration and preserves the other channel's half of CR0.
6. Section 26.10.14, p616: CR1.DAC_OUT1/2 control only external pads; internal connections remain available. Shared OPA/DAC outputs must not drive simultaneously. This is explicit vendor guidance, not inferred from the pin table alone.
7. Datasheet Table5-2 pp32-33 and manual Table29-1 p646: DAC1/OPA1 output share PB00; DAC2/OPA2 output share PB01. PB00 is also OPA2_INP3; PB01 is also OPA1_INP3. Owning every selected physical pin prevents overlapping live safe drivers, including crossed input/output use.
8. Datasheet section7.3.14 Table7-30 p62: DAC tSTART is 3 microseconds TYPICAL only, characterized at 3.3V, code0xFFF, capacitive load <=50pF and resistive load >=5kOhm; no maximum is provided. Output resistance is typically4180Ohm in the listed condition. Direct DAC is not a low-impedance buffered output. Do not promise settled output from a fixed3us delay or from DOR readback.

## Clock, reset, and BGR findings

1. CN manual section4.2 clock tree p26: PCLK routes to DAC and OPA through SYSCTRL_APBENx. Section4.7.13 p59: APBEN2 DAC bit10 and OPA bit9 are configuration-clock enables, protected by KEY=0x5A5A. ADC has an independent APBEN1 bit0 (section4.7.12 p58).
2. Section4.7.16 p63: APBRST2 DAC bit10 and shared OPA bit9 are active-low reset controls. OPA1/OPA2 share the OPA reset. A clock-acquisition helper must not reset the block or release externally asserted reset as an implicit side effect.
3. Figure26-1 p602 labels the clock after PCLK and APBEN2.DAC as ADCCLK. This conflicts with CR0's explicit dac_pclk terminology. There is no DAC clock divider/selector or documented dependency on ADC_CR configuration. SDK DAC_Init/DAC_DeInit enable only DAC gate; pure DAC examples enable DAC and GPIOB only and do not configure ADC. Therefore use PCLK/DAC gate; record the figure-label inconsistency instead of importing an ADC clock dependency.
4. Section25.12.19 p599: BGR_CR.BGREN bit0, TSEN bit1. BGREN, ADCEN, TSEN, VC enables and OPA1/2 enables start BGR; its startup is approximately30us, including hardware-triggered startup. BGR only disables through software or POR. Preserve BGR/TS and never disable these shared resources in OPA drop.
5. Section29.4 p647: the vendor's recommended sequence enables GPIO and OPA clocks, configures analog pins/input switches/mode, explicitly enables BGR and waits stable, then enables OPA. The same example proceeds into level calibration; the chapter otherwise describes calibration as separately enabled by CALEN. There is no numeric startup maximum or mandatory-calibration guarantee established here.

## OPA modes and register ownership

1. Sections29.1-29.3, pp644-646: OPA1 and OPA2 each have three external positive inputs, two external negative inputs, one internal DAC positive input, and one external output. All selected I/O must be analog.
2. Table29-1 p646 corroborated by datasheet pp32-33 and SDK opa.h:
   - OPA1: INP1=PA03, INP2=PA06, INP3=PB01, INN1=PA04, INN2=PA07, OUT=PB00.
   - OPA2: INP1=PA04, INP2=PA06, INP3=PB00, INN1=PA05, INN2=PA07, OUT=PB01.
   - Internal INP4 is matching DAC_OUTx. Defer this route unless DAC channel lifetime/ownership and pad-disconnect are explicitly modeled.
3. Section29.6.1 pp649-650: CR has BIAS[15:13], AMP[12:10], INN2EN[9], INN1EN[8], INP4EN[7], INP3EN[6], INP2EN[5], INP1EN[4], reserved bit3, MODE[2:1], EN[0]. No separate output enable/disconnect field is present in CR or CAL.
4. MODE=0 or1 means external-feedback amplification; use canonical0. MODE=2 means internal PGA; AMP codes0..4 give gains2,4,8,16,32. MODE=3 means internal follower. BIAS codes0..7 correspond1..8microamps. AMP codes5..7 are not documented as valid.
5. SDK examples for follower/PGA set InputN=NONE; standalone connects its selected negative input. SDK OPA_Init explicitly removes external negative input selection for follower. Narrow safe APIs should also remove external negative switches for PGA per its example, rather than permitting accidental external loading of the internal network.
6. The register map provides no supported mode that retains an enabled amplifier while disconnecting its pad. All three mode examples configure analog output and call OPA_Start. Thus conservatively reject any matching inherited OPA EN=1 when constructing direct DAC output, independent of MODE.
7. SDK OPA_DeInit toggles the shared OPA reset after clearing one instance. This is unsuitable with independently owned live OPA instances. Clear only owned EN/configuration; preserve peer and shared gates/reset.

## Explicit conflicts and deferred claims

- OPA CR reset: current CN section29.6.1 p649 says0x0000E000 (BIAS=7), and SDK opa.h defines OPA_CR0_RESET_VALUE=0x0000E000. The previously reviewed EN snapshot section29.6.1 p715 said0x00000E00 despite identical field positions. The current June-cover EN snapshot section29.6.1 p709 now says0x0000E000, agreeing with CN and SDK. Use explicit BIAS=7 configuration; do not copy the old reset value. Vendor SVD does not state an OPA CR register-specific reset value, so it does not resolve the conflict by itself.
- LPMODE: datasheet section4.15 p20 mentions normal/low-power modes; Table7-34 p65 footnotes condition some electrical figures on OPA_CR.LPMODE=0. Neither selected CN/EN OPA register description, CMSIS header, nor SVD defines LPMODE. Treat as unresolved source discrepancy; do not assign reserved bit3 or expose low-power control.
- OPA startup: visually verified Table7-34 p65 places2.5us TSTART in the MINIMUM column, not typical or maximum. `l012-datasheet-opa-p65.png` is the visual verification. No safe maximum-ready interval can be derived from this row.
- Calibration duration: CN section29.6.2 pp651-652 gives CALPERIOD=8*2^n OPACLK cycles and AZRUN high for twice the period. SDK opa.h enum names advertise16*2^n clocks. This may distinguish one phase from total operation, but the sources do not justify conflating them; defer a calibrated API pending precise contract/hardware validation.
- Level calibration: START starts/stops calibration only when CALMODE=1 and CALEN=1. AZRUN explicitly does not indicate completion in level mode. SDK opa.h bias names ending36/20/11/10/8/7/6/4US are calibration times, as confirmed by example comments (BIAS8uA, AZ requires4us); these are not startup bounds. SDK OPA_AutoZeroSoft_Stop documents a10us delay after calibration stops.
- SDK OPA internal-to-ADC table claims channels13/12 carry OPA outputs, but the selected manual ADC channel table section25.5 p578 and section25.12.4 p591 identifies these as internal DAC outputs. Do not add OPA internal ADC routing from that SDK table. Existing analog-pad ADC routes can observe PB00/PB01 where ownership is explicitly coordinated.

## Validation boundary

This report establishes source evidence and conservative software contracts only. It does not establish electrical performance, calibration accuracy, startup/settling maxima, or on-board behavior. No HAL tests, builds, firmware execution, or hardware measurements were run for this source-only assignment.
