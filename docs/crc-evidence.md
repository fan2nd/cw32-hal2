> Historical software-verification references: HAL tests and fixture harnesses cited below were deleted on 2026-10-08. Their reported results apply to the dated source snapshot only; source facts remain unchanged. See [current HAL layout and build scope](hal-production-layout.md).

# Hardware CRC implementation evidence

The HAL exposes the documented fixed-preset CRC engine on all 13 reviewed
families. This is source-backed implementation and host/compile validation,
**not board validation**. `crc-evidence.json` pins 72 official PDF/archive/SDK/index
inputs by SHA-256 and retains exact URLs, family policies and reference vectors.
The earlier `read-only-crc-wwdt-audit.md` contains the detailed manual analysis;
its historical “not implemented” status describes the earlier audit only.

## Current source layout

The driver is a single `embassy-cw32/src/crc.rs`. Its existing `Crc` owner
accesses the typed PAC directly; there is no separate register wrapper or
backend module. Fallible reset/clock checks and the initial CR write complete
before constructing `Crc`, preserving error-side gate state. Drop disables the
gate only after successful construction.

The pinned Embassy revision `f16efeffe37581092ec184718e6fdb1620393214`
uses `embassy-stm32/src/crc/mod.rs` to select substantive `v1.rs` or `v2v3.rs`
implementations. Each implementation's `Crc` methods directly access its PAC.
That is an organization reference, not evidence that STM32 algorithms, reset
semantics or register transactions apply to CW32. The family capability cfgs
and native CW32 transaction widths below remain unchanged.

## Capability contract

| Family | Valid MODE | Payloads per native transaction | Default |
|---|---|---|---|
| F002/F003 | 4–7 | 8 bits | CCITT, mode 4 |
| F020 | 0–7 | 8/16/32 bits | CCITT, mode 4 |
| F030/A030 | 0–9 | 8/16/32 bits | CRC32, mode 8 |
| L010/L011/L012 | 0–7 | 8 bits | CCITT, mode 4 |
| L031/R031/W031/L083 | 0–7 | 8 bits | CCITT, mode 4 |
| L052 | 0–7 | 8 bits | CCITT, mode 4 |

Build selection asserts each family's exact reviewed CRC register version.
`crc_poly_8005` exposes modes 0–3, `crc_32bit` exposes modes 8–9, and
`crc_input_16bit`/`crc_input_32bit` expose genuine native transaction methods.
These describe hardware capabilities, not HAL enable aliases. Unsupported
variants and feed methods do not exist in the selected Rust API.

The four common presets retain encodings 4, 5, 6 and 7. The public `Config` holds
a typed `Mode`, so arbitrary numeric modes and unsupported custom parameters are
rejected by Rust before any MMIO occurs. The public API retains `new`, `reset`,
`feed_byte`, `feed_bytes`, and `read() -> u32`. Existing native-width methods
remain on F020/F030/A030. There are no implicit word-to-byte emulation methods.

## Register protocol and source boundaries

CRC base is 0x40023000 on every reviewed family. CR is +0x00, DR is +0x08 and
RESULT is +0x0c. Construction writes CR exactly once after reset/clock checks;
`reset()` always writes it again, including when the selected mode is unchanged.
The fixed preset initializes the accumulator. Reading RESULT neither resets it
nor writes CR. CRC16 reads are zero-extended to the compatible u32 return type.

- F020/F030/A030 use the PAC's 8-, 16- and 32-bit DR views. Their documented
  sequence 00,11,22,33,44,55,66,77 is equivalent to halfwords 0x1100,0x3322,
  0x5544,0x7766 and words 0x33221100,0x77665544. Native feeds are low byte first.
- Other families use their own SDK's 32-bit DR store containing a zero-extended
  byte. DR bits 7:0 alone are payload. A word-sized bus store here consumes one
  byte, not four. F002/F003 RESULT's field is RESULT16; the other byte-payload
  families call the low-16-bit field RESULT.
- SYSCTRL AHBEN bit 2 gates CRC and AHBRST bit 2 is active-low peripheral reset.
  Both gate read-modify-writes are protected by a critical section. On
  L010/L011/L012, every enable/disable replaces AHBEN[31:16] with 0x5a5a while
  preserving unrelated low-half fields. AHBRST is unkeyed and is never written.
- `try_new` returns `HeldInReset` without writing the clock or CRC registers
  when the reset bit is low. It returns `ClockNotEnabled` when the clock-enable
  readback fails; no CRC register is then written. `new` is a panic wrapper.
  There is no unbounded polling. These checks cannot diagnose an external bus
  fault, and safe code must not alter clocks/reset behind an active driver.
