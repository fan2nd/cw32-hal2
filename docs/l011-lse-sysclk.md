# CW32L011 init-only LSE system clock

The system target is limited to CW32L011K8T6 (LQFP32) and CW32L011K8U6 (QFN32). It uses the existing native `Config.lse` declaration and `LseFaultDetection` policy. The shared L010/L011 backend exposes `Sysclk::LSE` under `rcc_lse`; generated exact-package qualification restricts that backend to the existing three L010 and these two L011 packages. Generic-family targets and CW32L012 do not gain LSE SYSCLK. The resulting system roster has 21 exact packages; the previous 19 system profiles and all 23 auxiliary profiles retain their existing qualifications.

This is one-time initialization, without runtime switching, recovery, RTC migration or low-power restoration. `Config::new()` remains factory HSI with the own numeric divisor 24, nominally 4 MHz. Successful LSE SYSCLK initialization retains factory-calibrated, enabled HSI. HSI/HSE selection and auxiliary-only `Config.lse` keep their existing paths. No public LSI SYSCLK or new RTC capability is introduced.

## Declaration and incoming electrical state

`Sysclk::LSE` requires `Some(Lse)`; pure frequency validation returns `LseNotConfigured` otherwise. Nominal LSE is 32768 Hz, with board-declared minimum and maximum per-cycle frequencies. Those bounds propagate through AHB and cumulative AHB×APB divisors. HSI accuracy does not supply the LSE declaration. The fixed 1 MHz time driver rejects this divided 32768 Hz system tree before peripheral-token acquisition or RCC MMIO.

The native declaration retains independent four-bit running/startup drives, startup counts 256/1024/4096/16384, crystal/bypass mode, a nonzero CPU polling budget and the full board supply/ambient envelope. Crystal load, drive and startup must be board-qualified. The existing L011 source ceiling is 100 kHz over 1.7–5.5 V and −40..85 °C. Bypass additionally requires high 0.7VDDIO..VDDIO, low VSS..0.3VDDIO, high and low pulses each at least 450 ns, edges at most 50 ns and 45–55% duty. The RM allows up to 1 MHz whereas the datasheet limits 100 kHz; qualification keeps the stricter limit. The 1.50 s crystal startup figure is typical, without a guaranteed maximum. Polling counts CPU iterations, not elapsed time, and cannot progress after CPU clock loss.

The incoming source, HCLK/PCLK, Flash latency, HSI trim and any unchanged LSI used during initialization must already be electrically legal for the declared board. Real reset alone does not establish factory trim. The initializer cannot retroactively repair an illegal incoming clock tree. Register reads do not measure supply, temperature, frequency, jitter, wiring or reset history. HCLK/PCLK ceilings are 24 MHz below 1.8 V and 96 MHz at or above 1.8 V, within the own 1.7–5.5 V envelope and other applicable source/peripheral conditions.

## Factory monitor and legal bridge are separate requirements

`StartupOnly` requires LSECCS clear and leaves it clear. It does not require factory-matching LSI. Startup edge counting, STABLE and pad/parameter checks do not establish continuing availability. STABLE can remain set after source loss; no LSEFAIL/LSEFAULT or error return need occur, and the CPU can stop. Inherited CLKCCS cannot detect loss while LSECCS is disabled. Selecting LSE itself neither starts nor calibrates LSI; the enclosing HSI calibration path may separately need unchanged legal LSI.

`MonitoredExistingRoutes` requires an already stable, legally operating LSI with non-erased own factory calibration matching the typed ten-bit TRIM field at halfword address `0x001007C2`. This admission occurs in the first native preflight before configuration-gate writes. Software LSIEN may be clear if a native hardware client already requests the oscillator. Original TRIM and WAIT remain unchanged throughout initialization; the HAL does not prepare or calibrate a monitor. Readiness reached during a later cold bridge cannot manufacture the missing factory-monitored entry fact.

The datasheet factory accuracy is −10/+25% over −40..85 °C, giving 29520..41000 Hz. The RM independently specifies a 32.8 kHz ±10% legal adjustment regime. Its register description also gives a 30–36 kHz adjustment range and approximately 0.4% trim step, while the datasheet gives 0.16%. These source statements are not silently reconciled. The 41000 Hz detector bound conservatively covers the own factory accuracy table; it does not make every inherited rate below 41000 Hz legal or enlarge the RM legal-adjustment contract. Factory match and STABLE do not measure actual rate or individual-cycle jitter. The board/platform must satisfy both the applicable legal-entry condition and the detector-window qualification.

