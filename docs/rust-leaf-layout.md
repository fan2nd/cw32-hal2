# Rust leaf-module layout

The latest user direction supersedes the earlier blanket `module/mod.rs` rule:
a single-file module is `module.rs`; a directory retains `mod.rs` only when it
groups real sibling files or submodules. This follows the actual pinned
`embassy-stm32` tree at `f16efeffe37581092ec184718e6fdb1620393214`.
HAL tests remain deleted. Nothing was restored or moved into replacement tests.

## Mechanical migration

Outside the separately owned GPIO refactor, 473 leaf directories were flattened:
44 in the HAL, 16 in the data generator, two in the PAC generator/templates and
411 in the generated PAC. The exact old/new map and file hashes are in
[`rust-leaf-layout.json`](rust-leaf-layout.json).

Representative resulting paths:

- `embassy-cw32/src/{dma,macros,mode,spi,time}.rs`
- `embassy-cw32/src/rcc/{f030,f020,hsi_48mhz,bounds,operating,...}.rs`
- `embassy-cw32/src/timer/{low_level,simple_pwm,classic,v1,...}.rs`
- `embassy-cw32/src/exti/{engine,registers,irq_groups}.rs`
- `cw32-data-gen/src/{clock,interrupts,dma,pinouts}.rs`
- `cw32-metapac-gen/src/data.rs` and `res/src/metadata.rs`
- `cw32-metapac/src/peripherals/<kind>_<version>.rs`
- `cw32-metapac/src/registers/<kind>_<version>.rs`
- `cw32-metapac/src/chips/<chip>/{pac,metadata}.rs`
- `cw32-metapac/src/chips/metadata_NNNN.rs`
- `cw32-metapac/src/{common,metadata,all_chips,all_peripheral_versions}.rs`

Meaningful groups such as ADC classic/registers, I2C L012/timing, RCC, EXTI,
timer/BTIM/buffered, RTC/datetime and USART retain their group directories.
Module names and public Rust paths do not change when a file moves.

GPIO is now a single `embassy-cw32/src/gpio.rs`. Its separately reviewed shared
direct-PAC implementation and generated capability/address hooks are described
in [the GPIO implementation report](gpio-shared-implementation.md). It is a
semantic consolidation, so it is not included in the move-only identity claim.

All generated PAC changes originate in `cw32-metapac-gen` and its templates.
A fresh generated tree was formatted, checked against the exact expected file
map and installed. No generated Rust was hand-edited. Literal module/include
references, active source-audit path consumers and the project-wide structural
validator were updated. Historical source reports retain their dated facts and
are identified as historical rather than silently rewritten as new results.

## Equivalence and checks

An external temporary `syn` AST comparison first checked all 513 mechanical
checkpoint files. After the separate GPIO integration, 512 unaffected or
move-only Rust files were rechecked. It resolves
literal file references, translates only their exact old/new paths, and compares
non-documentation tokens. All 512 final comparisons match. The intentionally changed GPIO implementation
and HAL build hooks are separately reviewed semantic changes, not asserted to
be token-identical. This comparison includes the regenerated PAC
outputs and retained generator/data tests. The only separately reviewed
producer edits change output paths and remove unnecessary directory creation;
its generated runtime output is covered by the comparison.

The final combined layout/GPIO snapshot passed all 108 ARM release builds for
54 selections across 13 families (`rt` and `rt,defmt`) and both genuine F030
firmware examples, with zero warnings. ATIM is outside this snapshot. All 903
frozen inputs, including reviewed pinout JSON and both Cargo lockfiles, remained
unchanged. The source-manifest SHA-256 is
`5c15bc70a77342acf76bd4c499b4c63f6ee04d5408940d8434bfd917f9bc96bf`.
See [the frozen build record](verification-logs/rust-leaf-layout/final-main/matrix-summary.json).

Final structural validation passes all 507 owned Rust source files: no directory
containing only `mod.rs`, no HAL test paths/cfgs/compile-fail fixtures, and only
one GPIO source file. Rust formatting is clean. Existing schema/macro/generator
checks (93 successful cases), PAC smoke/metadata checks (three cases), 21 metadata
contracts, 17 provenance checks and retained hardware-source audits passed.
No replacement HAL test framework was created.

The Stage11 archive is immutable recovery. No archive or Library version was
deleted, no code was published, and no firmware was executed.
