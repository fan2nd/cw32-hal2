> Historical software-verification references: HAL tests and fixture harnesses cited below were deleted on 2026-10-08. Their reported results apply to the dated source snapshot only; source facts remain unchanged. See [current HAL layout and build scope](hal-production-layout.md).

# Owned BTIM polling counters

## Scope and public API

The existing `timer::low_level::Timer<'d, T>` now accepts sealed `BasicInstance`
peripherals as well as the existing GTIM instances. Each driver retains its `Peri`
for its entire lifetime. This follows Embassy-stm32's separation of basic counter
capability from general-purpose compare/PWM capability without pretending CW32
register layouts or prescalers are STM32-compatible.

`BTIM1`, `BTIM2` and `BTIM3` implement only `BasicInstance`. They do not implement
`Instance` or `GeneralInstance4Channel`; there is no basic-timer PWM surface.
Existing x030/F020 GTIM PWM bounds remain unchanged. Every Rust module uses the
repository's `module/mod.rs` organization. Private BTIM adapters are selected by
real canonical IP/version cfgs, and use the actual family PAC types:

- `btim_v1`: F030/A030/F020
- `btim_cw32f002_v1`: F002/F003, without the absent DMA register
- `btim_cw32l031_v1`: L031/R031/W031/L083
- `btim_cw32l052_v1`: L052, with its actual CR/ETR register names
- `btim_cw32l010_v1`: L010/L011, with IER
- `btim_cw32l012_v1`: L012, with DIER including DMA request controls

The implementation supports internal PCLK, continuous up-counting, explicit
start/stop, counter read/write, checked frequency/period changes and polling
OV/UIF acknowledgement. PCLK is never multiplied by two. The maximum supported
clock divisor is `32768 * 65536 = 2^31`, which fits `u32`; frequency-selection
products use `u64` and ceil division so the exact resulting rate never exceeds
the requested whole-Hertz rate. Invalid configuration performs no register writes.

All families expose the existing 16 power-of-two prescalers from 1 to 32768 and
periods from 1 through 65536 ticks. Linear-PSC hardware supports additional
integer divisors up to 65536, intentionally outside this bounded API. This is
partial hardware-mode coverage. The whole-Hertz rate getter may return zero for
an explicit sub-Hertz configuration; `kernel_clock()/clock_divisor()` expresses
the exact rational frequency.

## Ownership and sequencing

All three BTIMs share one peripheral clock and reset. A per-instance constructor
cannot prove ownership of its siblings, including timers left running by earlier
firmware or configured through the PAC. It therefore never asserts the shared
reset and never gates the shared clock on drop. Construction enables the group
clock and software-initializes only the selected bank; drop stops that bank and
disables its requests. Other banks, reset controls and unrelated clock bits are
preserved. L010/L011/L012 APBEN2 writes include the required 0x5A5A key and bit 2;
other families use bit 12 without a key.

Construction and timing changes stop the counter before clearing CNT and writing
ARR. This avoids immediate ARR updates falling below the current CNT. Classic
PRS and L031/L052-style PSC changes latch on the next EN rising edge or overflow;
a running counter is restarted, while a stopped counter stays stopped. The
L010/L011/L012 adapter writes PSC, keeps URS=1 and UDIS=0, then writes EGR.UG=1 to
reset counter/divider phase. No GTIM-style preload claim is inferred for BTIM.
External/slave/reset-input modes and output toggling are disabled in owned banks.

The overflow indicator is one sticky bit, so multiple overflows coalesce. It is
not a lossless elapsed-time counter or Embassy global time driver.

## Manual evidence and command masks

`btim-counter-evidence.json` records each family's own manual, SDK source hashes,
canonical register IR hash, instances, version, gate and verified mode. A030 uses
the existing documented shared-x030 compatibility evidence. All six canonical
BTIM variants already had their vendor ISR access corrected to `Read`; modern
EGR registers were already `Write`. No register-data correction was needed here.

Command masks come from defined fields and access semantics, independently of
reset values:

- Classic ICR: OV bit 0, TI bit 1, TOP bit 2 are R1W0. Write 0x6 to clear only OV,
  or 0 to clear all defined flags. Reserved bits are written zero.
- L010/L011/L012 ICR: UIF bit 0 and TIF bit 6 are R1W0. Write 0x40 to clear only UIF,
  or 0 to clear both. Reserved bits are written zero.
- Modern EGR: UG bit 0 is a write-only command. Write 1 directly, with no read or
  read-modify-write of EGR. ISR is never written on any family.

## Verification and exclusions

Run `ci/check-btim-hal.sh` with the repository toolchain, or an existing compatible
Cargo/Rustup environment. It uses one dedicated `target/btim` directory, disables
incremental/debug info, and cleans family-specific build outputs between families.
The script checks formatting and canonical evidence, runs full host library tests
for 13 family aliases, checks positive/negative ARM type contracts, and links real
ARM smoke executables for one exact part per family. Generated reports and raw
host/link logs are under `docs/verification-logs/btim-*`.

Ten new tests cover the production engine, exact PAC RAM offsets, power-of-two
encodings, rate bounds, invalid setters, R1W0 masks, update order, stopped/running
state, and the actual public Timer constructor/reconfiguration/Drop. The handover
test starts with an already-running bank and verifies both neighbouring banks at
the family's real bank stride remain unchanged. Clock RAM tests verify all reset
registers remain untouched and foreign clock bits survive repeated enables.

ARM negative tests reject a second mutable peripheral borrow, using UART as a
timer, claiming BTIM PWM capability, capture/compare, external-counter setup and
async waits. ELF inspection verifies ARM executable type, Thumb entry, vector
layout, exact chip FLASH/RAM bounds and initial stack pointer. No firmware is run.

No async/shared-IRQ integration, DMA API, one-shot, external-counter/trigger/gate
mode, timer pin route, PWM, capture, encoder, hardware measurement or board-flash
validation is included. Leaving the shared clock enabled is an intentional
ownership tradeoff; low-power group shutdown requires a future group-owner API.
