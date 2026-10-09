# Typed SPI register operations

The blocking SPI driver now keeps ISR reads in the generated `pac::spi::regs::Isr`
value and consumes `txe`, `rxne`, `ud`, `ov`, `sserr`, `modf`, and `busy` directly.
It no longer maintains a second set of hardware bit masks. The existing
configuration already uses typed CR1, SSI, data-field and enable setters; the
remaining interrupt and CR2/CR3 initialization stores now name their actual PAC
fields too. No additional register adapter or generic register helper was added.

## Command semantics and exact stores

All five supported SPI register versions have the same eight R1W0 ICR commands:
FLUSH, RXNE, SSF, SSR, UD, OV, SSERR, MODF. Their own manuals specify reset and
no-op values of `0x000000ff`. Bits 31:8 are reserved and must keep their default
zero. `0xffffffff` would therefore be an incorrect seed.

The authored `cw32-data/register-writes.yaml` owns those command facts. The shared
data/PAC generation path emits distinct `Icr::reset_value()` and
`Icr::write_noop()` constructors; ordinary `Default` and `Reg::write` remain zero.
The SPI driver starts with `Icr::write_noop()`, calls all eight corresponding
setters with false, and performs one `write_value`. This is exactly the prior
zero store, with every flag-clear and buffer-flush command explicit. The complete
reset is intentional only during disabled initialization or disabled error
recovery. There is no read/modify/write of ICR and no selective live clear that
could inadvertently flush transmission. Any future selective clear must start
with the no-op constructor and change only its intended fields.

IER initialization/drop stores still equal zero, disabling all eight interrupt
sources. Classic CR2 clears HDOE. L010/L011 CR2 clears EN, ADCRX and ADCTX; L012
CR2 clears EN, DMARX and DMATX. Low-power CR3 clears HDOE. All of those register
stores remain exactly zero, including reserved bits.

## Preserved behavior

- Each status decision uses the same one ISR read as before. Error priority is
  ModeFault, Overrun, ChipSelectFault, Underrun.
- Errors still disable, clear all owned flags and buffers, re-enable, and return
  the selected error. There is no retransmission or extra status read.
- Successful flush/transfer still requires TXE and not BUSY. Received frames are
  drained during writes, and stale receive data is consumed only when RXNE is set.
- Width remains bits minus one, with CR1 width changed only while disabled. The
  4–16-bit word API and payload masks are unchanged.
- CW32 DR remains the generated 32-bit register access with a 16-bit payload field.
  This does not change to STM32's IP-specific byte/halfword access conventions.
- `ClockBounds`, divider selection, instance electrical limits, generated
  `RccInfo`, pin ownership, and constructor validation are unchanged.

The pinned upstream Embassy revision is
`f16efeffe37581092ec184718e6fdb1620393214`. Its
[`embassy-stm32/src/spi/mod.rs`](https://github.com/embassy-rs/embassy/blob/f16efeffe37581092ec184718e6fdb1620393214/embassy-stm32/src/spi/mod.rs) uses typed status snapshots in `check_error_flags`
and typed ready predicates in `check_tx_ready`/`check_rx_ready`; its
`flush_rx_fifo` and `RegsExt` choose data access widths for each STM32 IP.
Those patterns informed this cleanup without importing STM32 register semantics.

## Source proof and verification boundary

[Own-family ICR evidence](spi-register-write-evidence.json) records all 13 family
mappings, 12 pinned manual identities, exact PDF/printed pages and extraction
hashes. Original PDF text is not redistributed. Source sections are x030
§19.8.6, F002 §16.7.6, F003 §17.7.6, F020 §18.8.6, L031/R031/W031 §19.8.6,
L052/L083 §20.8.6, L010/L011 §17.7.7 and L012 §22.7.7.

[Operation correspondence](spi-typed-register-equivalence.json) records frozen
before/after source hashes, each accessor's correspondence to the former bit
predicate, final command/control words, unchanged DR widths, and byte-identical
clock/configuration-selection and ownership code. Transfer bodies are identical
after substituting the typed accessors for their previous predicates.

`tests/verify_spi_register_writes.py` validates the own-family source pages,
authored command entries, generated register IR and generated PAC seeds. It is a
source/PAC validator, not a HAL model or test harness. Ordinary ARM production
builds and the existing blocking firmware link are recorded separately in the
handoff. No firmware is executed and no silicon/electrical validation is claimed.
