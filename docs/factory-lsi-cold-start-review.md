# Independent review: F020/F030/A030 factory-LSI cold start and SYSCLK admission

Completed 2026-10-09. Verdict: **accepted as a source-backed design for subsequent implementation planning, with no unresolved design blocker in revision 2.** This is not implementation approval, a test pass, silicon qualification, or a claim of automatic LSI fault recovery.

## Reviewed immutable input

- Design: `docs/factory-lsi-cold-start-design.md`
- SHA256: `c9f16c7346b703e4e05b8d4fc9512eff81c9248aa18700afb10b0a2c6473c616`
- Size: 37,057 bytes.
- Baseline: accepted Stage55 source snapshot.
- Original v1 and v2 design bytes, author receipts, both own manuals and all three correct electrical datasheets are archived under `external-evidence/stage56/lsi-cold-admission-review/originals/`. `external-evidence/stage56/lsi-cold-admission-review/original-source-manifest.json` records original paths, archive paths, full hashes and independently extracted physical PDF pages. `external-evidence/stage56/lsi-cold-admission-review/reviewed-code-hashes.json` records the inspected baseline files and final unchanged-hash checks.

The mutable `external-evidence/stage56/lsi-cold-admission-review/partial.md` was used only for early orientation. It is not the accepted design. Revision 1 (`bb3fcc80124df2a1fc0253abb0f26682c8ae3471f64f36b4aa5404d294bddc0c`) was superseded after the sequencing clarification below.

## Findings resolved before acceptance

### 1. Mandatory CCS writes do not prevent a real reset route

Both own CR1 tables give reset `0x00000001`, classify HSECCS/LSECCS as RW, and prescribe writing one to CCS controls. The reset state therefore has these controls clear; they are not documented read-as-one bits. The narrative separately says external fault detection defaults disabled. The accepted design loads only stopped LSI TRIM before the first CR1 write, then always writes mandatory CCS ones. It never clears an inherited CCS bit to manufacture admission.

Evidence: CW32x030 RM Rev2.5 physical PDF71 and59–60; CW32F020 RM Rev1.4 PDF69 and57–58. LSI register reset is unknown rather than a factory-TRIM promise (x030 PDF74; F020 PDF72). The general stability section says hardware clears STABLE on shutdown; ISR reset `0x00000801` independently leaves LSISTABLE and LSIRDY low (x030 PDF58/79; F020 PDF56/77).

Normal power-on reset can satisfy the complete current predicates: HSI selected, CR1=1, external and LSI stable indicators low, no LSI ready enable/flag or pending RCC IRQ, MCO off, RTC source LSE, AWT source HSIOSC, UART sources PCLK, GPIO filters zero and PB11 AF0. Closed reset GPIO gates are handled through the disclosed whole-bank windows. This supports genuine cold start without vendor SystemInit preloading LSI TRIM. It does not promise acceptance of every warm reset or firmware jump.

### 2. A new detector must not be introduced against an unready LSI reference

CCS=0 before the first CR1 write proves detector inactivity at that instant. It alone does not justify enabling mandatory CCS while inherited HSE/LSE is already running and the LSI reference is newly starting. The own runtime-monitor descriptions require LSIEN but establish no stable-reference holdoff.

Revision 2 explicitly requires HSEEN=LSEEN=0, both external register/ISR stable indications low, and no HSE/LSE selected as SYSCLK on the stopped route. New requested external sources start only after LSI is ready. Running factory-matching LSI reuse remains a separate path and can retain existing external consumers. This conservative restriction preserves the real reset route without inventing a detector startup guarantee.

### 3. Permanent target LSI differs from the existing temporary HSI calibration stop

Revision 1 required HSIEN on every CR1 write and on all post-request failures, while preserving a calibration procedure that must stop HSI while executing on LSI. That was internally inconsistent with the actual existing `hsi_48mhz.rs` sequence.

Revision 2 resolves this: target LSIEN stays asserted permanently after the enable attempt; HSI is retained except for the existing separately admitted calibration stop; successful finalization requires HSI enabled, ready and qualified. Failure during HSI calibration may leave HSI stopped or incompletely restarted, with the CPU on LSI. No unsafe cleanup/retrim/re-enable is promised. The pre-enable recheck also advances expected TRIM to the admitted factory code after a successful write while retaining original WAIT/reserved bits and all root/observer exclusions.

### 4. The same oscillator cannot expose contradictory timing qualification

The baseline `LsiClock::bounds()` uses `ClockBounds::rtc_source()`, which currently sets `rate_only=false`. Merely adding rate-only SYSCLK LSI would leave an alias to a strict-cycle capability for the same three-family oscillator.

Revision 2 includes the narrowly scoped consistency correction across LsiClock, CalendarClock::Lsi, RTC source bounds and divided calendar bounds on F020/F030/A030, even when SYSCLK is HSI. Its new compatibility section correctly identifies that `has_cycle_timing_bounds()` changes and existing strict-duration assertions will reject downstream calls. Existing RTC documentation promising exact duration helpers needs corresponding revision. The inspected in-tree RTC operations and examples do not call the strict-duration helpers; that is static evidence, not a build result. Native HSIOSC RTC and other families are outside this change.

## Accepted technical boundaries

