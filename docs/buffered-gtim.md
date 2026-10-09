> Historical software-verification references: HAL tests and fixture harnesses cited below were deleted on 2026-10-08. Their reported results apply to the dated source snapshot only; source facts remain unchanged. See [current HAL layout and build scope](hal-production-layout.md).

# L010/L011/L012 buffered GTIM counters and PWM

Source/software qualification, 2026-10-08. No firmware has been executed on a
microcontroller; electrical waveform behavior remains unvalidated.

## Public coverage and upstream shape

`timer::low_level::Timer<T: BasicInstance>` and
`timer::simple_pwm::SimplePwm<T: GeneralInstance4Channel>` now support L010's
`GTIM1`, L011's `GTIM1/2` and L012's `GTIM1/2/3/4`. L010's manual calls the sole
instance GTIM while its own SDK/CMSIS and existing PAC call it GTIM1 at
0x40001800. No duplicate ownership token or fabricated alias is introduced.

The actual pinned Embassy-stm32 timer sources were inspected, including
`PwmPin`, `SimplePwm`, channel methods, sealed instance/channel traits and
`embedded-hal` PWM. Their revision and file hashes are in
[buffered-gtim-evidence.json](buffered-gtim-evidence.json). The existing HAL's
upstream `Peri`, generated singleton tokens, typed pin routes and borrowed
channel handles remain in use. Borrowing a channel prevents dropping its owner
or changing the shared frequency. `split` intentionally borrows the owner
instead of consuming/leaking it. Module roots use `<module>/mod.rs`.

Implemented: internal-PCLK continuous, edge-aligned up-counting, polling overflow,
and four simple push-pull PWM outputs. Counter construction is stopped; PWM
construction starts with all channels disabled at zero duty. Capture, encoder,
external counting, interrupt/async, DMA, one-shot, advanced timers, hardware
trigger selection, cascade routing and Embassy global time-driver support are
not exposed. BTIM does not acquire PWM capability.

## Actual register versions and clocks

Each own current manual was checked independently: L010 CN 1.2, L011 CN 1.1 and
L012 CN 1.4, plus the corresponding official SDK and datasheet. L010/L011 select
`gtim_cw32l010_v1`; L012 selects `gtim_cw32l012_v1`. Their shared buffered register
layout is not substituted with the classic GTIM map. L010/L011 IER has only IRQ
enables. The L012 manual's DIER at 0x0c is named IER by its own SDK/PAC and also
contains DMA enable bits. Each adapter clears its own actual request register.
ISR is read-only; no ISR writes are made. EGR is written with UG only, never RMW.

GTIM has independent APBEN1/APBRST1 controls: L010 bit 6, L011 bits 6/7, L012
bits 6/7/11/12. APBEN1 requires key 0x5A5A in bits 31:16; APBRST1 is unkeyed and
reset-active-low. Critical-section RMW/readback preserves neighboring timers and
peripherals. Only the owned instance is reset/gated. Shared BTIM controls and
ADC/RTC/Flash drivers are untouched.

PCLK is the kernel clock, without an APB timer multiplier. PSC stores divisor−1;
the common public power-of-two divisor subset 1…32768 is retained. Buffered GTIM
ARR=0 inhibits counting, so explicit periods are 2…65536 ticks. Frequency
selection clamps to at least two ticks and returns a rational frequency no
higher than requested; requests between PCLK/2 and PCLK therefore yield PCLK/2.
Classic GTIM and existing BTIM keep their 1…65536-tick behavior. Frequency
arithmetic uses widened integer calculations and the exact total divisor.
Invalid frequency/period requests leave driver state and registers unchanged.

## Buffering, endpoints and observable transitions

CR1 uses ARPE=1, URS=1, UDIS=0; all compare channels retain OCyPE=1. PSC/CCR
shadow values commit on an update; ARR writes while stopped already affect its
shadow even with ARPE enabled. Initialization and period changes stop the timer,
write reload/prescaler/compare state, issue UG to latch pending values and reset
CNT/prescaler phase, then restore the intended running state. A stopped counter
stays stopped. URS suppresses UG-generated update requests/UIF; overflow polling
is cleared separately with a direct R1W0 ICR write.

