# UART/SPI module-split validation

Date: 2026-10-14 (UTC). Baseline: `e75f62e7f792dede2b2d3c89bbaba5f9aee25531`.
This receipt covers the mechanical replacement of the two flat drivers with
real `usart/mod.rs` and `spi/mod.rs` module trees. Public paths, generated hooks,
family qualification and executable behavior are intended to remain unchanged.
The separately reviewed current-capability verifier correction is described in
[the module mapping](uart-spi-module-layout.md); it does not change HAL behavior.

## Toolchain and source identity

Official Rust/Cargo 1.99.0 and `thumbv6m-none-eabi` were used. A full HAL and
example SHA-256 snapshot was captured after both source authors froze their
changes, before acceptance commands. All 473 captured HAL/example files were checked again after the compiler
groups, with no drift from the frozen identity. Per-command logs and checkpointed result files preserve
completed work rather than relabeling old batch results as current tests.

Independent relocation review compared the exact git baseline with all 22 new
files. It preserved 611 UART and 323 SPI comment/doc lines, all impl bodies,
Drop implementations, unsafe contracts, cfg predicates, fields and generated
macro roots. Differences are module/import/reexport wiring, the reviewed
minimum parent-scoped visibility promotions (39 UART, 27 SPI), and four
formatter-added trailing parameter commas. No `include!` or path-attribute
substitute was introduced. Implementation submodules remain private; only the
existing SPI `mode` and `word` namespaces are public.

## ARM compilation and firmware links

All **113 ARM release-library commands passed**. The checkpointed runner
executes the exact command set from
`bash ci/check-hal.sh --matrix-only`: 54 chip selections with `rt`, the same 54
with `rt,defmt`, and five GTIM1 time-driver selections (L083, L052, L010, L011,
L012). Each is a release ARM library build, not a firmware execution.

All **136 firmware-link commands passed**: 125 main selections, one ordinary
blocking example and ten explicitly enabled time examples. The main runner
covers every declared chip selection in the ten dedicated
UART/SPI example groups plus the six UART-using DMA/clock groups. All ordinary,
RTS/CTS, idle, buffered, borrowed, shared-vector, byte/nine-bit, half-duplex,
classic/native DMA and byte/wide IRQ-PIO bins selected by those features are
included. Required-feature time examples are explicitly compiled separately.
Counts below are Cargo commands, not distinct chips or ELF files.

| Example group | Chip-selection commands |
|---|---:|
| `f020-dma` | 3 |
| `four-channel-dma` | 11 |
| `hex-clock` | 5 |
| `hse-clock` | 21 |
| `l012-dma` | 2 |
| `l012-spi-dma` | 2 |
| `pll-clock` | 8 |
| `spi-async` | 11 |
| `spi-dma` | 12 |
| `uart-buffered` | 8 |
| `uart-dma` | 21 |
| `uart-dma-tx` | 7 |
| `uart-flow-control` | 7 |
| `uart-half-duplex` | 2 |
| `uart-idle` | 3 |
| `uart-ninebit` | 2 |
| `cw32f030`, ordinary blocking bin | 1 |
| `hse-clock`, L012 `embassy-time` bin | 2 |
| `pll-clock`, all eight `embassy-time` selections | 8 |

The ordinary group uses its existing `-Tlink.x` linker configuration. Other
commands use `cargo build --locked --release --bins --target thumbv6m-none-eabi
--manifest-path examples/GROUP/Cargo.toml --no-default-features --features CHIP`;
the explicit time commands use `--bin hse-time` or `--bin pll-time` and add
`,embassy-time`. Linker memory limits and example literals were not enlarged or
changed. F002/F003 retain IRQ-PIO SPI support and lack SPI DMA; they are not
incorrectly classified as having no async backend.

## Public API and ownership

All **58 final public-API compile expectations passed** against the real ARM
HAL. Expected failures match their intended diagnostic class or lifetime
message, rather than accepting an arbitrary compiler failure.

- Fourteen buffered-borrow checks cover standalone and split-owner Drop reuse,
  unsafe/static constructor boundaries, live buffer/owner/prefix escape,
  disjoint split/fill/write, a prefix across await, consuming with a live prefix,
  and borrowed RTS/RTSCTS constructors.
