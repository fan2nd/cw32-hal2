# Safe init-admitted static SRAM copies

`dma::CopyChannel::new(p.DMA_CH1, Irqs)` consumes the physical singleton returned
by ordinary HAL initialization and returns an admitted capability. Its safe
`copy(source, destination)` method consumes that capability and two exclusive
`&'static mut [W]` slices. Both configuration rejection and successful completion
return the same owner type; errors and forgetting permanently retain resources.
The supported families remain F030/A030 and L083, with generated SRAM metadata.
The word type is sealed to u8/u16/u32. Separate [L083 staged UART/SPI APIs](l083-peripheral-dma.md)
qualify only their RX/TX hardware requests; unsafe borrowed hardware constructors
remain x030-only.

The existing unsafe `OwnedCopy::new(Channel, ...)` and unsafe borrowed `Transfer`
constructors remain the explicit low-level integration path. No caller-supplied
safety condition has been added to the new safe copy method or to generic HAL
initialization. This software-copy layer introduces no cancellation acknowledgment,
timeout-release mechanism or claim of hardware validation. The separate
[staged UART TX API](uart-dma-tx.md) accepts ordinary borrowed input slices
through constructor-owned private storage; it does not change CopyChannel's
software-copy contract. RX and SPI have separate staged lifecycle contracts.

## Supported platform boundary

Normal firmware enters Rust after a hardware reset, or after trustworthy
low-level runtime integration has established a clean handover: no outstanding
bus access, no armed or clock-gated DMA operation, and no pending request that
can resume when a clock is enabled. This is the platform basis for HAL startup,
not a per-copy responsibility passed to ordinary safe code. Arbitrary inherited
active DMA can already corrupt the application's stack/static memory before
`init` runs, so this API does not claim to recover that unsupported situation.

Register snapshots cannot authenticate boot history. EN=0, STATUS=0, empty flags,
zero descriptors, a closed clock gate or a sticky reset-source flag do not prove
that arbitrary preceding bus accesses drained. No such proof is claimed here.
A gated unknown transfer can resume when its clock opens. The admitted path
therefore combines the supported platform entry with one-time HAL initialization
and conservative rejection observations; it never manufactures quiescence by
clearing EN, resetting the controller or waiting an arbitrary number of cycles.

The PAC retains pinned chiptool's existing raw-access model: register methods
use safe Rust syntax but are technically unsafe hardware integration and may
violate memory safety/ownership. Its README and the HAL's `pac` reexport now say
so explicitly. Safe HAL operations are resource-safe within that boundary;
this change does not claim whole-PAC-plus-HAL soundness against arbitrary
safe-syntax raw register writes. Unsafe `steal`, raw writes, an external debugger
and low-level runtime takeover must preserve existing ownership, including after
error/forget. A feature flag or wrapper would not make arbitrary PAC writes safe.

## Startup admission and shared RCC lifetime

After clocks and any selected time driver initialize successfully, and before
peripheral tokens are returned, `dma::init` runs once under a critical section:

1. Missing generated SRAM metadata records `MissingMemoryMetadata`, without
   opening the DMA clock or reading channel registers.
2. An already-enabled shared DMA gate or asserted shared reset records
   `InheritedState`. It does not release reset, clear flags, enable a vector,
   reprogram a channel or toggle the gate.
3. Otherwise central generated `DMA::RCC_INFO` enables the clock using the same
   bounded attempt budget as clock initialization. Failure records `ClockNotReady`
   and does not inspect channel registers. A failed enable can have changed the
   gate; no retry or claim of preserved hardware state is made.
4. After enable acknowledgment, ISR and every physical channel's CSR, CNT,
   SRCADDR, DSTADDR and TRIG must equal their documented zero reset values.
   Any nonzero observation rejects the entire controller with `InheritedState`.
   The full-register check includes reserved fields and is deliberately
   conservative. It is a read, not a guessed register-clear/no-op command.
5. Only the default observation records `Available`. Nondefault rejection occurs
   after gate enable and leaves the gate on. This diagnostic is valid within
   supported reset/clean-runtime entry, not harmless active-bootloader recovery.

Admission is never rerun at channel acquisition. All outcomes are retained for
that boot. Existing `Raw` admission is never overwritten, including an unsafe
pre-init token fabrication. No DMA reset is issued, no flags are acknowledged,
and no IRQ is enabled during admission. `CopyChannel::new` changes `Available`
to `Owned` once and enables only its bound real vector; failure returns the
unused singleton and changes no hardware state.

