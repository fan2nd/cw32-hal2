# RTC Alarm A and retained alarm events

This increment adds typed weekly Alarm A programming on the eleven existing
calendar families, plus Alarm A/B status and selective acknowledgement. It adds
run-mode async notification only on CW32L010/L011/L012. F002/F003 have no RTC.
It does not change the calendar source, oscillator lifecycle, calibration,
prescalers, electrical limits or timekeeping accuracy.

`Rtc`, `DateTime`, `RtcConfig` and `Peri` remain in the existing RTC module.
The pinned Embassy revision is `f16efeffe37581092ec184718e6fdb1620393214`.
Its actual `embassy-stm32/src/rtc/{mod,v2,v3}.rs` has no public alarm-match or
alarm-wait API to copy. These are CW32-specific extensions using the project's
existing owned driver and typed interrupt binding organization; STM32 register
protocols have not been imported.

## Public operations

- `set_alarm_a(AlarmAConfig)` writes only disabled ALARMA. Binary hours are
  0..=23; minute/second are 0..=59. `None` ignores that component. `AlarmDays`
  selects weekdays with explicit Sunday translation. The hardware's current
  12/24-hour format is preserved, including noon/midnight conversion.
- `set_alarm_a_enabled(bool)` changes only `CR2.ALARMAEN`. Both programming
  operations reject an already enabled `IER.ALARMA` rather than commandeering
  a retained interrupt. Reconfiguration also rejects an enabled comparator.
- `alarm_status(Alarm::A/Alarm::B)` reports comparator enable, interrupt enable
  and its sticky match flag. This is one bounded observation; it cannot promise
  that no event occurs immediately after the read.
- `clear_alarm(Alarm::A/Alarm::B)` writes the documented ICR read-one/reset seed
  `0x0000_007f`, changing only the selected typed R1W0 field to zero. Other flags
  receive one; reserved bit 5 keeps its documented reset value. No ISR write or
  ISR read/modify/write occurs. Same-bit events coalesce, and a simultaneous
  event/acknowledgement can be lost. A later event can immediately reassert it.
- `wait_for_alarm(alarm, binding).await` is available on the three direct-access
  RTCs. It observes an already enabled comparator. It accepts an existing flag
  immediately and does not interpret or configure the disputed Alarm B match.

The caller controls acknowledgment, then enabling, then waiting. Changing the
calendar can skip or repeat recurring weekly matches. These operations are not
an absolute deadline scheduler or an event counter.

## Access, IRQ ownership and cancellation

Every operation retains exclusive access through the original `Rtc`'s owned
`Peri<RTC>` and clock capability. All checks are fallible and bounded. A failed
write can be partial; there is no rollback guarantee.

Classic families perform the documented WINDOW pre-wait with interrupts
available, at most 1,000 polls separated by the existing HCLK ClockBounds delay.
They recheck WINDOW under a critical section, unlock, set ACCESS, perform a
fixed-size register operation, release ACCESS and relock on every exit. They
never stop START, reset the RTC or change the source. The manual's one-second
ACCESS requirement remains an application/board timing precondition: ordinary
interrupts cannot extend the protected transaction, but debugger/NMI/bus stalls
and actual silicon timing have not been measured. Stopped RTCs need no ACCESS
transaction.

The L010/L011/L012 own access sections explicitly exclude ALARMx, CR2, IER, ISR
and ICR from DATE/TIME/AWTARR synchronization. Their alarm operations perform
only a fixed-size direct transaction under short ordinary-interrupt masking.
The L010 RTCLPM precondition remains checked. No application closure executes
while RTC registers are unlocked.

An async wait rejects *any* preexisting IER bit before modifying NVIC or IER.
The generated source-qualified capability asserts a dedicated RTC GLOBAL
vector. The `Binding<RTC, AlarmInterruptHandler>` proof and mutable RTC borrow
scope ownership to the wait. Setup disables/unpends the vector, enables only
the selected IER bit, checks readback, marks the waiter active and enables NVIC.
Registration of the waker precedes each flag check, so an event before the
first poll is still observed. The ISR checks active ownership and enabled
alarm flags, disables NVIC delivery, and wakes; it never acknowledges a flag,
waits for a window or unlocks RTC registers.

On completion or cancellation, a guard disables RTC NVIC delivery, removes its
selected IER bit with the direct-access protocol, clears active ownership and
unpends NVIC. It leaves the comparator, calendar, oscillator and hardware flags
intact. Previous NVIC state is deliberately not restored. A future dropped
before its first poll has no hardware effect. Safe exclusive ownership prevents
simultaneous RTC operations during the wait. External PAC/debugger access can
violate the driver's assumptions, as with other owned HAL peripherals.

Classic async support remains out of scope: its manual's general WINDOW/ACCESS
protocol has no demonstrated bounded interrupt/cancellation path. SDK shortcut
writes are insufficient evidence to relax that protocol.

