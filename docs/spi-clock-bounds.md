> Historical software-verification references: HAL tests and fixture harnesses cited below were deleted on 2026-10-08. Their reported results apply to the dated source snapshot only; source facts remain unchanged. See [current HAL layout and build scope](hal-production-layout.md).

# Source-qualified SPI master clock bounds

The SPI divider solver consumes `Clocks::pclk_bounds()` and preserves its exact
source numerator and cumulative hardware divider. It chooses the fastest
supported SPI divisor for which the upper SCK endpoint is no greater than both
`Config::frequency` and the existing family cap. It never uses a rounded nominal
rate to prove either ceiling. The RCC implementation is shared with ADC; SPI
contains no independent oscillator percentage table.

`get_current_frequency()` remains a nominal, rounded-down `Hertz` result.
`get_current_frequency_bounds()` returns the separate `rcc::ClockBounds`, with
outward-rounded minimum/maximum accessors and source temperature/supply
qualification. Construction and idle `set_config` use the same bound-aware
validation. Prior nonzero, nominal PCLK-relative request-range, family-cap,
sampling-delay and input-pull errors remain intact. A request above nominal
PCLK/2 (PCLK/4 for the five shared-L031 families) remains `FrequencyTooHigh`;
this is the existing API range policy, separate from the actual-clock safety
comparison. At an exact nominal endpoint, a coarser divider can be necessary:
for example, an 8 MHz nominal PCLK and 1 MHz ceiling on power-of-two SPI now
select /16 (500 kHz nominal), because /8 would permit more than 1 MHz. L012
can instead use its finer even-divider steps when permitted. A low request that an ideal source could attain can now return
`FrequencyTooLow` when even the largest divider's upper endpoint is too fast.

These guarantees require factory trim, unchanged RCC registers, and each
family's documented ambient TA/supply interval. They do not imply measured
frequency or qualification outside those intervals. In particular, do not
extend an 85 C HSI bound into a 105 C part-temperature tail, or a 105 C bound
into a 125 C general operating rating. Board voltage/frequency, rise/fall,
loading, slave timing and signal integrity remain independent obligations.

## Preserved own-family policies

Each family's own datasheet and manual, original URL, SHA256, printed/PDF page,
and reproducible source-page extraction hashes are indexed in
[`spi-clock-source-policy.json`](spi-clock-source-policy.json). Canonical
oscillator facts remain in [`adc-clock-source-bounds.json`](adc-clock-source-bounds.json).
No sibling's HSI envelope is borrowed because register layouts happen to match.
Raw source-page text is acquired/extracted locally and is not bundled. The retained
F020 filename V1.3 source actually prints Rev 1.2; its original URL and superseding
Rev 1.3 identity are explicitly recorded in the source policy and central catalog.

- F030/A030 retain the existing 16 MHz electrical-table policy, /2 through
  /128. Their feature summaries also say 12 Mbit/s; this change does not
  reinterpret that existing policy. No accepted 16 MHz absolute-overrun
  witness was established for their supported HSI/divider paths.
- F002/F003/F020 retain conservative 12 MHz, /2 through /128. Their 12 Mbit/s
  summary versus 16 MHz electrical-table conflicts are not resolved here.
- L031/R031/W031/L052/L083 retain conservative 12 MHz and /4 through /128.
  Their manuals' feature lists restrict master rate to PCLK/4 despite the
  register tables describing /2. The prohibited /2 encoding stays unused.
- L010/L011 retain 24 MHz, powers of two /2 through /256. Their own feature
  summaries say 24 Mbit/s; the 41.6 ns master-period rows are conditional on
  PCLK=48 MHz and their characterization conditions. These facts are not a
  claim of observed electrical failure at every voltage/load.
- L012 retains 24 MHz, every even divisor /2 through /256. Its own datasheet
  CN V1.0 §4.21 printed p27/PDF p30 explicitly gives master SCK up to 24 MHz.
  The malformed period/frequency/unit row in Table 7-39 is not reinterpreted
  as a 24 ns limit. UM CN V1.4 §22.7.1 printed p516/PDF p542 defines
  `SCK=PCLK/[2*(BR+1)]`, BR=0..127.

The reviewed source sections establish no additional absolute master minimum
SCK or source-clock frequency beyond the legal divider/source-relative
constraints above. Slave timing limits are not imported into the master
solver. The lower bound is reported for callers but introduces no invented
minimum-SCK requirement. Low-power input filtering and frame gaps remain off,
so their optional source-relative restrictions are not silently activated.

### Concrete L012 regression

HSIOSC 96 MHz divided by 2 yields 48 MHz nominal PCLK. At a 24 MHz request the
previous /2 selection yields 24 MHz nominal SCK but permits 24.48 MHz at the
qualified upper HSI endpoint. The new selection is /4 (BR=1), yielding
12 MHz nominal SCK and an 11.76..12.24 MHz envelope. PCLK's 48.96 MHz upper
endpoint is below the device's 96 MHz nominal system ceiling; this SPI
counterexample does not depend on a separate full-speed RCC overrun claim.

Older 12 MHz examples demonstrate an adopted-policy overrun only, not an
unambiguous violation of the conflicting 16 MHz electrical maximum.

## Unchanged behavior and limits

No SPI transfer engine, transaction ordering, CS handling, ownership,
peripheral/pin metadata, word widths, CPOL/CPHA semantics, DMA/async support,
register-enable sequencing or recovery policy changes. Every receive frame is
still drained and successful blocking transfers wait for TXE and BUSY clear.
Chip select remains externally managed. Existing blocking waits are unbounded;
this patch adds no wall-clock timeout or hardware-fault recovery guarantee.
No UART, I2C, timer, RCC or ADC production file is owned by this patch.

## Verification

- `embassy-cw32/src/spi/tests/clock_bounds/mod.rs` exercises the real
  `rcc::Config::frequencies()` -> PCLK bounds -> production SPI selector for
  every supported HSI/AHB/APB combination on all 13 families. It checks each
  SPI encoding's exact upper-endpoint boundary and neighboring integer requests,
  zero/range/overflow edges, fastest-safe selection, nominal separation,
  outward endpoint rounding and preserved source qualifications.
- Expected rates use independent integer cross-products with the canonical
  source envelope. They do not call the production comparison helper. The
  original register/protocol vectors use test-only ideal clocks and retain
  their former answers; these are separately identified from production HSI
  tests. The L012 regression is an explicit test.
- `tests/verify_spi_clock_bounds.py` matches production-reported source bounds,
  qualifications, supported HSI divisors, SPI caps and divider limits to each
  own-family source policy. It verifies retained extract hashes; setting
  `CW32_SPI_SOURCE_ROOT` also replays original PDF SHA256 identities.
- `tests/test_spi_clock_bounds_contracts.py` compiles all 13 family APIs on
  Cortex-M0+ with `defmt`, preserving nominal `Hertz`, returning distinct
  `ClockBounds`, and retaining blocking traits. Each positive control precedes
  four exact-diagnostic negative controls, for 52 intentional failures.
- Existing SPI register/protocol tests and ARM ownership, mode, instance,
  route and word-width contracts are rerun; results are recorded in the
  handoff's verification manifest/logs.

These are source, arithmetic, host-model, RAM-register and compile-only
checks. No firmware was flashed or executed on a device, and no electrical
or silicon behavior is claimed validated. Independent review remains an
acceptance requirement.
