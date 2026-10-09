# RTC next-batch audit

2026-10-08. Thirteen family profiles reviewed against their own pinned manuals,
SDK headers/drivers and canonical PAC. Eleven have RTC; F002/F003 do not.
The initial read-only review was followed by an explicitly scoped PAC correction
batch. No RTC HAL, device execution, board testing, push or merge was performed.

The machine-readable [audit](rtc-next-batch-audit.json) contains all family
facts, 67 rehashed source artifacts, exact official URLs, PDF-page/section maps,
and clock/IRQ details. [Correction provenance](rtc-pac-corrections.json) records
all before/after canonical hashes and the unchanged non-RTC baseline.

**Page convention:** page numbers below are physical PDF pages, starting at one.
Most manuals' printed page is one lower. L012 manual printed page is 26 lower.
This matters: x030 COMPEN is physical page 190, printed page 189.

## Findings and correction boundary

The completed correction batch changes eight input profiles and five authored
RTC templates, with no unrelated normalized IR changes:

- F020, L031, L052, L083, R031 and W031: remove obsolete COMPEN.FREQ[19:16].
  Every own current manual reserves COMPEN[31:16]. The existing x030 correction
  was already right. Three canonical templates are involved because the
  L031/L083/R031/W031 RTC maps have exact shared IR.
- F020: TAMPDATE and TAMPTIME become read-only; their own tables mark every
  implemented field RO. Offsets and fields stay unchanged.
- L010 and L011: DATE.DAY is six bits [5:0], MONTH is five bits [12:8]. The
  copied eight-bit SDK/SVD fields exposed reserved bits [7:6] and [15:13].
  L012 already had the correct widths.
- No alarm polarity was changed. **ALARMB is an unresolved source contradiction
  on every RTC-bearing family**, not just F020. ALARMA tables say mask zero
  compares and one ignores; ALARMB tables reverse that. The common A/B examples
  and SDK masks use zero=compare and one=ignore. A HAL must not guess the B
  polarity from its register name or silently copy A.

Own rendered PDF tables were inspected for each distinct RTC manual, plus the
F002/F003 address tables. The exact mutation proof reconstructs the previous IR
by undoing only the authorized changes. Every original SVD span/access condition
is checked before accepting the corresponding override. The F020 alarm field
names SECONDMASK/MINUTEMASK/HOURMASK/WEEK are SDK aliases of the documented bit
positions, not additional offset errors. Mixed CR1 status/control fields remain
in a RW register: the backend must never attempt to write WINDOW or WAIT as
controls; current IR does not encode separate per-field access.

## Family map

All implemented RTCs use IRQ2, named RTC; all six event sources share it. The
configuration bus is PCLK, while the functional source is selected inside RTC.

| Family | Canonical RTC version | Base | Protocol | Own manual | Access / CR1 / DATE / ALARMB / ICR pages |
|---|---|---|---|---|---|
| A030 | v1 | 0x40002800 | classic WINDOW→ACCESS | shared x030 V2.5 | 179 / 188 / 190 / 192 / 196 |
| F002 | none | none | absent | F002 V1.4 §2.2.2, table 2-1 p23 | no RTC in manual address map, SDK or PAC |
| F003 | none | none | absent | F003 V2.3 §2.2.2, table 2-1 p25 | no RTC in manual address map, SDK or PAC |
| F020 | cw32f020_v1 | 0x40002800 | classic WINDOW→ACCESS | F020 V1.4 | 176 / 185 / 187 / 189 / 193 |
| F030 | v1 | 0x40002800 | classic WINDOW→ACCESS | shared x030 V2.5 | 179 / 188 / 190 / 192 / 196 |
| L010 | cw32l010_v1 | 0x40004400 | WAIT→ACCESS→WINDOW | L010 V1.2 | 143 / 152 / 154 / 157 / 160 |
| L011 | cw32l011_v1 | 0x40004400 | WAIT only | L011 V1.1, June 2026 bytes | 143 / 152 / 154 / 157 / 160 |
| L012 | cw32l012_v1 | 0x40004400 | WAIT only | L012 V1.4 | 194 / 202 / 204 / 207 / 210 |
| L031 | cw32l031_v1 | 0x40002800 | classic WINDOW→ACCESS | L031 V1.6 | 171 / 180 / 182 / 184 / 188 |
| L052 | cw32l052_v1 | 0x40002800 | classic WINDOW→ACCESS | L052 V1.5 | 184 / 193 / 195 / 197 / 201 |
| L083 | cw32l031_v1 | 0x40002800 | classic WINDOW→ACCESS | L083 V2.0 | 196 / 205 / 207 / 209 / 213 |
| R031 | cw32l031_v1 | 0x40002800 | classic WINDOW→ACCESS | R031 V1.3 | 173 / 182 / 184 / 186 / 190 |
| W031 | cw32l031_v1 | 0x40002800 | classic WINDOW→ACCESS | W031 V1.4 | 172 / 181 / 183 / 185 / 189 |

