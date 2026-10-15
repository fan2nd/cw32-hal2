# L010/L011 RCC module-split validation

Executed on 2026-10-15 (UTC), against source baseline
`029e90be138a5f536b363b5bf3c90deddd379646`. This receipt concerns only the
behavior-preserving split of the 3,622-line L010/L011 RCC backend into eleven
private responsibility modules rooted at `mod.rs`. It adds no clock source,
package qualification, low-power/RF support, recovery behavior, or public API.
See [the layout and ordered-effects ledger](l010-rcc-module-layout.md),
[the pinned source inverse](l010-rcc-module-relocation.json), and
[the capability relocation](l010-rcc-capability-relocation.json).

## Frozen inputs and source correspondence

The independent run froze 1,956 source, manifest/lock, policy, test and generated
input files. Every accepted command checked that freeze before and after its
execution. Rust/Cargo 1.99.0, Python 3.12.14 and `thumbv6m-none-eabi` were used.
No prior batch's successful outcome was substituted for an executed command.

The isolated checkout generated its own PAC/data from 390 generator inputs
verified byte-identical to the recorded Git baseline. Its roster has all
54 existing selections, including W031, and 910 generated files. No generated
output from a different worktree or a different hardware roster was copied in.
Official acquired source bytes were read from the existing pinned cache.
Acceptance is bound to this snapshot; later roster changes need their own
integration verification.

All eleven source files invert to the complete original 3,622 lines and original
SHA-256 `1f40106a0ab9f60fddcb90b3e2b8a8ced96e824a3b2fa95bf6ef1d639f8bf99d`.
The packaged verifier rejects fourteen dangerous mutation classes without relying
on candidate-file hash rejection. A separate independent comparator passed
23 mutation classes and reviewed transport, cfg, closure laziness, return targets,
MMIO order and error precedence. Its independently enumerated formatter changes
were 77 trailing commas and two inspection match-arm tail blocks. This is not
an assertion of machine-code identity or measured oscillator timing.

The root is now 692 lines, the coordinator 291, and the largest intact phase
923. No resulting source file exceeds 1,000 lines. The shared RCC root,
generated-hook build script, other backends, native LSE implementation and
peripheral inspection implementation remain unchanged.

## ARM compilation, firmware links and documentation

- All 113 standard release-library commands passed: 54 selections with `rt`,
  the same 54 with `rt,defmt`, and the five existing GTIM1 selections. Commands
  match the matrix in `ci/check-hal.sh --matrix-only`.
- Three additional release-library commands passed, completing GTIM1 coverage
  for all five exact L010/L011 packages.
- All 57 affected firmware-link commands passed. These are Cargo command counts,
  not distinct chip or ELF counts. Six ordinary example groups cover all five
  exact packages: HSE, native LSE calendar, LSE SYSCLK, live factory-LSI SYSCLK,
  RTC calendar and RTC alarms. Additional commands cover retained HSE, HSE time,
  raw-HSI calendar/alarms, native-LSI calendar, and default-HSI Embassy time.
- Seven normal `cargo doc --no-deps` configurations passed: the two generic
  aliases and all five exact packages. Existing public RCC paths were inspected.

Fourteen current linked output files were additionally checked as ARM Thumb
ELF32 executables with vector-table and text sections. Earlier variants share
output filenames and were overwritten by later successful link commands;
those fourteen files are not a substitute for the 57 command receipts.
No firmware was flashed or executed.

## API contracts and actual-source host checks

All 101 external ARM API expectations passed: twelve positive and 89 negative
cases. They use exact-feature compiled HAL artifacts from the independent frozen
matrix, identified by Cargo fingerprints and hashes, without changing dependency
resolution. Expected failures require the specific diagnostic-code set and the
intended named item, not merely a nonzero exit.

The controls cover preserved public configuration/source/clock/error paths,
family-specific HSI divider variants, private modules/fields/helpers, non-Clone
clock capabilities, consumed SYSCTRL/pin ownership, and qualified LSI/LSE APIs.
Generic L010/L011 remain without qualified LSI/LSE SYSCLK variants, clock owners,
or LSE configuration fields. PLL remains absent.

