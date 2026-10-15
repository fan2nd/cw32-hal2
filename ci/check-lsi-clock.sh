#!/usr/bin/env bash
# Local ordinary-library/firmware compilation, not HAL tests or silicon execution.
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_INCREMENTAL=0
for chip in cw32f002f3p7 cw32f002f3u7; do
  cargo build --locked --manifest-path firmware/Cargo.toml -p embassy-cw32 \
    --target thumbv6m-none-eabi --no-default-features --features "$chip,defmt,time-driver-gtim"
  cargo build --release --locked --manifest-path firmware/Cargo.toml -p embassy-cw32 \
    --target thumbv6m-none-eabi --no-default-features --features "$chip,defmt,time-driver-gtim"
  cargo build --release --locked --manifest-path examples/lsi-clock/Cargo.toml \
    --target thumbv6m-none-eabi --no-default-features --features "$chip,defmt"
done
for chip in cw32f003f4p7 cw32f003f4u7 cw32f003e4p7; do
  cargo build --release --locked --manifest-path firmware/Cargo.toml -p embassy-cw32 \
    --target thumbv6m-none-eabi --no-default-features --features "$chip,defmt,time-driver-gtim"
  cargo build --release --locked --manifest-path examples/lsi-clock/Cargo.toml \
    --target thumbv6m-none-eabi --no-default-features --features "$chip,defmt"
done
for chip in cw32f002 cw32f003; do
  cargo check --locked --manifest-path firmware/Cargo.toml -p embassy-cw32 \
    --target thumbv6m-none-eabi --no-default-features --features "$chip,defmt"
done
for chip in cw32f020c6u7 cw32f030c8t7 cw32a030c8t7; do
  cargo check --locked --manifest-path firmware/Cargo.toml -p embassy-cw32 \
    --target thumbv6m-none-eabi --no-default-features --features "$chip,defmt,time-driver-gtim1"
  cargo build --release --locked --manifest-path examples/lsi-clock/Cargo.toml \
    --target thumbv6m-none-eabi --no-default-features --features "$chip,defmt"
done
cargo build --release --locked --manifest-path examples/rtc-calendar/Cargo.toml \
  --target thumbv6m-none-eabi --no-default-features --features cw32f030c8t7 --bin preserve_calendar
cargo build --release --locked --manifest-path examples/hse-clock/Cargo.toml \
  --target thumbv6m-none-eabi --no-default-features --features cw32f020c6u7 --bin crystal
cargo build --release --locked --manifest-path examples/pll-clock/Cargo.toml \
  --target thumbv6m-none-eabi --no-default-features --features cw32a030c8t7 --bin pll-uart
for chip in cw32f002f3p7 cw32f003f4p7; do
  cargo build --release --locked --manifest-path examples/hex-clock/Cargo.toml \
    --target thumbv6m-none-eabi --no-default-features --features "$chip" --bins
done

# L031 exact3 and shared-backend regressions. This is a future local recipe,
# not an implementation acceptance receipt or an instruction to run it in full.
for chip in cw32l031c8t6 cw32l031c8u6 cw32l031f8u6; do
  cargo build --release --locked --manifest-path firmware/Cargo.toml -p embassy-cw32 \
    --target thumbv6m-none-eabi --no-default-features --features "$chip,defmt,time-driver-gtim1"
  cargo build --release --locked --manifest-path examples/lsi-clock/Cargo.toml \
    --target thumbv6m-none-eabi --no-default-features --features "$chip,defmt"
done
cargo build --locked --manifest-path firmware/Cargo.toml -p embassy-cw32 \
  --target thumbv6m-none-eabi --no-default-features --features cw32l031c8t6,defmt,time-driver-gtim1
for chip in cw32l031 cw32l031f8p6; do
  cargo build --release --locked --manifest-path firmware/Cargo.toml -p embassy-cw32 \
    --target thumbv6m-none-eabi --no-default-features --features "$chip,defmt"
done
for chip in cw32r031c8u6; do
  cargo build --release --locked --manifest-path firmware/Cargo.toml -p embassy-cw32 \
    --target thumbv6m-none-eabi --no-default-features --features "$chip,defmt,time-driver-gtim1"
done
for chip in cw32l031c8t6 cw32r031c8u6; do
  cargo build --release --locked --manifest-path examples/lse-sysclk/Cargo.toml \
    --target thumbv6m-none-eabi --no-default-features --features "$chip" --bin crystal
done
cargo build --release --locked --manifest-path examples/lse-sysclk/Cargo.toml \
  --target thumbv6m-none-eabi --no-default-features --features cw32l031f8u6 --bin bypass
cargo build --release --locked --manifest-path examples/rtc-calendar/Cargo.toml \
  --target thumbv6m-none-eabi --no-default-features --features cw32l031c8t6 --bin preserve_calendar
