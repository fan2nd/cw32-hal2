# Source-only HAL capability inventory

Run from a source checkout with Python 3.11+, PyYAML and the repository's Rust
host toolchain:

```
./d fetch-sources       # verifies cached official SVDs, or acquires pinned inputs
./d gen                # regenerate normalized hardware from the authored inputs
python3 ci/update-functional-coverage.py
python3 ci/update-functional-coverage.py --check
```

`./d gen` builds only the host data generator. Generating this inventory does not
require a HAL build matrix, generated PAC, old compiler receipts, test counts,
`docs/hal-verification-current.json`, or either historical coverage report.
`ci/update-hal-coverage.py` is a compatibility entry point for the same operation.
`--validate-only` checks declarations and consumed source data without writing.

The two deterministic outputs are local build products:

- `build/reports/hal-functional-coverage.json`: family, function, peripheral
  instance and mode rows; actual scope, limitations, evidence, package signal
  availability, selected hardware facts and input hashes
- `build/reports/hal-coverage.json`: per-instance mode/status summary pointing
  to the detailed inventory

`build/` is ignored and excluded from source packages. The package retains the
hand-maintained `ci/hal-capabilities.yaml` declarations. Neither output is a new
source of hardware facts. Old stage reports and old `docs/*coverage*.json` are
historical snapshots, not current capability authorities.

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

`ci/hal-capabilities.yaml` explicitly declares the reviewed implementation
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
selectors must match the generated instance inventory. L012 ADC1 support therefore
cannot silently mark ADC2 implemented.

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

## Current distinctions that must survive updates

- Ordered ADC scans exist on all thirteen families: 1–4 slots on
  F002/F003/F020/F030/A030 and 1–8 on the others, including both L012 ADCs.
  Classic scans share sample time and exclude buffered/internal multi-slot use;
  L010/L011/L012 provide per-slot sample times. All classic lines, L010/L011 and
  L012 ADC1 additionally support finite software single reads and ordered scans
  with typed IRQ completion. L012 ADC2 remains blocking: its shared ADC2_DAC
  interrupt needs a separately owned lifetime and terminal-fault policy. See
  [L010/L011](adc-low-async.md) and [remaining async ADC scope](adc-remaining-async.md).
  Continuous scans, hardware-triggered scans and safe ADC DMA remain absent.
- UART blocking/async and qualified RTS/CTS constructors exist. Pin availability
  differs by instance and exact selection; no software FIFO or throughput claim.
  F030/A030 additionally have safe staged TX DMA, finite-chunk RX and combined
  split full-duplex DMA with ordinary caller slices and separate wire flush.
  Static staging/channel/UART ownership is retained across cancellation; clean
  completion permits reuse and retained RX tails, while DMA errors permanently
  quarantine resources. FE/PE reports promptly but recovery awaits the old count.
  RX gaps can lose frames without overrun detection. No DMA RTS/CTS, continuous
  RX, safe early abort, bounded liveness or zero-copy guarantee.
- F030/A030 SPI additionally supports a safe staged byte async bus with private
  TX/RX SRAM and two admitted DMA channels. Both clean terminals plus wire idle
  precede success. Cancellation requires preserving the original CS selection
  through a successful flush; errors permanently retain the pair. No generic
  cancellation-safe device adapter or lossless-throughput claim follows.
- GTIM polling capture/encoder exists across families. ATIM capture/encoder,
  complementary pairs, constructor-only symmetric dead time and software BK1
  are limited to L010/L011/L012. External BK1 is qualified on L011/L012 only.
  Classic ATIM advanced modes remain open; no output-safety guarantee is made.
- RTC Alarm A and A/B event observation exist on all eleven RTC families.
  Async waits are limited to L010/L011/L012. Alarm B mask programming remains
  source-disputed; observing Alarm B flags does not resolve its polarity.
- L012 IR attaches while preserving its disputed MOD selector. R031 IR has no
  qualified output pin because its only documented route is debug PA13.
- Direct crystal/bypass HSE is qualified on F020/F030/A030/L031/R031/W031/L083
  where exact-package oscillator routes are qualified. L031 QFN20 and its
  package-less alias have no qualified HSE route; all five modeled exact L083
  packages have PF0/PF1. Each family retains its own electrical, fallback,
  peripheral-owner and initialization constraints; see the [L031/R031/W031](qualified-l031-hse.md)
  and [L083](qualified-l083-hse.md) contracts. The same external-high-speed
  inventory mode has a separate F002/F003 family override
  for direct digital HEX on qualified PB0/PB1. Its board-guaranteed actual
  4..32 MHz interval, 1.65..5.5 V supply, -40..105 C ambient, waveform/level
  limits, retained-HSI/bus limits and stable quiescent entry all apply together.
  Retained AWT reserves its external pad independently; active external AWT
  admits only exact ready reuse, and active ETR rejects new HEX requests.
  Initialization publishes frozen whole-boot reservations; failure after
  acquisition requires reset. Digital HEX adds no crystal drive, PLL, runtime
  switching, sleep/resume or fault recovery. STABLE is a startup latch; clock
  loss can stop the CPU, so no automatic fallback or wall-clock timeout is
  guaranteed. See [the complete HEX contract](qualified-hex.md).
  Other external clock backends remain separate gaps. HEX, LSE and PLL register
  availability is read from each family's register YAML and does not establish
  implementation support.
