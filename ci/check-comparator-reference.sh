#!/usr/bin/env bash
# Normal production builds and real firmware links; no HAL tests or execution.
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_RELEASE_DEBUG=0
if [[ -x .cargo/bin/cargo && -d .rustup/toolchains ]]; then
  export CARGO_HOME="$PWD/.cargo" RUSTUP_HOME="$PWD/.rustup"
  export PATH="$CARGO_HOME/bin:$PATH"
fi
artifacts="${CW32_REFERENCE_ARTIFACTS:-$PWD/build/comparator-reference}"
mkdir -p "$artifacts"
for chip in cw32l010 cw32l011 cw32l012 cw32f030; do
  for modes in rt rt,defmt; do
    cargo build --offline --locked --release --target thumbv6m-none-eabi \
      --manifest-path firmware/Cargo.toml -p embassy-cw32 \
      --no-default-features --features "$chip,$modes" \
      > "$artifacts/hal-$chip-${modes//,/-}.log" 2>&1
  done
done
for group in comparator-reference comparator; do
  parts=(cw32l010f8p6 cw32l011k8t6 cw32l012c8t6)
  if [[ "$group" == comparator ]]; then parts+=(cw32f030c8t7); fi
  for chip in "${parts[@]}"; do
    cargo build --offline --locked --release --target thumbv6m-none-eabi \
      --manifest-path "examples/$group/Cargo.toml" --no-default-features \
      --features "$chip" > "$artifacts/$group-$chip.log" 2>&1
    target="${CARGO_TARGET_DIR:-$PWD/examples/$group/target}"
    cp "$target/thumbv6m-none-eabi/release/cw32-$group-example" \
      "$artifacts/$group-$chip.elf"
    readelf -h "$artifacts/$group-$chip.elf" > "$artifacts/$group-$chip.elf-header.txt"
  done
done
printf '%s\n' 'Passed 8 representative ARM library builds and 7 real firmware links; no hardware execution.'