The clearable flag mask is 0x00F01E5F. Initialization clears defined flags with
zero; clearing UIF writes 0x00F01E5E to preserve all other pending events. Reserved
bits remain zero. Multiple overflows coalesce and are not a lossless timebase.

- Ordinary intermediate PWM duty writes remain buffered until the next update.
- Repeating the same forced/disabled mode and polarity only updates retained
  compare preload. It does not restart the timer or commit a peer's pending duty.
- Entering/leaving an endpoint/disabled mode or changing polarity quiesces the
  affected channel, stops the whole timer, writes the new CCR/polarity, issues UG,
  transitions through Freeze into the requested mode, and resumes. This resets
  every channel's phase and commits every channel's pending compare value.
- A frequency change quiesces all outputs, scales requested duty fractions,
  commits all four compare values with one UG, and begins a new period at zero.
- Disable retains requested duty and actively drives the inactive polarity;
  CCyE stays set until drop disconnects the owned pins. Drop first forces inactive,
  stops, disconnects pins, disables timer outputs/requests and gates its own clock.

Forced mode 4 is inactive, mode 5 active, mode 6 PWM1. CCyP selects active-low;
CCyNP remains clear. Zero and full duty use forced modes, so a 65536-tick full
period is never truncated into 16-bit CCR. Inherent methods use exact u32 timer
ticks. The embedded-hal u16 interface uses normalized 0…65535 scaling with both
endpoints preserved. `current_duty_cycle` reports requested duty, including
pending/disabled values.

Neither mode transitions nor frequency changes promise glitch-free or
phase-continuous output. They can truncate pulses on this timer's other channels.
No motor/power-stage safety guarantee is made; such uses need independent
protection and silicon validation.

## Package routes and electrical limits

The 62 retained own-datasheet/manual/SDK PWM cells are L010 8, L011 17 and L012 37.
L010 SOP16 and the common-package alias expose 6; its TSSOP20/QFN20 expose 8.
L011's two packages expose 17 each; L012's two packages expose 37 each, including
source-qualified AF8/9. Seven oscillator-shared alternatives are withheld, as
are debug/reset/input-only candidates. L010 PA3's SDK `GTIMCH4` spelling is
normalized only by explicit own-source identity evidence. No ETR/TRGO or
speculative trigger/cascade route is promoted.

The route proof includes original PDF cell coordinates, SDK archive membership,
exact package bonding and each family's own electrical tables:
[buffered-pwm-route-evidence.json](buffered-pwm-route-evidence.json).
Pins use the existing documented digital push-pull/no-pull AF configuration.
PCLK arithmetic does not establish pad bandwidth, rise/fall time, current or
load capability. Datasheet limits depend on VDDIO and load; design/simulation
figures are not measurement. L010's own 20 mA VOH row is 2.7 V while L011/L012's is
2.55 V, so one family's pad rating is not inherited by another. No rating is
extrapolated to undocumented VDD/load/duty conditions; the board must independently
satisfy those limits. The HAL does not measure supply voltage or output loading.

## Verification

`ci/check-buffered-gtim-hal.sh` runs original-source/register/route checks,
generator tests, all 13-family host regressions, precise ARM positive/negative
contracts for the three family aliases and seven exact parts, plus 14 actual
exact-part ELF links with/without defmt. ELF inspection verifies vector layout
and FLASH/RAM bounds; it does not execute firmware. Test results are recorded in
[buffered-gtim-verification.json](buffered-gtim-verification.json).

Production-engine host tests model preload/overflow/UG, counter state, exact
endpoints, retained disabled state, polarity, peer-channel interactions, invalid
configuration, drop order and the active-low gate/reset pulse. RAM-backed actual
PAC tests separately verify physical offsets, protected gate writes, flag masks,
all four CCRs, request disabling and untouched reserved/read-only holes. Public
channel tests exercise the actual normalized embedded-hal methods. Model tests
establish software sequencing, not physical pad edges or silicon behavior.
