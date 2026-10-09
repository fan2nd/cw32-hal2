# Independent L012 ADC scans

Exact-part, ordinary Cortex-M0+ firmware examples for CW32L012C8T6 and
CW32L012C8U6. Build with one feature selected and target thumbv6m-none-eabi.
No firmware was executed by the source verification workflow.

- `owned_ordered`: ADC1 executes three caller-ordered external slots, including
  a repeated pin; ADC2 remains independently owned and executes its own two
  slots. Dropping ADC1 preserves ADC2 and the common analog resources.
- `borrowed_internal`: all eight ADC1 slots use borrowed external pins and
  owned internal handles, including repeated sources and different sample
  times. An independent ADC2 remains alive; subsequent single reads demonstrate
  that eight-slot scans do not change the single-read contract.

These examples declare a 3.0–3.6 V supply, VDDA=VDD, qualified HSI temperature
range, and HSI/8. Adjust the board declaration and source impedance/acquisition
assumptions before using on hardware. PA0, PA1 and PA8 must remain within
0..VDDA. Results are raw 12-bit counts; no average or calibration is applied.
The existing binaries use bounded polling. The async binaries below add ADC1
interrupt completion; DMA, external triggers and dual coupling remain absent.

```
cargo build --offline --locked --release --bins --target thumbv6m-none-eabi \
  --manifest-path examples/l012-adc-scan/Cargo.toml --no-default-features \
  --features cw32l012c8t6
```

## ADC1 async examples

- `async_single`: ADC1 raw single reads with `with_timeout`; the next loop can
  reuse the owner after successful cancellation. An independent blocking ADC2
  reads PA8 after every ADC1 attempt.
- `async_scan_borrowed`: eight ADC1 slots mix PA0/PA1, temperature and bandgap,
  with repeated handles and per-slot sample times. `select` demonstrates caller
  cancellation, followed by immediate reuse through `with_timeout`, a single
  read and a separate blocking ADC2 read. A canceled/error scan leaves every
  output element unchanged.

Enable `async` to use the existing pinned Embassy executor, time and futures
revision. This feature reserves GTIM1 and its interrupt for the existing run-mode
time driver. Keep the declared HSI/8 clock and board conditions qualified, or
recalculate the ADC and time-driver requirements for your board. ADC2 has no
async constructor because ADC2_DAC shared-vector ownership is not yet exposed.

```
cargo build --offline --locked --release --bins --target thumbv6m-none-eabi \
  --manifest-path examples/l012-adc-scan/Cargo.toml --no-default-features \
  --features cw32l012c8t6,async
```

The same command supports `cw32l012c8u6`. `Config.timeout` bounds synchronous
poll attempts, not total await time. Caller deadlines need working timer/IRQ
service; neither timeout winning nor maximum IRQ latency is promised. A terminal
cleanup fault persists until chip reset. The example's `unwrap` makes such a
fault visible through its panic handler. See the complete
[ADC1 async/source contract](../../docs/adc-remaining-async.md) and
[time-driver contract](../../docs/time-driver.md). Building these ordinary
firmware examples does not execute them or qualify silicon behavior.
