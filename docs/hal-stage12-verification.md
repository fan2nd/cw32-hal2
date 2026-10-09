# Stage 12: shared GPIO and practical Rust module layout

This checkpoint implements the latest requested HAL organization. Single-file
modules are flat `.rs` files; `mod.rs` remains only for meaningful multi-file
groups. HAL tests remain deleted. No replacement test harness was introduced.

## Included changes

- Flattened 473 module paths: 44 HAL, 16 data-generator, two PAC-generator/template
  and 411 generated PAC files. PAC output comes from the updated generator.
- Replaced eight GPIO files (2,091 lines) with one direct-PAC `gpio.rs` (677 lines).
  There is no new register adapter, family wrapper or runtime dispatch layer.
- Shared GPIO operations follow the pinned Embassy `gpio_block` approach. Build
  metadata generates real port addresses, pin/AF/pull/gate masks. Actual optional
  register differences remain explicit IP/capability cfgs.
- Preserved public ownership and routing APIs, valid family pads, reserved bits,
  critical sections, atomic latch commands, shared clock gates and EXTI behavior.
- Refreshed source provenance paths and retained the original source/revision
  history. Package-neutral Flash aliases are described as not qualified for the
  storage API, rather than incorrectly assuming all have unknown memory sizes.

## Source review

The mechanical migration initially passed a 513-file Rust AST comparison. After
GPIO integration, the final move-only comparison covers 512 files, excluding the
semantically changed HAL `build.rs` and GPIO from that equivalence claim.

The shared GPIO implementation was independently checked against original PAC
IR for 13 families, 50 ports, 760 used registers and 7,233 pin fields. The raw PAC
pointer view accesses only the verified same-family subset. L012's compressed
port addressing and PF3-only pull-down, three-bit AF reserved bits, optional
registers and L010/L011 gate handshakes are preserved.

Exact source hashes and evidence are in `gpio-shared-inventory.json`,
`gpio-shared-implementation.md`, and `rust-leaf-layout.json`.

## Verification

Final combined run passed at 2026-10-08 14:57:26 UTC: 108 ARM release
builds across 54 selections and 13 families, both real GPIO example ELFs,
zero warnings, and all 903 input hashes unchanged. Input manifest SHA-256:
`5c15bc70a77342acf76bd4c499b4c63f6ee04d5408940d8434bfd917f9bc96bf`.

The final combined build result is recorded in
`verification-logs/rust-leaf-layout/final-main/matrix-summary.json`. It is the
single authority for all-feature compilation after both changes. The earlier
isolated GPIO run is explicitly partial and is not a complete matrix result.

Retained data/PAC/generator checks, source-byte provenance verification, rustfmt
and the flat-module structural check remain active. Historical HAL test logs
are dated evidence only; their test sources were deleted as requested.

This checkpoint adds no new peripheral driver coverage. The reviewed ATIM and
Flash breadth candidates remain outside it. `hal-coverage.json` lists remaining
family/peripheral gaps. No firmware was flashed or executed, and compilation
and source review do not establish silicon or electrical validation.
