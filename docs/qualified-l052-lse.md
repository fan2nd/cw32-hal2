# CW32L052 LSE / RTC source qualification

Source-only review, 2026-10-09. No repository edits, implementation, HAL tests, Cargo, upload, remote write, or silicon execution. Original files remain intact. PDF page numbers below are one based; for these manuals/datasheet, printed page = PDF page - 1.

## Own official authorities

All four originals were rehashed against `sources/evidence-sources.json`; exact verified URL/length/hash receipt is `lse-l052-source-receipt.json`.

- `vendor:CW32L052_UserManual_CN_V1.5.pdf`, Rev1.5, SHA-256 `4bac53df4db69a3b76c833cb14dd45b0e5c7f9884506c83a5cda6ea00a859f41`.
- `vendor:CW32L052_DataSheet_CN_V1.3.pdf`, Rev1.3, SHA-256 `f03e2c3545b52942b576f669e3de69e1962631834ef6a01e51b45996fbeb3e7f`.
- `vendor:CW32L052_UserManual_EN_V1.0.pdf`, Rev1.0, SHA-256 `20a7b6fcbdb1b6075c45ddc904a2acf75d92e1491e3186e1d604970e600bc088`; corroboration, not replacement for newer Chinese authority.
- `vendor:CW32L052_StandardPeripheralLib_V1.4.zip`, V1.4, SHA-256 `d05a1ff5749ad8c06a51ad8a34d53bc85e30b1350478bcabc2858c2e58d8fa1f`; exact archive member hashes in `lse-l052-sdk-member-receipt.json`. Header confirms native fields; SDK code is not authority for safe initialization.

## Exact package and electrical bounds

DS Table3-1 PDF10/printed9 and order table PDF79/printed78 identify CW32L052C8T6 = LQFP48, CW32L052R8S6 = LQFP64 7x7 mm, CW32L052R8T6 = LQFP64 10x10 mm. Admit only those three exact parts; not the family alias. Table5-2 PDF26/printed25 places PC14/OSC32_IN at pin3 and PC15/OSC32_OUT at pin4 for both 48- and 64-pin packages. Native rendered table visually verified.

Direct digital LSE routes are PB12/AF3 (physical pin25 on LQFP48, pin33 on LQFP64) and PF1/AF1 (physical pin6 on both) on all three packages (DS PDF32/printed31 and PDF34/printed33; UM Table9-2 PDF146-147/printed145-146). PF1 is also HSE OSC_OUT; selected direct LSE AF must be checked as inherited ownership, not assumed free because its output driver is disabled. The direct LSI route is PC4/AF6 at physical pin24, bonded only on R8S6/R8T6 (DS PDF27/printed26, PDF33/printed32); it is absent on C8T6. L031's PB11/AF1 rule does not apply.

UM §4.3.5 PDF55-56/printed54-55: MODE=0 crystal requires PC14 and PC15 analog; MODE=1 bypass requires PC14 digital input and permits PC15 general GPIO. UM states crystal and bypass upper ceiling 1 MHz; DS Table7-16 PDF50/printed49 characterizes a 32.768 kHz crystal only. Proposed initial admission: nominal 32768 Hz in either mode, positive board-provided per-cycle minimum/maximum bracketing nominal, no arbitrary-frequency crystal claim. Crystal load, parasitics, drive and amplitude remain board-qualified. DS gives 1.5 seconds typical startup and no maximum, design-guaranteed rather than production-tested; never convert to guaranteed software timeout.

DS Table7-4 PDF43/printed42: 1.65-5.5 V, VDDA=VDD, -40 to +85 C at ordinary maximum power for temperature grade6; do not project conditional low-power +105 C extension. Bus limit is 24 MHz below1.8 V and48 MHz at/above1.8 V. LSE input Table7-14 PDF49/printed48: maximum1 MHz, typical32.768 kHz, no specified positive minimum; high0.7-1.0 VDDIOx, lowVSS-0.3 VDDIOx, minimum high and low450 ns, maximum rise and fall50 ns. UM adds45-55% duty. Native PDF table columns visually verified.

## Native LSE settings: a real two-phase analog distinction

UM §4.7.7 PDF76-77/printed75-76; LSE+0x24 reset0x00000A2B:

| Field | Bits | Own meanings |
|---|---:|---|
| DRIVER |1:0| Post-stable drive:0 least,1 smaller,2 normal,3 stronger;3 recommended |
| AMP |3:2| Post-stable amplitude:0 least,1 smaller,2 normal,3 larger;2 recommended |
| WAITCYCLE |5:4|0=256,1=1024,2=4096,3=16384 LSE cycles |
| MODE |6|0 crystal,1 bypass |
| PDRIVER |9:8| Pre-stable drive:0 least,1 smaller,2 normal,3 stronger |
| PAMP |11:10| Pre-stable amplitude:0 least,1 smaller,2 normal,3 larger |
| STABLE |15| Read-only live oscillator status |

Bits7,14:12,31:16 reserved, preserve defaults. No reserved encodings inside any of these two-bit drive/amplitude/wait fields. Reset decodes PAMP2, PDRIVER2, AMP2, DRIVER3, WAIT2, MODE0. Native PDF76 inspected visually; current native IR already contains PDRIVER/PAMP but not typed LSE enums.

UM PDF56/printed55 explicitly describes different parameter banks before and after stability, then explicitly requires all oscillator parameters before enable and forbids changes after start. EN PDF56/printed55 repeats both statements. No reviewed text explicitly says firmware must rewrite the fields at STABLE, nor names a software phase-switch command. The natural hardware-selection interpretation is supported by the separate banks but the precise switching instant being the observable STABLE bit is an inference, not an explicit silicon timing guarantee. Safe bounded software policy: program all four independent analog settings while stopped; never perform a post-enable analog rewrite. Do not silently map one setting onto both phases. A public startup-drive/startup-amplitude pair or an explicit own-family preset is needed in addition to the existing run pair. No source identifies a universal board-independent pair.

## CCS, LSI prerequisites and failure ownership

CR1+0x04 reset1, key0x5A5A in31:16, LSIEN3/LSEEN4/LSELOCK5/LSECCS6/HSECCS7/CLKCCS8: UM PDF71/printed70. All CCS bits are configurable here; the x030 mandatory-one rule does not apply. Preserve HSECCS and CLKCCS while enabling LSECCS under the active-LSE policy. CLKCCS=1 switches an affected currently selected external SYSCLK to HSI8MHz. It is not an RTC source fallback. L052 has no PLL source.

UM PDF61/printed60: startup detector needs LSECCS; WAIT encodings0..3 count256/1024/4096/16384 within1/1/1/2 seconds. PDF62/printed61: running detector needs LSECCS AND software LSIEN=1, and sees128 LSE clocks within256 LSI cycles. Do not assume enabled CCS requests LSI automatically. Leave monitored LSI enabled and freeze its trim/wait settings; no calibration on a live source. Stable state establishes neither waveform electrical validity nor calibrated frequency nor continuous RTC availability. Datasheet1.5s typical versus1s detector windows supplies no startup guarantee; a bounded poll can fail even for a normally valid crystal.

LSI+0x20: TRIM9:0, WAIT11:10 encodings0=6/1=18/2=66/3=258 cycles, STABLE15; UM PDF57/printed56 andPDF74/printed73. Parameters must be set before start. Own factory halfword0x00100A02-0x00100A03; UM PDF59/printed58 says32.8kHz nominal and warns against tuning outside +/-10%. DS PDF51/printed50 gives factory-calibrated +/-3% at -40..85 C and +/-1% at25 C. SDK archive `Libraries/src/system_cw32l052.c` explicitly loads that halfword, but an Embassy startup is not evidence SystemInit ran. Reject erased0xffff; use10-bit field width; preserve WAIT/reserved bits. Before changing mismatched trim, require repeated unchanged LSIEN0, LSI.STABLE0, ISR.LSISTABLE0, no LSI-ready enabled/pending observer, no HSE/LSE detector and no documented LSI consumer. A factory-matched live LSI may be preserved, never stopped just to obtain a preferred configuration.

UM PDF78-80/printed77-79: ISR+0x10 has live LSESTABLE15 and LSISTABLE14, sticky transition LSERDY4/LSIRDY3, startup LSEFAIL5 and running LSEFAULT7. IER+0x0C keyed enables match; ICR+0x14 is R1W0 (0 clears,1 no effect). HSE/LSE runtime loss uses FAULT vector47; start failure/readiness uses RCC vector20. Preserve unrelated flags and observers. A cold start should reject retained fault or LSE-ready/failure interrupt ownership. Software timeout is not a hardware stop command. On error retain source enable, pad reservation and necessary detector LSI; publish no usable LSE/RTC capability and do not clear another owner's flags. LSE register, LSEEN and LSELOCK are power-on-reset-only reset state (PDF71/printed70 andPDF77/printed76); do not imply an ordinary CPU/system reset clears them. LSELOCK only prevents clearing LSEEN, not parameter writes.

