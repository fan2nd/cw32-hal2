# Independent F002/F003 qualified HEX safety review

Review date: 2026-10-09. Read-only review of frozen implementation; no source-tree edits, uploads, firmware execution, flashing, mocks or silicon validation. Schema/API-shape acceptance for the three metadata definitions is a separate review.

## Disposition

No blocking runtime defect found within the documented one-time, continuously powered/clocked, electrically legal initialization contract. Accept only with the qualifications below; this is not acceptance of arbitrary inherited register states, live clock handover, clock-loss recovery, automatic fallback, or DeepSleep entry/resume.

The two documentation nits identified during review are resolved in the final freeze: `Error::ClockSwitchTimeout` now describes the requested source, and public `Config` documents legal entry, continuous electrical qualification of every bridge source, incoming Flash wait, LSI qualification, and IRQ/DMA/NMI quiescence. This reviewer independently reconstructed the old file by undoing exactly those rustdoc edits and matched its previous SHA256, proving no runtime lines changed. The retained source/bridge electrical assumptions must not be dropped from the release description.

## Frozen identity

Independent SHA256 verification found zero mismatches across all 962 manifest entries. Implementer confirmed 29 changed/added authored files. Initial reviewed freeze was subsequently superseded by the verified rustdoc-only refinement; final identities follow. All 962 final manifest entries were independently rechecked with zero mismatches.

- implementation-freeze.json: a9c9e5276044605976c566f48fa28f2b4f2533850bc38693dbd4f535ea674a2f
- source-manifest.json: 672a18839a33acd3d05e60067a8520470ccaf6201d457881b7c36f45d4f4bfad
- implementation.patch: 7a753ee62c9243ee1561ab4dc850355330cb5b2892bd10248773ae909da20dff
- Stage27 baseline digest recorded in freeze: b5795126c3cf57cebd1d9122783397baf1229a0c14ee89c6a528c4beb669c4fd
- f002_f003.rs: 772aa48cb07c809c918d8beeca6ac9e8774fa063794e782895d133b8ee30faa1
- embassy-cw32/build.rs: 431cae50eb2f92d8897f8e270c001b9f79fcd140da27ae156e71007306df8b5b
- docs/qualified-hex.md: c69ecf28f9ba147d64c04c5e13847654835efb7c73af70b78f278eccc1b6c720

## Independent original-source checks

Reviewed the actual local PDFs under `/workspace/shared/cw32-sources`, independently hashed them, and extracted text for inspection. The original URLs below come from frozen `sources/evidence-sources.json`; no live re-download was performed. No vendor PDF or extracted text is redistributed in this report.

- F002 RM CN V1.4: https://www.whxy.com/uploads/files/20240920/CW32F002_UserManual_CN_V1.4.pdf
  SHA256 e453b3f82d9d180a86cd5f7cb18d858d7f228afc8101d87c700ca9d5c02d5add
- F002 DS CN V1.2: https://www.whxy.com/uploads/files/20251230/CW32F002_DataSheet_CN_V1.2.pdf
  SHA256 6d0c5c37d069e5b4e33d394b0be6c53938868e9375e7b4bbbbdd40d62bb9d506
- F003 RM CN V2.3: https://www.whxy.com/uploads/files/20240920/CW32F003_UserManual_CN_V2.3.pdf
  SHA256 0fa58dac223add7f2ac1ee714a7df7db4e414e0f193601b80dda96a948bfc738
- F003 DS CN V1.9: https://www.whxy.com/uploads/files/20251226/CW32F003_DataSheet_CN_V1.9.pdf
  SHA256 5fe15321b3963472c2629030767cf61a0ce36063815b54add74025b5b13b95dd

Relevant original sections: both RMs §§4.3.3, 4.4.2–4.4.3, 4.5.1–4.5.5, 4.7 CR0/CR1/HEX, GPIO §8.5–8.6, AWT source selection and CR; DS external-input and HSI tables and general/I/O electrical conditions. F002 selected qualification pages are PDF 42/47/54/55/57/129 and DS 22/30/31/36/37/40; F003 counterparts are PDF 44/49/56/57/59/131 and DS 23/31/32/37/38/41. Transition and GPIO sections were inspected additionally.

## Findings

