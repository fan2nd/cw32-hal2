#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_HOME="$PWD/.cargo" RUSTUP_HOME="$PWD/.rustup" PATH="$PWD/.cargo/bin:$PATH"
export CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_RELEASE_DEBUG=0
mapfile -t affected < <(python3 - <<'PY'
import tomllib
f=tomllib.load(open('embassy-cw32/Cargo.toml','rb'))['features']
print('\n'.join(sorted(x for x in f if x.startswith(('cw32f030','cw32a030','cw32l052','cw32l083')))))
PY
)
for chip in "${affected[@]}"; do
 for modes in rt rt,defmt; do
  echo "=== Affected $chip $modes ==="
  cargo build --manifest-path firmware/Cargo.toml --offline --locked --release --target thumbv6m-none-eabi -p embassy-cw32 --no-default-features --features "$chip,$modes"
 done
done
for chip in cw32f002 cw32f003 cw32f020 cw32l010 cw32l011 cw32l012 cw32l031 cw32r031 cw32w031; do
 echo "=== Other $chip rt,defmt ==="
 cargo check --manifest-path firmware/Cargo.toml --offline --locked --release --target thumbv6m-none-eabi -p embassy-cw32 --no-default-features --features "$chip,rt,defmt"
done
(cd examples/cw32f030 && cargo build --offline --locked --release --bin dma_owned_copy)
mapfile -t parts < <(python3 - <<'PY'
import tomllib
print('\n'.join(sorted(x for x in tomllib.load(open('examples/autotrim-counter/Cargo.toml','rb'))['features'] if x.startswith('cw32'))))
PY
)
for chip in "${parts[@]}"; do
 echo "=== Link AUTOTRIM $chip ==="
 cargo build --offline --locked --release --target thumbv6m-none-eabi --manifest-path examples/autotrim-counter/Cargo.toml --no-default-features --features "$chip"
done
echo 'Stage17 affected builds and nine firmware links passed; no execution.'
