# Bounded active LSE qualification

Active LSE configuration is qualified on the eleven exact parts below. Every family alias and all other parts retain only previously established pad-ownership facts.

| Part | Package | PC14 / PC15 physical pins | Direct LSE output |
|---|---|---|---|
| CW32F030C8T7 / CW32A030C8T7 | LQFP48 | 3 / 4 | PB12 AF3; PF1 AF1 |
| CW32F020C6U7 | QFN48 | 3 / 4 | PB12 AF3; PF1 AF1 |
| CW32L031C8T6 / CW32L031C8U6 | LQFP48 / QFN48 | 3 / 4 | PB12 AF3; PF1 AF1 |
| CW32L031F8U6 | QFN20 | 1 / 2 | Neither route bonded |
| CW32R031C8U6 | QFN48 | 2 / 3 | PB12 AF3; PF1 AF1 |
| CW32W031R8U6 | QFN64 | 61 / 62 | PB12 AF3; PF1 AF1 |
| CW32L052C8T6 | LQFP48 | 3 / 4 | PB12 AF3; PF1 AF1 |
| CW32L052R8S6 / CW32L052R8T6 | LQFP64 7×7 / 10×10 mm | 3 / 4 | PB12 AF3; PF1 AF1 |

 Qualification is projected after exact-package expansion through `ClockLimits.lse_configuration`, not placed in family electrical profiles.

The source facts were checked on 2026-10-09 against the exact originals identified below. This is source qualification, not silicon testing.

## Own-source evidence

The canonical authority is `sources/evidence-sources.json`. New citations use canonical `vendor:` IDs in `source_ref`; local labels are in `citation_ref`.

| Original | SHA-256 | Relevant PDF pages (one based) |
|---|---|---|
| CW32x030 User Manual CN Rev2.5 | `1afd49261f0f0689af8cb8ebf1b0ac1c00e3209b20d3c722106707ff4a10bdd2` | 51–52 modes; 59–60 detectors; 64 sequence; 71 CR1; 76 LSE; 79–80 status; 94 MCO; 152–154 GPIO; 173 standalone AWT; 184 RTC wake; 187–195 RTC reset/controls; 357 UART |
| CW32A030 Data Sheet CN Rev1.1 | `690433f36376352ee342ae3e2aa76786f2e7e59892a2519411f2754ac32c33a9` | 22–23 bonded pads; 27 direct output AF; 35 conditions; 41 bypass; 42 crystal |
| CW32F030 Data Sheet CN Rev1.9 | `04ef91434320e5d05a3b6690fead0fedb31a7d9bad28c96b655a6e13a22e46b2` | 24/26 bonded pads; 30 direct output AF; 38 conditions; 44 bypass; 45 crystal |

PDF page number is the printed page plus one for these originals. The common manual explicitly covers both F030 and A030. F030 SDK V2.2 headers independently corroborate field locations and encodings; SDK absence is never evidence of an absent consumer or automatic request.

The machine-readable review is in [lse-active-first-cohort.json](lse-active-first-cohort.json) and [lse-active-rtc-admission.json](lse-active-rtc-admission.json). Their exact digests are locked by [lse-qualified.yaml](../cw32-data/lse-qualified.yaml). F020 uses its separate [own-source review](lse-active-f020.json), [RTC admission](lse-active-f020-rtc-admission.json) and [current-original correspondence](lse-f020-source-receipt.json). It does not inherit x030 citation authority.

F020 RM CN Rev1.4 SHA-256 `279521eaf7d32b241e614a9553044a4d99c1e7ca2566efa3b0fdd2b66c106eed` covers mode/start at PDF49–50, mandatory CCS at69, LSE at74, detectors at57–58, gate/reset at80–85, MCO at92, AWT at170, RTC compensation at177, wake at181, reset roster at184–192 and UART at295. Its selected own datasheet CN Rev1.3 SHA-256 `1e330d800f10114c654b97cbd45b64d56a99c3cb39138d4ef20de3c12968dab0` covers QFN48 bonding at PDF23–24, output routes at28, supply/temperature at36 and external-input/crystal conditions at42–43.

F020 Table5-2 was visually inspected: PC14/PC15 are QFN48 pins3/4; QFN20 and QFN32 have neither oscillator pad. Thus F020F6U7, F020K6U7 and family aliases remain unqualified. The official F020 SDK LSE enable function incorrectly names PC13/PC14; the own datasheet governs the implementation. The obsolete same-basename PDF actually printed Rev1.2 is not used.

