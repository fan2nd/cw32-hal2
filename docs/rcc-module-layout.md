# RCC backend module organization

This structural refactor starts from commit
`a0df1333fb9aed61049f550cc17026b6a0d300a6`. The L012 and shared L052/L083
backends now use directory modules rooted in `mod.rs`. Public RCC paths,
generated hooks, hardware qualification and clock-transition behavior remain
unchanged. The source correspondence is recorded in
[`rcc-module-relocation.json`](rcc-module-relocation.json).

## Responsibility boundaries

Paths below are relative to `embassy-cw32/src/rcc/`. Line counts identify the
accepted split snapshot, not a permanent limit or an architectural target.

| Backend file | Lines | Responsibility |
| --- | ---: | --- |
| `l012/mod.rs` | 630 | Public configuration, clock/error types, initialization and publication |
| `l012/io.rs` | 87 | Guarded polling, barriers, flash latency and source checks |
| `l012/configure.rs` | 1185 | Complete ordered configuration transaction and its local macros |
| `l012/inspection.rs` | 103 | Retained-clock inspections and inspection-error translation |
| `l012/lse_sysclk.rs` | 538 | LSE-target snapshots, admission, ownership and completion |
| `l012/lsi_sysclk.rs` | 403 | LSI-target phase identity, acknowledgments, snapshots and completion |
| `l052_l083/mod.rs` | 964 | Public configuration, clock/error types, initialization and monitor forwarding |
| `l052_l083/io.rs` | 138 | Polling, barriers, flash latency and HSE/PLL verification |
| `l052_l083/hsi.rs` | 209 | Existing HSI configuration and clock switching |
| `l052_l083/hse.rs` | 616 | Existing HSE configuration and LSE-monitor preparation |
| `l052_l083/lsi/mod.rs` | 233 | LSI state/identity/phase types and inspection-error translation |
| `l052_l083/lsi/state.rs` | 791 | State capture, identity and transition admission |
| `l052_l083/lsi/observe.rs` | 390 | Observation, ownership, guarded waiting and source/latency checks |
| `l052_l083/lsi/transition.rs` | 553 | Complete ordered LSI configuration transaction |

The pre-existing `l052_l083/lse_sysclk.rs` and
`l052_l083/l083_lse_sysclk.rs` leaves remain byte-identical. They are not new
pieces of the former flat file. The L010/L011 and L031/R031/W031 backend
sources are also unchanged.

L012's long `configure` function deliberately remains intact. Its ordering,
local macros, short-circuit errors and rollback paths form one transaction;
extracting arbitrary chunks solely to meet a line limit would not improve its
responsibility boundary. L052/L083's LSI methods move as complete methods,
with the state type in their common parent and the transition function intact.

## API, cfg and visibility

The public declarations remain in each backend root. Existing `rcc` exports,
initialization entry points, generated configuration constants, peripheral
inspection hooks and clock publication keep their original paths. No build
script or generated-hook input changes are part of the split.

New child modules are private. Necessary access uses restricted visibility,
not new public API. In particular, the nested LSI transition's
`configure_lsi` is visible only within `crate::rcc::l052_l083`; restricted
reexports retain the backend call path. The existing crate-visible
`lsi_inspection_error` and L012 inspection hook keep their original visibility.

The shared `hse` module uses `any(rcc_hse, rcc_lse)` so moving
`prepare_lse_monitor` does not accidentally strengthen its original cfg.
Original item cfg predicates and enclosing impl context remain effective.

## Mechanical correspondence

The relocation manifest embeds reversible maps for all 2,905 original L012
lines and all 3,839 original L052/L083 lines. Each original line is covered
exactly once. Reversing explicitly listed module wiring, ancestry/visibility
changes and whitespace-only formatting reproduces both original file hashes.
Three listed optional trailing signature commas are accounted for in the
L052/L083 map. Complete item/method anchors and final destination hashes are
included; this is not a comparison of selected snippets alone.

Independent review also compared ordered tokens, cfg context and comments
for 104 function/method bodies and 37 data definitions. Finite in-memory
negative checks reject MMIO-constant, removed-guard, statement-order, cfg,
visibility, missing-file, duplicate-file and unlisted-file mutations for both
backends, even with destination-hash checks bypassed. These checks supplement
rather than replace compiler, API, source-provenance and hardware validation.

## Live evidence and historical receipts

Exactly six live rows in `ci/hal-capabilities.yaml` become 23 rows:

- Whole-file L012 evidence for factory LSI and external low-speed SYSCLK
  expands to the complete ordered six-file replacement set.
- Whole-file L052/L083 factory-LSI evidence expands to its complete ordered
  eight-file replacement set, excluding the two pre-existing LSE leaves.
- Each `pub enum HseMode` reference and the `pub struct Pll` reference retains
  its literal anchor at the appropriate new `mod.rs` path.

Collapsing those exact replacement sequences recovers the complete baseline
YAML object and original YAML bytes. All other fields, scope, families,
limitations and evidence rows are unchanged. Thirty-six reference mutations
were rejected, including incomplete, extra, substituted, duplicate and
reordered replacement sets.

The canonical source lock/catalog contain no direct old RCC-root consumer
backlinks requiring relocation. Existing `project_audit_inputs` still pin
historical audit documents, not the new source snapshot. Historical receipts,
vendor identities and provenance implementation remain byte-identical. Old
flat paths inside those historical documents retain their historical meaning;
there are no placeholder old files or missing-reference exemptions.

## Separate validation maintenance

Acceptance exposed pre-existing validation drift, separately documented in
[`rcc-validation-maintenance.json`](rcc-validation-maintenance.json):

- Two HSE source checkers expected the old electrical base alone, while the
  baseline generated exact packages already included qualified LSE/LSI layers.
  They now require the exact complete composition, pinned qualification
  objects/policy map, exact package roster and base-only generic aliases.
  Original source/HSE/PAC assertions remain. Unknown fields are not filtered.
- Two synthetic generator test setups lacked later-required RTC, MCO and ATIM
  catalogs and referenced inputs. Only fixture setup changed; production
  generation and original test assertions remain unchanged.

These are explicitly additional gate/test repairs, not effects of moving
source files and not new hardware qualification. Original failures are
retained alongside passing reruns. The maintenance receipt records baseline
reproduction methods, exact before/after hashes and validation limits. Final
combined compiler/example/source-gate results belong to the batch validation
report; this layout document makes no hardware or silicon-validation claim.

## Deferred scope

L010/L011 configuration-phase extraction is a separate follow-up. It must
first identify transaction phases from existing guards, snapshots, writes,
acknowledgments, rollback and error precedence, then prove ordered-token and
control-flow correspondence across each proposed boundary. It must not move
fallible operations across earlier guards or reorder rollback/publication.
This batch neither changes that backend nor hides its long configuration
function behind a cosmetic `mod.rs` wrapper.
