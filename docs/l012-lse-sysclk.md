# CW32L012 init-only LSE system clock

Direct `Sysclk::LSE` is limited to CW32L012C8T6 (LQFP48) and CW32L012C8U6 (QFN48), using the existing native `Config.lse` declaration. Generic CW32L012 and guessed packages are excluded. This adds two targets to the former 21-part SYSCLK roster while preserving those records and all 23 auxiliary declarations. There is no PLL, public LSI SYSCLK, runtime retuning, RTC migration or recovery mode. `Config.lse=None` and auxiliary-only HSI/HSE paths retain their existing contracts.

## Frequencies and board declaration

LSE nominal SYSCLK is 32768 Hz. Nonzero per-cycle bounds must enclose nominal and cover the complete declared supply, temperature, aging, load and waveform conditions. Average ppm does not establish this contract. Crystal mode needs board-qualified running/startup drive and load; bypass additionally needs the own datasheet's voltage, duty, pulse and edge conditions. The qualified source ceiling remains 100 kHz, despite the manual's 1 MHz statement. Supported board supply is 1.7–5.5 V and ambient −40..85°C, subject to every applicable source/peripheral constraint.

Configured factory HSI defaults to /12: 8 MHz nominal, 7.84–8.16 MHz. This differs from silicon reset and effective CCS fallback /24: 4 MHz nominal, 3.92–4.08 MHz after factory trim. Raw factory HSIOSC remains enabled at 96 MHz nominal, 94.08–97.92 MHz. The legal incoming HSIOSC adjustment range is 90–100 MHz; an uncalibrated legal entry is not claimed factory-accurate.

Pure validation independently admits LSE at final AHB/APB divisors, configured factory HSI at those divisors, and the full 4.08 MHz fallback without divider-retention credit. Actual HCLK/PCLK must fit 24 MHz below 1.8 V or 96 MHz at/above 1.8 V. HSI/1 with insufficient bus division fails even at nominal 96 MHz. Exact rational bounds govern these comparisons. The fixed 1 MHz Embassy time driver rejects every divided 32768 Hz plan before singleton tokens or RCC MMIO; no timer doubling or replacement timebase is introduced.

Incoming source, buses, Flash latency, oscillator trim and unchanged bridge LSI must already be electrically legal. Reads do not measure frequency, voltage, temperature, jitter, wiring or reset history. The pre-Rust bus-master/memory-ownership handover remains separate from the functional observer requirements below.

## Monitor, source owners and operational handover

`StartupOnly` leaves LSECCS clear. STABLE is a startup latch and may remain set after later source loss; execution can halt without an error return. A legal unchanged LSI bridge does not require or establish factory monitor qualification.

`MonitoredExistingRoutes` requires LSI already stable before the first configuration-gate write and an unchanged own nine-bit factory TRIM at halfword `0x001007C2`; erased calibration refuses. Software LSIEN may be clear because hardware can request LSI automatically. The own factory 32.8 kHz ±10% envelope is 29520..36080 Hz. Detector admission requires `LSE_min × 256 > 129 × 36080`, whose least integral minimum is 18181 Hz. The extra edge is a separate engineering margin beyond the hardware 128-edge threshold, not measured cycle jitter or a continuity guarantee. Later bridge readiness cannot replace the original entry admission.

Enabled inherited HSE needs an exact declaration and stable parameter/pad reuse; it is never stopped or retuned. RTC/AWT SOURCE3 owns raw HSIOSC even with START clear and requires already enabled, stable, factory-ready HSI. SOURCE1 requires declared enabled HSE. Retained RTC PSC1 keeps its actual input at/below 1 MHz even when START is clear.

Temporary LSI need follows the actual HSI trim bridge, retained HSECCS/LSECCS and enabled LSI-selected LVD/VC clients, including zero filter count. If entry LSI was nonstable, consumer refusal runs before oscillator changes and repeats before LSIEN, using that immutable entry classification. It rejects RTC SOURCE2/reserved, UART1/2 SOURCE3 regardless of RX/TX, operational UART3 SOURCE3, I2C1/2 master/slave raw CLKSRC1/3, enabled LPTIM ICLKSRC3 regardless of operating mode, MCO4, enabled LSIRDY, and enabled LSI-filtered LVD or any of four VCs. Every needed temporary LSIEN assertion, including initially stable LSI, has an adjacent full original-policy/source/factory-monitor and guarded-HSI/WAIT3 commit check after all inspection gates restore.