## Board contract

Both crystal and bypass use nominal **32,768 Hz** in this cohort. The board must supply a positive minimum and maximum frequency that bracket the nominal value and cover **every individual cycle**, including tolerance, temperature, supply, aging and jitter. A nominal frequency does not establish an accuracy bound. The own-source ceiling for both crystal/ceramic and bypass is **1 MHz**; this does not expand the cohort to other nominal frequencies.

The x030/F020 qualified operating conditions are **1,650–5,500 mV** and **−40 to +105 °C**, with VDDA equal to VDD and the device's remaining electrical conditions satisfied. The datasheets' conditional low-power extension to +125 °C is deliberately not projected.

Crystal mode requires PC14 and PC15 configured as analog. Crystal manufacturer characteristics, load capacitance, layout parasitics, drive and amplitude remain board responsibilities. The datasheets give **1.5 s typical** startup, with no maximum; it must not become a guaranteed timeout.

Bypass requires PC14 as a digital input. PC15 can be general-purpose GPIO under the documented mode, subject to ownership retained by the HAL. The input must satisfy **45–55% duty cycle**, **at least 450 ns high and low**, **at most 50 ns rise and fall**, high level **70–100% of VDDIO**, and low level **0–30% of VDDIO**. The waveform and voltage obligations cannot be established by software frequency bounds alone.

## Native oscillator and consumer facts

`LseDrive` and `LseAmplitude` expose all four documented encodings 0–3. `LseWait` exposes 256, 1,024, 4,096 and 16,384 LSE cycles at encodings 0–3. These enums are authored in `sysctrl_v1.yaml` and own F020 `sysctrl_cw32f020_v1.yaml`; metadata stores the cycle counts, not an alternate register-access abstraction.

`CR1.LSEEN` is the software enable; `LSE.STABLE` is a separate read-only observation. The configuration must be completed before enable and remain unchanged while enabled. `LSELOCK` only prohibits clearing LSEEN; it is not a lock on the other oscillator parameters. Only power-on reset resets the documented LSE register and LSEEN state.

CR1 writes require **KEY=0x5A5A** in bits 31:16. The x030/F020 own CR1 tables explicitly require **CLKCCS, HSECCS and LSECCS to be written as 1**. Those existing backends retain that requirement. L031/R031/W031 instead have configurable CCS controls as detailed below. Startup failure is `ISR.LSEFAIL` bit 5; running failure is `ISR.LSEFAULT` bit 7. Running detection requires LSI and compares 128 LSE cycles in 256 LSI cycles. These status flags are not enables; ICR uses write-zero-to-clear semantics and does not justify clearing unrelated flags.

Consumers whose inherited configuration matters:

- RTC `CR1.SOURCE=0`, with the full reset admission rule below
- RTC `COMPEN.EN=1`, conservatively rejected even with another SOURCE: table12-3 (x030 PDF180/printed179; F020 PDF177/printed176) permits LSE-compensated1Hz output independently of SOURCE
- Standalone AWT `CR.SRC=3` and local `CR.EN`; the shared x030/F020 AWT native enum has independently sourced LSE=3 in each manual
- UART1, UART2 and UART3 `CR2.SOURCE=2`
- SYSCTRL MCO `SOURCE=6`
- Direct digital LSE outputs **PB12 AF3** and **PF1 AF1**

Configuration bus gates do not establish that RTC, AWT or UART working clocks are unused. Admission conservatively rejects a selected direct LSE output AF even while its pad driver is disabled. Parked AWT and UART LSE selectors likewise block a new start. GPIO **DIR is offset 0**, while **SPEED is offset 8**. The analog selector is offset 28. The route list is exact and unique, not an arbitrary caller-selected subset.

## RTC admission

RTC `START=0` is insufficient. When `CR2.AWTEN=1`, wake sources 0–3 use RTCCLK divided by 2, 4, 8 or 16 independently of START; sources 4–7 use RTC1Hz and require START. Existing wake, alarms, timestamp capture, output selection, compensation, calendar contents or pending status must therefore be preserved.

New startup deliberately requires a pristine RTC snapshot for **every SOURCE value**. Read **CR0, CR1, CR2, IER and ISR first**, rejecting mismatches before reading COMPEN or calendar registers. This conservatively covers compensated output without opening WINDOW/ACCESS on a running retained calendar. If those controls differ from pristine reset, stop admission before reading calendar state. Otherwise inspect the complete thirteen-register readable reset roster below. Do not write KEY, open ACCESS, clear flags, reset RTC, or modify an existing RTC while attempting to prove it is unused.

