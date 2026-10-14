# UART and SPI module organization

This structural refactor starts from commit
`e75f62e7f792dede2b2d3c89bbaba5f9aee25531`. It separates the existing UART
and SPI implementations into directory modules rooted in `mod.rs`, without
expanding hardware qualification or changing the public module paths.

## Live capability evidence mapping

The following are the maintained capability anchors. All 13 destinations were checked against the final split source; each literal
anchor exists at its declared path.

| Existing source | Anchor | Destination |
| --- | --- | --- |
| `usart.rs` | `pub struct Uart` | `usart/mod.rs` |
| `usart.rs` | `pub async fn write_u16` | `usart/nine_bit.rs` |
| `usart.rs` | `pub struct HalfDuplexUart` | `usart/half_duplex.rs` |
| `usart.rs` | `pub fn new_with_rtscts` | `usart/uart.rs` |
| `usart.rs` | `pub fn new_with_cts` | `usart/tx.rs` |
| `usart.rs` | `pub fn new_with_rts` | `usart/rx.rs` |
| `usart.rs` | `pub async fn read_until_idle` | `usart/rx.rs` |
| `usart.rs` | `pub struct BufferedUartRx` | `usart/mod.rs` |
| `usart.rs` | `pub fn new_with_dma` (TX evidence, two entries) | `usart/dma_tx.rs` |
| `spi.rs` | `pub struct Spi` | `spi/mod.rs` |
| `spi.rs` | `pub fn new_async` | `spi/pio.rs` |
| `spi.rs` | `pub fn new_with_dma` | `spi/dma_backend.rs` |

Paths in this table are relative to `embassy-cw32/src/`. The 13 live pointers
in `ci/hal-capabilities.yaml` move to the file actually containing each anchor.
The SPI backend name `dma_backend` avoids colliding with the imported
`crate::dma` subsystem. Public paths such as `usart::Uart`, `spi::Spi`,
`spi::mode` and `spi::word`, as well as generated macro hook paths, are retained.

## Source verifier migration

The mechanical migration changes three path-set assertions:

- `tests/verify_buffered_uart_sources.py`: buffered evidence points to
  `usart/mod.rs`; idle evidence points to `usart/rx.rs`. Both remain exact
  equality checks, with their existing documentation evidence paths.
- `tests/verify_native_uart_idle_sources.py`: required idle evidence points to
  `usart/rx.rs`. The existing superset assertion is retained.

Family/pin/source checks and qualification are unchanged. A separately reviewed
current-contract correction is required because the unchanged baseline checker
already fails its old one-byte-read declaration check, and its historical
three-family DMA equality no longer describes the baseline nine-family scope.
A third pre-existing mismatch is its five-bin example set: the unchanged
baseline already includes the sixth `borrowed` bin, restricted to
`cw32f002f3p7`. The exact six-bin set and that exact required feature are now
checked, and the complete unchanged baseline Cargo.toml is hash-pinned in
the migration manifest. Dependency, selection and lock checks remain intact.

`uart-current-capability-boundary.json` pins the baseline commit and capability
YAML digest, the immutable historical evidence digest and canonical historical
DMA digest, and the complete baseline buffered/idle/DMA mode objects. The
checker pins this manifest's digest and derives expected current objects only
through three explicit source-and-anchor relocation entries. Complete object
equality retains all current family, status, scope, limitation, evidence,
override and pin/fact constraints. Historical three-family DMA bounds remain
checked against their historical object. Current nine-family bounds and all
three override family groups are also asserted explicitly.

The obsolete one-byte phrase is replaced with checks for the existing clean
prefix/read_exact, empty-read preservation, contiguous BufRead prefix,
consume-only-exposed, IRQ-disjoint and borrowed-storage leak-obligation
clauses. These check declarations, not runtime behavior. Neither the old
receipt nor any vendor hash is rewritten; hardware scope does not expand.

## Historical records

Existing hash-pinned validation receipts remain unchanged and refer to their
original source snapshots. In particular, old `usart.rs` and `spi.rs` paths in
those receipts are not live links to the refactored source. The earlier flat
layout descriptions in `hal-helper-layout.md`, `uart-typed-registers.md`,
`hal-stage24-verification.md` and `spi-metadata-refactor.md` describe their
respective stages; this document supersedes only their file-organization
statements. Their behavioral findings are not re-certified by a file move.

## Acceptance status

The 13 literal anchor destinations and path-only capability migration passed
structural comparison: all other capability YAML data is identical to baseline.
The isolated current capability contract passes after the separately described
strict migration. After exact catalog-driven source-cache restoration, both complete source
checker commands passed:

- `python3 tests/verify_buffered_uart_sources.py --sources "$PWD/sources/vendor"`:
  111 locked source identities, 13 families, 54 selections, 39 instances,
  7 register profiles, 2,649 package-filtered routes; 9 historical qualified
  example tuples, 19 selected signal routes, 8 exact features and 6 declared bins.
- `python3 tests/verify_native_uart_idle_sources.py --sources "$PWD/sources/vendor"`:
  27 locked source identities, 3 families, 10 selections, 8 instances,
  2 register versions and 417 package-filtered routes.
- Isolated current-contract check: one unchanged positive case and 11 rejected
  mutations covering family additions/removals, limitations, paths, L012
  override, pins, facts, anchor, clean-prefix/borrowed-storage clauses and
  historical DMA drift.

Initial missing-PDF failures and the pre-existing declaration/example failures
were retained as failed attempts, not relabeled as passes. The source recovery
used canonical pinned URLs/digests and executed no vendor binaries. Complete
compiler/runtime regression acceptance is recorded separately by the batch
validator. No new Miri or hardware pass is claimed here.

## Canonical source-consumer backlinks

The first final generation attempt correctly rejected a missing
`embassy-cw32/src/usart.rs` evidence reference. The remaining references were
11 live consumer backlinks in `sources/evidence-sources.json`, not immutable
project-audit snapshots. Each previously named the whole UART implementation.
Each is now replaced, at the same list position, by the complete ordered set
of its 12 relocated source files. No SPI backlink required migration.

Affected canonical nodes are artifact 6 members 19, 28, 38, 39 and 60, and
artifacts 24, 25 and 42 plus each artifact's text node. Collapsing every exact
12-file sequence to its former path reproduces the complete baseline JSON
object exactly. Vendor URLs, source IDs, paths, hashes, byte lengths, member
indexes and historical receipts are unchanged. The provenance validator and
its missing-file enforcement are unchanged; no placeholder source file or
missing-reference exemption is introduced.

Four focused provenance regression tests verify the full destination set and
exact 11 consumer identities, reject omitted/unexpected destinations and
replacement consumers, and reject a nonexistent relocated file through the
original validator. The initial whole-suite run also detected
stale generated provenance views; those are regenerated through the existing
provenance command, not accepted as current. Final whole-suite and generation
results belong to the batch validation receipt.
