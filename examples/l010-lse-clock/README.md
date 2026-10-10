# Native CW32L010 calendar examples

Normal linked firmware for the exact F8P6, F8U6 and Y8M6 packages. Select one feature and one of `crystal_calendar`, `bypass_calendar`, or `hsi_calendar`. The first two use PB01 input and (crystal only) PB00 output; PA04 toggles when the read calendar second changes. HSI uses the existing one-argument CalendarClock constructor and requests no LSE.

The LSE source declaration and drive values are examples to replace with board-qualified values, not measured guarantees. Both examples deliberately select StartupOnly for ordinary reset startup: later source loss may leave STABLE latched, so neither successful RTC reads nor PA04 are fault monitors. For MonitoredExistingRoutes, the incoming LSI must already be stable, legal and unchanged; existing brake/timer fault consequences are part of that policy. No LSI calibration is performed by this path.

Start from genuine hardware reset with no intervening timer/GPIO setup, or establish the public init handover conditions. New LSE startup with RTC SOURCE=0 requires RTC_OUT/RTC_1Hz observers to be disconnected or independent of the transition, including BTIM trigger/reset, GTIM/ATIM TI, LPTIM, and external PB04/PB06 users. GPIOB temporarily operates as a whole bank, even before errors. These examples deliberately provision only an unset calendar; use attach_preserving_state for an existing one.

LSE writes PSC1=0 and PSC2=0x3fff. Its nominal calendar transition rate is exactly 32768/32768 Hz; bounds follow the board-declared source. HSI retains its qualified 48-MHz source and divider rule. Build and ELF inspection establish software/link coverage only; crystal startup, board electrical behavior and silicon fault response require physical validation.
