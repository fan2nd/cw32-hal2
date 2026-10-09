# F020 ADC/PWM route qualification

Source-level qualification, 2026-10-08. No hardware validation is claimed.
This change promotes only the own-F020 ADC pin map and GTIM CH1–4 output
routes. It does not establish analog accuracy, electrical safety, pulse edge
quality, oscillator tolerance, or any additional ADC/timer operating mode.

## Independently checked primary evidence

The machine-readable source inventory and complete 73-cell GTIM comparison are
in `f020-adc-pwm-route-evidence.json`. All 15 F020 source hashes retained by the
prior audit were rechecked against the acquired files, without changing the
prior audit snapshot.

- Current F020 datasheet, printed Rev 1.3, SHA-256
  `1e330d800f10114c654b97cbd45b64d56a99c3cb39138d4ef20de3c12968dab0`.
  Tables 5-3–5-6, printed 26–27 / PDF 27–28: all 73 GTIM AF cells
  independently parsed by numbered table column and matched to the own SDK.
  Exactly 46 are CH1–4 outputs; the 27 ETR/TOGP/TOGN cells remain unpromoted.
- Own reference manual CN V1.4, SHA-256
  `279521eaf7d32b241e614a9553044a4d99c1e7ca2566efa3b0fdd2b66c106eed`.
  Table 21-5, printed 377 / PDF 378: every external hardware mux value and
  AIN source label independently extracted and checked against the SDK.
- Own SDK V1.2 archive, SHA-256
  `1d77fece47a0c615c8ae51374ea946b17ab489042222f33e38d93e6969945b5d`.
  GPIO header SHA-256
  `eccc3bb68452d2e3218397b2a795c4330e4a3d9b5a76481de128655e4602874d`:
  exact macro names, source lines, target GPIO fields, and AF selector values
  checked for all 73 source GTIM cells. ADC header SHA-256
  `e3b36174dea21de3d8d74c248c18c59450056728dbebce25751a55c92f3dd0cf`:
  pin comments at lines 189–201 and mux macros at lines 206–218 all checked.
- Current datasheet table 5-2, printed 22–23 / PDF 23–24: independent
  PDF-grid extraction checked all 13 ADC signal labels and physical positions
  in all three package columns. Every analog and PWM source route then joins
  to the existing hash-pinned, reviewed physical pinout sidecar, including
  its exact table row, source pin label, page, type and position.

The own-F020 external map is PA0–PA7 → mux 0–7, PB0/PB1/PB2 → mux 8/9/10,
PB10/PB11 → mux 11/12. The sidecar preserves the datasheet's ADC_IN labels,
the manual's AIN labels, the hardware mux encoding, and the emitted IN signal
separately. Their numeric correspondence is proven for F020 only.

## Discrepancies and boundaries

1. The source-root file named `CW32F020_DataSheet_CN_V1.3.pdf` is printed
   Rev 1.2, hash
   `9fe3f5cf054612faf3b94de3e0f4166886e7b7ab2a43ebced009a48270aaca91`.
   Qualification uses the current Rev 1.3 copy and rejects substitution of
   the older hash. Filename equality is not source-version evidence.
2. Copied ATIM cells in the datasheet do not create an F020 ATIM instance.
   The source sidecar permits only GTIM1–4 CH1–4; ATIM and all other timer
   signals remain excluded. The candidate AF sidecar is unchanged.
3. SDK/SVD 14-bit ADC extension names do not establish a safe 14-bit HAL
   mode. The route work supports only the independently documented external
   mux map. Internal-source operation, external-reference ownership,
   conversion policy and driver support are separate implementation reviews.
4. Oscillator-capable PF0/PF1/PC14/PC15 have documented PWM alternatives and
   are included in the source-route counts. Their oscillator aliases are
   retained explicitly. A route does not authorize clock reconfiguration
   or taking an already clock-owned pin.

## Exact projections

| Exact part | Package | ADC routes | PWM routes |
| --- | --- | ---: | ---: |
| CW32F020F6U7 | QFN20 | 9 | 17 |
| CW32F020K6U7 | QFN32 | 11 | 32 |
| CW32F020C6U7 | QFN48 | 13 | 46 |
| CW32F020 alias | Common-package intersection | 9 | 17 |

Per-route package positions are explicit, including null for unbonded pins.
The alias has no fabricated physical package. PA13/PA14 SWD, dedicated NRST,
and input-only PF3/BOOT cannot become ADC or PWM routes. The projection also
requires the actual reviewed ADC `cw32f020_v1` and GTIM `v1` identities.

## Files and integration

- New canonical sidecars: `cw32-data/af/cw32f020-analog.yaml` and
  `cw32-data/af/cw32f020-pwm.yaml`.
- The F020 input selects those sidecars using `analog_metadata` and the
  additional `pwm_metadata` field. Its existing serial selection is unchanged.
- `cw32-data-gen/src/af/f020/mod.rs` qualifies the source tuples and joins
  exact package positions before projecting them. It validates the entire
  sidecar before publishing any modified core. The generic serial route
  engine then checks the actual GPIO IR field offsets/selector widths.
- `cw32-data-gen/src/af/f020/tests/mod.rs` tests source qualifications,
  exact-package/common-alias projections and intentional invalid mutations.
- `tests/verify_f020_adc_pwm_routes.py` verifies the independent source
  evidence, canonical sidecars and regenerated chip route sets. With
  `--sources DIR` it also checks original source hashes, PDF table cells and
  header macros. `--sidecars-only` is available for pre-generation review.

Suggested aggregate checks:

```sh
cargo test --locked --offline -p cw32-data-gen
python3 tests/verify_f020_adc_pwm_routes.py
python3 tests/verify_f020_adc_pwm_routes.py --sources "$CW32_SOURCES"
```

The route implementation does not edit HAL or build-script files, existing
serial/candidate/pinout metadata, register templates, or generated trees.
Regeneration and actual typed HAL ownership/negative compile/link tests are
coordinated with the separate F020 backend change.
