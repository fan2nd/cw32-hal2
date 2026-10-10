# Stage54 native L011/L012 LSE and RTC runtime review

2026-10-09 UTC. Independent, read-only review of the Stage54 candidate. No candidate edits, Cargo or HAL tests/harnesses were performed by this reviewer.

## Final disposition

The reviewed runtime implementation supports the accepted bounded design. No unresolved runtime blocker has been established. One real example defect was reported and corrected during review: both new-family LSE examples retained the L010 PB1/PB0 constructor arguments. That would pass compilation on parts exposing those pins but fail `LseClock::acquire` at runtime. All four new features now use PC14/PC15 (crystal) or PC14 (bypass), and old L010 features retain PB1/PB0. Corrected example hashes are recorded in the final receipt and match the successfully linked exact-part examples.

Accepted for the bounded software runtime scope described below, after the source correction and final evidence correspondence check. The final authored-source manifest is SHA256 `afb2a651bc8e1b5f9246311b03d80cf88b1b70669a4bdc085ac19a3ae17ac38a`, 1188 files; every listed current file matches. The accepted generated manifest is SHA256 `5f0d08c8d6bfa1b4901dc5dccef6c0da248f1727309f8016c1c08f02b33e05f0`, 902 files, all matched. Runtime v1 is not used as a substitute for these final tested bytes. This acceptance is not silicon validation or a whole-HAL soundness proof.

## Identity and own-source evidence

The starting authored freeze is `runtime-candidate-v1-source-manifest.json`, SHA256 `2dd294eeb0d6a9b314f3eb46026638a63ca9a71b9a59ff263021d8f9be9c10a5`, 1188 files. The accepted Stage53 ZIP independently hashes to `778274a52b4994a0e379963afa8c870d522cfd42f3338cfacc950d13c7a4b032`. The accepted design independently hashes to `49c833504e231493cfe1c04f6f3656070dd0c6fceb858e92cbc042cd2f5c1d58`.

All seven original PDF/SDK artifacts and all five PDF text extracts match the design's original manifest. Vendor payloads were read locally and are not copied here. Own source IDs used below are:

- L011-RM: `vendor:CW32L011_UserManual_CN_V1.1.pdf`, SHA256 `b245887e9037caf267f739e25bd3c354c2579d23315fc06389e055f371159f4f`.
- L011-DS: `vendor:CW32L011_DataSheet_CN_V1.1.pdf`, SHA256 `0b7414049824881920fc38f829029e3ba0af88feb4536fb27351df0d60f688a5`.
- L012-CN: `vendor:CW32L012_UserManual_CN_V1.4.pdf`, SHA256 `a9e54694a26f03c1f3e3041f40844900168ca2b8e6142f3328671b92e6b7a340`.
- L012-EN: current `vendor:CW32L012_UserManual_EN_V1.0.pdf`, SHA256 `f56d5ed899dd090b469fac6a09084aab9f5068ae3b56cf7562b83997bd2f1088`.
- L012-DS: `vendor:CW32L012_DataSheet_CN_V1.0.pdf`, SHA256 `08a605fe8843d1eefa2b1115474b5a74df2833068f0ad6f35225e4f39377da76`.
- L011-SDK: `vendor:CW32L011_StandardPeripheralLib_V1.0.3.zip`, SHA256 `76adfe39360eb1d05ef58c25f26a8c1f99f2bc2f8fef677214aaca85cffc679e`.
- L012-SDK: `vendor:CW32L012_StandardPeripheralLib_V1.0.5.zip`, SHA256 `8b0a4c0ee865642d08f353aec98906c900a8b10f672120e649e6b1bf5e29c18f`.

Page notation below is one-based PDF/printed. L011 RM printed pages are PDF minus1; L012 manuals minus26; the datasheets minus3. The accepted design review remains design evidence, not runtime/build evidence. This review verifies runtime consumption of the already separately reviewed models rather than repeating their complete serialized-schema review.

## Source capability, monitoring and exact scope

`embassy-cw32/src/rcc/lse_native_low_power.rs:61–100` rejects zero poll budgets, invalid/inverted bounds, envelopes outside 1.7–5.5V and −40..85°C, board conditions outside source conditions, unavailable exact package pads, and insufficient monitored count margin before admission writes. The nominal source is32768Hz; maximum100kHz. L011-DS48/45 and L012-DS55/52 independently specify this bypass ceiling,450ns high/low minima and50ns edge maxima. The own RMs'1MHz claims are not used to broaden the feature. L011-DS50/47 and L012-DS57/54 give only typical1.50s startup, not a successful-start deadline.

