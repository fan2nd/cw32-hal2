#!/usr/bin/env bash
# Local recipe: seven actual ARM libraries and five ordinary firmware ELFs.
# Counts are recipe scope, not execution receipts; final-source clean is separate 2+2.
# No HAL tests, probes, register models, hosted CI or firmware execution.
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_INCREMENTAL=0

cargo build --locked --manifest-path firmware/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32f030k8t7,rt,defmt --release -p embassy-cw32 --lib

cargo build --locked --manifest-path firmware/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32f030k8u7,rt,defmt --release -p embassy-cw32 --lib

cargo build --locked --manifest-path firmware/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32a030c8t7,rt,defmt --release -p embassy-cw32 --lib

cargo build --locked --manifest-path firmware/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32f030c8t7,rt,defmt --release -p embassy-cw32 --lib

cargo build --locked --manifest-path firmware/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32f030,rt,defmt --release -p embassy-cw32 --lib

cargo build --locked --manifest-path firmware/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32a030,rt,defmt --release -p embassy-cw32 --lib

cargo build --locked --manifest-path firmware/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32f030f8v7,rt,defmt --release -p embassy-cw32 --lib

cargo build --locked --manifest-path examples/mco/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32f030k8t7 --release --bin hclk

cargo build --locked --manifest-path examples/mco/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32f030k8u7 --release --bin pclk

cargo build --locked --manifest-path examples/mco/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32a030c8t7 --release --bin hclk

cargo build --locked --manifest-path examples/mco/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32f030c8t7 --release --bin pclk

cargo build --locked --manifest-path examples/rtc-calendar/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32a030c8t7 --release --bin preserve_calendar
