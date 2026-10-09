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
