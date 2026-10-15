# L010/L011 RCC module extraction

This is an organization-only change to the existing L010/L011 backend. It does
not add clock sources, chip/package qualification, oscillator ownership,
low-power transitions, recovery, RF functionality, or a new public API.

The source baseline is commit `029e90be138a5f536b363b5bf3c90deddd379646`.
Its 3,622-line `embassy-cw32/src/rcc/l010_l011.rs` is byte-identical to the
planning baseline `cd0a91e9a0070dc30f86a960c5c758c27e906efa`:
`1f40106a0ab9f60fddcb90b3e2b8a8ced96e824a3b2fa95bf6ef1d639f8bf99d`.

## Responsibilities

All module declarations remain private. The public types and functions stay at
`crate::rcc` through the existing backend re-exports.

- [mod.rs](../embassy-cw32/src/rcc/l010_l011/mod.rs), 692 lines: API types,
  frequency calculation, publication, initialization and HSE pin reservation.
- [io.rs](../embassy-cw32/src/rcc/l010_l011/io.rs), 77 lines: complete bounded
  polling, barriers, Flash latency access, external-fault and HSE-parameter checks.
- [inspection.rs](../embassy-cw32/src/rcc/l010_l011/inspection.rs), 51 lines:
  retained/native configuration-gate error mappings.
- [lsi_sysclk.rs](../embassy-cw32/src/rcc/l010_l011/lsi_sysclk.rs), 667 lines:
  immutable live-LSI identity and the complete existing LSI finishing routine.
- [lse_sysclk.rs](../embassy-cw32/src/rcc/l010_l011/lse_sysclk.rs), 334 lines:
  immutable native-LSE identity and the complete existing LSE finishing routine.
- [configure/mod.rs](../embassy-cw32/src/rcc/l010_l011/configure/mod.rs), 291
  lines: original entry observations, private carriers, the original lazy clock
  security closure, and the ordered coordinator.
- [configure/admission.rs](../embassy-cw32/src/rcc/l010_l011/configure/admission.rs),
  495 lines: retained RTC/ADC/analog admission and frozen ownership facts.
- [configure/flash_bridge.rs](../embassy-cw32/src/rcc/l010_l011/configure/flash_bridge.rs),
  604 lines: Flash authority, conservative WAIT, guarded buses, admission refresh,
  unchanged HSI enable and HSI selection.
- [configure/calibrate_hsi.rs](../embassy-cw32/src/rcc/l010_l011/configure/calibrate_hsi.rs),
  923 lines: complete optional LSI bridge, HSI retrim/restart, clock-security restore,
  and the alternative DIV-only branch. Acknowledgment loops remain intact.
- [configure/establish_hse.rs](../embassy-cw32/src/rcc/l010_l011/configure/establish_hse.rs),
  405 lines: the existing optional unowned HSE stop, parameter/pad setup and start.
- [configure/finish.rs](../embassy-cw32/src/rcc/l010_l011/configure/finish.rs), 198
  lines: temporary-LSI restoration, direct target tails, ordinary final mux/bus/
  WAIT operations and final read-only checks.

Formatting expands some previously compressed lines. No file remains above
1,000 lines; the previous approximately 2,000-line configure function is actually
split into bounded responsibilities, not moved intact to another filename.

## Transport and return contract

The ordered original configure spans are entry 709–815, admission 816–1135,
clock-security closure 1136–1155, Flash/bridge 1156–1578, calibration 1579–2255,
HSE establishment 2256–2541, and finish 2542–2702. Their order is unchanged.
The complete function-closing line belongs to the finishing span.

`Entry` is constructed only after the original entry checks and key normalization.
It contains the previously evaluated SYSCTRL handle; the original CR0, CR1, HSI,
HSE, LSI and LSE values; trim; needs-trim/live-LSI facts; and monitor flags. Helper
aliases only copy these values. No read, predicate, `default`, function call or
snapshot constructor is introduced while transporting them.

`Expected` exists only under the original
`all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk)` condition. Its seven
fields directly replace the seven named `expected_*` locals. Each field
qualification is explicitly enumerated in the inverse record. It is private
stack state, not a rollback mechanism. Mutations remain at the original
acknowledgment points, including WAIT and its mirror. Admission and finish only
borrow expectations immutably; bridge, calibration and HSE phases receive the
mutability their existing code needs. HSE phase transport exists only for the
original L010 live-LSI branch, not L011.

`Targets` exists only when either native LSE or qualified live-LSI exists. Each
field retains its own original cfg. No absent capability is represented by a
fabricated `None`. The original conditional `.then` closures still perform all
snapshot observations in the coordinator. Only Flash/bridge borrows this carrier
mutably, solely to attach the original L011 Flash authority without recapturing
clock sources.

