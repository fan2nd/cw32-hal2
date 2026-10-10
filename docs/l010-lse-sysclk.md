# CW32L010 init-only LSE system clock

The system target is limited to CW32L010F8P6, CW32L010F8U6 and CW32L010Y8M6. It uses the existing native `Config.lse` declaration and `LseFaultDetection` policy. `Sysclk::LSE` is exposed only under `all(cw32l010, rcc_lse)`, whose generated exact-package validation admits these three parts. Generic CW32L010 and CW32L011/L012 do not gain this target. The existing 16 system-target profiles and all 23 auxiliary-LSE paths retain their previous behavior.

This is an initialization contract, not runtime switching, recovery, RTC migration, low-power restoration, or hardware qualification. `Config::new()` remains factory HSI. Successful initialization keeps HSI factory calibrated and enabled so the existing raw-HSI RTC capability remains valid. No public LSI system-clock or RTC capability is added.

## Declaration and physical obligations

`Sysclk::LSE` requires `Some(Lse)`; a missing declaration returns `LseNotConfigured` during pure frequency validation. The source is nominally 32768 Hz. Its exact declared bounds propagate through the selected AHB and cumulative AHB×APB divisors. HSI retains its own factory-qualified bounds; it never supplies the LSE accuracy declaration.

The native declaration specifies mode, separate running/startup drive, startup wait count, nonzero CPU polling budget, per-cycle frequency bounds and the full board supply/ambient envelope. Crystal drive/load/startup and bypass waveform require board qualification. For L010 bypass, the own sources require at most 100 kHz, high/low pulses at least 450 ns, rise/fall at most 50 ns, high 0.7VDDIO..VDDIO, low VSS..0.3VDDIO and duty 45–55%. Crystal startup 1.50 s is typical only, without a guaranteed maximum. A polling budget is not a time limit; no poll can run after the CPU clock stops.

The incoming source, buses, Flash latency, HSI trim and any unchanged LSI used during initialization must already be electrically legal. The initializer cannot repair an illegal incoming tree retrospectively. Actual supply, ambient, frequency, jitter, reset history and wiring are not measured. Own limits are 24 MHz below 1.8 V and 48 MHz at/above 1.8 V over the qualified 1.62–5.5 V range, with other source/peripheral/thermal restrictions still applying.

## Startup and runtime fault policy

`StartupOnly` requires LSECCS clear and leaves it clear. Startup edge counting and enabled/STABLE/parameter/pad checks do not establish continuing availability. STABLE can remain set after loss; no LSEFAIL/LSEFAULT need appear, and the CPU can stop without reaching an error. CLKCCS cannot provide detection while LSECCS is disabled. LSE selection alone does not request or calibrate LSI. The enclosing HSI transition can separately require a temporary LSI request.

`MonitoredExistingRoutes` requires stable, legally operating LSI before any LSI-enable write. LSIEN may be 0 when a native hardware client already requests it. The original TRIM and WAITCYCLE remain unchanged. L010 uses its inherited-legal 32.8 kHz ±10% regime, with upper bound 36080 Hz; it does not borrow another family's factory calibration or substitute the separate factory ±3% figure. Every declared LSE minimum must satisfy `LSE_min × 256 > (128 + 1) × 36080`: 128 hardware edges, 256 LSI cycles and one additional software margin edge. Whole-hertz minimum is therefore at least 18181 Hz, while the declaration must also contain nominal 32768 Hz. Average ppm and STABLE do not prove individual-cycle timing.

Native monitor bookkeeping captures the admitted LSI parameters and remains part of every later health check. Later LSI readiness cannot manufacture the inherited fact required by the initial preflight. The initializer preserves interrupt enables, sticky flags, fault/brake routes and timer state. Real runtime faults may latch timer capture/system-brake flags, request an already enabled IRQ or asynchronously clear PWM MOE before an error is returned. Existing AOE/output settings govern later behavior. No flag clearing, automatic monitor preparation, IRQ acquisition, fallback-policy change or output recovery is performed.

With selected LSE, inherited CLKCCS = 1 provides the documented effective HSI 4 MHz fallback after a detected runtime fault; CLKCCS = 0 does nothing. The own RM does not establish how HSI.DIV, HSIEN, CR0 or bus-divider fields mutate during fallback. Neither execution continuity nor register preservation is promised. Frozen LSE timings become invalid after loss/fallback even if execution resumes.

## Cold HSI calibration and unchanged LSI

