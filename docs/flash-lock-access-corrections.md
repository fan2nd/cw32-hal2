# F002/F020 FLASH protection-map corrections

Own-manual page images establish that FLASH_PAGELOCK bits15:8 are reserved on
F002 and F020. Both expose only LOCK0..7 plus the keyed bits31:16. Their vendor
CMSIS/SVD definitions contain extra lock fields, so import equality alone did
not justify the earlier complete-register reuse.

- F002 UM CN1.4 §7.9.3, printed96/PDF97: remove LOCK8/9 from
  `flash_cw32f002_v1`. F003 genuinely retains ten lock fields; its unchanged
  complete IR is restored as `flash_cw32f003_v1`.
- F020 UM CN1.4 §7.9.3, printed119/PDF120 and §7.6.1, printed113/PDF114:
  remove LOCK8..15 into `flash_cw32f020_v1`. F030/A030 retain the unchanged
  sixteen-field `flash_v1` map. Current F020 DS CN1.3 table6-1 corroborates the
  32-KiB main-flash range; the SDK's copied page127/0xffff operations are not
  authority to expose a larger region or extra protection fields.

Exact manual URLs, SHA256 values, field positions and original/corrected IR
hashes are in `flash-lock-access-corrections.json`. Profile `field_removals`
guard each removed field's original name, bit offset and width. Source refresh
fails rather than silently applying the fix to a changed vendor definition.
The exact-reuse ledger remains252 source templates and becomes135 canonical
maps after the two necessary splits. No approximate semantic merging is used.

No clock WAIT, controller mode, status, address, capacity or unrelated field was
changed. Existing HAL clock initialization uses the preserved WAIT fields, not
these protection bits. No FLASH programming driver is enabled by this correction.
Raw register access remains the caller's responsibility; the correction removes
incorrect named getter/setter APIs rather than claiming to forbid raw bit writes.

`tests/check_flash_lock_fields.py` checks own PDF hashes, source lineage,
field-only changes, guarded profiles and all seven affected chip selections.
Ten positive PAC controls retain valid locks/key and the F003/x030 upper fields;
76 expected E0599 failures reject every removed getter/setter across the affected
features. These compile-only checks execute no device MMIO.
