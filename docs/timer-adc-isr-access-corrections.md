# Timer and ADC ISR access corrections

2026-10-08. This bounded correction follows the [ADC/GTIM audit](adc-timer-next-batch-audit.md).
It changes only documented ISR access and exact canonical reuse. It does not
implement ADC or timer drivers for additional families.

## Corrected own-family sources

Every defined field in the following interrupt status registers is read-only.
Flag clearing uses the separate ICR register. Imported SVDs incorrectly inherit
read/write access; each input override asserts that exact old access and carries
its own manual's URL, SHA-256, section and both page numbers.

| Family | Register | Own manual | Printed / PDF page | Instances |
| --- | --- | --- | --- | --- |
| F002 | GTIM.ISR, 0x318 | CN V1.4, 12.7.12 | 182 / 183 | GTIM |
| F003 | GTIM.ISR, 0x318 | CN V2.3, 12.7.12 | 184 / 185 | GTIM |
| F020 | GTIM.ISR, 0x318 | CN V1.4, 14.8.12 | 246 / 247 | GTIM1–4 |
| L052 | GTIM.ISR, 0x318 | CN V1.5, 15.8.13 | 277 / 278 | GTIM1–3 |
| L083 | GTIM.ISR, 0x318 | CN V2.0, 15.8.13 | 289 / 290 | GTIM1–4 |
| L012 | ADC.ISR, 0x78 | CN V1.4, 25.12.9 | 597 / 623 | ADC1–2 |

The L012 PDF has 26 front-matter pages. Its printed and PDF page numbers must
not be derived using another family's offset.

Exact URLs, hashes, original SVD lineage, old/new canonical identities and
before/after YAML hashes are in [the evidence record](timer-adc-isr-access-corrections.json).
F002 and F003 share the corrected template, but each has independent source
proof and its own provenance override. Register width, fields, offsets, reset
values, clear semantics and reserved masks are unchanged.

## Exact deduplication

Correcting just ISR access makes the complete normalized IR equal in two cases:

- F020 `gtim_cw32f020_v1` equals existing `gtim_v1`.
- L083 `gtim_cw32l083_v1` equals existing `gtim_cw32l031_v1`.

Both equalities include descriptions, names, all register/field offsets and
widths, and access. Corrected YAML bytes also equal the retained canonical
source. The removed templates' source-version identities are retained in the
reuse ledger. Only the corresponding F020/L083 input version selections change.
The ledger now has 133 canonical templates for the same 252 source templates.
This raw-IP identity does not grant HAL compatibility or safe pin exposure.

Normal generation still asserts curated access; it never patches the authored
YAML. Import mode requires the old SVD access to match before applying the
reviewed correction. No generator or failed-refresh boundary was weakened.

## Checks completed

Run the focused proof with:

```sh
python3 tests/check_timer_adc_isr_access.py --manual-dir /path/to/acquired/manuals
```

`--source-only` omits generated/PAC checks and can run before regeneration.
The normal proof is included in `./d test`.

After one coordinated successful `./d gen-all`:

- Six own-family manual hashes, register headings, original SVD instance lineage
  and exact overrides pass.
- Five access-only YAML snapshots and two exact canonical merges pass.
- All 24 affected family/package chip records select the expected read-only
  register and canonical identity.
- Six positive Cargo controls permit ISR reads and ordinary control-register
  writes/modifies.
- Thirty isolated Cargo negative cases reject ISR `write()` and `modify()` with
  exactly E0599 on the intended expression. Toolchain or unrelated build failures
  cannot count as a successful negative test.
- All 13 families pass independent source/generated parity; 19 metadata contracts,
  including every canonical IR ledger hash, pass.
- Data validation and PAC inventory pass: 54 chip features, 1,845 peripheral
  instances, 1,513 interrupt entries, 133 register templates.
- All seven generation-boundary tests pass, including rejected curated-directory
  aliases/symlinks and vendor refresh isolation.

Logs are under [verification-logs/isr-access](verification-logs/isr-access/).
These checks do not execute firmware or MMIO and do not validate silicon.
L011's separately confirmed GPIO ISR correction is included in the companion
[GPIO correction report](gpio-isr-access-corrections.md).
