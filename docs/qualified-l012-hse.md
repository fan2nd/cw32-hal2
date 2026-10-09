# Qualified CW32L012 direct HSE

This candidate adds one-time HSI/HSE clock initialization for CW32L012C8T6
(LQFP48), CW32L012C8U6 (QFN48), and their genuine common-package alias.
The default remains factory HSI /12, nominal 8 MHz. No PLL exists on L012.
The own-source receipt is [qualified-l012-hse-source-receipt.json](qualified-l012-hse-source-receipt.json).
Hardware execution has not been performed.

## Source declaration

`Config.hse: Option<Hse>` declares crystal or bypass mode, minimum/nominal/maximum
frequency, source operating conditions, and the board-selected drive. `Config.sys`
selects HSI or HSE. HSE requires its declaration and remains enabled/reserved if
HSI is the selected system source. Frequencies must satisfy
`0 < minimum <= nominal <= maximum`, with the entire actual interval inside
4–32 MHz. The own RM and DS agree on that interval for crystals; it is their
intersection for bypass (DS alone permits 1 MHz). Lower bypass frequencies are
not qualified by this API.

The board envelope must fit VDD 1.7–5.5 V and ambient −40–85 °C, and be contained
in the source's declared conditions. VDDA must equal VDD. Actual HCLK and PCLK
must not exceed 24 MHz if minimum VDD is below 1.8 V, otherwise 96 MHz. Factory
HSIOSC has an independent 94.08–97.92 MHz envelope. All calculations retain exact
rational bounds through dividers; rounded nominal Hertz never proves electrical
admission. External tolerance, loading, supply/temperature drift, aging and
short-term cycle variation belong in the declared endpoints. Nothing is measured.

Bypass waveform limits apply together: 40–60% duty; high and low each at least
15 ns; rise/fall each at most 20 ns; high 0.7×VDDIO..VDDIO; low VSS..0.3×VDDIO;
and the own TC-pad limits. A 32 MHz, 40%-duty signal fails the pulse-width limit.
The source must run continuously. Crystal drive/load/ESR/layout and startup are
board responsibilities. The datasheet's 2 ms startup is typical at 8 MHz and
cannot establish a maximum software wait.

## Startup and exact inherited reuse

An enabled HSE without a declaration returns `InheritedHseEnabled` before MMIO
writes, including configuration-gate writes. A declared enabled HSE is accepted
only when enabled/stable, fault-free, and already exactly matches all requested
mode/drive/wait/filter/detector and generated pad controls. This restriction
applies whether or not SYSCLK or RTC/AWT currently selects HSE. Exact reuse never
writes HSE configuration or oscillator pads, never stops HSE, and does not change
a retained source owner. Mismatches fail before oscillator/pad mutation; inspection
may temporarily enable and restore a configuration bus gate.

Fresh HSE requires both HSEEN=0 and STABLE=0 before pad/configuration writes. The
own HSE register has no frequency-range selector. Only drive codes 0–7 are named
and admitted, despite four-bit fields and SDK macros extending to 15. Pre-start
PDRIVER and run DRIVER use the same declared value, WAITCYCLE is fixed at 262144
cycles, and DIGFLT is false. DIGFLT=true is deferred; any future support must
prove actual maximum <=8 MHz, not merely nominal <=8 MHz. HEXENPOL and RFU are
preserved; automatic external oscillator enable through HEXEN is outside this API.

DETCNT is ceil(8,000,000,000 / actual minimum HSE Hz), fits 1..2047, and must
satisfy minimum_HSE × DETCNT > 131072 × 36080. This uses L012's own legal LSI
maximum, not L011's source discrepancy. HSECCS, LSECCS, CLKCCS and LSELOCK remain
unchanged. LSI configuration and the incoming software request remain unchanged;
hardware detector/filter requests may keep LSI running with LSIEN clear.

The fixed L012 CCS fallback is nominal HSI4 MHz, equivalent to HSIOSC /24, with
3.92–4.08 MHz bounds. It is independent of the HAL's default /12=8 MHz and the
requested HSI divider. Selected source, requested retained HSI and fixed fallback
must all fit the final bus and Flash configuration. No fault or ready flag is
cleared to make initialization succeed.

## Pins and retained owners

The own DS Table5-2 identifies PF0/OSC_IN at physical5 and PF1/OSC_OUT at physical6
on both packages. The RM's PB7/PC13 example is inconsistent and is not used.
Crystal reserves both PF0/PF1; bypass reserves PF0. Existing common LSE protection
continues to reserve PC14/PC15 according to mode and the independent PINLOCK +
LSELOCK rule before backend writes. Ownership is the whole-boot union and never
shrinks on a fault. Generated helpers use the actual GPIOF gate and fields,
configure input first, preserve other pins/ODR/shared filters/flags, and never
write unsupported PF0/PF1 pull-down bits. These pads have no ADC/VC/OPA analog
route overlap. Digital AF owners must be quiescent.

