# Bounded active LSE qualification

This candidate qualifies active LSE configuration only for **CW32F030C8T7** and **CW32A030C8T7**, both **LQFP48**. Their oscillator pads are **PC14 / OSC32_IN at pin 3** and **PC15 / OSC32_OUT at pin 4**. Family aliases and every other part retain only previously established pad-ownership facts. Qualification is projected after exact-package expansion through `ClockLimits.lse_configuration`, not placed in family electrical profiles.

The source facts were checked on 2026-10-09 against the exact originals identified below. This is source qualification, not silicon testing.

## Own-source evidence

The canonical authority is `sources/evidence-sources.json`. New citations use canonical `vendor:` IDs in `source_ref`; local labels are in `citation_ref`.

| Original | SHA-256 | Relevant PDF pages (one based) |
|---|---|---|
| CW32x030 User Manual CN Rev2.5 | `1afd49261f0f0689af8cb8ebf1b0ac1c00e3209b20d3c722106707ff4a10bdd2` | 51–52 modes; 59–60 detectors; 64 sequence; 71 CR1; 76 LSE; 79–80 status; 94 MCO; 152–154 GPIO; 173 standalone AWT; 184 RTC wake; 187–195 RTC reset/controls; 357 UART |
| CW32A030 Data Sheet CN Rev1.1 | `690433f36376352ee342ae3e2aa76786f2e7e59892a2519411f2754ac32c33a9` | 22–23 bonded pads; 27 direct output AF; 35 conditions; 41 bypass; 42 crystal |
| CW32F030 Data Sheet CN Rev1.9 | `04ef91434320e5d05a3b6690fead0fedb31a7d9bad28c96b655a6e13a22e46b2` | 24/26 bonded pads; 30 direct output AF; 38 conditions; 44 bypass; 45 crystal |

PDF page number is the printed page plus one for these originals. The common manual explicitly covers both F030 and A030. F030 SDK V2.2 headers independently corroborate field locations and encodings; SDK absence is never evidence of an absent consumer or automatic request.

The machine-readable review is in [lse-active-first-cohort.json](lse-active-first-cohort.json) and [lse-active-rtc-admission.json](lse-active-rtc-admission.json). Their exact digests are locked by [lse-qualified.yaml](../cw32-data/lse-qualified.yaml).

## Board contract

Both crystal and bypass use nominal **32,768 Hz** in this cohort. The board must supply a positive minimum and maximum frequency that bracket the nominal value and cover **every individual cycle**, including tolerance, temperature, supply, aging and jitter. A nominal frequency does not establish an accuracy bound. The own-source ceiling for both crystal/ceramic and bypass is **1 MHz**; this does not expand the cohort to other nominal frequencies.

The qualified operating conditions are **1,650–5,500 mV** and **−40 to +105 °C**, with VDDA equal to VDD and the device's remaining electrical conditions satisfied. The datasheets' conditional low-power extension to +125 °C is deliberately not projected.

Crystal mode requires PC14 and PC15 configured as analog. Crystal manufacturer characteristics, load capacitance, layout parasitics, drive and amplitude remain board responsibilities. The datasheets give **1.5 s typical** startup, with no maximum; it must not become a guaranteed timeout.

Bypass requires PC14 as a digital input. PC15 can be general-purpose GPIO under the documented mode, subject to ownership retained by the HAL. The input must satisfy **45–55% duty cycle**, **at least 450 ns high and low**, **at most 50 ns rise and fall**, high level **70–100% of VDDIO**, and low level **0–30% of VDDIO**. The waveform and voltage obligations cannot be established by software frequency bounds alone.

## Native oscillator and consumer facts

`LseDrive` and `LseAmplitude` expose all four documented encodings 0–3. `LseWait` exposes 256, 1,024, 4,096 and 16,384 LSE cycles at encodings 0–3. These enums are authored in `sysctrl_v1.yaml`; metadata stores the cycle counts, not an alternate register-access abstraction.

`CR1.LSEEN` is the software enable; `LSE.STABLE` is a separate read-only observation. The configuration must be completed before enable and remain unchanged while enabled. `LSELOCK` only prohibits clearing LSEEN; it is not a lock on the other oscillator parameters. Only power-on reset resets the documented LSE register and LSEEN state.

CR1 writes require **KEY=0x5A5A** in bits 31:16. The own CR1 table explicitly requires **CLKCCS, HSECCS and LSECCS to be written as 1**. This cohort must retain that requirement. Startup failure is `ISR.LSEFAIL` bit 5; running failure is `ISR.LSEFAULT` bit 7. Running detection requires LSI and compares 128 LSE cycles in 256 LSI cycles. These status flags are not enables; ICR uses write-zero-to-clear semantics and does not justify clearing unrelated flags.

Consumers whose inherited configuration matters:

- RTC `CR1.SOURCE=0`, with the full reset admission rule below
- RTC `COMPEN.EN=1`, conservatively rejected even with another SOURCE: table12-3 (PDF180/printed179) permits LSE-compensated1Hz output independently of SOURCE
- Standalone AWT `CR.SRC=3` and local `CR.EN`; the x030 AWT native enum has its own source-backed LSE variant
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