Classic families use chapter 12: access §12.3.5, CR1 §12.5.3, DATE §12.5.6,
ALARMB §12.5.9, ICR §12.5.15. L010/L011 use chapter 10: access §10.3.6,
CR1 §10.5.3, DATE §10.5.6, ALARMB §10.5.10, ICR §10.5.15. L012 uses the
same latter subsection numbering in chapter 13.

Common register offsets: KEY 0x00, CR0 0x04, CR1 0x08, CR2 0x0C,
COMPEN/COMPCFR1 0x10, DATE 0x14, TIME 0x18, ALARMA/B 0x1C/0x20,
TAMPDATE/TAMPTIME 0x24/0x28, AWTARR 0x2C, IER/ISR/ICR 0x30/0x34/0x38.
L010/L011/L012 additionally have AWTCNT 0x3C, PSC 0x40 and SSCNT 0x44.
No RTC backup-storage array is present. Do not reinterpret timestamp registers
as scratch storage.

## Calendar representation and limits

All eleven devices store a BCD calendar, not an epoch-second counter:

- YEAR 00–99, MONTH 1–12, DAY 1–28/29/30/31; WEEK 0–6 with Sunday=0.
- MINUTE and SECOND 0–59. No leap-second value 60. H24=1 selects 00–23.
- H24=0 selects 1–12 with TIME.HOUR bit21 indicating PM: midnight is 0x12,
  noon is 0x32. Simply stripping bit21 gives the wrong noon/midnight mapping.
- Automatic leap-year adjustment is documented. No century register or
  application epoch is supplied. A first API should explicitly choose
  2000–2099, reject out-of-range years, validate full month/day/leap-day
  combinations, and document the end-of-century rollover. This year epoch is
  a software convention, not stored hardware metadata.
- DATE and TIME writes reject illegal values; rejection is not an adequate
  error-reporting API. Validate before writes and verify the settled readback.
  A reset DATE=0 is invalid and must not become a fabricated date.
- Classic DATE uses full eight-bit DAY/MONTH slots even though valid BCD values
  occupy fewer bits. The three low-family DATE maps reserve their extra bits.

L010/L011/L012 SSCNT0[19:0] represents the partial half-second and SSCNT1[20]
the half-second flag. Raw subsecond ticks = (PSC2+1)×SSCNT1+SSCNT0. TIME writes
clear SSCNT. Their access sections require two equal SSCNT reads and rejection
of SSCNT0=0; do not import STM32's descending SSR conversion formula. A joint
calendar/subsecond snapshot needs additional rollover consistency handling.

## Access, synchronization and coherent reads

KEY is write-only. Unlock with 0xCA then 0x53; relock with 0xCA then a value
other than 0x53, such as 0x00. KEY and ICR are exempt from write protection.
Reading does not generally require destroying an existing lock state or
stopping the counter. The synchronized transaction protocol differs:

1. Classic: wait for WINDOW=1, unlock, set ACCESS, perform a bounded transaction,
   clear ACCESS, relock. The manual suggests 10 ms spacing and at most 1000
   attempts while waiting. ACCESS must be released within one second. Several
   SDK functions instead set ACCESS before waiting WINDOW; that sequence must
   not be copied as evidence for the classic backend.
2. L010: for DATE/TIME/AWTARR while running, wait WAIT=0, unlock, set ACCESS,
   wait WINDOW=1, complete the transaction within one second, clear ACCESS,
   relock. This is deliberately different from classic. Other registers are
   directly accessible after unlocking. SYSCTRL.CR2.RTCLPM bit7 must be zero:
   §4.7.3 p70 says writes cannot synchronize when RTCLPM=1.
