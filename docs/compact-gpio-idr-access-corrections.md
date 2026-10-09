# F002/F003 GPIO input access

Both own-family manuals, section 8.6.14, mark GPIOx_IDR at offset 0x50
read-only for x=A,B,C. F002 CN V1.4 uses printed page 113/PDF page 114;
F003 CN V2.3 uses printed page 115/PDF page 116. The
[source receipt](compact-gpio-idr-access-corrections.json) records exact source
identities, bank-specific SVD declarations and before/after canonical hashes.

The selected F002 SVD marks IDR and every field RW. The selected F003 SVD
also marks each IDR register RW, but its GPIOA fields are explicitly RO and
GPIOB derives GPIOA. Its separate GPIOC fields remain RW. Consequently F003
GPIOA/B has both a register-container/manual disagreement and field access
information lost in the existing projection. The other banks have a direct
SVD/manual disagreement at both levels.

This correction sets only the IDR register access to Read in the shared GPIO
and GPIOC authored YAML. Four guarded input overrides enforce the original
ReadWrite container access and retain independent own-family evidence.
Normalized reuse hashes change atomically with the authored sources; the
F002/F003 source membership remains unchanged. Generic SVD field-access
preservation is separate and is not enabled here.

The generated accessor becomes Reg<Idr, R>, removing write, modify and
write_value eligibility. Existing read/getter behavior, 32-bit bus width,
offsets, value field setters, other registers and methods, bank mappings and
pin inventories remain unchanged. In particular, GPIOC keeps its existing
PIN0..5 fields. HAL input and trigger sampling already use read(). There is
no HAL implementation change or hardware-validation claim.

Run the source proof with:

```sh
python3 tests/check_compact_gpio_idr_access.py --source-only --sources /path/to/evidence
```

After ordinary data/PAC regeneration, omit --source-only. Use --compile for
two ordinary PAC compile controls and 18 forbidden IDR register-access cases.
With --baseline /path/to/pre-correction/tree, the check also requires each
affected generated peripheral file to differ only in the IDR access marker.

The earlier GPIO ISR receipt remains a dated access-only snapshot. Its test
checks the compact templates' current ISR contract without chaining inverse
edits of later access corrections.
