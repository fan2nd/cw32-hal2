# IRQ PIO SPI byte master

F020, F002, F003, L010, L011, L012, L031, R031, W031 and L052 provide
`Spi<'d, Async, Master>::new_async` and `try_new_async` with ordinary borrowed
peripheral/pin owners and caller buffers. This is an explicitly qualified
interrupt-assisted PIO implementation. F020/L031/L052/R031/W031 additionally
have the Stage93 [staged DMA extension](spi-dma.md), whose separate main software
verification and independent runtime and metadata/source reviews accepted the
bounded change. Its final-source clean replay had not run at main acceptance.
Their PIO constructors remain borrowed; each constructor fixes its owner's
backend. F030/A030/L083 retain their six existing DMA instances.

Historical Stage89 runtime and source/metadata reviews accepted that frozen
PIO implementation. Its main verification passed fourteen standalone ARM
libraries, nineteen linked ELFs and seven finite source/data commands. At main acceptance
the separate final-source clean replay had not run; it remains required for
final-package completion. These software results do not establish hardware
behavior. See the [verification boundary](#source-evidence-and-verification-boundary).

## Scope and API

| Family | SPI instances | Typed register version |
| --- | --- | --- |
| F020 | SPI1, SPI2 | `v1` |
| F002, F003 | SPI | `cw32f002_v1` |
| L010, L011 | SPI | `cw32l010_v1` |
| L012 | SPI1, SPI2, SPI3 | `cw32l012_v1` |
| L031, R031, W031 | SPI1 | `cw32l031_v1` |
| L052 | SPI1, SPI2 | `cw32l031_v1` |

These are fourteen family/controller pairs across ten families, 36 existing
chip selectors and 26 exact packages. Package pin availability still governs
construction; a family alias is not a fabricated exact package or memory map.
Stage93 adds seven DMA family/controller pairs on F020/L031/L052/R031/W031,
bringing the DMA cohort to eight families and thirteen pairs, with separate main
software acceptance and a final-source clean replay still unrun at that point.
`spi_async_pio` and `spi_dma` are independently qualified and coexist
on those five families. Neither qualification implies the other. F030/A030/L083
remain DMA-only; F002/F003/L010/L011/L012 remain PIO-only.

The constructors take the peripheral, SCK, MOSI, MISO, typed IRQ binding and
`Config`, in that order. `try_new_async` validates before changing peripheral
or pins and returns `ConfigError`; `new_async` follows the existing panic-on-
invalid-configuration convention. The owner retains its `Peri<'d, T>` and all
three `Flex<'d>` pins. No `'static` tokens, DMA channels or private staging
buffers are required. The binding must satisfy the instance's required handler,
including the full L012 shared handler described below.

The common lifetime-generic async methods and `SpiBus<u8>` implementation
select the backend installed by the constructor. Static PIO tokens still select
PIO through `new_async`; DMA still requires `new_with_dma` with all static
resources. No backend-switching API or borrowed DMA constructor is introduced.
The dedicated `InterruptHandler` retains its public binding shape and dispatches
to separate PIO/DMA states with active-before-MMIO guards. L012's shared PIO
handler remains PIO-only.

The five dual-capable families gain the existing DMA-gated variants in the
already `#[non_exhaustive]` enums:
`ConfigError::{DmaBuffer, DmaChannel, DmaQuarantined}` and
`Error::{Dma, DmaBusy, DmaQuarantined}`. External matches
already require a fallback; cfg-specific/in-crate exhaustive handling may need
updates. The preserved PIO signatures do not imply an unchanged variant set.

Inherent `read`, `write`, `transfer`, `transfer_in_place` and `flush` implement
`embedded_hal_async::spi::SpiBus<u8>`. `transfer` clocks the larger slice length,
zero-fills absent TX bytes and discards excess RX. `read` transmits zeroes;
`write` drains every received frame. In-place transfer takes each original TX
byte before replacing it with received data. Completed bytes can remain visible
after cancellation or error; no whole-buffer atomicity or transferred-count
result is provided.

An unpolled future does nothing. Every polled call, including an empty call,
first settles prior cancelled or forgotten work and establishes the ordinary
flush boundary. Empty calls launch no new frames. Blocking `Word` support
remains `u8`, `u16` and the existing 4–15-bit wrappers; async accepts only bytes.
There is no slave, half-duplex, hardware CS, timeout, split/free/raw conversion,
automatic retransmission or new device adapter.

The [clock bounds](spi-clock-bounds.md), sampling-delay meanings, pull
restrictions and safe package routes remain authoritative. L010/L011/L012 use
their actual typed register layouts and enable placement. No new frequency,
ADC trigger, route, package or low-power qualification follows from PIO.
The bounded Stage93 DMA extension has its own source and verification scope.

## Task work, interrupts and completion

At most one frame is outstanding. The owner retains whether it owes an RX
frame or final wire-idle confirmation; operation indices and caller slices
belong only to the future. Every DR access and caller-buffer access happens in
task context. Each async poll performs at most one DR read and one DR write,
with a fixed amount of status/enable and caller-byte work, independent of buffer
length. Preparation and abandoned-frame drains share that same DR budget.
Already-ready work beyond the budget self-wakes and returns Pending.

Each instance's static state holds lifecycle/notification/error information,
never a driver, future, pin or caller-buffer pointer. Waker registration precedes
the readiness/error check; exact wait sources are armed and then rechecked to
cover completion around registration. The ISR first checks its instance's
active guard before MMIO, observes enabled pending sources, records the first
error, masks local one-shot notification and wakes outside the state borrow.
It does not read/write DR, clear ICR, advance transfers, reset/gate a controller
or inspect an inactive peer. A stale extracted wake has no authority to access
registers or caller memory.

Only the required ready and error notifications are armed. TXE is not left
armed while waiting for RX or BUSY, and software-CS operation leaves SSF/SSR
notifications off. Success requires all requested RX frames consumed and an
error-free observation of TXE=1 and BUSY=0, with local notification masked before
the decisive check. RXNE, TXE alone, an interrupt, EN=0 and FLUSH are not wire
completion proofs.

There is no BUSY-clear interrupt. Final finishing work checks once per poll and
self-wakes while BUSY remains set or TXE clear. This is cooperative polling that
consumes CPU, not a low-power wait. There is no built-in deadline or guaranteed
finite completion if hardware or clocks fail. Bounded per-poll work does not
bound the time required by a whole operation.

## Cancellation, CS and synchronous calls

The caller owns CS. After cancelling or forgetting an operation, keep the
original device selected and successfully await `flush` before deselecting,
selecting another device, releasing the transaction or changing configuration.
One frame may remain on the wire or one received byte may need discarding. The
ISR never launches another frame. The next operation discards its predecessor's
received byte before starting its own data at index zero.

Ordinary future Drop masks only that instance's notification and clears its
waiter, preserving pending phase and any observed error. It does not spin, reset,
access DR or destructively clear flags. Forgetting a future bypasses that guard;
the retained owner phase and one-shot ISR still prevent stale caller access.
The next mutating method masks/replaces the old waiter and settles pending work.

Successful flush proves current physical idle; it does not mean that every byte
of a cancelled request was transmitted. A generic `SpiDevice` adapter that drops
CS or releases its lock as soon as its future is cancelled does not meet this
contract. No cancellation-safe adapter is supplied or qualified.

Generic synchronous blocking methods, including empty calls and `blocking_flush`,
make one bounded attempt to settle pending PIO work before their existing data
path. They can discard one pending RX frame and return `Error::PioBusy` if work
remains; `PioBusy` maps to `ErrorKind::Other`. They report pending hardware errors
through the ordinary destructive recovery path before starting new data.
`set_config` returns `ConfigError::Busy` if it cannot establish clean idle.
It preserves a pending hardware error for the next flush/transfer and makes no
recovery, ICR clear or configuration write in that case. Successful configuration
still requires all external chip selects inactive. Ordinary blocking owners have
no async phase; these guards do not add an async wait to their data methods.

## Errors, destructive teardown and reacquisition

A status snapshot selects `ModeFault` before `Overrun`, then `ChipSelectFault`,
then `Underrun`; an already-latched error takes precedence over later observations.
Errors are observed before RX copy, advancement or clear. Task-side error handling
captures the result, masks notification, retires software work and uses the
existing own-IP disabled `enable(false) -> clear() -> enable(true)` recovery.
It returns the captured error and launches no replacement frame. Earlier received
bytes remain visible; lost or failed data and target protocol state are not
repaired. A later successful flush proves current idle only.

All five typed IPs use R1W0 ICR with `Icr::write_noop()` equal to `0x000000ff` and
reserved upper bits zero. Normal RX handling reads DR only when RXNE is set and
never writes ICR. Full owned-flag/FLUSH clear is limited to disabled initialization
or explicit destructive error recovery. It is never a flush-completion shortcut;
there is no ICR read-modify-write or imported byte-width MMIO rule. DR remains a
32-bit typed access with its existing 16-bit payload field.

Driver Drop deactivates and masks the instance under the lifecycle lock before
disabling SPI, gating its clock and disconnecting owned pins. It does not wait
for BUSY/RXNE, retain/leak pins or claim graceful wire completion. An unfinished
frame can be truncated. Use successful flush before intentional graceful teardown;
after destructive Drop or recovery, application-level target resynchronization
may be necessary. These rules do not change the separate DMA pair-retention and
quarantine contract.

Both blocking and async constructors on these ten families fence a forgotten
older async owner's static state after pure preflight and before pin mutation or
reset/configuration. Failed preflight leaves the old owner untouched. A successful
replacement masks old sources only if the active guard proves the clock is live,
retires old state, then resets/configures only its own controller. A blocking
replacement leaves ISR dispatch inactive. Reconstruction deliberately abandons
old physical work; it does not prove its completion or recover the target.
Thus the common blocking constructor/Drop lifecycle paths change, even though
ordinary blocking data/error/divider behavior is retained. The existing gate
helper can sample up to 100,000 times during construction/teardown; that finite
cost is distinct from the async poll budget. No new unlimited reset/idle loop
is introduced.

## L012 SPI23 and W031 ownership

L012 SPI2 and SPI3 share the actual `SPI23` vector. Both constructors require
`SharedInterruptHandler<interrupt::typelevel::SPI23>` through their required
instance handler. The binding must dispatch both real instances; a binding that
services only one member does not satisfy either shared constructor.

Construction, cancellation, reconfiguration and Drop never disable or unpend
SPI23. Each operation masks and changes only its own register bank; an inactive,
unconstructed, dropped or blocking peer is skipped before any MMIO. SPI2 can
coexist with async or blocking SPI3, and vice versa. The shared line stays enabled
after first use, including after the last HAL owner is dropped. External code
must leave an unowned peer's sources quiescent or provide compatible handling;
the HAL cannot inspect or reset a possibly gated peer to enforce that condition.

W031's internal RF host uses the same SPI1 controller and GPIOB functions. Its
own datasheet says external SPI is unavailable during RF use. Safe external pad
selection does not create independent RF ownership. The existing
[W031 SPI1/GPIOB boundary](qualified-w031-lsi-sysclk.md) applies, including possible
whole-GPIOB effects during existing clock inspection. This API adds no RF access,
reset, inspection, arbitration, coexistence, continuity or internal-pad entitlement.

## Source evidence and verification boundary

Stage93 coexistence has separate main software acceptance. Its final-source
clean replay had not run at main acceptance. These receipts remain separate
from the Stage89 results below; those historical reviews, source identities
and counts do not accept the current coexistence change.
L012 SPI DMA remains unqualified and L012 UART RX remains blocked on its
separate RC/ORE proof. Neither qualification is reopened here.

[Own-family evidence](spi-async-pio-evidence.json) records the pinned source
identities and status, interrupt, enable, error, ICR/DR and W031 ownership
locators. Those facts support the hardware commands. Independent actual
[runtime review](spi-async-pio-runtime-review.json) and
[source/metadata review](spi-async-pio-metadata-review.json) accepted the frozen
implementation without blocking findings; their identities and scope remain
separate from build receipts. Historical SPI proofs and build/test counts remain
evidence for their dated snapshots only.

Main validation passed fourteen standalone ARM libraries and nineteen real linked
firmware ELFs, with all 33 commands exiting zero. The 1,447-file authored source
snapshot, toolchain and Cargo configuration remained unchanged during that
matrix. These seven existing finite source/data commands also passed:

1. `python3 tests/verify_spi_clock_bounds.py`
2. `python3 tests/verify_spi_register_writes.py`
3. `python3 cw32-data/tools/source_provenance.py --root . --write`
4. `python3 cw32-data/tools/source_provenance.py --root . --sources "$CW32_SOURCES"`
5. `python3 ci/update-functional-coverage.py`
6. `python3 ci/update-functional-coverage.py --check`
7. `python3 tests/test_module_layout.py`

Main validation ran no canonical generation, and all 910 frozen generated files
remained byte-identical. The only recorded warnings were the same three existing
unused generated RCC constants: `RCC_LSE_SYSCLK_LSE_EDGES`,
`RCC_LSE_SYSCLK_LSI_CYCLES` and `RCC_LSE_SYSCLK_MARGIN_LSE_EDGES`. Each appeared
in the L010F8P6 standalone library, async example and blocking example rows and
the L011K8T6 standalone library and async example rows: three warnings per row,
fifteen total. The remaining twelve standalone library rows and sixteen ELF rows
recorded no warnings. There were no failed build rows.

At main acceptance clean had not run. Final-package completion requires a
separate final-source replay of two standalone ARM libraries and five real ELFs.
Its one offline `./d gen-all` bootstrap replaces command 3 above, followed by
commands 1, 2, 4, 5, 6 and 7, for seven commands total including generation. All
910 regenerated files must match before clean ARM builds. That clean receipt
must identify the final exported source; main acceptance does not substitute for
it or retrospectively claim that it had already run.

The ordinary [SPI examples](../examples/spi-async/README.md) supplied exact-package
async use and blocking compatibility; the existing DMA examples supplied
regression links. No new aggregate check, CI matrix, HAL test, synthetic lifecycle
probe, host model, firmware execution, hardware timing, low-power or RF validation
is part of this scope.
