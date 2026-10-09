> Historical software-verification references: HAL tests and fixture harnesses were deleted on 2026-10-08. Counts and commands below apply to the dated source snapshot. See [current HAL layout and build scope](hal-production-layout.md).

# Stage 8: low-power and dual ADC, reserved-region Flash, RTC PAC corrections

This checkpoint completes the basic blocking ADC family scope across all 13
current data profiles. It does not complete every ADC mode or every peripheral.
No firmware was flashed or executed on silicon.

## Added drivers and ownership limits

L010/L011 sequence ADCs use package-qualified inputs, family electrical and
acquisition limits, and preserve the shared comparator/BGR clock resources.
L012 ADC1 and ADC2 borrow a `Common` BGR owner, preserve shared analog resources,
and reject inherited sibling synchronization before conversion. Conversion remains
software-triggered, blocking and single-slot. Scan, DMA, hardware triggers and
owned external-reference support remain unavailable. The stricter intersection
of conflicting manual/datasheet timing limits is enforced.

L031/R031/W031 Flash supports eight exact parts. Safe `ReadNorFlash` is provided;
program and erase require an explicitly unsafe exclusive reserved-region and
sustained operating-conditions contract. Unknown-capacity generic aliases are
rejected. The controller uses byte programming, 512-byte erase pages and temporary
4-KiB group unlocks with restoration. Post-trigger waits are intentionally
unbounded, because aborting an in-flight operation is not documented. There is no
`NorFlash` or `MultiwriteNorFlash` implementation: the published power-loss
isolation guarantee is insufficient for those traits. No option/protection/security
programming or asynchronous Flash API is exposed.

## Measured acceptance

The unchanged combined main-tree matrix completed at 2026-10-08 11:19:21 UTC:

- All 54 HAL selections across 13 families
- 108 optimized ARM configurations (`rt` and `rt,defmt`)
- 11,867 unit, 24 IRQ-binding, 1 API and 144 documentation test executions
- Zero warnings and 906 unchanged scoped inputs
- Source manifest SHA-256: `d1e6be64a785f5f3596fcd3d755caaf67940c5227d3b2891d3ddffa63f49c64b`

`./d test` and `./d check` both completed successfully, including authoritative
YAML/generated parity, source audits, all 54 PAC selections and metadata tests.
Supplemental merged-source checks include low-power ADC package contracts and
five exact-part ELFs, dual ADC 84 precise negative ARM cases and two exact-part
ELFs, and Flash 33 API negatives plus eight partition-checked ELFs. Independent
reviews matched the accepted implementation hashes. Logs and source manifests
are in `docs/verification-logs/stage8-combined`, `low-adc-merged`,
`l012-adc-merged`, and `flash-merged`.

These results establish source/model/type-system/build/link properties, not
physical ADC accuracy, Flash endurance/power-loss behavior, RF coexistence or
low-power retention. Official-source acquisition remains pinned and excludes
vendor SDKs/manuals from this archive; see `docs/evidence-acquisition.md`.

## RTC PAC corrections relative to Stage 7

Own manuals confirm eight affected profiles and five canonical templates:

- Remove obsolete COMPEN.FREQ fields on F020, L031/L052/L083/R031/W031
- Make F020 timestamp capture registers read-only
- Restrict L010/L011 DATE.DAY to bits 5:0 and MONTH to bits 12:8

The audit checked 159 register offsets and 667 field spans. Ten positive builds,
20 diagnostic-specific negative cases, two reserved-bit-preservation tests and
32 affected generated profiles passed. All 128 non-RTC templates remained
unchanged. See `docs/rtc-pac-corrections.json` and `docs/rtc-next-batch-audit.md`.

RTC HAL remains absent. ALARMB polarity documentation conflicts across all
RTC-bearing families and has not been guessed. Timer cascading, master/slave,
TRGO and hardware-triggered ADC still require instance-specific source routing;
no universal TriggerSource or invented internal route is exposed. Additional
GTIM family implementations are a separate pending batch.

## Structure and reproducibility

Rust modules retain `<module>/mod.rs` organization, hardware-specific cfg names,
and pinned upstream data/PAC generator architecture. `docs/hal-coverage.json`
records implemented and missing scope per family and peripheral. Source refresh
cannot silently overwrite curated register corrections. Build commands and
source acquisition requirements are in the root README.