3. L011/L012: for DATE/TIME/AWTARR while running, wait WAIT=0, unlock, access,
   wait WAIT=0 again for loading to complete, then relock. Check readiness
   between timing-register writes. CR1[1:0] is reserved, so do not use an
   L010 ACCESS/WINDOW helper. Stopped-counter accesses need no running handshake.

All manuals allow a fast same-register read by retrying until two consecutive
values match. **That is not a complete multi-register calendar snapshot.**
Independently successful GetDate and GetTime calls can straddle midnight.
A HAL should use the family-specific synchronization transaction where
applicable, and bounded time/date/time or repeated complete-pair consistency
checks plus decoded-value validation. There is no common documented full-calendar
hardware latch to promise stronger atomicity. L011/L012 WAIT is load status,
not a calendar freeze bit. A board rollover test remains necessary.

Use bounded waits and errors for a stopped/missing functional clock, stuck
WINDOW/WAIT, invalid BCD and unstable reads. A scope guard must restore ACCESS
and locking on every exit. Never hold ACCESS across an await, callback, long
critical section or debugger-dependent delay. Do not stop/restart RTC to make
ordinary reads easy; that changes elapsed time.

## Clocks and accuracy prerequisites

Classic SOURCE[10:8]: 0=LSE, 2=LSI, 4/5/6/7=HSE divided by 128/256/512/1024.
The calendar path assumes 32768 Hz. No programmable calendar prescaler exists.
The manuals explicitly restrict HSE-derived operation to general timing/counting
because of accuracy limits. Removing COMPEN.FREQ does **not** invalidate these
HSE SOURCE encodings.

L010/L011/L012 SOURCE: 0=LSE, 1=HSE, 2=LSI, 3=HSIOSC. These are undivided HSE
and HSIOSC, not the RCC `hsi` output. PSC1[27:20] divides by PSC1+1 to at most
1 MHz; PSC2[19:0] divides again by PSC2+1. TICKCLK must be exactly 2 Hz for the
nominal calendar rate. Default PSC1=0, PSC2=16383 works for 32768 Hz LSE.
The full 8-bit/20-bit divisor ranges and checked arithmetic must constrain
configuration; an arbitrary rounded divider must not be presented as exact time.

The existing `embassy-cw32/src/rcc/mod.rs` explicitly supports only public HSI
paths. It does not publish an RTC kernel frequency or a stable long-lived
LSE/LSI capability. Required work before a normal RTC constructor:

- An owned, source-qualified clock configuration with readiness timeout,
  source frequency, oscillator enable lifetime and documented sleep behavior.
- Board-qualified LSE pin/crystal or bypass mode, drive strength, load and
  startup handling. Do not borrow STM32 backup-domain/LSE drive settings.
- Preserve already-running sources. The RCC low-family init path temporarily
  disables HSI for calibration; if a bootloader left RTC on HSIOSC, ticks can
  be lost before an RTC attachment method even runs. Attach preservation must
  include the earlier RCC transition contract, not just RTC registers.
- Distinguish configuration-bus gating from the functional oscillator. Classic
  APBEN1/APBRST1 RTC is bit3; low-family APBEN2/APBRST2 is bit1, with 0x5A5A
  upper-halfword key for gate writes. APB reset is active-low and must not be
  pulsed by the ordinary constructor.
- Read readiness; do not interpret the datasheet's typical startup value as a
  guaranteed delay, or a nominal RC frequency as a measured frequency.

Own datasheet LSI accuracy ratings below apply under their stated conditions.
All specify ±1% at 25°C; the broader temperature ranges differ substantially.
LSE startup is typically 1.5 s, without a specified maximum in these tables.
Actual LSE accuracy depends on the external resonator, load, layout and operating
conditions; stable-ready status does not measure drift.

| Family | LSI error over temperature | Temperature °C | LSE / LSI physical PDF pages |
|---|---|---|---|
| A030 | −3% … +3% | −40 … +105 | 42 / 43 |
| F020 | −5% … +5% | −40 … +105 | 43 / 44, December 2025 replacement |
| F030 | −3% … +3% | −40 … +105 | 45 / 46 |
| L010 | −3% … +3% | −40 … +85 | 42 / 43 |
| L011 | −10% … +25% | −40 … +85 | 50 / 51 |
| L012 | −10% … +10% | −40 … +85 | 57 / 58 |
| L031 | −3% … +3% | −40 … +85 | 46 / 47 |
| L052 | −3% … +3% | −40 … +85 | 50 / 51 |
| L083 | −3% … +3% | −40 … +85 | 54 / 55 |
| R031 | −3% … +3% | −40 … +85 | 53 / 54 |
| W031 | −3% … +3% | −40 … +85 | 52 / 53 |