| Register | Offset | Reset | Defined-bit mask |
|---|---:|---:|---:|
| CR0 | 0x04 | 0 | 0xEF |
| CR1 | 0x08 | 0 | 0x703 |
| CR2 | 0x0C | 0 | 0x6FF |
| COMPEN | 0x10 | 0 | 0xFFFF |
| DATE | 0x14 | 0 | 0x07FFFFFF |
| TIME | 0x18 | 0x00120000 | 0x003F7F7F |
| ALARMA | 0x1C | 0x00120000 | 0x7FBFFFFF |
| ALARMB | 0x20 | 0x00120000 | 0x7FBFFFFF |
| TAMPDATE | 0x24 | 0 | 0xFF3F |
| TAMPTIME | 0x28 | 0 | 0x003F7F7F |
| AWTARR | 0x2C | 0xFFFF | 0xFFFF |
| IER | 0x30 | 0 | 0x5F |
| ISR | 0x34 | 0 | 0x5F |

KEY is write-only and ICR is an R1W0 command register, so neither belongs in the observation roster. Reset-default SOURCE0 alone does not imply an active RTC; a complete pristine snapshot establishes this limited admission case. Every other snapshot is conservatively rejected rather than reset or rewritten.

## Data validation

`electrical::apply_lse` first rejects duplicate mappings throughout its YAML catalog and then rejects missing required exact parts, additional family or exact-part qualifications, preseeded family configuration, wrong package/pads, source-policy drift, wrong canonical source identity, incorrect own-family evidence, and invalid source page references. It compares the entire configuration with the new source review, checks exact reset roster and route ordering/uniqueness, binds native source enums and the MCO source fact, and verifies selected register positions and defined-bit masks.

The focused `electrical::lse_tests` cases exercise malformed qualification, incomplete/duplicated reset sets, wrong masks, duplicated direct routes, changed AWT/MCO encodings, incorrect DIR offset, and preseeded family qualification. Ordinary ARM compilation and linked examples provide software validation only. No firmware execution or silicon result is claimed.

## Initialization and ownership

`Config.lse=None` adds no LSE enable/parameter/pad writes. Existing mandatory CLKCCS/HSECCS/LSECCS setup remains. Enabled-source reuse requires exact parameters, pads, EN/STABLE and no LSEFAIL/LSEFAULT; it never stops or rewrites the oscillator. New startup reserves requested pads before writes and rejects retained consumers before touching pads. Inspection enables configuration gates with readback and restores their incoming state on success. A gate failure may leave a gate enabled, but aborts before source/pad mutation. No shared reset or fault-clear write is used.

`LseClock` retains SYSCTRL and the actual oscillator pins, and verifies the frozen init record. RTC operations check the selected source before/after bounded waits and transactions. Bounds are a declared healthy-source envelope. The manual documents SYSCLK fallback to HSI, not automatic RTC fallback to LSI; calendar continuity or elapsed time after a fault is unqualified.

## L031 / R031 / W031 monitored-source qualification

Each family has an independent [L031](lse-active-l031.json), [R031](lse-active-r031.json), or [W031](lse-active-w031.json) record, its own RTC admission record, and its own original/page/SDK correspondence receipt. No equal-layout inference extends another family's citation authority. L031 ambient/supply is −40…85 °C / 1.65…5.5 V; R031 is −40…85 °C / 2.2…3.6 V; W031 uses the conservative LDO/DCDC intersection −40…85 °C / 2.0…3.6 V. RF operation is unqualified. Both modes retain the common 1 MHz ceiling and nominal 32,768 Hz contract.

These native GPIO blocks have no SPEED, LOCK, HIGHIE or LOWIE registers. The data generator checks the complete own-source native register roster, and the build generator emits only present typed PAC accesses. Every available pad control, peripheral reset/gate, and bonded output route remains checked. QFN20 L031F8U6's empty direct-output list is independently proven package absence, not missing metadata.

The CR1 CCS bits are configurable hardware. Requiring monitoring is the software policy for this held-source capability: a fresh requested start enables only LSECCS; CLKCCS/HSECCS remain exactly inherited. None leaves all three unchanged. Exact enabled reuse requires LSECCS, software-enabled stable LSI and exact LSE/pad configuration, and never changes CCS or restarts LSE.

