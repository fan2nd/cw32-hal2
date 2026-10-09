#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_HOME="$PWD/.cargo" RUSTUP_HOME="$PWD/.rustup" PATH="$PWD/.cargo/bin:$PATH"
export CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_RELEASE_DEBUG=0
for chip in cw32a030 cw32f002 cw32f003 cw32f020 cw32f030 cw32l010 cw32l011 cw32l012 cw32l031 cw32l052 cw32l083 cw32r031 cw32w031; do
  for modes in rt rt,defmt; do
    echo "=== Family $chip $modes ==="
    cargo build --manifest-path firmware/Cargo.toml --offline --locked --release --target thumbv6m-none-eabi -p embassy-cw32 --no-default-features --features "$chip,$modes"
  done
done
for group in lcd halltim; do
  mapfile -t parts < <(python3 - "$group" <<'PY'
import sys,tomllib
f=tomllib.load(open(f'examples/{sys.argv[1]}/Cargo.toml','rb'))['features']
print('\n'.join(sorted(x for x in f if x.startswith('cw32'))))
PY
)
  for chip in "${parts[@]}"; do
    echo "=== Exact $group $chip rt,defmt ==="
    cargo build --manifest-path firmware/Cargo.toml --offline --locked --release --target thumbv6m-none-eabi -p embassy-cw32 --no-default-features --features "$chip,rt,defmt"
  done
done
for group in lcd halltim ram-parity; do
  mapfile -t parts < <(python3 - "$group" <<'PY'
import sys,tomllib
f=tomllib.load(open(f'examples/{sys.argv[1]}/Cargo.toml','rb'))['features']
print('\n'.join(sorted(x for x in f if x.startswith('cw32'))))
PY
)
  for chip in "${parts[@]}"; do
    echo "=== Link $group $chip ==="
    cargo build --offline --locked --release --bins --target thumbv6m-none-eabi --manifest-path "examples/$group/Cargo.toml" --no-default-features --features "$chip"
  done
done
echo 'Stage16 family/affected builds and genuine firmware links passed; no execution.'
