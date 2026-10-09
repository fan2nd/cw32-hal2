> Historical software-verification references: HAL tests and fixture harnesses cited below were deleted on 2026-10-08. Their reported results apply to the dated source snapshot only; source facts remain unchanged. See [current HAL layout and build scope](hal-production-layout.md).

# I2C oscillator-envelope timing correction

Status: implementation and isolated validation candidate; awaiting independent
source/code review before integration. No firmware was flashed or executed.

## Shared source and qualification

Both classic and CW32L012 command/FIFO I2C now receive the frozen RCC
`ClockBounds` through `Clocks::pclk_bounds()`. They do not own or infer oscillator
percentages. `docs/adc-clock-source-bounds.json` is the common own-family source
policy; `docs/i2c-clock-bounds-sources.json` indexes all thirteen families' own
I2C manuals/datasheets and the additional NXP bus-specification limits. Original
PDF bytes are hash-verified against the canonical acquisition lock. The NXP
PDF is Rev. 7.0, published 2021-10-01, acquired 2026-10-08, Table 11 printed/PDF
page 44. Vendor PDFs and extracts are not redistributed in this deliverable.

The envelope assumes RCC-loaded factory trim, unchanged clock registers,
the source-qualified supply interval, and the published ambient-temperature
interval. In the common policy, F/A devices are qualified from -40 to +105 C;
L/R/W devices from -40 to +85 C. Some packages support a hotter operating
range, but that does not establish the same HSI accuracy above +85 C. Query
RCC `temperature_range_c()` and `supply_range_mv()` for the exact family bounds.
There is no temperature measurement or runtime check. Source guarantees do
not establish board electrical compliance or silicon validation.

## Classic controller

The source equation is `SCL = PCLK / (8 * (BRR + 1))`, BRR=1..255. BRR is now
selected from the upper PCLK envelope, rounded upwards. This preserves the
requested rate ceiling, including the own-datasheet 1 MHz controller ceiling,
over the qualified source interval. The register-dependent filter policy is
unchanged: FLT=1 for BRR<=9, otherwise FLT=0. Rounded whole-Hz upper PCLK is
safe here because the permitted ceiling times the integer divisor is integral;
it cannot reject an otherwise exact rational BRR comparison.

A 24 MHz nominal PCLK requesting 1 MHz previously chose BRR=2 and could
produce 1.02 MHz or 1.05 MHz. It now selects BRR=3, nominal 750 kHz. At an
8 MHz nominal PCLK requesting 100 kHz, BRR changes from 9 to 10, including
the corresponding documented filter transition. Fractional HSI division is
retained for nominal/range reporting by dividing the original RCC envelope.

The reviewed classic sources do not supply independent count-to-tLOW/tHIGH,
data setup/hold, or data-valid equations. This change guarantees the documented
divider-rate ceiling and preserves the filter rule. It does not infer a 50%
duty cycle, apply L012 equations to classic hardware, or certify those separate
electrical timings. Pull-ups, rise/fall time, load and board qualification remain
application responsibilities.

## CW32L012 controller

UM CN V1.4 Tables 23-1..3, printed pp534-535, remain the waveform model.
The solver searches every prescaler and low-count candidate and derives the
least valid high/START-hold counts. Endpoint handling is now:

- Upper PCLK: maximum SCL, minimum low/high/START-hold/START-and-STOP-setup,
  data setup and bus-free intervals, and worst-case rise-cycle quantization.
- Lower PCLK: maximum data-valid/acknowledge duration, including the configured
  worst-case SDA rise budget.
- Minimum zero-rise latencies: minimum durations and the fastest possible SCL.
- Maximum latencies: SETHOLD/CLKLO/DATAVD internal constraints and the slowest
  generated SCL reporting bound.

DATAVD=1 stays the shortest allowed hold. The own datasheet's data-hold minimum
is zero, while the manual independently requires DATAVD>=1. Filters keep their
0..15 fields, SDA>=SCL ordering and pre-prescaler count restrictions. All four
MCCR fields remain six-bit, BUSIDLE remains the documented minimum subject to
its high-count restriction, and the post-prescaler clock remains at least eight
times the generated bus frequency.

The existing stricter bus-spec limits are unchanged. In Standard/Fast/Fast+
modes, the selected low/high/START-hold/START-setup minima, in ns, are
4700/4000/4000/4700, 1300/600/625/600 and 500/260/260/260. These dominate the
own datasheet minima; Fast START hold retains its stricter own 625 ns limit.
Data setup is at least 250/100/50 ns. Data-valid maxima of 3450/900/450 ns come
from NXP Table 11, whose footnote 3 applies these maxima when SCL low is not
stretched. The solver conservatively budgets worst-case SDA rise within those
maxima. Board fall time, noise, electrical levels and arbitration/firmware stalls
are not modeled as physical timing guarantees.

