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
  F030/A030/L083 additionally have safe staged TX DMA, finite-chunk RX and combined
  split full-duplex DMA with ordinary caller slices and separate wire flush.
  Static staging/channel/UART ownership is retained across cancellation; clean
  completion permits reuse and retained RX tails, while DMA errors permanently
  quarantine resources. FE/PE reports promptly but recovery awaits the old count.
  RX gaps can lose frames without overrun detection. No DMA RTS/CTS, continuous
  RX, safe early abort, bounded liveness or zero-copy guarantee.
- F030/A030/L083 SPI additionally supports a safe staged byte async bus with private
  TX/RX SRAM and two admitted DMA channels. Both clean terminals plus wire idle
  precede success. Cancellation requires preserving the original CS selection
  through a successful flush; errors permanently retain the pair. No generic
  cancellation-safe device adapter or lossless-throughput claim follows.
- GTIM polling capture/encoder exists across families. Buffered ATIM on
  L010/L011/L012 supports capture/encoder, complementary pairs, constructor-only
  symmetric dead time and software BK1. ATIM capture/encoder remains limited to
  those three families; external BK1 is qualified on L011/L012 only.
  F030/A030 classic ATIM also supports optional complete A+B pairs, interior
  comparison duty, fixed symmetric dead time and explicit global MOE. It excludes
  duty endpoints, per-pair gating, brake/rearm, inversion and runtime timing
  changes; other classic advanced modes remain unqualified. No pad-level or
  shutdown safety guarantee is made. See [the classic contract](classic-atim-complementary-evidence.json).
- RTC Alarm A and A/B event observation exist on all eleven RTC families.
  Async waits are limited to L010/L011/L012. Alarm B mask programming remains
  source-disputed; observing Alarm B flags does not resolve its polarity.
- L012 IR attaches while preserving its disputed MOD selector. R031 IR has no
  qualified output pin because its only documented route is debug PA13.
- Direct crystal/bypass HSE is qualified on F020/F030/A030/L010/L011/L012/L031/
  R031/W031/L052/L083 where the selected package's oscillator routes are qualified.
  L031 QFN20 and its package-less alias have no qualified HSE route. L010/L011
  cover all five modeled exact packages; L012 covers both exact packages and their
  common alias; L052 covers its three and L083 its five modeled exact packages.
  Each family retains its own electrical, fallback, peripheral-owner and
  initialization constraints; see [L010/L011](qualified-l010-l011-hse.md),
  [L012](qualified-l012-hse.md), [L031/R031/W031](qualified-l031-hse.md),
  [L052](qualified-l052-hse.md) and [L083](qualified-l083-hse.md). The same external-high-speed
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
  These are bounded one-time source modes; runtime switching, low-power
  restoration and source-loss recovery remain outside their contracts.
- CW32F002F3P7/F3U7 qualify init-only factory-LSI SYSCLK with their own
  31,160–34,440 Hz rate bounds and 16 KiB Flash / 2 KiB SRAM. Generic F002
  remains excluded; see the [F002 contract](f002-factory-lsi-sysclk.md).
- CW32F003F4P7/F4U7/E4P7 qualify the same init-only selection using their
  own 31,816–33,784 Hz (±3%) factory bounds and 20 KiB Flash / 3 KiB SRAM.
  Generic F003 remains excluded. Both exact-package groups have no RTC API
  or dedicated LSI_OUT and inspect AWT, UART1/2, whole GPIOA/B/C, MCO and
  ready/NVIC observers. Whole-bank sampling, filters and events may advance
  before failure; restoring gates cannot undo that progress. F003 ATIM/IR
  dependencies add no direct LSI root or inspection gate; unrelated gate/reset
  bits are preserved. ADC rejects rate-only timing, and the fixed 1 MHz time
  driver rejects selected LSI before singleton acquisition or RCC MMIO.
  AWT retains independent HSIOSC timing. No rollback or hardware validation
  is promised; see the [F003 contract](f003-factory-lsi-sysclk.md).
- CW32L031C8T6/C8U6/F8U6 add init-only factory-LSI SYSCLK using their own
  31,816–33,784 Hz bounds at 1.65–5.5 V and −40–85°C, with 64 KiB Flash
  and 8 KiB SRAM. Matching cold trim still needs the complete nine-gate,
  eleven-selector admission. Whole-GPIOA/B/C/F inspection can advance events
  even before failure. PB11 AF is conservatively inspected on unbonded F8U6;
  documented AWT-overflow FILTER7 is conservatively rejected. Configurable
  CCS and inherited HSE/LSE ownership remain distinct from classic policy.
  All RTC LSI aliases become rate-only under every SYSCLK on these exact
  three parts. Generic/other L031 stay unchanged; board LSE
  retains its independent qualification. ADC and the fixed 1 MHz time driver
  reject selected LSI. See the [L031 contract and separate implementation
  evidence](l031-factory-lsi-sysclk.md).
