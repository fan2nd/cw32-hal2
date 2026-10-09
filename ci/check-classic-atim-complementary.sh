#!/usr/bin/env bash
# Ordinary production ARM libraries and application firmware; no HAL tests.
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ -x .cargo/bin/cargo && -d .rustup/toolchains ]]; then
  export CARGO_HOME="$PWD/.cargo" RUSTUP_HOME="$PWD/.rustup"
  export PATH="$CARGO_HOME/bin:$PATH"
fi
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_RELEASE_DEBUG=0
# Set this outside the source tree when retaining per-selection ELF evidence.
artifact_dir="${CW32_CLASSIC_PWM_ARTIFACT_DIR:-$PWD/build/classic-atim-complementary}"
mkdir -p "$artifact_dir"
target_dir="${CARGO_TARGET_DIR:-$PWD/target}"
for chip in cw32f030 cw32f030c8t7 cw32f030k8t7 cw32f030k8u7 cw32f030f6p7 cw32f030f8v7 cw32a030 cw32a030c8t7 cw32l010f8p6 cw32l011k8t6 cw32l012c8t6; do
  for options in rt rt,defmt; do
    echo "ARM library $chip,$options"
    cargo build --offline --locked --release --target thumbv6m-none-eabi --manifest-path firmware/Cargo.toml -p embassy-cw32 --no-default-features --features "$chip,$options"
  done
done
for chip in cw32f030c8t7 cw32f030k8t7 cw32f030k8u7 cw32f030f6p7 cw32f030f8v7 cw32a030c8t7; do
  echo "Classic application $chip"
  cargo build --offline --locked --release --target thumbv6m-none-eabi --manifest-path examples/classic-atim-complementary/Cargo.toml --no-default-features --features "$chip"
  cp "$target_dir/thumbv6m-none-eabi/release/cw32-classic-atim-complementary-examples" "$artifact_dir/$chip.elf"
done
for chip in cw32l010f8p6 cw32l011k8t6 cw32l012c8t6; do
  echo "Buffered regression application $chip"
  cargo build --offline --locked --release --target thumbv6m-none-eabi --manifest-path examples/atim-complementary/Cargo.toml --no-default-features --features "$chip"
  cp "$target_dir/thumbv6m-none-eabi/release/cw32-atim-complementary-examples" "$artifact_dir/$chip.elf"
done
echo 'Classic and buffered scoped ARM builds passed; firmware was linked, not flashed or run.'