Ordinary reset cannot be assumed to contain factory HSI trim. The initializer retains the existing guarded HSI→LSI→HSI calibration path rather than requiring a specially prepared bootloader. RTC SOURCE = 3 owns raw HSIOSC even with START = 0 and requires already enabled, stable, factory-matching HSI before oscillator changes. A needed HSI start/restart or wait for initially unready HSI also rejects MCO SOURCE = 3 and enabled HSIRDY. A factory-ready HSI divider-only change is allowed and preserves raw HSIOSC.

The existing `needs_trim || needs_lsi` request is retained. If entry LSI was not stable, first-request admission is latched from that original snapshot. It runs after retained-configuration checks and repeats immediately before setting LSIEN, even if STABLE has become 1 in the meantime. The read-only vetoes are:

- RTC SOURCE = 2 regardless of START or quiet-calendar state; reserved SOURCE = 4..7 also reject
- UART1/2 SOURCE = 3 regardless of RXEN/TXEN
- Enabled LPTIM with LSI source 3, including count/encoder paths
- MCO SOURCE = 4 and enabled LSIRDY request
- Enabled VC1/VC2 or LVD selecting its LSI filter source, including zero filter count
- Any held-reset domain that must be inspected

RTC/UART/LPTIM/shared-analog configuration gates use their existing native ownership helpers separately and restore their incoming gate state. No peripheral control, status, timer state or borrowed flag is written. Enable failure and restoration failure remain distinguishable through `LseConfigurationGateTimeout`; failed restoration takes precedence over a semantic conflict and does not mean rollback succeeded. Non-target callers retain their previous error classification.

Stable, legal automatic clients with LSIEN = 0 remain admitted. LSI TRIM/WAIT never change, readiness is bounded while execution continues, and the original software LSIEN request is restored after the bridge without demanding STABLE clear. A new monitored source must already have passed early stable-LSI admission; cold-bridge admission cannot upgrade it.

The visible vetoes do not establish universal idleness. The functional handover must permit or disconnect retained GTIM/ATIM LSI_OUT selector 9, whole-bank GPIO LSI filters, uninspected IWDT and downstream timer/ADC/GPIO or external participants across temporary LSI start and software-request restoration. Their dormant work gates stay closed. RTC SOURCE = 2 rejection also prevents newly activating its prescaler/AWT/RTC_1Hz tree. Genuine untouched reset history can establish reset routes; register resemblance, missing tokens, closed gates and interrupt masking cannot establish that history or disconnect observers.

## Native ownership and pad handover

The target uses unchanged native `preflight → start → verify`, including exact enabled-source reuse, central ownership checks, fault state, monitor policy and pad admission. It does not duplicate a weaker oscillator start. Disabled-source admission retains the RTC/UART/LPTIM source and configuration requirements. Exact healthy reuse may retain existing users of the unchanged LSE; it does not stop or retune their source.

When RTC selects LSE SOURCE = 0, a quiet control/ISR image is necessary but insufficient to prove no RTC_OUT/RTC_1Hz observer. Before new startup, the functional handover must disconnect or leave inactive PB04/PB06 RTC digital outputs and external users, BTIM RTC trigger/reset roots, GTIM/ATIM capture/trigger paths and downstream timer/ADC/GPIO cascades, including all dependent RTC_OUT/RTC_1Hz observers. Visible PB04/PB06 AF2 digital outputs are rejected before PB1 bypass mutation. DATE/TIME/PSC/AWTARR are not used as reset detectors.

GPIOB is a whole-bank working gate. Every pad inspection, including reuse and final verification, can resume sampling, filtering and armed edge capture before success or an eventual error. The handover must permit that progress; restoring the gate does not undo events. Unrelated ODR, controls, FLTCLK and flags remain untouched. No timer gate is opened to prove idleness. Crystal owns PB1 input and PB0 output; bypass owns PB1 and does not rewrite PB0. Existing analog/AF0/direction/pull/open-drain/filter/edge checks and L010 PINLOCK reuse behavior remain intact.

Inherited and requested pad reservations are monotonic. An inherited crystal or pin lock cannot lose PB0 ownership through a later bypass request, error, drop or forgotten configuration. Pure validation errors before token acquisition do not claim hardware mutation or new runtime reservation.

## Ordering, Flash and publication

Pure validation checks the requested LSE bounds, board conditions, polling budgets and requested HSI at the final dividers. Independently, the full fixed-fallback upper bound 4.08 MHz (4 MHz with factory ±2%) must fit both bus ceilings without divider credit, regardless of inherited CLKCCS. HSI/1 with AHB/1 still exceeds the 48 MHz ceiling at its positive bound and is rejected. Keeping extra Flash wait does not authorize an overclock.

