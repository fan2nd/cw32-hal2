#!/usr/bin/env bash
# Real ARM library compilation and linked external-input firmware, never HAL tests.
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_RELEASE_DEBUG=0
mapfile -t chips < <(python3 - <<'PYCHIPS'
import tomllib
f=tomllib.load(open('embassy-cw32/Cargo.toml','rb'))['features']
print('\n'.join(sorted(c for c in f if c.startswith(('cw32l010','cw32l011','cw32l012')))))
PYCHIPS
)
for chip in "${chips[@]}"; do
  for options in "$chip,rt" "$chip,rt,defmt"; do
    cargo build --manifest-path firmware/Cargo.toml --offline --locked --release -p embassy-cw32 --target thumbv6m-none-eabi --no-default-features --features "$options"
  done
done
# One exact part from each pre-existing classic timer family catches cfg leakage.
for chip in cw32f030c8t7 cw32a030c8t7 cw32f020c6u7 cw32f002f3p7 cw32f003e4p7 cw32l031c8t6 cw32r031c8u6 cw32w031r8u6 cw32l052c8t6 cw32l083mct6; do
  cargo build --manifest-path firmware/Cargo.toml --offline --locked --release -p embassy-cw32 --target thumbv6m-none-eabi --no-default-features --features "$chip,rt,defmt"
done
mapfile -t parts < <(python3 - <<'PYPARTS'
import tomllib
f=tomllib.load(open('examples/timer-input/Cargo.toml','rb'))['features']
print('\n'.join(sorted(c for c in f if c.startswith('cw32'))))
PYPARTS
)
for chip in "${parts[@]}"; do
  cargo build --offline --locked --release --bins --target thumbv6m-none-eabi --manifest-path examples/timer-input/Cargo.toml --no-default-features --features "$chip"
done
