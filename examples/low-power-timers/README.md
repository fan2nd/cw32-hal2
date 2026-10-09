# Run-mode AWT and LPTIM firmware examples

`polling` starts an owned periodic timer and polls its elapsed event. `interrupt` installs a typed interrupt binding and awaits events under the pinned Embassy thread executor. Neither changes GPIOs or enters deep sleep. These are real Cortex-M firmware binaries; compilation/linking is not hardware validation.

Build either binary with an exact package feature, for example:

```sh
cargo build --locked --manifest-path examples/low-power-timers/Cargo.toml \
  --target thumbv6m-none-eabi --no-default-features --features cw32l010f8p6 --bin interrupt
```

Supported selections are the thirteen exact-package features listed in Cargo.toml. The build script derives FLASH/RAM sizes from selected-chip metadata. AWT uses HSIOSC on A030/F002/F003/F020/F030/L031/R031/W031. LPTIM uses PCLK on L010/L011/L012/L052/L083. Their default reload/divider values intentionally produce different periods on different clock trees.

L052/L083 bind a vector shared with BTIM2. This example never enables BTIM2; a real application that does must bind and service its handler too. No constructor/handler/drop unpends or disables a shared NVIC line. Events coalesce. To recover from an LPTIM synchronization failure, stop using the faulted object and reconstruct through a reborrowed singleton, causing an independently owned LPTIM reset.

No pins need wiring for these examples. Observe the `nop` after an elapsed event with a debugger if desired, noting that default debug freeze settings affect timer progress. Do not interpret a successful link as proof of timer, clock-domain, or deep-sleep behavior on a board.
