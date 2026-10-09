#!/usr/bin/env bash
# Ordinary production builds and genuine scan/trigger firmware; no HAL tests.
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_HOME="$PWD/.cargo" RUSTUP_HOME="$PWD/.rustup"
export PATH="$CARGO_HOME/bin:$PATH"
export CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_RELEASE_DEBUG=0
./ci/check-hal.sh --matrix-only
for chip in cw32l010f8p6 cw32l010f8u6 cw32l010y8m6 cw32l011k8t6 cw32l011k8u6; do
 echo "=== ordered ADC scans $chip ==="
 cargo build --offline --locked --release --bins --target thumbv6m-none-eabi --manifest-path examples/adc-scan/Cargo.toml --no-default-features --features "$chip"
done
for chip in cw32l010f8p6 cw32l010f8u6 cw32l010y8m6; do
 for extra in '' ',embassy-cw32/time-driver-gtim1'; do
  echo "=== existing timer ADC ownership $chip$extra ==="
  cargo build --offline --locked --release --bins --target thumbv6m-none-eabi --manifest-path examples/trigger-routing/Cargo.toml --no-default-features --features "$chip$extra"
 done
done
