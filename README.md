# embassy-cw32

See [constructor API migration](docs/constructor-api-migration.md) for fallible `new*` APIs and migrating former `try_new*` callers.

Experimental, source-backed CW32 data/PAC/HAL workspace, modeled on the
`stm32-data → stm32-metapac → embassy-stm32` architecture.

**Active development: verified data/PAC and Embassy-style HAL implementations.**

**This is not complete all-chip/all-peripheral support.** A peripheral appearing in
a generated PAC is not a completed driver. Vendor metadata also contains errors.
See `docs/upstream.md`, the source manifests, and the coverage report before use.

Finite software ADC reads and ordered scans support typed IRQ bindings and
caller-composed deadlines on [L010/L011](docs/adc-low-async.md),
[all classic lines and L012](docs/adc-remaining-async.md).
[L012 ADC2](docs/adc-l012-adc2-async.md) checks actual DAC IRQ quietness before
using shared ADC2_DAC; ordinary direct-loading DAC outputs can coexist.
Stage91 main produced five standalone ARM libraries and ten linked ELFs in
sixteen attempts, retaining the initial link failure and reviewed example-only
size-profile correction. At main acceptance clean replay and final54 report
binding had not run; final completion requires their separate receipts.
These software results are not hardware validation.

Stage92 extends [finite staged UART RX and combined/split DMA](docs/classic-uart-dma-rx.md)
to fourteen exact F020/L031/L052/R031/W031 parts using their own byte-RDR
evidence and the existing receive algorithm. L012 remains TX-only for UART DMA.
Stage92 main passed eight standalone ARM libraries and 21 ELF links, all 29
commands exiting zero without warnings. Independent runtime and metadata/source reviews accepted the bounded main
result. At main acceptance, final capability reports and the separate
final-source two-library/three-ELF clean replay had not run.
Historical copy/TX results below do not verify the RX extension.

## Current software API additions