All 48 host tests passed: 22 L010 and 26 L011. Thirty-six unique production items
were baseline-compared and instantiated for both families. They cover divider
encodings, frequency/operating bounds, guarded poll order and iteration budgets,
inspection-error mapping, live-LSI identity, family-specific error precedence,
and buffered Flash errors. The tests use actual generated PAC register/value
encodings and hash-bound generated constants.

The register bank, Flash access state and observation trace are explicitly
modeled. These are selected-production-body tests, not a full init/configure
state-machine execution, real MMIO test, interrupt test, or electrical proof.

## Source, generation and aggregate gates

All sixteen targeted source-quality commands passed. These include clock
contracts, RCC-control metadata, operating envelopes, layout/lint, scoped rustfmt,
source inverse/mutations, capability inverse/mutations and provenance. The
provenance suite passed 53 tests; transactional generation passed ten tests.
Both own-source qualification checkers passed their historical LSE and exact
live factory-LSI policies while validating all 54 generated selections.

Hardware-source provenance verified 43 required originals and 565 required
source files, plus two acquisition-metadata files. The two historical website
HTML snapshots remain explicitly outside the hardware scope. No canonical source
pin, qualification policy, immutable review receipt or gate was weakened.
Functional coverage remains thirteen families, 458 functions and 2,430 rows.

`./d test` and `./d check` passed in full. The latter regenerated data/PAC into a
temporary directory, required byte equality, rejected zero and multiple chip
selection, and exercised the complete 54-selection PAC metadata roster.
The final `./d gen-all` also passed; every one of the 910 generated files remained
byte-identical to the independently captured baseline. Final provenance and
`git diff --check` passed.

Exactly three live capability rows become 23 ordered references/anchors. The
two whole-file rows each cover all eleven modules, and the HSE type anchor stays
at the backend root. Reversing only this finite migration recovers both the
complete parsed baseline object and the exact baseline YAML bytes. Twenty-one
negative capability mutations were rejected; 23 historical/gate inputs were
verified unchanged. Historical source paths and hashes retain their historical
meaning and were not relabeled or recertified.

## Distribution and retained attempts

The source-only archive was built from an exact hash-verified clean staging tree:
tracked source plus the explicit eleven-module and new evidence/verifier
whitelist. It contains all new modules and required historical review records,
not the removed flat backend, acquired vendor bytes, local scratch directories,
compiled executables or generated PAC/data. The standalone inverse and all
fourteen mutation classes also passed inside that staging tree without Git
history or a compiler.

The initial generator attempt lacked the relative acquired-source path. The
supported pinned-cache path was supplied and generation then completed in the
isolated checkout. An initial development setup also unnecessarily regenerated
the firmware lock. Those exploratory builds were excluded; the exact baseline
lock was restored, all fourteen author checks repeated, and the independent full
matrix used that unchanged lock. No dependency upgrade is part of this change.
Both preliminary attempts remain separate from final acceptance receipts.

Completed build caches were reclaimed only after their processes ended and
logs, input/artifact identities and receipts were verified and retained. The authored source and generated data/PAC in both worktrees were not removed
or altered by that cleanup.
Per-command logs and scratch harness products remain local build outputs rather
than distributable source.

## Limits

Normal rustdoc retains five existing numeric-bracket link warnings at
`flash.rs:133`, which is byte-identical to baseline. No relocated RCC source
emitted a broken-link warning. Strict whole-crate rustdoc was not run, and no
warning-free-build claim is made. New RCC files pass scoped rustfmt; this is not
a claim about formatting every historical source file.

This is a pass of the declared RCC acceptance suite, not every repository gate.
The broader `./d audit-current`, `tests/test_discovery_scope.py` and
`tests/verify_dma_owned_evidence.py` were not run in this isolated RCC validation.
Their unrelated discovery/evidence maintenance is outside this split; no result
for them is inferred from the gates above.

There is no hardware, oscillator startup/loss timing, electrical, board, Miri,
or remote-CI acceptance claim. Source equivalence preserves the existing reviewed
behavior and its limits; it does not independently prove the historical hardware
behavior. This receipt does not itself publish or authorize a commit.
