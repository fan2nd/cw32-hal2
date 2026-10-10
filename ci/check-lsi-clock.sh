#!/usr/bin/env bash
# Local ordinary-library/firmware compilation, not HAL tests or silicon execution.
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_INCREMENTAL=0
for chip in cw32f002f3p7 cw32f002f3u7; do
  cargo build --locked --manifest-path firmware/Cargo.toml -p embassy-cw32 \
    --target thumbv6m-none-eabi --no-default-features --features "$chip,defmt,time-driver-gtim"
  cargo build --release --locked --manifest-path firmware/Cargo.toml -p embassy-cw32 \
    --target thumbv6m-none-eabi --no-default-features --features "$chip,defmt,time-driver-gtim"
  cargo build --release --locked --manifest-path examples/lsi-clock/Cargo.toml \
    --target thumbv6m-none-eabi --no-default-features --features "$chip,defmt"
done
for chip in cw32f002 cw32f003; do
  cargo check --locked --manifest-path firmware/Cargo.toml -p embassy-cw32 \
    --target thumbv6m-none-eabi --no-default-features --features "$chip,defmt"
done
for chip in cw32f020c6u7 cw32f030c8t7 cw32a030c8t7; do
  cargo check --locked --manifest-path firmware/Cargo.toml -p embassy-cw32 \
    --target thumbv6m-none-eabi --no-default-features --features "$chip,defmt,time-driver-gtim1"
  cargo build --release --locked --manifest-path examples/lsi-clock/Cargo.toml \
    --target thumbv6m-none-eabi --no-default-features --features "$chip,defmt"
done
cargo build --release --locked --manifest-path examples/rtc-calendar/Cargo.toml \
  --target thumbv6m-none-eabi --no-default-features --features cw32f030c8t7 --bin preserve_calendar
cargo build --release --locked --manifest-path examples/hse-clock/Cargo.toml \
  --target thumbv6m-none-eabi --no-default-features --features cw32f020c6u7 --bin crystal
cargo build --release --locked --manifest-path examples/pll-clock/Cargo.toml \
  --target thumbv6m-none-eabi --no-default-features --features cw32a030c8t7 --bin pll-uart
for chip in cw32f002f3p7 cw32f003f4p7; do
  cargo build --release --locked --manifest-path examples/hex-clock/Cargo.toml \
    --target thumbv6m-none-eabi --no-default-features --features "$chip" --bins
done
