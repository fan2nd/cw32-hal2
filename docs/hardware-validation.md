# Hardware acceptance checklist

This is a future validation plan, not a record of tests performed.

## Common prerequisites

Record chip marking, package, silicon revision, board schematic, supply voltage,
clock sources, toolchain, generated-source hashes, and probe firmware. Keep SWD
connected and start from reset defaults. Verify physical pin availability against
the datasheet. The HAL's `init` configures the selected HSI clock path; use only
a corrected, dated checkpoint and observe each device's voltage/frequency limits.

## PAC/interrupts

- Check all vector names/numbers against startup assembly and CMSIS header.
- Confirm NVIC priority values map to four levels on F030.
- Trigger each shared IRQ independently and simultaneously; each handler must
  inspect its own source, and clearing one source must not lose another.
- Confirm no-feature and multiple-chip builds fail deliberately.

## Clock transitions

- Sweep every documented supply-voltage/frequency boundary, especially the
  below-1.8 V 24 MHz HCLK/PCLK limit on families where specified.
- Scope transitions from reset and valid bootloader states, with matching and
  differing factory trim, all allowed input sources and bus prescalers.
- Verify no HSI trim write occurs while enabled or stable. Confirm temporary
  oscillator readiness, source switching and stopped-state detection on silicon.
- Confirm FLASH wait states are raised before frequency increases and lowered
  only after the final clock and bus dividers take effect.
- Exercise oscillator failures and denied writes with a debug fixture. Verify
  bounded return, unpublished clock state and documented reset-before-retry.
- Verify that clock-security settings, watchdog use of LSI and unrelated clock
  gates survive the transition; include unexpected external-source loss.

## GPIO

- Test floating, pull-up and pull-down input with external known levels.
- Scope initial output configuration for glitches in both starting levels.
- Exercise push-pull and open-drain at both slew-rate settings.
- Check that adjacent pins retain direction, latch, alternate function and pulls.
- Test drop/reconfigure, cloned unsafe-token misuse exclusion, and port-clock
  sharing. Verify no GPIO driver disables a clock needed by another pin.
- Check pins with alternate/reset/BOOT/oscillator functions separately; never infer
  that a successful register write proves the special function was disconnected.

## CRC

- Feed ASCII `123456789` as bytes. Expected standard check values include
  CRC32 = `0xcbf43926`, MODBUS = `0x4b37`, CCITT-FALSE = `0x29b1`,
  XMODEM = `0x31c3`, and MPEG-2 = `0x0376e6e7`.
- Verify each supported preset against an independent reference implementation;
  F020 has eight CRC16 presets and does not support the two CRC32 modes.
- Compare byte/halfword/word accesses with the manual's low-byte-first ordering.
- Check incremental feed, empty input immediately after reset, repeated reset,
  peripheral drop/reacquisition, and preservation of other AHB clock-enable bits.

## Required before async/DMA release

Exercise interrupt-before-first-poll, interrupt-during-arm, cancelled futures,
shared IRQs, waker replacement, spurious IRQs, buffer lifetime, and DMA abort.
Measure peripheral timing; do not substitute host mocks for these checks.