- Inherited LSE pad ownership is protected on all eleven LSE-bearing families,
  independent of HSE qualification. Crystal, bypass, family-specific pad locks
  and package bond-outs determine the boot-retained reservation before safe pin
  construction. Active LSE crystal/bypass setup and an owned calendar source
  are qualified only for eight exact x030/F020/L031/R031/W031 packages. All require
  explicit board electrical and every-cycle frequency bounds around nominal
  32768 Hz. Cold startup rejects retained RTC state, including compensation
  consumers independent of RTC.SOURCE; exact already-enabled reuse preserves
  configuration. Poll-budget exhaustion retains the source and reservations
  and requires reset. There is no automatic calendar fallback, low-power
  restoration or post-fault elapsed-time guarantee. See the
  [active LSE contract](qualified-lse.md), [ownership contract](inherited-lse-pads.md) and
  [qualified LSE examples](../examples/lse-clock/README.md).
- L010 and L011 each implement exactly two authored BTIM1 UPDATE routes:
  BTIM2 TRGI cascading and ADC START_CONVERSION through a separate one-shot
  ADC owner. No periodic or lossless scan, immediate pipeline drain, atomic
  cascade cutoff, fanout or trigger-rate guarantee is declared. Every documented
  route is enumerated individually; newly added routes default to unimplemented.
  Other routes remain a per-destination gap, not a universal trigger selector claim.
- L010/L011 VCDIV owns one immutable reference bank for VC1/VC2. L012 VC12REF
  owns the VC1/VC2 bank and VC34REF owns the VC3/VC4 bank. The comparator's
  internal-divider-input mode retains a typed bank-qualified borrow and an owned
  package-qualified positive input. Both consumers may borrow an unchanged bank;
  comparator and reference board supply declarations must match.
  Construction returns InUse for an enabled paired consumer selecting the bank,
  including a forgotten driver. Reference Drop retains the divider and shared
  VC gate; comparator Drop only disables its own instance. Held reset is refused;
  no separate divider kernel clock/reset, reconfiguration or disable API is claimed.
- Divider sources are VDD on L010, VDDA on L011/L012, or nominal 1.6 V Vcore,
  with exactly eight taps from 1/8 through 8/8. VDDA must equal VDD; supply bounds
  are 1620..5500 mV on L010 and 1700..5500 mV on L011/L012. L012 inherited DIV
  codes 8..15 are refused before reference writes, preserving the disputed high
  bit. No calibrated precision, divider-ready flag or guaranteed settling is
  claimed; low-family comparator Readiness remains Unknown. This internal
  comparator API leaves the ADC external-reference gap unimplemented. Fixed
  BGR/DAC input and advanced comparator routes remain separate gaps. See
  [reference ownership and evidence](comparator-reference.md).
- DMA on F030/A030/L083 has unsafe borrowed software copies, unsafe-entry
  owned static SRAM copies and safe init-admitted CopyChannel copies.
  L083 requires selected-part SRAM metadata and adds
  software copies only; hardware-request constructors and routes remain x030-only.
  Entry requires hardware exclusivity, including error and forgotten transfers.
  Only clean recorded TC without TE, STATUS=5 and SOFTSRC=0 returns owners.
  Errors permanently reserve resources, forgetting leaks them, and Drop may block
  forever. Safe init-admitted copies require normal reset/clean-runtime entry;
  dirty startup rejection cannot repair an arbitrary active bootloader handover.
  Safe peripheral integration covers staged x030 UART TX/RX and paired SPI
  master. RX gaps can lose frames; safe early abort, circular operation and
  lossless reception remain unavailable. Other DMA-bearing families
  remain PAC-only. See [the DMA ownership contract](dma-owned-copy-evidence.md),
  [safe copies](dma-safe-owned-copy.md), [UART TX DMA](uart-dma-tx.md),
  [UART RX DMA](uart-dma-rx.md) and
  [SPI DMA](spi-dma.md).
- The optional Embassy time driver reserves the whole GTIM/GTIM1 plus IRQ and
  requires continuous run mode and service strictly within 32768 actual ticks.
  HSI budgets are <31.207619 ms at ±5% or <32.125490 ms at ±2%; qualified HSE uses
  its own bounds. Flash stalls, debugger stops and deep sleep can lose elapsed
  time. These are required bounds, not measured latency guarantees.
- RF remains explicitly user-deferred on R031/W031.

Every row has `hardware_validated: false`. This tool performs no compilation,
board execution, electrical validation or automatic semantic proof. No
completion percentage is calculated. Source, build and runtime evidence remain
separate, and a complete advanced-mode hardware audit remains open.
