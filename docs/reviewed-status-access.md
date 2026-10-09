# Reviewed oscillator access promotion

Exactly 23 further `SYSCTRL.STABLE` fields are read-only through the existing
`cw32-data/field-access.yaml` API restriction: HSI/LSI/PLL on x030 and F020;
HSI/HEX/LSI on F002 and F003; HSI/LSI on L010, L011, the shared L031/R031/W031
map and L083; and HSI/LSI/HSE on L052. These are 30 family occurrences.
The 18 existing SYSCTRL restrictions and all other field-access rules remain
unchanged. The total is 41 normalized SYSCTRL STABLE restrictions.

The original SVD field access and each family's own manual agree on RO.
This fixes our field-access projection loss, not vendor errata. All 25 relevant
manual table pages were rendered and visually inspected during the independent
review. x030 has an explicit shared manual/source relationship; L031, R031 and
W031 retain separate own-manual and SVD bindings despite their reused map.

The [compact reviewed selection](reviewed-status-access-selection.json) records
the original independent selection/evidence hashes, exact field and register
spans, all 30 manual page/section bindings, source IDs/URLs/hashes and the exact
generic capture input hashes. It is review evidence, not a second active hardware
model. Only `field-access.yaml` drives normal generation. Its new rule identities
and bit spans were emitted from the actual captured rows after exact agreement
with that selection, current input profiles, source lock and curated registers.

No register YAML, normalized IR, reuse mapping/hash, runtime code or schema
changes. Register byte offsets remain HSI 0x18, HEX/HSE 0x1c, LSI 0x20 and PLL
0x28. All containing registers remain 32-bit RW, all getters and configuration
setters remain, and only the selected 23 `set_stable` methods disappear. This
field API restriction does not sanitize whole-register writes or change hardware
behavior. The rest of the captured corpus is not activated.

## Reproduce the bounded promotion

Run the generic importer separately per family into `CAPTURES/<FAMILY>` using
`cw32-data-gen --import-registers --manifest cw32-data/inputs/<family>.yaml`.
Pass `--root "$PWD" --out-dir "$CAPTURES/<FAMILY>"`. This retains
each family's original access capture independently of shared-map conflicts.
Use the exact locked source inputs. Then prepare an external reviewable YAML:

```sh
python3 cw32-data/tools/promote_status_access.py \
  --capture-root "$CAPTURES" --output /tmp/field-access.candidate.yaml \
  --receipt /tmp/status-promotion-receipt.json
```

The helper checks all 30 real candidate rows, including explicit field RO
origin, projection, span, register offset, absence of array/command semantics,
own-family source bindings and complete map consumers. It appends only the 23
reviewed rules and preserves existing YAML bytes. Output and receipt paths must
resolve outside the source and capture trees. Existing outputs survive failed
validation. The helper never edits the authored access file; applying its
reviewed candidate is a separate source change. It is idempotent once adopted.
Omit `--output` to verify that the current active rules equal the captured
selection. The optional receipt binds exact selection, parser/helper, capture
rows and before/after rule-file hashes; it is not a generation dependency.

Current source facts can be checked without any historical archive:

```sh
python3 tests/check_reviewed_status_access.py --sources "$CW32_SOURCES"
./d gen-all
python3 tests/check_reviewed_status_access.py --sources "$CW32_SOURCES" --generated
# Use the allocated target directory and ordinary PAC consumers only:
CARGO_TARGET_DIR=/path/to/reserved-target CARGO_BUILD_JOBS=1 \
CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 \
python3 tests/check_reviewed_status_access.py --sources "$CW32_SOURCES" --compile
```

The current fact check rehashes original SVDs and manuals, checks the exact XML
declarations and manual RO rows, and verifies the generated sidecars and all 41
getter/restriction/RW-container contracts. Compiler controls cover all 12 affected
families: getters, every remaining configuration setter, and whole-register
write/modify/write_value remain usable; each of the 30 selected family setter
calls must fail specifically with E0599. These are data/PAC checks, with no HAL
test, firmware execution or physical hardware claim.