1. Admission and frequency arithmetic: `Hex::bounds`/`Config::frequencies` enforce nonzero ordered min <= nominal <= max, own RM/DS intersection 4–32 MHz for the entire actual envelope, conditions coverage, package presence and minimum-VDD bus ceiling. Negative tolerance at nominal 4 MHz and positive tolerance at nominal 32 MHz correctly fail. External envelopes do not acquire factory-HSI accuracy. HSI bounds remain separate. AHB/APB division and outward upper rounding are preserved; nominal 24 MHz + 30 ppm requires WAIT1. The maximum bus limits remain 24 MHz below 1.8 V and 48 MHz otherwise; WAIT2 is merely a bridge setting.

2. Waveform requirements are correctly declarations, not software validation: 40–60% duty, >=15 ns each high/low, <=20 ns each rise/fall, voltage levels and I/O ratings all apply together. A 32 MHz waveform with 40% duty violates the minimum pulse width. No default oscillator ppm is invented.

3. Independent AWT inputs: `configure` retained-owner inspection inspects AWT with restored APB gate and without resetting/reprogramming it. Active PB0 and PB1 are reserved independently of system PINMUX/HEXEN, including HSI-only requests. A configured/retained system HEX and AWT can reserve both pads. Active external AWT plus a new HEX request permits only exact inherited enabled/ready/mux/gate/pad reuse; otherwise it rejects before clock/pad mutations. No HEX/pad writes occur in the accepted reuse path. Active ETR with requested HEX rejects conservatively. Source bounds for the other AWT input are not fabricated.

4. Retained HSIOSC users: active AWT HSIOSC and active HSIOSC-filtered LVD reject necessary HSI trim changes. AWT LSI is not reprogrammed. Temporary LSI enable restoration preserves the original software request and does not require a retained LSI consumer to stop.

5. Transition: FLASH central gate and WAIT2/readback precede source/divider changes. Guard AHB is max(inherited,/4), APB max(inherited,/8), so inherited stronger divisors are never weakened. Unchanged HSI is enabled/read before leaving the inherited source. Trim changes use unchanged legal LSI, stop HSI and await both enable-clear and !STABLE, then trim/start/read/select HSI. Live HSI DIV writes preserve trim and are supported by the dedicated RM §4.5.2 flow. HEX changes occur only after reaching HSI, clearing HEXEN and awaiting !STABLE; pad and PINMUX/PINEN are set before enabling/waiting. Selected source and final buses are read back before final actual-HCLK latency reduction. DSB/ISB is applied at switches and bus/latency boundaries.

6. Pad programming: generated F002/F003 path uses its actual GPIO version, with no invented LOCK write at +0x3c. DIR disables output first, then pad-local pull/IRQ/open-drain/filter/AF/analog bits are configured. RMW preserves other pins and shared filter-clock bits. Frozen reservation is checked by Flex::new before gate/pad writes, protecting safe GPIO-based drivers after successful init. Raw PAC use can invalidate that contract.

7. Clock-loss semantics: own STABLE is explicitly startup-only and remains asserted after later source failure. Own CR1 has HSIEN/HEXEN/LSIEN, with bit 2 and 15:4 reserved. Implementation does not turn copied SDK CCS/LSELOCK helpers or generic automatic-switch prose into a usable automatic fallback. Bounded iteration polls cannot guarantee wall-clock progress after the selected external CPU clock disappears.

8. Entry/failure qualifications: incoming and unchanged bridge sources must be electrically legal and continuous; STABLE cannot qualify unknown trim. Incoming Flash wait must already be adequate. LSI bridge must retain its documented 32.8 kHz +/-10% range. Normal interrupts are masked, but DMA/NMI/other clock-dependent users must be quiescent by caller contract; no claim that critical-section masks NMI is made. Invalid pure configuration fails before singleton acquisition. RCC failures after acquisition expose no tokens and publish no RCC clocks; reset is required. A later time-driver failure can leave verified RCC clocks published without tokens, an existing explicitly documented exception.

## Verification boundary

This reviewer performed source/hash and original-manual inspection, not a second production build matrix. Builder build/data results must remain separately identified. Three metadata schema definitions, provenance-chain/legal ancestry, source redistribution clearance, analog waveform measurement and silicon behavior are not approved by this report.
