# Owned HCLK and PCLK output on CW32F030C8T7

These are two ordinary firmware examples for exactly CW32F030C8T7 (LQFP48),
using PA8 AF4 at package pin 29. `hclk` selects HCLK and `pclk` selects PCLK.
Both select MCO /8 and hold the real SYSCTRL and PA8 owners for the entire loop.
See the [complete ownership and electrical contract](../../docs/f030-owned-mco.md).

The example board declaration is 3.0–3.6 V, ambient −20–70°C and total PA8 load
at most 50 pF, including traces, receiver and measurement equipment. The same
operating declaration is used during HAL initialization and MCO construction.
These are example assertions that must be qualified for the actual board before
use; they are not measurements, a certified BOM or universal board settings.

Both examples explicitly select factory HSI /6, AHB /1 and APB /2. Their nominal
bus frequencies are 8 MHz HCLK and 4 MHz PCLK. With MCO /8 the nominal outputs
are 1 MHz and 500 kHz respectively; `clock_bounds()` retains the exact divided
source-qualified logical bounds, including HSI error. No frequency is measured.
The API supports /1, /2, /8, /64, /128, /256, /512 and /1024; these examples
deliberately use just the conservative /8 choice.

The HAL's conservative 20 MHz admission policy does not establish loaded
45–55% duty, rise/fall time, receiver suitability or pad-level pulse timing.
The vendor fmax table is design/simulation data, explicitly unmeasured, and is
conditional on duty and edge timing. Qualify the actual waveform, voltage/current,
load, routing and thermal limits separately. Observation with suitable equipment
does not itself make the output a qualified receiver clock.

The input state must have MCO disabled and GPIOA out of reset. Construction
disconnects PA8, clears its lock and interrupt enables, and opens the whole
GPIOA gate. Other pins' sampling, filters and armed events may advance, including
before a constructor error; the gate remains enabled. Existing PA8/downstream
functions must tolerate disconnection, clock activity and stopping. Drop only
attempts a bounded shutdown; forgetting the owner may leave output active.
No oscillator or system/bus divider is changed by MCO construction or Drop.

Because the owner holds SYSCTRL, a live HAL `CalendarClock`, `LsiClock`,
`LseClock`, or an `Rtc` retaining such a clock cannot be held simultaneously.
The existing RTC preservation example is a separate executable, not a combined
MCO/RTC example. A forgotten reborrow can outlive its Rust owner in hardware;
safe token exclusion is not permanent hardware exclusion.

After ordinary data/PAC generation, build from the repository root:

```sh
cargo build --release --locked --manifest-path examples/mco/Cargo.toml \
  --target thumbv6m-none-eabi --no-default-features --features cw32f030c8t7 --bin hclk
cargo build --release --locked --manifest-path examples/mco/Cargo.toml \
  --target thumbv6m-none-eabi --no-default-features --features cw32f030c8t7 --bin pclk
```

This crate has its own workspace, matching the other examples. Its build script
checks the exact generated 64 KiB Flash / 8 KiB SRAM map, writes `memory.x`, and
passes `-Tlink.x` explicitly. The lockfile retains the existing dependency versions
and source pins. Build/link success supplies no physical waveform validation.
