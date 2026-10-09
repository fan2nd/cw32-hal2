# L012 analog output firmware

Four genuine bare-metal programs, for either exact L012 C8T6/C8U6 package:
- `dac_steps`: PB0 12-bit steps and PB1 8-bit steps, unbuffered outputs.
- `opa_buffer`: PA3 positive input, PB0 output, OPA1 follower.
- `opa_pga`: PA4 positive input, PB1 output, OPA2 nominal gain8. Keep multiplied voltage within the linear output range.
- `opa_standalone`: PA6 positive input, PA7 negative input, PB0 output. Provide a stable external feedback network before running.

These assume a declared 3.3V board with VDDA=VDD, sufficient loads, and safe external input voltages. They are compile/link validated only. Nothing was flashed. OPA's example pause is a bring-up margin, not a guaranteed maximum settling interval: qualify BGR startup, OPA startup and output settling on the actual board. The drivers do not calibrate.

`cargo build --locked --target thumbv6m-none-eabi --no-default-features --features cw32l012c8t6 --bins`

Use `cw32l012c8u6` for QFN48. See `../../docs/dac-opa.md` for electrical limits, ownership and exact deferred functionality.
