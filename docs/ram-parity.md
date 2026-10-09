# RAM parity diagnostics

`embassy_cw32::ram::Ram` owns the `RAM` peripheral token through `Peri` for its
entire lifetime. The concrete singleton needs no user-implementable instance
trait. Construction, drop and `release` perform no register writes. All thirteen
families support the same diagnostic API, backed directly by typed PAC fields.

- `status` reads checking/interrupt state, the error flag, and (only if flagged)
  the complete reported address. The observation is sequential, not atomic.
- `error_pending` reads ISR.PARITY independently of IER.PARITY.
- `reported_address` returns a typed integer diagnostic, never a Rust pointer.
  With no pending error it can contain the reset value 0x2000_0000 or stale data.
- `set_interrupt_enabled` changes only IER.PARITY and checks readback. It changes
  error reporting, not RAM checking, and preserves the read-only EN bit.
- `acknowledge` writes ICR with its source-qualified no-op seed and clears only
  PARITY by writing zero. It never performs a read-modify-write of the command.

There are two hardware layouts. L010/L011/L012 have IER.PARITY at bit 0 and no
readable EN. Every other family has IER.PARITY at bit 1 and read-only EN at bit 0.
All families use ISR.PARITY and ICR.PARITY at bit 0, full 32-bit ADDR at offset
0x04, and the same base address 0x4002_2400. Four normalized templates remain
because their source descriptions differ; the corrected L031 and L083 templates
are exactly equal and now share one canonical IR.

## Memory and interrupt boundaries

Every own-family manual states that checking is enabled after power-up and
cannot be configured by the user. CPU writes calculate a parity bit for each
byte. Reads compare data with its parity and latch an error when they differ.
There is no documented enable/disable control, initialization command, SRAM
reset, scrub, error injection or error correction command. No such routine is
exposed, safe or unsafe. The HAL never scans or initializes SRAM, overwrites a
stack or heap, changes vectors, or affects code stored in RAM. Ordinary runtime
startup remains responsible for initializing its data and establishing the Rust
execution environment. This driver does not certify startup or retained RAM.

A detected error means memory may already be corrupt. Reading an address or
clearing a flag does not make that data valid or execution safe to resume. The
manual does not specify first-error versus last-error address retention, an
atomic status/address snapshot, an event queue, or error-versus-clear priority.
Acknowledgment can coalesce concurrent errors. Software must choose an
appropriate fault policy without claiming the corrupted address is recoverable.
The manuals explicitly associate misaligned RAM accesses with HardFault; parity
is documented as a flag and optionally an IRQ, not as guaranteed HardFault/reset.

The interrupt is IRQ3, shared with FLASH: `FLASHRAM` on twelve families and
`FLASH_RAM` on L052. This driver never enables, disables, unpends or installs the
NVIC vector, and never reads or clears FLASH flags. Enabling RAM's source with a
pending error can immediately request the shared interrupt. Applications must
arrange the shared handler before unmasking the vector and handle both sources.
There is no independent RAM clock or reset gate; no `RCC_INFO` entry is invented.
The RAM source mask persists across drop/release by design.

## Corrections to vendor data and old PAC

These are register/data corrections separate from the new HAL:

1. L031/R031/W031/L052 SVDs truncate ADDR to 14 bits. Their own RAM_ADDR tables
   specify bits 31:0 and reset 0x2000_0000. Guarded field-width overrides preserve
   the complete absolute address. L052's CMSIS mask also conflicts with its manual;
   L031/R031/W031 CMSIS masks claim 32 bits while their structs retain 14 bits.
2. L010/L011 SVDs assign IRQ3 only to FLASH. Their own interrupt tables, RAM
   chapters and CMSIS identify the shared FLASH/RAM vector. Guarded association
   overrides retain FLASH ownership and add RAM.
3. The pinned chiptool IR has no per-field access information. Consequently the
   old PAC generated `Ier::set_en` for a read-only hardware status field. The
   source-reviewed field-access sidecar now removes exactly that setter through
   Rust syntax-tree processing, retains `en`, and leaves `set_parity` available.
   It does not alter upstream IR schemas or remove low-level raw PAC access.

Most classic SDK RAM drivers reuse bit-1 `RAM_IT_PARITY` for bit-0 ISR and ICR.
F002/F003 use bit-0 status masks and shift only for IER, correctly. The new HAL
uses each register's typed field independently. L010's SDK address-mask macros
are also misspelled/truncated; its manual and SVD agree on a full address.

## Evidence and verification

[The evidence ledger](ram-parity-evidence.json) records every family's own
manual, datasheet, SDK artifact, exact URL, printed revision, SHA-256, relevant
manual pages, and SDK archive member chain. A030 uses the explicitly shared
F030/A030 manual and F030 SDK with its own A030 datasheet. Vendor originals and
PDF text remain external and are not redistributed.

`tests/verify_ram_parity_sources.py` checks original bytes, manual semantics,
SDK discrepancies, normalized field/IRQ projection and all 54 selected chip
metadata records. The independently implemented SVD comparison remains active.
The historical RTC review boundary is reconstructed by reversing only the
individually recorded later RAM corrections, preserving its original digest.

Validation compiles normal ARM libraries and links a real passive diagnostic
example on one exact package from each of the thirteen families. Separate PAC
compile-only checks require readable EN where present, writable PARITY, full
address reads, and exact E0599 rejection of EN setters and ADDR/ISR writes.
No HAL tests, HAL test harnesses, firmware execution, fault injection, flash,
publication or silicon validation are performed.
