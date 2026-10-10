# Production build checks

Run `./ci/check-hal.sh` from the repository root. Python 3.11+, Rust and the
`thumbv6m-none-eabi` target are required. Repository-local Cargo/Rustup are used
when available; otherwise the normal environment is used.

The script derives all 54 HAL chip selections from `embassy-cw32/Cargo.toml` and
builds optimized ARM libraries with `rt`, then `rt,defmt` (108 builds). It then
links the existing F030 blocking and asynchronous GPIO firmware examples using
their own Cargo/linker configuration, plus both ATIM counter/main-PWM binaries
for eleven exact packages in `examples/atim`. The example phase also links
`examples/l083-crypto` hardware-word AES and raw TRNG firmware for all five exact
L083 packages. It also links L010/L011 EOS-driven ADC single/ordered-scan examples for all five
exact packages with caller timeout/select composition. It does not execute firmware.
The safe staged UART TX DMA sender in `examples/uart-dma-tx` links for
F030C8T7, A030C8T7 and all five exact L083 packages, with its own metadata-derived memory/linker configuration,
flash/local input slices larger than staging, and repeated write/flush calls.
The safe paired SPI DMA firmware in `examples/spi-dma` links for both exact x030
C8T7 packages and all five L083 packages, exercising all byte async bus methods through borrowed slices,
unequal lengths and chunking. Source-only CS/cancellation limits remain; links
do not establish a hardware transaction or generic device-adapter guarantee.
The UART RX/combined group also links the L083 UART1/UART4 shared-vector
application with both partners live. The SPI group includes a L083 SPI2 NOR ID
reader in addition to SPI1 bus operations.

- `--matrix-only`: all declared HAL ARM builds
- `--examples-only`: the real firmware groups declared in the script
- no option: both phases

HAL host/unit/integration tests, register models, synthetic compile-contract and
link-fixture engines were deleted at the user's request. They have not been
moved or replaced. Data, PAC, generator, package-route and source-provenance
checks remain under `./d test`, `./d audit-current` and `./d check`; they are separate from local HAL build checks. See [validation scope](../docs/validation-scope.md).

For input-frozen records, use:

```
python3 ci/run-verified.py --scope hal-matrix \
  --output docs/verification-logs/<batch>/<run> -- ./ci/check-hal.sh
```

The runner records command, log, exit status and before/after source hashes. The
`hal-matrix` scope covers HAL/PAC source, metadata, reviewed pinout inputs consumed by the HAL build
script, Cargo files, local build scripts, structural validator and real examples. All older verification reports describe their dated historical
source snapshots, including tests since deleted. See
`docs/hal-production-layout.md` for the migration and its verification scope.

## Source-only capability inventory

After `./d fetch-sources` and `./d gen`, run
`python3 ci/update-functional-coverage.py` (or the compatible
`ci/update-hal-coverage.py` entry point). `--check` compares local outputs;
`--validate-only` checks without writing. Both reports go to ignored
`build/reports/` and are excluded from packages. No historical verification
JSON or HAL build matrix is required. Maintain `ci/hal-capabilities.yaml` with
source changes; see [the declaration schema and limits](../docs/functional-coverage.md).

`ci/check-classic-atim-complementary.sh` scopes ordinary ARM/library builds to
F030/A030 and buffered regression, plus real ELFs for every exact x030 package.
The new classic API is limited to optional complete pairs, interior duty, fixed
dead time and global MOE. Its own-PDF/data audit is
`ci/verify-classic-atim-complementary-data.py`; it runs under `./d audit-current`.

`./ci/check-lse-rtc.sh` builds the ordinary ARM libraries for thirteen family representatives, the F030 and F020 alias/unbonded exclusions, the previous sixteen qualified x030/F020/L031/R031/W031/L052/L083 packages with defmt, thirty-two LSE firmware links, and fifteen retained calendar firmware links. It does not execute firmware or add HAL tests. This is the script's full declared scope, not a claim that every command was rerun for either the L052 or L083 addition. The three native L010 packages expanded that historical qualification set to nineteen and use the separate ordinary crystal/bypass/HSI calendar examples in `examples/l010-lse-clock`; that directory is not included in this older script. Their distinct StartupOnly/MonitoredExistingRoutes and functional handover limits are documented in [the native L010 contract](../docs/qualified-l010-lse.md). Four subsequently qualified native L011/L012 packages bring the current auxiliary-LSE total to twenty-three; their separate examples remain outside this older script.