## Complete documented retained consumer graph

| Consumer | Native source/ownership facts | Own UM PDF / printed |
|---|---|---|
| SYSCLK | CR0.SYSCLK4=LSE,3=LSI; do not reconfigure selected source |70 /69 |
| RTC | CR1.SOURCE0=LSE,2=LSI,4..7=HSE/128../1024; SOURCE1/3 reserved |193 /192 |
| RTC compensation | Table12-3 explicitly shows COMPEN.EN1 and RTC1HZ01 using compensated LSE with SOURCE='-'; independent of calendar SOURCE |185 /184,195 /194 |
| RTC wake/calendar/output/alarm/timestamp | START0 is insufficient. AWTEN plusAWTSRC0..3 uses RTCCLK/2../16 without START;4..7 usesRTC1Hz and needsSTART1. Preserve retained configuration/calendar/status |184-189 /183-188,192-200 /191-199 |
| AUTOTRIM (no standalone AWT peripheral) | CR+0 SRC10:8:0HSIOSC,1LSI,2HSE,3LSE,4ETR,5..7reserved. MD2:1:0HSI-calibration,1LSI-calibration,3wake timer,2reserved. MD1 consumes LSI independently of SRC (which is high-frequency reference). AUTO3 may autonomously alter trim; EN0/1 controls execution. Timer LSE and HSI-calibration referenceLSE are both owners |167-170 /166-169,175 /174 |
| UART1,2,3 | CR2+4 SOURCE9:8:0/1PCLK,2LSE,3LSI. UCLK also drives UART timeout/autobaud/general timer; TXEN/RXEN0 is insufficient idle proof |359 /358,390 /389 |
| LPTIM | CFGR+0x0C ICLKSRC26:25:0/1PCLK,2LSE,3LSI. Internal clock also feeds external-input filtering, trigger filtering and encoder; external count source is not proof internal clock unused. CR+0x10.EN0 module disable; no need/permission to write command/reset bits to inspect |224-225 /223-224,235-237 /234-236 |
| LCD | CR1+4.CLKCS7:0LSI,1LSE; CR0.EN0 enable. Scan and charge-pump behavior are retained peripheral ownership |536 /535,553-554 /552-553 |
| MCO | +0x70 SOURCE3:0:4LSI,6LSE;0disabled,1HCLK,2PCLK,3HSIOSC,5HSE,8RC150K,9RC10K;7,10..15reserved |94 /93 |
| Direct outputs | LSE PB12AF3 andPF1AF1; LSI PC4AF6 only64-pin packages. Treat selected AF conservatively even if driver disabled |146-147 /145-146 plus own DS pin tables |
| GPIO filter | FILTER+0x40 FLTCLK18:16=5LSI.7=LPTIM PWM is an indirect LSE/LSI path already requiring LPTIM inspection. Per-pin enables15:0 |157 /156 |
| Watchdogs | IWDT runs dedicated RC10K, notLSI/LSE; WWDT runsPCLK. Do not import another family's watchdog autostart ofLSI |340-341 /339-340,350 /349 |

Autonomous/configuration-only gates: RTC APBEN1.3, UART2/3 APBEN1.7/.8, UART1 APBEN2.9, AUTOTRIM APBEN2.13. Their configuration gates being off is not idle proof. APBEN1.15 LPTIM and.13 LCD explicitly gate BOTH configuration and work clocks, as do GPIO AHB gates (PDF81-83/printed80-82). This distinction matters: blindly enabling a disabled LPTIM/LCD gate to inspect can resume a retained PCLK/other-source owner. Minimal bounded policy can treat documented work-gate-off as inactive while preserving it off, and inspect locals/selectors only when gate already on; if stricter parked-selector rejection is required, a separate proven inspection policy is needed. Do not assert bus-gate-off idle for RTC/UART/AUTOTRIM.

For LCD's reset default CLKCS0=LSI, rejecting the source selector unconditionally would reject cold reset. Require localEN0 (or documented work gate off), rather than pretending source0 is not LSI. For AUTOTRIM, inspect mode and automatic trim in addition to SRC; source-only checks miss implicit calibration consumers. Conservative cold-LSI admission can reject any enabled or armed automatic calibration, both reserved mode/source encodings, all direct LSI selectors, and existing active timer paths without changing them.

## RTC reset admission and usable source

