# RTC calendar firmware

Three ordinary Cortex-M0+ binaries toggle PA4 on observed calendar-second transitions. Connect only a suitable indicator/load; PA4 is an example output, not a claimed board LED. Build with one exact package feature and target `thumbv6m-none-eabi`.

- `preserve_calendar`: attach to a compatible retained calendar and read it.
- `initialize_calendar`: explicitly provision an unset/stopped calendar. Replace the sample date with the intended time before use.
- `set_calendar`: explicitly correct an existing calendar, preserving its 12/24-hour format. Replace the sample date before use.

Classic-family firmware requires board startup/bootloader to have already loaded factory LSI trim. `LsiClock::new` checks the pinned calibration halfword, preserves trim, enables the source and waits for startup. An unmatched trim is an error; the example does not silently calibrate it. The LSI calendar's exact nominal rate is 32800/32768 ticks per SI second, before its own ±3% or F020 ±5% tolerance. This is unsuitable as an accuracy claim.

L010/L011/L012 use frozen HSIOSC and rated-source-safe prescalers. L010 also requires RTCLPM=0 for writes. No example enables alarms, writes compensation, enters deep sleep or claims battery retention. L010 ACCESS waiting is capped at32 reads and can require a retry; no unbounded retry is hidden here. A debugger breakpoint within a calendar write violates its timing assumptions. Links are compile evidence, not on-device rollover/access-window evidence. Do not flash without reviewing board pins, source calibration and initial time.