- CW32R031C8U6 alone adds factory-LSI SYSCLK with own 31,816–33,784 Hz
  bounds at 2.2–3.6 V and −40–85°C, QFN48, 64 KiB Flash and 8 KiB SRAM.
  Its own-source admission reuses the unchanged nine-gate/eleven-selector
  native runtime. RTC LSI source and calendar-tick bounds become rate-only
  under every SYSCLK, retaining nominal 32800/32768. Generic R031
  gains no LSI SYSCLK capability. Dedicated 16 MHz RFCLK is independent,
  but PCLK/GPIOA and PA00..PA03 carry an indirect RF-host effect. Existing
  functional handover permits the inspection interval, including before
  failure, and keeps inherited RF-fed HSE bypass available. No RF registers
  are read or written and safe init gains no hidden memory-safety precondition.
  Fixed 1 MHz time-driver refusal stays before singleton acquisition and MMIO.
  Main acceptance covers six actual libraries, seven linked ELFs and six
  passed source/data checks; see the [own R031 contract](qualified-r031-lsi-sysclk.md),
  [runtime/source review](r031-factory-lsi-runtime-review.json) and
  [metadata review](r031-factory-lsi-metadata-review.json). Final-package clean
  replay of one library and two ELFs is required and tracked separately.
- CW32W031R8U6 alone adds the separate own-source factory-LSI qualification:
  QFN64, 64 KiB Flash / 8 KiB SRAM, 31,816–33,784 Hz at 2.0–3.6 V and −40–85°C.
  Shared native executable code, nine-gate/eleven-selector admission and old
  exact-package envelopes remain unchanged. RTC LSI aliases become rate-only
  under every SYSCLK; generic W031 stays excluded. RFCLK uses a dedicated
  32 MHz oscillator, while PCLK/SPI1 and whole-GPIOB inspection can affect
  PB03/04/05/13 internal host traffic and PB06 IRQ events. Functional handover
  permits that interval, even before failure; no hidden Rust memory-safety
  precondition or RF access is added. Fixed 1 MHz time-driver refusal remains
  before singleton acquisition and RCC MMIO. Main verification passed five
  actual library builds, six ELF links, generation and six focused source/data
  commands with the 1,294-file input snapshot unchanged. Independent runtime/
  source and metadata/projection reviews accept the frozen main implementation.
  At main acceptance, clean replay had not run; final-package completion
  requires a separate one-library/one-ELF clean receipt. See the
  [W031 contract](qualified-w031-lsi-sysclk.md).
- CW32L052C8T6/R8S6/R8T6 add a separate native factory-LSI SYSCLK path:
  nominal 32,800 Hz, 31,816–33,784 Hz at 1.65–5.5 V, VDDA=VDD and −40–85°C,
  with 64 KiB Flash / 8 KiB SRAM. Generic L052 and every L083 stay excluded.
  Full native snapshots cover RTC, AUTOTRIM, UART1/2/3, GPIOA/B/C/D/F,
  MCO and PC4 AF, plus tagged LCD/LPTIM work gates. Off work gates stay off
  without local register reads; cold gate-on LCD requires both EN=0 and BUMP=0.
  Whole GPIO banks can advance sampling, filters and armed events, including
  on unrelated pins and before failure. Restoration does not undo progress.
  PC4/AF6 is inspected conservatively on unbonded C8T6 without a pin token.
  Matching cold trim skips no admission; live matching LSI is never stopped
  or retrimmed. Configured factory HSI is checked at final bus divisors before
  the final source-only LSI commit. There is no standalone AWT or L052 PLL.
  Every exact-L052 RTC LSI alias becomes rate-only under HSI/HSE/LSE/LSI;
  board-LSE provenance and native HSI-based RTC sources stay unchanged.
  ADC timing refuses rate-only PCLK before its gate/reset writes; strict
  duration helpers retain refusal and the fixed 1 MHz time driver refuses
  selected LSI before singleton acquisition and RCC work. This is source
  reasoning, not exercised runtime evidence. Final main verification passed
  eight actual library builds and eight ELF links with zero warnings; generation
  and all six finite Python source/data checks passed. At main acceptance,
  clean replay had not run. Final-package completion requires a separate
  two-library/three-ELF clean receipt bound to the final source ZIP. See the
  [L052 contract](l052-factory-lsi-sysclk.md),
  [runtime/source review](l052-factory-lsi-runtime-review.json) and
  [metadata review](l052-factory-lsi-metadata-review.json).
