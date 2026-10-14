# LCD clock/blink and L012 WWDT warning validation

Date: 2026-10-14 (UTC). Baseline: `c1ae33f01078d1f6058377ca8d6fd0011e7c1050`.
Scope: retained LSE source ownership and blink/event APIs for L052/L083 LCD,
and irreversible L012 WWDT early-warning notification service. This is software
validation, not hardware qualification or a watchdog response-time guarantee.

## Environment and ARM builds

Reused official Rust 1.99.0, thumbv6m-none-eabi, pinned SDK/SVD inputs and generated
PAC. All 113 library build commands passed: 54 chip selections with runtime, the
same 54 with runtime plus defmt, and five qualified GTIM1 time-driver selections.
Its first run
completed 51 commands before execution was interrupted during command 52; the
remaining 62 commands completed successfully in a separate resumed run against
identical source hashes. Existing generated RCC_LSE_SYSCLK_* unused-constant
warnings remain; this is not a warning-free claim.
The incomplete attempt is not counted as a passing command.

All 18 affected release example build commands passed:

| Group | Exact chip selections | Bins per selection |
|---|---:|---:|
| LCD: blocking, shared AUTOTRIM async, LSE/blink | 8 | 3 |
| AUTOTRIM shared-vector regression | 8 | 2 |
| L012 WWDT warning | 2 | 1 |

These are 18 commands covering 42 binary/chip targets, not hardware executions.
Commands use `cargo build --locked --release --bins --target thumbv6m-none-eabi
--manifest-path examples/<group>/Cargo.toml --no-default-features --features
<chip>`. Both C8T6 and C8U6 are included for WWDT. Initial author development-profile
links exceeded flash; release links pass without enlarging memory limits. Only
release examples are accepted here. Existing LCD async callers explicitly handle
the new `Result` from frame/presentation operations.

## Actual public API compilation

All 37 expected outcomes passed against the ARM library. Negative checks match
the exact intended Rust diagnostic class, rather than accepting any build failure.

- LCD, each of L052 and L083: one positive return-type/API probe; simultaneous
  event waits and blink changes during a live wait reject with E0499; dropping
  the owner with a live wait rejects with E0505 (8 outcomes).
- LSE, each of L052 and L083: positive consuming constructor; reused consumed
  clock, oscillator pad, and SYSCTRL reject with E0382; dropping a reborrowed
  SYSCTRL owner while the lease remains live rejects with E0505 (10 outcomes).
- Generic L052/L083 aliases reject `new_with_lse` with E0599; only qualified
  exact packages expose this constructor (2 outcomes).
- WWDT: positive bound handler and wait; double wait and feeding while the
  future is live reject with E0499; dropping its owner rejects with E0505;
  absent IRQ binding rejects with E0277 (5 outcomes).
- Twelve non-L012 family selections reject the L012-only
  `WindowInterruptHandler` import with E0432 (12 outcomes). These probes do not
  name package pins or construct peripherals, avoiding incidental pin failures.

## Executed host/source tests

LCD: 13 tests extract the actual state, ISR, wait guard, asynchronous event/frame
methods, blink setter/getters, interval encoder and owner Drop into mocked MMIO.
They cover unpolled/cancelled/forgotten waits, restart and owner retirement,
event-before-registration and hardware-before-handler races, waker replacement,
inactive handling, all intervals 1..=64, invalid 0/65/255, preservation of modeled
unrelated fields/shared state, frame rejection without changes, and setter
retirement versus invalid-setter preservation. Multiple callbacks coalesce.
Constructor/LSE verification and sequencing, blocking waits, and presentation RAM
ordering are source-reviewed and compiled, not executed by this host harness.
The mock owner has no real retained LSE field or oscillator to release.

WWDT: 10 grouped scenarios extract the actual warning module and owner Drop.
They cover disabled/not-running/reset checks, failed IE readback with permanent
service retained, repeated enable, pending cancellation, orphan-event coalescing,
forgotten-future retirement, live waking and consumption, and owner Drop leaving
IE, gate and watchdog state unchanged while ISR service continues. RawWaker
clone/drop/wake callbacks assert they run outside modeled critical sections.
The public owner's startup/state guards and `try_unleash` are source-reviewed
and compiled, not executed by this harness. MMIO, NVIC and counter timing are
modeled; no test proves a real warning is delivered before reset. No automatic
feed or post-Drop shutdown is promised.

Total: 23 host tests/grouped scenarios. No Miri run and no hardware execution are
claimed. Prior batches' tests are not relabeled as validation of this batch.

## Reproduction and provenance

Temporary source-extraction scripts, exact API probe source/diagnostics, command
lists and logs are kept outside the maintained source tree's test framework.
No permanent HAL test framework is introduced. After central documentation closure, ./d gen-all completed successfully, including
canonical source-lock, evidence-reference and dependency-pin gates. All 910
generated PAC/data files matched their pre-generation SHA-256 inventory; all
frozen HAL and affected example hashes also remained unchanged. git diff --check
passed. Source inventory validation reports 13 families, 458 peripheral functions,
and 2430 rows, fingerprint
`85214dcf57f8a9c02b49eff1f3277c0bf9acc5e12cc222c396adf7db87f56167`.
Publication is established separately by the containing commit.

Frozen HAL SHA-256 identities:

| Source | SHA-256 |
|---|---|
| lcd.rs | `9c11b6aa759e1e2288b428becbf9cbe1bd9f0960810ba64352ef877f5b64f403` |
| wdg/windowed.rs | `9a263ed25f40079899f698ddd896f4924db11aaf74a37486c3193e7f66c45835` |
| wdg/mod.rs | `2b1bd7e1b414a5c9587f837a86f1e29342dfddf63064d38270078c32feb89136` |
