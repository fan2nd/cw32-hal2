# LCD crosspoint walk

Ordinary bare-metal firmware, not a HAL test harness. Select the actual exact
package feature. It links to that part's generated FLASH/RAM memory map.

Wire a compatible 1/4-duty, 1/3-bias passive panel as described in `src/main.rs`.
The example explicitly assumes VDD ≤ 3.6 V and panel peak-drive tolerance ≥ 3.6 V.
Adjust those declarations, drive strength and contrast to the board and glass
specifications before use. Do not wire an external resistor/capacitor bias network.
Unused wired panel segments must still be owned by the constructor; extend its
pin list to include them before using a larger panel. The four listed segment
pads are a minimal four-segment example, not a board-vendor pinout claim.

Build: `cargo build --manifest-path examples/lcd/Cargo.toml --no-default-features
--features cw32l052c8t6 --target thumbv6m-none-eabi` (one shell command).
Use `cw32l083vct6` for L083 LQFP100; the same example wiring is valid there.
The polling budget counts polls, not microseconds. A timeout preserves LSI and
shared gates; the error should be handled according to the application's policy.

No hardware flashing or display verification has been performed.