The surprising L011 −10/+25% table was visually verified, not inferred from
L010 or normalized to a symmetric range. The JSON pins the exact datasheets.

## Reset, retention and power domains

Every own reset summary (§4.1) excludes RTC from NRST, watchdog, LVD,
SYSRESETREQ and LOCKUP reset domains, while POR/BOR resets the entire MCU.
See the per-family reset-summary page in JSON. Do not treat BOR as an ordinary
non-POR event just because a single reset flag was consumed or cleared.

The L/R/W initialization text says non-power resets retain RTC registers and
counting; x030/F020 explicitly say to inspect whether RTC already runs and
preserve DATE/TIME after non-POR. The conservative common policy is:

- First inspect source, START, clock readiness, format, prescalers where present,
  valid calendar data and available boot/reset cause.
- Attaching to a compatible running RTC changes no calendar, clock source,
  prescaler, calibration, alarm, timestamp or interrupt configuration.
- Return an error on incompatible configuration; do not silently repair by
  resetting. Changing 12/24-hour mode also requires conversion, not a bit flip.
- Cold initialization or deliberate time setting is a separate explicit API.
  Do not copy SDK deinit, whole-register defaults or blanket RESETFLAG clearing.
- Drop should leave timekeeping and its functional source alive. Releasing a
  configuration-clock resource must not release the oscillator needed by RTC.

The register inventories provide no backup SRAM/register array, independent
VBAT selector, or proven battery-switched RTC domain. Datasheet pin/source
review does not establish an independent VBAT supply. Retention with VDD kept
above its reset threshold must not be described as retention across loss of
VDD. An external battery powering the board is a board-level design decision.
Deep-sleep capability likewise depends on the selected oscillator remaining
available; HSIOSC/HSE support in run mode is not a promise of deep-sleep time.

## IRQs, alarms, wake timer and timestamp

IER, ISR and ICR have identical implemented mask 0x5F:
ALARMA bit0, ALARMB bit1, AWTIMER bit2, TAMP bit3, TAMPOV bit4, INTERVAL bit6.
ISR is read-only. ICR is **R1W0**: zero clears, one does nothing. Its reset is
0x7F; bit5 is reserved with reset one and bits31:7 reserved with reset zero.
A bounded mask write can use `0x7F & !(requested & 0x5F)`. Starting a typed write
from all zero would clear every source, losing unrelated pending events.
The SDK's `~RTC_IT` demonstrates W0C intent but writes reserved upper bits one;
it is not the preferred reserved-bit-preserving HAL sequence.

ALARMA/B match weekday selection plus configurable hour/minute/second fields.
WEEKMASK bit0 corresponds to Sunday, bits1–6 Monday–Saturday; one enables that
day. There is no year/month/day match. An `await_until(DateTime)` API would need
software date gating and race handling, not a direct register mapping. Matching
sets the sticky ISR flag even before ISR servicing; enabled events request the
one shared RTC IRQ. ALARMB polarity remains blocked as described above.

Periodic INTERVAL encodings: 0 off; 1 every half second; 2 on second change;
3 minute change; 4 hour change; 5 date change; 6/7 month change. These are
calendar boundaries, not an arbitrary duration timer starting at the call.

The 16-bit auto-reload **downcounter** has duration (AWTARR+1)/clock:

- Classic AWTSRC 0–3 uses RTCCLK /2,/4,/8,/16; 4–7 uses RTC1Hz /1,/2,/4,/8.
- Low-family AWTSRC bit2 chooses RTCCLKD or TICKCLK; AWTPRS[1:0] divides that
  by 2/4/8/16. The latter requires START=1. AWTCNT is readable only on the low
  families; retry until two reads match.
- The advertised 61 μs–145.63 h range assumes the nominal 32768 Hz/default
  clock path. Derive the actual range from the chosen source and prescalers.

