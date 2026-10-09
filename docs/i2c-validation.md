> Historical software-verification references: HAL tests and fixture harnesses cited below were deleted on 2026-10-08. Their reported results apply to the dated source snapshot only; source facts remain unchanged. See [current HAL layout and build scope](hal-production-layout.md).

# F030/A030 I2C implementation and validation boundary

`embassy-cw32::i2c` is an experimental, blocking seven-bit I2C master for
source-verified CW32F030/CW32A030 instances. It does not imply I2C HAL support on
any other CW32 family. The PAC may describe additional chips independently.

## Sources and register decisions

- CW32x030 User Manual CN V2.5, chapter 20, §§20.4–20.7.
- CW32x030 User Manual EN V1.0, chapter 20, used as a cross-check.
- [Official F030 Standard Peripheral Library V2.2](https://www.whxy.com/uploads/files/20241111/CW32F030_StandardPeripheralLib_V2.2.zip),
  `Libraries/inc/cw32f030_i2c.h`, `Libraries/src/cw32f030_i2c.c`, CMSIS register header.
- Pinned [Embassy I2C architecture](https://github.com/embassy-rs/embassy/tree/f16efeffe37581092ec184718e6fdb1620393214/embassy-stm32/src/i2c):
  `Peri` ownership, sealed instance/pin traits, Config, blocking bus methods, and
  embedded-hal 1.0 transaction semantics. CW32 register transitions are original
  implementation using the CW32 sources, not renamed STM32 register operations.

The baud-rate divider is `8*(BRR+1)`, where BRR is **1..255**, not 0..255.
The configured SCL rate is a ceiling, capped at the documented 1 MHz capability.
Integer division rounds the divisor up from the initialized **upper PCLK
envelope**. Nominal Hertz is retained for reporting, not used as an electrical
ceiling. See `docs/i2c-clock-bounds.md` for source-qualified temperature/supply
bounds and the classic waveform-analysis limit. FLT selects simple filtering at BRR<=9 and advanced
filtering at BRR>9, per §20.4.3. The driver does not provide an override that can
silently violate that rule.

SI is write-zero-to-clear and write-one-no-op. Clearing SI advances the hardware
state machine, so data, ACK/NACK and START/STOP settings are prepared first.
The driver waits for the documented 08/10, 18/40, 28, 50/58 status codes, and
waits for hardware to clear STO before reporting a successful transaction.
F8 is only "no relevant state information" and is **not** treated as a bus-idle
flag. Startup additionally checks both physical lines before requesting START;
the hardware performs its own bus-state arbitration after START is requested.

GPIO configuration uses the internal verified open-drain AF helper. It sets the
open-drain register before enabling output, preserves each configured pull-up
and neighboring pins, and leaves the input path readable. External pull-ups and
board-level rise-time/electrical validation remain required.

## API behavior

- `I2c<'d, Blocking>` owns the peripheral, SCL and SDA pins through `Peri`.
- Both a panicking and checked constructor are provided. Invalid configuration
  is rejected before MMIO. Reborrow tokens if they must remain usable on error.
- `blocking_read`, `blocking_write`, `blocking_write_read`, and
  `blocking_transaction` implement seven-bit master communication.
- Addresses are **unshifted** values from 0 to 127. The driver constructs SLA+R/W
  in DR; it does not write the target address into slave own-address registers.
- Adjacent writes share one address phase; adjacent reads share one address
  phase and ACK across buffer boundaries. Direction changes use repeated START.
- The last byte of each contiguous read run is NACKed. The transaction ends in
  one STOP, with completion checked before success.
- An empty write performs an address probe; an empty operation list is a no-op.
  Any empty read or invalid address rejects the entire transaction before MMIO,
  including any earlier writes in that operation list.
- NVIC interrupts are not unmasked by this blocking driver. Do not independently
  unmask its I2C IRQ while using it.

## Failure and timeout contract

Address NACK and data NACK map to the corresponding embedded-hal error sources.
Arbitration loss is reported for 38 and the arbitration-to-slave states 68/78/B0.
The driver does not issue STOP or retry after arbitration loss, so it does not
terminate the winning master's transaction.

`Config::poll_limit` bounds **each** wait, including line release, hardware state
completion and STOP. This is a CPU polling-iteration limit, not milliseconds.
Elapsed time depends on CPU frequency, optimization, memory timing and interrupt
latency. A long transaction can use the bound for each byte plus its addressing
and START/STOP phases. Clock-stretching devices must complete within this bound.

A failure after acquiring mastership gets a bounded STOP attempt. State 00 is
cleared with the documented STO/SI sequence; if cleanup cannot complete, EN is
toggled and SI cleared using the manual's fallback. A START that never completes
is cancelled by disabling/re-enabling the controller. A fault latched while idle
is reported and cleared before any new user bytes are transmitted. A prior NACK/bus error is
preserved even if cleanup also times out. The driver never retries bytes or
pulses GPIOs to recover a slave: earlier bytes may already have changed it, and
an external device can still hold SCL/SDA low. Resolve that board/device problem
before retrying. Partially received buffer prefixes may have been updated.

## Deliberate limitations

- No async constructor, IRQ state machine, DMA or async trait. Consequently there
  is no async cancellation contract or DMA buffer-lifetime claim.
- No slave operation, ten-bit address trait, SMBus protocol, or multi-master
  scheduler. Arbitration failures are safely reported, but multi-master timing
  is not characterized by these tests.
- No silicon revision detection or chip-revision-specific errata workaround.
  Passing host and compile tests is not a claim of silicon errata compliance.
- No hardware-in-loop validation, logic-analyzer capture, electrical validation,
  oscillator-accuracy calibration or high-speed signal-integrity claim.

## Reproducible checks

Host tests execute the production protocol engine against a deterministic
register-state model and exercise real PAC accessors over RAM. They cover BRR
boundaries/rounding/filtering, RCC gate/reset masks, register offsets, START,
repeated START, consecutive operation merging, final NACK, STOP completion,
NACK classification, arbitration without STOP, finite busy/stretch/timeout
behavior, fault cleanup and partial-read contents.

Run (with the repository's cached Rust toolchain configured if needed):

```sh
cargo test --offline -p embassy-cw32 --no-default-features --features cw32f030c8t7 i2c
cargo test --offline -p embassy-cw32 --no-default-features --features cw32a030c8t7 i2c
python3 tests/test_i2c_hal_contracts.py
CW32_I2C_TEST_CHIP=cw32a030c8t7 python3 tests/test_i2c_hal_contracts.py
```

The ARM compile controls verify both instances and all blocking operations,
embedded-hal 1.0 compatibility, borrowing retained until Drop and reuse after
Drop. Negative controls reject incorrect instance/signal pins, concurrent
peripheral/pin borrowing, unimplemented ten-bit/async traits, fake async
construction and user-defined routes that bypass the sealed traits. Every
verified AF route is compile-checked on the selected LQFP48 package. A TSSOP20
positive control additionally verifies valid I2C1/I2C2 routes, with a negative
control rejecting its unbonded PB6 pin.

Validation completed on the active source tree: 28/28 focused host tests on each
of F030C8T7 and A030C8T7; ARM positive controls and nine intended failures on each
family; A030 ARM `defmt` configuration check. Exact command output is retained in
`docs/verification-logs/i2c-host-{f030,a030}.log`,
`docs/verification-logs/i2c-arm-{f030,a030}.log`, and
`docs/verification-logs/i2c-arm-defmt.log`. These are build/model results only.
