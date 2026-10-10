#!/usr/bin/env bash
# Production compilation only. Source/data/PAC audits remain ./d test and ./d check.
# Requires Python >=3.11, Rust, and thumbv6m-none-eabi. Never executes firmware.
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ -x .cargo/bin/cargo && -d .rustup/toolchains ]]; then
  export CARGO_HOME="$PWD/.cargo" RUSTUP_HOME="$PWD/.rustup"
  export PATH="$CARGO_HOME/bin:$PATH"
fi
export CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_RELEASE_DEBUG=0
mode=${1:-all}
case "$mode" in
  all|--matrix-only|--examples-only) ;;
  *) echo "Usage: $0 [--matrix-only|--examples-only]" >&2; exit 2 ;;
esac

if [[ "$mode" != --examples-only ]]; then
  mapfile -t chips < <(python3 - <<'PY'
import tomllib
with open('embassy-cw32/Cargo.toml', 'rb') as f:
    features = tomllib.load(f)['features']
for name in sorted(features):
    if name.startswith('cw32'):
        print(name)
PY
)
  [[ ${#chips[@]} -gt 0 ]] || { echo 'No HAL chip features found' >&2; exit 1; }
  echo "HAL production matrix: ${#chips[@]} chip features"
  for chip in "${chips[@]}"; do
    echo "=== $chip: Cortex-M0+ release library and runtime ==="
    cargo build --manifest-path firmware/Cargo.toml --locked -p embassy-cw32 --release --target thumbv6m-none-eabi \
      --no-default-features --features "$chip,rt"
    echo "=== $chip: Cortex-M0+ release library with runtime and defmt ==="
    cargo build --manifest-path firmware/Cargo.toml --locked -p embassy-cw32 --release --target thumbv6m-none-eabi \
      --no-default-features --features "$chip,rt,defmt"
  done
  echo '=== L083 selected HSE backend with GTIM1 time driver, runtime and defmt ==='
  cargo build --manifest-path firmware/Cargo.toml --locked -p embassy-cw32 --release --target thumbv6m-none-eabi \
    --no-default-features --features cw32l083mct6,rt,defmt,time-driver-gtim1
  echo '=== L052 selected HSE backend with GTIM1 time driver, runtime and defmt ==='
  cargo build --manifest-path firmware/Cargo.toml --locked -p embassy-cw32 --release --target thumbv6m-none-eabi \
    --no-default-features --features cw32l052c8t6,rt,defmt,time-driver-gtim1
  echo '=== L010/L011/L012 selected HSE backend with GTIM1 time driver, runtime and defmt ==='
  for chip in cw32l010f8p6 cw32l011k8t6 cw32l012c8t6; do
    cargo build --manifest-path firmware/Cargo.toml --locked -p embassy-cw32 --release --target thumbv6m-none-eabi \
      --no-default-features --features "$chip,rt,defmt,time-driver-gtim1"
  done
  echo 'HAL ARM production matrix passed.'
fi
if [[ "$mode" != --matrix-only ]]; then
  echo '=== Link qualified x030 LSE calendar firmware ==='
  for chip in cw32f030c8t7 cw32a030c8t7; do
    cargo build --offline --locked --release --bins --target thumbv6m-none-eabi \
      --manifest-path examples/lse-clock/Cargo.toml --no-default-features --features "$chip"
  done
  echo '=== Link both digital HEX inputs for five exact F002/F003 packages ==='
  for chip in cw32f002f3p7 cw32f002f3u7 cw32f003e4p7 cw32f003f4p7 cw32f003f4u7; do
    cargo build --locked --release --bins --target thumbv6m-none-eabi \
      --manifest-path examples/hex-clock/Cargo.toml --no-default-features --features "$chip"
  done
  echo '=== Link qualified crystal/bypass HSE firmware for twenty-one exact packages ==='
  for chip in cw32f020c6u7 cw32f030c8t7 cw32a030c8t7 cw32l031c8t6 cw32r031c8u6 cw32w031r8u6 cw32l010f8p6 cw32l010f8u6 cw32l010y8m6 cw32l011k8t6 cw32l011k8u6 cw32l052c8t6 cw32l052r8t6 cw32l052r8s6 cw32l083rbt6 cw32l083rcs6 cw32l083rct6 cw32l083mct6 cw32l083vct6 cw32l012c8t6 cw32l012c8u6; do
    (cd examples/hse-clock && cargo build --locked --release --bins --target thumbv6m-none-eabi \
      --no-default-features --features "$chip")
  done
  echo '=== Link standalone CW32F030C8T7 blocking and async GPIO firmware examples ==='
  (cd examples/cw32f030 && cargo build --locked --release --bins)
  echo '=== Link safe staged UART TX DMA firmware for two exact x030 and five exact L083 packages ==='
  for chip in cw32f030c8t7 cw32a030c8t7 cw32l083rbt6 cw32l083rcs6 cw32l083rct6 cw32l083mct6 cw32l083vct6; do
    cargo build --offline --locked --release --target thumbv6m-none-eabi \
      --manifest-path examples/uart-dma-tx/Cargo.toml --no-default-features --features "$chip"
  done
  echo '=== Link staged UART RX/full-duplex and L083 shared-vector DMA firmware for seven exact packages ==='
  for chip in cw32f030c8t7 cw32a030c8t7 cw32l083rbt6 cw32l083rcs6 cw32l083rct6 cw32l083mct6 cw32l083vct6; do
    cargo build --offline --locked --release --bins --target thumbv6m-none-eabi \
      --manifest-path examples/uart-dma/Cargo.toml --no-default-features --features "$chip"
  done
  echo '=== Link safe staged SPI1 and L083 SPI2 DMA firmware for seven exact packages ==='
  for chip in cw32f030c8t7 cw32a030c8t7 cw32l083rbt6 cw32l083rcs6 cw32l083rct6 cw32l083mct6 cw32l083vct6; do
    cargo build --offline --locked --release --bins --target thumbv6m-none-eabi \
      --manifest-path examples/spi-dma/Cargo.toml --no-default-features --features "$chip"
  done
  echo '=== Link ATIM counter/main-PWM firmware for 11 exact packages ==='
  for chip in cw32f030c8t7 cw32a030c8t7 cw32f003e4p7 cw32l031c8t6 cw32r031c8u6 cw32w031r8u6 cw32l052c8t6 cw32l083mct6 cw32l010f8p6 cw32l011k8t6 cw32l012c8t6; do
    echo "=== Link ATIM counter/main-PWM for $chip ==="
    cargo build --locked --release --bins --target thumbv6m-none-eabi \
      --manifest-path examples/atim/Cargo.toml --no-default-features --features "$chip"
  done
  echo '=== Link reserved-FLASH storage firmware for 37 exact packages ==='
  mapfile -t flash_chips < <(python3 - <<'PYPARTS'
import tomllib
with open('examples/flash-storage/Cargo.toml', 'rb') as f:
    print('\n'.join(sorted(c for c in tomllib.load(f)['features'] if c.startswith('cw32'))))
PYPARTS
)
  for chip in "${flash_chips[@]}"; do
    echo "=== Link reserved-FLASH storage for $chip ==="
    RUSTFLAGS="-C link-arg=-Tlink.x" cargo build --locked --release --target thumbv6m-none-eabi \
      --manifest-path examples/flash-storage/Cargo.toml --no-default-features --features "$chip"
  done
  echo '=== Link external comparator polling firmware for 13 exact packages ==='
  for chip in cw32f030c8t7 cw32a030c8t7 cw32f020c6u7 cw32f002f3p7 cw32f003e4p7 cw32l031c8t6 cw32r031c8u6 cw32w031r8u6 cw32l052c8t6 cw32l083mct6 cw32l010f8p6 cw32l011k8t6 cw32l012c8t6; do
    cargo build --locked --release --target thumbv6m-none-eabi \
      --manifest-path examples/comparator/Cargo.toml --no-default-features --features "$chip"
  done
  echo '=== Link AWT/LPTIM polling and IRQ firmware for 13 exact packages ==='
  for chip in cw32f030c8t7 cw32a030c8t7 cw32f020c6u7 cw32f002f3p7 cw32f003e4p7 cw32l031c8t6 cw32r031c8u6 cw32w031r8u6 cw32l052c8t6 cw32l083mct6 cw32l010f8p6 cw32l011k8t6 cw32l012c8t6; do
    cargo build --locked --release --bins --target thumbv6m-none-eabi \
      --manifest-path examples/low-power-timers/Cargo.toml --no-default-features --features "$chip"
  done
  echo '=== Link EAU/CORDIC firmware for both exact L012 packages ==='
  for chip in cw32l012c8t6 cw32l012c8u6; do
    cargo build --locked --release --bins --target thumbv6m-none-eabi \
      --manifest-path examples/l012-math/Cargo.toml --no-default-features --features "$chip"
  done
  echo '=== Link DAC/OPA firmware for both exact L012 packages ==='
  for chip in cw32l012c8t6 cw32l012c8u6; do
    cargo build --locked --release --bins --target thumbv6m-none-eabi \
      --manifest-path examples/l012-analog/Cargo.toml --no-default-features --features "$chip"
  done
  echo '=== Link qualified L083 HSI-fed PLL firmware ==='
  for chip in cw32l083mct6 cw32l083rbt6 cw32l083rcs6 cw32l083rct6 cw32l083vct6; do
    cargo build --offline --locked --release --target thumbv6m-none-eabi \
      --manifest-path examples/pll-clock/Cargo.toml --no-default-features --features "$chip" --bin pll-uart
  done
  for mode in fractional low-voltage fractional,low-voltage; do
    cargo build --offline --locked --release --target thumbv6m-none-eabi \
      --manifest-path examples/pll-clock/Cargo.toml --no-default-features --features "cw32l083rct6,$mode" --bin pll-uart
  done
  for mode in embassy-time embassy-time,low-voltage; do
    cargo build --offline --locked --release --target thumbv6m-none-eabi \
      --manifest-path examples/pll-clock/Cargo.toml --no-default-features --features "cw32l083rct6,$mode" --bin pll-time
  done
  echo '=== Link hardware-word AES and raw TRNG firmware for five exact L083 packages ==='
  for chip in cw32l083mct6 cw32l083rbt6 cw32l083rcs6 cw32l083rct6 cw32l083vct6; do
    cargo build --locked --release --bins --target thumbv6m-none-eabi \
      --manifest-path examples/l083-crypto/Cargo.toml --no-default-features --features "$chip"
  done
  echo '=== Link owned and borrowed ordered ADC scans for five exact packages ==='
  for chip in cw32l010f8p6 cw32l010f8u6 cw32l010y8m6 cw32l011k8t6 cw32l011k8u6; do
    cargo build --offline --locked --release --bins --target thumbv6m-none-eabi \
      --manifest-path examples/adc-scan/Cargo.toml --no-default-features --features "$chip"
  done
  echo '=== Link L010/L011 asynchronous single/ordered ADC firmware ==='
  for chip in cw32l010f8p6 cw32l010f8u6 cw32l010y8m6 cw32l011k8t6 cw32l011k8u6; do
    cargo build --offline --locked --release --bins --target thumbv6m-none-eabi \
      --manifest-path examples/adc-scan/Cargo.toml --no-default-features --features "$chip,async"
  done
  echo '=== Link L010/L011 timer-cascade and triggered-conversion firmware ==='
  for chip in cw32l010f8p6 cw32l010f8u6 cw32l010y8m6 cw32l011k8t6 cw32l011k8u6; do
    cargo build --locked --release --bins --target thumbv6m-none-eabi \
      --manifest-path examples/trigger-routing/Cargo.toml --no-default-features --features "$chip"
  done
  echo '=== Link LCD, HALLTIM and passive RAM diagnostic firmware ==='
  for group in lcd halltim ram-parity; do
    mapfile -t parts < <(python3 - "$group" <<'PYEXAMPLES'
import sys,tomllib
f=tomllib.load(open(f'examples/{sys.argv[1]}/Cargo.toml','rb'))['features']
print('\n'.join(sorted(c for c in f if c.startswith('cw32'))))
PYEXAMPLES
)
    for chip in "${parts[@]}"; do
      cargo build --locked --release --bins --target thumbv6m-none-eabi \
        --manifest-path "examples/$group/Cargo.toml" --no-default-features --features "$chip"
    done
  done
  echo '=== Link AUTOTRIM counter firmware for eight exact packages ==='
  for chip in cw32l052c8t6 cw32l052r8s6 cw32l052r8t6 cw32l083mct6 cw32l083rbt6 cw32l083rcs6 cw32l083rct6 cw32l083vct6; do
    cargo build --locked --release --target thumbv6m-none-eabi \
      --manifest-path examples/autotrim-counter/Cargo.toml --no-default-features --features "$chip"
  done
  echo '=== Link RTC calendar firmware for36 explicit package/legacy selections across eleven families ==='
  mapfile -t rtc_chips < <(python3 - <<'PYRTC'
import tomllib
print('\n'.join(sorted(c for c in tomllib.load(open('examples/rtc-calendar/Cargo.toml','rb'))['features'] if c.startswith('cw32'))))
PYRTC
)
  for chip in "${rtc_chips[@]}"; do
    cargo build --locked --release --bins --target thumbv6m-none-eabi \
      --manifest-path examples/rtc-calendar/Cargo.toml --no-default-features --features "$chip"
  done
  echo '=== Link qualified timer-input and UART RTS/CTS firmware ==='
  for group in timer-input uart-flow-control; do
    mapfile -t chips < <(python3 - "$group" <<'PYPARTS'
import sys,tomllib
print('\n'.join(sorted(c for c in tomllib.load(open(f'examples/{sys.argv[1]}/Cargo.toml','rb'))['features'] if c.startswith('cw32'))))
PYPARTS
)
    for chip in "${chips[@]}"; do
      cargo build --offline --locked --release --bins --target thumbv6m-none-eabi --manifest-path "examples/$group/Cargo.toml" --no-default-features --features "$chip"
    done
  done
  echo '=== Link all declared RTC alarm firmware selections ==='
  mapfile -t rtc_alarm_chips < <(python3 - <<'PYALARMS'
import tomllib
print('\n'.join(sorted(c for c in tomllib.load(open('examples/rtc-alarms/Cargo.toml','rb'))['features'] if c.startswith('cw32'))))
PYALARMS
)
  for chip in "${rtc_alarm_chips[@]}"; do
    cargo build --offline --locked --release --bins --target thumbv6m-none-eabi --manifest-path examples/rtc-alarms/Cargo.toml --no-default-features --features "$chip"
  done
  echo 'HAL firmware example builds passed. No firmware was flashed or run.'
  for group in classic-adc-scan l012-adc-scan crc-checksum classic-atim-complementary; do
    mapfile -t selection < <(python3 - "$group" <<'PYPARTS'
import sys,tomllib
print('\n'.join(sorted(c for c in tomllib.load(open(f'examples/{sys.argv[1]}/Cargo.toml','rb'))['features'] if c.startswith('cw32'))))
PYPARTS
)
    for chip in "${selection[@]}"; do
      cargo build --locked --release --bins --target thumbv6m-none-eabi --manifest-path "examples/$group/Cargo.toml" --no-default-features --features "$chip"
    done
  done

fi