Hardware counts 128 LSE edges during 256 LSI periods. The existing software adds a separate one-edge margin, requiring `u64(LSE_min) × 256 > 129 × 41000`, so the integral minimum is 20661 Hz. The declaration must still contain nominal 32768 Hz. The hardware threshold is 128, not 129. This is L011's own factory-monitor qualification; L010's inherited-legal 36080 Hz bound and 18181 Hz result do not apply.

Fault/IRQ/brake routes, interrupt enables, sticky flags and timer state remain unchanged. A real fault may latch capture or system-brake flags, request an already enabled IRQ or asynchronously clear PWM MOE before an initialization error is returned. Existing AOE/output state governs later behavior. Initialization does not acquire IRQs, service handlers, clear flags, change fallback policy or restore outputs.

## Cold HSI calibration and first LSI request

Raw HSIOSC is nominally 96 MHz with the own factory ±2% envelope, 94.08–97.92 MHz. Its legal adjustment regime is 90–100 MHz. The existing HSI factory halfword is `0x001007C0`, with eleven-bit TRIM, DIV[14:11], STABLE15 and no HSI WAIT field. Numeric divide 24 has encoding 14; numeric divide 32 has encoding 0. These own generated facts are reused, without family-independent substitutes.

Reset trim values do not prove factory calibration. The SDK SystemInit explicitly loads both factory trims in software and has masked-all-ones replacement values; Rust startup is not that SDK routine. The HAL retains its erased-HSI calibration error, writes no LSI TRIM/WAIT and imports neither SDK startup nor its replacement values. Ordinary legal cold entry remains supported through the existing guarded HSI→unchanged-LSI→factory-HSI bridge. StartupOnly adds no factory-LSI entry requirement.

RTC SOURCE3 owns raw HSIOSC even with START clear. Before oscillator changes, that owner requires HSI already enabled, stable and factory matching. A necessary HSI start/restart or wait for initially unready HSI also rejects raw-HSI MCO SOURCE3 and enabled HSIRDY. A factory-ready divider-only change remains allowed because it does not change raw HSIOSC. L011 additionally exposes PB0 AF3 HSIOSC_OUT independently of MCO: the functional handover must permit or disconnect its external observer across HSI start/retrim interruptions.

The first-LSI-request decision is latched from entry as `LSE target && (needs_trim || needs_lsi) && !entry_lsi.STABLE`. For such a request, read-only consumer admission runs after retained-configuration checks and repeats immediately before setting LSIEN. Later STABLE cannot bypass the second admission. It rejects:

- RTC SOURCE2 regardless of START/AWTEN; reserved SOURCE4..7 also reject. AWT's RTCCLKD/TICKCLK root is covered by the RTC source check.
- UART1/2/3 SOURCE3 regardless of RXEN/TXEN. UART3 participates only in the L011 compile-time tuple, using its own central APBEN1/APBRST1 bit8 gate/reset facts.
- Enabled LPTIM with ICLKSRC3, including count and encoder modes.
- MCO SOURCE4 or enabled LSIRDY.
- Enabled VC1/VC2 or LVD selecting LSI filtering, including zero filter count, using the shared VC configuration gate.
- Any held-reset domain being inspected, checked before, during and after inspection without releasing reset.

Configuration gates are inspected and restored independently through the central RCC helpers. Enable/restore failures preserve the target's `LseConfigurationGateTimeout { enable_failed, restore_failed }` classification. Restoration failure takes precedence over a semantic conflict and never claims successful rollback. HSI/HSE and auxiliary-only paths retain their earlier classification. Active ADC, SYSCLK-filtered LVD, PCLK-filtered comparators and comparator blanking retain the existing ownership vetoes.

Stable, legal automatic LSI clients with LSIEN clear remain admitted. Only the original software LSIEN request is restored after the bridge; automatic users may keep STABLE set. CLKCCS is temporarily cleared for HSI retrim and restored to its original value. No LSI trim or wait field is written.

