# LSE SYSCLK and the same oscillator for RTC

Two ordinary bare-metal firmware examples for exactly CW32F020C6U7,
CW32F030C8T7 and CW32A030C8T7. Build with an exact feature, for example:

```
cargo build --release --locked --manifest-path examples/lse-sysclk/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32f020c6u7,defmt --bin crystal
cargo build --release --locked --manifest-path examples/lse-sysclk/Cargo.toml --target thumbv6m-none-eabi --no-default-features --features cw32f030c8t7,defmt --bin bypass
```

These are board declarations, not measured qualification. Both examples assert
VDD 3.0–3.6 V, ambient −20…70 °C and nominal 32768 Hz with every-cycle frequency
between 32766 and 32770 Hz over supply, load, temperature, aging and short-term
variation. Replace these with the actual qualified board data. HSI /6 and AHB/APB
/1 retain the existing defaults; HSI and factory detector LSI remain enabled.

`crystal` requires the board's 32768 Hz crystal, load capacitors and layout on
PC14/PC15, physical pins 3/4 on the exact QFN48/LQFP48 package. The actual crystal
and board must qualify Strong drive, Normal amplitude and the selected startup
count. `bypass` requires an external digital clock on PC14 (pin 3): high level
0.7×VDDIO…VDDIO, low level VSS…0.3×VDDIO, high and low pulse widths at least
450 ns and rise/fall times at most 50 ns, plus the device's full I/O requirements.
The source must retain 45–55% duty and satisfy those limits every cycle.
PC15 is not consumed by bypass; inherited reservations still apply.

Both call only `Config.lse` plus `Sysclk::LSE`, then borrow the same source with
`LseClock` for RTC. The example epoch is deliberate demonstration provisioning;
replace it with intended wall time. A compatible running RTC is preserved by
`initialize_if_unset`, but fresh LSE admission still requires its documented RTC
reset image. Arbitrary warm handover is not supported.

No fixed 1 MHz time driver is selected. Poll budgets are iterations, not elapsed
startup deadlines. After LSE loss/fallback, published LSE bounds and downstream
timing assumptions are invalid. Errors can leave partial state and require reset;
ordinary reset may retain LSE and POR may be needed. Whole-bank GPIO inspection
can advance sampling/filter/events. See the [full source and handover contract](../../docs/classic-lse-sysclk.md).

The build script obtains exact memory limits from generated metadata and supplies
`-Tlink.x` explicitly. Compilation/link/ELF inspection do not run this firmware or
qualify physical startup, detection windows, board timing, fault recovery or RF.
