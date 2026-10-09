# Classic GTIM PWM route qualification

Source qualification, 2026-10-08. This adds only reviewed CH1–4 output sidecars
for CW32F002/F003/L031/R031/W031/L052/L083. Original broad SDK candidate sidecars
remain `candidate-sdk-and-register-verified`; serial and analog qualification
remain separate. No silicon, electrical, capture, DMA, trigger or ATIM support is
established by this work.

## Primary evidence and independent verification

`classic-pwm-route-evidence.json` records each original PDF's numbered AF table,
actual column selector, pin row, function, PDF/printed page and bounding box.
Each retained sidecar route also records its exact original SDK macro, source
line, GPIO bank register and field, own-datasheet package row, and physical pin
positions for every supported package.

The original sources in `/workspace/shared/cw32-sources` were independently
re-read, not inferred from a sibling family:

- F002 datasheet CN 1.2; own manual CN 1.4; SDK 1.2
- F003 datasheet CN 1.9; own manual CN 2.3; SDK 1.7
- L031 datasheet CN 1.9; own manual CN 1.6; SDK 1.4
- R031 datasheet CN 1.2; own manual CN 1.3; SDK 1.1
- W031 datasheet CN 1.3; own manual CN 1.4; SDK 1.3
- L052 datasheet CN 1.3; own manual CN 1.5; SDK 1.4
- L083 datasheet CN 1.9; own manual CN 2.0; SDK 2.2

Exact original filenames, official source URLs and SHA-256 identities are in
each sidecar's `sources` and the evidence document. Datasheet AF cells are
independently parsed from positioned PDF words, using the actual numbered
columns including blanks. SDK macros are parsed independently from the original
header register assignments. Original package grids are re-extracted with
PDF-grid and independent Poppler physical-position comparison. The resulting
235 PDF cells and 249 SDK macros reproduce the previous read-only audit; none
of its SDK-only cells is accepted. Manuals and SDK archives are rehashed for
provenance, while timer behavior is qualified separately by the HAL work.

## Scope and exclusions

| Family | PDF PWM cells | Retained | SDK-only excluded | Safety excluded |
| --- | ---: | ---: | ---: | ---: |
| F002 | 15 | 13 | 3 | 2 |
| F003 | 18 | 16 | 0 | 2 |
| L031 | 20 | 20 | 0 | 0 |
| R031 | 13 | 13 | 7 | 0 |
| W031 | 16 | 16 | 4 | 0 |
| L052 | 56 | 48 | 0 | 8 |
| L083 | 97 | 89 | 0 | 8 |

All 215 retained routes are output-capable, non-debug, non-reset, non-BOOT,
non-radio and non-oscillator pads. The precise excluded tuples are recorded:

- F002 SDK-only: PB7/AF5/GTIM_CH1, PC3/AF5/GTIM_CH3,
  PC4/AF5/GTIM_CH4. They are absent from its own AF tables/package grids.
- F002 and F003 debug outputs: PA2/AF5/GTIM_CH3 and PA5/AF5/GTIM_CH4.
- R031 SDK-only: PA0/AF6/GTIM2_CH1, PA1/AF6/GTIM2_CH2,
  PA2/AF6/GTIM2_CH3, PA3/AF3/GTIM2_CH2, PA3/AF6/GTIM2_CH4,
  PB8/AF6/GTIM1_CH3 and PB9/AF6/GTIM1_CH4. The radio-reserved
  PA0–3 pads are also explicitly rejected by the generator.
- W031 SDK-only: PA15/AF2/GTIM2_CH1, PB3/AF2/GTIM2_CH2,
  PB4/AF6/GTIM1_CH1 and PB5/AF6/GTIM1_CH2. The generator also
  rejects radio-reserved PB3/4/5/6/13.
- L052/L083 oscillator exclusions: both AF alternatives on each of
  PC14/OSC32_IN, PC15/OSC32_OUT, PF0/OSC_IN and PF1/OSC_OUT.
  Their documented routes are preserved in exclusion evidence, but do not
  become PWM metadata without an explicit clock-pin ownership design.

F002/F003 use the actual `GTIM` peripheral, never a guessed `GTIM1` alias.
L031/R031/W031 have GTIM1–2, L052 GTIM1–3, and L083 GTIM1–4. L052
PA7/AF1 is GTIM2_CH1; L083's same coordinate is GTIM4_CH1. Those cells are
not interchangeable. Current normalized L083 register identity is
`gtim_cw32l031_v1`; its own-source route qualification remains independent.

## Exact-package projections

- F002 QFN20 and TSSOP20: 13 each; family alias 13
- F003 QFN20/TSSOP20: 13; TSSOP24: 16; family alias 13
- L031 TSSOP20: 9; QFN20: 6; LQFP32/QFN32: 14;
  QFN48/LQFP48: 20; family alias 6
- R031 QFN48: 13; family alias 13
- W031 QFN64: 16; family alias 16
- L052 LQFP48: 36; either LQFP64: 48; family alias 36
- L083 each LQFP64: 48; LQFP80: 68; LQFP100: 88; family alias 47

The L083 die has 89 retained alternatives, but package bonding is not monotonic
by lead count. The LQFP100 package exposes 88 of these, rather than all 89.
PF4/AF2/GTIM4_CH2 is present on the 64/80-pin packages but absent on LQFP100,
so it is also absent from the 47-route family alias.
Every exact-part route set is computed from its own pin grid. The family alias
must equal the set intersection of all its exact-package route sets.

F003 PB7/AF5/GTIM_CH1 is a useful package-negative case: absent on both
20-pin packages, present at physical position 12 on TSSOP24. F002 has no
qualified PB7 route at all.

## Generator and tests

`cw32-data-gen/src/af/classic_pwm/` validates own source identities and hashes,
the immutable canonical route-array digest, exact package provenance, actual
GTIM register identity, and actual selected GPIO AF register/field IR. Checking
the complete qualified array prevents a coherent substitution of an SDK-only
route, not merely malformed fields. Failure leaves the input core unchanged.
The separate F020 qualification is unchanged.

Source/sidecar checks (no generation or compilation):

```
python tests/verify_classic_pwm_routes.py --sources /workspace/shared/cw32-sources --sidecars-only
python tests/test_classic_pwm_routes.py
python tests/test_module_layout.py
```

After the coordinating build regenerates metadata, run the same verifier
without `--sidecars-only` to compare every exact-package and family-alias GTIM
projection. `tests/reviewed_metadata.py` now calls the independent qualifier for
these seven families. Rust tests cover canonical source identity, all package
projections, source/AF/channel/package mutations, all exclusion categories,
wrong register identities, missing GPIO IR and transactional failure.

This route-only change does not claim Rust tests, full HAL tests, or regenerated
metadata were executed; the coordinating task owns generation and compilation.
