# Init-only LSE system clock on five exact L031/R031/W031 packages

`Config.sys = Sysclk::LSE` selects the existing `Config.lse = Some(...)` source
on CW32L031C8T6, CW32L031C8U6, CW32L031F8U6, CW32R031C8U6 and CW32W031R8U6
only. This is one board-qualified nominal 32768 Hz source, shared unchanged by
SYSCLK/HCLK/PCLK, `Clocks.lse`, `LseClock` and the RTC calendar's /32768 division.
It adds a public exhaustive enum variant on those packages. Downstream matches
must handle it. Default HSI and all old HSI/HSE plus auxiliary-LSE paths retain
their admission, operation order and health contracts. The classic three, native
seven, L052/L083, aliases and all other exact packages are outside this addition.

The board bounds apply to every individual LSE cycle, across supply, temperature,
aging, load and short-term variation. Qualify crystal analog/startup behavior or
bypass levels, duty, edges and pulse widths separately; no maximum startup is
promised. PC14 is reserved for bypass and PC14/PC15 for crystal, retaining the
union of inherited reservations. L031F8U6 has those LSE pads but no HSE pair:
optional HSE still rejects. W031's default HSI-only board envelope is wider than
LSE qualification; declare a covering board/source interval inside 2.0–3.6 V.
No RF operating mode is read or qualified.

## Own monitor and handover

The internal detector reference uses the own factory LSI halfword at 0x00100A02,
masking native ten-bit TRIM only after rejecting raw 0xFFFF. Zero is not erased.
Its nominal/minimum/maximum rate facts are 32800/31816/33784 Hz under the existing
own RTC factory conditions; this does not qualify LSI SYSCLK or every-cycle LSI
timing. The new target alone requires strict `256 * LSE_min > 129 * 33784`.
Hardware specifies 128 LSE edges per 256 LSI cycles; the extra edge is software
margin. Integral 17023 Hz fails and 17024 Hz passes this margin. This rate model
does not prove every detector window, a loss deadline or CPU progress. General
auxiliary LSE does not acquire this target-only restriction.

The new target classifies native LSI before any monitor parameter/request change
and again before requesting it. Factory-matching enabled, dual-stable LSI can be
borrowed, including selected LSI. Matching enabled but dual-unstable LSI may wait
only with no detector and no selected-LSI state. Matching disabled, dual-unstable
LSI may start only with neither detector, selected-LSI state nor ready observer.
Mirror disagreement, disabled-but-stable/selected state, or a detector without
an already requested dual-stable matching reference rejects conservatively.
Separate reads can straddle a real transition; rejection does not prove a chip
fault. Both current stable indications, request, factory TRIM, unchanged WAIT and
inherited CCS policy must agree before the HSI trim bridge or LSE startup.

Matching TRIM is never rewritten and does not acquire the mismatched-trim owner
proof. Requesting matching cold LSI may resume parked AWT SOURCE1, UART SOURCE3,
GPIO FLTCLK5, MCO SOURCE4 and bonded PB11 AF1 consumers. GPIO-bank gate inspection
can advance sampling, filters and armed events, including before an error. The
functional handover must allow this work; restoring gates cannot undo events.
Normal clock-sensitive work, interrupts and DMA obey the existing initialization
exclusion and pre-Rust quiescent bus-master contract. This is not arbitrary
bootloader sanitization or a hidden safe-Rust memory-safety obligation.

Mismatched LSI retains this family's unchanged two stopped/consumer/stopped
passes and trim-only write. No classic or L052/L083 admission is imported. It
never stops/retrims live LSI or changes WAIT. Existing ready events and interrupts
are preserved; no flag clearing, reset, selector rerouting or work-gate shutdown
manufactures admission. Fresh LSE retains complete RTC/consumer/pad preflight;
exact monitored live reuse retains consumers without oscillator/pad writes.

## Sequence and limits

Initialization preserves incoming CLKCCS/HSECCS and LSELOCK. After Flash/bus
guards it escapes through unchanged HSI, establishes full factory-LSI readiness,
performs an independently admitted HSI trim bridge if needed, sets configured HSI
and prepares optional HSE. Retained HSIOSC AWT/LVD users still prohibit HSI trim.
It then starts or exactly borrows LSE while HSI executes. Fresh startup enables
only LSECCS and LSEEN. LSI's software request remains permanently enabled.

Complete source/monitor/HSI/HSE/CCS/pad checks surround gate inspection. Final
AHB/APB divisors are explicitly installed with HSI selected and acknowledged.
Pure validation separately proves this HSI tree legal even when CLKCCS is zero.
Final Flash WAIT covers the larger upper HCLK of configured HSI and board LSE.
One final LSE mux write follows, with no later CR0/divider/source cleanup. Full
checks, Flash WAIT and a final selector/divider readback precede publication.
Inherited HSE ownership is retained on this early-return path. The stronger
shared-LSE monitor health marker is installed only after all target checks pass;
failed RCC init and old auxiliary paths do not acquire it.

Configured HSI is the own-manual-derived fallback model; no forced /6 CCS rule is
stated. DeepSleep's fixed 8 MHz wake behavior is unrelated and excluded. Frozen
LSE rates are invalid after loss/fallback; no clock republication or transparent
recovery, calendar continuity, runtime switching, sleep/wakeup or RF is promised.
Every AHB/APB choice gives nominal PCLK below 1 MHz, so the fixed time driver
rejects before singleton acquisition or RCC writes. Peripheral timing constraints
remain independent; useful operation at these rates is not universally promised.

Waits are iteration budgets, not durations. Errors publish no clocks/tokens and
retain requests, pad reservations and diagnostics; partial guards/calibration/gate
changes may remain. Reset before retrying; ordinary reset can retain LSE. The
incoming source/voltage/Flash state must be legal and remain available during the
transition. Arbitrary asynchronous source loss during earlier RMW is excluded.

Source binding: [six-original qualification](l031-r031-w031-lse-sysclk-qualification.json).
Implementation/build review and physical silicon qualification are separate.
Publication remains HOLD.
