# Safe HSI recalibration: x030, F020, F002 and F003

## Correctness finding

The original F030/A030/F020 HSI initialization changed oscillator TRIM while HSI
was running. F002/F003 similarly did not provide a stopped-oscillator path when
initialization entered with HSI enabled. The earlier host model accepted those
writes and therefore could not establish that the sequence followed the manual.
This was a real initialization correctness defect, including the historical
stage 2 implementation. Existing delivered archives have not been modified.

## Source evidence

- CW32x030 User Manual CN Rev 2.5 §4.3.4 p49 expressly prohibits changing
  HSIOSC parameters after startup. EN Rev 1.0 §4.3.4 p50 corroborates it.
  §4.5.2 permits changing HSI DIV while preserving TRIM; §4.5.7 permits leaving
  PLL through stable HSI before PLL shutdown. SYSCTRL PLL.STABLE must clear
  before changing its input oscillator.
- CW32F020 User Manual CN Rev 1.4 §4.3.4 p47 has the same oscillator restriction.
  §4.5.2 p61 permits live DIV-only changes with TRIM unchanged. Its PLL switch
  constraints similarly prohibit a direct PLL-to-LSI jump.
- CW32F002 User Manual CN Rev 1.4 §4.5.2 p50 explicitly preserves TRIM during
  live DIV changes; §4.5.5 p51 configures TRIM before enabling HSI.
- CW32F003 User Manual CN Rev 2.3 §4.5.2 p52 and §4.5.5 p53 provide the same
  sequence. No blanket oscillator prohibition is claimed for F002/F003; their
  implemented safe sequence follows the documented trim-before-enable order.
- All five datasheets' Table 7-4 limits HCLK/PCLK to 24 MHz at
  1.65 V <= VDD < 1.8 V. At VDD >= 1.8 V, the datasheet ceilings are 64 MHz for
  F030/A030 and 48 MHz for F020/F002/F003. This HAL still requests at most
  48 MHz because the supported source is calibrated 48 MHz HSIOSC.

Reference manuals and datasheets are pinned in the repository's source/clock/
pinout records; this review used their actual contents rather than SDK startup
routines that write calibration directly.

## Implemented protocol

The shared `rcc/hsi_48mhz.rs` implementation now:

1. Validates inputs, source/divider encodings and factory trim before writes.
2. Enables FLASH configuration and raises WAIT to the documented value 2.
3. Selects temporary AHB /4 and APB /8, verifies both, then applies barriers.
4. Starts/verifies the existing HSI, selects it, and stops/verifies PLL. This
   preserves the documented PLL-to-HSI route before any LSI handover.
5. If HSI TRIM already equals the factory value, leaves HSI running and skips
   LSI completely. The eventual live DIV change retains TRIM.
6. Otherwise enables/verifies LSI without modifying its TRIM or WAITCYCLE;
   selects stable LSI; disables HSI and waits for both HSIEN=0 and STABLE=0;
   writes/verifies factory TRIM; restarts/verifies HSI; and selects it again.
7. Restores the original LSI software-enable request. It does not require
   LSI.STABLE to clear: a watchdog or other hardware consumer may keep LSI on.
8. Applies the requested HSI divider and final bus dividers, then rechecks
   final HSI DIV/TRIM and STABLE after mux/bus writes. It publishes frequencies
   and reduces flash latency only after successful final readbacks.

HSIEN and required CCS bits are read back after protected CR1 writes; LSIEN is
also verified before relying on its software request. A denied write cannot be
hidden by an oscillator that happened to be stable already.

The independent F002/F003 backend uses the same temporary bus/LSI protection
without introducing nonexistent PLL, CCS, cache or prefetch controls. It applies
TRIM and DIV before restarting HSI when calibration is necessary; its matched
TRIM path keeps the documented live-DIV behavior.

## Operating assumptions and failure boundary

The caller must enter initialization from a clock/voltage operating point that
is legal for the selected device. The conservative transition bound is at most
72 MHz from the manuals' clock/flash descriptions, not permission to overclock
a part. Temporary /4 AHB gives at most 18 MHz HCLK, below the 24 MHz low-voltage
ceiling; /8 APB further divides it. This avoids briefly driving 48 MHz through
an old /1 bus when the requested final HCLK is 24 MHz or less. It also avoids
the excessively slow LSI execution caused by choosing temporary AHB /128.

Final HCLK/PCLK must satisfy the actual board voltage limits. The public
`Config::frequencies` documentation states the below-1.8-V 24 MHz requirement;
this API does not measure VDD. Oscillator tolerance and board electrical
requirements still apply.

Every hardware wait is bounded. Failed calibration may leave temporary LSI
selected or enabled, with conservative flash latency retained. No clocks are
published on failure. The existing initialization ownership contract consumes
the singleton set on hardware failure; reset before retrying.

## Regression model

The updated register models reject live TRIM writes, require HSI disabled and
STABLE cleared before calibration, require a stable CPU clock source, preserve
LSI parameters and original software-enable state, reject direct PLL-to-LSI
transitions, and enforce the conservative buses before HSI changes. Added cases
cover unchanged trim, active-HSI entry, LSI startup failure, HSI stop/restart
failure, denied mux/restore/mandatory-CR1 writes and final autonomous divider
changes. PAC-on-RAM tests use the matched-trim path because plain RAM does not
emulate oscillator STABLE side effects.

These are source-level and software-model tests. They do not replace oscillator,
voltage-corner, flash-execution or board validation.
