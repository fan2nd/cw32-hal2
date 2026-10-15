# RCC module-split validation

Validation began 2026-10-14; final checks completed 2026-10-15 (UTC). Baseline: `a0df1333fb9aed61049f550cc17026b6a0d300a6`.
This receipt covers the mechanical replacement of the flat L012 and L052/L083
RCC backends with responsibility modules rooted at `mod.rs`. It does not add
clock qualification, change a transition algorithm, or re-certify historical
silicon acceptance. The layout, exact inverse maps and separate validation
maintenance are recorded in [the module layout](rcc-module-layout.md),
[the relocation manifest](rcc-module-relocation.json) and
[the maintenance receipt](rcc-validation-maintenance.json).

## Frozen inputs and independent correspondence

Official Rust/Cargo 1.99.0, Python 3.12.14 and `thumbv6m-none-eabi` were used.
After both source authors froze their changes, the validator captured all
485 HAL/example input files, including manifests/locks and the matrix script.
Every accepted compiler group and the final publication checks compared these
files again. Checkpointed commands retain their individual logs, exit codes,
expected diagnostic classes and hashes. Earlier acceptance logs are not used
as newly executed tests.

Independent review compared 104 complete function/method bodies and 37
struct/enum/const/static definitions, with cfg-bearing impl context, ordered
tokens and inline comments. The explicit inverse mappings reconstruct all
2,905 L012 and 3,839 L052/L083 baseline lines exactly once. Fourteen new module
files and the two unchanged pre-existing LSE leaves are bound by the manifest.
Their compact sorted path-to-SHA-256 digest is
`4e84feb7b6578f077399e8eb993826abdc03b18e51a394d6a964aa5a19dc34e2`.
The shared RCC root, generated-hook generator and HAL root remain unchanged.

Six live capability references become 23 exact ordered references/anchors;
reversing that explicit migration reconstructs both the baseline YAML bytes
and the complete parsed object. One positive and 36 negative capability cases
passed; an additional 16 source-map mutations were rejected. This is stronger
than an item-count or whitespace-only comparison, but not a hardware proof.

## ARM builds and firmware links

All **113 ARM release-library commands passed**: all 54 chip selections with
`rt`, the same 54 with `rt,defmt`, and five existing GTIM1 selections. The
checkpointed runner executes the exact matrix from
`bash ci/check-hal.sh --matrix-only`; no chip selections are combined into an
invalid multi-device build.

All **237 firmware-link commands passed**: 176 ordinary selections and 61
explicit clock variants. These counts are Cargo commands, not distinct chips
or individual ELF files. The ordinary commands build every enabled bin for
every declared chip selection in these groups:

| Example group | Commands |
|---|---:|
| `hse-clock` | 21 |
| `lse-sysclk` | 23 |
| `lse-clock` | 16 |
| `lsi-clock` | 28 |
| `rtc-calendar` | 36 |
| `rtc-alarms` | 36 |
| `lcd` | 8 |
| `pll-clock` | 8 |

The additional commands cover both L012 packages' retained-HSE and HSE time
bins, both L012 live-LSI calendar bins, and all five exact L083 packages across
11 PLL combinations: HSE crystal/bypass, fractional HSI, low voltage,
fractional/low voltage, HSE/low voltage and HSI/HSE time-driver modes. The
ordinary groups cover all ten affected exact packages, crystal/bypass LSE
SYSCLK, factory-LSI targets, RTC preservation, and LCD LSE ownership examples.
No firmware was flashed or run.

## Public API, ownership and host helpers

All **71 public-API compile expectations passed** against the actual ARM HAL.
Positive controls preserve public configuration/source/clock/error paths for
all ten exact packages. Generic L012/L052/L083 retain HSI while LSI/LSE SYSCLK
variants remain unavailable. The historical generic L083 PLL type remains
public; no new generic qualification is inferred from its type's existence.
Five exact L083 PLL controls compile; L052/L012 PLL selection is rejected.
Private backend modules and crate-only generated hooks remain inaccessible.

LSE crystal/bypass ownership controls compile for L012, L052 and L083. Negative
cases reject moved pins, reuse of a moved clock, and simultaneous live mutable
pin borrows; dropping a borrowed capability permits Rust reborrowing, without
claiming hardware pad release. RTC and LCD constructors consume their LSE
capability; attempted reuse or borrowed conversion is rejected. Expected
failures require the intended Rust diagnostic class, not arbitrary failure.

**54 actual-source host tests passed**: 26 L012 and 28 L052/L083 tests. Forty-three
extracted production items were baseline-compared. Coverage includes divider
encodings, guarded polling order and CPU-iteration budgets, inspection-error
priority, LSI acknowledgment/phase identity, inherited HSI encodings, HSE bounds
and 99 HSI/PLL admission combinations. Actual PAC value types and ClockBounds
execute with hashed generated qualification constants. Minimal state/config
carriers and the fault callback are modeled; no real MMIO or full init state
machine executes in these tests.

