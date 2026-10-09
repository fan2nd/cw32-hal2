# Next verified batch: module layout, watchdog and additional serial families

The final combined regression passed on 2026-10-08 at 06:21:31 UTC.
All 1,177 tested input files and executable flags were unchanged.
See `hal-stage4-verification.md` and `verification-logs/stage4/`.

## Module organization

All project-owned file-backed Rust modules now use `<module>/mod.rs`. This
includes HAL modules/backends/tests and generator helper modules. Generated PAC
peripherals, register descriptions, per-chip PAC/metadata and shared metadata
follow the same organization directly from their generator. Crate entrypoints
retain standard Cargo names. Inline namespaces are not separate files.

`tests/test_module_layout.py` passes for all 476 Rust source files and checks
explicit path attributes. `./d check` passed all 54 PAC selections and reproduced
all generated data/PAC bytes without changing curated YAML.

## Hardware-meaningful cfg names

All active custom predicates describe a chip family, peripheral IP version or
specific capability. Generic HAL-prefixed labels were removed, and shared RCC/
GPIO branches now use exact version unions. The naming validator checks all 46
declared predicates. The first integrated run was stopped when this additional
requirement arrived; only the fresh renamed-source run can validate delivery.

## Independent watchdog

All 13 families now have Embassy-style IndependentWatchdog new/unleash/pet,
with bounded fallible variants. Hardware-specific behavior requires START before
configuration, so new stores configuration and unleash applies it. Clock-source
and tolerance groups follow each datasheet, including distinct L010/L011 LSI
limits. No implicit feed/stop/reset on Drop. Independent review found no
functional defect; L011 protocol evidence remains SDK-only where its manual is
unavailable. Timing estimates assume documented electrical and calibration
conditions; they are not a measured wall-clock guarantee.

## UART, SPI and I2C

L031/R031/W031 gain blocking and interrupt-driven UART using their actual
R1W0 ICR masks. L031/R031/W031/L052/L083 gain blocking SPI/I2C; their SPI ceiling
is conservatively min(PCLK/4, 12 MHz), resolving contradictory source limits.
There are no new DMA-backed or buffered serial APIs.

675 new serial AF routes are verified against each family's datasheet/manual.
Five SDK defects/omissions are resolved through explicit coordinate evidence;
39 SDK-only radio/unbonded routes remain excluded. Package-safe constructor
availability is narrower than peripheral existence: the generic L031 alias
has no safe I2C SCL and no distinct safe UART3 full-duplex pair; some small exact
packages lack UART1 TX or both UART1 routes. These restrictions are retained,
not bypassed with unreviewed debug/radio pins.

## Additional PAC correction

031 UART TIMCNT is now read-only, per each of the three manuals §18.9.6.
Offset, width and field layout remain unchanged. This corrects an SDK/SVD access
permission error present in earlier checkpoints; new driver code never writes
TIMCNT. PAC compiler-negative checks now include all three UARTs in all three
families, for 27 total forbidden-access checks.

No board was flashed or run. Full remaining per-family/peripheral scope remains
in `hal-coverage.json`.