- Nine half-duplex checks cover byte/u16 APIs, absent split/split_ref/Deref,
  private inner ownership, wrong pin, missing typed interrupt binding, two
  simultaneous mutable futures and dropping an owner with a live future.
- Nine-bit methods compile on L012; both nine-bit methods and the half-duplex
  type are rejected on twelve other family representatives (25 checks).
- The root SPI configuration/error/word names, `spi::mode`, `spi::word` and
  generic wide transfers compile. Four probes reject private implementation
  modules with E0603. Five handler/backend checks cover L083 UART shared-vector
  acceptance and single-handler rejection, L012 SPI23 binding, F002 IRQ-PIO
  handler availability and F002 DMA-constructor absence.

Two invalid initial fixture expectations were corrected and excluded from the
58 accepted cases: the L010 standalone RTS probe first used PB0 instead of the
actual PB1 route, and an obsolete planning statement incorrectly expected F002
to lack IRQ-PIO SPI. The corrected route and existing PIO/no-DMA boundary passed.
No production change was made to satisfy either fixture.

## Fresh actual-source host and structural tests

**47 tests passed across 12 freshly compiled host harnesses.** Extractors read
the final individual modules, not cached flat sources or a concatenated
pseudo-monolith. They preserve executable method bodies; standalone harness
scope removes parent-only visibility and supplies modeled owner, register,
channel, critical-section or wait types where necessary.

- Ring/storage: 7 tests cover capacities 1–12 and every wrap offset/occupancy,
  a live shared occupied prefix while appending only into free slots, overflow,
  sticky-error/clean-prefix ordering, empty reads, partial consume, the 64-byte
  copy cap, overconsume rejection, local storage reuse and 120,000 deterministic
  mixed operations checked against a reference queue.
- Native idle loop and actual wait-guard Drop: 6 tests cover unpolled/empty and
  rejected nine-bit calls, RC plus idle, zero-byte idle, buffer completion,
  preserved prefixes on error and cancellation cleanup. Event readiness and
  receive-helper results are mocked here; the next tests separately exercise
  actual receive helpers.
- Receive helpers: 2 tests compare byte receive results/register trace with the
  baseline for 512 status/ack/data combinations and check wide payload/error
  priority. This is a modeled register trace, not an MMIO execution.
- UART configuration: 4 L012 and 2 classic tests cover parity/data width,
  byte-only rejection, nine-bit masks and L012 Down-pull rejection.
- Half-duplex wrappers: 6 tests cover pre-flush byte rejection, empty reads,
  byte/u16 flush ordering, unpolled futures and pending flush cancellation/retry.
- TX DMA completion: 4 tests cover idle/quarantine without polling, retained
  in-flight state, error quarantine, and request closure before retirement.
  RX staging: 2 tests cover no access outside Ready and partial/empty copying
  until retirement. Actual TX/RX Drop bodies: 2 tests check retained resources
  when unresolved and release after clean completion.
- Atomic pair admission: 3 tests execute the actual critical-section admission
  body with type-level channel indices supplied as local test indices. Both
  available channels commit together; same-channel or either unavailable,
  owned/raw channel rejects without a partial claim.
- SPI words/dispatch: 6 tests cover 4/8/9/16-bit PIO buffers, masking, unequal
  lengths and wide DMA rejection before touching the backend, including empty
  calls. Byte DMA dispatch remains accepted. Three additional actual SPI Drop
  tests cover unresolved/error pair retention, clean-completion release after
  backend deactivation, and ordinary PIO teardown without DMA retirement.

Twelve additional source assertions check request-close/retirement order,
in-flight publication before start, pair validation before either claim,
SPI wire completion before both retirements, retained SPI storage/pins,
RX interrupt/direction disable before ring detachment, retained UART clocks,
inactive shared-vector exclusion and waking outside the lock. Half-duplex
constructor source assertions also check preflight, one pin, high latch before
open-drain AF, both directions and SIGNAL. These are source checks, not executed
hardware or full-driver lifecycle proofs. The initial new Drop model omitted a
modeled RX phase field; that fixture compile error was corrected before all
47 accepted tests ran.