`Admitted` transports `preserve_hse` and `needs_lsi`, plus native-LSE-only
`first_lsi_request` and `lse_admission`. Admission computes them exactly once.
Flash/bridge returns the replacement LSE admission after its original owned-WAIT
refresh; calibration uses the frozen first-request fact, not a new STABLE read.
The relevant existing admission/snapshot types and register values are Copy.
The new carriers have no Drop, heap allocation, unsafe pointer state, callback,
constructor method, or externally observable storage.

Each nonfinal helper returns `Result` and the coordinator immediately applies
`?`. A formerly enclosing `return Err(...)` now returns from that helper and
immediately escapes the coordinator. Returns inside preexisting closures still
return only from those closures. Finish is the coordinator's direct tail call,
so either native target's early `return finish_*` escapes both functions without
running another phase. The clock-security closure stays at its original seam,
captures the original config/CR1 observation/SYSCTRL handle, and is passed by
reference, never evaluated while constructing helper arguments.

A single function-scoped `allow(unused_mut)` preserves the original entry
bindings verbatim even though later mutation now resides in private carriers.
Other introduced aliases and parameters use the cfg of their actual consumers.

## Ordered effects and precedence

This ledger describes control flow, not a flattened list of unconditional reads.
The inverse proof also preserves every enclosing cfg, branch, closure, loop,
short-circuit operand and return, including repeated observations.

### Common entry and admission

1. Frequency validation precedes hardware access. The entry reads CR0, CR1, HSI,
   HSE, LSI and LSE in that order. Only a selected native target's `.then` closure
   then reads CR2, IER and MCO; live-LSI additionally reads its factory trim
   volatile value. The HSI factory volatile read remains later, after incoming
   clock/source legality checks. No snapshot is eagerly constructed.
2. Qualified L011 live-LSI entry runs `post_observation` and `admit` before
   generic readiness errors. External fault checks and HSI calibration validation
   retain their original order. Initial live-LSI verification and HSI-owner
   conflicts precede key normalization and admission.
3. Native LSE preflight precedes the first configuration-gate write. Retained
   admission inspects RTC, then ADC, then the shared VC/LVD domain. Every
   `inspect_for_init` call, its lazy closure, gate restoration, result mapping,
   post-inspection operational check and subsequent semantic conflict check
   remains whole. RTC's L011 raw PSC1 observation stays conditional. A gated
   inspection failure is not replaced by a previously buffered semantic error.
4. Retained HSE ownership and pin checks remain between RTC and ADC admission.
   The special live-LSI configuration-gate error continues to win before later
   source verification or ordinary retained-HSE errors. Comparator pad conflicts,
   LVD/PCLK filters and comparator blanking retain their existing priority.
5. `needs_lsi` and the first native-LSE LSI request are frozen from these original
   observations. Existing first-request ownership checks remain before Flash.

### Generic L010 and L011, and non-live-LSI branches

1. Flash configuration-gate enable/readback precedes Flash WAIT inspection and
   the conservative WAIT write/poll/barrier. Guarded HCLK/PCLK dividers follow,
   with their original readback and barrier.
2. Native-LSE admission, if compiled and selected, is refreshed only after the
   owned WAIT change and original non-WAIT policy checks. Unchanged HSI is then
   enabled and acknowledged, selected in CR0, acknowledged, and fenced.
3. If trim or inherited consumers require LSI, the software request and readiness
   poll occur at the original point. If trim changes, clock security is disabled;
   CR0 selects LSI and fences; HSI is stopped; factory TRIM and requested DIV are
   written; HSI restarts; CR0 returns to HSI and fences; original clock-security
   policy is restored. Otherwise only the existing DIV write occurs.
4. The final legacy HSI poll reads HSI first, then short-circuits through trim,
   divider, readiness, CR1 HSIEN, `ccs_unchanged(false)` and CR0 HSI selection.
   The closure's CR1 read is not hoisted or evaluated on a failed earlier operand.
5. Optional unowned HSE stop, stop acknowledgment, pad/parameter setup, restart
   and readiness checks retain their full existing order. Finish restores only
   temporary software LSIEN and does not require automatic LSI STABLE to clear.
6. Ordinary finish selects the requested source before final bus dividers,
   preserving both readback/barrier pairs. Final Flash WAIT follows. Fault checks
   precede final CR0/CR1/HSI/HSE/LSI/LSE observations; final predicates retain
   left-to-right short-circuit order, including `ccs_unchanged(true)`.
   HSE parameters/pads and optional LSE verification precede the final fault and
   mux checks. Only the unchanged root `init` publishes the successful Clocks.

### Qualified L010 live factory-LSI

- Each original live-LSI acknowledgment loop begins with retained identity,
  followed by CR0, CR1, HSI and HSE observations. Conditional WAIT reads observe
  authoritative Flash WAIT before the route mirror. Key normalization and the
  existing source/parameter/readiness comparisons retain their original order.
- Where present, L010 LSI readiness rejection remains before clock identity
  rejection. Each original acknowledgment field can only progress from its
  admitted prior value to its target; no reversal, skipped poll, shared-loop
  abstraction or reordered field comparison is introduced.
