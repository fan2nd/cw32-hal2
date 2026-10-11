# Owned bus-clock output on CW32F030C8T7

`rcc::Mco` owns the real `SYSCTRL` peripheral and PA8 through `Flex`. It is
available only on exact CW32F030C8T7 (LQFP48), at PA8 AF4, package pin 29.
Generic F030, the C8 alias, other F030 packages, A030 and F020 do not gain this
API. An absent qualification is not a claim that their silicon lacks MCO.

The immutable constructor selects already initialized HCLK or PCLK and one
of /1, /2, /8, /64, /128, /256, /512 or /1024. It changes no oscillator,
calibration, trim, SYSCLK source, HSI divider, AHB divider or APB divider.
There is no raw oscillator/PLL-output selection, runtime source/divider setter,
synthetic token, checked shutdown, token-release API, low-power restoration or
clock-fault recovery. Raw PAC `source`/`div` getters and setters retain their
existing `u8` signatures.

## Admission and board declarations

`Mco::new(sysctrl, pa8, source, prescaler, board)` consumes `Peri` values and
returns `Result<Mco, McoError>`. The board argument is explicit:

```rust
McoBoardConditions {
    operating_conditions: OperatingConditions {
        min_supply_mv: 3_000,
        max_supply_mv: 3_600,
        min_temperature_c: -20,
        max_temperature_c: 70,
    },
    max_load_pf: 50,
}
```

These are example declarations requiring actual board qualification. They are
not measured values, a qualified BOM or defaults. The voltage interval must cover
the actual PA8 I/O supply, and the total-load maximum includes routing, receiver
and attached measurement equipment. Both operating intervals must be ordered;
load must be nonzero. Zero is rejected as an incomplete declaration, not as a
silicon minimum capacitance.

The constructor reads published `try_clocks()` data. A prospective
`Config::frequencies()` result does not establish initialization. The selected
bus must have `has_cycle_timing_bounds() == true`; rate-only PLL/LSI-rooted buses
are rejected even when division makes their average rate small. Existing
HSI/HSE/LSE-rooted buses remain eligible only under their original qualified
source and initialized-clock contracts.

The supplied operating interval must fit the published source's voltage and
ambient envelope, and the MCO policy of 2.4–5.5 V and −40–105°C. Total load must
be at most 50 pF. The normal ambient policy excludes the low-power 125°C
extension. Source conditions can narrow this range, and VDDA, junction,
power/current and other datasheet limits still apply.

Frozen `Clocks` does not retain the operating declaration used at initialization.
MCO therefore checks compatibility with the selected source envelope; it cannot
recover or verify equality with the earlier board declaration. The caller must
truthfully describe the same running board, while all earlier source/init
requirements continue to hold. The examples use the same constant for both
declarations. A narrower later declaration cannot prove that actual conditions
changed or retroactively justify initialization.

Admission divides the exact source bounds by the selected metadata divisor and
requires the maximum to be at most 20 MHz, including fractional remainders.
Equality is admitted; rounded nominal or whole-hertz rates are not the criterion.
`clock_bounds()` returns that retained exact logical-clock envelope and
`clock_bounds().nominal()` gives its nominal rate. It is conditional on continued
source/board validity, not a measurement, live health check or pad timing bound.
A valid sub-hertz output may round to zero in a whole-hertz accessor. Duration
getters explicitly panic if a rounded nanosecond result exceeds `u64`; even a
valid slow clock can exceed that representation for a sufficiently large cycle
count. No public duration signature changes.

## Electrical limits do not qualify the waveform

The own datasheet Table 7-27 and Figure 7-7 label their values as design/simulation
data, not measured data. At SPEED1 and CL=50 pF, the table lists 20 MHz for
2.4≤VDDIO<2.7 V and 30 MHz for VDDIO≥2.7 V; a separate CL=30 pF row lists 50 MHz
above 2.7 V. This API uses the conservative 20 MHz policy across its voltage
range. Allowing a declared load up to 50 pF is a conservative board-design
interpretation of the CL=50 pF row, not additional measured characterization.