StartupOnly requires inherited CCS0 and leaves it0. Runtime health remains an EN/STABLE/parameters/fault snapshot; the public enum and RTC docs explicitly state that later loss can remain invisible. Own L011-RM51/50,55–57/54–56,72/71 and L012-CN57/31,61–62/35–36,77/51 plus L012-EN59/33,65–66/39–40,82/56 distinguish startup counting from the enabled failure detector.

MonitoredExistingRoutes deliberately retains fault routing and resulting capture/IRQ/brake effects. It requires pre-existing stable LSI and own factory matching, without setting LSIEN or altering TRIM/WAIT in the leaf. `monitor_qualified` at lines119–138 reads the explicit aligned halfword0x001007C2, rejects erased0xffff and masks through the selected PAC TRIM setter:10 bits on L011,9 on L012. Own L011-RM70/69 and L012-CN75/49/L012-EN79/53 establish the address and widths. HSIOSC's separate0x001007C0 remains unchanged in the RTC data.

The admission snapshot freezes LSI TRIM/WAIT before the first gate write; start and later healthy checks enforce this same identity. `MONITOR_LSI` never acquires a different value silently. The sufficient count comparison uses u64 and exactly256×LSE_min >129×LSI_max, keeping hardware128 and engineering margin1 distinct. L011 uses41000Hz from L011-DS51/48 factory−10/+25%; L012 uses36080Hz from L012-DS58/55 ±10%. Their minimum integral declarations are20661Hz and18181Hz. L011's narrower manual/legal range is not conflated with the factory envelope. Matching and STABLE are explicitly not frequency/jitter measurements; the accepted legal-platform and board/window conditions remain real limits.

`embassy-cw32/build.rs:3610–3903` binds the emitted values to family policy,0x001007C2,4-bit drive fields, own10/9-bit TRIM,128/256, margin1, exact selectors and1/16384/32768 RTC divisors. Exact active-LSE scope at lines3906 onward adds only L011K8T6/K8U6 and L012C8T6/C8U6. No PLL or additional family is inferred from the common module name.

## Fresh admission, reuse and permanent pad ownership

Fresh admission at leaf lines214–259 rejects enabled/locked/readiness/fault/IRQ ownership, SYSCLK=LSE and MCO=LSE; it refuses required held resets without manipulating them. Reused sources instead require exact parameters, selected monitoring semantics and current health. Raw PINLOCK reuse is refused only on the two new families (lines379–389); LSELOCK-only exact reuse may remain locked and performs no oscillator write. Running sources are never stopped, retuned or switched between monitoring modes.

`rcc/mod.rs:123–139` unions inherited and requested pad ownership before backend admission, including error returns. `build.rs:2351–2362` derives inherited ownership from EN and applicable locks; no fault/Drop/readback path shrinks the permanent set. `gpio::Flex::new` checks reservation before gate/unlock/pad writes. LseClock verifies the frozen config and exact physical token identity, consumes SYSCTRL plus the actual pads and does not release the reservation after failed acquisition, Drop or forgetting. Frozen clocks are published only on backend success.

The commit sequence at leaf lines610–689 rechecks the central image, consumer selectors and frozen monitor source; it changes only MODE/DRIVER/PDRIVER/WAIT while EN0, verifies them, then keyed-RMW enables LSE and the requested CCS. Faults win over readiness. CR2/IER remain untouched and are verified; source health is checked before and after final pad verification. `RCC_LSE_CHANGE_MASK=0x00040f7f` permits only requested fields plus RO STABLE in the final preservation comparison. No flag clear, fault recovery or failed-start rollback is supplied.

## Consumer inspection and honest functional limits

`configuration_consumers` at leaf lines264–371 opens only qualified configuration domains. RTC quiet-source0 admission excludes CR0 except H24, WAIT, CR2, compensation, IER and ISR. It does not read/write a nonexistent ACCESS field on either new family. UART selection2 is rejected regardless of TX/RX enables. L011 additionally inspects UART3; own L011-RM78/77 and79/78 identify the corresponding configuration-only gates. L012 additionally checks both master and slave I2C CLKSRC, rejecting reserved1 and LSE2 while leaving0/3 uninterpreted. Own L012-CN573/547,583/557 and EN626/600,636/610 establish separate selectors; the master code3 conflict remains visible.