RCC reset polarity and enable identities come from the existing central
source-qualified metadata. For both native profiles, AHBEN.DMA=1 enables the
configuration/working clock and AHBRST.DMA=0 asserts reset. Shared DMA reset and
clock-disable policy are already disabled in generated RCC controls. No safe
copy, completion, Drop, error, or last-owner action resets/gates the controller.
The public RCC surface exposes queries, not arbitrary reset/disable methods.
The newly admitted normal init path retains the DMA gate even if no copy is used;
this is a small deliberate power/lifecycle change from previous late enable.

## Capability and state transitions

The pinned Embassy singleton macro has private token fields, a crate-private
one-time `take`, and unsafe `steal`. `CopyChannel` adds the ownership purpose not
encoded by the raw token: evidence that this boot's controller was admitted and
has never passed through the raw transfer path. It has a private wrapped static
`Channel`, no Clone/Copy, no reborrow, no raw mutable access and no token-release
method. The original `Channel` remains the type-erased low-level handle.

| Event | Admission | Transfer state | Resources visible to application |
|---|---|---|---|
| Before successful HAL startup | Unavailable | Idle | No normal singleton yet |
| Qualified startup | Available | Idle | Physical singleton |
| Safe acquisition | Owned | Idle | CopyChannel |
| Rejected copy configuration | Owned | Idle | Same CopyChannel and both slices |
| Successful launch | Owned | Running | Only OwnedCopy future |
| Clean completion handback | Owned | Idle | Same CopyChannel and both slices |
| Transfer error/ambiguous completion | Owned | Finished/reserved | Only Error; no owner recovery |
| Forget future | Owned | Running or Finished/reserved | No owner recovery |
| Drop future | Owned | Waits; Idle on clean success, reserved on error | Owners discarded, never exposed |
| Raw Channel::new or consuming into_raw | Raw permanently | Existing raw lifecycle | Raw Channel only |

`CopyChannel::into_raw` is a consuming downgrade. A raw `Channel` cannot be
converted back to `CopyChannel`. Raw acquisition marks `Raw` even if performed
using a temporary token reborrow; returning the raw transfer's software phase to
Idle after TE cannot restore admission. Failed admission never turns Available
because the registers subsequently look idle. There is no safe force/retry/reset
or quarantine recovery operation.

The generic owner parameter on `OwnedCopy<W, C>`, `CopyResources<W, C>` and
`CopyConfigError<W, C>` avoids a second engine or repeated wrapper lifecycle.
The default C remains `Channel<'static>` for existing raw callers. The new safe
path uses C=`CopyChannel`. Only private construction creates the future, and
success/configuration-error outputs preserve C exactly. Public resources can be
used after handback, but do not permit constructing a running future or minting
a capability. Both actual owner types are Unpin; the Future implementation has
that ordinary bound without pretending Pin makes forgotten borrows safe.

## Memory, completion, interrupt and cancellation proof

Prelaunch validation preserves the existing rules: equal nonempty lengths up to
65535, aligned 32-bit nonwrapping addresses, entire byte ranges within the exact
selected SRAM extent and disjoint buffers. Static exclusive references express
ownership of both memory regions; no source/destination reference is exposed
while DMA may access them. Every ordinary rejection precedes hardware launch and
returns the same owners. No additional operation after launch can return an
ordinary configuration error and release them.

The common engine uses software BLOCK, REPEAT=1, equal width and both address
increments. State becomes Running before DMA enable/SOFTSRC, under the same
critical section as ISR observation and first-poll waker registration. A SeqCst
fence before enable publishes source data. Wakers are taken under the critical
section and invoked outside it.

The handler first checks that this channel is Running. It does not read or
acknowledge flags for an Idle, unclaimed, rejected or already-finished channel.
That matters when DMACH23/DMACH45 invokes multiple handlers. For owned Running
state it reads its TC/TE only; TE wins over simultaneous TC. The typed R1W0
`ICR::write_noop` seed clears only this channel's observed flags and preserves
all other channels and reserved bits. It disables this channel and interrupts
after terminal indication, fences, and publishes Finished. Disabling EN supplies
no release evidence of its own.