## Layout, evidence and documentation

`python3 tests/test_module_layout.py` passed for 595 Rust source files. Rustfmt
checking of all 22 moved files and `git diff --check` passed. The layout checker
was not relaxed. Both migrated original-source checkers passed against `sources/vendor`:

- `python3 tests/verify_buffered_uart_sources.py --sources "$PWD/sources/vendor"`:
  111 exact source identities, 13 families, 54 selections, 39 instances,
  seven register profiles and 2,649 package-filtered routes; nine historical
  example tuples, 19 signal routes, eight features and six current declared
  bins/dependencies.
- `python3 tests/verify_native_uart_idle_sources.py --sources "$PWD/sources/vendor"`:
  27 identities, three families, ten selections, eight instances, two register
  versions and 417 package-filtered routes.

The strict current-boundary contract passes one positive and 11 negative
mutation cases (added/removed family, limitation, evidence path, L012 override,
pins, facts, source anchor, clean-prefix clause, borrowed-storage obligation,
and historical DMA drift). Exact equality is preserved; immutable historical
receipts and vendor hashes were not rewritten. Initial missing-input failures
and the pre-existing declaration/example-list drift remain recorded as failed
attempts, followed by accepted complete reruns.

Four additional serial audits pass: SPI register command semantics for all
13 families/five register versions; SPI clock policy for 54 metadata selections,
86 SPI instances and 26 exact PDF identities/page extractions; UART typed
registers for 13 families/seven versions; and flow control for 54 selections and
877 CTS/RTS routes. The SPI clock audit initially lacked its distinct superseded
F020 datasheet; acquiring its exact pinned historical hash resolved that input
failure, without substituting the differently hashed current revision.

`python3 ci/update-functional-coverage.py --validate-only` passed for 13 families,
458 peripheral functions and 2,430 rows, fingerprint
`0926d1954a6ed751213eee6a2726cb7fa1b76af5006e1ba72800d750a7d4b188`.
These are source/data/declaration checks, not hardware execution.

The first post-test `./d gen-all` attempt failed its unchanged provenance gate:
11 live canonical backlinks still named the deleted `usart.rs`. Transactional
output protection preserved all 910 generated data/PAC files. The reviewed
repair expands each old whole-file backlink to the exact complete 12-module
UART tree. Reversing that expansion restores the complete baseline canonical
JSON; no source ID, vendor hash, URL, version or qualification changes. No
provenance implementation or historical receipt was edited. Four added tests
check complete destinations and exact consumer identities, omitted/extra or
replaced consumers, and missing files. The full **53 provenance tests passed**;
the original 49 test bodies are unchanged. An intermediate run failed only
because its derived reference index was stale, then passed after refreshing
those derived navigation views. After repair, `./d gen-all` passed and all
**910 generated data/PAC files remained byte-identical** to the pre-test
snapshot. `./d provenance` also passed its canonical lock, four derived views,
evidence-reference and dependency-pin checks in offline metadata mode. Derived
provenance navigation reflects the relocated source paths, not a hardware-data
change. The closed-documentation publication gate reruns generation/parity and
frozen-source checks before publication.
Two extra replay attempts were rejected immediately because their source-root
argument was relative; they are not source-validation passes. Accepted commands
above use the required absolute path.

Normal `cargo doc --no-deps` passed on F002, F030, L010, L012 and L083 with runtime
and defmt, with broken-link warnings enabled. Public root reexports, `Spi`,
`Uart`, `BufferedUartRx`, `Config`, `Word`, `mode::Master`, `word::U9`, and the
L012 half-duplex/shared-handler pages were checked at their preserved paths.
No moved UART/SPI source emitted a rustdoc location warning.
A whole-crate strict `-D rustdoc::broken_intra_doc_links` attempt failed on the
five pre-existing numeric bracket links in `flash.rs:133` (`PC[0]` through
`PROG[4]`). That file is byte-identical to baseline. This is not a whole-crate
strict rustdoc pass; the initial failed command is retained separately.

## Limits

