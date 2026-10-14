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

## Current distinctions that must survive updates

- Ordered ADC scans exist on all thirteen families: 1–4 slots on
  F002/F003/F020/F030/A030 and 1–8 on the others, including both L012 ADCs.
  Classic scans share sample time and exclude buffered/internal multi-slot use;
  L010/L011/L012 provide per-slot sample times. All classic lines, L010/L011 and
  both L012 ADCs additionally support finite software single reads and ordered
  scans with typed IRQ completion. ADC2 checks actual DAC DMAUDRIE1/2 zero in a
  valid retained clock/reset domain; ordinary direct-loading DAC outputs coexist.
  Only unverified local completion masking permits shared ADC2_DAC disable.
  Stage91 main produced five standalone ARM libraries and ten ELF links in
  sixteen attempts, retaining the failed link and reviewed example-only profile
  correction. At main acceptance clean 2-library/3-ELF replay and final54 report
  binding had not run; final completion requires their separate receipts.
  See [L010/L011](adc-low-async.md), [remaining async ADC scope](adc-remaining-async.md)
  and [ADC2's shared-vector contract](adc-l012-adc2-async.md).
  Continuous scans, hardware-triggered scans and safe ADC DMA remain absent.
- UART ordinary blocking/async and qualified RTS/CTS constructors exist. Pin
  availability differs by instance and exact selection; these ordinary owners
  have no software FIFO or throughput guarantee.
  Direct `BufferedUartRx`/`BufferedUart` owners separately collect
  continuously through IRQs into consumed static bounded storage on all thirteen
  families and 54 existing selections. Batched inherent and async
  trait reads drain the retained clean prefix before returning the first error;
  later ISR observations are discarded until the error is consumed. Async
  `BufRead::fill_buf` borrows a stable contiguous occupied prefix; `consume`
  releases only its exposed bytes. IRQ appends cannot overwrite a held prefix,
  but holding it can cause overflow. Empty reads preserve bytes/errors.
  Ten families can silently overwrite RDR before ISR
  service; native ORE and software overflow do not count every lost frame.
  Hardware RTS follows RC, not software occupancy. No buffered idle or DMA ring,
  lossless guarantee or hardware timing qualification. Stage83 main verification
  passed thirteen actual ARM libraries, eleven ELF links and five focused
  source/data gates; independent main runtime and metadata/source reviews
  accepted the frozen implementation. Three existing native L010/L011 RCC
  LSE-constant warnings remain in each applicable row. Main ran no hardware
  generation, and all 905 generated files stayed unchanged. At main acceptance
  clean had not run; final-package completion requires a separate final-source
  three-library/three-ELF clean receipt and five focused gates. See
  [the buffered receive contract](uart-buffered-rx.md).
  L010/L011/L012 additionally expose finite async native idle reception with
  ordinary `UartRx<Async>`/`Uart<Async>` owners and caller slices. Buffer-full or
  observed RXIDLE returns a count; simultaneous RC is consumed before idle,
  without exact packet attribution.
  First nonempty poll discards only stale idle, preserving unread bytes/errors;
  service delays can still overrun RDR. This idle API adds no numeric timeout or
  software ring.
  Stage82 main verification passed six actual ARM libraries, five ELF links and four
  finite source/data gates. Three existing native L010/L011 RCC LSE-constant
  warnings remain in applicable rows. At main acceptance clean had not run;
  final-package completion requires a separate final-source two-library/
  two-ELF clean receipt. These software results do not establish hardware behavior.
  See [the finite idle contract](uart-idle.md).
  F030/A030/L083 additionally have safe staged TX DMA, finite-chunk RX and combined
  split full-duplex DMA with ordinary caller slices and separate wire flush.
  Exactly CW32F020C6U7/K6U7/F6U7 and eleven exact L031/L052/R031/W031
  selections have staged byte TX and [finite RX/combined split DMA](classic-uart-dma-rx.md).
  The three F020 parts use native CH1/CH2 with separate vectors; the eleven
  four-channel parts retain CH2/3 sharing DMACH23. All five generic aliases
  reject safe DMA/staging with `MissingMemoryMetadata`. F020F6U7/L031F8P6
  have no safe UART1 RX/TX. L031F8U6 has UART1 RX-only on PA9/AF2 or PC15/AF6
  despite its absent safe TX pad. The complete roster and own-source RX
  qualification are in [classic UART DMA RX](classic-uart-dma-rx.md); the
  [existing TX routes](uart-dma-tx.md#native-four-channel-cohort) remain.
  SPI DMA has its separately declared scope; L012 UART DMA remains TX-only.
  Stage92 main passed eight standalone ARM libraries and 21 ELF links without
  warnings; independent runtime and metadata/source reviews accepted the bounded main result. At main acceptance final capability
  reports and the separate final-source two-library/three-ELF clean replay had not run.
  No same-instance buffered-plus-DMA owner or DMA RX ring is introduced.
  Static staging/channel/UART ownership is retained across cancellation; clean
  completion permits reuse and retained RX tails, while DMA errors permanently
  quarantine resources. FE/PE reports promptly but recovery awaits the old count.
  RX gaps can lose frames without overrun detection. No DMA RTS/CTS, continuous
  RX, safe early abort, bounded liveness or zero-copy guarantee.
- F020/F002/F003/L010/L011/L012/L031/R031/W031/L052 SPI adds ordinary-borrowed
  IRQ PIO byte async master on fourteen family/controller pairs, across the
  existing 36 selectors. All five async `SpiBus<u8>` methods use at most one
  outstanding frame and one DR read/write per poll; only task context accesses
  caller buffers or DR. Final TXE/BUSY checks self-wake without a deadline or
  low-power guarantee. Cancellation/forget requires keeping the original CS
  selected through a successful flush; errors and destructive driver Drop do
  not establish successful wire completion. Common blocking constructor/Drop
  code on these ten families also fences forgotten async state. L012 SPI2/SPI3
  require the full SPI23 handler for either PIO or DMA construction and isolate
  each register bank, including an inactive or blocking peer. W031 keeps the existing internal-RF/external-SPI
  ownership exclusion. This PIO profile adds no DMA admission, wider async
  words, new routes or automatic CS/device cancellation guarantee. Historical
  Stage89 main validation passed
  14 standalone ARM libraries and 19 linked ELFs, all 33 exits zero, plus seven
  source/data commands. No canonical generation ran; all 910 generated files
  stayed unchanged. Three existing RCC LSE-constant warnings remain in each
  L010F8P6 library/async/blocking row and L011K8T6 library/async row; the other
  12 libraries and 16 ELFs were warning-free. Independent runtime and
  source/metadata reviews accepted the frozen implementation. At main acceptance
  clean had not run; final-package completion requires its separate final-source
  2-library/5-ELF replay and seven commands including one generation bootstrap.
  No hardware result is claimed. See [the contract](spi-async-pio.md).
- Staged byte SPI DMA covers F030/A030/L083 and the fourteen exact
  F020/L031/L052/R031/W031 parts from Stage93, with Stage94 adding exactly
  CW32L012C8T6/C8U6 SPI1/SPI2/SPI3. The declared total is nine families and
  sixteen family/controller pairs; the native addition is one family, three
  pairs, two parts and six selected routes. Private static TX/RX SRAM and two
  admitted DMA channels isolate caller buffers. Both clean terminals, wire idle
  and fenced retirement precede received-byte access and success. Cancellation
  requires preserving the original CS through successful flush; errors and
  unresolved Drop retain the pair, stages, SPI, pins and clocks. Borrowed PIO
  remains available with a constructor-selected backend. Stage94 has separate
  main software acceptance as detailed below; at main acceptance final-source
  clean had not run and requires its own receipt. No generic cancellation-safe device adapter,
  lossless-throughput or hardware claim follows. See [SPI DMA](spi-dma.md).
- GTIM polling capture/encoder exists across families. Buffered ATIM on
  L010/L011/L012 supports capture/encoder, complementary pairs, constructor-only
  symmetric dead time and software BK1; external BK1 is qualified on L011/L012
  only. Native classic ATIM capture/encoder is separately qualified on
  F030/A030/F003/L031/R031/W031/L052/L083. Six optional independent capture
  halves CH1A/B through CH3A/B use latest-value latches, separate capture and
  sticky overwrite flags, native input filters and explicit acknowledgments.
  Native CH1A/CH1B QEI supports three edge modes and ARR1..65535 with fixed
  PRS=/1; count/direction/OVF/UNDF observations remain non-atomic and coalesced.
  No CH4 external input, capture IRQ/DMA, lossless history, signed position,
  index, or maximum external edge-rate guarantee is added. The new enum and
  CAPnA/B routes reflect own-input qualification and exact package bonding,
  independently of PWM. See [the native contract](timer-event-input.md).
  F030/A030 classic ATIM also supports optional complete A+B pairs, interior
  comparison duty, fixed symmetric dead time and explicit global MOE. It excludes
  duty endpoints, per-pair gating, brake/rearm, inversion and runtime timing
  changes; other classic output modes remain unqualified. No pad-level or
  shutdown safety guarantee is made. See [the classic contract](classic-atim-complementary-evidence.json).
- BTIM1, BTIM2 and BTIM3 support internal-PCLK, continuous run-mode coalesced
  ARR-overflow IRQ events on all thirteen families and 54 existing selections.
  `Timer<'d, T>` remains Blocking; only the typed Async constructor with an
  `OverflowInterruptHandler<T>` binding exposes `wait_overflow()`. BTIM
  `take_overflow()` consumes one retained software-or-hardware event in either
  mode. Successful old Blocking construction also retires a forgotten Async
  owner's static ISR state; failed pure validation leaves that state unchanged.
  Canceling a wait preserves counting, overflow enable and events; forgetting
  an owner can leave them active. Shared L052/L083 BTIM2/LPTIM and L012
  BTIM3/HALLTIM dispatch remains per owner, with no partner access or group
  gate/reset changes. Native UG still reaches TRGO with MMS=0, so retained
  recipients keep their functional handover contract. Other timer IRQ modes,
  DMA, one-shot, general trigger routing, lossless counts, sleep/wake and
  clock-loss recovery remain gaps. The following historical receipts cover
  the earlier BTIM delivery; current GTIM/ATIM events are described separately.
  Main verification passed 16 actual ARM library builds and 23 ELF links,
  all exit zero. Four finite source/data gates passed separately; 1,353 source
  files and all 905 generated files stayed unchanged during the final matrix.
  Three existing native L010/L011 RCC warnings remain in applicable rows.
  Independent main runtime and metadata/source reviews accepted the frozen
  implementation. At main acceptance clean had not run; final-package completion
  requires a separate
  final-source 2-library/6-ELF receipt. Earlier partial/failed attempts retain
  their recorded scope in the linked verification history.
  See [the bounded contract and verification boundary](btim-overflow-irq.md).
- GTIM/ATIM internal-PCLK continuous up-counters expose the existing typed
  `Timer<T, Async>` overflow-wait API on 34 GTIM and eleven ATIM own-family
  associations. A static latch belongs to each bank; even shared L012 GTIM34
  dispatch keeps GTIM3 and GTIM4 independent. Inactive handlers return before
  timer MMIO. Successful constructor reuse retires state before RCC mutation
  after pure validation, including the direct classic complementary path.
  Start/stop retain requests and events. Valid timing changes mask/retire the
  overflow request and synthetic update, then restore request/running state;
  invalid changes leave prior state unchanged. Per-owner drop does not disable
  or unpend a shared vector. Main acceptance covers 16 actual ARM libraries,
  27 ELF links and ten finite source/data gates. The 43 successful ARM rows
  retain the failed M24 lockfile attempt and manifest-only continuation; the
  ten gates retain failed D08 and the existing AF-expectation helper correction.
  Three existing L010/L011 RCC `dead_code` warnings remain in applicable rows.
  At main acceptance clean had not run; final-package completion requires a
  separate final-source 2-library/6-ELF receipt. Source qualification and
  library/ELF links do not establish hardware timing or lossless delivery.
  See [the bounded counter-event contract](timer-event-input.md).
- RTC Alarm A and A/B event observation exist on all eleven RTC families.
  Async waits are limited to existing L010/L011/L012 HSIOSC/LSE sources; new
  native live-LSI waits refuse with `UnsupportedAsyncAlarmSource` before RTC,
  NVIC or waiter state changes. Alarm B mask programming remains source-disputed;
  observing Alarm B flags does not resolve its polarity.
- Native live factory-LSI RTC covers exactly CW32L010F8P6/F8U6/Y8M6,
  CW32L011K8T6/K8U6 and CW32L012C8T6/C8U6. Read-only `LsiClock` acquisition
  requires already software-enabled/stable own factory trim; it owns SYSCTRL
  independently of selected SYSCLK. SOURCE2 and exact divisors `(1, 16400)`
  yield calendar /32800, nominal 1 Hz with own rate-only ranges 0.97–1.03,
  0.90–1.25 and 0.90–1.10 Hz. Ordinary POR startup, watchdog preparation,
  cycle-period bounds, sleep/retention/wake and continuous health are excluded.
  Preserved attachment and explicit stopped-unset initialization use guarded
  register-domain observations; domain loss returns `ClockDomainUnavailable`
  and suppresses later KEY cleanup after that loss. L010 new-LSI correction
  refuses with `UnqualifiedWriteTiming` before KEY/ACCESS writes. Synchronous
  direct alarms remain available. Generic native HSIOSC aliases and default
  constructors retain behavior; exhaustive exact-seven `CalendarClock`
  matches must add `Lsi`. Stage79 main verification passed twelve actual
  libraries and seven ELF links, all with exit zero. Generation, eight finite
  source/data gates and the 905-file zero-delta comparison passed separately.
  Independent main runtime and source/metadata reviews accepted the frozen
  implementation. Three existing LSE-constant warning headers remain in
  L010/L011 rows. At main acceptance clean had not run; final-package completion
  requires a separate final-source two-library/two-ELF clean receipt.
  See the [source contract](native-live-factory-lsi-rtc.md).
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
- CW32L012C8T6/C8U6 add own live-only factory-LSI SYSCLK: original software
  LSIEN, STABLE and nine-bit factory equality; rate-only 29,520–36,080 Hz at
  1.7–5.5 V and −40–85°C, 64 KiB Flash / 8 KiB SRAM. Ordinary POR and
  automatic-only requests are refused. There is no LSI/request/IWDT preparation;
  `Config.lse=None` preserves inherited LSE. Shared ADC work-gate and HSE GPIOF
  windows can advance activity, including PF3 LSI_OUT. Both PB11 AF4 and PF3 AF2
  remain complete metadata routes; the scalar PB11 anchor is nonexhaustive.
  Default native RTC keeps strict raw 96 MHz HSIOSC, while ADC/PWM cycle timing and the
  fixed 1 MHz time driver refuse the rate-only system tree. Generic L012 remains
  excluded; its historical Stage75 L011 exclusion is not a current blanket
  exclusion or rewritten acceptance claim. Main verification passed seven actual libraries,
  six linked ELFs and six finite source/data gates, with independent runtime
  and metadata/source acceptance. At main acceptance clean replay had not run;
  final-package completion requires a separate final-source two-library/two-ELF
  clean receipt. Board qualification remains unverified. See the
  [contract and finite matrix](l012-live-factory-lsi-sysclk.md).
- CW32L011K8T6/K8U6 add a separate live-only factory-LSI contract, with
  64 KiB Flash / 6 KiB SRAM and own rate-only 29,520–41,000 Hz at 1.7–5.5 V,
  VDDA=VDD and −40–85°C. Original software LSIEN, STABLE and ten-bit factory
  equality are required. `Config.hse=None`, `Config.lse=None`, original
  HSEEN/HSECCS/LSECCS=0 and HSI/LSI entry exclude cold startup and monitoring.
  No LSI/request/IWDT operation creates admission. Restored RTC inspection
  rejects SOURCE1 and requires actual raw PSC1 ≥97 plus original factory-ready
  HSI for SOURCE3; default native RTC remains raw 96 MHz HSIOSC. The RM ±10%
  adjustment warning remains alongside the DS −10/+25% factory envelope.
  No new direct LSI_OUT pad or GPIO/UART/LPTIM/IWDT inspection is introduced.
  ADC/PWM strict timing and the fixed 1 MHz time driver refuse rate-only LSI.
  Generic L011 remains excluded. Main seven-library/seven-ELF verification
  passed with all exits zero; generation and seven finite data gates passed
  separately. Three existing LSE-constant warning headers remain. At main
  acceptance clean had not run; completion requires a separate final-source
  two-library/two-ELF receipt. See the
  [contract and finite matrix](l011-live-factory-lsi-sysclk.md).
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
  with 64 KiB Flash / 8 KiB SRAM. Generic L052 stays excluded; the separately
  qualified exact L083 parts are described below.
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
- CW32L083RBT6/RCT6/RCS6/MCT6/VCT6 add a separate exact-five factory-LSI
  SYSCLK capability, with 31,816–33,784 Hz at 1.65–5.5 V, VDDA=VDD and
  −40–85°C. RBT6 has 128 KiB Flash, the others 256 KiB, all with 24 KiB SRAM.
  Generic L083 and native L010/L011/L012 stay excluded. Admission covers six
  UARTs, all six GPIO banks, full RTC/AUTOTRIM/LVD controls, tagged off/on
  LCD/LPTIM work and complete bonded LSI/PLL output rosters. Conservative
  unbonded AF0 guards create no pin tokens. Whole-bank inspection can advance
  unrelated pins, filters, armed events and external loopbacks before failure.
  Cold matching trim still requires full proof; live matching LSI is retained.
  Inherited PLL departure uses a proven HSI/HSE bridge and requires PLLEN0 plus
  both ready0 before reference changes. WAIT2 remains on success; potential
  HSE/LSE fallback uses the undivided 48.96 MHz HSI bound without divider credit.
  Configured factory HSI is checked at final bus divisors before the final
  source-only LSI commit. No later source/Flash/pad/policy write precedes publication.
  Every exact-L083 RTC LSI alias is rate-only under HSI/HSE/PLL/LSE/LSI, with
  its prior numeric bounds and nominal 32800/32768 calendar fraction unchanged.
  Strict duration assertions, ADC ordering and fixed 1 MHz time-driver refusal
  remain distinct source-review obligations; library compilation does not
  execute them. RCC failure publishes no clocks, requires reset and promises
  no rollback or clock-loss/sleep recovery. Main ten-library/ten-ELF verification
  and independent implementation reviews passed. At main acceptance clean had
  not run; final-package completion requires a separate two-library/three-ELF
  clean receipt. The all-54 comparison
  permits five new LSI capability objects and exact bonded PLL_OUT projections,
  keeping old source profiles and RTC numeric facts unchanged. See the
  [L083 contract](l083-factory-lsi-sysclk.md); no hardware result is claimed.
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
  Apart from the exact L031/R031/W031/L052/L083 changes above, other RTC/LSI qualifications
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
- DMA on F030/A030/L083, F020 and L031/L052/R031/W031 retains unsafe
  borrowed software copies and unsafe-entry owned static SRAM copies.
  Safe init-admitted CopyChannel copies require qualified exact SRAM; x030/L083
  keep their existing selections.
  Exactly CW32F020C6U7/K6U7/F6U7 add safe CopyChannel copies on the native
  two-channel controller, with separate DMACH1/DMACH2 IRQs and qualified
  8 KiB SRAM at `0x20000000..0x20002000`. Generic F020 retains
  `MissingMemoryMetadata` rejection; this adds no borrowed hardware requests.
  The native four-channel cohort adds exactly CW32L031C8T6/C8U6/F8P6/F8U6/K8U6/K8V6,
  CW32L052C8T6/R8S6/R8T6, CW32R031C8U6 and CW32W031R8U6, each with
  64 KiB flash and 8 KiB SRAM at the same qualified SRAM interval. Their
  four generic aliases retain missing-memory rejection. CH1 binds DMACH1,
  CH2/3 share DMACH23 and CH4 binds DMACH4, with no fifth channel.
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
  SPI master, plus finite staged UART TX/RX and combined/split owners on the
  fourteen exact F020/L031/L052/R031/W031 parts, whose Stage93 SPI DMA scope
  is separately declared. Exactly L012 C8T6/C8U6 add Stage94 SPI1/SPI2/SPI3
  paired DMA while UART remains TX-only. Qualified SRAM and normal reset/clean-
  runtime entry are required; no other unqualified peripheral consumer is added.
  The classic UART families' own byte-RDR examples, software RC clear and FE/PE without ORE/NE
  qualify finite RX; chunk gaps can lose frames without an overrun count.
  Safe early abort, circular operation and lossless reception remain unavailable.
  Stage92 RX/combined main passed eight libraries and 21 ELF links without
  warnings; independent runtime and metadata/source reviews accepted the bounded main result. At main acceptance final capability
  reports and separate two-library/three-ELF clean replay had not run.
  Other unqualified DMA consumers remain PAC-only. Historical Stage84 copy/TX
  main passed 15 actual ARM libraries and 16 ELF links, all 31 commands exiting zero. Generation
  and five finite source gates passed in documented segments; independent
  runtime and metadata/source reviews accepted the main work. Only L010F8P6
  and L011K8T6 library rows emitted three existing RCC warnings each; F020
  rows and all ELF links were warning-free. At main acceptance clean had not
  run; final-package completion requires a separate final-source
  2-library/4-ELF clean receipt. No silicon behavior is claimed.
  Historical Stage85 copy/TX main passed six standalone ARM libraries and 21
  ELF links, all 27
  commands exiting zero without warnings. Generation, five finite source gates
  and the capability renderer passed in documented segments; three checker
  preparation failures remain recorded. The ARM matrix preserved its 1,402-file
  source snapshot and all 907 generated files. At main acceptance clean had not
  run; final-package completion requires a separate final-source
  2-library/4-ELF clean receipt. R031 whole-GPIOA effects and W031 internal RF
  boundaries preserve existing functional handover obligations; DMA admission
  establishes no RF-safe handover or coexistence. See
  [the four-family contract](four-channel-dma-owned.md),
  [the own-F020 contract](f020-dma-owned.md),
  [the DMA ownership contract](dma-owned-copy-evidence.md),
  [safe copies](dma-safe-owned-copy.md), [UART TX DMA](uart-dma-tx.md),
  [UART RX DMA](uart-dma-rx.md) and
  [SPI DMA](spi-dma.md).
- AWT retains HSIOSC polling and IRQ-backed coalesced events on its eight
  families; LPTIM retains PCLK run-mode events on L010/L011/L012/L052/L083.
  Only CW32F020C6U7, CW32F030C8T7, CW32A030C8T7, CW32L031C8T6,
  CW32L031C8U6, CW32L031F8U6, CW32R031C8U6 and CW32W031R8U6 additionally
  admit `new_lse`/`new_async_lse` with a real held `LseClock`. The current
  canonical AWT IPs are `awt_cw32f030_v1` for F020/F030/A030 and
  `awt_cw32l031_v1` for L031/R031/W031; F002/F003 keep `awt_v1` and their
  HEX selectors. Generic aliases and other packages gain no LSE AWT API.
  `set_config` preserves the source; readiness is only a current predicate
  snapshot, independent of UD/software event retention. A retained event can
  survive source loss, while a wait without an event can remain pending forever
  because loss provides no driver wake. LseClock acquisition/reacquisition can
  temporarily enable whole GPIOC and advance retained filters/edges even before
  an error; AWT health checks do not open GPIO banks. L031/R031/W031 FILTER7
  uses AWT underflow, so takeover, reconfiguration, stopping, Drop and source
  loss can change or stall inherited filtering. Forgetting can leave counting
  and IRQs active. Drop/error retain source enables, faults and pad reservations;
  this API cannot share the source with an independently owned live RTC.
  No low-power wake, monotonic deadline, automatic fallback or hardware
  validation follows. Stage80 main produced 11 actual ARM libraries and 8
  linked ELFs with zero warnings, from a successful library prefix and resumed
  ELF tail on identical source/toolchain. Generation, four finite source/data
  gates and the whole54/905 zero-delta comparison passed separately. Independent
  runtime and metadata/source reviews accepted the frozen main implementation;
  early assertion/cache failures remain recorded. At main acceptance clean had
  not run; final completion requires a separate final-source 2-library/2-ELF
  clean receipt. See [the LSE AWT contract](lse-awt.md) and
  [the existing timer contracts](low-power-timers.md).
- The optional Embassy time driver reserves the whole GTIM/GTIM1 plus IRQ and
  requires continuous run mode and service strictly within 32768 actual ticks.
  HSI budgets are <31.207619 ms at ±5% or <32.125490 ms at ±2%; qualified HSE uses
  its own bounds. Flash stalls, debugger stops and deep sleep can lose elapsed
  time. These are required bounds, not measured latency guarantees.
- Owned MCO is one partial SYSCTRL mode for exactly CW32F030C8T7,
  CW32F030K8T7, CW32F030K8U7 and CW32A030C8T7, each on PA8/MCO/AF4.
  Generic family/density aliases and every other package remain outside the
  API. The required-signal filter does not promote family AF discovery to
  package qualification. Holding real SYSCTRL and PA8 selects immutable
  initialized HCLK or PCLK with /1, /2, /8, /64, /128, /256, /512 or /1024. Published cycle-qualified
  bounds, source-compatible explicit board conditions and exact maximum-rate
  division must pass. Rate-only PLL/LSI roots are rejected. The 20 MHz and
  0 < load <= 50 pF policies do not establish the vendor table's loaded duty or
  edge conditions; its values are explicitly unmeasured design/simulation data.
  Shared GPIOA progress, non-restored PA8 function/lock, bounded error/Drop
  observations and forgotten output remain functional limits. Live safe RTC
  clock ownership is excluded by SYSCTRL; forgotten hardware is not permanently
  excluded. There is no source/divider mutation, oscillator or bus-clock change,
  recovery, other package or hardware-validation claim. Historical Stage76
  C8T7-only main verification passed five actual ARM library builds and three
  ordinary firmware links with no warnings,
  plus generation, the full projection comparison, 19 schema contracts and eight
  finite source/data gates. At main acceptance clean had not run; final-package
  completion required a separate clean 1-library/1-HCLK-ELF receipt. These
  historical results do not certify the later four-part scope. The local
  recipe remains separate from these results. See [the current MCO contract](f030-owned-mco.md).
- RF remains explicitly user-deferred on R031/W031.

Every row has `hardware_validated: false`. This tool performs no compilation,
board execution, electrical validation or automatic semantic proof. No
completion percentage is calculated. Source, build and runtime evidence remain
separate, and a complete advanced-mode hardware audit remains open.

## Native L012 owned DMA

The original Stage86 scope on exactly CW32L012C8T6/C8U6 provides owned
static-SRAM copies and staged byte UART TX. The separate Stage94 native SPI
addition and its separate main acceptance are described below.
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
is added. SPI DR-read-clear evidence does not resolve UART's separate RC/ORE
receive proof.
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
See [the own-L012 contract](l012-dma-owned.md) and [ordinary firmware](../examples/l012-dma/README.md).

## Classic I2C asynchronous master

F030/A030/F020/F002/F003/L010/L011/L031/R031/W031/L052/L083 share the
source-qualified classic SI engine across five existing register versions and
17 family/controller slots. Owned seven-bit async master transactions retain
package-qualified pins and expose inherent methods plus embedded-hal-async.
L012 has its own command/FIFO blocking and async backend, described separately
below. Ten-bit, slave and I2C DMA remain unimplemented.

Only START/address/data event waits are asynchronous. Bus-idle, STOP and cleanup
use finite synchronous sample budgets; no built-in SI deadline exists. Live
arbitration never authorizes STOP, and cancellation makes no rollback or physical
bus-repair promise. Verified local shutdown permits checked exclusive reset and
configuration before reuse; failed readback is sticky across reconstruction.
Main acceptance covers 13 actual standalone ARM libraries and 17 linked ELFs,
with six finite source/data commands passed and all 910 generated files unchanged.
The final example profile is size-optimized `z`; the libraries retain their
separate firmware profile. Earlier failed attempts and superseded example links
remain historical evidence. L010/L011 retain three existing RCC warnings each.
At main acceptance clean had not run; final-package completion requires a separate
six-library/six-ELF clean receipt. No behavioral HAL test, lifecycle probe, or
hardware validation is claimed. See
[classic-i2c-async.md](classic-i2c-async.md).


## L012 command/FIFO asynchronous master

CW32L012 adds owned seven-bit async master transactions on I2C1 and I2C2, including
ordinary inherent methods and embedded-hal-async. The family alias remains a
library selection; executable examples admit only the qualified CW32L012C8T6
and CW32L012C8U6 packages with 64 KiB Flash and 8 KiB RAM. Both ordinary binaries
use I2C1 PA4/PA5 AF2 and I2C2 PB0/PB1 AF8, including adjacent reads through the
real async trait. This instantiates those routes, not every promoted AF route.

TXE, RXNE and initial STOP-event waits are asynchronous and have no internal
deadline. Initial bus-idle sampling, final STOP/idle confirmation, retirement,
reset and Drop use finite synchronous samples. The per-instance ISR masks its
vector and wakes; it never accesses I2C MMIO or a caller buffer. All requests
remain disabled between waits and in blocking operation.

Cancellation may drain queued/active work and preserve target effects. MEN-only
disable precedes bounded drain; FIFO reset follows confirmed local idle, with
held master RESET as the fallback. A later call restores configuration only from
a confirmed stopped state. Failed shutdown/reset/configuration is sticky across
replacement owners until chip reset. No rollback, physical bus repair or
multi-master noninterference is promised. A read run remains limited to 256
bytes, timing remains PCLK-only, and board rise/filter/electrical assumptions
remain necessary. DMA, slave and ten-bit APIs, retries, new clock sources,
low-power behavior, RF and silicon validation are outside this scope.

Stage88 main execution passed six actual standalone ARM libraries, six linked
ordinary ELFs and six finite source/data commands. All 910 generated files stayed
unchanged. L6 (`cw32l011k8t6,rt`) retains three existing RCC warnings. Independent
acceptance is recorded separately; at main acceptance clean had not run and
final-package completion requires a separate final-source three-library/three-ELF
clean receipt. No hardware result is claimed. The shared blocking/async retirement
order corrects a source-qualification gap, not a reproduced silicon defect.
See [the L012 contract](l012-i2c-async.md) and
[ordinary firmware](../examples/l012-i2c/README.md).

## Stage92 classic UART DMA receive extension

The current extension adds finite RX and combined/split UART DMA to exactly
three F020 and eleven L031/L052/R031/W031 parts using the existing receive
engine. Generic SRAM rejection, all safe-pin filters and L012 TX-only UART DMA
remain. The [contract and own-source evidence](classic-uart-dma-rx.md) record
main verification of eight standalone ARM libraries and 21 ELF links, all 29
commands exiting zero without warnings. Both finite source checks passed after
the preserved initial D01 PIN/AFR comparison failure and its checker-only
correction. Main ran no canonical data generation; all 910 canonical generated files
stayed unchanged. Independent runtime and metadata/source reviews accepted the bounded main
result. At main acceptance
final capability reports and separate final-source two-library/three-ELF clean
replay had not run. The UART/DMA runtime algorithms are unchanged; no hardware
or all-DMA-complete claim follows.

## Stage94 native L012 SPI DMA main acceptance

Exactly CW32L012C8T6/C8U6 add finite full-duplex eight-bit master DMA on
SPI1/SPI2/SPI3, retaining the existing borrowed PIO API. Native RX/TX selectors
are 6/7, 8/9 and 10/11. CR2 request RMW preserves sampled EN and unrelated bits;
classic gates remain in CR1. SIZE0 selects byte SRAM elements, not CPU byte MMIO
or a peripheral-access width. The 24 MHz maximum, even divisors 2..256, selected
package routes and GPIO handover restrictions remain unchanged.

Generic `cw32l012` exposes the compile surface but retains
`MissingMemoryMetadata` safe staging and clean-init admission rejection.
Admission remains sequential; failure admitting the second channel can consume
the first admission. Four physical channels permit at most two reserved DMA
pairs, with PIO available on the third controller. Clean Drop does not restore
admission. SPI2/SPI3 require the complete associated SPI23 handler, whose
PIO2/DMA2/PIO3/DMA3 visits each check activity before MMIO. Inactive/gated peers
are skipped; no teardown disables/unpends shared vectors or changes sibling
banks. DMA now requires `Binding<T::Interrupt, T::InterruptHandler>`; generic
wrappers constrained to `InterruptHandler<T>` may need the associated bound or
an appropriate type equality despite preserved concrete dedicated handlers.

The existing five async byte methods retain CS and cancellation obligations:
keep the original device selected through successful flush. Both clean channel
terminals, error-free TXE1/BUSY0 and both fenced retirements precede receive-byte
access. Errors quarantine permanently; unresolved Drop retains both channels,
stages, SPI, pins and clocks without a hidden wait. PIO keeps its separate
destructive recovery/Drop. L012 UART remains TX-only, with RX/combined DMA
blocked on its separate RC/ORE proof.

Stage94 main passed six standalone ARM libraries and fifteen ELF links in
21 accepted Cargo rows, all exiting zero, plus five finite source/declaration
commands. Independent runtime and metadata/source reviews accepted the bounded
implementation. Main ran zero canonical generation. At main acceptance
final-source clean had not run; completion requires its separate two-library/
six-ELF receipt after one existing generation bootstrap, with all 910 generated
files required to match main by path and bytes.
Historical Stage86/89/93 receipts keep their original scope and do not accept
Stage94. No hardware execution, throughput, cancellation-safe generic device
adapter or finite-liveness claim follows. See the [native contract](spi-dma.md)
and [native source ledger](l012-spi-dma-evidence.json).

## Fixed baseline and final 54-selection reconciliation

The Stage91 agreed baseline contains these ten bounded deliveries. Earlier accepted
stages retain their original source, main, clean and publication receipts; the
table does not turn those historical builds into new Stage91 commands.

| Stage | Bounded delivery | Selection/application boundary | Disposition |
| --- | --- | --- | --- |
| 82 | [Finite UART idle receive](uart-idle.md) | Native L010/L011/L012, ten selections and qualified RX routes | Prior accepted scope |
| 83 | [IRQ UART bounded ring receive](uart-buffered-rx.md) | All 13 families/54 selections, instance/pad and storage constraints | Prior accepted scope |
| 84 | [Native F020 DMA copies and UART TX](f020-dma-owned.md) | Four selections; safe memory/staging only exact C6U7/F6U7/K6U7; F6 UART1 lacks safe TX | Prior accepted scope |
| 85 | [Native four-family DMA copies and UART TX](four-channel-dma-owned.md) | L031/L052/R031/W031, 15 selections; 11 exact memory-qualified parts | Prior accepted scope |
| 86 | [Native L012 DMA copies and UART TX](l012-dma-owned.md) | Three selections; safe memory/staging only C8T6/C8U6 | Prior accepted scope |
| 87 | [Classic seven-bit I2C async master](classic-i2c-async.md) | Twelve families/51 selections, qualified instances, pins and timing | Prior accepted scope |
| 88 | [L012 seven-bit I2C async master](l012-i2c-async.md) | Three selections; both native command/FIFO controllers | Prior accepted scope |
| 89 | [SPI byte async master](spi-async-pio.md) | IRQ PIO on ten families/36 selections/14 family-controller pairs; retain prior F030/A030/L083 staged-DMA scope | Prior accepted scope |
| 90 | [Timer overflow and native classic capture/QEI](timer-event-input.md) | 34 GTIM and 11 ATIM event associations; classic ATIM inputs on eight families; prior BTIM/buffered scopes retained | Prior accepted scope |
| 91 | [L012 ADC2 finite software async single/scan](adc-l012-adc2-async.md) | Three selections; checked DAC IE-zero admission, direct DAC coexistence, distinct shared-vector fault policy | Main accepted within scope; at main acceptance final clean and final54 binding had not run |

The 54-selection semantic source/data review was completed against its recorded
inputs. At main acceptance the final source/report binding and final-source
clean replay had not run; final completion requires their separate receipts.
This overall exit check stays within Stage91, separate from its accepted main
five-library/ten-ELF scope and required clean two-library/three-ELF replay. It is
not a new 54-build campaign or evidence of every mode operating on every part. The selected roster remains
13 family aliases, four F030 density aliases and 37 exact-package selections
across 13 families. Generic/density aliases must not acquire invented package
or memory guarantees, and alias routes remain common-package intersections.

At final acceptance, reconcile every selected chip's identity, memory, register
kind/version/address, complete interrupt owners, RCC domains, DMA requests and
qualified routes against current canonical data and its accepted generated
correspondence. Join those facts to the maintained per-family/peripheral mode
declarations and the ten scopes above. The source-only renderer's family/mode
rows and per-selection routes do not independently establish simultaneous pin
compatibility, memory admission, shared-owner policy or source interpretation.

The final freeze must bind current input hashes and both existing deterministic
reports after their normal update/check. Main preserved canonical hardware
inputs and all 910 accepted generated paths/bytes; final clean must establish
its own correspondence. That generated-data check is separate from capability
reporting. Neither stale ignored reports nor an unchanged file count establishes
final currentness. Main acceptance alone does not close the fixed baseline;
separate final-source clean and final54 binding receipts are required.

RF and low-power/deep-sleep/wake integration remain explicitly user-deferred.
Existing unimplemented sleep/wake declarations still mean no API is supplied.
Advanced SPI slave modes, I2C slave/ten-bit, ADC continuous/general-triggered/DMA/
coupled modes, full external-reference ownership, and additional timer
capture-IRQ/DMA/cascade/history/complementary modes remain outside this baseline.
Keep previously qualified narrow trigger and timer modes intact. Peripheral DMA
remains limited to the declared admitted copy/UART/SPI cohorts; circular/scatter,
broader requests, lossless receive and safe early-abort/bounded-drop guarantees
remain absent or experimental. RTC Alarm B and L012 IR MOD remain source-disputed;
R031 IR output remains route-unqualified. All `hardware_validated` values remain
false, and unaudited exact hardware modes retain `mode_hardware_present: null`.