cargo build --release --locked --manifest-path examples/lse-clock/Cargo.toml \
  --target thumbv6m-none-eabi --no-default-features --features cw32l031c8t6 --bin crystal_calendar
cargo build --release --locked --manifest-path examples/hse-clock/Cargo.toml \
  --target thumbv6m-none-eabi --no-default-features --features cw32l031c8t6 --bin crystal

# R031 exact1 additions to the future recipe. The R031 release library and
# selected-LSE crystal ELF already appear above. These rows are not receipts.
cargo build --locked --manifest-path firmware/Cargo.toml -p embassy-cw32 \
  --target thumbv6m-none-eabi --no-default-features --features cw32r031c8u6,defmt,time-driver-gtim1
cargo build --release --locked --manifest-path firmware/Cargo.toml -p embassy-cw32 \
  --target thumbv6m-none-eabi --no-default-features --features cw32r031,defmt
cargo build --release --locked --manifest-path examples/lsi-clock/Cargo.toml \
  --target thumbv6m-none-eabi --no-default-features --features cw32r031c8u6,defmt --bin cw32-lsi-clock-example
cargo build --release --locked --manifest-path examples/rtc-calendar/Cargo.toml \
  --target thumbv6m-none-eabi --no-default-features --features cw32r031c8u6 --bin preserve_calendar
cargo build --release --locked --manifest-path examples/lse-clock/Cargo.toml \
  --target thumbv6m-none-eabi --no-default-features --features cw32r031c8u6 --bin crystal_calendar
cargo build --release --locked --manifest-path examples/hse-clock/Cargo.toml \
  --target thumbv6m-none-eabi --no-default-features --features cw32r031c8u6 --bin crystal


# Exact L052 additions to the future recipe, not finite acceptance receipts.
# The historical recipe above has 24 library commands and 32 linked ELFs.
# These six library builds and seven ELFs bring the future totals to 30 and 39.
# Run the separate focused matrix for this addition; do not run this aggregate
# script merely to produce those receipts.
for chip in cw32l052c8t6 cw32l052r8s6 cw32l052r8t6; do
  cargo build --release --locked --manifest-path firmware/Cargo.toml -p embassy-cw32 --lib \
    --target thumbv6m-none-eabi --no-default-features --features "$chip,defmt,time-driver-gtim1"
  cargo build --release --locked --manifest-path examples/lsi-clock/Cargo.toml \
    --target thumbv6m-none-eabi --no-default-features --features "$chip,defmt" --bin cw32-lsi-clock-example
done
cargo build --locked --manifest-path firmware/Cargo.toml -p embassy-cw32 --lib \
  --target thumbv6m-none-eabi --no-default-features --features cw32l052c8t6,defmt,time-driver-gtim1
cargo build --release --locked --manifest-path firmware/Cargo.toml -p embassy-cw32 --lib \
  --target thumbv6m-none-eabi --no-default-features --features cw32l052,defmt
cargo build --release --locked --manifest-path firmware/Cargo.toml -p embassy-cw32 --lib \
  --target thumbv6m-none-eabi --no-default-features --features cw32l083rct6,defmt,time-driver-gtim1
cargo build --release --locked --manifest-path examples/hse-clock/Cargo.toml \
  --target thumbv6m-none-eabi --no-default-features --features cw32l052c8t6 --bin crystal
cargo build --release --locked --manifest-path examples/lse-sysclk/Cargo.toml \
  --target thumbv6m-none-eabi --no-default-features --features cw32l052r8s6 --bin crystal
cargo build --release --locked --manifest-path examples/rtc-calendar/Cargo.toml \
  --target thumbv6m-none-eabi --no-default-features --features cw32l052r8t6 --bin preserve_calendar
cargo build --release --locked --manifest-path examples/pll-clock/Cargo.toml \
  --target thumbv6m-none-eabi --no-default-features --features cw32l083rct6 --bin pll-uart


# Exact L083 additions to the future recipe, not finite acceptance receipts.
# Reuse the existing RCT6 release library and HSI-fed PLL ELF above.
# Six additional library builds and eight ELFs bring the future totals to
# 36 library commands (31 builds, five historical checks) and 47 linked ELFs.
# The finite main 10-library/10-ELF and clean 2-library/3-ELF matrix is separate;
# it includes actual L010/F030 release builds not supplied by these additions.
for chip in cw32l083rbt6 cw32l083rcs6 cw32l083mct6 cw32l083vct6; do
  cargo build --release --locked --manifest-path firmware/Cargo.toml -p embassy-cw32 --lib \
    --target thumbv6m-none-eabi --no-default-features --features "$chip,defmt,time-driver-gtim1"
done
cargo build --locked --manifest-path firmware/Cargo.toml -p embassy-cw32 --lib \
  --target thumbv6m-none-eabi --no-default-features --features cw32l083rct6,defmt,time-driver-gtim1
