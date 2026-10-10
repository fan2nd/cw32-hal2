# embassy-cw32

Experimental, source-backed CW32 data/PAC/HAL workspace, modeled on the
`stm32-data → stm32-metapac → embassy-stm32` architecture.

**Active development: verified data/PAC and Embassy-style HAL implementations.**

**This is not complete all-chip/all-peripheral support.** A peripheral appearing in
a generated PAC is not a completed driver. Vendor metadata also contains errors.
See `docs/upstream.md`, the source manifests, and the coverage report before use.

Finite software ADC reads and ordered scans support typed IRQ bindings and
caller-composed deadlines on [L010/L011](docs/adc-low-async.md),
[all classic lines and L012 ADC1](docs/adc-remaining-async.md). L012 ADC2 retains
blocking reads/scans; its ADC2_DAC shared-vector lifetime remains separate.

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
  master SPI, blocking seven-bit master I2C and polling BTIM1–3 counters
- All except L010/L011: polling window watchdog with explicit irreversible start
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
- F020 additionally: blocking 12-bit ADC and GTIM counter/simple PWM
- F002/F003/L031/R031/W031/L052/L083 additionally: blocking 12-bit ADC,
  with package-qualified analog pins and family-specific reference/supply limits.
  F002 exposes VDD reference and VDD/3 only; no TS or internal reference.
  L010/L011 sequence ADCs provide blocking single conversions and [ordered 1–8 slot scans](docs/adc-scan-sequences.md); L012 dual ADCs provide blocking single conversions and ordered 1–8 slot scans;
  L012 ADC1/ADC2 borrow a common BGR owner and preserve shared analog resources.
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
  and add native LSE on their four exact qualified packages.
  Classic families normally retain preconfigured factory-trim LSI and its exact
  32800/32768 rate and tolerance. Twenty-three qualified LSE packages can hold an
  explicitly initialized LSE source. The previous sixteen retain their monitor
  checks; native L010/L011/L012 check their selected fault-detection contract around
  calendar operations, with no progression guarantee under `StartupOnly`. Typed weekly Alarm A programming and A/B
  event status/acknowledgement are supported; L010/L011/L012 also have scoped
  run-mode async waits. Alarm B mask programming remains contradictory in the
  own manuals/SDKs. No low-power wake or precision wall-clock promise.
  See [RTC source and lifecycle evidence](docs/rtc-remaining-calendar.md) and
  [alarm/event scope](docs/rtc-alarms.md).
- All 13 families: ReadNorFlash and inherent blocking program/erase on 37 exact
  parts, requiring an unsafe exclusive reserved-region/electrical contract.
  Generic unknown-capacity selections remain rejected. Post-trigger waiting is
  unbounded; NorFlash/Multiwrite power-loss guarantees are not implemented.

- All 13 families: package-qualified blocking/async UART RTS+CTS, TX+CTS and
  RX+RTS constructors; no software receive FIFO or lossless-flow guarantee
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

L083 UART uses partner-safe shared interrupt handlers. L012 I2C uses its own
command/FIFO engine with bounded recovery and a 256-byte contiguous-read limit.
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

x030/L083 `dma::CopyChannel::new` consumes a physical DMA singleton admitted by
normal HAL initialization. Its safe `copy` method moves the capability and two
exclusive static SRAM buffers into `OwnedCopy`. Only clean completion returns
them; errors permanently consume the owners because TE does not prove bus
quiescence. Forgetting leaks them, and Drop may wait forever if hardware stalls.
Normal reset/clean-runtime entry is required by the platform: init rejects dirty
DMA state but cannot sanitize arbitrary active bootloader handovers. Direct PAC
access remains a technically unsafe ownership escape, as in pinned chiptool.

The existing unsafe `OwnedCopy::new` and borrowed `Transfer` API remain available
for low-level integration. Raw acquisition/downgrade cannot regain safe-copy
admission. F030/A030/L083 `UartTx::new_with_dma` consumes static staging, UART and
channel owners, then accepts ordinary safe slice writes. It copies each chunk
into private SRAM; cancellation retains the current chunk for a later write or
flush to reap cleanly, while errors permanently quarantine resources. Chunk
gaps and retained-resource costs apply. `flush` separately waits for the wire.
`UartRx::new_with_dma` receives finite chunks into its own static SRAM; cancellation
retains the chunk and later reads preserve clean surplus bytes. `Uart::new_with_dma`
prepares once and supports both directions through ordinary split halves. Line
errors report promptly, but recovery must await the old count; receive/rearm/copy
gaps may lose frames with no overrun indication. See [RX and shared lifetime
scope](docs/uart-dma-rx.md). No borrowed zero-copy, safe early-abort, DMA RTS/CTS
or circular reception is provided by these UART APIs. L083 UART1–6 and SPI1–2
use independently [qualified staged requests](docs/l083-peripheral-dma.md);
unsafe borrowed hardware-request constructors remain x030-only.
See [UART TX scope and evidence](docs/uart-dma-tx.md),
`docs/dma-safe-owned-copy.md` and the underlying `docs/dma-owned-copy-evidence.md`.

F030/A030/L083 `Spi::new_with_dma` adds a safe byte `SpiBus` with two private static
staging buffers and two admitted channels. Both clean DMA terminals and wire
idle precede success. Cancellation retains the active chunk: keep the original
CS selection and await successful `flush` before changing devices. DMA/SPI errors
permanently retain the pair. Chunk gaps, copies and receive-overrun risk under
contention remain; generic cancellation-safe `SpiDevice` composition and silicon
validation are not claimed. See [SPI DMA lifecycle and evidence](docs/spi-dma.md)
and the genuine [x030/L083 firmware examples](examples/spi-dma/README.md).

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
Unsupported scope includes additional ADC/timer modes, peripheral DMA beyond staged x030/L083 UART TX/RX and SPI master,
external/PLL modes beyond each explicitly qualified family/source and low-power policy. Check the coverage ledger before selecting a device/API.

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