## Source, metadata and generation gates

`./d lint`, `./d test` and `./d check` passed. The layout gate covers 607 Rust
files. `./d check` regenerated into a temporary directory, required exact
committed data/PAC parity, rejected zero/multiple chip selections and exercised
all 54 PAC metadata selections. Separate provenance and transactional-generation
suites passed **53 and 10 tests**, respectively.

All 16 targeted commands passed: clock contracts, RCC-control metadata,
13-family operating envelopes, both affected HSE source checkers, LCD evidence,
RTC calendar/alarm/remaining evidence, all six L010/L011/L012/L031/L052/L083 LSE
source/data gates and functional coverage. Each LSE gate checks the complete
54-selection generated roster. These remain source/data/qualification checks,
not executions of clock hardware.

Full hardware-scope acquisition/provenance verified **43 required originals
and 565 source files**, plus two generated acquisition-metadata views. The two
historical website-discovery HTML snapshots are explicitly outside this scope.
Canonical source pins, their historical evidence and distribution policy were
not changed. The current capability report remains 13 families, 458 functions
and 2,430 rows.

The final `./d gen-all` passed and all **910 generated
data/PAC files remained byte-identical** to the captured pre-validation set.
Provenance was rechecked after refreshing its derived navigation views. The
final source-only package was produced by the unchanged packaging script in an
exact hash-verified clean staging tree, with all 16 RCC modules and required
historical receipts present and both obsolete flat source paths absent.

## Failures retained and resolved

The first aggregate host attempt stopped at an unchanged synthetic-generator
fixture missing the subsequently required live-LSI catalog. The independently
reviewed maintenance corrects only two fixture setups: the library fixture and
its integration fixture now copy the required authored RTC live-LSI, MCO and ATIM catalogs and their
hash-bound policy inputs. Production generation algorithms, policy files and
negative assertions are unchanged. The prior aggregate rerun ended without a terminal result and is not counted
as a pass. A fresh complete aggregate command was executed on 2026-10-15.

The unchanged old L012/L052 HSE checkers initially compared a base electrical
object directly with later fully composed exact-package objects. The separately
reviewed repair constructs the complete expected object from the exact base,
exact qualified LSE package configuration and hash-pinned family LSI policy,
checks the complete selection roster, and retains full dictionary equality.
It does not filter keys or accept subsets. The maintenance includes seven
positive and 44 negative cases; independent review adds seven positive and
160 mutation rejections. Historical receipts are not rewritten.

Missing pinned English manuals and two acquisition-metadata views initially
blocked source/aggregate checks. The existing acquisition tool recovered the
exact official bytes and generated its own metadata; successful reruns followed.
One RTC alarm invocation initially omitted its required source-root argument;
its corrected explicit absolute source root passed. An initially stale derived
provenance index was refreshed using the existing `--write` flow. No canonical
pin or gate was weakened to turn any of these failures into a pass.

Two API fixture assumptions were corrected without production edits: the L012
and L052/L083 gate-error variants differ, and generic L083 already exports PLL
types. All failed fixture attempts remain separate from the 71 accepted cases.
The accepted probe lock matches the firmware lock's dependency versions and
checksums; only the temporary probe package differs.

## Documentation, formatting and limits

Normal `cargo doc --no-deps` passed for exact and generic L012/L052/L083, six
configurations, and their preserved root paths were inspected. No relocated RCC
source emitted a broken-link location warning. A fresh whole-crate strict
`-D rustdoc::broken_intra_doc_links` attempt still fails on the five existing
`flash.rs:133` numeric bracket links. That file is byte-identical to baseline;
this receipt does not claim strict whole-crate rustdoc passes.

Optional `rustfmt --edition 2024 --check` is **not clean**. It requests import
ordering, new L012 signature/path wrapping and trailing-blank cleanup; both
baseline flat files also fail that formatter, and the two unchanged LSE leaves
retain their baseline formatting. The accepted freeze was not reformatted
mid-validation. `./d lint` and `git diff --check` pass. Formatting cleanup is a
separate task, not a hidden passing gate or a semantic acceptance claim.

Legacy untracked `.validation` contents would be selected by the existing
packaging script when run directly in this long-lived checkout, including local
vendor copies and test executables. The accepted clean staging package includes
only tracked accepted source plus the explicit new-source/docs whitelist.
No vendor PDF, local test executable or scratch content was distributed; the
broader local-scratch packaging hazard is separate tooling cleanup.

No hardware, physical oscillator timing, electrical behavior, recovery from
source loss, IRQ/NVIC effects or board acceptance is proved. No Miri run or
remote-CI result is claimed. Existing generated unused-constant warnings remain;
this is not a warning-free-build claim. Historical acceptance is ancestry only,
not a newly executed clock test.
