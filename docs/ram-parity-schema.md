# Independently authored mixed-register field access projection

The pinned upstream chiptool IR describes register access but has no field
access property. The existing independently authored chip metadata schema and
its upstream provenance remain unchanged. `cw32-data-serde::register_write::FieldAccesses` is a
small original sidecar schema, authored for the RAM audit, not copied from a
vendor, chiptool or the Embassy metadata schema.

`cw32-data/field-access.yaml` maps canonical register versions to read-only
fields in mixed read/write registers. Every entry supplies block, register,
fieldset, field, expected bit offset/width and nonempty source citations.
Unknown properties are rejected by serde. No optional general-purpose raw flag
or backend adapter is added to the HAL.

The data generator requires the named register to remain read/write, the
fieldset to match, and the exact field span to match. It rejects duplicate
restrictions, missing fields, arrays and absent evidence. The normalized
`cw32-data/data/field-access` projection carries only the selected version.
The PAC generator uses chiptool's own name sanitizer, parses generated Rust,
removes exactly one setter in the correct `regs` fieldset implementation, and
requires exactly one getter to remain. It fails if those guards no longer hold.
Generated field value representations and writable neighboring fields retain
the upstream layout and behavior.

The initial entries restrict only classic RAM IER.EN. Read-only whole registers
continue to use normal IR `Read` access. RAM ICR's separate existing command-seed
schema supplies its R1W0 no-op. Schema round-trip/rejection fixtures, source-only
projection comparison and positive/negative ARM PAC compilation cover these
boundaries without exercising a hardware address.

The reviewed `cw32-data-serde/src/lib.rs` file remains byte-identical to the
Stage14 independent replacement. New access records are additive to the
previously project-authored command-semantics sidecar module; no prior type or
wire representation is modified. No existing provenance approval is advanced.
