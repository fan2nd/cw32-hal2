# Timer command and counter-field metadata

The timer metadata review adds source-backed ICR command seeds to all 12
canonical BTIM/GTIM variants and removes only the CNT.UIFCPY setter from the
four buffered variants. CNT[15:0] remains readable and writable; UIFCPY remains
readable. The evidence covers each of the 13 consuming families independently,
including L010 and L011 despite their shared canonical register representation.

Exact original-manual URLs, printed revisions, PDF SHA-256 values, section and
printed/PDF page numbers, and freshly extracted page hashes are in
[timer-command-evidence.json](timer-command-evidence.json). Original documents
remain external. The source lock records these evidence links without changing
any source pin or redistribution policy.

| IP | ICR reset/no-op word | Defined W0C mask | Reserved bits retained as one |
| --- | --- | --- | --- |
| Classic BTIM | `0x00000007` | `0x00000007` | None |
| Buffered BTIM | `0x00000041` | `0x00000041` | None |
| Classic GTIM | `0x000003ff` | `0x0000027f` | Bits 7 and 8 (`0x180`) |
| Buffered GTIM | `0x00f01e5f` | `0x00f01e5f` | None |

ICR writes zero to clear and one to preserve a defined flag. Build a command
with the generated `regs::Icr::write_noop()`, change only intended fields with
the typed setters, and issue one direct `icr().write_value(command)`. A false
value clears the selected flag. Do not use a zero-default `.write(...)` closure,
read-modify-write, or an unrestricted complement of ISR. The no-op seed retains
documented reserved defaults, including classic GTIM bits 7 and 8. The generated
`reset_value()` is explicit and does not change chiptool's zero `Default`.

The existing schema-version-1 overlays already express the entire change:
`ReadOnlyField` in `field-access.json` implies read-only access, and
`RegisterWrite` in `register-writes.json` carries reset/no-op words and the list
of W0C fields. No new required or optional schema field, generator change, or
Rust schema change was introduced.

Authored register YAML, normalized chiptool IR, canonical reuse hashes and
source-version mappings are unchanged. The review records and validates their
exact existing identities instead of changing the whole-IR reuse ledger for
semantics represented outside that IR. GTIM and BTIM EGR access is unchanged;
their own manuals describe different access. Existing independent replacement
and upstream ancestry records are preserved byte-for-byte.

`python tests/verify_timer_commands.py` verifies the original PDF hashes and
fresh page extraction, family-to-variant mappings, masks, reserved defaults,
overlay projections, generated typed seeds, remaining CNT writes and UIFCPY
reads, and all unrelated overlay records. Add `--compile` for compile-only
checks of the actual 12 generated PAC modules and four exact E0599 failures for
forbidden UIFCPY setters. These checks do not execute firmware, access MMIO,
model the HAL, or establish silicon behavior.