`./d audit-current` includes `ci/verify-l052-lse-data.py` for the three exact L052 parts, using their own locked originals and SDK members. The L052 addition retains evidence for five selected ordinary library configurations and ten linked crystal/bypass ELFs (the three L052 parts plus F030C8T7 and L031C8T6 regression). Four library commands and five example-pair commands have retained numeric exit status 0; the earlier C8T6 library pass is supported by its original `Finished`/`PASS` log, with no numeric exit receipt reconstructed. These are software checks only.

`./d audit-current` also includes `ci/verify-l083-lse-data.py` for the five exact L083 parts, with four own locked originals, five SDK members and the native register/package facts. The earlier auxiliary-LSE L083 candidate has retained passes for seven ordinary library configurations and fourteen linked crystal/bypass ELFs: all five L083 parts, L052C8T6 and F030C8T7. Those receipts apply to the frozen candidate source, and do not claim a rerun of the full script or hardware execution. Its source-qualified detector-margin check runs before peripheral acquisition or RCC writes; see the [L083 contract](../docs/qualified-l083-lse.md).

`./d check-lsi-clock` is a reusable future local recipe with twenty library
commands: fifteen actual builds and five historical checks (two generic
F002/F003 and three classic). The builds comprise four F002 (two profiles),
three F003, three exact L031 release, one L031C8T6 debug, generic L031 and
excluded F8P6, and R031C8U6/W031R8U6 release configurations. The exact/representative builds
enable defmt and the appropriate time driver; the generic/excluded L031
builds enable defmt only. Generic aliases still gain no LSI SYSCLK capability.

The recipe links twenty-five ELFs: the previous fifteen (five F002/F003 LSI,
three classic LSI, one HSI/calendar, HSE and PLL each, and four HEX PB0/PB1),
plus three L031 LSI, four shared-backend LSE SYSCLK, L031 HSI/calendar,
L031 auxiliary-LSE/calendar and L031 HSE. L031C8T6/C8U6/F8U6 share the existing
RTC example branch and retain their own −40–85°C envelope.

The focused L031 implementation verification is a different finite scope:
ten actual library builds and ten linked ELFs in the main tree, followed by
two representative libraries and all three new LSI ELFs in one clean replay.
Its exact configurations and limits are in the [L031 contract](../docs/l031-factory-lsi-sysclk.md).
These are planned counts, not execution receipts. The earlier F003 acceptance
scope remains its own historical nine-library/six-ELF main and two/three clean
record. Consult the separate [runtime/source review](../docs/l031-factory-lsi-runtime-review.json)
and [metadata review](../docs/l031-factory-lsi-metadata-review.json) for their
main-source decisions and recorded outcomes. Final clean replay requires a
separate receipt bound to the final packaged source. Recipe counts are not
completion receipts. Do not run the whole reusable script merely
to satisfy the focused scope.

The commands compile production sources and examples; they do not run HAL
tests, prove runtime time-driver/ADC rejection, or execute hardware. Static
review must trace those guards through the actual generated exact-part cfg.
Each example's build.rs derives memory bounds and links with `-Tlink.x`:
F002 has 16 KiB Flash / 2 KiB RAM, F003 has 20 KiB Flash / 3 KiB RAM, and
exact L031 has 64 KiB Flash / 8 KiB RAM.
Inspect each linked ELF's entry, vectors, PT_LOAD regions, Flash use and static
RAM/stack headroom before claiming it fits or flashing it; never expand
memory.x to hide overflow. Startup, error paths and electrical behavior remain
unvalidated. See the [F002 contract](../docs/f002-factory-lsi-sysclk.md) and
[F003 contract](../docs/f003-factory-lsi-sysclk.md) and
[L031 contract](../docs/l031-factory-lsi-sysclk.md). No hosted workflow is added.


