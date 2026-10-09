> Historical software-verification references: HAL tests and fixture harnesses cited below were deleted on 2026-10-08. Their reported results apply to the dated source snapshot only; source facts remain unchanged. See [current HAL layout and build scope](hal-production-layout.md).

# CW32F020 bounded ADC12 and GTIM counter/PWM

This batch qualifies only F020 alongside the existing F030/A030 implementation.
It does not select the x030 chip cfg for F020 or alias SYSCTRL/ADC pointers.
The real `adc_cw32f020_v1` remains selected. GTIM uses canonical `v1` because
its entire corrected IR is exactly equal, independently of this HAL review.

## Primary evidence and resolved discrepancies

- Own [RM CN V1.4](https://www.whxy.com/uploads/files/20240920/CW32F020_UserManual_CN_V1.4.pdf), SHA256 `279521eaf7d32b241e614a9553044a4d99c1e7ca2566efa3b0fdd2b66c106eed`.
- Current printed [DS CN V1.3](https://www.whxy.com/uploads/files/20251230/CW32F020_DataSheet_CN_V1.3.pdf), SHA256 `1e330d800f10114c654b97cbd45b64d56a99c3cb39138d4ef20de3c12968dab0`.
- Own [SDK V1.2](https://www.whxy.com/uploads/files/20240115/CW32F020_StandardPeripheralLib_V1.2.zip), SHA256 `1d77fece47a0c615c8ae51374ea946b17ab489042222f33e38d93e6969945b5d`.
- Complete per-file source hashes and own-family register/clock inventories:
  `adc-timer-next-batch-evidence.json`; promoted route proof:
  `f020-adc-pwm-route-evidence.json` and its companion report.

The source-root file named DS V1.3 is printed Rev1.2, with a different hash;
all current AF and electrical conclusions use the `current-datasheets` copy.
The old `f020-backend-compatibility.md` statement that both ADC and GTIM have
incompatible layouts is superseded for this precisely bounded register subset.
SDK/SVD-only ADC RESULT140–143 and ENABLE14 remain raw PAC registers, but provide
no documented 14-bit HAL mode. Copied ATIM AF cells cannot create a nonexistent
ATIM peripheral. ADC trigger bit 0 is not the x030 ATIM signal; this driver writes
TRIGGER=0 only. No trigger, scan, accumulation, DMA or asynchronous ADC API exists.
The DS accuracy table mentions internal calibration, but no documented software
ADC calibration command was found. No such operation or accuracy promise is added.

## Independently checked hardware contract

### ADC

Own RM §§21.4.1–4, printed pp373–375, table21-5 p377 and §§21.13.1–12,
printed pp398–406 establish:

- Single software-triggered 12-bit conversion, right-aligned RESULT0; acquisition
  encodings 5/6/8/10 cycles plus 19 comparison cycles; PCLK divided by 1–128.
- EN startup requires READY, with approximately 40 us startup. Polls are bounded by
  a nonzero iteration budget. Completion requires EOC and START cleared; OVW or
  timeout discards the result, stops conversion, drains it and powers down.
- CR0 BIAS[15:14] and reserved fields retain documented reset 0. No calibration
  field is invented. CR1 DMA/ALIGN/DISCARD are zero, MODE=0, CR2/TRIGGER/IER=0.
- ICR is R1W0 for bits 0–6; READY bit 7 is read-only and is never cleared. ISR and
  results are read-only. HAL uses CR0/CR1/START/CR2/TRIGGER/RESULT0/IER/ICR/ISR only.
- VDDA minimum is a board guarantee, not a nominal reading. RM table21-3 limits
  ADCCLK to 0.5/2/4/12/24 MHz at the 1.65/1.8/2.0/2.4/2.7 V boundaries. Internal 1.5 V
  requires VDDA >= 1.8 V (2 MHz below 2 V, otherwise 4 MHz), internal 2.5 V requires >= 2.8 V
  and 4 MHz. Comparison is rational before integer division, avoiding truncation.
- BUF is mandatory for internal/high-impedance sources and caps 200 kSPS. DS
  tables 7-29/31, printed pp 50/52, require TS startup <= 45 us and acquisition >= 5 us.
  The driver waits 50 nominal us, reduces the clock for temperature acquisition,
  and repeats follower settling after external-source switches. BGR is given 25
  nominal us after newly enabling it (RM VC section: approximately 20 us).
- Internal VDDA/3, TS and BGR use mux 13/14/15. External reference is withheld
  because ExRef pin ownership has not been designed. Temperature is raw counts,
  not calibrated degrees. Source impedance, input range, clock tolerance and
  board settling remain caller/electrical responsibilities.

The ADC owner exclusively controls its gate and analog resources. Blocking reads
have no pending futures to cancel. Hardware errors stop the engine; rejected
configurations leave it untouched. Drop stops and
powers down before gating the clock. Channel erasure retains the pin's exclusive
borrow/ownership. Analog mode remains after each borrowed conversion. Comparator
or raw ADC access must not concurrently manipulate the shared BGR/TS circuitry.

### GTIM

Own RM §§14.3.1.1–2 p215 and §§14.3.4.1–3 pp226–228 establish 16-bit CNT/ARR/CCR,
power-of-two prescaler 0–15, direct PCLK with no APB doubling, immediate ARR/CCR,
and prescaler activation at overflow or EN 0→1. Reconfiguration stops, clears CNT
before lowering ARR, writes PRS, clears only OV, then resumes if previously running.
Period=ARR+1 is represented as u32, including 65536.

Mode 8 forces low, 9 high, E is high for CNT>=CCR, F high for CNT<CCR. Both output
polarities use forced endpoint modes for exact 0% and 100%, including ARR 65535.
Other channels' packed modes are retained. Frequency changes quiesce outputs,
preserve duty fraction/enable/polarity and start a new period. Drop forces inactive,
stops, then disconnects owned pins before the gate is disabled. Immediate updates
are not glitch-free; no preload/update-event mechanism is invented. Interrupt and
DMA request registers are zeroed; ICR R1W0 writes preserve reserved reset-one bits 7/8.

No capture, encoder, down/center alignment, synchronization, external clock,
ATIM/complementary/dead-time API or Embassy time driver is added.

### Clocks and package routes

Own SYSCTRL has APBEN1/APBRST1 at 0x38/0x48 and APBEN2/APBRST2 at 0x34/0x44.
ADC is APB2 bit 2. GTIM1/2 are APB1 bits 1/2; GTIM3/4 APB2 bits 10/11. Reset is
active-low. Real-PAC modifications preserve neighboring gates/resets and use
readback barriers. Successful HAL clock initialization is required first.

Package projection supplies 9/11/13 analog pads and 17/32/46 typed PWM routes for
QFN20/QFN32/QFN48. The family alias uses the common-package intersection. Analog
source signal identity and mux values are retained separately in the sidecar.
SWD/reset/input-only/unbonded pads remain excluded. Oscillator-multiplexed pads are
alternatives only: the caller must resolve existing oscillator/board ownership.

## Module and validation boundary

`adc/mod.rs` keeps the shared Embassy-style owner, sealed channel API and bounded
engine; `adc/classic/mod.rs` adapts only the independently reviewed register subset.
`timer/mod.rs`, `timer/low_level/mod.rs` and `timer/simple_pwm/mod.rs` keep common
counter/PWM logic; `timer/v1/mod.rs` adapts the exact reviewed register version.
Build cfgs retain actual version names and assert all selected ADC/GTIM versions.

Host tests exercise the actual production engine and setter/reconfiguration paths,
plus RAM-backed selected F020 PAC adapters. Tests cover every PRS encoding,
period 1/65536, rational timing, voltage boundaries, conversion abort/retry,
internal-source settling, no partial writes on invalid frequencies, prescaler
activation, R1W0 masks, both PWM polarities/endpoints and drop ordering. F020 RAM
sentinels explicitly guard reserved/default fields, read-only registers, unused
14-bit registers and the absent integer-prescaler offset.

`test_f020_adc_timer_hal_contracts.py` runs alias and all three exact parts through
positive typed routes and diagnostic-specific ownership/unsupported-API failures.
`test_f020_hal_links.py` links ADC12/PWM/counter with GPIO/UART/CRC into real
Cortex-M0+ ELF images with runtime/defmt, verified 32 KiB FLASH/8 KiB SRAM/vector tables.
No host/RAM/compiler/ELF test executes firmware or establishes silicon accuracy,
electrical safety, oscillator tolerance, waveform behavior or hardware reliability.
Hardware checks remain required at voltage/clock/source-impedance corners, internal
source settling, nonunity APB, reconfiguration/drop transitions and all PWM endpoints.

Standard `./d test` checks committed F020 proof and generated projections, then
reinspects original PDF cells through `verify_f020_adc_pwm_routes.py --sources
"$CW32_SOURCES"`. This source audit requires Poppler `pdftotext` and the pinned
Python `pdfplumber` dependency in `requirements-dev.txt`. All 15 official F020
inputs are already hash-pinned by the evidence-acquisition manifest.
