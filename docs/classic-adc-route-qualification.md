# Classic ADC route qualification

Source qualification on 2026-10-08; no silicon validation. Scope: F002/F003,
L031/R031/W031, L052/L083 external ADC routes. The existing F020/x030 routes now
also emit explicit hardware mux metadata. No additional timer routes are enabled.

## Source identities and mappings

Each `cw32-data/af/cw32*-analog.json` pins its own datasheet, reference manual,
SDK archive, ADC header and exact-package pinout sidecar. The generator also pins
these identities independently in `af/classic_adc/profiles/mod.rs`. Every route
retains source signal spelling, SDK macro and line, manual table/mux cell,
datasheet pin row/page, all exact package positions, and any oscillator aliases.

- F002/F003: mux0–12 are PB2, PA1, PA4, PA6, PA7, PC0, PC1, PC2, PB0, PB1,
  PB6, PB5, PB3. Datasheets call these ADC_AIN0–12.
- L031/W031: mux0–7 are PA0–7; mux8–12 are PB0, PB1, PB2, PB10, PB11.
- R031: ADC_IN0–8 are PA4–7, PB0, PB1, PB2, PB10, PB11, with hardware mux4–12.
  Generated PA4 remains signal IN0 and carries adc_mux4. Neither source identity
  nor encoding is inferred from the other.
- L052/L083: mux0–7 are PA0–7; mux8/9 are PC4/5; mux10–12 are PB0/1/2.
  Datasheets call these ADC_AIN0–12.

W031's SDK ADC header incorrectly comments channels8–12 as PC4/PC5/PB0/PB1/PB2.
Its own datasheet Table5-2 and own RM Table22-5 independently agree on
PB0/PB1/PB2/PB10/PB11. The sidecar records all five discrepant comment pins and
selects the two agreeing primary documents. The SDK macros correctly encode8–12.
No SDK comment is silently rewritten or imported from another family.

## Exact package counts

- F002: both QFN20 and TSSOP20 have13; family alias13.
- F003: QFN20, TSSOP20 and TSSOP24 have13; family alias13.
- L031: TSSOP20 has9, QFN20 has7, both QFN32 parts have10, QFN48/LQFP48 have13;
  family alias7, the actual common intersection.
- R031: QFN48 and alias have9.
- W031: QFN64 and alias have13.
- L052: LQFP48 has11, both LQFP64 parts have13; family alias11.
- L083: all five exact parts and alias have13.

Package matching is exact; no largest-die fallback is accepted. Debug, reset,
input-only and unbonded pads are rejected. Analog routes never have digital AFs.
Internal channels, electrical limits and shared analog ownership belong to the
separately qualified HAL backend; these route records do not authorize them.

## Verification receipts

`tests/verify_classic_adc_routes.py --sources /workspace/shared/cw32-sources`
rehashes all primary sources and pinout sidecars, independently reads each own
PDF pin-table grid's analog labels/physical positions, checks manual mux rows,
and checks exact SDK macro/comment lines. All seven families pass. The same
script checks all generated exact parts and alias intersections.

Generator tests cover source/mux/channel substitutions, wrong pin/peripheral/AF,
SDK/manual/datasheet/package provenance, altered source identities, invalid
register versions, wrong alias policy, duplicate/missing routes, and failure
without partially mutating a core. R031 mux4 versus source IN0 and W031's copied
SDK-comment pins receive explicit regressions.

Serde tests prove old digital/analog pin records deserialize with no mux and
serialize without adding a null field, while R031 IN0/mux4 round-trips intact.
Static metadata tests require ADC muxes, reject digital-route muxes, and preserve
R031's offset. Independent metadata contracts allow only this narrow documented
PeripheralPin extension and reject collapsed/missing R031 muxes. Parent HAL tests
also check every generated channel trait against the explicit metadata.

Logs are in `docs/verification-logs/classic-adc/`. Source and package checks,
45 generator unit tests,7 generation-boundary tests,4 serde tests, metapac
generator tests,21 metadata contracts, and static metadata tests for R031/F002/
L052/F030 passed. Generated/vendor parity, layout and data validation also passed.
Physical accuracy, settling and electrical behavior remain untested on hardware.