- The `Peri<'d, CRC>` singleton/borrow is held for the full driver lifetime.
  Drop disables only the owned CRC clock, using the same keyed gate policy.
  It does not feed CRC, touch RESULT or pulse peripheral reset.

A030 uses the common CW32x030 manual, whose scope explicitly includes both
F030 and A030. Its own datasheet address map agrees; the official A030 manual
and SDK listing pages and datasheet are pinned as supplementary evidence.

Important evidence limits:

1. F020's SDK includes copied CRC32 definitions, but its own manual chapter 10
   documents only eight CRC16 presets. The HAL follows the manual. Its PAC
   RESULT32 is a 32-bit bus view containing only 16 valid result bits, not CRC32.
2. L011's own manual V1.1 is now acquired and hash-verified. Chapter 9
   pp131–136, especially table 9-1 p132 and §9.6 p136, documents all eight
   algorithms, MODE 0–7, DR[7:0] and RESULT[15:0]. Section 9.3.2 p133 documents
   byte input. AHBEN §4.7.11 p76 confirms CRC bit 2 with key 0x5a5a;
   AHBRST §4.7.14 p79 confirms unkeyed active-low reset bit 2. This closes the
   earlier audit's missing-manual gap. Its SDK's eight example vectors remain
   supplementary checks; no native halfword/word feed or CRC32 is inferred.
3. L052 INIT +0x10 exists in the header/SVD/PAC, but manual V1.5's CRC chapter
   neither lists nor describes it. The SDK does not use it. HAL never touches it
   or exposes arbitrary seeds; the register's existence is not disputed.
4. CRC has no completion/fault interrupt API. F020 FAULT belongs to SYSCTRL.

## Fixed algorithms and validation vectors

`RefIn` reverses each input byte's bits. `RefOut` reverses the width-bit
remainder before XOR-out. These are fixed presets, not configurable knobs.

| MODE | Name | Polynomial | Seed | RefIn/RefOut | XOR-out | `123456789` |
|---|---|---|---|---|---|---|
| 0 | IBM/ARC | 8005 | 0000 | yes/yes | 0000 | bb3d |
| 1 | MAXIM | 8005 | 0000 | yes/yes | ffff | 44c2 |
| 2 | USB | 8005 | ffff | yes/yes | ffff | b4c8 |
| 3 | MODBUS | 8005 | ffff | yes/yes | 0000 | 4b37 |
| 4 | CCITT | 1021 | 0000 | yes/yes | 0000 | 2189 |
| 5 | CCITT-FALSE | 1021 | ffff | no/no | 0000 | 29b1 |
| 6 | X25 | 1021 | ffff | yes/yes | ffff | 906e |
| 7 | XMODEM | 1021 | 0000 | no/no | 0000 | 31c3 |
| 8 | CRC32 | 04c11db7 | ffffffff | yes/yes | ffffffff | cbf43926 |
| 9 | CRC32/MPEG-2 | 04c11db7 | ffffffff | no/no | 00000000 | 0376e6e7 |

The test-only independent bitwise calculator checks these vectors, the empty
message and the manual/example's 00,11,...,77 bytes. It is compiled only under
`#[cfg(test)]` and is never a software implementation/fallback in the HAL.
Protocol tests check ordering, reset writes, incremental feeds, non-resetting
reads, key replacement, held reset and failed gate paths. Actual PAC adapters
and public methods are exercised against aligned owned RAM to check write
width, offsets, CRC16 zero-extension, reserved-byte preservation on native
stores, byte zero-extension on word containers, Drop behavior and untouched
L052 INIT. RAM does not emulate peripheral arithmetic; model results are not
hardware known-answer results.

Run with the repository's `.cargo`/`.rustup` environment and
`CARGO_INCREMENTAL=0`:

- `python tests/audit_crc_sources.py`: pinned inputs, SDK mode/data protocol,
  curated access-width/field metadata, keyed gates and every chip's CRC/IRQ map.
- `cargo test --offline -p embassy-cw32 --no-default-features --features <chip>
  --lib crc::`: RAM/protocol/vector tests, selecting one exact chip at a time.
- `python tests/test_crc_hal_contracts.py`: positive controls plus exact Rust
  diagnostics for unsupported modes/widths, wrong token, repeated ownership,
  borrowed-token aliasing, numeric modes, custom seeds/polynomials and IRQ API.

Before a hardware-support claim, run all selected presets' empty and nonempty
vectors on each family, test split feeds/reset/read continuity, and confirm
native mixed-width ordering on F020/F030/A030. Neither these host checks nor the
source review substitutes for those target tests.
