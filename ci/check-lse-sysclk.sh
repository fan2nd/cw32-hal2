#!/usr/bin/env bash
# Local, explicitly invoked production-library and firmware check matrix.
# Counts below describe planned scope, not completed runs; record each executed subset separately.
# No hosted-CI wiring, HAL tests or hardware execution.
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_INCREMENTAL=0
# Seventeen libraries: classic3, exact5 L031/R031/W031, exact3 L052, exact5 L083,
# plus excluded F020F6U7 (ordinary library compatibility, not LSE SYSCLK admission).
for chip in cw32f020c6u7 cw32f030c8t7 cw32a030c8t7 cw32f020f6u7 cw32l031c8t6 cw32l031c8u6 cw32l031f8u6 cw32r031c8u6 cw32w031r8u6 cw32l052c8t6 cw32l052r8s6 cw32l052r8t6 cw32l083rbt6 cw32l083rct6 cw32l083rcs6 cw32l083mct6 cw32l083vct6; do
  cargo check --locked --manifest-path firmware/Cargo.toml -p embassy-cw32 \
    --target thumbv6m-none-eabi --no-default-features --features "$chip,defmt"
done
# Sixteen exact packages, each with crystal and bypass SYSCLK+RTC firmware (32 binaries).
for chip in cw32f020c6u7 cw32f030c8t7 cw32a030c8t7 cw32l031c8t6 cw32l031c8u6 cw32l031f8u6 cw32r031c8u6 cw32w031r8u6 cw32l052c8t6 cw32l052r8s6 cw32l052r8t6 cw32l083rbt6 cw32l083rct6 cw32l083rcs6 cw32l083mct6 cw32l083vct6; do
  cargo build --release --locked --manifest-path examples/lse-sysclk/Cargo.toml \
    --target thumbv6m-none-eabi --no-default-features --features "$chip,defmt" --bins
done
cargo build --release --locked --manifest-path examples/lse-clock/Cargo.toml \
  --target thumbv6m-none-eabi --no-default-features --features cw32f030c8t7 --bin crystal_calendar
cargo build --release --locked --manifest-path examples/lsi-clock/Cargo.toml \
  --target thumbv6m-none-eabi --no-default-features --features cw32f020c6u7
cargo build --release --locked --manifest-path examples/hse-clock/Cargo.toml \
  --target thumbv6m-none-eabi --no-default-features --features cw32f030c8t7 --bin crystal
cargo build --release --locked --manifest-path examples/pll-clock/Cargo.toml \
  --target thumbv6m-none-eabi --no-default-features --features cw32a030c8t7 --bin pll-uart
