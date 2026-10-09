# Staged SPI DMA firmware

The primary Cortex-M0+ application supports CW32F030C8T7, CW32A030C8T7 and the
five exact L083 packages RBT6, RCS6, RCT6, MCT6 and VCT6. It
owns SPI1 (PA5 SCK, PA7 MOSI, PA6 MISO), PB0 active-low chip select, DMA channel
3 for TX and higher-priority channel 2 for RX. Bind both DMA handlers on DMACH23
and the SPI1 error handler. Normal reset/clean-runtime `hal::init` is required.

The two singleton SRAM buffers have different capacities (16/24). Ordinary
stack slices and a flash slice exercise unequal-length transfers, in-place
replacement, reads, writes, empty calls and multi-chunk transfers through all
five `embedded_hal_async::spi::SpiBus<u8>` methods. RX beyond the caller's read
length is discarded; missing TX is zero. No expected-device-data assertion is
made. Arbitrary byte traffic is suitable only for a device/fixture selected and
configured for this example; review its protocol and electrical limits before
flashing. An ELF link does not prove a physical transaction worked.

The L083-only `spi2_flash_id` application uses SPI2 (PA2 SCK, PA1 MOSI, PA0
MISO), PB0 active-low CS, TX channel 5 and higher-priority RX channel 4. It binds
both DMA handlers on DMACH45 and the independent SPI2 handler. All these pins,
and those used by SPI1, are bonded on every selected L083 package.

Connect a voltage-compatible SPI NOR flash supporting the mode-0 JEDEC ID
command `0x9f` and status-register-1 command `0x05`, with common ground and any
required HOLD/WP wiring. Check the device's datasheet and board schematic first.
The default requested SCK is 1 MHz. The application reads three ID bytes with
an in-place transfer, then repeatedly writes a status command and reads one
byte under the same CS selection. It never issues a flash program, erase or
write-enable command. ID/status bytes pass through `black_box` for inspection;
their values are not asserted. Both logical writes and reads use paired byte
DMA, including receive discard and zero dummy transmission. The 16/24-byte
private buffers remain owned throughout. L083 retains its existing maximum
12 MHz SCK policy and supported divisors 4 through 128.

Keep the original CS selection through any cancellation, then await successful
`bus.flush()` before deselecting or selecting another device. A generic device
adapter that drops CS or releases its bus lock on cancellation cannot infer this
property from `SpiBus`. The example does not cancel operations; on error it
halts while retaining the selection and poisoned bus. Completed chunks may
already be visible before later cancellation/error. Chunk boundaries can pause
arbitrarily; DMA arbitration can still cause receive overrun.

Build from the repository root with the project's Rust/Cargo toolchain:

```sh
cargo build --offline --locked --release --bins --target thumbv6m-none-eabi \
  --manifest-path examples/spi-dma/Cargo.toml --no-default-features --features cw32f030c8t7
cargo build --offline --locked --release --bins --target thumbv6m-none-eabi \
  --manifest-path examples/spi-dma/Cargo.toml --no-default-features --features cw32a030c8t7
for chip in cw32l083rbt6 cw32l083rcs6 cw32l083rct6 cw32l083mct6 cw32l083vct6; do
  cargo build --offline --locked --release --bins --target thumbv6m-none-eabi \
    --manifest-path examples/spi-dma/Cargo.toml --no-default-features --features "$chip"
done
```

The exact L083 features automatically enable the example-local `l083` switch
so `--bins` includes `spi2_flash_id`. Select an exact chip, not that switch
alone. Generic L083 lacks an exact qualified SRAM extent and is not offered.

The build script derives FLASH/RAM from the selected package metadata and links
with the real Cortex-M runtime. See [source and lifecycle evidence](../../docs/spi-dma.md).
