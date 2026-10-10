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
for chip in cw32f003f4p7 cw32f003f4u7 cw32f003e4p7; do
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

# L031 exact3 and shared-backend regressions. This is a future local recipe,
# not an implementation acceptance receipt or an instruction to run it in full.
for chip in cw32l031c8t6 cw32l031c8u6 cw32l031f8u6; do
  cargo build --release --locked --manifest-path firmware/Cargo.toml -p embassy-cw32 \
    --target thumbv6m-none-eabi --no-default-features --features "$chip,defmt,time-driver-gtim1"
  cargo build --release --locked --manifest-path examples/lsi-clock/Cargo.toml \
    --target thumbv6m-none-eabi --no-default-features --features "$chip,defmt"
done
cargo build --locked --manifest-path firmware/Cargo.toml -p embassy-cw32 \
  --target thumbv6m-none-eabi --no-default-features --features cw32l031c8t6,defmt,time-driver-gtim1
for chip in cw32l031 cw32l031f8p6; do
  cargo build --release --locked --manifest-path firmware/Cargo.toml -p embassy-cw32 \
    --target thumbv6m-none-eabi --no-default-features --features "$chip,defmt"
done
for chip in cw32r031c8u6 cw32w031r8u6; do
  cargo build --release --locked --manifest-path firmware/Cargo.toml -p embassy-cw32 \
    --target thumbv6m-none-eabi --no-default-features --features "$chip,defmt,time-driver-gtim1"
done
for chip in cw32l031c8t6 cw32r031c8u6 cw32w031r8u6; do
  cargo build --release --locked --manifest-path examples/lse-sysclk/Cargo.toml \
    --target thumbv6m-none-eabi --no-default-features --features "$chip" --bin crystal
done
cargo build --release --locked --manifest-path examples/lse-sysclk/Cargo.toml \
  --target thumbv6m-none-eabi --no-default-features --features cw32l031f8u6 --bin bypass
cargo build --release --locked --manifest-path examples/rtc-calendar/Cargo.toml \
  --target thumbv6m-none-eabi --no-default-features --features cw32l031c8t6 --bin preserve_calendar
cargo build --release --locked --manifest-path examples/lse-clock/Cargo.toml \
  --target thumbv6m-none-eabi --no-default-features --features cw32l031c8t6 --bin crystal_calendar
cargo build --release --locked --manifest-path examples/hse-clock/Cargo.toml \
  --target thumbv6m-none-eabi --no-default-features --features cw32l031c8t6 --bin crystal

# R031 exact1 additions to the future recipe. The R031 release library and
# selected-LSE crystal ELF already appear above. These rows are not receipts.
cargo build --locked --manifest-path firmware/Cargo.toml -p embassy-cw32 \
  --target thumbv6m-none-eabi --no-default-features --features cw32r031c8u6,defmt,time-driver-gtim1
cargo build --release --locked --manifest-path firmware/Cargo.toml -p embassy-cw32 \
  --target thumbv6m-none-eabi --no-default-features --features cw32r031,defmt
cargo build --release --locked --manifest-path examples/lsi-clock/Cargo.toml \
  --target thumbv6m-none-eabi --no-default-features --features cw32r031c8u6,defmt --bin cw32-lsi-clock-example
cargo build --release --locked --manifest-path examples/rtc-calendar/Cargo.toml \
  --target thumbv6m-none-eabi --no-default-features --features cw32r031c8u6 --bin preserve_calendar
cargo build --release --locked --manifest-path examples/lse-clock/Cargo.toml \
  --target thumbv6m-none-eabi --no-default-features --features cw32r031c8u6 --bin crystal_calendar
cargo build --release --locked --manifest-path examples/hse-clock/Cargo.toml \
  --target thumbv6m-none-eabi --no-default-features --features cw32r031c8u6 --bin crystal