Only recorded TC without TE, CSR.STATUS=Complete (5) and SOFTSRC=0 permit
handback. The manuals define TC as all data correctly transferred. This supplies
the architectural successful-completion evidence; the extra readbacks reject
inconsistency conservatively. Acquire before async Ready, final SeqCst before
resource return, and Transfer Drop's fence preserve CPU/compiler ordering.
Fences do not prove another bus master drained; the documented completion does.

On TE, TC+TE or inconsistent success readback, the future forgets its internal
Transfer and resources. It never invokes the raw Transfer Drop that would reset
state to Idle. The static allocations stay live and reserved even if hardware
might later access them. A forgotten whole future likewise retains its owner,
static buffers and reservation; a late IRQ can record Finished but never frees
it. No safe destructor dependency is used to keep borrowed stack/heap memory
alive.

Drop retains the prior behavior: synchronously service flags until a terminal
completion/error, even with CPU interrupts masked. It does not clear EN first.
On clean completion it discards the recovered static owners; on error it leaks
all owners. Hardware starvation/failure may make Drop wait forever. This is a
memory-safe static-owned subset, not bounded early cancellation. The borrowed
unsafe Transfer contract still requires the caller to retain forgotten borrows
and independently prove error-time quiescence before reuse.

## Own-manual evidence

Document/text SHA-256s, exact source URLs and page hashes are in
`dma-safe-owned-copy-evidence.json`, extending the existing
`dma-owned-copy-evidence.json`. For these retained texts, printed pages equal
zero-based PDF page indices; PDF viewers showing one-based pages add one.

| Claim | CW32x030 CN V2.5 | CW32L083 CN V2.0 |
|---|---|---|
| System reset scope includes DMA; reset entry behavior | §4.1 p41 | §4.1 p46 |
| Peripheral reset restores registers/state machines/control; no independent outstanding-bus-drain statement | §4.2 p43 | §4.2 p48 |
| Shared DMA gate reset 0, enable 1 | §4.7.12 p81 | §4.7.12 p85 |
| Shared DMA reset bit reset 1, assert 0 | §4.7.15 p84 | §4.7.15 p89 |
| ISR reset 0 | §8.8.1 p137 | §8.8.1 p145 |
| CSR/TRIG reset 0, STATUS5 completion | §§8.8.3–4 p139 | §§8.8.3–4 p147 |
| SOFTSRC completion and CNT/SRCADDR reset 0 | §§8.8.4–6 p141 | §§8.8.4–6 p149 |
| DSTADDR reset 0 | §8.8.7 p141 | §8.8.7 p150 |
| Software BLOCK setup and final completion | §§8.4.1–2 pp127–128 | §§8.4.1–2 pp136–138 |
| Equal widths, count bound, REPEAT=1 | §8.5 p134 | §8.5 p142 |
| TC correctness and separate TE causes | §8.6 p135 | §8.6 p143 |
| SRAM accessibility/alignment | §2.3 p28; §§6.1–3 pp105–106 | §2.3 p32; §§6.1–3 pp114–115 |

These are each family's own manuals; shared register layout is not used as a
substitute. Exact part SRAM capacity remains generated from the already-reviewed
datasheets. No metadata, register schema or PAC adapter was introduced by this
safe layer. Pinned Embassy revision
`f16efeffe37581092ec184718e6fdb1620393214` supplies the Peri/singleton/IRQ/future
idioms; its STM32-specific cancellation sequence is not imported as CW32 proof.

## Compatibility and validation scope

The raw constructor signatures retain their defaults and unsafe obligations.
Existing raw Channel reborrows and borrowed Transfer behavior remain available.
Raw acquisition now permanently revokes future safe admission, and an unowned
shared-vector handler no longer clears stale flags. Low-level integrations must
own/silence inherited interrupt sources before raw Channel acquisition; otherwise
unmasking an IRQ with stale TCIE/TEIE flags can repeatedly interrupt. Ordinary
reset-started applications have no inherited source.

The real F030 and all five exact L083 firmware examples now use safe acquisition
and safe copies, including blocking u16, concurrent u8/u32 shared-vector futures,
comparison and recovered-owner reuse. Their build receipts record exact ARM
commands, code snapshots, exit status and linked ELF hashes outside the source
package. Existing source/data/PAC evidence validators are retained. No HAL test,
mock register bank, synthetic API compile probe, firmware execution, flash,
silicon, throughput or liveness claim is made.