RTC SOURCE owns its raw clock even when calendar START=0 because PSC1/PSC2 feed
AWT. HSE-selected RTC requires declared exact HSE reuse. Raw-HSI RTC requires
already enabled/stable factory-correct HSI and blocks stop/retrim. Its actual
PSC1 output must already be <=1 MHz. Source, prescalers, calendar and AWT are
never rewritten. LSE/LSI RTC source configuration is preserved.

Before clock changes, the initializer reads the actual central gated registers,
restoring prior gate state and never resetting shared blocks:

- Either ADC1 or ADC2 enabled: reject, even without a current conversion
- Enabled LVD with nonzero SYSCLK filter: reject; LVD is read directly and has no RCC gate
- Any enabled VC1–4 with a nonzero PCLK filter or any of its 32 CR2 blank-trigger bits: reject
- Either OPA CALEN, AZRUN or START set: reject; calibration uses PCLK even when AZRUN is low
- Either enabled DAC channel with TEN, DMAEN or nonzero wave mode: reject

Static LVD/VC/OPA/DAC operation and shared BGR/TS/reference settings are preserved.
Other timers, serial devices, DMA, external devices and asynchronous handlers
must be quiescent under the entry contract. This is not an arbitrary live
bootloader takeover API.

## Transition and failure contract

Incoming clocks, buses, supply, trim and Flash latency must already be legal and
stable. RM §4.4.2, printed33/PDF59, defines a safe HSIOSC operating range of
90–100 MHz and requires user calibration for nonfactory frequencies. That is a
caller precondition for incoming nonfactory trim whenever HSI is enabled,
including when HSI was initially disabled. It is not a guarantee for every TRIM
code and not a measurement the HAL can make. The initializer cannot repair an
illegal clock before its first instruction executes.

After preflight, Flash WAIT is raised to3 without altering FETCH, CACHE,
CACHEINVALID or RFU. Guard prescalers are at least AHB/8 and APB/8 and preserve
larger incoming divisors, covering the own legal HSIOSC upper range even below
1.8 V. After Flash acknowledges WAIT3, unchanged HSI is enabled and polled for
STABLE. The first CR0 write installs both guarded prescalers and explicitly
selects HSI together, then verifies selector/dividers/HSIEN/STABLE/faults and
executes a barrier. It never writes back an inherited external selector while
changing the guard dividers. Factory-correct HSI is reused. When retrim is permitted, the initializer
uses unchanged stable LSI, temporarily suppresses CLKCCS, selects LSI, observes
HSI disabled and STABLE clear, installs factory trim/divider, restarts HSI,
selects HSI and restores the original security policy. A divider-only change is
allowed by the own §4.5.2 and leaves raw HSIOSC owners undisturbed.

Fresh HSE is then configured and started under verified HSI; inherited exact HSE
is untouched. Final prescalers are installed while HSI is still selected; all
requested/fallback sources have already been qualified at those divisors. Only
then is HSE selected. This prevents an external fallback from racing a prescaler
read-modify-write into reselecting a stale external source. Flash stays at maximum
WAIT until the final source/dividers acknowledge. WAIT can then be lowered to
cover the greatest actual HCLK across selected source, requested HSI and fixed
fallback, with final source/trim/divider/enable/STABLE/policy/pad/fault checks.

STABLE only reports completed startup and does not clear on later run failure.
Relevant sticky HSE/LSE FAIL/FAULT causes refusal. Finite register-poll budgets
require continuing CPU execution; a stopped CPU clock makes no elapsed-time
promise possible. Subsequent source loss or fallback invalidates frozen
external timing. Runtime retuning, sleep/resume and recovery/refreezing are
not supported. Failed RCC initialization publishes no clocks or peripheral
tokens, consumes singleton ownership and requires reset before retrying. A later
time-driver failure may occur after RCC clocks were frozen, as in existing HAL
initialization.

## Validation boundary

The new source/data verifier checks original-source hashes and table text,
selector-free typed enums, access-lowering dependency, normalized reuse history,
actual package routes and generated metadata/PAC shape. Required production ARM library
builds cover the family alias and both exact parts, with rt/defmt and selected
time-driver configurations. The real crystal/bypass examples must link for both
exact packages. These checks do not simulate or execute HAL behavior and do not
establish silicon timing, source startup or board qualification.