Propose the same conservative strategy as accepted cohorts: require pristine readable reset state for every SOURCE before a new LSE start; check CR0/CR1/CR2/IER/ISR first, then COMPEN/calendar/alarms/timestamp/AWTARR, twice. Never write KEY/ACCESS, clear flags or reset retained RTC merely to inspect it. The own thirteen-register roster is `lse-active-l052-rtc-admission.json`, UM PDF192-200/printed191-199.

The important new value is ALARMA reset0x04120000, not0x00120000; native PDF196/printed195 visually checked, repeated by EN PDF210/printed209. ALARMB remains0x00120000. Defined masks and other values match the accepted roster: CR0EF, CR1703, CR26FF, COMPENFFFF, DATE07FFFFFF, TIME003F7F7F, ALARM7FBFFFFF, TAMPDATEFF3F, TAMPTIME003F7F7F, AWTARRFFFF, IER/ISR5F. Reset TIME/ALARMB00120000, AWTARRFFFF; others0. KEY andICR remain excluded.

RTC source0 uses fixed32768 division. A lifetime-held LSE capability should consume SYSCTRL, use frozen board bounds, verify mode/all four analog settings/wait, pads, LSEEN/live stable/no faults, LSECCS and unchanged ready factory LSI. Reuse an already enabled LSE only on full match without touching it or RTC. Drop must not stop shared oscillators. No claim of failover, elapsed-time continuity, calendar accuracy beyond declared bounds, or source health measurement follows from STABLE.

## Native GPIO and metadata fit

UM PDF151-157/printed150-156: DIR+0 (1input), OPENDRAIN+4, no SPEED at+8, PDR+C/PUR+10, AFRH+14/AFRL+18 four-bit fields, ANALOG+1C (1analog), RISEIE+24/FALLIE+28/HIGHIE+2C/LOWIE+30, ISR+34/ICR+38, LCKR+3C keyed0x5A5A, FILTER+40. L052 curated IR calls the real LCKR `LOCK`; source names and native symbol spelling must remain distinguished. GPIO port gates unkeyed. Cold pad admission must see every edge/level interrupt, full AF, pulls and lock. Preserve unrelated pins/locks/filter/pending flags; don't write fictitious SPEED.

Existing accepted `LseConfiguration` is insufficient as a complete qualification for this family. Existing exact-part projection, source refs, bounds, configurable_ccs, optional SPEED, startup-cycle vector, RTC-reset rows and LSE-output routes can represent many facts. Minimal additions/changes needed:

1. Explicit native optional pre-stable drive/amplitude feature plus separate configuration values on L052; exact-match reuse includes them. Typed own LSE enums must bind each native field.
2. Native consumer-kind qualification for AUTOTRIM rather than required AWT; mode/auto ownership; LPTIM/LCD local-enable and gate semantics. Keep direct typed PAC projections rather than inventing register-offset access abstractions.
3. Package-specific direct LSI output route facts, replacing hard-coded PB11. UART count should come from own qualified roster (threeL052; sixL083), not shared assert3.
4. Native GPIO lock accessor and full IE roster; speed=None alone does not establish absent lock or absent level interrupts. This also matters to L083's LCKR spelling.
5. Own RTC ALARMA reset value and source evidence. The current roster representation can already store it; validators must stop hard-coding the old cohort value.

Do not extend to arbitrary nominal frequency, live analog retuning, existing RTC reconfiguration, direct LSE SYSCLK selection, autonomous calibration, power-mode API, or inactive-device register mutation as a side effect.

## Actual uncertainties and rejected shortcuts

- The text does not explicitly specify the analog bank switch instant relative to STABLE. Programming all fields before enable avoids dependence on a live-write interpretation; no need to claim a precise automatic transition time.
- The own CN andEN manuals agree on ALARMA's0x04120000 reset. It is a native difference, not an unresolved conflict.
- SDK RCC_LSE_Enable uses a whole-register write containing only Mode/Wait/Amp/Driver, therefore clears both pre-stable fields. It also enables LSECCS/CLKCCS without establishing LSI, and its bypass `PUR &= bit14` can clear unrelated pull-ups. These are unsafe shortcuts, not grounds to override manual requirements. Library API accepts only the run amplitude/drive.
- A generated helper currently hard-codes standaloneAWT and threeUARTs and the older cold-LSI direct-output route. Extending feature flags alone would miss consumers.
- The gate policy for LCD/LPTIM must be implemented explicitly before admission; gate-restoring temporary enable does not erase work that occurred while enabled.
- No silicon measurements were obtained. Bounded nominal32768 crystal/bypass qualification is supportable with the additions above; unmodified existing metadata/sequence is not.