The current source adds [4–16-bit IRQ PIO SPI words](docs/spi-async-word-widths.md)
on the existing ten qualified families; staged DMA remains byte-only and rejects
other words without an implicit PIO fallback. It also adds
[coalesced capture IRQ waits](docs/timer-capture-irq.md) on the existing
InputInstance/OverflowInstance intersection, and
[LCD async frame waits/presentation](docs/lcd.md#asynchronous-frame-waits) on
L052/L083, together with [AUTOTRIM coalesced underflow waits](docs/autotrim-async.md)
on its existing HSIOSC counter mode. The LCD async example owns both clients
on AUTOTRIM_LCD. No new clock, pin, DMA or electrical qualification is implied.

These additions passed the [current software acceptance](docs/unblocked-hal-extensions-validation.md):
113 ARM library commands and 91 example build commands, plus focused software
checks. This receipt is not a publication claim; inclusion in main is established
by the commit containing these changes. Historical stage receipts below describe
their frozen original scopes, not these new APIs.
Cancellation does not imply a completed SPI transaction, captured-event history,
or an atomic LCD frame update. No hardware validation is claimed.

## Second software API batch

The current source additionally implements L012-only
[ordinary nine-bit UART PIO](docs/uart-l012-nine-bit.md),
[explicit ten-bit I2C master](docs/l012-i2c-ten-bit.md),
[coalesced HALLTIM capture waits](docs/halltim-async.md), and
[IRQ-assisted completion of existing CORDIC Q1.31 operations](docs/cordic-async.md).
[L012 LVD/comparator fresh-window edge waits](docs/l012-analog-edge-waits.md)
cover only existing qualified analog owner configurations, without filters,
new references or analog-readiness guarantees. These additions passed their
[separate software acceptance](docs/l012-extensions-validation.md): 113 ARM library commands,
68 example build commands, 26 cohort API expectations, 12 IRQ ownership
expectations and focused host suites. The receipt does not itself establish
publication to main; the first batch's receipt is not reused as validation. Existing DMA restrictions,
other-family exclusions and numerical/electrical limits remain in force.
No hardware execution or low-power/RF capability is claimed.

## Workspace

- `cw32-data/`: curated versioned register YAML, chip/source manifests, verified
  part facts and routing/electrical YAML, acquisition and validation tools
- `sources/`: canonical source/version/hash catalog; downloaded originals in ignored `sources/vendor/`
- `cw32-data-serde/`: upstream-shaped chip metadata schema
- `cw32-data-macros/`: independently implemented metadata proc macro with the
  required upstream-compatible behavior and recorded predecessor provenance
- `cw32-data-gen/`: verified-input SVD import and normalization with chiptool
- `cw32-metapac-gen/`: actual upstream-derived chiptool PAC renderer
- `cw32-metapac/`: generated PAC, per-chip features, static metadata, vectors
- `embassy-cw32/`: ownership, type-level interrupts, and implemented HAL drivers

Hardware YAML throughout `cw32-data/` is authoritative. Normal generation never
replaces it with vendor data. `./d import-registers` creates separate review
candidates; explicit canonical mappings share only exactly equal normalized IRs.
See `docs/architecture-audit.md` for the upstream comparison and remaining gaps.
The import CLI uses a fresh staging directory. Direct library callers should also use a fresh candidate directory: existing files are never pruned, and an older chip file is not evidence of a new import projection.

`./d` passes its own workspace explicitly to the data generator with `--root`.
When calling `cw32-data-gen` directly, use `--root /path/to/workspace` to select
the authored inputs. Without it, the CLI retains the checkout path compiled into
the executable, which may differ from the current directory when a target cache
is shared. An omitted `--out-dir` is derived from the final selected root.


Upstream Git dependencies are pinned. Do not replace them with a floating branch
without regenerating the PAC and running the contract tests.

Leaf Rust modules use `<module>.rs`; directories with `mod.rs` are retained only
when they group real sibling files or submodules. The PAC generator emits the
same layout.
Custom cfg predicates name concrete chips, peripheral versions or capabilities;
there are no generic HAL-prefixed cfg labels. Run structural validators locally with `./d lint`.

## Build

The supplied scripts require Python 3.11+ and Bash (Linux, WSL or macOS).
The root Cargo workspace contains the data schema and generators;
`firmware/Cargo.toml` is the separate HAL/PAC workspace.

The verified build uses Rust 1.99.0 (rustfmt 1.10.0) and target
`thumbv6m-none-eabi`. Install an appropriate official Rust toolchain in your
execution environment. No toolchain is bundled in this source tree. The toolchain
and all dependency source pins are recorded alongside the verification results.

```sh
python3 -m pip install -r requirements-dev.txt
./d fetch-sources
./d fetch-evidence
./d gen-all
./d test
./d audit-current
./d check
cargo check --manifest-path firmware/Cargo.toml -p cw32-metapac --features cw32f030c8t7 --target thumbv6m-none-eabi
```

The source archive excludes generated PAC, chip JSON, build reports and compiler outputs.
The Stage25 unchanged hardware-output comparison covered 251 JSON files and
136 generated normalized-register YAML files (387 total); 13 separate report
JSON files are not counted as hardware data.
`./d gen-all` runs the independent generator workspace without requiring a
pre-existing PAC. It stages data, formatted PAC, coverage and provenance before
replacing the generated outputs. A failed generation step leaves existing outputs
unchanged; a failed commit rename rolls back the preceding renames. This is not
an atomic multi-directory swap or power-loss recovery: run one generation at a
time, and do not consume outputs during the brief commit. If filesystem errors
also prevent rollback, the command reports and retains its backup directory under
`build/` for recovery. Firmware workspace commands run after successful generation. Use `--manifest-path firmware/Cargo.toml` for HAL/PAC Cargo commands. Source identities
and download instructions are retained in `sources/`.

The source audits also require the pinned official manuals, SDK members and
PDF text extraction. `./d fetch-evidence` reconstructs these outside the
redistributed source tree; see [evidence acquisition](docs/evidence-acquisition.md)
for the configurable cache, hashes and extraction requirements.

Select exactly one chip feature. There is no implicit default chip. Generated
features span the imported vendor families; see `build/reports/coverage.json` (generated by `./d gen-all`) for
the precise catalog-to-feature mapping and missing metadata. A family PAC is not
an assertion that every package or memory variant is fully described. The generic
`cw32f030` feature is a register-level die description; it does not assert a memory
size or package bond-out. Exact ordering-code features such as `cw32f030c8t7` carry qualified package and
memory facts. Short legacy aliases are not substitutes for a verified package
feature. Application linker configuration,
runtime, critical-section implementation, board routing, and clock requirements
remain the application author's responsibility.

## Experimental HAL

The current HAL tree follows the pinned Embassy production-module organization
with flat leaf files and meaningful multi-file groups. HAL tests remain deleted.
See [the leaf-layout report](docs/rust-leaf-layout.md) for current paths and
[the test-removal report](docs/hal-production-layout.md) for the historical
deletion inventory.

RCC configuration declares board supply and ambient-temperature intervals. These
are assertions, not measurements. Actual upper/lower source-qualified clock bounds
constrain bus ceilings, Flash waits, ADC timing and SPI/I2C rates. Conservative
bounds may reject a nominal maximum setting; nominal Hertz accessors remain
nominal. L011/L012 ADC defaults require a faster qualified clock configuration;
ADC examples explicitly select a qualified HSI divider. See the Stage 10 report
and the [ordered scan examples](examples/adc-scan/README.md).

The HAL uses Embassy's actual `Peri<'d, T>`, generated singleton types,
sealed peripheral/pin traits and type-level `bind_interrupts!` bindings.
`try_init(Config)` configures the selected verified clock path with bounded readiness
polling; `init` is the panicking convenience wrapper. Failed hardware initialization
may consume ownership and require a device reset before retrying. The incoming
clock/voltage state must be valid and its active source must remain available
through initialization. Final HCLK/PCLK must satisfy the device's supply-voltage
limits; many families allow only 24 MHz below 1.8 V. Initialization is not a
fault-tolerant recovery controller for arbitrary external-clock loss.

- All eleven LSE-bearing families: read-only inherited LSE pad reservation before
  token return, with source-specific pad locks and exact package routes. GPIO and
  peripheral pin construction reject reserved pads before GPIO writes.
  Twenty-three exact packages support explicitly requested, board-qualified nominal
  32768 Hz crystal/bypass setup and an owned RTC source. The previous sixteen
  x030/F020/L031/R031/W031/L052/L083 packages retain their own monitoring,
  electrical and consumer limits. The five L083 parts
  additionally require `256 * LSE_min_hz > 129 * 33784` before peripheral acquisition
  or RCC writes; this sufficient detector margin does not establish board accuracy.
  Those sixteen use their own conservative RTC admission and bounded polling.
  The three native L010 packages use a separate source-zero observer/handover
  contract. `StartupOnly` leaves CCS clear and can retain STABLE after clock loss;
  `MonitoredExistingRoutes` requires legal stable unchanged LSI and retains
  deliberate hardware fault routes. Neither promises continuing timekeeping.
  L011/L012 use own-source factory-matching LSI for monitoring, with 41000Hz/36080Hz
  maxima and explicit whole-GPIOC/downstream handover limits; closed output banks
  and L012 UART3 are not an absence proof. See the [native L011/L012 contract](docs/qualified-l011-l012-lse.md)
  and [native L010 contract](docs/qualified-l010-lse.md). Failure requires
  reset (POR may be needed for retained LSE controls); no automatic RTC fallback, low-power recovery or elapsed-time accuracy
  after a clock fault is promised. Other parts retain read-only inherited-pad
  protection. See [the ownership contract](docs/inherited-lse-pads.md) and
  [active LSE contract](docs/qualified-lse.md) and
  [qualified LSE examples](examples/lse-clock/README.md).
- All 13 families: GPIO and typed async GPIO interrupts, HSI clocks, independent
  watchdog, documented CRC16 presets, blocking/interrupt-driven UART, blocking
  master SPI, blocking seven-bit master I2C and polling BTIM1–3 counters.
  BTIM1/2/3 additionally provide typed Async coalesced ARR-overflow waits across
  all 54 existing selections; `Timer<'d, T>` still defaults to Blocking.
  Successful reconstruction through either mode retires forgotten static IRQ
  state. Shared L052/L083/L012 vectors dispatch each enabled owner's handler
  without touching partner registers or changing shared gate/reset policy.
  Native initialization/reconfiguration still generates UG and can affect
  retained TRGO recipients. Main verification passed 16 actual ARM libraries
  and 23 ELF links, all exit zero; four finite source/data gates passed.
  Three existing native L010/L011 RCC warnings remain in applicable rows.
  Independent main runtime and metadata/source reviews accepted the frozen
  implementation. At main acceptance clean had not run; final-package completion
  requires a separate
  final-source 2-library/6-ELF receipt. Earlier partial/failed attempts remain
  recorded in [the verification history](docs/btim-overflow-irq.md#verification-boundary).
  No lossless count, stopped-clock progress, sleep/wake or hardware claim.
  See [the overflow-event contract](docs/btim-overflow-irq.md).
- GTIM/ATIM counters add the same typed Async coalesced overflow API across
  34 GTIM and eleven ATIM own-family/instance associations. L012 GTIM3/GTIM4
  share GTIM34 with separate owners/handlers; GTIM1 can remain reserved for
  Embassy time. Classic ATIM on F030/A030/F003/L031/R031/W031/L052/L083 adds
  native polling capture for six A/B halves and CH1A/CH1B QEI, using independently
  qualified bonded input routes. QEI count/direction/wrap observations are
  non-atomic; capture is a latest latch with separate sticky overwrite status.
  No capture IRQ/DMA, CH4 external input or lossless history is added.
  Main acceptance covers 16 actual ARM libraries, 27 linked ELFs and ten
  finite source/data gates. Three existing L010/L011 RCC `dead_code` warnings
  remain in applicable rows. The initial static correction, failed lockfile
  attempt and failed AF-expectation gate remain recorded in the contract.
  At main acceptance clean had not run; final-package completion requires a
  separate final-source 2-library/6-ELF receipt. The preceding BTIM receipts
  retain their earlier scope. See the
  [counter-event and native-input contract](docs/timer-event-input.md),
  [event example](examples/timer-overflow/README.md) and
  [native-input examples](examples/classic-atim-input/README.md).
- All except L010/L011: polling window watchdog with explicit irreversible start
- CW32F002F3P7 and CW32F002F3U7: init-only factory LSI SYSCLK with own
  31,160–34,440 Hz rate bounds, complete cold-source admission and explicit
  whole-GPIOA/B/C functional handover. Generic F002 is excluded;
  there is no RTC API. ADC and the fixed 1 MHz time driver reject the rate-only
  system tree. Metadata `lsi_output_pin` is now optional to express actual
  hardware absence. See [the exact-two contract](docs/f002-factory-lsi-sysclk.md).
- CW32F003F4P7, CW32F003F4U7 and CW32F003E4P7: init-only factory LSI
  SYSCLK with own 31,816–33,784 Hz (±3%) rate bounds, 20 KiB Flash / 3 KiB
  SRAM and the whole-GPIOA/B/C handover. Generic F003 remains excluded.
  ADC and the fixed 1 MHz time driver reject this rate-only system tree;
  AWT keeps independent HSIOSC timing. ATIM/IR add no inspection windows
  or new API qualification. See [the exact-three contract](docs/f003-factory-lsi-sysclk.md).
- CW32L031C8T6, CW32L031C8U6 and CW32L031F8U6: init-only factory LSI
  SYSCLK with own 31,816–33,784 Hz (±3%) rate bounds at 1.65–5.5 V and
  −40–85°C, 64 KiB Flash / 8 KiB SRAM and default retained HSI /6.
  Whole-GPIOA/B/C/F inspection can advance events even before failure.
  RTC LSI aliases become rate-only under every SYSCLK on these three parts;
  generic/other L031 keep their previous qualifications.
  See [the exact L031 contract and review status](docs/l031-factory-lsi-sysclk.md).
- CW32R031C8U6 (QFN48, 64 KiB Flash / 8 KiB SRAM): init-only factory LSI
  SYSCLK with own 31,816–33,784 Hz rate bounds at 2.2–3.6 V and −40–85°C.
  The shared native runtime sequence is unchanged. This exact part's RTC LSI
  aliases become rate-only under every SYSCLK; generic R031
  remains excluded from LSI SYSCLK. RFCLK has its own 16 MHz oscillator, while
  PCLK/GPIOA inspection can affect RF host activity. The existing functional
  handover and retained external-source availability apply without a hidden
  Rust memory-safety precondition or RF register access. Main acceptance covers
  six actual library builds, seven linked ELFs and own-source/data checks; see
  the [R031 contract](docs/qualified-r031-lsi-sysclk.md),
  [runtime/source review](docs/r031-factory-lsi-runtime-review.json) and
  [metadata review](docs/r031-factory-lsi-metadata-review.json).
  Final-package clean replay is required and tracked separately.
- CW32W031R8U6 (QFN64, 64 KiB Flash / 8 KiB SRAM): separate init-only
  factory LSI SYSCLK qualification at 31,816–33,784 Hz, 2.0–3.6 V and −40–85°C.
  The shared native runtime is unchanged; RTC LSI aliases become rate-only
  under every SYSCLK. Generic W031 remains excluded. Dedicated 32 MHz RFCLK
  is separate, while PCLK/SPI1 and GPIOB inspection can affect internal
  PB03/04/05/13 host and PB06 IRQ activity. Functional whole-bank handover
  applies without a hidden Rust memory-safety precondition or RF operation.
  Fixed 1 MHz time-driver refusal precedes singleton acquisition and RCC MMIO.
  Main verification passed five actual library builds, six ELF links, generation
  and six focused source/data commands. Independent runtime/source and metadata
  reviews accept the frozen main implementation. At main acceptance, clean
  replay had not run; final-package completion requires a separate clean receipt.
  See the
  [own W031 contract and evidence status](docs/qualified-w031-lsi-sysclk.md).
- CW32L052C8T6, CW32L052R8S6 and CW32L052R8T6: separate init-only
  factory LSI SYSCLK with own 31,816–33,784 Hz rate bounds at 1.65–5.5 V,
  VDDA=VDD and −40–85°C; all three have 64 KiB Flash / 8 KiB SRAM.
  Whole-GPIOA/B/C/D/F inspection may advance events even before failure;
  off LCD/LPTIM work gates remain off. Live factory-matching LSI is retained
  without stop/retrim. Every exact-L052 RTC LSI alias becomes rate-only under
  HSI, HSE, LSE and LSI SYSCLK. Generic L052 remains excluded; the separate
  exact-five L083 contract follows below.
  Final main verification passed eight actual library builds and eight ELF
  links with zero warnings, plus generation and six finite Python source/data
  checks. At main acceptance, clean replay had not run; final-package completion
  requires a separate final-source two-library/three-ELF clean receipt.
  These software results do not establish hardware behavior. See the
  [L052 contract](docs/l052-factory-lsi-sysclk.md),
  [runtime/source review](docs/l052-factory-lsi-runtime-review.json) and
  [metadata review](docs/l052-factory-lsi-metadata-review.json).
- CW32L083RBT6/RCT6/RCS6/MCT6/VCT6: init-only factory LSI SYSCLK with own
  31,816–33,784 Hz rate bounds at 1.65–5.5 V, VDDA=VDD and −40–85°C. RBT6
  retains 128 KiB Flash, the other four 256 KiB, and all five 24 KiB SRAM.
  Generic L083 and generic L010/L011/L012 remain excluded. Full six-UART,
  six-GPIO-bank and complete bonded LSI/PLL route admission preserves off
  LCD/LPTIM work gates; inspection can advance unrelated pins and events.
  The inherited PLL must depart through a proven HSI/HSE bridge and reach
  coherent stopped state before reference changes. Flash WAIT2 is retained;
  potential external-source fallback requires raw 48.96 MHz factory-HSI
  coverage without divider credit. Configured HSI must fit final bus divisors.
  Exact-L083 RTC LSI aliases become rate-only under every SYSCLK, including
  HSI/HSE/PLL/LSE. Failure requires reset and provides no source-loss recovery.
  Main 10-library/10-ELF verification and independent implementation reviews
  passed within the finite source scope. At main acceptance clean had not run;
  final-package completion requires a separate clean 2-library/3-ELF receipt. See the
  [L083 source contract and finite scope](docs/l083-factory-lsi-sysclk.md).
- CW32L010F8P6/F8U6/Y8M6: init-only live factory-LSI reuse, nominal 32,800 Hz
  with 31,816–33,784 Hz rate bounds at 1.62–5.5 V and −40–85°C. Original entry
  must already have software LSIEN, STABLE and factory-matching TRIM; ordinary
  POR, stopped startup and automatic-only requests are refused. No LSI parameter
  or IWDT operation prepares admission. Running consumers and observers may
  progress, including watchdog reset; necessary whole-bank windows can advance
  unrelated events. New-target `Config.lse=Some(_)` is refused; inherited LSE
  and old HSI/HSE/LSE contracts are preserved. Default native RTC retains strict raw-HSI
  bounds; ADC and buffered complementary PWM return cycle-qualification errors.
  Main eight-library/seven-ELF verification, six finite source/data/layout
  gates and independent runtime/metadata reviews passed. The main logs retain
  three existing unused LSE-constant warnings in L010/L011 rows. At main
  acceptance clean had not run; final-package completion requires a separate
  final-source two-library/two-ELF clean receipt. No hardware result follows.
  That L010 addition left generic L010 and all L011/L012 unqualified at that
  historical acceptance point; the separate exact L012 and L011 contracts below
  do not rewrite that evidence.
  See the [live-only contract](docs/l010-live-factory-lsi-sysclk.md).
- CW32L012C8T6/C8U6: init-only reuse of an originally software-requested,
  stable, own factory-matching LSI, nominal 32,800 Hz and rate-only
  29,520–36,080 Hz at 1.7–5.5 V and −40–85°C. Both parts retain 64 KiB Flash
  and 8 KiB SRAM. Ordinary POR and automatic-only requests are refused; no LSI
  parameter/request or IWDT operation prepares admission. `Config.lse` must
  be `None`; inherited LSE remains protected. Shared ADC work-gate inspection
  and HSE GPIOF windows can advance activity, including PF3 LSI_OUT effects.
  PB11 AF4 and PF3 AF2 are both retained routes. Default native RTC keeps raw 96 MHz
  HSIOSC; ADC/PWM cycle timing and the fixed 1 MHz time driver reject the
  rate-only system tree. Main verification passed seven actual libraries, six
  linked ELFs and six finite source/data gates, with independent runtime and
  metadata/source acceptance. At main acceptance clean replay had not run;
  final-package completion requires a separate final-source two-library/two-ELF
  clean receipt. Board qualification remains unverified. Generic L012 remains
  excluded. The historical Stage75 L011 exclusion belongs to that acceptance
  point; the separate L011 contract below does not rewrite it. See the
  [own live-only contract](docs/l012-live-factory-lsi-sysclk.md).
- CW32L011K8T6/K8U6: init-only reuse of originally software-requested, stable,
  factory-matching ten-bit LSI, nominal 32,800 Hz and rate-only 29,520–41,000 Hz
  at 1.7–5.5 V, VDDA=VDD and −40–85°C. Both parts retain 64 KiB Flash / 6 KiB
  SRAM. `Config.hse` and `Config.lse` must be `None`; original HSEEN, HSECCS
  and LSECCS must all be zero, with HSI or LSI entry. Ordinary POR, cold startup,
  automatic-only request and new monitors are excluded. RTC SOURCE1 refuses
  after its real restored configuration-gate window; SOURCE3 requires original
  factory-ready raw HSI and actual PSC1 raw ≥97. Default native RTC stays raw 96 MHz
  HSIOSC. The DS −10/+25% factory envelope does not withdraw the RM ±10%
  calibration-adjustment warning or qualify cycle timing. Main verification passed
  seven actual libraries and seven linked ELFs, all with exit zero; generation
  and seven finite data gates passed in separate invocations. Three existing
  LSE-constant warning headers remain. At main acceptance clean had not run;
  completion requires a separate final-source two-library/two-ELF receipt.
  No hardware result is claimed. See the
  [own live-only contract](docs/l011-live-factory-lsi-sysclk.md).
- F020/F030/A030: init-only factory LSI SYSCLK with complete cold-start admission,
  explicit whole-GPIO-bank inspection effects, permanent target request and
  rate-only bounds. The same families' LsiClock/RTC aliases are also rate-only;
  strict duration helpers, ADC and complementary PWM reject those bounds, and
  the 1 MHz time driver rejects selected LSI before singleton acquisition. See
  [the contract and compatibility change](docs/factory-lsi-sysclk.md).
- CW32F020C6U7, CW32F030C8T7 and CW32A030C8T7: init-only board-qualified
  LSE SYSCLK using the single `Config.lse` declaration, internal factory-LSI
  detector preparation, retained HSI and inherited PLL/reference. The new target
  alone requires the modeled 129/256 detector margin; the fixed 1 MHz time driver
  rejects it before ownership. See [the contract and exhaustive-match compatibility
  change](docs/classic-lse-sysclk.md).
- CW32L031C8T6/C8U6/F8U6, CW32R031C8U6 and CW32W031R8U6: init-only
  LSE SYSCLK through the same `Config.lse` tuple and its own factory-LSI monitor.
  Incoming CLKCCS/HSECCS stay unchanged; matching cold LSI can resume parked
  consumers without TRIM/WAIT writes. Final HSI/buses/Flash precede one LSE mux
  write. See [the separate five-package contract](docs/l031-r031-w031-lse-sysclk.md).
- CW32L052C8T6, CW32L052R8S6 and CW32L052R8T6: init-only board-qualified
  LSE SYSCLK from the same `Config.lse` tuple, with separate startup/run analog
  settings and the own factory-LSI monitor. Configured HSI is checked at final
  bus dividers; fixed fallback coverage uses undivided 8.16 MHz without assuming
  divider retention. Whole-bank inspection and matching stopped-LSI startup
  can resume functional consumers. See [the exact-three contract](docs/l052-lse-sysclk.md).
- CW32L083RBT6, CW32L083RCT6, CW32L083RCS6, CW32L083MCT6 and CW32L083VCT6:
  init-only LSE SYSCLK through the same `Config.lse` declaration and own
  factory-LSI monitor. The historical Stage60 addition brought LSE system targets
  from eleven to sixteen exact packages; all twenty-three auxiliary-LSE
  qualifications stay unchanged.
  The new target requires declared minimum VDD at least 1.8 V, conservative raw
  factory-HSI fallback coverage at 48.96 MHz with no HSI/AHB/APB divider credit,
  and retained Flash WAIT2. Inherited enabled PLL first escapes through legal
  unchanged ready HSI, then stops with both enable-clear and not-stable
  acknowledgments; this intentionally stops its outputs. The caller must hand
  over quiescent consumers. No runtime switching, independent PLL-output
  lifecycle or fault recovery is added. See [the exact-five contract](docs/l083-lse-sysclk.md).
- CW32L010F8P6, CW32L010F8U6 and CW32L010Y8M6: init-only native LSE SYSCLK
  through the existing `Config.lse` declaration. The historical Stage61 addition
  brought system targets to nineteen exact packages. StartupOnly leaves LSECCS clear and can halt the
  CPU after source loss without an error return. MonitoredExistingRoutes
  requires already stable, legally operating, unchanged LSI; it does not
  prepare a factory monitor. The HSI-calibration path may temporarily request
  inherited-legal LSI under target-only first-request ownership checks and an
  explicit residual-consumer handover. Factory HSI remains enabled on success.
  Requested HSI is checked at final dividers; fixed fallback coverage uses
  undivided 4.08 MHz without assuming divider retention or promising fallback.
  Final divisors precede LSE selection and no CR0 write follows it. The previous
  sixteen system paths and all twenty-three auxiliary qualifications remain
  unchanged. Generic L010 aliases remain excluded. See the
  [L010 system contract](docs/l010-lse-sysclk.md) and
  [crystal/bypass examples](examples/lse-sysclk/README.md).
- CW32L011K8T6 and CW32L011K8U6: their init-only native LSE SYSCLK addition
  brought the historical Stage62 roster to twenty-one exact system packages.
  The prior nineteen system paths and all twenty-three auxiliary qualifications
  were unchanged; generic aliases remain excluded. PC14/PC15 are pins 2/3 on
  both packages, and the native example uses
  StartupOnly, independent Level2/Level10 drives and 16384 startup cycles with
  example 3.0–3.6 V, −20…70 °C and every-cycle 32766–32770 Hz declarations
  that require board qualification.
  Default HSI /24 is retained factory calibrated on success. StartupOnly needs
  no factory LSI monitor; its guarded HSI-calibration bridge uses unchanged,
  electrically legal LSI. MonitoredExistingRoutes instead requires stable,
  non-erased factory-matching LSI at entry, with own 41000 Hz maximum and
  `256 * LSE_min_hz > 129 * 41000`; there is no automatic monitor preparation.
  First-nonstable-request guards include UART3. The functional handover covers
  residual timer/GPIO/IWDT roots and dedicated PB0 AF3 HSIOSC_OUT observers.
  Configured HSI and full 4.08 MHz effective fallback are checked independently;
  no divider retention or execution continuity is promised. Final divisors
  precede the last LSE selection, with no later CR0 write; final Flash WAIT uses
  the maximum qualified HCLK bound (WAIT0 for the default HSI /24 configuration).
  Loss can stop the CPU, and errors publish no clocks and require reset before
  retry. See the [L011 system contract](docs/l011-lse-sysclk.md) and
  [crystal/bypass examples](examples/lse-sysclk/README.md).
- CW32L012C8T6 (LQFP48) and CW32L012C8U6 (QFN48): init-only native LSE SYSCLK
  brings the current exact system and auxiliary rosters to twenty-three each.
  The prior twenty-one system paths and all auxiliary qualifications remain
  unchanged; generic aliases and other packages stay excluded. Crystal owns
  PC14/PC15 pins 3/4; bypass owns PC14 only. Native examples reuse StartupOnly,
  independent Level2/Level10 drives and 16384 cycles, with explicit board
  declarations requiring qualification rather than measured validation.
  Configured HSI defaults to /12 (8 MHz); reset and effective fallback /24
  (4 MHz) are separate facts. Successful init retains factory HSI. StartupOnly
  can halt the CPU on loss; its unchanged-legal-LSI bridge does not prepare a
  monitor. MonitoredExistingRoutes needs stable factory-matching 9-bit LSI
  at entry and `256 * LSE_min_hz > 129 * 36080`. First-request checks cover
  own RTC/UART/I2C/LPTIM/analog consumers; UART3's closed disputed work gate stays
  closed. Functional handover includes shared ADC1/ADC2 work-gate progress,
  whole GPIOC, dedicated source outputs and timer/cascade/external observers.
  Configured HSI and full 4.08 MHz fallback are separately qualified without
  fallback divider credit. Final WAIT is established under verified HSI/final
  divisors before LSE selection; no CR0 or FLASH write follows that selection,
  even on error. Loss invalidates timing; errors publish no clocks and require
  reset. See the [L012 system contract](docs/l012-lse-sysclk.md) and
  [crystal/bypass examples](examples/lse-sysclk/README.md).
- F020/F030/A030: direct qualified HSE crystal/bypass system clocks with explicit
  board nominal/minimum/maximum bounds, preserved factory HSI, mandatory CCS/LSI,
  retained-source protection and oscillator-pad reservation. See
  [clock contracts and evidence](docs/qualified-hse.md) and
  [crystal/bypass UART firmware](examples/hse-clock/README.md).
- L031/R031/W031: direct qualified HSE crystal/bypass with the existing CCS
  policy preserved, conditional LSI retention, and source/pad ownership checks.
  L031 QFN20 and its package-less alias lack a qualified HSE route. W031 HSE
  requires at least 2.0 V; HSI-only qualification is unchanged. See
  [own-source limits and initialization contract](docs/qualified-l031-hse.md).
- L010/L011: direct qualified HSE crystal/bypass on all five modeled exact
  packages, with native eight-level drive fields and no frequency-range selector.
  Retained RTC/AWT, ADC/BGR, LVD and comparator owners are inspected; inherited
  CCS is preserved and fixed 4 MHz fallback is independently bounded. See
  [own-source limits and initialization contract](docs/qualified-l010-l011-hse.md).
- L012: [direct qualified HSE crystal/bypass](docs/qualified-l012-hse.md) on both
  exact packages and their common alias. Actual 4–32 MHz endpoints, board conditions,
  fixed 4 MHz CCS fallback and retained analog timing owners are checked; enabled
  inherited HSE requires an exact declaration and readback-only reuse. Default
  HSI remains 8 MHz. No PLL, runtime switching or source-loss recovery is provided.
- L052: direct qualified HSE crystal/bypass on all three modeled exact packages,
  with separate pre-start/run parameters, retained RTC/AUTOTRIM/LVD ownership,
  configurable CCS preserved, and independently qualified fixed HSI/6 fallback.
  HSI remains the default; no PLL hardware exists. See
  [own-source limits and initialization contract](docs/qualified-l052-hse.md).
- L083: direct qualified HSE crystal/bypass on all five modeled exact packages,
  with configurable CCS preserved, retained RTC/AUTOTRIM/LVD owners checked,
  and bounded entry from an inherited PLL through unchanged HSI. HSI remains
  the default. HSE source and retained-owner requirements also apply to HSE-fed
  PLL. Active LSE has a separate [exact-package contract](docs/qualified-l083-lse.md). See
  [own-source limits and initialization contract](docs/qualified-l083-hse.md).
- L083 additionally: one-time factory-HSI- or HSE-fed PLL for qualified SYSCLK and bus
  rates. `PllSource::HSE` uses the undivided crystal/bypass source in `Config.hse`.
  Actual input/output envelopes must fit their independent electrical
  limits and analog bins. Strict ADC cycle-duration APIs reject this new rate-only
  source; existing HSI/HSE ADC is unchanged. See [PLL qualification](docs/l083-hsi-pll.md)
  and [UART and Embassy timer firmware](examples/pll-clock/README.md). The
  [Stage42 combination receipt](docs/l083-pll-stage42.md) records historical HSI-PLL verification.
- F020/F030/A030 additionally: one-time factory-HSI- or HSE-fed system PLL with each
  family's actual tolerance envelope, analog bins and raw output ceiling.
  F020 is limited to 48 MHz raw output, F030/A030 to 64 MHz; full-envelope
  qualification is stricter than nominal-only selection. ADC and x030
  complementary PWM reject rate-only cycle timing before peripheral startup.
  See [own-source scope](docs/f020-x030-hsi-pll.md) and the
  [ordinary PLL firmware](examples/pll-clock/README.md).
  On all four PLL families, crystal admission follows the vendor-documented
  oscillator-to-PLL path without independently certifying hidden reference duty;
  bypass requires the OSC_IN waveform contract, including 40–60% duty. Both
  modes remain rate-only, with no runtime retuning or reference-loss recovery guarantee.
- F002/F003: direct digital HEX on PB0 or PB1 with explicit actual bounds,
  independent retained-AWT pad reservation and conservative exact-reuse admission.
  See [clock contracts and own-source conflicts](docs/qualified-hex.md) and
  [PB0/PB1 UART firmware](examples/hex-clock/README.md). No automatic clock-loss
  fallback or low-power resume is claimed.
- F030/A030 additionally: CRC32, blocking ADC, GTIM counter/simple PWM and
  unsafe borrowed DMA, owned static-SRAM software copies and
  [safe staged UART TX DMA](docs/uart-dma-tx.md) plus [finite-chunk RX and split
  full-duplex DMA](docs/uart-dma-rx.md) with ordinary caller slices
- F020 additionally: blocking 12-bit ADC and GTIM counter/simple PWM; exactly
  CW32F020C6U7/K6U7/F6U7 add [native two-channel safe owned SRAM copies and
  staged byte UART TX DMA](docs/f020-dma-owned.md). Generic `cw32f020` retains
  `MissingMemoryMetadata` rejection. [Finite RX and combined/split UART DMA](docs/classic-uart-dma-rx.md)
  are separately qualified on these exact parts. The Stage93 [SPI1/SPI2 DMA
  extension](docs/spi-dma.md) passed its separate main software verification;
  its final-source clean replay had not run at main acceptance.
- L012 additionally: [native four-channel owned SRAM copies and staged UART TX](docs/l012-dma-owned.md) on C8T6/C8U6; generic safe admission remains rejected.
- L031/L052/R031/W031 additionally: [native four-channel safe owned SRAM
  copies and staged byte UART TX DMA](docs/four-channel-dma-owned.md) on the
  eleven exact selections listed below. Their four generic aliases compile
  the controller/UART DMA surface but reject safe DMA/staging without memory metadata.
  [Finite RX and combined/split UART DMA](docs/classic-uart-dma-rx.md) extend those
  eleven exact parts. The Stage93 [SPI1 and L052 SPI2 DMA extension](docs/spi-dma.md)
  passed its separate main software verification; its final-source clean replay
  had not run at main acceptance. Borrowed hardware requests remain excluded.
- F002/F003/L031/R031/W031/L052/L083 additionally: blocking 12-bit ADC,
  with package-qualified analog pins and family-specific reference/supply limits.
  F002 exposes VDD reference and VDD/3 only; no TS or internal reference.
  L010/L011 sequence ADCs provide blocking single conversions and [ordered 1–8 slot scans](docs/adc-scan-sequences.md); L012 dual ADCs provide blocking single conversions and ordered 1–8 slot scans;
  L012 ADC1/ADC2 borrow a common BGR owner and preserve shared analog resources.
  Both also provide finite software async singles/scans with the distinct
  [ADC2 shared-vector admission and fault contract](docs/adc-l012-adc2-async.md).
- Classic ADCs: ordered software-triggered scans with four or eight slots,
  common acquisition timing and sealed owned/borrowed channels. Multi-slot scans
  require unbuffered external inputs; internal and follower-enabled inputs retain
  the single-read API. See [source qualification and limits](docs/classic-adc-scans.md).
- L010/L011: typed BTIM1 UPDATE cascade into BTIM2 and one-shot external ADC trigger,
  with destination-local mappings and exclusive static resource ownership.
  See `docs/trigger-routing-l010.md`, `docs/trigger-routing-l011.md` and `examples/trigger-routing`.
- F002/F003/L031/R031/W031/L052/L083: GTIM polling counter and simple PWM,
  with own-version prescaler handling and 215 qualified family routing entries.
  L010/L011/L012 buffered GTIM also provides counter/simple PWM, with 62 qualified
  family routes and an explicit two-tick minimum period. Mode changes restart the timer.
- AWT families retain HSIOSC polling/IRQ events; LPTIM families retain PCLK
  run-mode events. Exactly F020C6U7, F030C8T7, A030C8T7, L031C8T6/C8U6/F8U6,
  R031C8U6 and W031R8U6 additionally support AWT events with an owned,
  board-qualified `LseClock`. `new_lse` and `new_async_lse` retain SYSCTRL and
  the source pads; `set_config` retains that source. Readiness is a snapshot,
  independent of retained events; source loss supplies no wake and can leave
  `wait()` pending indefinitely. Acquiring/reacquiring `LseClock` may advance
  whole-GPIOC filters/edges, and native L031/R031/W031 FILTER7 recipients can
  change or stall when AWT changes or stops. Drop retains LSE/pad reservations;
  forgetting may leave AWT active, and an independently owned live RTC cannot
  share the same source owner. No low-power wake, deadline or silicon claim.
  See [timer scope](docs/low-power-timers.md), [owned LSE AWT contract](docs/lse-awt.md)
  and [firmware examples](examples/low-power-timers/README.md). Stage80 main
  produced 11 actual ARM libraries and 8 linked ELFs with zero warnings,
  using a successful library prefix and resumed ELF tail on identical source
  and toolchain. Generation, all four finite source/data gates and the
  whole54/905 zero-delta comparison passed separately. Independent runtime and
  metadata/source reviews accepted the frozen main implementation. Early
  assertion/cache failures remain recorded in [the verification history](docs/lse-awt.md#verification-boundary).
  At main acceptance clean had not run; final completion requires a separate
  final-source 2-library/2-ELF clean receipt.
- L052/L083: polling AUTOTRIM periodic down-counter on frozen raw HSIOSC,
  using source-cycle periods and qualified factory clock bounds. Calibration,
  FCAP measurement and deep-sleep timing remain unsupported; see
  `docs/autotrim-counter-evidence.md` and `examples/autotrim-counter`.
- L012: blocking EAU integer division/sqrt and checked-domain Q1.31 CORDIC
  functions with bounded polling; no Q1.15/IRQ/DMA or full-quadrant phase.
  See `docs/l012-math-evidence.md` for exact output scaling and source limitations.
- L012: polling Hall input capture on twelve package-qualified AF9 routes, with
  checked 24-bit periods, filters, clock bounds and explicit coalescing/overflow
  limits. See [Hall capture evidence](docs/halltim-evidence.md) and the
  [firmware example](examples/halltim/README.md).
- All eleven RTC-bearing families: bounded whole-second calendar with preserving
  attachment and explicit initialization. L010 retains frozen HSIOSC and adds
  held LSE on its three qualified exact packages; L011/L012 retain frozen HSIOSC
  and add native LSE on their four exact qualified packages. These same seven
  exact native parts additionally admit an independently owned, already-live
  factory-LSI source with read-only acquisition, SOURCE2 and exact `(1, 16400)`
  divisors. Its calendar /32800 bounds remain rate-only under every SYSCLK;
  generic native aliases remain HSIOSC. L010 new-LSI `set_datetime` refuses
  before KEY/ACCESS writes; guarded domain errors can leave partial state.
  New-source async waits refuse before registration, while synchronous direct
  alarms remain supported. Downstream exhaustive `CalendarClock` matches must
  add the native `Lsi` arm. Stage79 main verification passed twelve actual
  libraries and seven ELF links, all with exit zero. Generation, eight finite
  source/data gates and the 905-file zero-delta comparison passed separately.
  Independent main runtime and source/metadata reviews accepted the frozen
  implementation. Three existing LSE-constant warning headers remain in
  L010/L011 rows. At main acceptance clean had not run; final-package completion
  requires a separate final-source two-library/two-ELF clean receipt. See the
  [native live-LSI contract](docs/native-live-factory-lsi-rtc.md) and
  [ordinary firmware example](examples/rtc-calendar/README.md).
  Classic families normally retain preconfigured factory-trim LSI and its exact
  32800/32768 rate and tolerance. Twenty-three qualified LSE packages can hold an
  explicitly initialized LSE source. The previous sixteen retain their monitor
  checks; native L010/L011/L012 check their selected fault-detection contract around
  calendar operations, with no progression guarantee under `StartupOnly`. Typed weekly Alarm A programming and A/B
  event status/acknowledgement are supported; existing L010/L011/L012 HSIOSC/LSE
  sources also have scoped run-mode async waits. Alarm B mask programming remains contradictory in the
  own manuals/SDKs. No low-power wake or precision wall-clock promise.
  See [RTC source and lifecycle evidence](docs/rtc-remaining-calendar.md) and
  [alarm/event scope](docs/rtc-alarms.md).
- All 13 families: ReadNorFlash and inherent blocking program/erase on 37 exact
  parts, requiring an unsafe exclusive reserved-region/electrical contract.
  Generic unknown-capacity selections remain rejected. Post-trigger waiting is
  unbounded; NorFlash/Multiwrite power-loss guarantees are not implemented.

- All 13 families: package-qualified blocking/async UART RTS+CTS, TX+CTS and
  RX+RTS constructors. These ordinary owners have no software receive FIFO or
  lossless-flow guarantee.
- All 13 families and 54 existing selections: direct `BufferedUartRx`
  and `BufferedUart` owners collect RX through IRQs into bounded storage,
  including between reads. Safe constructors consume static storage; explicit
  unsafe `new_borrowed*` variants accept an ordinary mutable borrow released
  after owner Drop, subject to the documented no-leak/lifetime safety contract.
  Standalone RX Drop permits reuse; combined split halves share a lifetime and
  may both need dropping first. Inherent and async trait reads batch an
  available prefix; `read_exact` fills a slice, while nonwaiting `blocking_read`
  and `read_ready` support polling. Async `BufRead::fill_buf` lends a contiguous
  occupied prefix without copying; `consume` releases only the exposed bytes.
  ISR writes stay disjoint from a held prefix, but prolonged borrowing can cause
  overflow. Reads preserve the prefix before the first error; later observed
  input is discarded until that error is consumed. Empty reads preserve errors.
  Ten families can silently overwrite RDR before ISR service; native ORE on
  L010/L011/L012 and software overflow do not provide an exact loss count.
  Hardware RTS reflects RC, not ring fullness. No buffered idle or DMA ring.
  Stage83 main verification passed thirteen actual ARM libraries, eleven ELF
  links and five focused source/data gates. Independent main runtime and
  metadata/source reviews accepted the frozen implementation. Three existing
  native L010/L011 RCC LSE-constant warnings remain in each applicable row.
  Main ran no hardware generation; all 905 generated files stayed unchanged.
  At main acceptance clean had not run; final-package completion requires a
  separate final-source three-library/three-ELF clean receipt and five focused
  gates. No hardware result is claimed.
  See the [buffered receive contract](docs/uart-buffered-rx.md).
- L010/L011/L012: finite async `read_until_idle` on ordinary owned UART RX and
  combined UART. A full buffer or observed native RXIDLE returns the received
  count; co-latched RC is consumed before idle with best-effort boundary assignment.
  This ordinary idle API adds no software ring, numeric timeout or
  lossless-packet guarantee.
  Stage82 main verification passed six actual ARM libraries, five ELF links and four
  finite source/data gates. Three existing native L010/L011 RCC LSE-constant
  warnings remain in applicable rows. At main acceptance clean had not run;
  completion requires a separate final-source two-library/two-ELF receipt.
  No hardware result is claimed. See
  [the native idle receive contract](docs/uart-idle.md).
- L010/L011/L012: owned buffered GTIM/ATIM input capture and quadrature encoders,
  with modulo counts and coalesced/non-atomic capture observations
- All 13 families: external-input comparator and LVD polling, IR controller
  configuration and passive RAM parity diagnostics. L012 IR selector programming
  remains withheld due to conflicting sources; R031 exposes controller-only IR.
- L010/L011/L012: immutable shared comparator reference banks and typed bank-qualified
  comparator borrows; documented supply/core divider taps, no guaranteed settling
- L052/L083: internal-bias LCD with owned COM/SEG pins and frame observations
- L083: hardware-word AES ECB and raw, unassessed TRNG samples; no CryptoRng claim
- Optional Embassy time on reserved GTIM/GTIM1, in continuous run mode only.
  Every half-boundary must be serviced within 32768 actual ticks; Flash stalls,
  debugger halts and deep sleep cannot preserve elapsed-time accuracy.

Classic F030/A030/F020/F002/F003/L010/L011/L031/R031/W031/L052/L083 I2C
adds owned seven-bit async master transactions alongside blocking operation.
SI phases use IRQ wakeups; bus-idle, STOP and cleanup have synchronous finite
sample budgets, and SI waits have no built-in deadline. Cancelled operations
can leave target-side effects; checked local reset enables later reuse only
after confirmed shutdown. A readback failure remains faulted until chip reset.
Main acceptance covers 13 standalone ARM libraries, 17 linked ELFs and six finite
source/data commands. Final-package completion requires a separate clean receipt;
see [the classic async contract](docs/classic-i2c-async.md).

L083 UART uses partner-safe shared interrupt handlers. L012 I2C separately provides
owned seven-bit blocking and async masters using its own command/FIFO engine.
TXE, RXNE and initial STOP-event waits use interrupts without an internal deadline;
final idle confirmation and retirement use bounded synchronous samples. Adjacent
reads share one receive command, with a 256-byte contiguous-read limit. Cancellation
may drain queued work and leave target effects; reuse requires confirmed local
shutdown/reset and checked restoration. A failed lifecycle readback remains faulted
until chip reset. PCLK-only timing and board electrical qualification remain required.
Stage88 main execution passed six standalone ARM libraries, six linked ELFs and
six finite source/data commands; all 910 generated files were unchanged. L6 keeps
three existing RCC warnings. At main acceptance clean had not run; final-package
completion requires its separate final-source three-library/three-ELF clean receipt.
Independent acceptance is recorded separately; no hardware result is claimed.
See [the L012 async contract](docs/l012-i2c-async.md) and
[ordinary two-controller examples](examples/l012-i2c/README.md).
Package pin availability and family-specific timing/pull capabilities apply.
SPI, I2C and watchdog support does not imply all optional peripheral modes.

See [source-only capability inventory](docs/functional-coverage.md) for current
per-family/peripheral/mode scope. Dated verification remains separate.
The corrected clock sequences passed independent source review and the final
54-feature regression; see `docs/hal-stage10-verification.md`. Register-layout
compatibility alone never authorizes reuse of oscillator sequencing. Package-safe GPIO sets exclude
family-specific debug/reset/input-only pads and reserve radio-connected pads.
Pull/speed capabilities vary by family. Pin routing is generated from reviewed
metadata, not inferred from similarly named peripherals.

On x030/L083, exactly CW32F020C6U7/K6U7/F6U7 and the eleven exact
L031/L052/R031/W031 selections below, plus CW32L012C8T6/C8U6,
`dma::CopyChannel::new`
consumes a physical DMA singleton admitted by normal HAL initialization.
Its safe `copy` method moves the capability and two
exclusive static SRAM buffers into `OwnedCopy`. Only clean completion returns
them; errors permanently consume the owners because TE does not prove bus
quiescence. Forgetting leaks them, and Drop may wait forever if hardware stalls.
Normal reset/clean-runtime entry is required by the platform: init rejects dirty
DMA state but cannot sanitize arbitrary active bootloader handovers. Direct PAC
access remains a technically unsafe ownership escape, as in pinned chiptool.

The existing unsafe `OwnedCopy::new` and borrowed `Transfer` API remain available
for low-level integration. Raw acquisition/downgrade cannot regain safe-copy
admission. On F030/A030/L083, exactly CW32F020C6U7/K6U7/F6U7 and the
eleven exact L031/L052/R031/W031 selections below and CW32L012C8T6/C8U6,
`UartTx::new_with_dma` consumes static staging, UART and channel owners, then
accepts ordinary safe slice writes. It copies each chunk
into private SRAM; cancellation retains the current chunk for a later write or
flush to reap cleanly, while errors permanently quarantine resources. Chunk
gaps and retained-resource costs apply. `flush` separately waits for the wire.
On F030/A030/L083 and the fourteen exact F020/L031/L052/R031/W031 parts,
`UartRx::new_with_dma` receives finite chunks into its own
static SRAM; cancellation retains the chunk and later reads preserve clean
surplus bytes. `Uart::new_with_dma`
prepares once and supports both directions through ordinary split halves. Line
errors report promptly, but recovery must await the old count; receive/rearm/copy
gaps may lose frames with no overrun indication. See [RX and shared lifetime
scope](docs/uart-dma-rx.md). No borrowed zero-copy, safe early-abort, DMA RTS/CTS
or circular reception is provided by these UART DMA APIs. L083 UART1–6 and SPI1–2
use independently [qualified staged requests](docs/l083-peripheral-dma.md);
unsafe borrowed hardware-request constructors remain x030-only.
See [UART TX scope and evidence](docs/uart-dma-tx.md),
`docs/dma-safe-owned-copy.md` and the underlying `docs/dma-owned-copy-evidence.md`.

F020 uses its native two-channel controller and independent DMACH1/DMACH2 IRQs.
The three exact parts have 8 KiB SRAM at `0x20000000..0x20002000`; generic F020
cannot admit safe DMA or staging. UART1/2/3 TX and RX routes remain
package-qualified: F6 UART1 has no safe direction, and PA14 remains excluded.
[Stage92 finite RX and combined/split DMA](docs/classic-uart-dma-rx.md) use
RX selectors 0/2/4 and existing TX selectors 1/3/5 on either native channel.
Stage93 SPI1/SPI2 paired DMA has separate main software acceptance as scoped
below; its final-source clean replay had not run at main acceptance.
Borrowed hardware-request APIs remain excluded.
DMA UART construction does not install an RX ring or combine with a buffered
owner on the same UART. See [the own-F020 contract](docs/f020-dma-owned.md) and
[ordinary F020 copy/TX firmware](examples/f020-dma/README.md). The historical
Stage84 copy/TX main passed 15 actual ARM library builds and 16 ELF links; all 31 commands exited zero.
Generation and the five finite source gates passed in documented segments,
and independent runtime and metadata/source reviews accepted the main work.
Only the L010F8P6 and L011K8T6 library rows emitted warnings, three existing
RCC constants each; all F020 rows and all ELF links were warning-free.
At main acceptance clean had not run; final-package completion requires a
separate final-source 2-library/4-ELF clean receipt. See
[the verification boundary](docs/f020-dma-owned.md#verification-boundary)
for the preserved source-gate failures and generated delta. No hardware
result is claimed.

The native four-channel cohort is exactly CW32L031C8T6/C8U6/F8P6/F8U6/K8U6/K8V6,
CW32L052C8T6/R8S6/R8T6, CW32R031C8U6 and CW32W031R8U6, each with
64 KiB flash and 8 KiB SRAM. CH1 uses DMACH1, CH2/3 share DMACH23 and
CH4 uses DMACH4; there is no CH5. Generic `cw32l031`, `cw32l052`,
`cw32r031` and `cw32w031` retain `MissingMemoryMetadata` rejection.
UART1/2/3 TX uses selectors 1/3/5 and RX uses 0/2/4 on any of the four
channels, subject to existing safe package pins. The two L031 F8 packages have
no safe UART1 TX pad; [all eleven TX example routes](docs/uart-dma-tx.md#native-four-channel-cohort)
preserve that exclusion. L031F8U6 UART1 permits RX-only on PA9/AF2 or PC15/AF6;
L031F8P6 has no safe UART1 RX. Combined UART requires distinct safe TX/RX pins
and distinct channel owners. Stage93 SPI1 and L052 SPI2 paired DMA has separate
main software acceptance as scoped below; its final-source clean replay had not
run at main acceptance. No other peripheral request,
buffered-plus-DMA UART owner or borrowed hardware constructor is added.
Existing raw software-copy APIs keep their unsafe contracts. R031 GPIOA
whole-bank effects and W031 internal RF boundaries remain functional handover
limits; these MCU paths establish no RF-safe handover or coexistence.
See [the four own-source contracts](docs/four-channel-dma-owned.md) and
[ordinary four-channel firmware](examples/four-channel-dma/README.md).
The historical Stage85 copy/TX main passed six standalone ARM library builds
and 21 ELF links,
all 27 commands exiting zero without warnings. Generation, five finite source
gates and the capability renderer passed in documented segments; three source
checker preparation failures remain recorded. At main acceptance clean had
not run; final-package completion requires a separate final-source
2-library/4-ELF clean receipt. See [the verification boundary](docs/four-channel-dma-owned.md#verification-boundary).
No hardware result is claimed.

F020/F002/F003/L010/L011/L012/L031/R031/W031/L052 additionally provide
ordinary-borrowed `Spi<'d, Async, Master>::new_async` and all
five async `SpiBus<W>` methods on their fourteen SPI instances, now supporting
the existing sealed 4–16-bit PIO word set. The Stage93
F020/L031/L052/R031/W031 DMA extension and Stage94 native L012 extension
preserve these borrowed PIO constructors and fix each owner's backend at construction. This IRQ PIO path keeps at most
one frame outstanding; the task accesses DR and caller buffers, while the ISR only records errors, masks local notification and wakes.
Each async poll performs at most one DR read and one DR write. BUSY has no
completion interrupt, so the final wire-idle wait self-wakes and consumes CPU;
there is no built-in deadline or low-power guarantee. Preserve the original CS
selection after cancellation and successfully flush before changing devices.
Driver Drop and forgotten-owner reconstruction are destructive, not successful
wire completion. The ten-family common blocking constructor/Drop path also gains
the lifecycle fence needed to retire a forgotten async owner; ordinary blocking
data/error/divider behavior remains separate. L012 requires the full SPI23
handler for SPI2/SPI3; W031 external SPI cannot coexist with internal RF use.
See [the IRQ PIO contract](docs/spi-async-pio.md) and
[ordinary firmware examples](examples/spi-async/README.md). Historical Stage89
main validation passed 14 standalone ARM libraries and 19 linked ELFs,
all 33 commands exiting zero,
plus seven finite source/data commands. No canonical generation ran; all 910
generated files remained unchanged. The L010F8P6 library, async and blocking rows
and the L011K8T6 library and async rows each retain three existing RCC LSE-constant
warnings; the other 12 libraries and 16 ELFs were warning-free. Independent
runtime and source/metadata reviews accepted the frozen implementation. At main
acceptance clean had not run; final-package completion requires a separate
final-source two-library/five-ELF replay and seven commands including its one
generation bootstrap. These results establish no hardware behavior.

Stage93 main software verification passed thirteen standalone ARM libraries
and twenty-four linked ELFs, with all 37 Cargo invocations exiting zero. The
extension adds F020 SPI1/SPI2, L031 SPI1, L052 SPI1/SPI2, R031 SPI1 and W031 SPI1
to the existing F030/A030/L083 staged DMA cohort: five new families, seven new
family/controller pairs, fourteen exact parts and twenty selected
package/controller routes. That Stage93 DMA scope covered eight families and
thirteen family/controller pairs. Five finite source/data gates and the separate source
navigation refresh passed; no canonical generation ran and all 910 generated
files stayed unchanged. Independent runtime and metadata/source reviews accepted
the bounded main result. The original M16 Cargo link passed but artifact
retention failed with ENOSPC; reviewed recovery preserved that failure, reused
M01–M15, retained the original M16 outputs without rerunning Cargo and built only
M17–M37. L010F8P6/L011K8T6 library and async-PIO ELF rows each retain three existing
unused RCC LSE-constant warnings; the other eleven libraries and twenty-two ELFs
were warning-free. At main acceptance the separate final-source two-library/
six-ELF clean replay had not run. See [the main verification and recovery
boundary](docs/spi-dma.md); historical receipts above do not validate this change,
and no hardware result is claimed.

Stage94 adds native SPI1/SPI2/SPI3 DMA on exactly CW32L012C8T6/C8U6:
one family, three controller pairs, two parts and six selected routes. The
current declaration totals nine DMA families and sixteen controller pairs.
Native RX/TX selectors are 6/7, 8/9 and 10/11; native requests use CR2 while
classic profiles retain CR1. The existing 24 MHz/even-divisor L012 clock policy
and GPIO/package limits remain. Main passed six standalone ARM libraries and
fifteen ELF links in 21 accepted Cargo rows, all exiting zero, plus five finite
source/declaration commands. Independent runtime and metadata/source reviews
accepted the bounded implementation. At main acceptance the final-source
two-library/six-ELF clean replay had not run; final completion requires its
separate receipt. Main ran no canonical generation; clean requires one existing
bootstrap with all 910 generated files matching. The failed M07 Flash link,
reviewed companion-only s-to-z profile correction and native L012
new_admitted_pair warning remain recorded in [the acceptance boundary](docs/spi-dma.md).
No hardware result is claimed.

`Spi<'static, Async, Master>::new_with_dma` consumes static SPI/pin owners,
two private static staging buffers and two admitted channels. One common
lifetime-generic async `SpiBus<u8>` surface dispatches to the constructor's
installed PIO or DMA backend; it adds no borrowed DMA construction or backend
switching. The six dual-capable families, including L012, gain the existing DMA-gated
variants in the already `#[non_exhaustive]` `ConfigError` and `Error` enums.
PIO signatures remain; the conditional public variant set expands. DMA now
requires `Binding<T::Interrupt, T::InterruptHandler>`; generic wrappers written
against `InterruptHandler<T>` may need the associated-handler bound or an
appropriate type equality. Concrete dedicated bindings retain their handler.
L012 SPI2/SPI3 require the complete shared SPI23 handler, dispatching PIO2,
DMA2, PIO3 and DMA3 with active-before-MMIO guards. Both PIO and DMA peers
remain usable together. Both channel admissions are checked atomically before
either is claimed. A channel rejection leaves both admission states unchanged;
inputs remain consumed, and later SPI configuration failure does not roll back
successful pair admission.

Both clean DMA terminals and wire idle precede success. Cancellation retains
the active chunk: keep the original CS selection and await successful `flush`
before changing devices. DMA/SPI errors permanently retain the pair, while PIO
keeps its destructive error recovery and Drop. F020's two channels permit one
permanently admitted pair, so SPI1/SPI2 DMA examples are separate deployments;
clean Drop does not restore admission. Exact SRAM, package pins and clock
limits remain required. L012's four channels permit two reserved DMA pairs,
with PIO on a third controller. Unresolved Drop retains SPI, pins, both channels,
both stages and clocks; it cannot justify CS release. Generic `cw32l012`
retains missing-memory staging/admission rejection despite its compile surface.
L012 UART stays TX-only and its separate RX RC/ORE proof stays blocked. R031 GPIOA handover limits remain; W031 external SPI
remains unavailable during internal RF use. No RF coexistence is established.
Chunk gaps, copies and receive-overrun risk under contention remain; generic
cancellation-safe `SpiDevice` composition and silicon validation are not claimed.
See [SPI DMA lifecycle and evidence](docs/spi-dma.md) and the ordinary
[staged classic SPI firmware examples](examples/spi-dma/README.md) and
[native L012 deployments](examples/l012-spi-dma/README.md).

```sh
cargo check -p embassy-cw32 --no-default-features --features cw32f030c8t7,rt \
  --target thumbv6m-none-eabi
./ci/check-hal.sh
```

The HAL check builds all 54 chip selections for ARM with `rt` and `rt,defmt`,
then links the source-qualified firmware groups listed in that script. HAL unit/integration tests,
models and compile-contract harnesses have been removed. No firmware is flashed
or executed. See `docs/hal-production-layout.md` for the upstream layout mapping
and the historical verification boundary.
Unsupported scope includes additional ADC/timer modes, peripheral DMA beyond
the separately declared staged UART and SPI profiles (including the Stage93
five-family SPI extension with main software acceptance and the Stage94 native
L012 extension with separate main software acceptance), external/PLL modes beyond each explicitly
qualified family/source and low-power policy. Check the coverage ledger before
selecting a device/API.

Exactly CW32F030C8T7, CW32F030K8T7, CW32F030K8U7 and CW32A030C8T7
provide immutable owned HCLK/PCLK output on PA8 AF4 through `rcc::Mco`, retaining
the real SYSCTRL and PA8 owners. C8T7 uses LQFP48 pin 29; F030K8T7 uses LQFP32
pin 18 and F030K8U7 uses QFN32 pin 18. The K8 packages have no PC14/PC15 bonds
or LSE routes. Generic family/density aliases and all other packages remain
unqualified. All eight documented output divisors remain available; the two
ordinary examples choose /8 and the runtime algorithm is unchanged.

Admission uses published cycle-qualified logical bounds and an explicit
compatible board supply/ambient/load declaration. The conservative 20 MHz,
2.4–5.5 V, −40–105°C and nonzero total load at most 50 pF policy does not prove
loaded duty, edge timing or receiver suitability. Each family's own I/O table
is unmeasured design/simulation data, conditional on loaded duty 45–55% and
`tr + tf <= 2T/3`; package-specific current/power/thermal limits still apply.
A030 uses its own datasheet and the shared x030 RM; the F030 SDK is supplementary.
GPIOA gate activity, pin handover, bounded cleanup/Drop, forgetting and live RTC
ownership restrictions remain in the [complete contract](docs/f030-owned-mco.md)
and [examples](examples/mco/README.md).

Stage76's accepted five-library/three-ELF main results, original reviews and
separate one-library/one-HCLK-ELF clean scope remain historical C8T7 evidence.
At Stage76 main acceptance, clean replay had not run. Stage77 main verification
passed seven real ARM library builds and five linked ELFs: all twelve commands
exited zero with no warnings. Eight finite source/data gates and independent
main runtime and metadata reviews passed; the build-input snapshot of 1,330
authored and 905 generated files stayed unchanged during the matrix. At main
acceptance final-source clean had not run; completion requires a separate
two-library/two-ELF and finite-gates receipt. `./d check-mco` remains the current
7/5 local recipe, not an aggregate execution receipt. No hardware output or
loaded waveform is qualified by these software checks.

## Data and PAC verification

- `./d check`: reproducible regeneration plus per-chip target checks
- `./d test`: generator/schema and current data/PAC tests
- `./d audit-current`: current official source facts, projections and distribution checks
- `./d lint`: optional module-layout checks
- `python3 tests/validate_pac_inventory.py`: complete inventory/reference closure
- `python3 tests/check_pac_access.py`: compiler-enforced register-access contracts
- `python3 cw32-data/tools/coverage.py`: refresh catalog-to-output coverage

See [validation scope](docs/validation-scope.md) for historical-review and runtime limits.

Run commands from the repository root. Generated source presence and a successful
compile are distinct milestones; actual run results are recorded separately.

## Verification levels

1. **Catalogued**: official product listing found; no register compatibility claim.
2. **Source acquired**: a pinned vendor file was downloaded and hashed.
3. **PAC generated**: data imported and Rust code rendered through chiptool.
4. **Host/target checked**: specific software checks passed as recorded.
5. **Hardware validated**: physical board tests passed for the named device/revision.

No hardware-validation level is claimed. A successful Rust compilation cannot
prove a register map, peripheral timing, or silicon behavior correct.

## Licensing and source acquisition

Project-authored Rust code uses MIT OR Apache-2.0. Upstream permissions are
component-scoped. Earlier cw32-data-serde and cw32-data-macros adaptations had
unresolved source-package license evidence; the five affected live files now
have independently written, technically reviewed replacements. Original origin
and hashes remain in [the per-file ancestry review](docs/upstream-file-provenance.json).
This does not establish a license for the upstream predecessors or constitute
legal clearance. The release guard checks the exact reviewed replacement bytes
and excludes historical source copies. Vendor firmware,
SVD, and PDF documents have their own terms. Raw vendor archives stay under ignored
`sources/vendor/` or outside this repository; they are not bundled as project-licensed
source. Acquisition manifests preserve URLs/hashes so input provenance is auditable.
See `docs/upstream.md` and source notes for limitations and known vendor corrections.

### CW32L012 DAC and OPA

The source-qualified analog-output slice adds direct 8/12-bit DAC output and OPA
follower, PGA and external-feedback modes. It owns every participating package
pin, retains shared analog resources, checks inherited competing pad drivers,
and never promises startup/settling from typical/minimum figures. See
[`docs/dac-opa.md`](docs/dac-opa.md) and the four actual firmware programs in
[`examples/l012-analog`](examples/l012-analog). The original isolated candidate required
integration review; its compile/link receipts do not establish silicon behavior.

Stage15 integrates the reviewed DAC/OPA and AES/TRNG slices. See [scoped verification](docs/hal-stage15-verification.md) and [current source-only capability inventory](docs/functional-coverage.md) for exact current limits.

Stage16 adds LCD, RAM parity diagnostics and HALLTIM capture, and corrects the documented PAC interfaces. See [verification and limits](docs/hal-stage16-verification.md).

Stage17 adds the reviewed owned DMA path with unsafe entry and the AUTOTRIM counter subset. See [verification and boundaries](docs/hal-stage17-verification.md).

Stage18 expands reserved-region FLASH to 37 exact parts across all 13 families and blocking calendars to all 11 RTC families. See [verification and limits](docs/hal-stage18-verification.md).

Optional Embassy time support uses a reserved GTIM/GTIM1 at nominal 1 MHz.
It requires continuous run-mode clocks and a strict interrupt-blackout bound;
see [the exact contract and source evidence](docs/time-driver.md) and
[real Timer firmware](examples/embassy-time). No deep-sleep timing is claimed.

Stage19 integrates the optional run-mode Embassy time driver and bounded L010 timer/ADC trigger routes. See [verification and limits](docs/hal-stage19-verification.md).

Stage20 adds qualified UART RTS/CTS constructors and buffered timer capture/encoder APIs. See [verification and limits](docs/hal-stage20-verification.md).

Stage21 adds source-qualified Alarm A programming, A/B event handling and scoped async waits on the direct-access RTC variants. See [verification and limits](docs/hal-stage21-verification.md).

Native CW32L010/L011/L012 LSE/calendar qualification and functional handover limits: [qualified-l010-lse.md](docs/qualified-l010-lse.md). The new two-family [own-source contract](docs/qualified-l011-l012-lse.md) retains bounded functional exclusions. Normal crystal, bypass and preserved HSI calendar examples are in [examples/l010-lse-clock](examples/l010-lse-clock).

## Native L012 owned DMA

Exactly CW32L012C8T6/C8U6 provide owned static-SRAM copies and staged byte
UART TX, with Stage94 adding the separately scoped SPI1/SPI2/SPI3 paired DMA
above. Stage94 has separate main software acceptance; at main acceptance
final-source clean had not run and requires its separate receipt.
Each has 64 KiB Flash and 8 KiB SRAM; generic `cw32l012` retains safe admission
and staging rejection for missing memory metadata. Its native descriptor has
separate programmed CNT and current CCNT, and RESTART/SRCLOAD/DSTLOAD stay
zero. CH1/2 share DMACH12 IRQ9 and CH3/4 share DMACH34 IRQ10; IRQ11 is CORDIC.
UART1/2/3 use PA9/PA7/PA8 AF1 respectively on both packages. Only clean own
TC without TE plus STATUS5 permits request closure and staging reuse; software
copy additionally requires SOFTSRC0. Errors permanently retain resources,
forget leaks and Drop may wait indefinitely. Flush separately waits for TXBUSY0.

Native PAC SIZE/STATUS/TRANS/TYPE fields become typed enums; CCNT register
handles become read-only and CSR TC/TE setters are removed. STATUS stays RW.
Existing callers of these PAC fields must use the corrected types/access.
No DMA RX/combined UART, other unqualified hardware request or early-abort API
is added. SPI's native DR-read-clear proof does not resolve UART RX RC/ORE.
Stage86 main passed four standalone ARM libraries and eleven ELF links,
all fifteen commands exiting zero with no warnings. The 1,413 authored files,
910 generated files, toolchain and Cargo configuration stayed unchanged.
Generation and the DMA projection check passed in one successful segment;
the own-source checker, provenance, renderer, consistency and layout checks
passed in a separate five-command tail. Two earlier preparation failures are
preserved: current provenance misread retained historical source identities,
and a CSR prefix also matched CSRCADDR. The precise corrections preserve full
source identity and all five CSR access assertions; no original or hardware
fact was changed. At main acceptance clean had not run; final-package completion
requires a separate final-source two-library/four-ELF clean receipt.
See [the own-L012 contract](docs/l012-dma-owned.md) and [ordinary firmware](examples/l012-dma/README.md).
