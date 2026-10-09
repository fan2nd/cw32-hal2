# Safe finite-chunk UART DMA firmware

These real Cortex-M0+ applications use the production HAL, generated PAC,
pinned Embassy executor and real UART/DMA interrupts. The UART2 applications
support F030/A030 and all five qualified exact L083 packages.

- `cw32-uart-dma-example`: PA2 TX / PA3 RX on x030, or PA6 TX / PA7 RX on L083;
  independent static 32-byte staging,
  DMA channels 2 and 3 on their shared DMACH23 vector. It first uses `split_ref`,
  then owned `split` halves to repeatedly exchange 16-byte packets concurrently.
  RX is polled before TX. Both caller buffers are ordinary local arrays.
- `receive_only`: PA3 RX on x030 or PA7 RX on L083, static 16-byte staging and
  DMA channel 1. It repeatedly
  receives fixed packets through the standalone `UartRx::new_with_dma` API.
- `shared_vector` (L083 only): UART1 PA8 TX / PA9 RX and UART4 PA4 TX / PB5 RX
  exchange independent 16-byte packets concurrently. UART1 uses TX channel 2
  and RX channel 1; UART4 uses TX channel 5 and RX channel 4. All four private
  32-byte staging allocations and both UART owners remain live outside the
  joined exchanges and wire-completion flushes. DMACH1, DMACH23 and DMACH45
  bind the handlers for all four selected channels.

On L083, UART2 binds `SharedInterruptHandler<UART2_UART5>` once and leaves the
unconstructed UART5 partner inactive. The two-port example binds exactly one
`SharedInterruptHandler<UART1_UART4>` for both constructed partners. Neither
application binds the x030 single-instance UART handler on L083. All selected
pins are bonded on each of the five exact L083 packages.

Connect a voltage-compatible serial peer with common ground. Default settings
are 115200 baud, eight data bits, no parity and one stop bit. A PA2-to-PA3 x030
or PA6-to-PA7 L083 loopback can exercise the UART2 exchange application. For
`shared_vector`, use separate PA8-to-PA9 and PA4-to-PB5 loopbacks, or two
independent peers. Both ports must finish a packet before the next round begins.
This is not continuous-stream or lossless validation. A peer must account for
finite armed chunks and rearm/copy
gaps. Neither example enables RTS/CTS, and both halt on errors. Check the board
schematic for other devices on these pins.

Choose one exact package, after generating the PAC normally:

```sh
cargo build --offline --locked --release --bins --target thumbv6m-none-eabi \
  --manifest-path examples/uart-dma/Cargo.toml --no-default-features --features cw32f030c8t7
cargo build --offline --locked --release --bins --target thumbv6m-none-eabi \
  --manifest-path examples/uart-dma/Cargo.toml --no-default-features --features cw32a030c8t7
for chip in cw32l083rbt6 cw32l083rcs6 cw32l083rct6 cw32l083mct6 cw32l083vct6; do
  cargo build --offline --locked --release --bins --target thumbv6m-none-eabi \
    --manifest-path examples/uart-dma/Cargo.toml --no-default-features --features "$chip"
done
```

Each exact L083 feature enables the example-local `l083` switch, which includes
`shared_vector` in `--bins`; that switch alone is not a chip selection. The
generic `cw32l083` selection is deliberately absent because it has no exact
qualified SRAM extent for safe staging admission.

The build script writes a package-qualified `memory.x` from generated metadata
and supplies `-Tlink.x`. Compilation and linking do not demonstrate silicon
execution, error timing, throughput, losslessness or liveness. These applications
are not HAL tests, register models, cancellation harnesses or early-abort APIs.
See [the RX and shared ownership contract](../../docs/uart-dma-rx.md).
