> Historical software-verification references: HAL tests and fixture harnesses cited below were deleted on 2026-10-08. Their reported results apply to the dated source snapshot only; source facts remain unchanged. See [current HAL layout and build scope](hal-production-layout.md).

# CW32 window watchdog: polling reset-mode implementation

Implemented in `embassy-cw32/src/wdg/windowed/mod.rs`. This is host/source and
compile validation only. **No hardware execution, reset timing measurement or
silicon-level certification has been performed.** Early-warning IRQ/async
support is intentionally absent; WDT IRQ0 is shared with IWDT.

## Scope and checked selection

| Families | Register version | Base | Gate/reset |
|---|---|---|---|
| F002/F003/F020/F030/A030 | `v1` | `0x40002c00` | APBEN1/APBRST1 bit4 |
| L031/L052/L083/R031/W031 | `cw32l031_v1` | `0x40002c00` | APBEN1/APBRST1 bit4 |
| L012 | `cw32l012_v1` | `0x40005400` | APBEN2/APBRST2 bit5 |
| L010/L011 | absent | none | none |

Build selection must verify each selected register version before emitting
`wwdt`. L010/L011 retain their independent watchdog but have no window-watchdog
HAL types or WWDT token. L012's gate writes replace bits31:16 with `0x5a5a`,
preserving unrelated low gate bits; APBRST2 is unkeyed and active-low. Other
WWDT gates are unkeyed. The driver only reads peripheral reset, never changes it.

`window-watchdog-evidence.json` contains per-family policies and exact official
manual/SDK URLs and SHA-256s, including derived manual text hashes. It derives
its source inventory from the immutable preimplementation
`read-only-crc-wwdt-audit.json`. F030/A030 share the x030 manual explicitly covering
both families; A030 register compatibility remains covered by the repository's
A030 source evidence. No family is enabled merely by similarity to another.

## Source-backed public contract

- `WindowPrescaler` contains the exact PRS encodings 0–7, corresponding to PCLK
  divisors 4096, 8192, 16384, 32768, 65536, 131072, 262144 and 524288.
- `WindowConfig::from_counts(prescaler, reload, window)` requires
  `0x40 <= window < reload <= 0x7f`. Reload and window are private and cannot be
  mutated past validation. There is no always-open-window shortcut.
- `WindowTiming::from_config(config, frozen_pclk)` exposes exact PCLK-cycle
  numerators and their nominal PCLK denominator. Its microsecond helpers round
  down. `for_intervals(timeout_us, closed_us, frozen_pclk)` rounds both requested
  tick counts up, requires 2–64 reset ticks and 1–reset_ticks−1 closed ticks,
  and uses u64 arithmetic. The caller supplies the actual frozen RCC PCLK, not
  a guessed oscillator frequency. Count-based configuration is the primary API.
- `WindowWatchdog::try_new(Peri<WWDT>, config)` retains ownership and reads only
  the RAM-backed frozen RCC snapshot. It neither gates nor accesses peripheral
  MMIO and rejects an absent/zero PCLK. The infallible wrapper panics on errors.
- `try_unleash()` checks software state first, enables/readbacks the clock gate,
  and requires the reset line released plus exact CR0/CR1/SR reset values
  `0x7f/0x7f/0`. Inherited EN, IE or non-reset state is rejected. Bootloader
  watchdog takeover and peripheral-reset escape hatches are absent.
- Startup writes CR1 with PRS/WINR and IE=0, checks CR1, preloads CR0 with EN=0,
  checks the preload, marks software state Starting, then writes EN|reload.
  After activation only EN is checked; a naturally decaying counter must not
  fail a spurious equality test. Successful startup performs no extra feed.
- A failed EN readback retains Starting state and the clock gate. It is not
  proof the hardware stopped. Reset is required to recover that instance. An
  earlier failed configuration/preload can also leave non-reset hardware and is
  never silently rolled back or taken over. All readbacks have finite per-read
  budgets, configurable through `with_poll_limit`.
- Repeated `try_unleash()` returns AlreadyRunning without MMIO; repeated
  `unleash()` panics. Neither feeds nor restarts, unlike the IWDT repeat-start API.
- `try_pet()` reads CR0 once and rejects EN=0, count>WINR or count<0x40 without
  writing. Accepted refresh writes exactly EN|reload, never a read-modify-write
  of the down-counter. Live equality with WINR is valid. There is no busy wait
  for the window to open and no deferred/automatic feed.
- The live read/check/write is inside a short critical section. This only
  excludes ordinary interrupt preemption. Hardware counting, NMI, bus stalls,
  debugger activity and other unmaskable delays remain. A tick from 0x40 to
  0x3f can still reset the MCU between the read and the write, even when
  `try_pet()` returns success in a software model. Applications need margin.
- `try_status()` is read-only. It samples live count/EN and sticky POV; these two
  reads are not an atomic hardware snapshot. POV can remain set across feeds
  and is not a claim about the current period. No POV-clear API is exposed.
- EN and IE are irreversible until reset. Drop does nothing. There is no stop,
  disable, reconfigure-after-start, reset, interrupt-enable/disable, NVIC setup,
  shared IRQ handler, debugger setting, sleep setting or reset-cause clearing.

## Timing and system limits

