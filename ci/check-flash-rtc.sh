#!/usr/bin/env bash
# Normal production compilation and linked firmware; no firmware execution.
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_HOME="$PWD/.cargo" RUSTUP_HOME="$PWD/.rustup"
export PATH="$CARGO_HOME/bin:$PATH"
export CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_RELEASE_DEBUG=0
./ci/check-hal.sh --matrix-only
for group in flash-storage rtc-calendar; do
  mapfile -t chips < <(python3 - "$group" <<'PYCHIPS'
import sys,tomllib
print('\n'.join(sorted(c for c in tomllib.load(open(f'examples/{sys.argv[1]}/Cargo.toml','rb'))['features'] if c.startswith('cw32'))))
PYCHIPS
)
  for chip in "${chips[@]}"; do
    echo "=== $group real firmware: $chip ==="
    if [[ "$group" == flash-storage ]]; then export RUSTFLAGS="-C link-arg=-Tlink.x"; else unset RUSTFLAGS; fi
    cargo build --offline --locked --release --bins --target thumbv6m-none-eabi --manifest-path "examples/$group/Cargo.toml" --no-default-features --features "$chip"
  done
done
