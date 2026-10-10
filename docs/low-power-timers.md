# AWT and LPTIM run-mode timers

This change implements source-reviewed, owned polling and interrupt-driven timers on all thirteen currently supported families. It does **not** provide a low-power system path merely because the peripherals are named AWT or LPTIM.

| Families | Driver | Implemented source |
| --- | --- | --- |
| A030, F002, F003, F020, F030, L031, R031, W031 | `awt::Awt` | Undivided factory HSIOSC |
| L010, L011, L012, L052, L083 | `lptim::Lptim` | Qualified PCLK |

Both drivers follow the pinned Embassy ownership boundary: `Peri`, sealed instance traits, direct typed PAC registers, generated typed GLOBAL interrupts, and central `RCC_INFO`. The executable examples use the actual pinned Embassy thread executor. Their structure is informed by Embassy commit `f16efeffe37581092ec184718e6fdb1620393214`, but CW32 commands, flag polarity and update synchronization are independently reviewed rather than copied from STM32.

## Clock and ownership contracts

`hal::init` must first freeze the supported HSI-based RCC tree. AWT selects **HSIOSC**, before the HSI divider; its frequency must not be substituted by `Clocks.hsi` or PCLK. Its prescaler is 2..32768, encoding zero is reserved, and the required MD field is `11`. The period is ARR+1 counter ticks. Configuration completes while EN=0; EN rising reloads ARR. `set_config` deliberately stops the timer and leaves it stopped.

LPTIM selects PCLK with no input/trigger pins, no filter, no encoder, and immediate reload update. Its prescaler is 1..128. Software conservatively rejects ARR=0. `set_config` stops the timer, configures CFGR while EN=0, enables the clock domain, clears stale ARROK, writes ARR once, and waits for ARROK before stopping again. New writes never overlap an unacknowledged update. A synchronization timeout stops the core and faults the object; drop and reconstruct a reborrowed peripheral to recover with an independently owned LPTIM reset.

Clock bounds come from the existing generated, own-family electrical data in `cw32-data/electrical.yaml`. AWT uses `ClockBounds::hsi(1)` divided by its actual prescaler. LPTIM divides `bus_clock_bounds` by its actual prescaler. No new tolerance percentage or synthetic LSI/LSE bound is introduced. These contracts retain the existing declared VDD, ambient-temperature, factory-trim and frozen-clock qualifications; they do not measure frequency.

The LPTIM EN startup delay covers two counter-clock periods using conservative CPU-upper/counter-lower frequency endpoints. The calculation retains the exact rational endpoints, including valid counter rates below 1 Hz, rather than dividing by a rounded whole-hertz value. This is a relative cycle wait and does not add an absolute-period guarantee to rate-only sources. Its software-start latency is additionally documented as three kernel clocks. `start` and `start_once` restart CNT through acknowledged SRST; they do not promise divider-phase alignment or an exact wall-clock deadline. The driver exposes clock bounds and raw reload configuration rather than claiming an unsupported monotonic/deadline contract. Counter reads use a bounded search for two equal consecutive samples; a fast PCLK/div1 timer may legitimately return `UnstableCounter`.

AWT never pulses reset or disables a clock gate. LPTIM constructor reset is permitted by `RCC_INFO` only for an independently owned reset field, checked against generated shared-group metadata. Drop stops the owned timer, disables only its own event enable while stopped, and leaves every oscillator and bus gate alive. No shared BTIM/ADC/AWT group is reset or gated. AWT and LPTIM do not claim SYSCTRL ownership; the already-frozen RCC tree guarantees the source remains available under the safe run-mode HAL contract.

## Commands and interrupts

- AWT ISR is read-only; ICR.UD is R1W0. Its documented reset/no-op seed is `0x3f`, including reserved reset-one bits. Acknowledge changes only UD from that seed.
- LPTIM ISR is read-only; ICR has seven R1W0 event flags. Its `0x7f` no-op seed preserves all other events when acknowledging ARRM or ARROK.
- LPTIM CR/CR0 contains mixed commands: SNGSTART/CNTSTART are write-one commands; SRST/ARST are write-zero reset commands with ready readback. Its generated `0x18` no-op seed avoids accidental reset or replay of a start command. The driver waits for reset readiness before issuing SRST and waits for acknowledgment afterward.
- LPTIM IER is writable only while EN=0. The handler never changes it while counting. It clears ARRM, latches one coalesced software event and wakes the task.
- L052 shares `BTIM2_LPTIM` and L083 shares `BTIM2_LPTIM1`. Neither constructor, handler, cancellation nor drop disables/unpends the NVIC line. All other enabled sources on the vector require their own handler, including bootloader/raw-PAC owners. A foreign unhandled source may otherwise cause an interrupt storm.
- `wait().await` consumes an already-retained event. Cancelling that future leaves the timer and event intact. Multiple elapsed periods can coalesce into one event; this is not an event counter. Stopping retains an event; restarting deliberately discards it. Dropping the timer stops its hardware, but cancelling a wait does not.

Register enums and command seeds are curated in `cw32-data/registers/{awt,lptim}_*.yaml` and `cw32-data/register-writes.yaml`, projected through the normal data/PAC generators. IRQ and RCC routes are consumed from existing generated chip metadata. `docs/low-power-timers-evidence.json` records each own manual revision, official URL, SHA-256, PDF and printed pages, page-text hash, source SDK bytes, generated routes and qualified electrical profile.

## Remaining dependencies

LSI/LSE operation requires its own per-family electrical frequency minima/maxima, VDD/temperature qualifications, source-ready protocol, source lifetime capability, and preservation of independently owned RTC/IWDT/AWT/LPTIM/RF consumers. Existing HSI bounds must not be reused for a low-speed oscillator. LSE additionally needs approved board crystal/drive parameters and oscillator-pin ownership. F002/F003 AWT source encodings 2/3 refer to HEX inputs, whereas other AWT variants use HSE/LSE, so the new common typed source enum intentionally exposes only the reviewed HSIOSC encoding.

Deep-sleep wake further requires a source-reviewed system entry/resume sequence, wake-enable and pending-flag handling, retained clock/voltage/flash transitions, peripheral stop constraints, and a race-free executor sleep/wake path. An Embassy time driver requires persistent monotonic epoch/overflow accounting, alarm scheduling, rollover/compare race handling and a reviewed time queue. None is implemented by this patch. External counting, GPIO routing, triggers, encoder and PWM are also not implemented.

## Verification

Ordinary `cargo check` targets ARMv6-M for an exact orderable feature in each family. Both example binaries link for each exact feature. The interrupt example awaits real IRQ-backed futures under the pinned Embassy executor; it does not fake an interrupt or execute a host MMIO harness. No HAL tests or test harnesses are added, no firmware is executed, and no push is performed. See the frozen handoff manifest for commands, log hashes and results. Hardware clock-domain timing remains unvalidated on silicon.