Initial native LSE admission runs before the first configuration-gate write. Retained RTC/HSE/ADC/analog checks and target ownership checks precede oscillator mutation. Flash WAIT is raised to the own maximum 1 and verified, then monotonic AHB/APB guards of at least /8 are installed. Larger incoming division is preserved during guarding.

SYSCTRL.CR2.FLASHWAIT has the same function as FLASH.CR2.WAIT. The original native Admission includes whole CR2, so the target verifies every original non-WAIT CR2 bit, original IER, original LSE register and original LSI TRIM/WAIT after its own WAIT transition. Normalization uses the own typed FLASHWAIT field; no shadow 0x70 mask is added. Authoritative FLASH.WAIT readback is checked. Only after those identities pass does normal native preflight create a refreshed Admission, still before any oscillator enable/stop/trim mutation. Original non-WAIT identity remains checked through publication. This does not assume a measured alias timing model or permit unrelated route changes.

Factory HSI is made ready under guards. The unchanged LSI bridge is admitted and used only when required, inherited CLKCCS is temporarily cleared only while retrimming HSI and restored, and any requested/retained HSE is handled by the existing qualified path. Native LSE then starts or reuses the refreshed Admission and establishes its monitor marker.

While calibrated HSI is selected, final AHB/APB dividers are installed and verified under WAIT = 1. The final keyed CR0 read-modify-write selects native LSE code 4 while retaining those divisors and reserved fields. No later CR0 write occurs. Polls check faults before accepting target readiness; source/monitor/policy/divider mismatch is an error. This prevents a later divider RMW from reselecting failed LSE after hardware fallback.

Final Flash planning takes the maximum of declared LSE upper HCLK, requested factory-HSI upper HCLK at the final AHB divider, and full 4.08 MHz fixed fallback without divider credit. The own 24 MHz step and maximum WAIT = 1 apply. Lowering occurs only after target health/mux/divider verification, with fault/source/policy checks during the readback wait. Final inherited/requested source, original policy, LSI parameters, factory HSI, native LSE parameters/monitor/pads and mux/dividers are verified again after the final GPIOB inspection window. Only then does the backend return to the existing single `CLOCKS.set(Some(...))` publication point.

RCC errors publish no new clocks and return no peripheral tokens. They can leave source requests, gates, guards, Flash latency, calibration progress or tentative native monitor state changed. No broad rollback is promised. Reset is required before retry, and ordinary reset may retain LSE ownership/state. A source loss can prevent any error return. The fixed 1 MHz Embassy time driver rejects every divided 32768 Hz system tree before token acquisition or RCC MMIO; no slow-clock workaround or timer×2 claim is added.

## Own evidence and verification boundary

External evidence is the own CW32L010 manual/datasheet/SDK, not another family's source. PDF locators are one-based pages:

- CW32L010 User Manual CN V1.2, SHA-256 `b66ae2b2837cf22aede7f19312b82659ea10f96960bfe7965de8733bb72513fa`: PDF52–59 LSE/LSI startup, monitor and fallback; PDF63/67–71 switching and native controls; PDF75/77/78 IRQ semantics; PDF80–84 gates/resets; PDF87 MCO; PDF114 Flash; PDF124/130 GPIO filters; PDF139–140/152 RTC roots/source; PDF200/202 LPTIM; PDF234/281/311/377 timer LSI roots; PDF420 UART source
- CW32L010 Datasheet CN V1.3, SHA-256 `6fbefd334a86a1fafbec8ead5b6bd44dc99dfe564b604f69ec1edaa4ec789b86`: PDF32 bus limits; PDF40 bypass waveform; PDF42 crystal startup; PDF43 HSI/LSI qualification
- CW32L010 Standard Peripheral Library V1.0.9 archive, SHA-256 `84dbbeb8b684d0435ef9f926df8b899ceeb1d7bfd7767145b1e4f7e22210c716`; own SVD member SHA-256 `733cc8ff3e186a128342134aee71db1d155e5e2b95ab1c8c8a77054391cb8c66` corroborates native fields/selectors

The SDK LSE helper is not an implementation template: it changes CCS policy and exposes fields that the own RM reserves. The native MODE/WAITCYCLE/DRIVER/PDRIVER-only mutation remains authoritative. External vendor artifacts are not project-authored or assumed redistribution-cleared.

Software review/build evidence does not measure oscillator availability, bypass waveform, startup distribution, legal incoming trim, jitter, Flash alias timing, fault timing, fallback register effects or source-loss survival. No hardware execution, new HAL test/probe/harness, runtime recovery or whole-HAL soundness claim is made by this target.