The vendor's fmax definition also requires loaded duty of 45–55% and
`tr + tf <= 2T/3`. The rate check, source cycle bounds and integer divisor do not
prove these conditions. The MCO divider documentation does not establish
separate high/low pulse widths for every divider. The board designer must qualify
the actual loaded waveform and receiver requirements, including duty, rise/fall,
supply tolerance, routing, VOH/VOL/current, total load and power/thermal limits.
Until then, observation with suitable equipment must not be described as a fully
qualified receiver clock.

AF attachment uses the existing push-pull, no-pull, SPEED1 GPIO behavior. It
preserves PA8's inherited DRIVER strength. Qualification includes that actual
setting; the driver does not regulate strength or shape the waveform. No 50%
duty guarantee, glitch-free first/last pulse, phase continuity or fault-tolerant
delivery is implied. Board and downstream handover are functional/electrical
requirements, not additional hidden Rust memory-safety conditions on this safe API.

## Acquisition, shared GPIOA effects and errors

All numerical/board admission precedes GPIO or MCO writes. MMIO admission and
acquisition occur in one ordinary critical section. Nested existing HAL critical
sections do not add another owner or halt physical clocks, DMA, NMI or debugger
activity; later raw interference is outside the stable-register contract.

The hardware sequence is bounded and has no polling loop or hidden retry:

1. Nonzero inherited MCO SOURCE gives `OutputAlreadyEnabled`, even if it already
   matches the request. Reserved MCO bits give `ReservedMcoState`, and GPIOA held
   in reset gives `GpioResetAsserted`. No GPIO/MCO write has occurred. An idle MCO
   need not have DIV zero.
2. `Flex` opens GPIOA's gate, clears only PA8's lock bit using the bank lock key,
   preserves other low lock bits and disconnects PA8. The inherited GPIO gate
   read barrier is not a condition-checked acknowledgement. MCO performs its own
   gate/reset, lock and disconnected-pin readback. Failure is `GpioReadback`.
3. A typed MCO read-modify-write changes only SOURCE and DIV. Their readback is
   checked before AF attachment. Failure is `McoReadback`.
4. AF attachment selects PA8 AF4, digital push-pull/no-pull/SPEED1 with output
   enabled last. The driver then checks the complete PA8 configuration plus
   gate/reset/lock and rereads MCO. A MCO mismatch has priority over a pin
   mismatch (`PinReadback`). Success retains the owners and exact divided bounds.

Disconnected state means PA8 analog/input with pulls, open-drain and all four
edge/level enables clear. Configured state means AF4, digital/output,
push-pull/no-pull/SPEED1 and all edge/level enables clear. ODR and DRIVER are
inherited; unrelated pins are not compared against a whole-bank snapshot.

Opening the whole GPIOA gate can resume other pins' sampling, filters and armed
edge/level events before PA8 is disconnected, including on a constructor failure.
Interrupts can be serviced later. GPIOA remains enabled after success, error and
Drop; closing or resetting a shared bank would not undo this progress. Only PA8
is unlocked. PA8 stays unlocked, its prior function is not restored, and existing
downstream users must tolerate disconnection, clock activity and stopping.
The driver does not clear GPIO pending flags, configure NVIC or reset filters.

Before `Flex` acquisition, an error consumes passed tokens while leaving
GPIO/MCO hardware unchanged. A failed reborrow can end and allow use of the
original tokens; moved singletons are not reconstructed or returned.
After `Flex` acquisition, error cleanup drops the local pin owner inside the
same critical section before its final observation. `GpioReadback` has not
programmed MCO and reports only `pin_disconnected_readback`. For MCO/pin errors,
cleanup attempts PA8 disconnection first, disables only SOURCE, retains DIV and
reports `source_disabled_readback` and `pin_disconnected_readback` separately.
Those booleans are observed register states. A false value is a real unacknowledged
cleanup; even true values do not establish electrical behavior or protection
against later interference. Bus faults, aborts, reset and broken internal
invariants are not recoverable constructor results with guaranteed cleanup.

## Drop, forgetting and RTC ownership