Necessary HSI start/restart/retrim or waiting for initially unready HSI additionally refuses MCO3, enabled HSIRDY and either I2C side's raw1/3 selection. Factory-ready divider-only changes preserve raw HSIOSC. I2C code3's HSI/LSI disagreement remains unresolved; neither interpretation proves absence of an owner.

The shared ADC1/ADC2 gate is configuration-and-work. Opening it to inspect EN can resume conversion, triggering or other ADC progress before refusal. The supported functional handover must permit that interval; restoration cannot undo it. Active ADC, PCLK-dependent analog filtering/blanking, OPA calibration and DAC trigger/DMA/wave owners retain their existing refusal. LVD is ungated and does not borrow the VC gate.

The handover must also permit or disconnect PB0 AF3 HSIOSC_OUT, PB11 AF4/PF3 AF2 LSI_OUT, inaccessible UART3, direct timer inputs/cascades, whole-bank GPIO LSI filters, IWDT and downstream timer/ADC/GPIO or external observers. Closed gates, absent tokens, masked IRQs and reset-looking selectors do not prove inactivity. Dormant timer/output/GPIO or disputed UART3 work gates are not opened merely to inspect them. These are functional limits under the existing platform entry model, not a hidden unsafe caller obligation.

## Native pads, reset windows and calendar continuity

Crystal mode reserves PC14/PC15; bypass reserves PC14. Native independent running/startup drives, exact ready-source reuse, no enabled retuning, raw PINLOCK refusal and monotonic inherited/requested pad ownership remain. New-LSE native RTC SOURCE0 admission uses the quiet control/status image excluding only H24; reserved CR1 bits1:0 are not ACCESS/WINDOW. It preserves DATE/TIME/PSC/AWTARR, flags and alarms.

No independent RTC_OUT/RTC_1Hz or direct LSE_OUT observer may depend on an unowned transition. Visible L012 routes include RTC_OUT PA1/PA3 AF3 and PB14/PB15/PC13 AF4; LSE_OUT PB12 AF4, PF1 AF3 and PF3 AF1. LPTIM trigger codes1..5 remain conservative RTC dependencies. Native BTIM code3 is instance-specific (BTIM1 RTC_OUT, BTIM2 LPTIM_OV, BTIM3 VC4_OUT); SDK/manual BTIM, ATIM and trigger mapping conflicts are not silently resolved. All documented alternatives must be inactive or disconnected.

Every GPIOC pad window, including reuse and final verification, can resume whole-bank sampling, filtering and edge capture before success or failure. The handover permits that progress. PC13, unrelated controls, shared filter clock and flags remain preserved. Already-operational output banks/UART3 are inspected without opening their closed gates.

For the target, held reset is refused before, during and after each actual inspected domain window. Gates restore independently after reads, and restoration errors precede buffered semantic refusals. Configuration errors preserve both enable and restore failure fields. FLASH's active-low reset is checked around its configuration and authoritative WAIT windows; FLASH intentionally remains enabled. Fresh HSE pad configuration retains its existing leave-enabled GPIOF ownership; read-only matching restores its entry gate. No reset is asserted or released to obtain an inactive-looking image.

HSIOSC calendar source3 keeps raw 96 MHz, first divisor120 and second400000, total96000000. LSE calendar source0 keeps divisors1 and16384, total32768. Selecting SYSCLK does not migrate or rewrite RTC/AWT state. Calendar readiness remains conditional on the physical source's health.

## Flash, final selection and failure ownership