Original regressions, with default 100 ns rise bounds and no filter:

| Nominal PCLK / request | Corrected P, LO, HI, HOLD, VD | Nominal zero-rise Hz | Generated envelope Hz |
|---|---|---:|---:|
| 8 MHz / 100 kHz | 0, 38, 40, 36, 1 | 97,560 | 94,457..99,513 |
| 12 MHz / 100 kHz | 1, 28, 31, 27, 1 | 96,774 | 93,333..98,710 |
| 24 MHz / 1 MHz | 0, 12, 9, 6, 1 | 960,000 | 840,000..979,200 |

The 12 MHz case increases PRESCALE because three cycles at the fastest
12.24 MHz cannot provide 250 ns data setup. The low-clock regression uses
HSI96MHz/32, AHB/4 and APB/1: at nominal 750 kHz, 783 ns SDA rise previously
fit the 3450 ns data-valid budget. At the slow 735 kHz endpoint it requires
3504.089 ns, so the new solver rejects it. A 728 ns rise bound fits; 729 ns does
not. Slowing only the requested bus frequency cannot repair the fixed minimum
DATAVD interval in this case.

## Public reporting and unchanged behavior

`Config::frequency` remains the requested ceiling. `get_current_frequency()`
reports the nominal configured rate, rounded down, rather than a measurement.
For L012 it is the nominal zero-rise rate. Both backends add
`get_current_frequency_bounds() -> (Hertz, Hertz)`, rounded outward:

- Classic: oscillator-only divider-generated range. Rise time, SI service delay
  and stretching may make the physical bus slower than its lower endpoint.
- L012: generated range including oscillator uncertainty and the configured
  SCL rise upper bound. Stretching, FIFO service stalls and another master's
  activity may make actual throughput slower than its lower endpoint.

The maximum remains the useful safety bound. No positive throughput lower
bound is promised in the presence of stalls. Constructors and `set_config`
validate against the same frozen envelope before timing MMIO. Transaction
engines, FIFO protocol, timeouts, recovery, cleanup, pin routing, ownership,
clock gating and async exclusions are unchanged.

## Verification

- Classic production arithmetic: 9,216 RCC-divider/request cases per family
  against exhaustive BRR search using exact source fractions. The HSI divisor
  sweep 1..32 is a superset of supported settings; every AHB/APB setting is
  included. ADC divisors are intentionally absent. All BRR values and the
  filter boundary retain their original ideal-clock coverage.
- L012 production arithmetic: 107,520 RCC-divider/mode/filter/rise cases,
  checked against exhaustive field search and endpoint rational inequalities;
  14,210 accepted configurations and 93,310 unrepresentable ones. Also 756
  legacy ideal-clock cases, all three original source regressions, and the
  lower-endpoint data-valid boundary. The original ideal tests retain their
  expectations by using the RCC test-only ideal-clock constructor explicitly.
- Existing protocol models and RAM-backed register checks pass unchanged.
  Host I2C suites run independently on all thirteen family features.
- ARM typed contracts cover F030/A030, all three L012 features, twelve remaining
  serial packages, all twenty-one shared-serial features and all three F020
  packages: 389 intentional failures retain their exact expected diagnostics.
- ARM firmware links exercise construction, reconfiguration, nominal and
  envelope reporting, and write/read paths for one exact package of each of
  thirteen families, with `rt` and `defmt`. ELF vectors and FLASH/RAM ranges
  are inspected. The link fixture uses size optimization and LTO; it never
  executes the firmware.
- `tests/verify_i2c_clock_bounds.py` rehashes original sources and independently
  checks own-family rate/formula/filter evidence, shared HSI qualification,
  own L012 minima, NXP limits, and runtime clock-envelope plumbing.
- `tests/test_module_layout.py` and `tests/test_cfg_names.py` enforce the module
  directory layout and hardware/version-based cfg constraints.

Run the existing I2C contract scripts and `tests/test_i2c_clock_bounds_links.py`
with the repository's official Rust toolchain, `CARGO_INCREMENTAL=0` and an
appropriately bounded target directory. Set `CW32_SOURCES` to the acquired,
hash-pinned original PDFs before running the source verifier. Integration must
rerun relevant aggregate gates after combining the separately owned RCC fix.