These concrete checks do not establish universal idleness. The handover must permit or disconnect retained GTIM/ATIM LSI_OUT selector9, whole-bank GPIO LSI filtering, IWDT and downstream timer/ADC/GPIO or external participants across temporary LSI request and restoration. Dormant work gates are not opened to inspect them. Missing tokens, closed gates, interrupt masking or reset-looking selectors do not establish actual reset history or disconnect observers. This remains a functional handover under the existing pre-Rust bus-master/memory-ownership boundary; it adds no hidden unsafe obligation to safe callers.

## Native pads, consumers and retained RTC

The target reuses the unchanged native `preflight → start → verify` leaf. Crystal mode owns PC14 input and PC15 output; bypass owns PC14 without consuming or rewriting PC15. The native code retains independent drives, exact ready-source reuse, no retune after enable and monotonic inherited/requested pad reservations. L011 rejects raw PINLOCK for both new and reused requested sources and does not clear PINLOCK or LSELOCK. The own PINLOCK description is ambiguous about fault-time enable behavior; no fault-time pin-safety guarantee is inferred.

Starting a new LSE retains UART1/2/3 SOURCE2 and enabled LPTIM LSE/RTC-trigger admission. L011 LPTIM RTC triggers are codes1..4; code5 is PC13, not RTC. Native consumers are checked again after GPIOC activity and before oscillator commit. Healthy exact source reuse may keep existing users of the unchanged oscillator.

For RTC SOURCE0, native admission requires the quiet control/ISR record excluding H24; reserved CR1 bits1:0 are not treated as ACCESS/WINDOW. DATE/TIME/PSC/AWTARR and flags are preserved, without unlock, commands, reset or source migration. A quiet register image does not prove no RTC_OUT/RTC_1Hz recipient: RTCOUT0 selects RTC_1Hz, and RTC1HZ0 is reserved. The handover must disconnect or leave inactive PA1/PA3 AF3 RTC digital outputs and external users, BTIM1..3 code6, GTIM/ATIM TI code8, LPTIM trigger1..4 and downstream timer/ADC/GPIO cascades. Only already operational GPIOA is inspected for visible output conflicts; dormant output/timer banks remain closed. L011 has no direct LSE_OUT route.

Every GPIOC pad inspection, including exact reuse and final verification, can resume whole-bank sampling, filtering and armed edge capture before success or an error. The handover must permit that progress; gate restoration cannot undo it. PC13, unrelated controls, ODR, shared FLTCLK and flags remain untouched. Opening a gate is not a side-effect-free read and does not by itself imply a particular output glitch.

The existing HSIOSC calendar capability retains source3 with raw 96 MHz, bounds 94.08–97.92 MHz, actual first divisor120 and second400000. The existing LSE calendar capability retains first divisor1, second16384 and total32768, borrowing the same declared physical oscillator. Selecting SYSCLK does not migrate, reset or rewrite a retained RTC/AWT owner.

## Flash, fallback and final publication

Pure validation checks LSE bounds and board conditions, configured factory HSI at the final bus divisors, and full effective 4.08 MHz fallback without AHB/APB divider credit. Own numeric fallback divisor24 gives 96 MHz/24 = 4 MHz; factory ±2% gives 3.92–4.08 MHz. Admission covers this bound even when CLKCCS is disabled. It does not claim hardware writes HSI.DIV encoding14, preserves HSIEN/CR0/dividers or continues execution. There is no own-source basis for substituting undivided 97.92 MHz as this fallback bound. Separately, HSI/1 at AHB/1 is rejected because its positive bound exceeds the 96 MHz bus ceiling; nominal equality and extra Flash wait do not authorize overclocking.

With selected LSE, detected failure and inherited CLKCCS=1 give documented effective HSI4MHz fallback; CLKCCS=0 has no action. All frozen LSE timings become invalid on loss/fallback. No progress, rollback, divider retention, RTC continuity or bounded fault-to-fallback time is promised.

Before the first configuration-gate write, the target captures original source/policy identity and passes native preflight, including monitored entry qualification. Retained ownership and first-LSI admission precede oscillator changes. The native FLASH configuration gate is enabled, invalid incoming WAIT>3 rejects, and WAIT3 is written and verified. Monotonic AHB/APB guards of at least /8 preserve larger incoming divisions and the incoming source.

