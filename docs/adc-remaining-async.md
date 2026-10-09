# Finite software ADC interrupts on L012 ADC1 and classic families

Source-qualified scope, 2026-10-09. L012 ADC1 and all ten classic lines expose
`Adc<'d, T, Async>` for single reads and one finite ordered scan. Their existing
blocking APIs remain available. L012 ADC2 stays blocking because its ADC2_DAC
vector needs a separately owned shared-interrupt lifetime and fault policy.
This extends the [L010/L011 async scope](adc-low-async.md); each backend retains
its own register, electrical, source and cancellation contract.

## API and ownership

Classic families bind their dedicated ADC interrupt:

```rust,ignore
bind_interrupts!(struct Irqs {
    ADC => adc::InterruptHandler<peripherals::ADC>;
});
let mut adc = adc::Adc::try_new_async(p.ADC, Irqs, config)?;
let sample = adc.read(&mut pin, sample_time).await?;
let timing = adc.read_sequence(&sequence, &mut output).await?;
```

L012 retains the common analog owner, and only ADC1 implements the sealed async
instance capability:

```rust,ignore
bind_interrupts!(struct Irqs {
    ADC1 => adc::InterruptHandler<peripherals::ADC1>;
});
let common = adc::Common::try_new(p.BGR, 100_000)?;
let mut adc1 = adc::Adc::try_new_async(p.ADC1, &common, Irqs, config)?;
let mut adc2 = adc::Adc::try_new(p.ADC2, &common, config)?;
```

`new_async` is the panicking convenience constructor. `read` returns raw 12-bit
counts and `read_sequence` returns `SequenceTiming`. The existing sealed channel
handles, borrowed/owned erasure and package-qualified pins are unchanged. A
sequence references existing handles, so repeated channels do not duplicate pin
ownership. Output length must equal sequence length. The future borrows the
converter, channels, sequence and output until completion or drop.

F030/A030/F020/F002/F003 have up to four slots; L031/R031/W031/L052/L083 and L012
have eight. Classic scans require one common sample time and unbuffered external
channels for more than one slot. Internal/follower-enabled sources keep their
single-read qualification. L012 allows distinct sample times and qualified
internal sources in an ordered scan. Existing voltage/reference bounds, weak
source restrictions, HSI qualification and exact rational `ClockBounds` apply.
No additional route, internal reference, calibration or electrical range follows
from enabling interrupts.

## Completion and output commit

L012 uses EOS-only interrupts for singles and scans. The future verifies EOS,
EOC and `START=false`. Classic MODE0 singles use EOC; MODE4 scans use EOS, with
OVW enabled as an error wake in both modes. Classic singles require EOC and
`START=false`; scans require EOS and `START=false`. OVW wins over apparent success.
READY remains a
bounded startup poll because it has no enable or clear field.

The handler filters active ownership and the enabled pending owned source,
masks owned completion/error enables, retains status and wakes the per-instance
upstream `AtomicWaker`. The future registers its waker before its final status
check. Retained completion covers an event that arrives before registration.
Once an event has been observed, terminal readback uses a finite synchronous
poll; the task cannot depend on a second interrupt after the sole wake was masked.
The ISR never holds or writes a caller buffer pointer.

Results are read into a local array only after terminal observations. The driver
then verifies owned masking and selective acknowledgment and commits the whole
requested output slice without an intervening await. An error or cancellation
leaves every element unchanged. With all hardware triggers disabled and a finite
software mode, delayed collection cannot be overwritten by a subsequent automatic
scan. This does not detect arbitrary raw register interference.

L012's typed ICR write starts from its no-op value and clears only EOC/EOS,
preserving AWDL/AWDH and reserved bit2 (effective value 0x1c). Classic clears
EOC/EOS/OVW while preserving EOA/WDTL/WDTH/WDTR (effective value 0x3c). ISR and
RESULT are read-only; reading a result is not acknowledgment. No classic READY
clear or invented L012 READY/OVW/BUSY field is used.

## Cancellation, inheritance and faults

Async acquisition rejects inherited triggers, watchdog monitoring/interrupts and
DMA request enables before adopting the converter. Classic also rejects EOA
interrupt and accumulation; L012 rejects synchronized local or sibling SLAVE
state. An independent L012 sibling can remain owned and operating. Rejection
preserves those inherited configurations and flags. Common or local clock-gate
acquisition needed before readable preflight is a separate explicit operation.

Every new read first establishes stopped, disabled and cleared owned state,
including after a forgotten future. A drop guard masks owned events, requests
START zero, disables owned conversion state and selectively clears owned flags
with finite readbacks. Successful cancellation permits fresh initialization.
Failure of a required mask, stop, clear or configuration readback latches
`Error::Faulted` per instance until chip reset, including replacement owners and
switching between blocking and async. Reconfiguration does not clear the latch.
Normal cancellation does not disable NVIC. Failed interrupt masking can disable
only the metadata-proven dedicated ADC line to contain a stuck source.

L012 explicitly documents START zero as stop plus sequence-cursor reset.
Classic documents stop, and separately documents MODE4 as one complete selected
sequence per start. Classic reuse is a stated inference: establish stop, disable
and reinitialize EN, reprogram the full owned mode/slot configuration, clear owned
flags, then start a fresh finite sequence. This is not an explicit classic cursor
reset guarantee. Neither backend supplies independent drain acknowledgment,
maximum analog abort latency or instantaneous analog sampling cutoff. A readback
of START is the documented control state, not an invented BUSY handshake.