LPTIM checks enabled use of ICLKSRC=LSE and hardware RTC events with SOURCE0: codes1–4 on L011 (RM201/200, where5 is PC13),1–5 on L012 (CN253/227, EN273/247, where5 is RTC_1Hz). The new-family native CR/CR0 and3/4-bit selector differences are direct PAC branches, not copied L010 assumptions.

L012 UART3 is read only when already gated-on and reset-released. A closed gate is left closed and is not counted as evidence of no owner. This preserves the material CN83/57 versus current EN88/62 configuration-only/configuration+work conflict. The public init, source and implementation docs explicitly describe inaccessible UART3, unopened output banks, external users and downstream timer roots as unverified functional handover conditions.

GPIOA/B/F are never opened for output probes. Already operational routes are vetoed using digital-output direction, analog-off and exact AF: L011 RTC PA1/PA3 AF3; L012 also PB14/PB15 AF4, PC13 AF4 inside the GPIOC window, and direct LSE PB12 AF4/PF1 AF3/PF3 AF1. Own route pages are L011-RM122–123/121–122 and L012-CN154–155/128–129, EN167–168/141–142. No dormant BTIM/GTIM/ATIM operational gate is opened. In particular, the L012 BTIM/ATIM source disagreements are not converted into a positive runtime disconnection proof. Reserved RTC1HZ0, START0, stopped counters and reset-like selectors do not establish absence of recipients.

## GPIOC operation and central gate restoration

New-family `pads` at leaf lines546–597 opens only GPIOC after checking its active-low reset. L011-RM77/76 and L012-CN82/56/EN87/61 identify that gate as controlling the entire bank's configuration and operation. The public APIs disclose sampling/filter/armed-edge progress before errors and during reuse/acquisition. Restoration does not undo progress and IRQ masking is not claimed to suppress asynchronous effects.

Native PC14/PC15 checks use AFRH, correct3-bit AFR fields on L011 and4-bit PIN fields on L012; crystal requires both analog inputs, bypass transitions only PC14 ANALOG. Output/open-drain/pull-up/filter/rise/fall conflicts on claimed oscillator pads are rejected. PC13 and all unrelated pin controls, shared filter clock and event flags are never written. L012 PC13 RTC_OUT is inspected inside the admitted operational window before oscillator enable.

The central `RccInfo::inspect_for_init` uses keyed neighbor-preserving RMW, bounded enable/restore readbacks and an attempted restoration even when enable readback fails. Closure rejection is restored before propagation. Restoration failure is an error; there is no successful-restore claim on such an exit. No reset, unlock or event clear is used as an inspection primitive.

## RTC native protocol and divider bridge

L011-RM140/139,152/151,154/153 and L012-CN202/176,204/178 plus EN208/182,219/193,221/195 establish selected-source PSC1/PSC2, RTCCLKD≤1MHz,2Hz TICKCLK and native WAIT with bits1:0 reserved. `rtc/mod.rs:181–218` drains WAIT before DATE, checks clock health after the bounded wait and rejects retained control/ISR ownership twice before unlock. No ACCESS read or write is added to these families.

`initialize_native_source` at lines440–486 first rejects an unsafe target first-stage rate. It stages actual PSC1 to max(incoming actual divisor,target actual divisor), preserves PSC2, verifies staged PSC1, checks health, selects/verifies the requested source, checks health, and only then writes/verifies final PSC1/PSC2. The incoming RTCCLKD cannot increase under the staging step. The requested source is bounded before selection:97.92MHz/120=816kHz for HSIOSC, and at most100kHz/1 for LSE. Switching HSIOSC→LSE does not transiently expose the old high frequency through divisor1; switching LSE→HSIOSC has divisor≥120 first. The guarantee assumes legal incoming clocking and does not claim retrospective repair.

CalendarClock retains the one-argument HSIOSC constructor, now HsiOsc/Lse on the four new profiles. LSE selects SOURCE0 with encoded PSC1=0/PSC2=0x3fff, actual1/16384 and nominal1Hz calendar transitions. HSIOSC remains96MHz with120/400000 defaults. Attach verifies current source, actual first-stage upper ceiling, exact nominal factor pair, disabled compensation and exact LSE pair; it never changes source or calendar to manufacture compatibility. Reads, initialization and date writes revalidate health after completed WAIT/poll boundaries. StartupOnly remains intentionally unable to detect all loss.

