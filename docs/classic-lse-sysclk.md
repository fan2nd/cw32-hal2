# Init-only LSE system clock on three exact classic packages

`Config.lse = Some(...)` and `Config.sys = Sysclk::LSE` select the one declared,
board-qualified nominal 32768 Hz oscillator on **CW32F020C6U7, CW32F030C8T7 and
CW32A030C8T7 only**. PC14/PC15 are package pins 3/4, QFN48 on F020 and LQFP48
on F030/A030. Bypass needs PC14; crystal needs both pads. Inherited pad reservations
remain a union and are not released after an error. Other exact packages, family
aliases, the native seven and larger thirteen LSE packages are not extended.
The existing 23-package auxiliary LSE qualification remains unchanged.

This adds a variant to the public exhaustive `Sysclk` enum on these three parts.
Downstream exhaustive matches must handle `Sysclk::LSE`; this is a Rust source
compatibility change. The default remains HSI. Existing HSI/HSE/PLL/LSI targets,
including their auxiliary `Config.lse` admission and late startup order, retain
previous behavior. No second LSE or user-selected LSI declaration is required.

## Source and timing contract

The same validated `(Lse, ClockBounds)` tuple supplies SYSCLK/HCLK/PCLK and the
owned `LseClock`/RTC calendar source. The 32768 RTC division uses that tuple;
factory LSI's nominal 32800 Hz is not substituted. Every individual LSE cycle
must fit the board's bounds across load, supply, ambient temperature, aging and
short-term variation. Average accuracy alone is insufficient. Board qualification
also covers crystal drive/load and bypass voltage, duty, pulse width and edges.
The HAL does not measure these assertions or establish a maximum startup time.

The own manuals specify a detector count of 128 LSE edges per 256 LSI cycles.
The new SYSCLK target alone adds one software margin edge and requires, in wide
integer arithmetic, `256 * LSE_min_hz > (128 + 1) * own_factory_LSI_max_hz`.
F020's maximum is 34440 Hz, so integral minima start at 17355 Hz; F030/A030 use
33784 Hz, giving 17024 Hz. This is a modeled count-margin policy. The factory
LSI row is rate-only: neither it nor STABLE establishes an absolute cycle-jitter
bound, every-window guarantee, fault deadline or guaranteed CPU progress.
General auxiliary `Lse::bounds` is unchanged.

Configured HSI is separately validated under the final AHB/APB divisors and
retained. Flash wait covers the larger of declared LSE HCLK and configured HSI
HCLK. HSI fallback is not folded into the normal LSE clock bounds. After LSE loss
or automatic fallback, frozen LSE SYSCLK/HCLK/PCLK bounds and dependent timing
assumptions are invalid; no transparent fallback, calendar continuity, elapsed-time
accuracy, runtime switching, power-mode restore or physical recovery is promised.
All nominal LSE PCLK divider combinations are below 1 MHz, so existing time-driver
validation rejects them before singleton acquisition or RCC MMIO.

## Bounded initialization and handover

Pure validation requires the LSE declaration, nonzero budgets and all source and
operating conditions. An optional PLL declaration still requires PLL SYSCLK.
LSE preflight exactly verifies an already running source and its pads; fresh
startup requires the existing complete consumer, RTC-reset and idle-pad admission.
No active LSE is stopped, retuned or unlocked to gain admission.

The existing Stage56 factory-LSI preparation runs before the first CR1/mandatory
CCS write. Requested/selected factory-matching LSI is reused without TRIM/WAIT
writes. A nonmatching live source rejects. A stopped source, even with matching
trim, requires two complete consumer passes and a final use-edge pass. Raw erased
factory halfword 0xFFFF rejects before typed masking; only admitted TRIM changes.
WAIT and reserved fields remain. This cold admission can briefly open entire
GPIOA/B/C/F banks; sampling, filters and armed events may advance, including before
an error. Restoring a gate cannot undo that effect. GPIO configuration and flags
are preserved. This is a functional handover limit, not an extra hidden memory
safety obligation. The existing pre-Rust quiescent bus-master boundary still applies.

The first source-control write requests HSI and permanently requests detector LSI
with mandatory CLKCCS/HSECCS/LSECCS. Direct/mirrored LSI readiness and frozen
parameters are verified. An inherited PLL remains enabled with its reference
unchanged; an HSI-fed PLL requires already matching trim/divider. Inherited PLL
selection leaves through confirmed HSI. An independently admitted HSI calibration
may use the existing temporary LSI bridge while keeping the detector request.

After independent requested source preparation, LSE starts or is exactly borrowed
while HSI is selected. Complete source/pad, mandatory monitor, factory-LSI,
retained-HSI and inherited-PLL/reference checks precede the transition. Final
AHB/APB dividers are installed with an explicit HSI selector, then final Flash wait.
After revalidation there is one LSE selection and no later CR0 divider RMW or
source cleanup. Complete checks and final Flash/mux/divider readbacks precede
publication. Successful new-target init freezes detector parameters for later
LSE health checks; old auxiliary-LSE paths do not acquire that extra obligation.

This permits admitted ordinary cold entry, not arbitrary warm handover. For
example, running LSE plus stopped/unadmitted LSI rejects because cold LSI admission
excludes external requests/stability/CCS. A matching enabled LSI and exactly
matching running LSE can be borrowed. Do not weaken admission or falsely assert
prior calibration to force entry.

All waits are poll counts, not wall-clock deadlines. A failure publishes no clocks
or peripheral tokens, keeps reservations/diagnostics and may leave partial Flash,
bus, calibration, gate or source changes. Reset before retrying; ordinary reset
may retain LSE, so POR can be required. Entry must already have a valid stable
clock/voltage/Flash state. The existing contract excludes arbitrary asynchronous
source loss during register RMW. The new selector ordering is a prospective
constraint, not a claim that old paths have a proven contract violation.

## Own-source evidence and verification limits

[The source qualification](classic-lse-sysclk-qualification.json) binds original
IDs, hashes and PDF/printed page pairs. Counts are authored in the existing exact
package LSE YAML, projected through generated metadata and consumed by build.rs;
LSI maximum/conditions/address reuse their own Stage56 family facts. The selector
uses the existing typed PAC. No new raw-register adapter or HAL test harness is
introduced.

Principal pages: x030 RM CN2.5 PDF51–52,57,59–60,62–64,70–71,74,76,79–80,112;
F020 RM CN1.4 PDF49–50,55,57–58,60–62,68–69,72,74,77–78,110. The current own
F020 DS CN1.3 is the `current-datasheets/` original; F030 DS CN1.9 and A030 DS
CN1.1 independently establish factory LSI and board electrical limits.
Build and source checks do not constitute silicon startup/fault injection,
physical jitter/detection-window, board, RF or loss-recovery qualification.
