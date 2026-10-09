# Bounded classic ATIM complementary PWM

F030 and A030 now expose `timer::complementary_pwm::ComplementaryPwm::new3` and
`try_new3`. A constructor accepts up to three optional `ComplementaryPwmPair`
values, each retaining one main `PwmPin` and one `ComplementaryPwmPin`. At least
one complete A+B pair is required. F030 TSSOP20/QFN20 and the family alias have no
CH2B route, so they can use CH1 and/or CH3 without manufacturing a CH2 pin.

The module and familiar duty/frequency/master-output method names follow the
pinned Embassy revision `f16efeffe37581092ec184718e6fdb1620393214`. CW32 classic
has three physical pairs and no independent output gate. Its complete-pair
constructor, interior-only comparisons and fixed timing are explicit CW32
restrictions. It is a direct typed PAC implementation, separately selected from
buffered L010/L011/L012 in ordinary Rust modules. No register adapter, path
attribute, emulated CCER/BDTR, fabricated signal or compatibility field is used.
The buffered implementation changes only its module-relative imports.

## Owned setup and lifecycle

The whole ATIM token and both pad tokens for every supplied pair remain owned.
Pin wrappers disconnect their AFs. The constructor rejects missing pairs,
uninitialized clocks, unsupported frequency and unrepresentable dead time before
changing ATIM or RCC. It then uses central `RCC_INFO` gate/reset ownership.

With EN and MOE clear and every owned pad disconnected, initialization selects:

- CR.COMP=1, PWM2S=1, MODE=2, CT=0, DIR=0, ONESHOT=0 and ARR preload
- PCLK prescalers /1, /2, /4, /8, /16, /32, /64 or /256, never /128
- CH1–3 compare mode, A preload and noninverted A/B polarity
- OCMxA=6 (single-point PWM1), initial CCRA=period/2 and RCR=0
- all timer/channel IRQ, DMA, ADC trigger, slave/master and optional brake,
  comparator, safety and automatic-output-enable requests disabled

One software UG commits ARR/CCRA while URS temporarily permits it. The driver
uses a newly constructed typed CR value, so self-clearing BG/TG commands cannot
replay. It acknowledges only UIF with generated `Icr::write_noop()` (524287),
retaining reserved bit1 and all other flags. No ICR/ISR read-modify-write occurs.
Owned pads then connect and EN starts the counter, while MOE remains zero.
`set_master_output_enable(true)` is the explicit final output step.

`set_duty`/`try_set_duty` accept only 0 < compare < period for supplied pairs.
They update the selected preloaded CCRA for the next UEV without changing MOE,
restarting the timer or forcing a commit of peer comparisons. `get_duty` returns
the requested value, which may still be pending. `get_max_duty` returns the
period as an exclusive upper limit. The initial reference duty is period/2;
actual pad pulse width also depends on dead time. There is no `SetDutyCycle`
trait implementation because that trait promises unsupported endpoint methods.

Global disable clears MOE and makes no electrical level claim. Drop clears MOE
before stopping EN and disconnecting all owned pads, then gates ATIM through
central RCC. There is no pair-level enable/disable, brake input, software brake,
fault/rearm, inversion, phase-preservation or endpoint API in this first scope.
No post-disable pad level, minimum pulse, glitch-free waveform, motor safety or
silicon-validation guarantee follows from these register operations.

## Dead-time clock and bounds

Counter timing is selected against RCC's maximum qualified PCLK. Periods are
2..65536 and ARR=period−1. Frequency, prescaler and dead time are fixed for the
owner's lifetime. Dead time uses TCLK=PCLK/PRS, unlike buffered ATIM's /1 PCLK
basis. For an 8-bit code c, enabled insertion is:

| Code | TCLK ticks |
|---|---|
| 0..127 | c+2 |
| 128..191 | (64+(c&63))×2+2 |
| 192..223 | (32+(c&31))×8+2 |
| 224..255 | (32+(c&31))×16+2 |

A zero request explicitly disables DTEN. For nonzero requests, the smallest
code whose conservative minimum duration meets the request is selected. Code0
means two ticks, not zero. Requests above code255's 1010 ticks are rejected;
there is no saturation or nominal-clock substitution. Existing rational
`ClockBounds.divided_by(PRS)` values are retained until conversion to durations:
minimum rounds down and maximum rounds up.

For the factory-HSI x030 PCLK envelope [23.52,24.48]MHz, a 1000ns request chooses
code23 (25 ticks, [1021,1063]ns) at /1, or code2 (4 ticks, [1307,1361]ns) at /8.
The fourth row of manual p278 prints 63.25µs at its lower end for TCLK=125ns;
the row's formula and DTR register table give 514 ticks, or 64.25µs. The code
follows the consistent formula and register table.

## Source and verification scope

Own x030 manual Rev2.5 pp253–255,268,271,277–278,295–307 supplies the control,
preload, clock and command semantics. F030 datasheet Rev1.9 pp28–29 and A030
Rev1.1 pp25–26, own package grids, manual AF cells and exact SDK GPIO macros
independently qualify nine B routes each. These remain CH1B/CH2B/CH3B in authored
YAML and generated metadata, with explicit complementary pin-role mapping.
Package projection retains the common-package intersection for family aliases.
No BK routes are promoted by this implementation.

`classic-atim-complementary-evidence.json` and
`classic-atim-complementary-route-evidence.json` preserve the exact source URLs,
SHA256 identities and PDF cells. The ICR seed is separately checked against
each own classic-family manual for the three existing PAC variants. Canonical
register YAML, import register projection and reuse identities are unchanged;
only the authored command sidecar gains the three no-op records. Numeric typed
FLTR fields retain their existing capture/compare-dependent representation;
no misleading output-only global enum is introduced.

Run `python3 ci/verify-classic-atim-complementary-data.py --sources "$CW32_SOURCES"`
after normal generation. This checks own PDF/SDK evidence, canonical and
normalized register identities, generated command methods and exact-package
routes. `ci/check-classic-atim-complementary.sh` builds ordinary ARM libraries
and real firmware ELFs for all six exact x030 packages, plus buffered regression
libraries/firmware. It does not run a HAL test harness, flash hardware, or execute
firmware. Maintained coverage marks only this bounded source-level function as
partial; classic external brake and other families remain unimplemented.