- Init-only factory-LSI SYSCLK is qualified on F020/F030/A030. Cold admission
  precedes the first source-enable write and checks every documented shared
  consumer, gate, reset and inherited source request; factory-matching running
  LSI can be reused without retrimming. Inspecting whole GPIO banks can advance
  sampling, filters and armed events, including before a later failure.
  The three families' SYSCLK and existing RTC/LSI aliases share rate-only
  bounds, including divided RTC clocks; strict ADC and complementary-PWM
  timing guards remain enforced. A 1 MHz Embassy timebase rejects this rate
  before singleton acquisition. This is a static control-flow guarantee;
  compiling the time-driver feature does not execute that rejection.
  Apart from the exact L031/R031/W031/L052 changes above, other RTC/LSI qualifications
  are unchanged. Runtime switching and
  low-power restoration remain outside this slice. See the
  [complete factory-LSI contract](factory-lsi-sysclk.md).
- Init-only LSE SYSCLK is qualified on twenty-three exact packages through the single
  Config.lse declaration: classic3, five L031/R031/W031, three L052, five L083,
  three L010, two L011 and two L012.
  Family aliases and other packages are excluded. Classic CW32F020C6U7,
  CW32F030C8T7 and CW32A030C8T7 prepare factory detector LSI before mandatory
  CCS and retain HSI plus inherited PLL/reference; see the
  [classic contract](classic-lse-sysclk.md). L031/R031/W031 preserves configurable
  CCS and its own consumer/monitor admission; see the
  [five-package contract](l031-r031-w031-lse-sysclk.md). L052 retains separate
  startup/run analog banks and its independent undivided 8.16 MHz fixed fallback
  budget; see the [three-package contract](l052-lse-sysclk.md).
  L083 adds only CW32L083RBT6/RCT6/RCS6/MCT6/VCT6, with one analog bank and
  no L052 startup-drive/amplitude fields. Its historical Stage60 addition raised the system target count
  from eleven to sixteen while all 23 auxiliary-LSE qualifications remain unchanged.
  Its minimum declared VDD is at least 1.8 V; conservative raw factory-HSI
  fallback uses 48.96 MHz without HSI/AHB/APB divider credit, even with CLKCCS
  off. Configured HSI at final bus dividers is checked independently and Flash
  WAIT2 remains after selection. Inherited enabled PLL first escapes through
  independently legal unchanged ready HSI; both PLLEN=0 and PLL.STABLE=0
  precede any reference change. PLL configuration is preserved, but stopping it
  intentionally stops its outputs, which the caller must leave quiescent.
  There is no independent PLL-output lifecycle; see the
  [L083 system contract](l083-lse-sysclk.md).
  Native L010 adds only CW32L010F8P6/F8U6/Y8M6; the historical Stage61 addition
  brought the total to nineteen. Generic L010 system aliases remain excluded.
  Its existing native Lse declaration has independent four-bit run/startup drive
  and no amplitude field. StartupOnly keeps LSECCS clear; later loss can leave
  STABLE set and halt execution without returning an error. MonitoredExistingRoutes
  requires already stable, unchanged, inherited-legal LSI at most 36080 Hz; it
  does not cold-prepare a factory monitor. Establishing factory HSI can temporarily
  request unchanged LSI under first-request RTC/UART/LPTIM/MCO/IRQ/analog-owner
  checks, while residual timer/GPIO/IWDT and downstream progress require the
  explicit functional handover. Successful return retains factory HSI. Requested
  HSI is independently checked at final divisors; fixed fallback headroom uses
  full 4.08 MHz without divider credit regardless of CLKCCS, and promises no
  detection, fallback or continuity. See the [L010 system contract](l010-lse-sysclk.md).
  Native L011 adds only CW32L011K8T6/K8U6; the historical Stage62 total
  was twenty-one. Their PC14/PC15 are physical pins 2/3; bypass owns PC14 only.
  The example uses native Level2/Level10, 16384 startup cycles and StartupOnly
  with example 3.0–3.6 V, −20…70 °C and 32766–32770 Hz bounds
  that require board qualification.
  HSI defaults to /24 and stays factory calibrated on success. StartupOnly
  requires no factory LSI monitor; the guarded HSI-calibration bridge uses
  unchanged, electrically legal LSI. MonitoredExistingRoutes instead requires
  stable non-erased own factory-halfword matching before configuration writes,
  with 10-bit TRIM at 0x001007C2, unchanged WAIT, 41000 Hz maximum and
  `256 * LSE_min_hz > 129 * 41000`. There is no automatic monitor preparation,
  LSI TRIM/WAIT write or rate/jitter measurement. The RM legal-adjustment regime
  and DS factory range/trim-step discrepancy remain distinct.
  First-nonstable-request guards include UART1/2/3 SOURCE3 regardless of enables,
  RTC SOURCE2/reserved 4…7, enabled LPTIM ICLKSRC3, MCO4, LSIRDY and enabled
  LSI-filtered VC/LVD. Entry state remains latched for both admissions; later
  STABLE does not bypass the check immediately before LSIEN. Held reset rejects,
  and each configuration gate restores independently with specific error reporting.
  Residual timer/GPIO/IWDT/cascade observers remain a functional handover,
  including dedicated PB0 AF3 HSIOSC_OUT even with MCO disabled. GPIOC inspection
  can advance whole-bank events; no dormant timer work gate opens for inspection.
  Configured HSI at final divisors and full 4.08 MHz effective fallback are
  independently qualified without fallback divider credit. Initial WAIT3 is
  conservative; final WAIT0/1/2/3 uses the maximum qualified HCLK, and default
  HSI/24 uses WAIT0. This does not promise divider retention or CPU progress.
  Retained RTC/AWT owners are not reset or migrated. See the
  [L011 system contract](l011-lse-sysclk.md). All previous nineteen system
  projections and twenty-three auxiliary qualifications remain unchanged.
  Native L012 adds only CW32L012C8T6/C8U6, bringing the current system total
  to twenty-three. Crystal owns PC14/PC15 pins 3/4; bypass owns PC14 only.
  Native examples keep StartupOnly, Level2/Level10 drives, 16384 cycles and
  explicit declarations of 3.0–3.6 V, −20…70 °C and every-cycle 32766–32770 Hz
  requiring board qualification; these are not measured validation.
  Configured HSI /12 (8 MHz, 7.84–8.16 MHz factory bounds) remains distinct
  from reset and effective fallback /24. StartupOnly leaves LSECCS clear and
  can halt the CPU after loss. The unchanged-legal-LSI HSI bridge does not
  prepare a monitor. MonitoredExistingRoutes requires already stable,
  non-erased factory-matching 9-bit LSI TRIM at 0x001007C2, unchanged WAIT and
  `256 * LSE_min_hz > 129 * 36080`; 18181 Hz is the least integral minimum.
  Entry-nonstable first-LSI admission remains latched for repeated checks,
  including both I2C master/slave raw source1/3 and all four VC instances.
  HSI start/retrim checks also refuse disputed I2C source1/3. No ambiguous
  selector or closed gate supplies a positive absence proof. Held resets
  reject, and failed gate restoration keeps its specific error.
  Functional handover permits shared ADC1/ADC2 work when their configuration-
  and-work gate opens, along with whole-GPIOC events, dedicated HSI/LSI
  outputs, inaccessible UART3 and retained timer/cascade/external observers.
  An enabled-ADC refusal after opening does not undo earlier progress. Dormant
  timer/output/UART3 work gates stay closed. The existing platform bus-master
  and memory-ownership entry boundary remains in force.
  Own configured HSI at final divisors and full 4.08 MHz fallback are qualified
  independently, without fallback AHB/APB divider credit. Initial WAIT3 is
  followed by final WAIT under verified calibrated HSI/final divisors; default
  HSI/12 uses WAIT0. FLASH.WAIT and SYSCTRL.FLASHWAIT must agree, mixed readbacks
  fail closed, and FETCH/CACHE/CACHEINVALID are preserved. No CR0 or FLASH write
  follows the last LSE selection, including on error. Raw-HSIOSC and LSE
  calendars retain their existing sources/divisors; no RTC/AWT migration,
  rollback, fault recovery or execution continuity is promised. See the
  [L012 system contract](l012-lse-sysclk.md). All previous twenty-one system
  projections and all twenty-three auxiliary qualifications remain unchanged.
  The local script's planned scope of 24 libraries and 46 SYSCLK binaries is not an executed
  result; older actual verification receipts keep their original scope.
  For monitored operation, the modeled 129/256 count margin uses each family's LSI maximum and is
  distinct from board-qualified LSE every-cycle timing and rate-only LSI.
  Final bus divisors are installed on HSI before one LSE selection; Flash follows
  each family's qualified sequence. L083, L010, L011 and L012 permit no later CR0 write, even on
  failure. The same LSE tuple supplies RTC.
  The fixed 1 MHz time driver rejects before ownership. Old source targets and
  auxiliary-LSE paths retain admission/order. Frozen LSE timing is invalid after
  loss/fallback; no runtime switching, fault recovery or silicon guarantee follows.
