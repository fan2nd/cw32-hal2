# HAL capability inventory

`tools/hal-capabilities.yaml` records current implementation scope and limits.
`tools/update-functional-coverage.py` combines it with generated chip data to
produce `build/reports/hal-functional-coverage.json` and `hal-coverage.json`.
Reports are generated outputs, not hardware authorities. Local execution is
subject to the current no-test/no-build policy.

## What is declared and what is derived

`cw32-data/inputs`, register YAML and hardware sidecars remain canonical. The
normal data generator combines those with SHA-pinned official SVD inputs.
Coverage reads all declared HAL chip selections' normalized chip files for
hardware instances, register layouts, qualified signals and hardware limits.
It checks consumed ADC sequence/classic-scan, IR, RTC alarm, ATIM complementary
and HSE/HEX family projections against current authored YAML. HEX equality covers
the complete electrical facts from `cw32-data/hex-qualified.yaml`, including
absence on unqualified families. Regenerate all hardware
after changing any input: these focused checks are not full regeneration proof.

`tools/hal-capabilities.yaml` explicitly declares the reviewed implementation
scope. It is authored source, not a generated report. Each function has:

- `register_kinds`, or an explicit virtual-function inventory for IR, the
  reserved Embassy timebase, or the two user-deferred radio subsystems
- driver/source `evidence` with optional literal anchors; missing paths or
  anchors reject generation
- mode IDs, status, scope and limitations, optionally restricted by family,
  family/peripheral overrides, required package signals or register presence
- an explicit HAL pad exclusion policy matching `build.rs::safe_pin`, separate
  from hardware routing facts

Per-mode overrides may select named `peripherals` within their listed families;
selectors must match the generated instance inventory. L012 ADC1 and ADC2 retain
separate overrides: ADC1 uses its dedicated vector, while ADC2 requires checked
DAC-peer admission and its own shared-vector fault policy.

The validator rejects new undeclared hardware kinds, changed family membership,
duplicate modes/routes, unknown declaration keys/families and stale anchors.
Trigger declarations also require a known status, nonempty scope/limitations
and source evidence; implemented routes require a HAL driver source reference.
Review the declarations and pad policy whenever APIs, routing policy or source
interpretation changes. An existing Rust file, anchor, feature, cfg or successful
build is never a functional-completeness test. Anchor checks only locate the
maintainer's evidence; they cannot prove its interpretation is correct.

All source-declared implementations are bounded `partial` or
`experimental_partial`. `pac_only` is a peripheral summary with registers but
no implemented mode. `unimplemented` describes an absent API/integration.
`source_disputed`, `route_unqualified` and `deferred_by_user` are distinct.
`hardware_absent` applies to a missing block or a specifically checked
register-controlled mode, such as a family without a PLL register.

`mode_hardware_present: null` means that mode's exact hardware presence has not
been fully audited; it must not be read as either supported or absent.
Register presence does not establish every advanced mode. No standalone BGR
register does not imply no internal bandgap. Pin availability is reported per
chip selection after the maintained HAL pad exclusions: family aliases use
common-package intersections and are never treated as the union of packages.
UART full RTS/CTS requires TX, RX, RTS and CTS; transmit-only CTS requires TX
and CTS; receive-only RTS requires RX and RTS. Required signals are necessary,
not sufficient proof of a simultaneously usable
board/pin combination. Timer reservation, shared owners and electrical limits
still apply.
