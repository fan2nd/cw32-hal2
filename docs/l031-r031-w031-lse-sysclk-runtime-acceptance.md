# Independent runtime source review: Stage58 exact-five LSE SYSCLK

**Verdict: no blocking runtime source defect found in runtime-freeze-v1. The authored runtime slice conforms to accepted design revision 2. This is a pre-generation, pre-compilation source review, not an executed HAL, MMIO, hardware, or publication pass. Publication remains HOLD.**

Reviewed scope is the init-only nominal 32768 Hz LSE system target on CW32L031C8T6, CW32L031C8U6, CW32L031F8U6, CW32R031C8U6, and CW32W031R8U6. The seven-file runtime manifest is `8b2ed2b7ff22b4062bde029e5105290620f773e5adbe2fbe4189f2aaa2ed6b14`. Every listed file independently matches its hash, size and mode. Exact reviewed bytes are preserved under `reviewed-runtime-v1/`; `runtime-freeze-v1-check.json` records correspondence.

The design is `external-evidence/stage58/l031-lse-sysclk-design/design-v2.md`, SHA-256 `6661bb5461df1607f261af120ae589bf0d778fbdc87e4a781546a94602dd43ca`. The separate accepted design report was read. Stage57 source ZIP independently hashes to `4399316e13463450c2429ac8b830c6e1af8f3f46215f61f1631dcb8104d61671`; all 1,229 regular archive files match its baseline directory.

Any change to these seven runtime files requires a supplemental freeze and review of the changed bytes. This review does not cover an unfrozen replacement, generated metadata, new generator/build assertions, source-proof negative tests, or a later compile result. Those remain separate acceptance gates.

## Independent own-source evidence

I rehashed the six original source PDFs, independently extracted their relevant pages, and read the own-family LSI start/calibration/status, selected-source stop rule, detector, selector, CCS, Flash, RTC and electrical/package evidence. All original hashes match. The three own CR1 page renders were also visually inspected. Evidence and physical-page lists are saved in `docs/l031-r031-w031-lse-sysclk-runtime-source-evidence.json` and `external-evidence/stage58/l031-runtime-review/evidence/`; none of the three families is qualified merely by similarity to the classic cohort.

| Fact | L031 RM CN1.6 | R031 RM CN1.3 | W031 RM CN1.4 |
|---|---|---|---|
| LSI startup / selected-source rule | 54 / 55 | 56 / 57 | 55 / 55 |
| Factory calibration / startup latch | 56 / 57 | 58 / 59 | 57 / 58 |
| Runtime detector | 59 | 61 | 60 |
| Standard and LSE switch | 62–63 | 64–65 | 63–64 |
| CR0 / CR1 | 67 / 68 | 69 / 70 | 68 / 69 |
| LSI register / ISR | 71 / 75 | 73 / 77 | 72 / 76 |
| Flash waits / RTC start | 107 / 170 | 109 / 172 | 108 / 171 |

Each manual establishes LSE selector 4, independent CLKCCS/HSECCS/LSECCS controls, 128 LSE edges in 256 LSI cycles, LSI enable dependency, ten-bit factory LSI TRIM at 0x00100A02, and parameter setup before enable. Direct STABLE and the ISR mirror represent startup state, while the ready event is separate; STABLE is not continuous source validity. Selected-source stop inhibition makes EN=0 insufficient as a general stopped proof.

Own datasheet physical pages are L031 CN1.9 26/38/45/46/47, R031 CN1.2 29/42/52/53/54 and W031 CN1.3 30/41/51/52/53. These support the own pads, nominal LSE, maximum external envelope, factory HSI ±2%, factory LSI ±3%, and board conditions used by this design. The conservative W031 2.0–3.6 V LSE/monitor scope stays within both listed RF supply-mode ranges without qualifying RF. The Flash WAIT2/72 MHz row does not authorize a 72 MHz bus. The 1.50 s crystal-startup figure remains typical, with no maximum promise.

## Runtime findings

References below are to the frozen `embassy-cw32/src/rcc/l031_r031_w031.rs` unless otherwise stated.

### 1. Pure admission remains target-only and preserves the single source

`frequencies()` lines 282–376 computes the existing `(Lse, ClockBounds)` tuple once. Only the `Sysclk::LSE` arm requires it, performs the wide strict detector inequality, and qualifies factory-monitor board conditions. No new check was moved into general `Lse::bounds`, and no factory-LSI `ClockBounds::rtc_source()` is used for the CPU source.