Forgetting a future skips its destructor and promises no quiescence. Finite
hardware writes only peripheral result registers; no ISR/DMA pointer survives into
caller memory. The next operation repairs the peripheral explicitly.

L012 never resets/gates the ADC group or clears shared BGR/TS enables; `Common`
remains borrowed, and ADC2, comparator, OPA and DAC state is preserved. On
F030/A030/F020/F003, live BGREN and its shared ADC gate are retained. F002 reserves
BGREN/TSEN, while L031/R031/W031/L052/L083 reserve BGREN; no fabricated enable is
written. Their existing ADC reference/temperature ownership limits still apply.
Retained shared resources and unresolved fault state can retain power.

ADC2's finite conversion engine is qualified, but its IRQ30 is shared with DAC.
An ADC2-only owner cannot safely disable that vector after failed masking or
adopt inherited live DAC interrupts. The current DAC owner exposes neither a
checked quiet-peer lease nor a shared interrupt owner. This is the specific
remaining ADC2 async boundary, independent of ADC1 support.

## Deadlines and examples

`Config.timeout` counts finite synchronous poll attempts during setup, stop and
finalization. It does not bound time pending in `.await`. Applications use the
ordinary pinned `embassy_time::with_timeout` or `embassy_futures::select` with a
working time/wake source; losing futures are dropped. Progress needs running
clocks, enabled interrupts, a correct typed binding and eventual executor service.
There is no maximum IRQ, scheduling or whole-operation latency guarantee.

[The L012 examples](../examples/l012-adc-scan/README.md) include ADC1 single reads,
an eight-slot mixed-source scan, select/timeout cancellation and reuse, and an
independent blocking ADC2 neighbor. [The classic examples](../examples/classic-adc-scan/README.md)
cover all ten lines using generated package routes, full-length ordered scans,
internal supply single reads and cancellation/reuse. Their optional `async`
feature uses the existing executor/time/futures pin and family-appropriate GTIM
or GTIM1 time driver; it reserves that timer and interrupt. The separate
[run-mode time-driver contract](time-driver.md) applies.

These ordinary firmware examples are source deliverables, not HAL tests,
register models or evidence of execution. Compilation and silicon validation
must be reported separately. DMA, periodic/continuous acquisition, externally
triggered scans, synchronized ADCs, RF, deep sleep and lossless streaming remain
outside this scope.

## Own-source evidence

The [evidence record](adc-remaining-async-evidence.json) cites only existing
canonical manual/header source IDs. Exact URLs, revisions, hashes and archive
chains remain in [the single source authority](../sources/evidence-sources.json).
No new vendor artifact or SDK example is a qualification dependency. A030 uses
the explicitly shared CW32x030 manual and existing F030 register-source alias;
there is no claimed separate A030 SDK.

Page numbers are printed / one-based PDF pages. Classic IRQ tables are §5.4;
each own manual and CMSIS header independently identifies dedicated ADC IRQ12.

| Family | Slots | IRQ | MODE0 single | MODE4 scan | START/IER | ISR | ICR |
| --- | ---: | --- | --- | --- | --- | --- | --- |
| F030/A030 | 4 | 95/96 | 439/440 | 446/447 | 466/467 | 467/468 | 468/469 |
| F020 | 4 | 93/94 | 377/378 | 384/385 | 404/405 | 405/406 | 406/407 |
| F002 | 4 | 71/72 | 306/307 | 313/314 | 331/332 | 332/333 | 333/334 |
| F003 | 4 | 73/74 | 366/367 | 373/374 | 393/394 | 394/395 | 395/396 |
| L031 | 8 | 90/91 | 436/437 | 443/444 | 465/466 | 466/467 | 467/468 |
| R031 | 8 | 92/93 | 439/440 | 446/447 | 467/468 | 468/469 | 469/470 |
| W031 | 8 | 91/92 | 439/440 | 446/447 | 468/469 | 469/470 | 470/471 |
| L052 | 8 | 95/96 | 473/474 | 480/481 | 505/506 | 506/507 | 507/508 |
| L083 | 8 | 104/105 | 478/479 | 485/486 | 507/508 | 508/509 | 509/510 |

L012 CN1.4: IRQ tables70–71/96–97; finite conversion §25.5.2 at581–582/607–608;
stop/reset §25.5.1 at579/605 and START §25.12.2 at590/616; interrupt table
§25.10 at587/613; IER596/622 and ISR/ICR597/623; results598–599/624–625;
shared BGR599/625; DAC shared interrupt sources607/633.

F002's manual calls TRIGGER bit3 PA02 while its SDK/PAC calls it PA32. The bit
location and disable polarity agree, which suffices for software-only rejection;
no new GPIO trigger route is inferred. L031/R031/W031 SPI/I2C versus SPI1/I2C1
labels likewise retain their recorded alias scope. Existing voltage, sample-time
and reference conflicts remain in the electrical and scan source documents.

The existing Embassy pin `f16efeffe37581092ec184718e6fdb1620393214` supplies the
waker, binding, guard and caller-composition structure. It supplies no CW32
hardware or electrical evidence.
