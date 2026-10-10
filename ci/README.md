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

`./d check-lsi-clock` is a bounded local compile check: three F020/F030/A030
libraries with defmt and the fixed-time-driver cfg, three ordinary factory-LSI
calendar firmware builds, and one existing HSI, HSE and PLL firmware regression
each. These commands compile production sources and examples; they do not run
HAL tests, prove a runtime time-driver rejection, or execute hardware. Each
example's build.rs derives exact memory bounds and links with -Tlink.x. Inspect
the resulting ELF entry, vectors and PT_LOAD regions separately before flashing.


`./d check-lse-sysclk` is a bounded local compile/link entry: twenty ordinary ARM
libraries for the classic3, exact5 L031/R031/W031, exact3 L052, exact5 L083, exact3 L010 and excluded
F020F6U7, thirty-eight crystal/bypass SYSCLK+RTC firmware links, and four existing
HSI+aux-LSE, LSI, HSE and PLL firmware regressions. It runs no HAL tests or hardware
and adds no hosted workflow. The new example build script derives exact FLASH/RAM from metadata and
passes `-Tlink.x`; inspect each actual ELF's vectors, reset entry and PT_LOAD
regions separately. A compile pass does not execute cold/reuse/reject or fixed
1 MHz time-driver rejection. See [the new example contract](../examples/lse-sysclk/README.md).

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
