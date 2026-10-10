# RTC calendar breadth

The RC-source description and historical batch evidence below remain the baseline.
Current held LSE support covers nineteen exact packages: the previous sixteen
use their [own monitor contracts](qualified-lse.md), while three native L010
packages follow [StartupOnly/MonitoredExistingRoutes and functional handover](qualified-l010-lse.md).
L010 retains HSIOSC and adds LSE source0 with PSC1=0/PSC2=0x3fff; StartupOnly
checks configuration/startup state without proving continued calendar progression.
L011/L012 HSIOSC behavior is unchanged. Later alarm/async scope is in
[the current alarm contract](rtc-alarms.md); the unsupported-work list below
records the original batch rather than current capability declarations.

The shared blocking calendar now covers all eleven RTC-bearing families. F002
and F003 have no RTC. All operation paths use the selected direct typed PAC;
there is one calendar/BCD engine, three genuine access protocols, and generated
source facts. No register template, interrupt topology or canonical register
reuse assignment changes in this batch.

## Supported sources and rate

| Family | Functional source | Nominal Hz | Qualified range Hz | VDD mV | Ambient C |
| --- | --- | ---: | ---: | ---: | ---: |
| A030/F030 | factory-trim LSI | 32800 | 31816–33784 | 1650–5500 | −40…105 |
| F020 | factory-trim LSI | 32800 | 31160–34440 | 1650–5500 | −40…105 |
| L031/L052/L083 | factory-trim LSI | 32800 | 31816–33784 | 1650–5500 | −40…85 |
| R031 | factory-trim LSI | 32800 | 31816–33784 | 2200–3600 | −40…85 |
| W031 | factory-trim LSI | 32800 | 31816–33784 | 2000–3600 | −40…85 |
| L010 | frozen HSIOSC | 48000000 | 47040000–48960000 | 1620–5500 | −40…85 |
| L011/L012 | frozen HSIOSC | 96000000 | 94080000–97920000 | 1700–5500 | −40…85 |

These are each own datasheet's factory calibration ratings under its stated
conditions. Neither supply, ambient temperature nor actual frequency is measured.
The existing RCC board declaration must stay true throughout use. See the exact
own PDF pages, URLs, hashes and SDK archive members in
[the source evidence](rtc-remaining-evidence.json). R031/W031 changes concern
only RTC and shared SYSCTRL fields; no radio operation is introduced.

W031 Table 7-4 permits 1.8–3.6 V in RF-LDO mode and 2.0–3.6 V in RF-DCDC
mode. RTC does not inspect or configure that board supply mode. Its generic
source qualification therefore requires the conservative intersection,
2.0–3.6 V. Deferring radio support does not establish RF-LDO mode; the
mode-specific 1.8 V source fact remains recorded in the evidence.

Classic RTCs divide the source by 32768. The published LSI nominal is **32800**,
so their nominal calendar tick rate is **32800/32768**, not exactly 1 Hz. Even
before RC tolerance, that ratio implies about 84.4 extra calendar seconds per
SI day. `calendar_tick_bounds()` retains this fraction; its duration methods
use exact division and round outward. Whole-Hz getters are intentionally coarse.
No rounded frequency is substituted at a hardware ceiling.

L010 selects PSC1=59, PSC2=399999; L011/L012 retain PSC1=119, PSC2=399999. The
intermediate nominal rate is 800 kHz and its upper bound is 816 kHz, within the
manual's 1 MHz ceiling. TICKCLK is nominally 2 Hz. Attach accepts other exact
nominal divider products only if their actual upper intermediate rate fits.

## Source ownership and retained state

`LsiClock::new(SYSCTRL, polls)` requires a previously frozen RCC tree. It reads
the own factory calibration halfword, rejects erased storage, and compares the
trim with the typed SYSCTRL.LSI.TRIM field. A mismatch returns an error before
any write. The capability does **not** load calibration. LSIEN=0 is insufficient
to prove no hardware user is starting or requesting this shared oscillator.
Board startup or a bootloader must establish factory trim before acquisition.
This remains a concrete prerequisite for classic-family use; there is no safe
cold-source provisioning API in this batch.

For matching trim, acquisition enables only LSIEN using the SYSCTRL key,
preserves WAITCYCLE/trim and neighboring controls, and polls both enable and
STABLE. It leaves LSI enabled on timeout and Drop to preserve other users.
STABLE indicates startup detection, not measured accuracy or continuous
clock-failure detection. SYSCTRL's singleton prevents duplicate safe calendar
clock capabilities. Existing LCD/IWDT/RCC paths do not retune the source.

