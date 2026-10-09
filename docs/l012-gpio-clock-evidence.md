> Historical software-verification references: HAL tests and fixture harnesses cited below were deleted on 2026-10-08. Their reported results apply to the dated source snapshot only; source facts remain unchanged. See [current HAL layout and build scope](hal-production-layout.md).

# CW32L012 GPIO and HSI clock backend

Current correction: initialization rejects inherited enabled HSE before MMIO
writes; the earlier external-source preservation description below does not
establish HSE pad ownership. Common RCC now captures inherited LSE pad ownership
before the backend runs; safe GPIO and peripheral pin construction rejects
reserved pads. See the [LSE ownership contract](inherited-lse-pads.md) and
[HSE entry conditions and source evidence](l012-inherited-external-clocks.md).

## Scope and source identity

This is an independent backend for `gpio_cw32l012_v1`,
`sysctrl_cw32l012_v1` and `flash_cw32l012_v1`, not a renamed F030 implementation.
It implements blocking GPIO and an HSI-based final system clock tree for
`cw32l012`, `cw32l012c8t6` (LQFP48), and `cw32l012c8u6` (QFN48).
Peripheral singleton and interrupt names still come from selected-device PAC
metadata. A PAC register block or singleton does not imply a HAL driver.

Primary sources used:

