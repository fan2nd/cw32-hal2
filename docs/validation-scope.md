# Validation scope

Local tests and builds are prohibited under the current work policy. No CI is
configured. These manual entry points remain available for later authorization:

- `./d test`: generator, schema and data/PAC contracts.
- `./d audit-current`: official source facts, projections and distribution checks.
- `./d check`: regeneration, determinism and chip-feature builds.
- `./d lint`: module layout.

Source audits, compilation and hardware validation are separate results.
Missing evidence or a failed command is not a pass. Software checks do not
establish electrical behavior, runtime timing or cancellation safety.
