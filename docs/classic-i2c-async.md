Current extension: [explicit classic ten-bit addressing](classic-i2c-ten-bit.md) adds software-generated address phases. Earlier seven-bit acceptance below does not validate this extension.

# Owned classic I2C asynchronous master

The frozen implementation has independent [runtime acceptance](classic-i2c-async-runtime-review.json)
and [source/metadata acceptance](classic-i2c-async-metadata-review.json). Main evidence
contains 13 standalone ARM libraries and 17 final linked ELFs, plus six finite
source/data commands. All 910 generated files retain their baseline bytes.
No hardware or electrical validation is claimed.

The final examples use release size optimization `z`; the standalone libraries
retain their separate firmware profile. Preserved library inputs and rebuilt
example inputs have explicit independent correspondence, not one shared full-tree
hash. Four failed attempts are retained: an unavailable debug-pin route followed
by three real 16 KiB Flash overflow links. The final F002 examples retain all four
transaction forms and real memory bounds. Earlier successful `s`-profile examples
are historical, not part of the final 17. L010/L011 rows retain three existing RCC
warning kinds. At main acceptance clean had not run; final-package completion
requires separately recorded six-library/six-ELF clean and source receipts.

## Qualified scope

The classic seven-bit master covers F030, A030, F020, F002, F003, L010, L011,
L031, R031, W031, L052 and L083, using five existing typed register versions.
Each family's own manual and original SVD/header establish its SI state machine,
dedicated interrupt and independently owned reset field. Exact package metadata
continues to select bonded SCL/SDA pins. L012 retains its separate command/FIFO
backend. Ten-bit addressing, slave operation, retries and peripheral DMA are not
provided by this API.

`I2c<Async>` owns the peripheral and both pins, together with a verified interrupt
binding. It provides ordinary `read`, `write`, `write_read`, `transaction` and the
seven-bit `embedded_hal_async::i2c::I2c` trait. Board pull-ups and qualified clock
and electrical conditions remain necessary. Async scheduling does not establish
SCL waveform quality or measured throughput.

## Waiting and transfer results

START, address and data phases wait for the controller's SI interrupt. The ISR
masks only its dedicated vector and wakes the registered task. It does not clear
SI, advance a byte, read DR, or access caller buffers. The future registers before
arming and rechecks SI to close the completion-before-sleep window.

Initial bus-idle sampling, STOP completion and cleanup use finite synchronous
sample budgets. `Config::poll_limit` is a register/line sample count, not a time
interval. STOP completion produces no SI interrupt. An SI wait has no built-in
deadline; an application may compose it with an independently working timeout,
which cannot preempt synchronous polling or destructors.

Adjacent operations of the same direction share an address phase. Direction
changes use repeated START, and the final byte of a contiguous read run is
received with NACK. Address and data NACKs, arbitration loss and unexpected states
are errors; a final received-byte NACK is successful completion of that byte.
Successful transaction return requires the requested local STOP to complete.
There is no automatic retry and earlier transferred bytes are not rolled back.

## Cancellation and ownership

The driver retains a transaction marker across a forgotten future. The ISR and
static state contain no borrowed buffer or future pointer. Cancellation returns
memory ownership safely, but may truncate a byte, leave partial target effects,
or leave a target holding a bus line. It does not guarantee bus repair or
multi-master noninterference.

Cleanup samples the live SI/status before deciding whether one STOP is allowed.
Arbitration states never authorize a new STOP. Only the documented held master
states can authorize it; SI=0, an unknown/slave state, inherited state or an old
software master flag cannot. A normal STOP failure does not cause a second STOP.
Writing STO=0 does not clear a pending STOP.

The controller is then disabled locally and its EN/BRREN/control readbacks and
STO=0 are checked within a finite sample budget. A confirmed disabled instance
enters `NeedsReinitialize`. Before a later transaction it undergoes checked
assertion and release of its own central RCC reset field, reset-state checks,
and the documented timing/configuration sequence. Neighboring reset fields are
preserved. The bus-error-specific EN toggle is not used as proof that arbitrary
cancelled internal state was reset.

Failed shutdown, reset or configuration readback sets a sticky per-instance
`Faulted` condition. New operations, configuration changes and replacement
constructors reject it until chip reset. Failed reset release permits no further
I2C register access. Faulted Drop does not write I2C or its gate; owned pin teardown
can disconnect the pins. No such error is reported as successful STOP or confirmed
physical line release.

Forgetting a pending future can leave a held controller event. The next mutating
driver entry retires that marker before starting new work. Thus even an invalid
new request may first retire a previously forgotten transaction. An ordinary idle
invalid request causes no new START. Both blocking and async replacement
constructors fence old interrupt state before pin mutation or controller setup;
static faults cannot be bypassed by forgetting and reborrowing the owner.

The existing blocking transaction engine remains blocking. Its common constructor
and lifetime fence protect replacement after a forgotten asynchronous owner.
Local cancellation and fault cleanup never delegate ownership to an ISR.

## Evidence and finite acceptance

Current hardware facts are in [the typed I2C evidence](i2c-typed-register-evidence.json).
The existing source/PAC checker validates all old families and adds current classic
IRQ/reset/protocol facts. It does not test HAL behavior or depend on a historical
production hash. Main acceptance is planned as thirteen real ARM libraries and
seventeen ordinary ELFs; a final-source clean extraction has six libraries and
six ELFs. Main uses the existing shared target with per-command artifact retention;
clean uses separate fresh generation and ARM targets. Canonical inputs are
unchanged, so main does not regenerate data/PAC; clean bootstraps them once and
compares all 910 generated files with the accepted baseline.

Main source checks and real execution reviews precede their final administrative
receipt installation. Final-source clean evidence is recorded separately. No
HAL model, synthetic error injection, lifecycle probe, hosted CI, or hardware
result is included in these claims.