LSI reset trim is unspecified. Before requested LSE initialization, the immutable factory halfword is read and masked to the native ten-bit trim. Factory-matching trim and WAIT are preserved for existing users. An incompatible live source is rejected. Only a source repeatedly observed disabled/nonstable, with no documented consumer/detector or pending/enabled ready event, may receive a trim-only factory load. Checked, gate-restoring inspections cover SYSCLK, MCO, pristine RTC including independent LSE compensation, AWT, UART1/2/3, GPIOA/B/C/F filter selectors and exact-package bonded PB11 AF1. Reserved selectors reject admission. IWDT uses RC10K and ADC uses PCLK; neither adds a direct LSI selector. Completeness is a conservative software inference from the complete own-manual clock and consumer review, not a silicon STOP guarantee.

No live source is stopped or trimmed, and no pending event is cleared. LSI startup has bounded polling and preserves WAIT, gates and unrelated CR1 controls. A successful LSE start freezes the detector's LSI trim/wait; subsequent held-source health checks require unchanged parameters, LSIEN/STABLE, LSECCS, LSEEN/STABLE and no LSE faults. Failure never publishes Clocks or attempts rollback. No automatic RTC fallback, clock-loss recovery, low-power service or frequency/accuracy measurement is implied.

The authored schema intentionally adds required configurable_ccs and changes gpio_speed_offset to Option<u32>. Existing profiles explicitly carry false and numeric 8; these three families carry true and null. Older active-LSE JSON without the required field needs regeneration. This is an explicit source/wire contract update.

## CW32L052 native startup and consumers

The three exact L052 parts use their own [source qualification](qualified-l052-lse.md), [native facts](lse-active-l052.json), [RTC reset record](lse-active-l052-rtc-admission.json), and [original receipts](lse-l052-source-receipt.json). Nominal frequency remains 32768 Hz and the board must bound each cycle. General source conditions are 1.65–5.5 V and −40–85 °C; the board must separately qualify both analog banks and the electrical interface.

`Lse.startup_drive` and `startup_amplitude` are required on this native hardware in addition to running `drive` and `amplitude`. All four typed PAC fields are written while disabled and checked on reuse and health checks. No post-enable write or claim about the precise bank-switch instant is made. LSE reset is 0x0A2B and RTC ALARMA reset is 0x04120000; these are own L052 facts.

The optional `startup_consumers` record represents concrete native startup fields and AUTOTRIM/LPTIM/LCD hardware, not power-mode support. Existing `awt_source` values become `Some(3)` internally while their JSON remains the number 3. L052 has no standalone AWT, so it explicitly uses null and must provide the complete native fact record. Absent/null `startup_consumers` stays omitted on prior profiles. Exact UART names must equal the selected complete UART roster. L052 keeps the vendor PAC spelling `SORCE` / `sorce()` for the manual’s UART `SOURCE` field; no register rename or adapter is introduced. PC4/AF6 is the only LSI output and is bonded only on the two 64-pin packages. The existing raw AF catalog leaves LSIOUT in its unmerged special-function set, so qualification compares its complete bonded roster with the explicit route list without changing the general AF generator.

AUTOTRIM can consume or mutate LSI independently of its reference selector. Every requested LSE path, including reuse, rejects automatic calibration, enabled calibration, and reserved mode/source encodings before freezing the monitor. A mismatched stopped LSI also requires the full source/observer/consumer admission twice; only factory trim is written and WAIT is preserved. Matching live LSI is retained without retuning.

LPTIM and LCD RCC gates stop work as well as configuration. An off gate stays off and is not temporarily enabled for inspection. An already-on gate is read without writes and must remain unchanged; native EN and source determine whether the peripheral consumes the source. RTC, UART and AUTOTRIM have configuration-only gates, so their retained state is inspected even when their gates start off. RTC compensation remains an LSE consumer independently of RTC SOURCE, and a new start requires all thirteen own reset observations with controls first. No KEY, ACCESS, reset or flag-clear write manufactures admission.

`None` adds no inspection, source, analog or pad writes. Failures retain enables, reservations, detector LSI and diagnostic flags and publish no healthy capability. LSE parameters and enable are retained across ordinary reset; only POR clears the documented controls. Software cannot promise that a CPU reset makes retry possible. No automatic RTC failover or elapsed-time continuity is established.