L012 documents SYSCTRL.CR2.FLASHWAIT[6:4] as the same function as FLASH.CR2.WAIT[2:0]. The implementation writes only FLASH WAIT with key0x5A5A, preserving FETCH/CACHE/CACHEINVALID/RFU and all non-WAIT SYSCTRL brake/debug/wake controls. Original CR1/CR2/IER/MCO and source identity are retained; no live snapshot may rebase changed policy.

Before the initial WAIT3 write, authoritative Flash and the captured mirror must agree on WAIT0..3. Before the final write they must both show WAIT3, with verified calibrated HSI and final dividers. For either write, success occurs only when both show the requested new value. Matching old values may continue only within the poll budget; mixed or other values fail immediately, even if a benign observation race caused them. Every observation checks faults, source/monitor identity, policy, mux/dividers and non-WAIT controls before success or continued waiting. No atomic alias observation or propagation bound is promised.

WAIT3 precedes unchanged HSI enable and the combined HSI mux/monotonic AHB/APB guards of at least /8. A needed factory-HSI trim bridge uses unchanged LSI and restores original CLKCCS and software LSIEN without requiring automatic clients to stop. The original opaque native Admission receives its one existing pre-start maximum-WAIT update immediately before native LSE start, never afterward.

Final AHB/APB divisors and final WAIT are established under verified factory HSI. Final WAIT covers `max(LSE HCLK upper, configured HSI HCLK upper, 4080000)`, using `(upper−1)/24000000`; the default HSI/12 plan needs WAIT0, while faster admitted plans may need WAIT1/2/3. The final CR0 write then selects LSE. No CR0 or FLASH write follows, including timeout, fault, mismatch or pad/gate cleanup. No rollback, divider repair, retry write or stale LSE reselection can overwrite a hardware fallback.

Faults precede apparently ready state; gate restoration failures retain their detailed precedence. Final source/monitor/policy, mux/dividers, HSE/LSE pads and authoritative WAIT must pass before the existing sole clock-publication point. Real faults can set capture/brake flags, request enabled IRQs or clear PWM MOE before an error; existing output policy governs recovery. No flags or fault routes are cleared.

Failure consumes initialization ownership and publishes no clocks. Source requests, gates, calibration progress, a frozen monitor marker and pad reservations may remain; reset is required before retry, and ordinary reset can retain LSE. Frozen timing becomes invalid after loss/fallback. No bounded recovery time, continuing execution, RTC continuity or divider/register-retention guarantee is made. Poll budgets count CPU iterations only while the CPU runs.

## Own evidence and verification limits

One-based PDF/printed locators refer to the existing locked own L012 originals; vendor bytes remain external:

- CN User Manual V1.4, SHA-256 `a9e54694a26f03c1f3e3041f40844900168ca2b8e6142f3328671b92e6b7a340`: 59/33 calibration/legality; 73/47 and127/101 WAIT alias; 77/51 native LSE; 84/58 ADC work gate; 190/164 and201–204/175–178 RTC; 232/206 native BTIM selectors.
- Current EN User Manual V1.0 (June2026), SHA-256 `f56d5ed899dd090b469fac6a09084aab9f5068ae3b56cf7562b83997bd2f1088`: 53/27 clock tree; 63/37 factory trim; 66/40 fallback; 73–78/47–52 source/policy/HSI; 82/56 LSE; 88/62 UART3 gate disagreement; 91/65 active-low FLASH reset; 135/109 WAIT table/alias; 252/226 BTIM; 272–273/246–247 LPTIM; 552/526 UART source; 626/600 and636/610 I2C conflict.
- CN Data Sheet V1.0, SHA-256 `08a605fe8843d1eefa2b1115474b5a74df2833068f0ad6f35225e4f39377da76`: 35–40/32–37 pad/output routes; 47/44 bus/supply; 55/52 LSE electrical limits; 57–58/54–55 oscillator/factory qualification.

Existing locked SDK1.0.5 is corroboration, not a way to resolve conflicting manuals or import startup/calibration behavior. Build/readback validation does not establish real oscillator waveform, alias propagation, per-cycle jitter, fallback register effects or survival after clock loss. No new HAL test, model, probe, harness, alternate initialization route or silicon acceptance is implied.