- Factory-HSI- and HSE-fed system PLL is qualified on F020/F030/A030/L083, the four
  documented system-PLL families. R031/W031 radio synthesis remains separate
  and user-deferred. The nine F020 and twelve F030/A030/L083 HSI pairs remain admitted
  with entire actual input/output envelopes inside one qualified analog bin;
  F020 output is capped at48MHz and the other three at64MHz. `PllSource::HSE`
  takes the undivided source from `Config.hse` in crystal or bypass mode.
  Crystal admission relies on the vendor-documented internal composition and
  the existing board contract; hidden reference duty is not independently
  certified. Bypass requires the OSC_IN waveform limits, including 40–60% duty.
  Every HSE reference must fit 4–24 MHz and one analog input bin, and its
  multiplied output must fit one output bin and the independent raw cap.
  Independent outputs, runtime retuning, low-power restoration and guaranteed
  reference-loss recovery remain gaps. All PLL bounds are rate-only: ADC and F030/A030
  complementary dead-time admission retain their strict cycle-timing rejection.
  See [F020/x030](f020-x030-hsi-pll.md) and [L083](l083-hsi-pll.md).
- Inherited LSE pad ownership is protected on all eleven LSE-bearing families,
  independent of HSE qualification. Crystal, bypass, family-specific pad locks
  and package bond-outs determine the boot-retained reservation before safe pin
  construction. Active LSE crystal/bypass setup and an owned calendar source
  are qualified for twenty-three exact packages: the previous sixteen
  x030/F020/L031/R031/W031/L052/L083 packages, three native L010, two L011 and two L012 packages. The
  three L052 parts have separate pre-start/run analog settings, native AUTOTRIM
  admission, and LPTIM/LCD work-gate preservation; see their
  [own-source contract](qualified-l052-lse.md). The five L083 parts have one analog
  bank, six native UART consumers, GPIO LCKR and exact-package LSI routes. Their
  [own-source contract](qualified-l083-lse.md) requires the sufficient detector
  margin `256 * LSE_min_hz > 129 * 33784` before peripheral acquisition or RCC writes.
  The previous sixteen retain their own monitoring and consumer contracts.
  Native L010 uses independent four-bit running/startup drive without amplitude.
  `StartupOnly` leaves CCS clear and may retain STABLE after loss;
  `MonitoredExistingRoutes` requires legal stable unchanged LSI at most 36080Hz,
  checks `256 * LSE_min_hz > 129 * 36080`, and retains deliberate fault routes.
  Its source-zero startup also requires the public RTC observer and whole-GPIOB
  functional handover; dormant timer work gates are not probed. See the
  [native L010 contract](qualified-l010-lse.md). The [new L011/L012 contract](qualified-l011-l012-lse.md)
  uses factory-matching stable LSI with 41000/36080Hz upper bounds and explicit whole-GPIOC,
  closed-output/UART3 and retained-root functional handover exclusions. Native HSIOSC
  calendar sources remain available, with direct LSE source and PSC selection. The shared gate-inspection helper now
  attempts bounded restoration after initial enable-readback failure; successful
  HSI/HSE/PLL paths are unchanged. All twenty-three require board electrical and
  every-cycle frequency bounds around nominal 32768Hz. Each family applies its
  own retained-consumer admission; exact enabled reuse preserves configuration. Poll-budget exhaustion retains the source and reservations
  and requires reset; ordinary reset need not clear retained LSE controls, so
  POR may be required. There is no automatic calendar fallback, low-power
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
  L083 requires selected-part SRAM metadata. Unsafe borrowed hardware-request
  constructors remain x030-only; L083 UART/SPI RX/TX routes are exposed through
  the separately declared safe staged integration. L083 ADC/LCD/timer requests
  remain outside that integration.
  Entry requires hardware exclusivity, including error and forgotten transfers.
  Only clean recorded TC without TE, STATUS=5 and SOFTSRC=0 returns owners.
  Errors permanently reserve resources, forgetting leaks them, and Drop may block
  forever. Safe init-admitted copies require normal reset/clean-runtime entry;
  dirty startup rejection cannot repair an arbitrary active bootloader handover.
  Safe peripheral integration covers staged F030/A030/L083 UART TX/RX and paired
  SPI master with qualified SRAM and normal reset/clean-runtime entry. RX gaps can lose frames; safe early abort, circular operation and
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
