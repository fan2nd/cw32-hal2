# Independent review: L052/L083 serial GPIO hooks

Reviewed the newly added `alternate` and `alternate_with_type` paths in
`embassy-cw32/src/gpio/l052_l083/mod.rs`, the `Flex` ownership/initialization
boundary, SPI/I2C callers, and the accompanying RAM-backed GPIO tests.

Result: no blocking functional finding in the reviewed hook paths.

## Source-backed conclusions

- L052 UM CN V1.5 §§9.3.2–9.3.5 pp142–144 and L083 UM CN V2.0
  §§9.3.2–9.3.5 pp153–155 document digital output/input, analog disconnection
  and four-bit alternate selectors. Both SDK I2C polling-master examples
  explicitly configure SCL/SDA as `GPIO_MODE_OUTPUT_OD`.
- DIR=1 disables the output driver, ANALOG=1 disconnects digital input/output
  and weak pulls, OPENDRAIN=1 selects open drain. The hook disconnects before
  changing selector/type, configures pulls and the complete AF nibble, clears
  ANALOG, then enables output last. SPI MISO/UART RX retain DIR=1; SPI SCK/MOSI
  and UART TX use push-pull; I2C SCL/SDA use open drain with digital input.
- AFRL at +0x18 serves pins 0–7, AFRH at +0x14 serves pins 8–15. The complete
  four-bit target field is replaced without changing neighboring fields.
  Although the field is four bits, only AF0–AF7 are documented. The hook's
  `af < 8` check occurs before register writes.
- DIR +0x00, OPENDRAIN +0x04, PDR +0x0c, PUR +0x10, ANALOG +0x1c are
  individually masked. No +0x08 SPEED register exists or is written.
- Configuration locks are correctly unlocked at the `Flex::new` ownership
  boundary, before the hook runs: L052 `lock()`, L083 `lckr()`, both +0x3c.
  The write uses key 0x5a5a in bits31:16 and clears only the owned pin's lock.
  This is documented in §9.6.14, L052 p155 and L083 p167. The hook need not
  repeat the unlock because safe `Flex` exposes no relocking operation.
- The inherited disconnect path clears only the owned pin's rise/fall/high/low
  interrupt enables. It leaves pending ISR/ICR, the shared filter, output latch,
  neighboring configuration and SYSCTRL remapping unchanged.

## Checks and limits

`cargo test --offline --locked -p embassy-cw32 --no-default-features
--features cw32l052 --lib gpio::` and the same command for `cw32l083` pass all
13 GPIO tests on each family, including invalid-AF no-write checks and every
supported pad/AF nibble. Repository-local Rust and `CARGO_INCREMENTAL=0` were
used. The shared SPI/I2C contract run additionally compiled all 21 new feature
selections and 770 package-bonded signal routes.

The tests verify actual generated PAC writes against RAM. They do not establish
analog timing, absence of board-level glitches or electrical behavior on real
silicon. The stale `gpio_function` comment referring to a "GPIO-only backend"
was reported to the owner for documentation cleanup; it does not affect behavior.