For divisor D, reload R, window W and nominal frozen PCLK f:

- Closed interval: D × (R − W) / f seconds.
- Reset interval: D × (R − 0x3f) / f seconds.
- Open interval: D × (W − 0x3f) / f seconds.

The manual's f=24 MHz, PRS=1, R=0x6f, W=0x4f example gives a nominal opening at
10.922666… ms and reset at 16.384 ms. The exact cycle numerator is exposed so
integer microsecond truncation is not mistaken for exact hardware timing.

These formulas are PCLK-running-time estimates, not guaranteed elapsed-time
bounds. Prescaler phase on refresh is insufficiently specified for a deadline
from the Rust call. Clock tolerance, MMIO/reset propagation and arbitrary
preemption also matter. Normal Sleep retains WWDT behavior; **DeepSleep stops
counting**, resuming after wake. Existing SYSCTRL.DEBUG bit10 can freeze WWDT
while halted and is preserved. Wake/global clock changes, direct gate writes or
peripheral reset invalidate assumptions. The application must keep clocks and
gates running and preserve exclusive WWDT register access after startup, even
if the Rust object is dropped. There is no coverage claim during freeze/stopped
PCLK or a promise software can return an error before a physical reset occurs.

## Embassy adaptation

Pinned upstream: `f16efeffe37581092ec184718e6fdb1620393214`,
`embassy-stm32/src/wdg/mod.rs`, SHA-256
`7c689010c88a7233b1b946817929a6f32c3167c34d39be9998cacfad09daf8a8`.

The implementation adapts Embassy's Peri/instance ownership, new/unleash/pet
vocabulary, small configuration values and u64 tick arithmetic. It does not copy
STM32 addresses, immediate constructor activation, unconditional clock/reset,
2-bit-prescaler assumptions or window=reload semantics. The existing CW32 IWDT
engine is retained separately and unchanged by this module.

## Primary references

Printed manual pages (not PDF viewer indices):

| Family | Manual revision | WWDT chapter/pages | SYSCTRL gate/reset |
|---|---|---|---|
| F002 | CN V1.4 | §14 pp193–200 | §4.7.11/.14 pp60/63 |
| F003 | CN V2.3 | §15 pp253–260 | §4.7.11/.14 pp62/65 |
| F020 | CN V1.4 | §16 pp258–265 | §4.7.13/.16 pp80/83 |
| F030/A030 | x030 CN V2.5 | §17 pp320–327 | §4.7.13/.16 pp82/85 |
| L012 | CN V1.4 | §20 pp436–444 | §4.7.13/.16 pp59/63 |
| L031 | CN V1.6 | §17 pp311–318 | §4.7.12/.15 pp77/80 |
| L052 | CN V1.5 | §18 pp348–355 | §4.7.12/.15 pp81/84 |
| L083 | CN V2.0 | §18 pp360–367 | §4.7.13/.16 pp86/90 |
| R031 | CN V1.3 | §17 pp314–321 | §4.7.12/.15 pp79/82 |
| W031 | CN V1.4 | §17 pp314–321 | §4.7.12/.15 pp78/81 |

For each WWDT chapter: §x.3.1–.2 documents PCLK, DeepSleep and enable;
§x.3.3–.4 documents strict configured counts, inclusive live window and timing;
§x.3.5 documents IRQ/reset and irreversible IE; §x.4 gives configuration order;
§x.6 documents EN, PRS/WINR and write-zero-clear POV. Own-family SDK/CMSIS headers
independently supply register masks, bases, eight divisors and gate/reset masks.
L010/L011 own CMSIS inventories have no WWDT, as recorded in the prior audit.

## Verification and remaining work

`tests/audit_window_watchdog_sources.py` verifies hashed official source inputs,
manual irreversible-bit/clear/divisor rules, own SDK masks, bases and clock/reset
banks, 13 family policies and every selected package/alias metadata record.
`tests/test_window_watchdog_contracts.py` compiles a positive ARM API fixture and
intentional failures for singleton ownership, sealed instances, private counts,
typed prescalers, no stop/reset/IRQ/async API, and absent WWDT on L010/L011.

The file-backed Rust tests exhaust every u8 count pair for each divisor, test
all valid configurations and exact timing formulas, ceiling and overflow
boundaries, protocol order/fault paths, key replacement, bounded reads,
held/inherited reset state, poisoned/repeated start, live refresh boundaries,
tick-between-read/write races, read-only sticky status, no-MMIO preparation/drop
and PAC-over-RAM offsets. RAM tests check all other SYSCTRL words remain unchanged.

Focused validation on 2026-10-08 passed: 18 host tests for each of the 11
capable family selections (198 executions); ARM typed contracts for all 13
families; and the source audit for 47 capable package/alias metadata records.
These focused runs do not replace the parent batch's aggregate checks, exact-part
regression matrix, generation determinism or independent acceptance review.

Hardware validation remains required on each supported silicon family: valid,
early and missed feeds; live equality at WINR; count=0x40 latency; inherited
bootloader state; reset cause; clocks/gates; Sleep/DeepSleep/debug freeze and wake
clock changes. Measure actual PCLK, counter transitions and reset propagation.
Any future early-warning IRQ work must independently review shared IWDT/WDT
coexistence, W0C clearing and worst-case latency within the single remaining tick.
