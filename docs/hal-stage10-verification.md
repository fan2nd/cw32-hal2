> Historical software-verification references: HAL tests and fixture harnesses were deleted on 2026-10-08. Counts and commands below apply to the dated source snapshot. See [current HAL layout and build scope](hal-production-layout.md).

# Stage 10: qualified clock envelopes and in-project provenance

This corrective checkpoint supersedes earlier nominal-only RCC/ADC/I2C/SPI
boundary checks. It also includes the previously reviewed buffered GTIM and
L011/L012 RTC work. It is not complete all-peripheral support and has not been
run on a physical board.

## Clock and electrical corrections

RCC now accepts an explicit declared board VDD and ambient-temperature interval.
These declarations are not measurements. It validates interval ordering,
source-qualified supply/temperature ranges and actual upper HCLK/PCLK limits
before register access or peripheral-token acquisition. Slow defaults remain
valid. Final Flash wait states use the upper HCLK bound; conservative initial
waits and documented safe source transitions are preserved. Retained RTC/AWT
HSIOSC use is protected before recalibration.

The qualified factory-HSI envelope is family-specific: ±5% for F002/F020 and
±2% for the other families. Qualification is limited to −40…105°C for F/A and
−40…85°C for L/R/W. General device hot-operation tails do not establish oscillator
accuracy there. A narrower declared temperature interval does not silently reduce
these conservative error bounds. Source trim, supply and mode conditions remain
explicit, including W031's separate RF-DCDC requirement.

Own datasheet absolute HCLK/PCLK ceilings are 64 MHz for F030/A030/L083,
96 MHz for L011/L012 and 48 MHz for the other families. The lower-voltage
24-MHz ceiling is applied when the declared minimum VDD is below 1.8 V;
R031/W031 have higher supported supply floors. This qualified-envelope policy can
reject a nominal maximum HSI setting. It does not assert that every vendor nominal
maximum is universally unusable. A raw oscillator tuning range is not a license
to exceed a rated bus clock. Conflicting L012 WAIT4 prose is not used to permit an
otherwise over-limit bus configuration.

ADC uses the fast endpoint for maximum clock/rate and minimum acquisition, and
the slow endpoint for minimum clock and longest conversion/startup timing. CPU
settling delays account for the fast HCLK endpoint. Incompatible default
L011/L012 ADC configurations now explicitly reject rather than silently change
RCC. Runnable examples select HSI /10, giving nominal 9.6-MHz PCLK and a qualified
4.8-MHz ADC divider. Channel ownership and shared analog-resource behavior remain
unchanged. Poll budgets remain finite polls, not promised wall-clock deadlines.

SPI chooses a divisor against the actual upper endpoint and adopted family cap;
its nominal frequency accessor remains nominal, with a separate bounds accessor.
Classic I2C enforces the divider-rate ceiling without inventing undocumented
waveform equations. L012 I2C enforces minimum timing at the fast endpoint and
maximum data-valid timing at the slow endpoint. Clock stretching, board rise/fall,
slave behavior and signal integrity remain separate conditions.

See `adc-hsi-clock-bounds.md`, `rcc-operating-envelope.md`,
`spi-clock-bounds.md` and `i2c-clock-bounds.md` for exact contracts and source facts.

## Other included, bounded implementations

- Buffered GTIM counter/simple PWM on L010/L011/L012: 62 qualified die routes,
  at least two timer ticks per period, explicit whole-timer restart/peer-duty
  commit on mode or polarity changes; no waveform/glitch-free promise
- L011/L012 blocking whole-second RTC calendar: preserving attachment and
  explicit stop/write/restart, with /120 × /400000 default prescalers
  (800 kHz nominal intermediate clock, at most 816 kHz under +2%)
- Existing blocking ADC across all 13 families, classic GTIM across ten families,
  all-family BTIM/GPIO/serial/watchdog/CRC scope, and restricted Flash subset

The reviewed L010 timer-cascade/hardware-trigger candidate is deliberately not
included in this corrective snapshot. Timer routing/capture/encoder, safe DMA,
async time-driver, remaining RTC/Flash/comparator and accelerator/radio work
remain visible in `hal-coverage.json`.

## Measured integrated verification

The frozen main-tree matrix completed at 2026-10-08 13:39:49 UTC:

- All 54 HAL selections across 13 families; 108 optimized ARM configurations
- 13,982 unit + 24 IRQ-binding + 1 API + 144 documentation test executions
- Zero warnings; all 927 scoped inputs unchanged
- Matrix manifest SHA-256:
  `1346f0aed4349f36597afd34c642cff0439b0bee00b591b0459c8986f5ca6501`

The integrated operating-envelope suite completed at 13:40:15 UTC and full
`./d test && ./d check` completed at 13:41:43 UTC. Both preserved all 1,487 input
hashes. These include all-family source/state/configuration checks, 195 checks of
actual example clock setup, all 54 PAC builds/metadata tests and affected exact-
package GPIO/ADC links. Independent reviews separately reproduced ADC/RCC,
SPI/I2C arithmetic and ownership controls. Their reviewed patches were integrated
narrowly; final main-tree checks establish the combined boundary.

Logs and manifests are under `verification-logs/stage10-final` and
`verification-logs/stage10-integration`. Source, arithmetic, type-system and ELF
checks do not establish silicon behavior or replace board validation.

## Source provenance and reviewed replacement

Start with `build/provenance/SOURCE-CATALOG.md` and `build/provenance/reference-index.json`.
The canonical evidence lock records official URLs, filenames, actual printed
revisions/dates, hashes, chip scope, cited pages, transformations, corrections,
license status and upstream/toolchain/dependency pins. Unknown acquisition dates
remain unknown. The historically mislabeled F020 datasheet is distinguished from
its current revision. Fixed-byte acquisition closes previously missing source
members; raw vendor PDFs, SDKs and page extracts are not bundled.

Earlier schema/proc-macro adaptations had unresolved upstream license evidence.
Five affected live files have independently written replacements, reviewed for
functional compatibility. All 337 generated data and 471 PAC files remain byte-
identical after main-tree regeneration. Prior origin/hashes remain recorded;
retired source snapshots and patches containing deleted predecessor bodies are
excluded. This technical review is not legal clearance for the upstream code.
The package guard checks exact reviewed replacement identities and vendor-source
exclusions before producing an archive.

Use `./d provenance --write` after reviewed source changes, then `./d provenance`
and the documented source checks. Refresh never silently overwrites curated
hardware fixes. Runtime module layout remains `<module>/mod.rs`, and cfg names
continue to describe chips, peripheral versions or hardware capabilities.
