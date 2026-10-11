#!/usr/bin/env bash
# Future local recipe: five actual ARM libraries and three ordinary firmware ELFs.
# Counts are not execution receipts; final-source clean replay is separately 1+1.
# No HAL tests, probes, register models, hosted CI or firmware execution.
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_INCREMENTAL=0

cargo build --locked --manifest-path firmware/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32f030c8t7,rt,defmt --release -p embassy-cw32 --lib

cargo build --locked --manifest-path firmware/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32f030c8t7,rt,defmt,time-driver-gtim1 -p embassy-cw32 --lib

cargo build --locked --manifest-path firmware/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32f030,rt,defmt --release -p embassy-cw32 --lib

cargo build --locked --manifest-path firmware/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32a030c8t7,rt,defmt --release -p embassy-cw32 --lib

cargo build --locked --manifest-path firmware/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32f020k6u7,rt,defmt --release -p embassy-cw32 --lib

cargo build --locked --manifest-path examples/mco/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32f030c8t7 --release --bin hclk

cargo build --locked --manifest-path examples/mco/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32f030c8t7 --release --bin pclk

cargo build --locked --manifest-path examples/rtc-calendar/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32f030c8t7 --release --bin preserve_calendar