`HsiOscClock` is also available on L010. Its RCC initialization now checks a
retained HSIOSC-selected RTC before any oscillator mutation, as L011 already
did. Compatible factory trim remains running through the documented live
HSI-divider transition. Incompatible trim is rejected with the RTC gate left
enabled; it is never silently stopped and recalibrated.

`attach_preserving_state` enables the configuration bus through central
`RTC::RCC_INFO`, then validates source, compensation, divider, START and calendar.
It does not unlock, clear flags, set dates, stop or reset the RTC. Both 12/24-hour
formats are read correctly. An already active ACCESS is rejected and untouched.
`initialize_if_unset` is separate and explicit: START must be zero, DATE must be
zero, and event/interrupt/compensation controls must be inactive. It preserves
inactive alarm matches and wake reload values. It never pulses reset or clears
boot/reset/interrupt flags. Partial initialization is not rolled back.

## Access and calendar semantics

- Classic A030/F020/F030/L031/L052/L083/R031/W031: poll WINDOW before unlock and
  ACCESS. Failed polls have at least the manual's 10 ms CPU delay, calculated
  from maximum HCLK ClockBounds, with at most 1000 attempts. Recheck WINDOW
  after masking ordinary interrupts, then perform the fixed DATE/TIME write
  and readback. The ACCESS guard clears ACCESS before the unlock guard relocks.
- L010: require RTCLPM=0 for writes, leaving that mode unchanged. Wait WAIT=0,
  unlock, set ACCESS, then wait WINDOW. The ACCESS-held poll is capped at 32
  reads independently of a large caller timeout. Ordinary interrupts are masked
  for this transaction. Errors drop both guards. No stop/start shortcut is used.
- L011/L012: retain the WAIT-only sequence. Deliberately stop START, wait for
  loading, write DATE and TIME with WAIT checks, verify the stopped pair and
  restart only on success. Failure may leave timekeeping stopped. TIME resets
  subseconds. CR1 reserved ACCESS/WINDOW bits are never used.

The 32-read cap is a small bounded-work policy, **not a proven wall-clock WCET**.
It prevents a huge caller poll count or ordinary interrupt handler from keeping
ACCESS set. The source's one-second limit still requires the actual fixed
transaction to finish within one second. Debugger/NMI stalls, abnormal APB stalls
and source failure are outside that assumption; no general real-time or silicon
access-window guarantee is claimed. If WINDOW is not observed within 32 reads,
L010 returns an error after cleanup; it does not leave ACCESS open or hide an
unbounded retry. This must be measured on supported operating points before
relying on the deadline in a product.

Whole-second reads use the manuals' fast same-register read exception, expanded
to repeated complete TIME/DATE pairs. Short attempts mask ordinary interrupts,
check loading where applicable and validate decoded values. There is no source-
documented full-calendar latch. Midnight consistency therefore remains board
validation, not a compilation result. Neither reads nor Drop stop the counter.

The software epoch is 2000–2099. Full month/leap-day and Gregorian weekday
validation is shared with the prior implementation. BCD is checked before range
conversion; classic eight-bit day/month slots are validated in full. Weekday zero
is Sunday. Twelve-hour noon/midnight conversion handles the PM bit explicitly.
Leap seconds, an inferred century and invalid reset DATE=0 are rejected.

A deliberate calendar write preserves format and event settings but changes when
events may occur. A failed write can be partial and has no rollback guarantee.
Writing START or releasing ACCESS is not presented as an abort/quiescence proof.

## Verification and gaps

See the frozen receipt packet for ordinary ARM builds of all 47 RTC selections
plus F002/F003, and real Cortex-M firmware links for preserving attachment,
explicit initialization and deliberate setting on one exact package per family.
The [firmware examples](../examples/rtc-calendar/README.md) use PA4 as an example
indicator output, with generated package/GPIO selection. No firmware was flashed.
No HAL tests, simulator harnesses or mock-register execution were introduced.

Source/data/PAC validators rehash 87 vendor artifacts, verify archive members,
check exact own clock domains and selected registers, inspect generated profile
facts, round-trip optional schema metadata, and compare normalized register reuse.
The pinned Embassy RTC API source is commit
`f16efeffe37581092ec184718e6fdb1620393214`; only ownership/API conventions are
reused, never STM32 RTC register timing or backup-domain behavior.

Remaining unsupported work: safe cold LSI factory-trim provisioning, LSE/HSE
lifecycle, alarms (including the unresolved ALARMB polarity contradiction),
async waiting, subseconds, compensation, RTC outputs/timestamps, deep-sleep/wake
integration and a monotonic Embassy time driver. No battery-switched VBAT or
power-loss retention claim is made. Hardware checks remain necessary for source
startup/drift, reset retention, midnight/leap-day coherence, TIME write loading,
ACCESS timing and error cleanup at the slowest supported bus clocks.
