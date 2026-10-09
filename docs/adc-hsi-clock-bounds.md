> Historical software-verification references: HAL tests and fixture harnesses cited below were deleted on 2026-10-08. Their reported results apply to the dated source snapshot only; source facts remain unchanged. See [current HAL layout and build scope](hal-production-layout.md).

# Source-qualified HSI timing for ADC

This correction separates nominal display frequencies from the oscillator envelope used for electrical validation. It covers all 13 currently supported families and their factory-calibrated HSI divider paths. No firmware was executed or flashed; host arithmetic/protocol tests and ARM compile controls are software evidence, not silicon validation.

## Operating contract and evidence

The canonical, independently reviewed facts and precise own-family PDF citations are in `adc-clock-source-bounds.json`, resolving source IDs through `sources/evidence-sources.json`. The source verifier rehashes each cited artifact and reads each family's actual HSI table. F020 uses the current printed Rev1.3 datasheet, SHA-256 `1e330d800f10114c654b97cbd45b64d56a99c3cb39138d4ef20de3c12968dab0`, not the older Rev1.2 document with a misleading V1.3 filename.

| Families | Factory HSI | Qualified ambient TA | Relative envelope |
| --- | --- | --- | --- |
| F002, F020 | 48 MHz | -40 through +105 C | -5% through +5% |
| F030, A030, F003 | 48 MHz | -40 through +105 C | -2% through +2% |
| L031, R031, W031, L052, L083, L010 | 48 MHz | -40 through +85 C | -2% through +2% |
| L011, L012 | 96 MHz | -40 through +85 C | -2% through +2% |

Each family has its own evidence; equal numerical values are not cross-family substitution. The public `rcc::HSI_BOUND_TEMPERATURE_C` and `HSI_BOUND_SUPPLY_MV`, also available on every `ClockBounds`, identify its conditions. TA is ambient temperature, not junction temperature. The board must separately satisfy all thermal, VDDA/VDD, reference and pin restrictions. No voltage, temperature or clock frequency is measured by this HAL.

Some F/A general operating tables permit +125 C at low dissipation, but their factory-HSI accuracy stops at +105 C. Some L tables permit +105 C, but HSI accuracy stops at +85 C. HSI-based ADC timing outside the qualified interval is unqualified. The caller must remain inside it; there is no guessed wider default and no temperature field that pretends to enforce a physical condition.

Bounds require the documented factory trim loaded and checked by RCC. Arbitrary TRIM, AUTOTRIM, unverified inherited sources, external oscillators and future PLL paths are outside this contract. The typical 0.2% trim step is neither an additional error term nor evidence for a tighter tolerance. The table min/max values are characterized bounds under specified conditions; they do not imply every unit was production-tested at every corner or actually reaches an endpoint.

## Exact propagation and public API

`Clocks.hsi`, `.sys`, `.hclk`, and `.pclk` remain nominal whole-hertz floors. New `hsi_bounds()`, `sys_bounds()`, `hclk_bounds()`, and `pclk_bounds()` carry the factory source's nominal/minimum/maximum numerators and exact cumulative hardware divisor. They do not derive an envelope by multiplying an already rounded Hertz value.

`ClockBounds.nominal()` floors only for display, `minimum()` rounds down and `maximum()` rounds up. Internal maximum/minimum/acquisition checks retain exact rational values. All documented HSI, AHB and APB dividers are covered, including fractional /7, /14 and /28 paths. A bound cannot be constructed, converted from an arbitrary Hertz value or divided by an arbitrary factor through the public API.

The public ADC `Timing.frequency` and `conversion_time_ns()` remain explicitly nominal. `Timing.clock_bounds`, `minimum_acquisition_time_ns()` (rounded down), and `maximum_conversion_time_ns()` (rounded up) expose separate guarantees. Startup, settling, trigger latency and software overhead are excluded from conversion duration.

## Electrical validation and waits

- Every ADC clock ceiling and requested `Config.frequency` ceiling uses the fast source endpoint. The requested frequency is a maximum actual ADCCLK; the selected nominal rate may therefore be lower than before.
- Classic voltage/reference ADCCLK ceilings also imply the corresponding maximum sample rates at all supported 24..29 total conversion cycles. The independently stricter 200 kSPS follower limit and 5 us temperature-acquisition floor use the fast endpoint too. F002 retains its VDDA/3 channel and still exposes no unsupported temperature or bandgap channel.
- L010/L011 sequence and L012 dual ADC enforce their own voltage-dependent maximum ADCCLK, maximum sample rate and external-acquisition requirements. Internal channels retain at least 40 us acquisition. Only L011/L012 enforce the documented 4 MHz minimum using the slow endpoint. No minimum is invented for L010 or classic ADCs.
- A delay meant to last at least a wall-clock interval uses the fast HCLK endpoint to calculate CPU cycles. Startup waits involving 15 ADC cycles conservatively combine fast HCLK with slow ADCCLK. The common source makes this conservative rather than optimistic; temporary bounded variation cannot shorten the promised interval.
- The existing configurable `timeout` remains a finite register-poll budget, not a wall-clock duration or an assurance that conversion must finish before it expires. Slow-endpoint conversion-time introspection is available separately. A too-small budget can return the existing timeout errors; cleanup and retry behavior are unchanged. No arbitrary conversion of polls into microseconds was introduced.

No channel traits, ownership rules, register masks, comparator/BGR retention or L012 sibling/slave exclusion rules were changed. Validation still precedes ADC mutation, except the already documented external GPIO preparation before per-read timing rejection.

## Boundary regressions and defaults

At nominal 24 MHz PCLK, classic ADC /1 previously admitted actual 24.48 MHz, or 25.2 MHz for F002/F020. It now selects /2. The classic 5-cycle temperature conversion at nominal 1 MHz previously yielded less than 5 us at positive HSI tolerance; it now selects a slower legal divisor.

L011's default 4 MHz PCLK and L012's default 8 MHz PCLK with default 6 MHz ADC request can only select a nominal 4 MHz ADC. Its guaranteed lower endpoint is 3.92 MHz. Both now return `ClockBelowMinimum`. ADC construction never silently changes RCC to hide this failure.

A qualified low-voltage setup for L011/L012 is to choose RCC HSI /10 with AHB /1 and APB /1 before initialization. Nominal PCLK is 9.6 MHz, and default ADC /2 is 4.8 MHz with guaranteed [4.704, 4.896] MHz. The longest acquisition setting preserves both internal channels at this setting. Callers can choose another source/divisor combination satisfying their own board requirements. L010's default nominal 4 MHz PCLK now selects nominal 2 MHz ADC when the low-voltage 4 MHz ceiling applies; L010 has no documented 4 MHz minimum.

## Reproducible checks

Run `ci/check-adc-clock-bounds.sh` using the repository's approved Rust toolchain and acquired vendor sources (`CW32_SOURCES` may override their directory). It runs:

1. Hash- and page-verified own-family HSI evidence and policy checks.
2. All 13 complete host HAL unit suites, including real-RCC property tests across all supported HSI/bus/ADC divisors, sample times, voltage boundaries and available channel classes.
3. Existing ARM ownership/channel controls for every family, plus positive ClockBounds/Timing API controls and diagnostic-checked attempts to forge bounds or substitute nominal clocks.
4. Module layout and hardware-CFG checks.

Protocol tests may inject an exact ideal clock only through a test-only constructor to isolate existing state-machine behavior. Separate production arithmetic tests always use the real RCC envelope and independently assert the fast and slow endpoint inequalities. Public duration intermediates use u128 to avoid overflow at the slowest supported tree and maximum u32 cycle count. This does not remove the need for board-level analog validation.
