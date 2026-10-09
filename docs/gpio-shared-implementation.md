# Shared direct-PAC GPIO

Reviewed 2026-10-08. This is a production refactor, with no new HAL tests,
fixtures, test hooks, or hardware execution.

## Architecture

`embassy-cw32/src/gpio.rs` is the sole GPIO implementation. The former shared
API plus seven family backends occupied 2,091 lines in eight files; the shared
file is 677 lines. No register adapter, wrapper, dispatch enum, virtual trait,
family forwarding file, or additional GPIO module was introduced.

The architecture follows Embassy revision
`f16efeffe37581092ec184718e6fdb1620393214`:
`embassy-stm32/src/gpio.rs` accesses the PAC directly, and its `build.rs`
generates `gpio_block()` from chip data. Here each selected family's existing
`pac::gpio::Gpio` is constructed from that port's metadata address. No family
is reinterpreted as another family's F030 block. CW32L012 has a compressed
GPIOF address, so a metadata-generated address match replaces STM32's uniform
stride formula.

The source PAC retains its original GPIOC/F/B types and its register inventory.
Before generating a shared view, the build script checks every used port's
register offset, transaction width and access permission against that family's
GPIOA view. It also checks each accessed pin/AF field's position and width,
and the lock-key field. Different unused fields and registers are not treated
as interchangeable. A PAC value contains an address; this does not create a
Rust reference to a differently sized register struct.

The HAL only exposes the checked subset: invalid pins cannot be constructed
through safe singletons, unsafe `AnyPin::steal` retains family validity checks,
and absent AF halves are unreachable for those valid pins. Original PAC
metadata and curated register files are unchanged.

## Generated data and real differences

- Port addresses, gate masks and AF field width come from the selected PAC
  metadata. Assertions reject unsupported or inconsistent register views.
- Family pin validity is the union of output-capable pads in the existing,
  independently reviewed `cw32-data/pinouts/*.json`. This preserves all former
  `valid_pin` masks, including RF/input-only exclusions. Safe singleton
  generation still uses the selected package's metadata and existing special
  pad exclusions.
- AF fields are three bits on F002/F003/L010/L011 and four on the remaining
  IPs. Three-bit writes preserve the reserved nibble bit. F030/A030 and L012
  retain a selector bound of 15; other families retain the documented bound 7.
- Only F030/A030/F020 access SPEED. Only those IPs and L052/L083 unlock the pin;
  L083's real PAC spelling remains LCKR, versus LOCK on the other supported IPs.
- Level interrupt enables exist on F030/A030/F020/F002/F003/L052/L083; edge-only
  IPs never access HIGHIE/LOWIE.
- L010/L011 have no supported pull-down. L010 GPIOB's vendor PDR declaration is
  not used. L012 permits PDR access only for PF3. These manual/SDK-backed
  restrictions intentionally narrow overbroad vendor declarations.
- L010/L011/L012 require the AHBEN key. L012 GPIOF is bit 7; other F banks use
  bit 9. Gate bits are extracted from reviewed RCC metadata. The existing
  bounded L010/L011 gate-readback handshake is retained.

The machine-readable per-family/port facts, source paths, upstream hashes and
final code hashes are in `gpio-shared-inventory.json`. Original vendor
manuals, SDK archives and extracted source text are not bundled. Their source
identity and acquisition links remain in `sources/evidence-sources.json`
and its generated `reference-index.json`.

Existing own-manual/SDK evidence is reused rather than reissued:
- F020: `f020-backend-compatibility.md`.
- F002/F003: `f002-f003-serial-read-only-audit.md` and the GPIO entries in
  `gpio-async-evidence.json`.
- L010/L011: `l010-l011-hal.md`, `l010-l011-hal-evidence.json`, and
  `l011-manual-follow-up-audit.md`.
- L012: `l012-gpio-clock-evidence.md`, including UM §9.6.3's PF3-only PDR.
- L031/R031/W031: `l031-r031-w031-hal.md`.
- L052/L083: `l052-l083-gpio-clock-evidence.md` and
  `l052-l083-serial-gpio-review.md`.
- IRQ register/flag semantics across the families: `gpio-async-evidence.json`
  and `gpio-isr-access-corrections.md`.

Those documents preserve historical implementation and validation descriptions;
this refactor's file paths and current verification boundary are recorded here.

## Behavior retained

One input/output/analog/AF sequence now serves all families. Disconnect disables
output before changing analog/pulls/enables. Pull changes disable both pulls
before enabling one. AF changes select the mux and output type before output
is enabled. Output constructors still preload the atomic latch command.
All configuration RMW operations remain within critical sections and preserve
neighboring pins. BSRR/BRR/TOG remain dedicated atomic writes.

Pin ownership, public Pull/Speed availability, special-pad exclusions, AF
route traits, constructor/drop behavior and clock sharing are unchanged.
L012 invalid pull-down requests are rejected before a mode change; a
constructor may already have disconnected its owned pad, as before.
EXTI source is unchanged, including cancellation, one-shot wait state, shared
bank dispatch, active-mask checks and documented W0C no-op masks.

## Verification boundary

An independent source review checked 13 family masks, 50 ports, 760 used
register views and 7,233 pin fields. It found no blocking discrepancy between
the shared implementation, the frozen prior backends and source register IR.

The isolated implementation passed 12 representative-family ARM checks and
26 release-library builds before that matrix was deliberately stopped to avoid
duplicating the final combined 108-build matrix on the flattened main tree.
Both existing CW32F030C8T7 examples, `blocking` and `async_gpio_edge`, linked
successfully as ARM release ELFs. Frozen hashes and logs are under
`verification-logs/gpio-shared/`; that receipt explicitly records the partial
isolated matrix. The final combined-main receipt is the authority for final
all-54-feature coverage. Firmware was not flashed or run; source and compile
checks do not establish electrical behavior or silicon validation.