`./d check-lse-sysclk` declares a bounded local compile/link scope: twenty-four
ordinary ARM libraries for classic3, exact5 L031/R031/W031, exact3 L052, exact5
L083, exact3 L010, exact2 L011, exact2 L012 and excluded F020F6U7; forty-six crystal/bypass
SYSCLK+RTC firmware links; and four existing
HSI+aux-LSE, LSI, HSE and PLL firmware regressions. It runs no HAL tests or hardware
and adds no hosted workflow. The new example build script derives exact FLASH/RAM from metadata and
passes `-Tlink.x`; inspect each actual ELF's vectors, reset entry and PT_LOAD
regions separately. A compile pass does not execute cold/reuse/reject or fixed
1 MHz time-driver rejection. These numbers describe future local commands, not
completed runs. Record each actual executed subset separately and preserve older
validation receipts. See [the example contract](../examples/lse-sysclk/README.md).

The L052 addition covers only CW32L052C8T6, CW32L052R8S6 and CW32L052R8T6.
The example's exact-feature cfg supplies both independent startup analog fields;
the two firmware modes reuse the same declared source for SYSCLK and RTC. This
is the script's declared local command scope, not evidence that any command ran
or that excluded aliases were exhaustively checked. It adds no runtime
probe, synthetic harness, hosted CI, or physical startup/recovery qualification.
Generated PAC, chip JSON, build reports and compiler outputs remain excluded
from source deliverables. See [the L052 contract](../docs/l052-lse-sysclk.md).

The five L083 system targets add only RBT6/RCT6/RCS6/MCT6/VCT6. They retain
WAIT2 and require minimum VDD at least 1.8 V, covering conservative raw factory
HSI fallback at 48.96 MHz without divider credit. Inherited PLL is stopped only
after an unchanged-ready-HSI bridge and both stop acknowledgments; its outputs
are intentionally stopped. These are the local script's future command scope
and reviewed API conditions, not a claim of completed builds or hardware
qualification. See [the L083 system contract](../docs/l083-lse-sysclk.md).


The Stage61 script scope of twenty libraries and thirty-eight SYSCLK firmware
links is historical. Stage62 adds only CW32L011K8T6/K8U6, with PC14/PC15 on pins
2/3, native Level2/Level10 drives, 16384 startup cycles and StartupOnly. The
example retains HSI /24 and the existing generated-memory/link.x path. Its
3.0–3.6 V, −20…70 °C and 32766–32770 Hz declarations require actual board
qualification. The L011 41000 Hz factory-monitor contract is distinct from the
unchanged legal-LSI HSI-calibration bridge; no monitor is auto-prepared.
First-request checks include UART3; the residual handover explicitly includes
PB0 AF3 HSIOSC_OUT. See [the L011 contract](../docs/l011-lse-sysclk.md).

The existing L011/L012 auxiliary crystal, bypass and HSIOSC-calendar examples in
`examples/l010-lse-clock` remain separate preservation checks outside this
script. Neither the matrix description nor these available commands claim a
build, probe, test or hardware run. Current capability scope is twenty-three
system packages and twenty-three auxiliary packages; generic aliases and other
packages remain excluded. No dependency upgrade or hosted workflow is added.


The L012 addition admits only CW32L012C8T6/C8U6, with PC14/PC15 on pins 3/4.
The native example keeps configured HSI /12 (distinct from reset/fallback /24),
StartupOnly, independent Level2/Level10 drives and 16384 cycles. Every-cycle
frequency, operating conditions, waveform and startup settings are explicit
board declarations requiring qualification, not measured validation. The own
36080 Hz factory monitor is distinct from the unchanged-legal-LSI HSI bridge.
Shared ADC1/ADC2 inspection can resume work, and whole GPIOC, dedicated source
outputs, disputed closed UART3 and downstream timer/cascade observers retain
explicit functional handover limits. The final LSE selection permits no later
CR0 or FLASH write. See [the L012 contract](../docs/l012-lse-sysclk.md).
Stage62's 22-library/42-binary count remains historical; the current local
script plans 24 ordinary libraries and 46 SYSCLK binaries. Each executed subset
and its actual ELF inspection must be recorded independently.
