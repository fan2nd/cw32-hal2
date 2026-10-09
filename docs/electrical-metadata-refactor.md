# Source-qualified electrical metadata

2026-10-08. Project-authored extension to the pinned data/PAC/build architecture.
The upstream comparison is the actual local Embassy revision
`f16efeffe37581092ec184718e6fdb1620393214`: selected-chip metadata is consumed by
`embassy-stm32/build.rs`, while runtime hardware algorithms remain in drivers.
CW32 retains its own register identities and qualified clock arithmetic.

## Data flow

`cw32-data/electrical.yaml` is the versioned authored catalog. Each of its 13
profiles identifies its own family. It pins the existing source-audited policies
by SHA-256 and names the immutable original-source authority
`sources/evidence-sources.json`. Factory trim addresses additionally identify
the exact official SDK header, hash, physical source line and production-branch
qualification. The data generator validates the cited facts and emits typed
optional peripheral metadata:

- SYSCTRL `clock_limits`: factory HSI nominal/error, qualified supply/ambient,
  voltage-dependent bus ceilings, factory trim, default divider, initial WAIT
  and documented WAIT frequency quantum
- ADC `adc_limits`: supply, source-qualified minimum clock, sample/comparison
  cycles, voltage-dependent clock/rate/acquisition bands and internal-reference
  limits. Zero minimum denotes absence of a separately qualified minimum; it
  does not authorize a stopped ADC clock
- IWDT `iwdt_clock`: typical, slow and fast oscillator endpoints and LSI selection
- I2C `i2c_limits`: maximum SCL and the L012 qualified waveform timing table
- FLASH `flash_limits`: the currently qualified driver's supply/bus range,
  low-voltage limit, lock group/mask, cache-control presence and WAIT limits

The metapac generator carries these typed facts into selected-chip metadata.
The HAL build emits direct constants. There is no runtime registry, untyped
fact bag or new family-name cfg layer. ADC and I2C instances currently have
identical qualified limits within each selected chip; generation checks that
identity rather than silently choosing the first differing instance.

Optional fields deserialize absent as None and omit None during serialization.
This is an additive project-authored JSON extension. Adding fields is a Rust
struct-literal compatibility break for external code constructing these records;
it is not claimed to preserve source compatibility for such literals. Existing
project test constructors were updated. No upstream file is passed off as an
unchanged mirror; parent integration must refresh the combined provenance hash
with the concurrent SPI, GPIO-interrupt and RCC-control extensions.

## Preserved qualifications and arithmetic

`ClockBounds` implementation is byte-identical. It still retains exact fractions,
rounds public endpoints outward and applies rational comparisons/delay ceilings.
RTC's 97.92 MHz fast bound is derived from the same qualified nominal96MHz/+2%
facts. No supply, temperature or conservative source restriction is relaxed.
L011/L012 retain the manual's stricter48MHz ADC ceiling and4MHz minimum. L010
gets no invented minimum. All voltage band equality conventions are unchanged.

W031 FLASH remains2.0–3.6V to cover both RF DCDC and LDO supply modes, distinct
from its1.8V RCC/ADC qualification. The raw FLASH Vprog table alone starts at1.8V;
`docs/flash-storage.md` owns the existing stricter API policy. F002/F020 FLASH
lock-mask corrections remain8bits, F00310bits. L083/L01x FLASH storage remains
unsupported by this metadata projection, without changing their RCC WAIT facts.

The L012 I2C table retains the stricter existing combination of own datasheet,
manual and NXP UM10204 Rev7 timing constraints. No classic-I2C waveform equation
is newly claimed. The existing I2C evidence's F020 datasheet reference is a
historical filename alias; selected current F020 source identity is explicitly
recorded in the ADC/RCC/watchdog original-source policies. This refactor does not
reinterpret the existing1MHz value or label the legacy PDF as printedRev1.3.

## Moved versus intentionally retained

Moved fact cfgs: RCC bounds supply/temperature/error tables; RCC operating bus
ceilings; L010/L011 nominal HSI/default divider/initial WAIT facts; all backend
nominal HSI/trim-address/WAIT-quantum literals; ADC classic and sequence supply
facts; sequence voltage timing branches and L011-only numerical minimum; IWDT
oscillator/source table. Non-cfg chip facts in the same paths were also moved:
ADC sample/comparison cycles and reference/follower/acquisition limits, L012
voltage bands, I2C frequency/waveform limits and the Flash consumer constants.

Retained genuine register operations: HSI divider enum encoding/decoding, L083
PLL presence and mux validation, RCC temporary-LSI/RTC-preservation sequences,
keyed-write protocol values, PAC masks/accessors, classic ADC F002 unsupported
internal references and TS/BGR register layout. Fixed polling budgets and ADC
startup guard delays remain software policy, not claimed chip electrical maxima.
Presence/version selection is owned by the separate layout cleanup. I2C/timer/
USART/EXTI gate/topology cfg removal and central RCC control are separate,
coordinated changes; this packet does not replace those owners' work.

## Watchdog lifecycle cleanup

The sole `Io` trait and generic helper layer were removed from each watchdog.
Read/write/poll/prepare/start/refresh/status are inherent operations on existing
PacIo owners. The IWDT Starting state is recorded before START; every error exit
after unlocking relocks, does not feed, and refuses retry. WWDT preserves its
live counter checks, no-reset/no-stop policy and critical-section window guard.
An external source comparison verifies all nine moved lifecycle method bodies
are identical after receiver/call spelling and rustfmt-only normalization.

The initial generated watchdog gate accessors resolve selected SYSCTRL RCC
metadata and verify field widths/keys/reset bit agreement. They are an
integration point for the concurrently requested central RCC controller;
centralizing control must preserve the exact readback/timeout and LSI behavior.

## Verification

`electrical-metadata-equivalence.json` records13-family source/value comparison,
all65,536u16 voltage declarations per family, unchanged ClockBounds, original
factory-trim header hashes and exact watchdog lifecycle movement. No HAL test
engine, fixture, synthetic firmware or hardware execution was introduced.
Data/schema/generator tests and normal ARM release builds were run; final merged
matrix and layout/provenance checks are owned by parent integration.