Timestamp captures month/day/weekday and time, **not year**. A second timestamp
event while TAMP is uncleared sets TAMPOV. Do not advertise lossless event
queuing or tamper-driven backup erasure; those behaviors were not established.

Async work needs an RTC singleton/IRQ binding, AtomicWaker registration and
recheck ordering, source-selective clear, exact cancellation/Drop semantics,
preserved unrelated interrupts, wrap/deadline policy and sleep/wake validation.
An Embassy time driver must also be monotonic; user-settable calendar time is
not inherently monotonic. Defer both async alarms and a time-driver claim.

## Calibration

COMPEN/COMPCFR1 uses COMP[11:0], STEP/PERIOD[13:12], SIGN bit14, EN bit15;
upper sixteen bits are reserved in all reviewed manuals. Classic compensation
is documented for LSE; low-family compensation adjusts TICKCLK, with the
numeric examples using LSE/default PSC values. Periods are 32/128/256 s, maxima
511/2047/4095 pulses, documented steps 0.950/0.238/0.119 ppm and approximately
±488 ppm range. The step is not an accuracy guarantee. Do not apply those fixed
ppm figures blindly to arbitrary non-default RTCCLKD rates.

SIGN=0 compensates a fast source downward by increasing the count; SIGN=1
compensates a slow source upward. A precision external frequency reference and
a qualified RTC output pin are required to measure the actual error. Old SDK
`RTC_CalibrationConfig` functions still write Freq<<16 on classic families;
these are specifically unsafe templates for current manual-safe code. Likewise,
old RTC1HZ=PCLK output encodings are not legitimized by retained header enums.

## Bounded first HAL recommendation

Start with **L011/L012 blocking calendar**, sharing implementation only where
both own manuals agree and retaining their own PAC selection. Their WAIT-only
protocol avoids mixing classic/L010 handshakes, and direct HSIOSC is expressible
with the existing run-mode clock source after adding an explicit RTC clock
capability. Accurate low-power wall-clock use should instead wait for the
public LSE/LSI ownership/readiness work above.

Suggested shape:

- `Rtc::attach_preserving_state(Peri<RTC>, verified_clock)` returns a Result
  without resetting, setting time, clearing alarms or consuming global reset flags.
- Explicit `initialize_if_unset(config, datetime)` for a cold/unset clock;
  distinguish NotRunning, IncompatibleClock/Format, InvalidDateTime and timeout.
- `now()` returns a validated whole-second date/time using bounded coherent reads;
  `set_datetime(&mut self, ...)` validates before writing and waits for settlement.
- Keep one owned driver. Any immutable time-provider clone must share a
  synchronization policy with writers, rather than merely duplicating MMIO access.
- Do not reset/stop RTC in Drop. Defer calibration, timestamp/output pins,
  subseconds, low-power integration, ALARMB and async scheduling.