The exact board LSE bounds become `Clocks.source` and the same tuple remains in `Clocks.lse`. HCLK/PCLK retain exact cumulative dividers; later `LseClock` acquisition retrieves this tuple unchanged. Both old HSE fallback qualification and new LSE final-HSI qualification use the existing bound getters, so changing the temporary `Clocks.source` is sufficient despite copied nominal display fields.

Independent integer arithmetic confirms 17023 Hz fails the detector policy by 248, 17024 Hz passes by 8, configured HSI/2 at AHB/1 has upper HCLK 24.48 MHz and needs WAIT1, and HSI/1 at AHB/1 exceeds the 48 MHz bus ceiling. These checks are scalar admission calculations, not evidence of per-cycle LSI accuracy or every-window detection.

All 32 AHB/APB pairs give nominal PCLK from 32768 to 32 Hz. Unchanged `time_driver::validate_clock` rejects the lack of a positive integral divisor for 1 MHz. `lib.rs` lines 200–206 performs that validation before `Peripherals::take` and RCC MMIO. This is source/arithmetic verification; the 32 configurations were not compiled or executed by this reviewer.

### 2. New-target LSI classification matches the accepted state table

`LseSysclkState::admit()` lines 1148–1176 reads and rejects raw 0xFFFF before ten-bit masking. Zero and non-erased high-bit words remain legal. It captures WAIT and inherited policy/source state, rejects selected-but-unenabled/unready LSE, and classifies the monitor before its parameter/request changes.

The classifier at lines 1184–1217 admits exactly the accepted classes under unchanged WAIT/CCS/lock state:

- Factory-matching, EN1 and both stable indications high: unchanged borrow, including selected LSI, preserving ready observers.
- Factory-matching, EN1 and both stable indications low: bounded in-flight startup only with no selected LSI and no external detector.
- EN0 and both stable indications low: cold admission only with no selected LSI, detector or ready observer. Matching TRIM takes the existing no-write shortcut; mismatching TRIM still requires the separate stopped/consumer proof.
- Mixed stability, EN0 stable/selected, selected unready LSI, or detector use without a requested, dual-stable factory reference: rejection.

`external-evidence/stage58/l031-runtime-review/state-classification-check.json` records an independent accepted-class union versus the actual source predicate over 512 Boolean combinations, with no differences. This is a reviewer model enumeration, not execution of Rust or emulated registers. The unchanged parameter/policy comparisons and erased-word rule were separately read.

The request-edge reclassification additionally requires factory-matching TRIM. After the LSI request/readback, the new full readiness loop at lines 752–759 requires request, both current stable indications, factory TRIM, unchanged WAIT and preserved CCS/lock. Mixed polls never satisfy it. Every loop has an explicit iteration budget. An admission read straddling startup can conservatively reject; the code and docs do not call that a silicon fault.

### 3. Factory reuse, cold trim and enable effects remain distinct

`prepare_lse_monitor` is byte-for-byte identical to Stage57. A matching native TRIM still returns without parameter writes or the mismatched-trim owner proof, after the existing disabled-source ready-observer check. Enabling the matching cold source can resume parked direct consumers; `lib.rs` and the new document expressly state this functional effect and whole-bank sampling/filter/event effects.

The mismatching branch retains two stopped/consumer/stopped passes, checks both stable indications low, EN0, both detectors clear, no ready observers and unchanged TRIM/WAIT, then performs a trim-only RMW. The generated consumer implementation was read: it first invokes complete LSE/RTC admission, then excludes selected/reserved SYSCLK LSI, MCO, AWT, all UART LSI choices, GPIO FLTCLK5 and the exact bonded PB11 output. No classic proof, IWDT assumption, AUTOTRIM, LPTIM or LCD requirement is imported. This remains the accepted family-specific software proof, not an autonomous-request or physical-stop oracle.

### 4. HSI bridge and final source sequence follow the required order

The existing HSIOSC AWT/LVD retained-user restrictions, optional-HSE RTC/AWT admission, WAIT2 setup, monotonic AHB/APB guards, unchanged-HSI escape and optional HSI-trim bridge are preserved. New full LSI readiness precedes selecting temporary LSI or stopping/trimming HSI. `needs_lsi` remains true for this target, so bridge cleanup cannot withdraw LSIEN.