cargo build --release --locked --manifest-path firmware/Cargo.toml -p embassy-cw32 --lib \
  --target thumbv6m-none-eabi --no-default-features --features cw32l083,defmt
for chip in cw32l083rbt6 cw32l083rct6 cw32l083rcs6 cw32l083mct6 cw32l083vct6; do
  cargo build --release --locked --manifest-path examples/lsi-clock/Cargo.toml \
    --target thumbv6m-none-eabi --no-default-features --features "$chip,defmt" --bin cw32-lsi-clock-example
done
cargo build --release --locked --manifest-path examples/pll-clock/Cargo.toml \
  --target thumbv6m-none-eabi --no-default-features --features cw32l083rcs6,hse-crystal --bin pll-uart
cargo build --release --locked --manifest-path examples/lse-sysclk/Cargo.toml \
  --target thumbv6m-none-eabi --no-default-features --features cw32l083mct6,defmt --bin crystal
cargo build --release --locked --manifest-path examples/rtc-calendar/Cargo.toml \
  --target thumbv6m-none-eabi --no-default-features --features cw32l083vct6 --bin preserve_calendar

# Exact L010 live-only additions to the future recipe; not executed receipts.
# Eight library builds and six new ELF rows bring aggregate totals to
# 44 library commands (39 builds, five historical checks) and 53 ELF links.
# Reuse the existing F030 preserve_calendar ELF. Finite acceptance is separate:
# 8 libraries/7 ELFs in main and exactly 2 libraries/2 ELFs in clean.
for chip in cw32l010f8p6 cw32l010f8u6 cw32l010y8m6 cw32l010 cw32l011k8t6 cw32l012c8t6 cw32f030c8t7; do
  cargo build --release --locked --manifest-path firmware/Cargo.toml -p embassy-cw32 --lib \
    --target thumbv6m-none-eabi --no-default-features --features "$chip,rt,defmt"
done
cargo build --locked --manifest-path firmware/Cargo.toml -p embassy-cw32 --lib \
  --target thumbv6m-none-eabi --no-default-features --features cw32l010f8p6,rt,defmt,time-driver-gtim1
for chip in cw32l010f8p6 cw32l010f8u6 cw32l010y8m6; do
  cargo build --release --locked --manifest-path examples/lsi-clock/Cargo.toml \
    --target thumbv6m-none-eabi --no-default-features --features "$chip,defmt" --bin cw32-lsi-clock-example
done
cargo build --release --locked --manifest-path examples/lse-sysclk/Cargo.toml \
  --target thumbv6m-none-eabi --no-default-features --features cw32l010f8p6 --bin crystal
cargo build --release --locked --manifest-path examples/rtc-calendar/Cargo.toml \
  --target thumbv6m-none-eabi --no-default-features --features cw32l010f8p6 --bin preserve_calendar
cargo build --release --locked --manifest-path examples/hse-clock/Cargo.toml \
  --target thumbv6m-none-eabi --no-default-features --features cw32l011k8t6 --bin crystal

# Exact L012 live-only additions to the future aggregate recipe, not receipts.
# Existing release library rows already cover C8T6, L010F8P6, L011K8T6 and F030C8T7.
# Add three libraries and five ELFs: aggregate 47 library commands (42 builds,
# five historical checks) and 58 ELF links. The retained L010 LSI recipe uses
# defmt; this does not replace its separate finite Stage75 row without defmt.
# Actual planned main acceptance is exactly 7 libraries/6 ELFs; independent
# final-source clean is 2 libraries/2 LSI ELFs. Do not run this aggregate recipe
# to substitute for either finite matrix. All Stage75 receipts remain pending.
for chip in cw32l012c8u6 cw32l012; do
  cargo build --release --locked --manifest-path firmware/Cargo.toml -p embassy-cw32 --lib \
    --target thumbv6m-none-eabi --no-default-features --features "$chip,rt,defmt"
done
cargo build --locked --manifest-path firmware/Cargo.toml -p embassy-cw32 --lib \
  --target thumbv6m-none-eabi --no-default-features --features cw32l012c8t6,rt,defmt,time-driver-gtim1
for chip in cw32l012c8t6 cw32l012c8u6; do
  cargo build --release --locked --manifest-path examples/lsi-clock/Cargo.toml \
    --target thumbv6m-none-eabi --no-default-features --features "$chip" --bin cw32-lsi-clock-example
done
cargo build --release --locked --manifest-path examples/lse-sysclk/Cargo.toml \
  --target thumbv6m-none-eabi --no-default-features --features cw32l012c8t6 --bin crystal
cargo build --release --locked --manifest-path examples/rtc-calendar/Cargo.toml \
  --target thumbv6m-none-eabi --no-default-features --features cw32l012c8t6 --bin preserve_calendar
cargo build --release --locked --manifest-path examples/hse-clock/Cargo.toml \
  --target thumbv6m-none-eabi --no-default-features --features cw32l012c8u6 --bin crystal