SYSCTRL.CR2.FLASHWAIT[6:4] documents the same function as authoritative FLASH.CR2.WAIT[2:0]. After the owned Flash write/readback, the target normalizes only the typed FLASHWAIT field. Original non-WAIT CR2 bits, IER, MCO, LSE and LSI TRIM/WAIT are verified before and after refreshing native admission. A new snapshot cannot hide changed routes. No shadow bit mask or speculative alias timing model is introduced.

Ready unchanged legal HSI is selected under guards. If factory HSI calibration is needed, the admitted unchanged-LSI bridge is used, with second first-request admission at the enable edge. The original software LSI request and CLKCCS are restored; inherited/requested HSE policy and pad checks remain intact. Native LSE starts or exactly reuses its refreshed admission and establishes its monitor marker.

Under calibrated HSI and WAIT3, final AHB/APB divisors are installed and verified. The last keyed CR0 write selects LSE code4. No later CR0 write occurs, including during errors, polls or final pad checks, so a divider read-modify-write cannot reselect failed LSE after fallback. Faults are checked before accepting apparently ready source state. Source, policy, native monitor, preserved LSE fields, mux and final divisors remain checked through the final waits.

Final Flash wait uses the maximum of declared LSE upper HCLK, configured factory-HSI upper HCLK and full 4.08 MHz fallback. The own formula is `(upper_hclk - 1) / 24000000`, after earlier bound validation. Initial WAIT3 is a precaution, not every configuration's final requirement: final WAIT may be0/1/2/3. The default HSI/24 configuration permits WAIT0. Lowering occurs only after final target verification, with fault/source/policy checks throughout the readback wait. After final native GPIOC/pad verification, source/policy/monitor/mux/dividers and authoritative Flash WAIT are checked again. Only complete success reaches the existing single clock-publication point.

An RCC error publishes no clocks or peripheral tokens. Hardware can retain changed source requests, gates, guards, Flash wait, calibration progress, tentative monitor state and monotonic pad reservations. Reset is required before retry; ordinary reset may retain LSE ownership. Source loss can prevent any error return.

## Own evidence and verification limits

Qualification uses CW32L011's own selected sources. Locators below are one-based PDF/printed pages; vendor artifacts remain external and are not redistributed by this document.

- CW32L011 CN User Manual Rev1.1 with June 2026 cover, SHA-256 `b245887e9037caf267f739e25bd3c354c2579d23315fc06389e055f371159f4f`: 51/50 LSE limits; 54/53 HSI/LSI legality; 57/56 monitor/fallback; 61/60 §4.5.3 and 65/64 selection; 66–75/65–74 source/policy/trim/IRQ; 78/77 and 81/80 UART3 gate/reset; 85/84 MCO; 102/101 and 113/112 Flash; 123/122 PB0 AF3; 139–140/138–139 and 151–154/150–153 RTC/AWT; 200–202/199–201 LPTIM; 422–425/421–424 UART; 541/540 and 549–550/548–549 analog roots. The older same-filename RM is not substituted.
- CW32L011 CN Data Sheet Rev1.1, SHA-256 `0b7414049824881920fc38f829029e3ba0af88feb4536fb27351df0d60f688a5`: 28–29/25–26 PC14/PC15 package pads; 30/27 PB0 AF3; 39/36 Table7-4 bus/supply limits; 48/45 bypass; 50/47 crystal startup; 51/48 Tables7-18/7-19 HSI/LSI qualification.
- Locked CW32L011 SDK 1.0.3 archive, SHA-256 `76adfe39360eb1d05ef58c25f26a8c1f99f2bc2f8fef677214aaca85cffc679e`: own sysctrl header lines126–129 selector4, 196–228 HSI divider encodings and 249 LSI factory address; `Libraries/src/system_cw32l011.c` lines43–66 corroborate software factory-trim loading. SystemInit is a read-only archive member, not imported startup code or an individually locked new runtime input.

Generated qualification checks and runtime readbacks do not measure oscillator availability, waveform, startup distribution, actual legal trim, per-cycle jitter, Flash alias timing, fallback register effects or survival after clock loss. Software validation is separate from hardware/electrical qualification. This target introduces no new HAL tests, probes, alternate initialization path or recovery promise.
