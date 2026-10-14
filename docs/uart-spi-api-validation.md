# Buffered UART and SPI DMA admission validation

Validation date: 2026-10-14. Base: `a30b7167b6e389e8014fd6065a25e7aa2314a001`.
Scope: buffered UART partial-read API/error handling and SPI two-channel DMA
admission. No firmware was executed on hardware.

## Environment and source checks

- Rust/Cargo 1.99.0, rustfmt, `thumbv6m-none-eabi` target.
- Official pinned SDK/SVD recovery and `./d gen-all` succeeded in a fresh checkout.
- Changed Rust files passed `rustfmt --edition 2024 --check`.
- `tests/test_dma_metadata.py`: 6 passed, 1 skipped.
- `tests/test_metadata_contracts.py`: 22 passed.
- `tests/test_remaining_serial_af.py`: 7 passed.
- `tests/test_final_serial_af.py`: 10 passed.
- `tests/test_l031_shared_serial_sources.py`: structural/source-independent
  portion passed; optional original PDF/SDK replay skipped.
- A temporary, untracked host harness compiled the actual `RxRing` implementation
  extracted from `usart.rs` with a minimal error-type stand-in. Both tests passed,
  covering wrapped ring positions, empty/partial reads, queued-prefix delivery,
  retained discontinuity errors, and sizes around the final 64-byte batch cap
  (63/64/65/96-byte rings, requests through 128 bytes). It did not execute IRQs,
  MMIO, the async scheduler or a real UART.

Original-manual replay is not a pass: `verify_spi_register_writes.py` could not
run to completion because the fresh checkout lacked the pinned original PDFs.
`verify_buffered_uart_sources.py` was not completed with its original evidence
set. Downloads were stopped rather than making unrelated hardware-source
recovery a prerequisite for this software-only change. No register maps, DMA
routing facts or peripheral hardware qualifications are changed here.

## ARM builds and firmware links

`bash ci/check-hal.sh --matrix-only` passed with exit status 0: **113 ARM
release-library build commands**. All 54 declared chip features passed with
`rt` and `rt,defmt` (108 builds), followed by five GTIM1 time-driver selections:
L083MCT6, L052C8T6, L010F8P6, L011K8T6 and L012C8T6. These are library
builds, separate from the firmware links below.

The matrix and firmware links were run after the final UART batch-cap change;
source hashes were checked again afterward to confirm the tested source did
not change. `git diff --check` also passed.

All **22 affected real-firmware link commands passed**, after the final UART
64-byte batch cap was frozen. Each command used:

```text
cargo build --locked --release --bins --target thumbv6m-none-eabi \
  --manifest-path examples/<group>/Cargo.toml \
  --no-default-features --features <chip>
```

| Group | Exact chip feature selections | Passed commands |
| --- | --- | --- |
| uart-buffered | cw32f030c8t7, cw32f002f3p7, cw32l031c8t6, cw32l052c8t6, cw32l083rct6, cw32l010f8p6, cw32l011k8t6, cw32l012c8t6 | 8 |
| spi-dma | cw32f030c8t7, cw32a030c8t7, cw32f020c6u7, cw32l031c8t6, cw32l052c8t6, cw32r031c8u6, cw32w031r8u6, cw32l083rbt6, cw32l083rcs6, cw32l083rct6, cw32l083mct6, cw32l083vct6 | 12 |
| l012-spi-dma | cw32l012c8t6, cw32l012c8u6 | 2 |

The UART selections include the feature-gated `flow_control` and `shared_vector`
examples. SPI selections include eligible SPI2 firmware; both L012 selections
link all four mixed PIO/DMA configurations. Counts are Cargo invocations, not
binary counts. These examples provide their linker scripts through build.rs.
Existing warnings concern three generated unused `RCC_LSE_SYSCLK_*` constants.

## Boundaries

- Buffered UART storage still requires the existing static lifetime. This change
  does not add safe non-static buffer ownership/reclamation or `BufRead`.
- The 64-byte copy cap bounds work per read-side critical section, not elapsed
  interrupt latency or end-to-end hardware behavior.
- Pair admission validates both DMA channels before pair initialization. It is
  not transactional rollback of the entire SPI constructor or its pin/clock
  configuration.
- Hardware timing, DMA handshakes, IRQ races and UART wire behavior remain
  unverified. No new hardware qualification is claimed.
- The repository's full example suite and unrelated known generator-fixture and
  F002 ADC FLASH-capacity failures were not rerun for this focused change.
