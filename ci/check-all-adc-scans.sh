#!/usr/bin/env bash
# Ordinary ARM libraries and real scan/UART/CRC firmware; no HAL tests.
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_HOME="$PWD/.cargo" RUSTUP_HOME="$PWD/.rustup"
export PATH="$CARGO_HOME/bin:$PATH"
export CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_RELEASE_DEBUG=0
./ci/check-hal.sh --matrix-only
for group in classic-adc-scan adc-scan l012-adc-scan crc-checksum; do
 mapfile -t chips < <(python3 - "$group" <<'PY'
import sys,tomllib
print('\n'.join(sorted(c for c in tomllib.load(open(f'examples/{sys.argv[1]}/Cargo.toml','rb'))['features'] if c.startswith('cw32'))))
PY
)
 for chip in "${chips[@]}"; do
  echo "=== real $group firmware $chip ==="
  cargo build --offline --locked --release --bins --target thumbv6m-none-eabi --manifest-path "examples/$group/Cargo.toml" --no-default-features --features "$chip"
 done
done
mapfile -t chips < <(python3 - <<'PY'
import tomllib
print('\n'.join(sorted(c for c in tomllib.load(open('examples/uart-flow-control/Cargo.toml','rb'))['features'] if c.startswith('cw32'))))
PY
)
for chip in "${chips[@]}"; do
 echo "=== UART blocking and async echo $chip ==="
 cargo build --offline --locked --release --bin blocking_echo --bin async_echo --target thumbv6m-none-eabi --manifest-path examples/uart-flow-control/Cargo.toml --no-default-features --features "$chip"
done
for chip in cw32l010f8p6 cw32l010f8u6 cw32l010y8m6; do
 echo "=== timer ADC ownership and Embassy time $chip ==="
 cargo build --offline --locked --release --bins --target thumbv6m-none-eabi --manifest-path examples/trigger-routing/Cargo.toml --no-default-features --features "$chip,embassy-cw32/time-driver-gtim1"
done
