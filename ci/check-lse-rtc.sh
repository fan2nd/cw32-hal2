#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_RELEASE_DEBUG=0 CARGO_BUILD_JOBS=1
# Ordinary production builds only; this does not execute firmware or HAL tests.
for chip in cw32f030c8t7 cw32a030c8t7 cw32f020c6u7 cw32f020 cw32f020f6u7 cw32f020k6u7 cw32f002f3p7 cw32f003e4p7 cw32l031c8t6 cw32r031c8u6 cw32w031r8u6 cw32l052c8t6 cw32l083mct6 cw32l010f8p6 cw32l011k8t6 cw32l012c8t6 cw32f030c8 cw32f030f6p7; do
    cargo build --offline --locked --manifest-path firmware/Cargo.toml -p embassy-cw32 --release --target thumbv6m-none-eabi --no-default-features --features "$chip,rt"
done
for chip in cw32f030c8t7 cw32a030c8t7 cw32f020c6u7 cw32l031c8t6 cw32l031c8u6 cw32l031f8u6 cw32r031c8u6 cw32w031r8u6 cw32l052c8t6 cw32l052r8s6 cw32l052r8t6 cw32l083rbt6 cw32l083rct6 cw32l083rcs6 cw32l083mct6 cw32l083vct6; do
    cargo build --offline --locked --manifest-path firmware/Cargo.toml -p embassy-cw32 --release --target thumbv6m-none-eabi --no-default-features --features "$chip,rt,defmt"
    cargo build --offline --locked --manifest-path examples/lse-clock/Cargo.toml --release --bins --target thumbv6m-none-eabi --no-default-features --features "$chip"
done
for chip in cw32f030c8t7 cw32f020c6u7 cw32l010f8p6 cw32l011k8t6 cw32l012c8t6; do
    cargo build --offline --locked --manifest-path examples/rtc-calendar/Cargo.toml --release --bins --target thumbv6m-none-eabi --no-default-features --features "$chip"
done