Drop attempts PA8 disconnection first, then disables only MCO SOURCE with a read
barrier. The ordinary `Flex` destructor repeats the idempotent disconnect. This
bounded best effort returns no result and cannot report an acknowledgement
failure, guarantee an output level or promise clean last pulses. Normal
post-Drop state retains DIV, AF4 and SPEED1, preserves ODR/DRIVER, leaves PA8
unlocked and GPIOA enabled, and disconnects the pad. Before AF attachment, an
error can instead retain the older AF/SPEED values. Other pins and pending
status remain inherited or naturally advanced.

`mem::forget` is permitted and can leave the output active indefinitely. Moved
owners are lost; with reborrowed tokens, Rust can later permit original-token
reuse after the forgotten borrow ends. New `Mco` construction still refuses an
active SOURCE, while ordinary later PA8 use can disconnect/reroute the pad and
leave the orphaned MCO selector active. Shutdown is not leak-proof.

Owning real SYSCTRL excludes a simultaneous live safe `CalendarClock`, `LsiClock`
or `LseClock`; `Rtc` retains its `CalendarClock`, so this is not an init-only
restriction. Non-overlapping reborrows are possible after normal Drop. A
forgotten hardware output can outlive its Rust owner and later coexist with
an RTC after reborrow recovery; type-level exclusion is not permanent hardware
exclusion. There is no SYSCTRL split. The existing RTC preservation executable
remains separate from the two MCO examples.

## Sources, projection and verification boundary

The [exact qualification](f030-mco-qualification.json) and
[authored catalog](../cw32-data/mco-qualified.yaml) bind the locked own x030 RM
CN2.5, F030 DS CN1.9 and F030 SDK RCC/GPIO sources. The RM documents offset
0x70, SOURCE bits 3:0, DIV bits 6:4 and reserved bits 31:7. The complete table
uses register codes 0–7 for /1, /2, /8, /64, /128, /256, /512 and /1024;
disabled/HCLK/PCLK source codes are 0/1/2. No hardware encoding is inferred from
a public enum ordinal.

Only C8T7 gains `clock_limits.mco` and the PA8/MCO/AF4 route among all 54
selections. Optional schema shape changes also affect generated Rust metadata
for unqualified selections; `None` does not grant a capability. Generic family
AF discovery is not promoted into qualification. The HAL validates exact
identity, owner, route, electrical/table facts and selected register geometry
before generating `mco_f030` and constants. PAC register interfaces stay intact.

[The examples](../examples/mco/README.md) choose /8 with explicit matching
init/output board declarations. Their build script uses the exact 64 KiB Flash /
8 KiB SRAM map and explicit `link.x`. `./d check-mco` is a future local recipe
declaring five actual ARM library builds and three linked firmware binaries:
exact F030 release/debug, generic F030 release, exact A030 release and F020K6U7
release libraries, plus HCLK, PCLK and existing F030 RTC preservation ELFs.
The debug library includes the existing GTIM1 time driver.

That recipe is not an execution or acceptance receipt. The separate main
verification produced five real ARM library rlibs and three ordinary firmware
ELFs: all eight Cargo builds exited successfully with no warnings, and source
bytes and modes stayed unchanged during those builds. Generation and the complete
904-to-905-file comparison passed across all 54 selections, with only the approved
exact-part MCO fact/route and optional schema/projection changes. All 19 schema
contracts passed after generation; the earlier missing-generated-directory
failure remains recorded. Eight finite source/data gates passed in separate
receipts, including all 22 metadata contracts after correcting seven existing
L012/L083 route expectation omissions; the earlier failure remains recorded.
No aggregate recipe was run.

Independent main acceptance is recorded in the accompanying reviews. At main
acceptance clean had not run;
final-package completion requires a separate clean receipt. That final
source-only package replay is one exact F030 release library and one HCLK ELF
from a fresh extraction and target directory. Main and clean counts remain
distinct; final packaging/provenance checks are separate evidence. There are no
new HAL tests, compile probes, register models, hosted CI jobs,
divisor-instantiation matrices or hardware execution. Board and loaded-waveform
qualification remain unverified.