The dedicated branch at lines 866–953 runs after configured HSI and optional HSE are established and before the old final-selector tail. It checks full monitor readiness and HSI execution, then starts/reuses LSE. The common fresh path repeats preflight, freezes the monitor, configures pads/parameters while disabled and adds only LSECCS/LSEEN; exact reuse performs no oscillator/pad reconfiguration. Relevant faults are never cleared.

The full tree predicate checks LSI factory parameters/dual-state/request/CCS/lock; configured HSI request/STABLE/TRIM/DIV; LSE request/parameters/STABLE/faults; and requested or inherited HSE state/parameters/faults. Exact reused LSE retains the captured native register value. Source checks surround gate-preserving LSE and applicable HSE pad inspection.

The final divider write explicitly selects HSI with final AHB/APB. All three fields are acknowledged before barriers and final Flash latency. Flash WAIT uses the larger upper HCLK of the LSE and configured HSI trees. The final LSE selection is one keyed CR0 write. It is followed only by bounded reads, barriers, pad inspection/gate restoration and success bookkeeping; no CR0/source/divider write follows. Complete tree and Flash checks precede the last selector/divider readback and publication. Fault or fallback observations return an error.

### 5. Early return retains ownership and success is not left on failures

Lines 932–938 reproduce inherited HSE ownership in `clocks.hse` using captured inherited enable/mode before the new early return. Requested HSE already occupies that field. The full source check has established the applicable unchanged inherited or exact requested HSE state and pads.

Unchanged common RCC initialization reserves the union of requested/inherited LSE pads before backend work and never releases it within the boot. A failure can retain requests, admitted calibration, guard divisors, gates and the diagnostic common monitor snapshot. No cleanup attempts to stop LSE, withdraw required LSI or clear diagnostic faults.

The target success marker is assigned only at line 951, after every fallible target operation and the final mux/divider check. Only infallible success return and `CLOCKS` assignment remain. All earlier errors leave it unset. A later time-driver initialization is the existing separate publication case; this target cannot reach it when the fixed driver feature is enabled because pure validation rejects first. Public retry after an RCC hardware failure is not supported and consumes the singleton.

### 6. Old paths and later RTC health are preserved at their proper boundaries

`external-evidence/stage58/l031-runtime-review/preservation-check.json` establishes that removing only new target-guarded blocks, its unreachable enum arm and a changed comment makes the entire old-target `configure` body byte-for-byte equal to Stage57. The old monitor helper is also byte-identical. Thus old HSI/HSE targets retain their hardware read/write order, matching shortcut, mismatch proof and late auxiliary-LSE start.

The shared LSE leaf differs only by the backend-gated marker hook. With no successful LSE system target, it returns true without any additional peripheral register read and leaves the existing auxiliary readiness predicate unchanged. It adds private critical-section/Cell bookkeeping, so this is not a claim of instruction-for-instruction timing identity. Other backend gates exclude the hook. The classic backend, L052/L083 backend, native leaf, common bounds, operating validation, RTC acquisition and fixed driver are all independently byte-identical to Stage57.

After successful new-target init, later shared-LSE health additionally requires the frozen factory LSI dual-state/request/parameters and retained policy/lock. The existing LSE request/parameter/fault checks remain in force. Unchanged `LseClock::acquire` verifies the frozen tuple and pads without reconfiguration, and its `is_ready()` feeds ordinary RTC operations. `CalendarClock::Lse` retains the same bounds and /32768 divisor. The health hook does not falsely require CPU SYSCLK to remain LSE for an independent LSE calendar source, and no frozen CPU-rate validity after fallback is promised.

## Limits and next gate

No candidate file was edited; no Cargo command, project generator, HAL test or compiler was run. The source freeze, baseline correspondence, source extraction, source-level projection and independent arithmetic are recorded separately and do not substitute for later generated-output review or compilation.

No runtime defect requires a corrective supplemental freeze now. A later runtime edit must be supplied with a supplemental manifest and reviewed before its checks are treated as accepted. Exact-five metadata admission, fail-closed source proofs, derived output correspondence and any compile diagnostics remain the responsibility of their separate gates.

Maximum crystal startup, physical oscillator loss, detector windows, fallback latency, CPU progress and silicon behavior remain unestablished. RF, arbitrary asynchronous source loss during earlier RMW, low-power/wakeup behavior, post-fault clock republication and calendar continuity remain excluded. HOLD remains in effect.