- [CW32L012 User Manual CN V1.4](https://www.whxy.com/uploads/files/20260603/CW32L012_UserManual_CN_V1.4.pdf)
- [CW32L012 Datasheet CN V1.0](https://www.whxy.com/uploads/files/20250717/CW32L012_DataSheet_CN_V1.0.pdf)
- [Official Standard Peripheral Library V1.0.5](https://www.whxy.com/uploads/files/20260701/CW32L012_StandardPeripheralLib_V1.0.5.zip),
  particularly `Libraries/inc/cw32l012.h`, `cw32l012_sysctrl.h`, and
  `Libraries/src/cw32l012_sysctrl.c`, `cw32l012_gpio.c`.
- Reviewed repository metadata: `cw32-data/clock/cw32l012.yaml`,
  `cw32-data/pinouts/cw32l012.yaml`, and family-specific register IR.

Source SHA-256 values, reproduced for auditability:

| Source | SHA-256 |
|---|---|
| UM PDF | `a9e54694a26f03c1f3e3041f40844900168ca2b8e6142f3328671b92e6b7a340` |
| UM text | `57f16772d6cfdb8055f8f2f91b5fed7257a963aa36c648293eb23762a6757505` |
| Datasheet PDF | `08a605fe8843d1eefa2b1115474b5a74df2833068f0ad6f35225e4f39377da76` |
| Datasheet text | `00ba7e2c9d26a8ce5a4eaa4cc2e7790773c427b13a4d491838622041abfa8b0a` |
| CMSIS header | `3758779c7b9e60fda1aa80dfd2848b6986074d91a6763076fa6777d55b1ad000` |
| SYSCTRL header | `3746c16b7a3fa72d1b43d42c080111e64faa36e8beb4b1f19c13daf3c71b993c` |
| SYSCTRL source | `6c1764be4a47f37f052fe6abed87b6bf54df87385e0c4072b7db819bcb8926cf` |
| GPIO source | `ff7fa4d5aea661047f7c8ec39b63486dffd02c80abb70a07da23d3ad20f07a58` |

## GPIO semantic review

UM §§9.3–9.6, CMSIS GPIO layout, and the datasheet Table 5-2 establish:

- All A0–A15/B0–B15, C13–C15 and F0/F1/F3/F6/F7 are physical
  bidirectional GPIOs on the documented 48-pad packages. Family masks are
  A/B `0xffff`, C `0xe000`, F `0x00cb`.
- PF3/BOOT is **I/O**, unlike input-only PF3 on F030/L031. The datasheet
  explicitly lists its output alternate functions. Its board-level BOOT role
  still controls boot selection on reset; using it as GPIO must respect the
  boot strap circuit.
- PA13/PA14 default to SWD. They are excluded from safe singleton pins.
  `AnyPin::steal` remains unsafe and requires the caller to resolve SWD remapping.
  This backend never changes SYSCTRL_CR2.SWDIO.
- NRST is a dedicated input, not a GPIO pad. No internal RF-connected pad exists
  on this family. Oscillator pins require exclusive board-level ownership;
  GPIO does not stop an oscillator or reconfigure its source.
- `DIR=1` selects input; `ANALOG=1` disables the digital paths.
  Output selection is performed last after the latch was set by the public
  constructor. Disconnection keeps the output latch, disables weak pulls and
  the pin's rise/fall interrupt enables, and leaves neighboring pins untouched.
- Four-bit AFRL/AFRH fields are at offsets `0x18`/`0x14`; selecting GPIO clears
  the full nibble. Some summary tables enumerate only AF1–AF7 although the
  detailed routing table includes AF8/AF9. No alternate-function HAL driver is
  enabled by this backend.
- No SPEED, LOCK, HIGHIE or LOWIE register exists. `Speed::Default` represents
  the fixed hardware drive behavior. Reserved offsets never receive F030
  lock/speed/level-interrupt writes.
- **Only PF3 has a pull-down resistor**, explicitly stated by UM §9.6.3
  (printed page 133; source text around lines 6940–6957). `Pull::Down` on another
  pad panics instead of silently requesting nonexistent hardware. Backend mode
  methods validate that request before their writes. Constructors may already
  have acquired/disconnected the pin before the mode validation.
  Non-PF3 PDR bits are never written. Pull-ups and open-drain input/output are
  supported; high open-drain output normally needs an external pull-up.
- BSRR/BRR/TOG are distinct one-bit command writes, at offsets `0x5c`/`0x58`/`0x60`.
  IDR is actual input, ODR is output-latch readback. RAM tests do not pretend to
  simulate the latch side effects or electrical behavior.
- AHBEN requires key `0x5a5a` in the upper halfword. A/B/C/F gates are bits
  4/5/6/**7**, not F030/L031's GPIOF bit 9. Other gates and reset controls remain
  untouched. Port clocks remain enabled because multiple drivers may share a port.

## Clock semantic review

UM §§4.3.4, 4.4.2, 4.4.3, 4.5, 4.7 and 7.4; datasheet §4.7; SDK SYSCTRL:

- HSIOSC is factory-calibrated **96 MHz**. The calibration halfword is at
  **0x001007c0**, masked to HSI.TRIM[10:0]. Erased `0xffff` is rejected before
  writes. The 48 MHz/F030 calibration address and divider encodings do not apply.
- HSI.DIV[14:11] encodes `/32` as 0, `/1`–`/10` as 1–10, and
  `/12,/16,/20,/24,/28` as 11–15. All 16 encodings are documented.
- Hardware resets to `/24` (4 MHz). HAL default is explicitly `/12` (8 MHz),
  matching the common default API while documenting the different reset state.
- HCLK divides SysClk by 1–128, PCLK divides HCLK by 1–8. Typed settings cannot
  request more than the documented 96 MHz AHB/APB maximum. Returned nominal
  frequencies round down to whole hertz; oscillator tolerance and the board's
  voltage/temperature/electrical limits still apply.
- SYSCLK selectors are HSI=0, HSE=1, LSI=3, LSE=4. Selector 2 is reserved; there
  is no PLL. A reserved incoming selector is rejected before writes.
- HSI.STABLE is the current readiness state. ISR.HSIRDY is a clearable rising
  edge flag and is not required for readiness after a bootloader cleared it.
- CR0, CR1, AHBEN and FLASH_CR2 writes need key `0x5a5a`. Only those four keyed
  registers plus HSI are changed. SYSCTRL_CR2, all resets, external oscillators,
  LSI configuration, interrupt flags/masks, cache/prefetch controls and unrelated
  gates are preserved.
- FLASH configuration clock is enabled first. WAIT=3 covers every documented
  legal incoming/requested clock up to 96 MHz and is verified before clock
  changes. Final WAIT is 0/1/2/3 for HCLK up to 24/48/72/96 MHz. The FLASH_CR2
  latency field mirrors SYSCTRL_CR2[6:4]; direct FLASH access avoids writing
  unrelated SWD/brake/wakeup bits. Barriers follow latency and source changes.

### Safe calibration and bootloader handover

The manual forbids changing oscillator parameters while HSIOSC runs (§4.3.4)
while explicitly allowing **divider-only** changes with unchanged TRIM (§4.5.2).
The SDK's `SYSCTRL_HSI_Enable` writes TRIM directly and is therefore not copied
as proof that live trim writes are safe.

If HSI.TRIM already equals the masked factory value, only DIV changes and no
oscillator stop is necessary. Otherwise the sequence is:

1. Enable the FLASH configuration gate and verify conservative WAIT=3.
2. Enable LSI without changing its TRIM/WAITCYCLE, then wait for STABLE.
3. Select LSI and verify SYSCLK=3, retaining existing bus dividers.
4. Disable HSI and verify both its enable bit and STABLE are clear.
5. Write factory TRIM and the requested DIV while HSI is stopped, verifying both.
6. Enable HSI, wait for STABLE, select it, and verify the source and final bus dividers.
7. Restore the prior software LSI-enable state and set the verified final flash WAIT.

LSI may already be implicitly enabled by a watchdog/filter/security monitor;
leaving its parameters untouched preserves those uses. CLKCCS and its monitors
are preserved. UM §4.4.3.3 states fallback to HSI applies while **HSE/LSE is the
current source**, so verified LSI staging avoids such a restart during calibration.
Initialization is exclusive and occurs before DMA, application interrupt handlers
or peripherals depend on the clock tree.

The generic §4.5.6 text mentions HSI.WAITCYCLE, but the actual HSI register table
and CMSIS header contain no such field. The implementation does not invent one.

Every hardware wait has the caller-specified nonzero iteration budget, including
LSI readiness, HSI stop, HSI readiness, keyed writes, source switches, bus settings
and latency. This is a poll budget, not a wall-clock duration. An error publishes
no frozen frequencies and can leave temporary LSI selected/enabled or other partial
clock changes. HAL ownership is consumed; reset before retrying. This is not a
runtime reclocking API and does not migrate already-running peripheral timing.

## Verification

Owned source/tests:

- `embassy-cw32/src/gpio/l012/mod.rs`, `gpio/l012_tests.rs`
- `embassy-cw32/src/rcc/l012/mod.rs`, `rcc/l012_tests.rs`
- `tests/test_l012_hal_contracts.py`

The host tests use a protocol model for ready/stop side effects and the **real
generated PAC** against aligned RAM for addresses and raw access. Model checks
cover all 512 HSI/AHB/APB configurations, ordered LSI handover, stopped-only trim
writes, source/readiness/flash verification, final poll boundaries, all 12
effectful-write failures, state preservation, GPIO AF halves, masks, command
writes and PF3-only pull-down behavior.

The ARM contract script builds a release library with and without `defmt` for
the family alias and both exact packages, exercises all 38 safe package pads,
Peri ownership, embedded-hal digital traits and RCC APIs, and checks specific
compiler diagnostics for forbidden SWD/nonexistent pins, Low/High speed,
unported peripheral modules, invalid-family divider and conflicting pin borrows.

This document describes software/source validation only. No board, silicon,
oscillator measurement, reset/BOOT experiment, SWD handover or electrical GPIO
validation has been performed. No HSE/LSE/PLL setup, low-power/time driver,
EXTI, CRC, USART, SPI, DMA, I2C, ADC1/ADC2 or timer HAL is exposed for L012.

### Executed results (2026-10-08)

- `cargo test --offline -p embassy-cw32 --no-default-features --features CHIP,defmt --lib`
  passed **20 tests per feature**, for the family alias and both exact packages
  (60 passing test executions). Logs: `docs/verification-logs/l012-CHIP-host.log`.
- `python3 tests/test_l012_hal_contracts.py` passed all three feature matrices:
  **six ARM release library builds** (without/with defmt), all 38 safe pins per
  feature, and **63 expected, diagnostic-specific compiler rejections**.
  Log: `docs/verification-logs/l012-contracts.log`.
- `python3 tests/test_l012_example_links.py` passed **four exact-package ARM
  release executable builds**, covering both packages without/with defmt.
  Parsed ELF32 headers, 48 exception/interrupt vectors, reset Thumb address,
  stack origin, and all loadable FLASH/RAM segment bounds. Footprint is 4,232
  FLASH bytes without defmt or 4,258 bytes with defmt, and 24 bytes static RAM
  (stack excluded), against the documented 64 KiB FLASH and 8 KiB RAM.
  Log: `docs/verification-logs/l012-links.log`.
- Rustfmt check passed for all four backend/test Rust files. Python compile
  checks passed for both validation scripts.

The link fixture is generated from selected-package metadata and exercises
96 MHz `hal::init`, GPIO output construction and toggling. It supplies the
Cortex-M runtime, a single-core critical-section implementation and a panic
handler. It is linked and inspected only; it is never executed by the test.

## Independent clock-safety review (2026-10-08)

The repaired implementation was checked against UM CN V1.4 §§4.3.4, 4.3.7,
4.4.2–4.4.3, 4.5.2, 4.7.1–4.7.5, 4.7.11 and 7.4, and DS CN V1.0
Table 7-4. Two findings were addressed before final regression:

- The temporary AHB divisor must cover the documented **90–100 MHz** HSIOSC
  calibration range, not only the factory's nominal 96 MHz. AHB /8 and
  APB /8 are installed and verified before any HSI change. AHB /4 could
  raise a legal low-voltage entry state using 100 MHz HSI and a stronger
  divider to 25 MHz. The /8 guard bounds that stage to 12.5 MHz.
- In the matching-trim path, an external-clock security fallback can change
  HSI to the documented 4 MHz setting. The requested DIV/TRIM and STABLE
  are therefore rechecked **after** the HSI mux switch, before final bus
  settings, lowering flash wait or publishing frequencies. A model-injected
  fallback now returns an error and retains conservative flash latency.

WAIT=3 is appropriate for every **legal incoming HCLK up to 96 MHz** and the
final configured clocks. The 100 MHz HSIOSC upper calibration bound does not
permit running HCLK at 100 MHz: a legal entry state must already divide that
oscillator enough to meet the voltage-dependent CPU/bus limit. Initialization
makes no promise to repair an already overclocked CPU or inadequate flash wait.

The routine requires a stable, legal entry clock throughout its initial
transition. Preserving CLKCCS does **not** make initialization a complete
external-clock fault recovery routine. In particular, the first guarded
SYSCTRL.CR0 read/modify/write preserves the entry source selector; if hardware
changes HSE/LSE to HSI after the read but before the write, that store could
restore a stale external selector. The manual does not guarantee rejection of
such a write to a failed source. The post-mux checks detect a changed HSI
frequency when execution reaches them, but cannot guarantee execution reaches
that point after every possible external-clock failure. Board fault-injection
validation and a separately designed fault-tolerant transition would be needed
for that stronger guarantee.

Other checked properties: all 16 HSI divider encodings are documented; SYSCLK
selector 2 is rejected; HSI trim changes require a verified LSI bridge and
HSIEN/STABLE cleared; divider-only live changes are allowed by §4.5.2; AHBEN,
CR0, CR1 and FLASH.CR2 carry their required write keys; cache/prefetch and
unrelated fields are preserved; all polls are bounded. Restoring the software
LSIEN bit deliberately does not require LSI.STABLE to clear, because §4.7.2
explicitly permits hardware consumers to keep LSI running independently.
This is source/protocol review, not silicon or electrical validation.