No board was flashed or run. No Miri run is claimed for this batch, and earlier
Miri/acceptance receipts are not relabeled as current results. Mock tests do not
prove MMIO side effects, IRQ/NVIC ordering or timing, DMA handshakes, wire timing,
lossless receive behavior, scheduling, physical teardown or electrical safety.
Borrow checking cannot establish a caller's unsafe borrowed-buffer no-leak
promise. Existing generated `RCC_LSE_SYSCLK_*` unused-constant warnings remain;
this is not a warning-free compilation claim. No new hardware qualification or
remote CI result is asserted by this receipt.

## Frozen source inventory

| File relative to `embassy-cw32/src` | Lines | SHA-256 |
|---|---:|---|
| `spi/async_api.rs` | 80 | `a01b383cf139e5909258e291c7f282b61ceb8503acb6fb2418b049d524a8fc6c` |
| `spi/blocking.rs` | 464 | `95f014ccb75889878392f83287816ae3690cbde698e7b8d5731db52036b50a7a` |
| `spi/config.rs` | 224 | `7bde4c012a2c3e344abf83d94fd5e5937146087d7d29231cfefa2820dd1bc288` |
| `spi/dma_backend.rs` | 512 | `aec5cc23ee999aa4abc688143de4c659ece1ef8d74ae9f5b90600b643af5c20d` |
| `spi/interrupt.rs` | 56 | `e9cfea1b5cf6daf0c13747b56c4062c79cf46cc6011af83501c95719e2046442` |
| `spi/mod.rs` | 329 | `aee90f6b866a30e3a365347ef83decd1c148fdb25c93aa9d7dbed7e407354edb` |
| `spi/mode.rs` | 9 | `e3f18296586c63d9f1470d74cb5c54398e44284cd4c2087c4ea7cda5a6b9e594` |
| `spi/pio.rs` | 409 | `74d914a8fbed81d22fbb925ec350733b46e97a592ecd581ab4c56a0f552a6e49` |
| `spi/traits.rs` | 41 | `26bf363eb2d8ae112e9b3d841125aeeb700d9b8b39f330600ee32d414002d41f` |
| `spi/word.rs` | 61 | `d12030d3a35481403d6136bf85862b10c68d49dd076cf74d1ca911a2d21a5c92` |
| `usart/buffered.rs` | 638 | `9abbf2bc739a304d03968410fc62e01c351af348242f2a5d6ffcfc98c074b491` |
| `usart/config.rs` | 470 | `62a433a000569d1e215cdfc6e407965b383bbf3e24988617db8b215df54c176c` |
| `usart/dma_rx.rs` | 368 | `085d1aaf241661c2040648f64edbe81df0bafbce0b8a4dc78ff3d18d652889a9` |
| `usart/dma_tx.rs` | 176 | `59ba3d6dafea708708a98c3c3389f008edfca05d0b4ce64f73890b8ac0d6c9a4` |
| `usart/half_duplex.rs` | 154 | `96d00502d84c2ee96c3da1049754d2abcdd31c7217d81d20db0978f0a72fb89d` |
| `usart/interrupt.rs` | 322 | `92c1c5d79336740df78cb3702a81c223fafc66c8afa402ac9ed2697fa857d18f` |
| `usart/mod.rs` | 441 | `5d9c89a8b9c05f13d3b1421fbfc5b404c0a8382f32938ebee665d7e30279e50f` |
| `usart/nine_bit.rs` | 122 | `4545114b8399fdff9a604fe2d5a7b585a3e977d16af9884640550685c1f3cafa` |
| `usart/rx.rs` | 263 | `0b0c5ecba2d42a5875d0022bf2385eef39abea4d3f7f2fa9b8054463cf3d6568` |
| `usart/traits.rs` | 108 | `2ad7f4ccf3ea6079c99318da90f90dfe08782b825fe518f333adb014f809d033` |
| `usart/tx.rs` | 177 | `8e3d241b20ac84a765e2f006a795b05aa75dcb77085379324d79095f52b73dfe` |
| `usart/uart.rs` | 171 | `01da3fc1860d0b0aaef5824c59092f79ad809119d9c475122494e1d7a07faf8d` |
