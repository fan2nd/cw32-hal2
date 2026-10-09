> Historical software-verification references: HAL tests and fixture harnesses were deleted on 2026-10-08. Counts and commands below apply to the dated source snapshot. See [current HAL layout and build scope](hal-production-layout.md).

# Stage 4 verified source checkpoint

The fresh cfg-clean `./ci/check-hal.sh` run passed, exit 0, from
2026-10-08 06:12:29 to 06:21:31 UTC (542 seconds).

- 54 host chip selections across 13 families; 108 ARM release builds
- 5,845 host test executions (repeated across selections, not unique cases)
- 82 typed-contract invocations and 2,093 expected compiler failures
- 360 UART031 and 770 shared SPI/I2C route checks
- 25 fully linked/inspected ELF configurations
- 476-file module-layout and 46-predicate cfg-naming checks
- Zero unexpected errors and zero compiler warnings

All 1,177 code/data/test/CI/example input files and executable flags remained
unchanged during the run. Source manifest SHA256:
`c219a32e98cb518448d8d0e117645f03cb18004898f8c054d1f458ccceebc4d1`.
Narrative reports, root README/licenses, toolchains and generated build/cache
outputs are outside that source-input comparison. No firmware was executed.
The interrupted pre-rename run is not used as evidence for this checkpoint.

Full machine-readable counts, per-family results, exact ELF sizes and logs:
`verification-logs/stage4/hal-final-summary.json` and `hal-final-report.txt`.
Data/PAC tests and all 54 regeneration/metadata/ARM selections also passed;
27 compiler-negative PAC access checks include the corrected 031 TIMCNT.

The batch adds all-family independent watchdog, three-family UART and five-family
SPI/I2C support, 675 reviewed serial routes, directory-based module organization
and hardware-meaningful cfg names. See `stage4-changes.md` for exact API/source
limitations and `hal-coverage.json` for every remaining peripheral gap.

Clock initialization retains the documented stable-entry/electrical conditions.
Watchdog timing uses electrical-table bounds under documented calibration and
operating conditions; L011 protocol evidence is SDK-only. No board validation,
safe borrowed-buffer DMA API, or all-peripheral completion is claimed.