| Area | Independent review result |
|---|---|
| Running source | Matching factory TRIM is reused without TRIM/WAIT writes. Mismatch or incompatible active status rejects; never stop/retrim to gain admission. A stopped accidental match still undergoes full new-start admission. |
| Factory access | Own public address 0x00012602, aligned volatile u16 read, reject erased 0xFFFF before masking, ten-bit TRIM only; valid zero is not rejected merely for being zero. |
| Stopped proof | LSI enable, current source, both stability indications, LSIRDY enable/flag, pending RCC IRQ and complete direct roots are checked twice and again at the use edge. Double sampling does not replace exclusion of arbitrary register writers. |
| Root roster | RTC SOURCE2 regardless START; AWT SRC1 regardless EN; all three UART SOURCE3; HSE/LSE monitors; MCO source4; all GPIOA/B/C/F FILTER.FLTCLK5; PB11 AF1 LSI_OUT. Reserved selectors reject. Downstream cascades are covered by rejecting their direct LSI roots. |
| Watchdog | Own manuals identify IWDT's separate RC10K source. It is not an LSI owner on these families, and its running deadline is not suspended or reconfigured. |
| Configuration gates | Own manuals identify RTC/UART/AWT/FLASH gates as configuration-only. Required controls are read with a qualified open gate and bounded restoration. Reset-held blocks reject; no reset is released or pulsed. |
| GPIO gates | AHBEN controls work and configuration. The accepted mode explicitly allows whole-bank functional progress during bounded inspection, including before rejection. Preservation/restoration cannot undo sampling, filters or armed events. |
| Writes and failures | CR0/CR1 use KEY0x5A5A; classic LSI/AHBEN/APBEN are unkeyed. Neighbor-preserving writes, WAIT/reserved preservation, no flag clearing, IRQ state changes, resets or unrelated peripheral writes. A failed restoration may leave a gate active and must prevent subsequent trim mutation. |
| Ownership | Existing pre-Rust bus-master and HAL register-ownership model remains. Ordinary critical sections do not stop DMA, NMI/debugger changes or peripheral state machines. The GPIO condition is a hardware functional handover, not a new hidden Rust memory-safety obligation. |
| Transition | Preserve admitted HSI calibration/owners, confirmed PLL→HSI→LSI bridge, legal bus guards and Flash bounds for all reachable transition/fallback envelopes. No direct PLL→LSI transition. |
| Publication | Permanent target LSIEN, final source/parameter/readiness/divider/HSI checks before frozen clocks; errors publish no new clock state. Poll budgets are finite attempts, not real-time or failed-clock progress guarantees. |

The root roster was independently checked against both own manuals' LSI references, selector tables, clock-control descriptions, GPIO, RTC, UART, AWT and IWDT chapters. No additional direct LSI engine or classic AUTOTRIM peripheral was identified in that source model. Source-level evidence does not reveal arbitrary external software or wiring expectations; the explicit functional handover remains material.

RTC SOURCE inspection reads the static configuration field in CR1, the register whose WINDOW field the documented access protocol itself polls. It does not perform time/date reads, unlock RTC, assert ACCESS or use START=0 to infer absent AWT/output users. This distinction must survive implementation.

## Rates and consumers

Own factory full-temperature intervals at nominal 32,800 Hz are F020 31,160–34,440 Hz (±5%), F030/A030 31,816–33,784 Hz (±3%), with 1.65–5.5 V and −40..105°C qualification. F020 uses the actual Rev1.3 under `current-datasheets/`, physical PDF44; the similarly named top-level file is Rev1.2 and is not the authoritative input. F030 PDF46 and A030 PDF43 are their separate own tables. Retained HSI qualification remains required.

Rate-only is an appropriate conservative contract: calibration accuracy, duty and startup specifications do not independently establish the new strict absolute per-cycle claim. Exact numerators/divisors must remain intact through AHB/APB and RTC division. The rate-only field, methods, error variants and ADC guard must not depend accidentally on PLL cfg. Classic ADC and complementary PWM/dead time must refuse unqualified cycle timing before hardware use, including zero requested dead time for complementary PWM.

The existing fixed 1 MHz time driver cannot obtain a nonzero integer divisor from any selected 32,800 Hz LSI PCLK. The existing `Config::frequencies` → time-driver validation → `Peripherals::take` order must reject before singleton ownership. Ordinary UART/SPI/I2C/timer rates remain each constructor's own admission problem. No fractional time base, different tick rate, sleep continuity or general driver compatibility is established.

## Validation and remaining implementation work

Validation was read-only primary-source comparison and inspection of the exact Stage55 code paths. Fifteen independently inspected code/data/document/example files still match their recorded hashes at completion. No Cargo invocation, HAL test, new harness/adapter or candidate/source edit was performed.

A later implementation still needs family-bound metadata and protocol implementation, meaningful admission/failure/readback coverage, cfg/API checks, required repository validation and separate implementation review. In particular it must preserve the public whole-bank behavior and failure phases, classify same-source RTC aliases consistently, and avoid moving trim admission behind the first CR1 write. These are explicit design requirements, not unresolved source-design blockers. Hardware startup, electrical behavior, source loss and peripheral operation have not been tested on silicon.
