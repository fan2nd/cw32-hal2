#!/usr/bin/env bash
# Production libraries and genuine external-input/serial firmware only.
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_HOME="$PWD/.cargo" RUSTUP_HOME="$PWD/.rustup"
export PATH="$CARGO_HOME/bin:$PATH"
export CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_RELEASE_DEBUG=0
./ci/check-hal.sh --matrix-only
mapfile -t lowchips < <(python3 - <<'PY'
import tomllib
print('\n'.join(sorted(c for c in tomllib.load(open('embassy-cw32/Cargo.toml','rb'))['features'] if c.startswith(('cw32l010','cw32l011','cw32l012')))))
PY
)
for chip in "${lowchips[@]}"; do
 for extra in '' ',defmt'; do
  echo "=== timer inputs with reserved time resource $chip$extra ==="
  cargo build --manifest-path firmware/Cargo.toml --offline --locked --release -p embassy-cw32 --target thumbv6m-none-eabi --no-default-features --features "$chip,time-driver-gtim1$extra"
 done
done
for group in timer-input uart-flow-control; do
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
for chip in cw32l010f8p6 cw32l011k8t6 cw32l012c8t6; do
 echo "=== real ATIM capture/QEI plus Embassy time $chip ==="
 cargo build --offline --locked --release --bins --target thumbv6m-none-eabi --manifest-path examples/timer-input/Cargo.toml --no-default-features --features "$chip,time-driver"
done
echo '=== real SOP16 ATIM channel3 capture plus Embassy time ==='
cargo build --offline --locked --release --bin polling_capture --target thumbv6m-none-eabi --manifest-path examples/timer-input/Cargo.toml --no-default-features --features cw32l010y8m6,time-driver
