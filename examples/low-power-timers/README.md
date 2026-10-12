# Run-mode AWT and LPTIM firmware examples

`polling` starts an owned periodic timer and polls its elapsed event. `interrupt` installs a typed interrupt binding and awaits events under the pinned Embassy thread executor. Neither changes GPIOs or enters deep sleep. These are real Cortex-M firmware binaries; compilation/linking is not hardware validation.

Build either binary with an exact package feature, for example:

```sh
cargo build --locked --manifest-path examples/low-power-timers/Cargo.toml \
  --target thumbv6m-none-eabi --no-default-features --features cw32l010f8p6 --bin interrupt
```

The existing `polling` and `interrupt` binaries support the thirteen exact-package features listed in Cargo.toml. The build script derives FLASH/RAM sizes from selected-chip metadata. These two binaries use HSIOSC for AWT on A030/F002/F003/F020/F030/L031/R031/W031 and PCLK for LPTIM on L010/L011/L012/L052/L083. Their default reload/divider values intentionally produce different periods on different clock trees.

L052/L083 bind a vector shared with BTIM2. This example never enables BTIM2; a real application that does must bind and service its handler too. No constructor/handler/drop unpends or disables a shared NVIC line. Events coalesce. To recover from an LPTIM synchronization failure, stop using the faulted object and reconstruct through a reborrowed singleton, causing an independently owned LPTIM reset.

No pins need wiring for `polling` or `interrupt`. Observe the `nop` after an elapsed event with a debugger if desired, noting that default debug freeze settings affect timer progress. Do not interpret a successful link as proof of timer, clock-domain, or deep-sleep behavior on a board.

## Owned LSE AWT events

`lse_events` is an additional run-mode firmware binary enabled only by the nondefault `lse-awt` marker. It configures a crystal source through RCC, acquires an owned `LseClock` using reborrowed SYSCTRL/PC14/PC15 tokens, and passes it to `Awt::new_lse`. Its first phase starts AWT and polls for at most 100,000 iterations, observing the retained event and source readiness separately. It records whether an event was seen and drops the Blocking owner regardless. This is an iteration bound, not a time deadline, proof that an event occurred, or a startup-duration guarantee.

The binary then reacquires `LseClock` through the original tokens and constructs `Awt::new_async_lse` with the typed AWT interrupt binding. It continues awaiting retained events under the pinned Embassy executor. It creates no independent RTC owner. A successful link checks types, target and linking; it does not execute polling, reconstruction, interrupts, source loss, or a silicon timing sequence.

Build one of the six own-family selections already present in this crate:

```sh
cargo build --offline --locked --release --manifest-path examples/low-power-timers/Cargo.toml \
  --target thumbv6m-none-eabi --no-default-features \
  --features cw32f030c8t7,lse-awt --bin lse_events
```

| Existing feature | Own-family LSE qualification | PC14 / PC15 package pins |
| --- | --- | --- |
| `cw32f020c6u7` | F020 datasheet Rev1.3; 1650–5500 mV, −40–105 °C | QFN48: 3 / 4 |
| `cw32f030c8t7` | F030 datasheet Rev1.9; 1650–5500 mV, −40–105 °C | LQFP48: 3 / 4 |
| `cw32a030c8t7` | A030 datasheet Rev1.1; 1650–5500 mV, −40–105 °C | LQFP48: 3 / 4 |
| `cw32l031c8t6` | L031 datasheet Rev1.9; 1650–5500 mV, −40–85 °C | LQFP48: 3 / 4 |
| `cw32r031c8u6` | R031 datasheet Rev1.2; 2200–3600 mV, −40–85 °C | QFN48: 2 / 3 |
| `cw32w031r8u6` | W031 datasheet Rev1.3; 2000–3600 mV, −40–85 °C | QFN64: 61 / 62 |

The six board declarations in `lse_events.rs` are illustrative, not measurements or board qualification: 3000–3600 mV, −20–70 °C, a 32766–32770 Hz per-cycle envelope, Strong drive, Normal amplitude, Cycles16384, and a 20,000,000-attempt startup budget. Replace them with actual qualified crystal, loading, drive, amplitude, supply, temperature, aging and per-cycle bounds before running on a board. Nominal 32768 Hz or an average ppm specification alone is insufficient. Connect the appropriately qualified crystal network to PC14/OSC32_IN and PC15/OSC32_OUT; there is no GPIO output or pin-toggle claim. See the [LSE board contract](../../docs/qualified-lse.md) and [owned LSE AWT contract](../../docs/lse-awt.md) for source authority and limitations.

The metadata admission is exactly F020C6U7, F030C8T7, A030C8T7, L031C8T6, L031C8U6, L031F8U6, R031C8U6 and W031R8U6. This crate exposes six existing representative features; the other two L031 packages have library-level admission. The build rejects `lse-awt` with any other selection. F002/F003 remain HSIOSC examples, and the default L010 all-bin path skips `lse_events` through its required feature.

### Events, source health and retained recipients

- `take_elapsed()` and `wait()` consume retained, coalesced AWT events, including events preceding an LSE fault. Neither reports current source health, an event count, or uninterrupted elapsed wall time. `is_clock_ready()` is a separate snapshot, not a measured frequency or guarantee of another edge. It does not acknowledge events or faults or inspect/open GPIO banks.
- LSE loss produces no driver wake. With no retained event, the async wait can remain pending forever. Checks before or after the wait cannot enforce a deadline; that needs an independently waking timeout path that remains valid under the application's failure assumptions. This example supplies no such path and makes no deep-sleep or recovery guarantee.
- Acquiring or reacquiring `LseClock` can temporarily open the whole GPIOC gate to inspect PC14/PC15. Retained bank sampling, filters and armed edges can advance during that window, including before an acquisition error. Restoring the gate does not undo that progress. The example's Drop/reacquire phase repeats this existing effect; AWT adds no scan of other recipient banks.
- On L031/R031/W031, inherited GPIOA/B/C/F filters can use AWT underflow through shared `FILTER.FLTCLK=7`. Taking over AWT, changing its source/divider/reload, starting/stopping it, dropping it or losing LSE can change or stall these filters and delay or suppress edge/EXTI delivery. Owning AWT, SYSCTRL and oscillator pads does not own those recipients. Applications that need preserved filter timing must arrange a compatible functional handover. F020/F030/A030 have no such qualified FILTER7 route.
- Normal Drop stops and masks AWT while retaining LSE, monitoring LSI and oscillator-pad reservations. Reacquisition does not clear faults or recover the source and can fail. A successful replacement constructor clears old AWT retained events. Forgetting a running owner skips Drop and can leave counting/IRQs and recipient effects active; token lifetimes are not a shutdown guarantee.