The upstream reference is Embassy commit
[f16efeffe37581092ec184718e6fdb1620393214](https://github.com/embassy-rs/embassy/tree/f16efeffe37581092ec184718e6fdb1620393214/embassy-stm32/src/rtc).
Its `mod.rs` provides ownership, Result-returning reads and an immutable provider
pattern; its STM32 prescalers, SSR read protocol, backup-register methods and
unconditional enable/reset paths are not transferable hardware behavior.
Its generic DateTime accepts years up to 4095 and days up to 31; CW32 needs an
explicit 2000–2099 and month/leap-day validation boundary instead of unchecked
subtraction/truncation to the BCD hardware year.

## Verification and remaining board work

Completed:

- One frozen-scope `./d gen-all`; all 13 families / 37 exact catalog parts still
  generated. No extra canonical merge/split introduced.
- `python tests/check_rtc_pac_corrections.py --compare-baseline`: eight pinned
  manual/SVD/input proofs; five exact canonical deltas/reuse hashes; all 32
  affected generic/exact profiles; 10 positive PAC builds; 20 intended E0599
  failures; two pure host DATE-mask tests preserving reserved bits.
- The bounded own-manual table comparison verifies all 159 register offsets and
  667 field spans from ten distinct RTC manuals, normalizing only the documented
  F020 alarm naming aliases.
- Exact normalized hash equality for all 128 non-RTC authored and generated
  register templates.
- `python tests/audit_generated_parity.py`: full family source/PAC parity passed.

Not established by source or compilation: oscillator startup on a real board,
measured drift/calibration, battery or brownout retention, midnight/leap-day
read coherence, access-window timing, alarm B polarity, shared-IRQ races,
AWT off-by-one on silicon, low-power oscillator retention or wake behavior.
Hardware validation should cover reset types and preserved state, malformed
BCD rejection, 12-hour noon/midnight conversion, 23:59:59 transitions and leap
February, stuck/missing clocks, cancellation and source-specific flag clearing.

## Pinned manual sources

All SHA-256 values below were checked against the actual PDF bytes. SDK archives,
headers/drivers and datasheet hashes/URLs are in the accompanying JSON.

- [CW32x030_UserManual_CN_V2.5.pdf](https://www.whxy.com/uploads/files/20240920/CW32x030_UserManual_CN_V2.5.pdf)
  SHA-256: `1afd49261f0f0689af8cb8ebf1b0ac1c00e3209b20d3c722106707ff4a10bdd2`
- [CW32F002_UserManual_CN_V1.4.pdf](https://www.whxy.com/uploads/files/20240920/CW32F002_UserManual_CN_V1.4.pdf)
  SHA-256: `e453b3f82d9d180a86cd5f7cb18d858d7f228afc8101d87c700ca9d5c02d5add`
- [CW32F003_UserManual_CN_V2.3.pdf](https://www.whxy.com/uploads/files/20240920/CW32F003_UserManual_CN_V2.3.pdf)
  SHA-256: `0fa58dac223add7f2ac1ee714a7df7db4e414e0f193601b80dda96a948bfc738`
- [CW32F020_UserManual_CN_V1.4.pdf](https://www.whxy.com/uploads/files/20240920/CW32F020_UserManual_CN_V1.4.pdf)
  SHA-256: `279521eaf7d32b241e614a9553044a4d99c1e7ca2566efa3b0fdd2b66c106eed`
- [CW32L010_UserManual_CN_V1.2.pdf](https://www.whxy.com/uploads/files/20260626/CW32L010_UserManual_CN_V1.2.pdf)
  SHA-256: `b66ae2b2837cf22aede7f19312b82659ea10f96960bfe7965de8733bb72513fa`
- [CW32L011_UserManual_CN_V1.1.pdf](https://www.whxy.com/uploads/files/20260602/CW32L011_UserManual_CN_V1.1.pdf)
  SHA-256: `b245887e9037caf267f739e25bd3c354c2579d23315fc06389e055f371159f4f`
- [CW32L012_UserManual_CN_V1.4.pdf](https://www.whxy.com/uploads/files/20260603/CW32L012_UserManual_CN_V1.4.pdf)
  SHA-256: `a9e54694a26f03c1f3e3041f40844900168ca2b8e6142f3328671b92e6b7a340`
- [CW32L031_UserManual_CN_V1.6.pdf](https://www.whxy.com/uploads/files/20240920/CW32L031_UserManual_CN_V1.6.pdf)
  SHA-256: `4288cfd97b56385059c5a283f69972047af4773ef8bbc4d8b51d8155fb17a760`
- [CW32L052_UserManual_CN_V1.5.pdf](https://www.whxy.com/uploads/files/20240920/CW32L052_UserManual_CN_V1.5.pdf)
  SHA-256: `4bac53df4db69a3b76c833cb14dd45b0e5c7f9884506c83a5cda6ea00a859f41`
- [CW32L083_UserManual_CN_V2.0.pdf](https://www.whxy.com/uploads/files/20240920/CW32L083_UserManual_CN_V2.0.pdf)
  SHA-256: `9930bf1755f3bbf8933163c2d0da57fd9a4f3250a358a4c0bfc75ed4eda3a0a3`
- [CW32R031_UserManual_CN_V1.3.pdf](https://www.whxy.com/uploads/files/20240920/CW32R031_UserManual_CN_V1.3.pdf)
  SHA-256: `fbee9b6942be9fa09f00c946705644d5356c4249f3e2280e3dfe5f0cb342eddb`
- [CW32W031_UserManual_CN_V1.4.pdf](https://www.whxy.com/uploads/files/20240920/CW32W031_UserManual_CN_V1.4.pdf)
  SHA-256: `b6973677946a9332b0e5b3e954119768aa40469d44140a73e18648e9419bedc9`