- Expected WAIT/mirror, guarded divisors, HSI request/readiness, system selector,
  calibration parameters and security state update only after the original
  acknowledgment loop. HSE expectation mutations remain L010-only.
- The qualified live-LSI branch does not start or program LSI, does not restore
  temporary software LSIEN, and directly returns through the unchanged
  `finish_lsi_sysclk`. That routine's final selector/readback/fault windows remain
  intact in its own module.

### Qualified L011 live factory-LSI

- L011 post-observation checks retain their priority before generic entry errors
  and after each relevant complete acknowledgment observation. FLASH access
  failure remains buffered until source/fault verification completes.
- The first admitted FLASH observation attaches only the existing Flash authority.
  Guarded read/write/readback helpers remain whole, including gate/access checks.
- The WAIT transition keeps `flash_write.and(current_wait)?` in that order, then
  the authoritative/mirror equality check, then individual acknowledgment checks.
  It is not rewritten as `current_wait.and(flash_write)` or eager error handling.
- In the original loops, L011 LSI readiness rejection stays after the clock
  identity check, unlike the corresponding L010 branch. WAIT errors retain their
  later position. No L010 HSE transition logic is enabled for L011.
- The final existing LSI routine retains its L011-only constraints and guarded
  observations. Its early return bypasses all ordinary/LSE finish operations.

### Native LSE, qualified packages only

- Original target ownership and monitor prerequisites remain immutable. The
  frozen first-LSI classification is reused at the later request edge, even if
  STABLE has progressed. RTC/UART/LPTIM/analog inspection closures and their
  restore-before-conflict precedence stay within the existing methods.
- LSE start consumes either the narrowly refreshed native-target admission or the
  existing owned-WAIT-adjusted admission for a non-LSE system target.
- Selected LSE directly returns through the unchanged `finish_lse_sysclk`.
  Calibrated HSI carries the final bus dividers and conservative WAIT before
  the last CR0 mutation selects LSE. Subsequent fault, Flash and pad checks do
  not switch the selector back.

### Poll budgets and barriers

The unchanged `poll` and `poll_clock` routines iterate exactly `0..timeout`.
A zero timeout executes no predicate; `poll_clock` checks faults before each
predicate. Success returns before spin; exhausted iterations return the same
error. Specialized acknowledgment loops retain their exact first/last-iteration
checks, break conditions and spin placement. Helpers do not add polls or barriers.
The extraction preserves software iteration budgets and MMIO ordering, not
machine-code identity or a physical timing measurement.

## Reproducible inverse proof

Run:

```sh
python3 tests/verify_l010_rcc_split.py --self-test
```

The [pinned relocation record](l010-rcc-module-relocation.json) contains all 11
file hashes, exact scaffold text, source-line ownership, restricted per-line
rewrites and finite formatter inverse hunks. The verifier first requires the
exact module roster and manifest digest. It reverses only those precise hunks,
removes only the listed scaffolding, approves each field/ancestry/visibility edit,
and requires every original line exactly once. The reconstructed UTF-8 bytes
must have the original source SHA-256. No arbitrary expression normalization,
field-access erasure, unknown-token skipping or ignored source item is allowed.

Formatting is not described as whitespace-only: rustfmt adds trailing commas,
changes punctuation jointness next to one added comma, and wraps exactly two
existing inspection match-arm tail expressions in blocks. Those finite changes
are explicitly reversed. Original helper bodies, comments and whitespace are
recovered exactly, as are the configure branch/closure/return tokens.

Fourteen negative test classes cover read reordering, moved L011 observation,
eager closure evaluation, lost gate restoration, reversed Flash error priority,
wrong WAIT mirror, recomputed first-LSI classification, lost native return,
removed barrier, cfg widening, extra module, broadened visibility, duplicated
item and missing item. Negative runs disable candidate-file/raw hash checks so
that the exact inverse and original-source oracle must reject the change, rather
than passing through an opaque checksum-only mutation test.

The proof is a refactor acceptance record, not a claim that future implementation
changes should be hidden by regenerating it. A later behavior change needs its own
review and evidence. It proves preservation of the reviewed source behavior, not
that the historical behavior itself is hardware-qualified beyond its existing
scope.

## Validation boundary

Author checks passed on the frozen tree: exact inverse plus 14 negative classes;
targeted rustfmt; generic L010/L011 and all five exact packages with `rt` and
`rt,defmt`, 14 ARM builds using the unchanged baseline firmware lock. Exact-package
builds retain three preexisting generated unused-constant warnings. No new source
warnings remain. An initial build setup unnecessarily regenerated dependencies;
those exploratory results are explicitly excluded, the original lock was
restored, and all 14 author builds were repeated successfully.

Full matrix, linked examples, host/API tests, rustdoc, source qualification,
generation parity and packaging are separately recorded in
[l010-rcc-module-validation.md](l010-rcc-module-validation.md). No hardware test,
Miri run, publication or remote CI result is implied by this extraction proof.
