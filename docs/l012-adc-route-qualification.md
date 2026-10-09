# CW32L012 dual-ADC route qualification

Qualified on 2026-10-08 from this family's own official sources. This is
source-level verification, not silicon validation or electrical qualification.

## Primary evidence

The analog sidecar `cw32-data/af/cw32l012-analog.yaml` pins the following sources
by SHA-256. Independent source identities also live in the generator's
`af/l012_adc/sources/mod.rs` and the source verification test.

- [CN V1.4 reference manual](https://www.whxy.com/uploads/files/20260603/CW32L012_UserManual_CN_V1.4.pdf):
  Table 25-4, printed page 578 / PDF page 604. Both ADC columns and their merged
  PB10/PB02 cells were visually reviewed from a rendered original PDF page.
- [CN V1.0 datasheet](https://www.whxy.com/uploads/files/20250717/CW32L012_DataSheet_CN_V1.0.pdf):
  Table 5-2, printed pages 32-34 / PDF pages 35-37. All three pages were rendered
  and reviewed for analog labels, physical positions and pin type. The PDF has
  three front-matter pages; the reference manual has 26. These offsets are not
  borrowed from another CW32 family.
- [Standard Peripheral Library V1.0.5](https://www.whxy.com/uploads/files/20260701/CW32L012_StandardPeripheralLib_V1.0.5.zip):
  `Libraries/inc/cw32l012_adc.h`, macro lines 222-233; ADC1 comments 187-198 and
  ADC2 comments 205-216. Macro encodings, comments and both PDFs agree.
- The existing own-family `cw32-data/pinouts/cw32l012.yaml` supplies independently
  qualified exact-package positions and the common-intersection alias policy.

Every route retains the ADC instance, source signal spelling, independent mux
value, SDK macro/comment line, manual table cell, datasheet row/page, and exact
package positions. The generator emits `adc_mux` explicitly and never derives
the hardware selection from a parsed signal label.

## Qualified external routes

- ADC1 mux0-9: PA0, PA1, PA2, PA3, PA4, PA5, PA6, PA7, PB0, PB1.
- ADC2 mux0-9: PA5, PA6, PA7, PB0, PB1, PA8, PA9, PA10, PA11, PA12.
- Both converters: mux10 is PB10; mux11 is PB2.

Both exact parts, CW32L012C8T6 (LQFP48) and CW32L012C8U6 (QFN48), expose all 12
routes per converter. The generic CW32L012 alias is their actual intersection
and also exposes 24 instance-qualified routes. There are 17 distinct physical
pads: PA5/6/7, PB0/1/10/2 belong to both converters. For example, PA5 is ADC1
IN5/mux5 and ADC2 IN0/mux0. Peripheral identity is part of the route key.

Mux12 and mux13 are internal DAC_OUT2 and DAC_OUT1 connections. They are not
external GPIO channels even though PB1/PB0 also carry the DAC output pad
functions. Mux14/15 are internal TS/BGR channels and are outside these external
route records. PB11, debug pads and other undocumented routes are not exposed.

## Shared hardware constraints for the HAL

These findings do not by themselves authorize HAL behavior:

- RM section 25.11, printed page 588 / PDF page 614, places ADC1 at 0x40000000,
  ADC2 at 0x40000100, and shared BGR_CR at 0x400000FC. The generator checks each
  converter's address and its own `adc/cw32l012_v1/ADC` register identity before
  it projects either converter's routes.
- SDK `cw32l012_sysctrl.h` uses one APBEN1 bit0 ADC gate and one APBRST1 bit0 ADC
  reset: lines 445, 460, 474, 546 and 560. Reset/gate writes cannot be treated as
  independent per-ADC ownership operations.
- RM section 25.12.19, printed page 599 / PDF page 625, states that BGREN, ADCEN,
  TSEN, VC1/2/3/4 and OPA1/2 enables automatically start shared BGR. Only a
  software disable or POR disables BGR. BGR and TS require about 30 microseconds
  to settle after enabling. Disabling BGR on one converter's drop can affect
  other analog users.
- RM section 25.12.3, printed page 590 / PDF page 616, requires at least
  40 microseconds sampling duration for either internal BGR or TS channel.

## Checks

Run the source check before generated metadata is refreshed:

```sh
python tests/verify_l012_adc_routes.py --sources /path/to/cw32-sources --sidecars-only
cargo test --locked -p cw32-data-gen af::l012_adc
```

After normal data generation, omit `--sidecars-only` to verify the 24 generated
routes for each exact part and the generic alias; `--data-dir` can target an
uncommitted output directory. The source check rehashes primary files and the
pinout sidecar, independently parses original PDF pin-table grids and manual
mux rows, and checks exact SDK lines. No vendor PDF or SDK is redistributed.

Generator regressions cover all exact packages/alias, shared pads with different
mux meanings, DAC/internal-channel exclusion, source and package substitutions,
wrong ADC names/addresses/register versions, duplicate/missing instances or
routes, duplicate/undocumented package pins, and all-or-nothing mutation when
either converter's validation fails.