## Alarm B conflict

Alarm B is present and documented. Its mask polarity is contradictory, not
missing. The conflict was independently checked in rendered PDF pages for each
own manual, including both current x030 families explicitly named in their
shared manual. For each manual:

- ALARMA offset `0x1c`, bits 23/15/7 (`HOUREN/MINUTEEN/SECONDEN`) say zero compares
  and one ignores the component.
- ALARMB offset `0x20`, those same bit positions say zero ignores and one compares.
- The shared A/B examples label `0x7f063000` as an exact daily 06:30:00 alarm.
  Its three zero mask bits therefore imply comparison, contradicting B's table.
- The own SDK's `RTC_SetAlarm` forms one `RegTmp` from `RTC_AlarmMask` and time
  fields, then writes the identical value to either ALARMA or ALARMB without
  a polarity conversion.

No polarity was guessed. `AlarmAConfig` and the A-only programming methods make
that limit visible in the API. B status, acknowledgment and direct-family waits
use independently documented enable/flag/IER bits and leave its match untouched.

The table gives printed pages, with one-based PDF pages in parentheses. Full
URLs, PDF/member hashes, extracted page hashes, rendered image hashes and SDK
line spans are in [rtc-alarms-evidence.json](rtc-alarms-evidence.json).

| Family / own printed revision | Shared A/B example section and page | ALARMA section and page | ALARMB section and page |
|---|---|---|---|
| CW32A030 Rev 2.5 | 12.3.9, 181 (182) | 12.5.8, 190 (191) | 12.5.9, 191 (192) |
| CW32F020 Rev 1.4 | 12.3.9, 178 (179) | 12.5.8, 187 (188) | 12.5.9, 188 (189) |
| CW32F030 Rev 2.5 | 12.3.9, 181 (182) | 12.5.8, 190 (191) | 12.5.9, 191 (192) |
| CW32L010 Rev 1.2 | 10.3.10, 145 (146) | 10.5.9, 155 (156) | 10.5.10, 156 (157) |
| CW32L011 Rev 1.1 | 10.3.10, 145 (146) | 10.5.9, 155 (156) | 10.5.10, 156 (157) |
| CW32L012 1.4 | 13.3.10, 171 (197) | 13.5.9, 180 (206) | 13.5.10, 181 (207) |
| CW32L031 Rev 1.6 | 12.3.9, 173 (174) | 12.5.8, 182 (183) | 12.5.9, 183 (184) |
| CW32L052 Rev 1.5 | 12.3.9, 186 (187) | 12.5.8, 195 (196) | 12.5.9, 196 (197) |
| CW32L083 Rev 2.0 | 12.3.9, 198 (199) | 12.5.8, 207 (208) | 12.5.9, 208 (209) |
| CW32R031 Rev 1.3 | 12.3.9, 175 (176) | 12.5.8, 184 (185) | 12.5.9, 185 (186) |
| CW32W031 Rev 1.4 | 12.3.9, 174 (175) | 12.5.8, 183 (184) | 12.5.9, 184 (185) |

## Evidence and verification limits

`cw32-data/rtc-alarms.yaml` carries an own-family source profile. The data
generator validates its register version, mask fields, ALARMA polarity, B
exclusion, IRQ route and clear policy against the reviewed source, then projects
an optional `rtc_alarms` capability through chip JSON and generated PAC metadata.
The HAL build script consumes only that peripheral metadata; it does not read
the authored alarm catalog. F020's distinct PAC mask/week field names are used
directly. ICR commands use generated `Icr::write_noop()` from the existing
`register-writes.json` policy, preserving all unselected events/reserved bits;
`Default` remains zero and is never used for acknowledgement. The source/data validator checks
all seven register variants, every RTC chip selection's actual dedicated IRQ
route, 40 source hashes and 99 extracted manual page hashes. It never executes
HAL code or substitutes mock registers.

The ordinary ARM build receipts cover all thirteen catalog families with and
without rt/defmt. Real polling firmware links for all eleven RTC families; real
Embassy async firmware links for L010/L011/L012. Saved ELFs and exact executable
input hashes accompany the review packet. These are compile/link results, not
silicon validation; no flashing, HAL test harness or mock was used.

Board QA still needs alarm timing in both hour formats, rollover and calendar
jumps, near-boundary acknowledgment, IRQ arrival around cancellation, stale
flags and the documented ACCESS deadline. B match programming needs a resolving
vendor source or controlled silicon evidence before implementation.

The original clock limits remain: factory LSI is never retuned; classic nominal
calendar rate is 32800/32768 per SI second, with its existing RC envelope; HSI
bounds remain unchanged; W031 retains its conservative generic 2.0 V minimum.
There is no new LSE/HSE source, precise wall-clock, low-power wake, RTCOUT,
timestamp, periodic-interval, automatic wake timer or compensation claim.
