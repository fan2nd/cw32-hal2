#!/usr/bin/env bash
# Ordinary ARM builds and real RTC alarm firmware; never execute hardware.
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_HOME="$PWD/.cargo" RUSTUP_HOME="$PWD/.rustup"
export PATH="$CARGO_HOME/bin:$PATH"
export CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_RELEASE_DEBUG=0
./ci/check-hal.sh --matrix-only
mapfile -t chips < <(python3 - <<'PY'
import tomllib
print('\n'.join(sorted(c for c in tomllib.load(open('examples/rtc-alarms/Cargo.toml','rb'))['features'] if c.startswith('cw32'))))
PY
)
for chip in "${chips[@]}"; do
 echo "=== real RTC alarm firmware $chip ==="
 cargo build --offline --locked --release --bins --target thumbv6m-none-eabi --manifest-path examples/rtc-alarms/Cargo.toml --no-default-features --features "$chip"
done
for chip in cw32l010f8p6 cw32l011k8t6 cw32l012c8t6; do
 echo "=== real RTC alarm plus Embassy time $chip ==="
 cargo build --offline --locked --release --bins --target thumbv6m-none-eabi --manifest-path examples/rtc-alarms/Cargo.toml --no-default-features --features "$chip,embassy-cw32/time-driver-gtim1"
done
