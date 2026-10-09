#!/usr/bin/env bash
# Ordinary ARM compilation and external-pin firmware; no HAL tests or execution.
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_HOME="$PWD/.cargo" RUSTUP_HOME="$PWD/.rustup"
export PATH="$CARGO_HOME/bin:$PATH"
export CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_RELEASE_DEBUG=0
./ci/check-hal.sh --matrix-only
for chip in cw32a030c8t7 cw32f002f3p7 cw32f003e4p7 cw32f020c6u7 cw32f030c8t7 cw32l031c8t6 cw32l052c8t6 cw32l083mct6 cw32r031c8u6 cw32w031r8u6 cw32l010f8p6 cw32l011k8t6 cw32l012c8t6; do
 driver=time-driver-gtim1
 case "$chip" in cw32f002*|cw32f003*) driver=time-driver-gtim;; esac
 echo "=== reserved time resource $chip ==="
 cargo build --manifest-path firmware/Cargo.toml --offline --locked --release -p embassy-cw32 --target thumbv6m-none-eabi --no-default-features --features "$chip,rt,defmt,$driver"
done
for group in classic-timer-input timer-input atim-complementary classic-atim-complementary; do
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
for chip in cw32a030c8t7 cw32f020c6u7 cw32f030c8t7 cw32l031c8t6 cw32l052c8t6 cw32l083mct6 cw32r031c8u6 cw32w031r8u6; do
 echo "=== classic input plus time $chip ==="
 cargo build --offline --locked --release --bins --target thumbv6m-none-eabi --manifest-path examples/classic-timer-input/Cargo.toml --no-default-features --features "$chip,time-driver"
done
for chip in cw32l010f8p6 cw32l011k8t6 cw32l012c8t6; do
 echo "=== complementary PWM plus time $chip ==="
 cargo build --offline --locked --release --bins --target thumbv6m-none-eabi --manifest-path examples/atim-complementary/Cargo.toml --no-default-features --features "$chip,embassy-cw32/time-driver-gtim1"
 echo "=== buffered input plus time $chip ==="
 cargo build --offline --locked --release --bins --target thumbv6m-none-eabi --manifest-path examples/timer-input/Cargo.toml --no-default-features --features "$chip,time-driver"
done
echo '=== SOP16 ATIM capture plus time ==='
cargo build --offline --locked --release --bin polling_capture --target thumbv6m-none-eabi --manifest-path examples/timer-input/Cargo.toml --no-default-features --features cw32l010y8m6,time-driver