`Rtc::initialize_if_unset` and module docs explicitly extend downstream ownership to later RTC activation, not merely oscillator startup. They enumerate external pads, BTIM/GTIM/ATIM/LPTIM roots and cascades, including all disputed L012 alternatives. Owning PC14/15 and RTC does not magically own these other recipients. Partial failure may leave divider/source changes, without elapsed-time continuity or automatic fallback.

## Old behavior and safe-API boundary

Against the accepted Stage53 archive, the prior16 LSE configuration objects are exactly equal and retain no native profile. Each of the three L010 profiles retains all nine original values/routes,36080Hz, inherited_legal policy, margin1 and null factory address. The refactored L010 runtime branch uses its original GPIOB, ACCESS, LPTIM1–5, PINLOCK reuse and inherited-legal monitoring semantics; additional new-family checks are cfg-excluded. Its shared backend delta is documentary only. Classic LSE code, other-family clock backends, ClockBounds, GPIO implementation and RTC source-data file are byte-identical to Stage53. PLL rate-only and electrical behavior are therefore not widened by this change.

For `lse=None`, new admission/calibration/pad checks are not called. L012's expected-state helper resolves to the same old EN/CCS comparison and retains whole-register LSE preservation. For an explicit admitted request, only the expected EN/CCS transition and qualified LSE writable/status mask replace those equalities. Central reset/gate semantics stay shared.

The public docs preserve the existing pre-Rust bus-master quiescence platform boundary, including armed/gated and pending memory requests. Newly described output, timer, UART3, GPIO and real-fault observer limits are stated as functional support conditions, not safe-call memory-safety duties. No new raw DMA programming, ownership bypass, unsafe interrupt invocation or aliasing construction was found in this delta. The only added raw read is the own documented fixed factory halfword. This is not a whole-HAL soundness proof, and the existing raw PAC integration boundary is not newly certified here.

## Remaining limits

No board electrical/startup distribution, physical fault timing, jitter-per-window, retained-reset, sleep/resume, oscillator recovery or elapsed-time continuity was measured. The unresolved L012 gate/timer mappings and native PINLOCK fault semantics remain explicit scope exclusions. Software generation, exact-part builds and ELF inspection are separate evidence; their final-source correspondence was checked as described next.


## Final corrections and validation correspondence

The implementer recorded three small post-v1 corrections, all independently read here: the exact current L012 EN source requires the canonical `corroborating-language-edition` status while every other accepted source still requires `selected`; the example physical pins were corrected as described in R1; and the legacy GPIOB poll helper gained `#[cfg(rcc_cw32l010_v1)]`, eliminating a new-family unused warning without changing an executed path. Final leaf SHA256 is `0259c1b0d7d53fcf8c9f5a97e78b3905ae6164a309a537a7aeb6ce0deb727c5f`.

The executable-source freeze v4 has SHA256 `831a76642b33735ab76abe4afe40a1a5d31306e5035de13652a4ddc2a39ce377`. The final authored release differs from it only in six documentation/capability files: README, capability YAML, functional coverage, native implementation note, qualified-LSE overview and example README. These edits were reread for the exact23=16+7 scope and monitoring/handover limits. Runtime, generators, selected register layouts and example Rust files match the validated v4 freeze.

Observed implementer receipts establish successful full data/PAC generation,150 existing host data/schema tests,25 release ARM HAL builds (23 active exact parts plus generic L011/L012), and25 linked calendar ELFs (seven native packages × crystal/bypass/HSI plus four classic representative crystal examples). The initial generator source-status failure and stale derived provenance-view failure remain preserved; corrected generation and the derived-view source audit passed. This reviewer did not execute those commands or claim a HAL-test/hardware run.

The final ELF manifest SHA256 is `a45282983df060929a015c25a0c0985cc62da4ae702eaf002387c173e63ec7dd`. All25 saved ELF bytes independently match their hashes and sizes. All11 associated build-receipt/log pairs hash correctly and report exit0. The saved ELF structure review, SHA256 `b1c47e8cd87f63a1cbbac61b7ab12a50283dc75ca4d1af2a03af58d9f28eb4b1`, reports passed for all25 and binds the same final source, generated and ELF manifests. Its hardware_execution is false. This review inspected these existing receipts rather than running the ELF verifier or a new harness.

`receipt.json` is the final disposition; `final-correspondence.json` records the independent read-only hash checks. The earlier checkpoint receipt remains historical. Acceptance must not be carried to subsequently edited source bytes without reviewing their delta.
