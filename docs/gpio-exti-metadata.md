# Metadata-driven direct-PAC EXTI

Reviewed 2026-10-08. This is a source-preserving architecture refactor. It adds
no HAL tests, register models, fixture programs, or firmware execution.

## Authored facts and generated topology

`cw32-data/gpio-interrupt.yaml` explicitly records the existing serviced-source
policy for every one of the 50 GPIO banks across 13 families. Each profile
preserves its own manual citation from `gpio-async-evidence.json`, and records
the original policy source and its hash. The serviced mask is an upper bound
further narrowed by RAM ownership. It is not a package-pad map or proof that
every documented status field is physically implemented.

The separately authored ICR no-op command mask follows the own-manual field
domain: PIN0..7 on F002/F003, PIN0..15 elsewhere. It is never derived from a
serviced mask, reset value, package union, or safe pin list. L011/L012 retain
their full-width serviced policy. All sparse banks retain their original
serviced masks and their larger documented command domains.

The source generator projects `PeripheralGpioInterrupt` through the existing
chip JSON and PAC metadata pipeline. Its fields are `serviced_mask`,
`clear_noop_mask`, and `level_trigger`. The HAL build consumes those facts,
checks them, and emits mask constants. Real GPIO peripherals and their
existing `GLOBAL` links determine the compact state slots, bank count,
complete vector memberships, and normal `GpioInterrupt` / `PortInterrupt`
implementations. Pin-to-interrupt implementations use those same links.
There are no hand-written family mask/count/vector tables in the HAL.

## Direct selected-family PAC access

The `Registers` wrapper, `Block` dispatch enum, `RegisterAccess` trait and
`with_block!` macro are removed. The one-shot engine accesses
`pac::gpio::Gpio` directly using the existing generated `gpio_block(port)`.
No family is reinterpreted using another family's PAC layout.

Before emitting that view, `build.rs` checks every actual bank against its
selected family's GPIOA view for:

- RISEIE/FALLIE at 0x24/0x28; ISR/ICR at 0x34/0x38; IDR at 0x50
- HIGHIE/LOWIE at 0x2c/0x30 only for native-level hardware; absence at those
  offsets for edge-only hardware
- Exact 32-bit transaction width, absence of register arrays, matching access
  permissions, and each serviced field's one-bit width and position
- Strict read-only ISR and read/write trigger/ICR registers. IDR must be
  readable and have identical actual/common access. Some pre-existing vendor
  IDR metadata is read/write; the HAL only reads it and this refactor does not
  change that metadata.

Sparse vendor ICR fieldsets do not override the explicit manual command
width. Their exposed fields must lie inside the authored command domain;
all serviced fields must match. The raw 32-bit PAC command register has
proved width/access/offset on every bank. Its separately source-qualified
command is `clear_noop_mask & !owned`, leaving upper reserved bits zero and
writing one to every unowned documented no-op field. No ICR read or ISR
write is introduced. Register IR and PAC hardware definitions are unchanged.

## Ownership and behavior correspondence

Arming, IRQ completion, waker registration, latched fired state, polling,
retained Ready, cancellation and destructor sequences are unchanged. The
old generic register parameter is replaced by a concrete selected-PAC handle
and a port index. A private polling callback that always received a no-op
closure was removed; no test hooks were added.

IRQ service still reads the RAM active mask before constructing or accessing
the bank's MMIO. An inactive partner of a shared vector remains untouched.
Completion still disables triggers before W0C, latches completion before
waking, and preserves every foreign source. Existing clock, filter, NVIC,
pin ownership and binding rules remain unchanged.

## Verification boundary

Task-local source/IR correspondence checks (outside the project) verified:

- Exact old/new masks, state slots, and vector memberships for all 13 families
  and 50 banks; all 54 generated chip records project the same profile facts
- Direct-view register and field proofs against the actual bank IR
- Normalized statement-by-statement lifecycle and register-access equivalence
- The actual mask/state/vector output from every representative production build

Normal optimized `thumbv6m-none-eabi` builds passed for one exact package per
family with `rt`, plus F002/L010/L083 with `rt,defmt`. Both existing F030
blocking and asynchronous GPIO firmware examples linked using their real
Cargo and linker configurations. These are 18 production artifacts, not a
repeat of the full 108-selection matrix and not firmware execution.

The frozen build receipt contains identical before/after hashes for all 915
build inputs. Existing own-manual command-source checks, data validation,
and existing data/schema/generator tests also passed. Receipts and generated
source captures are under `verification-logs/exti-metadata/`.
