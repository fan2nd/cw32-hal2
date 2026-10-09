> Historical software-verification references: HAL tests and fixture harnesses cited below were deleted on 2026-10-08. Their reported results apply to the dated source snapshot only; source facts remain unchanged. See [current HAL layout and build scope](hal-production-layout.md).

# CW32F020 backend compatibility review

This is a source-level review, not silicon validation. Similar family names and
identical SVD register descriptions are not sufficient evidence for a HAL backend.
The real F020 PAC remains selected throughout; no register-block pointer casts are
used to substitute an F030 SYSCTRL, GPIO or DMA controller.

## Sources

- [CW32F020 User Manual CN V1.4](https://www.whxy.com/uploads/files/20240920/CW32F020_UserManual_CN_V1.4.pdf), SHA-256 `279521eaf7d32b241e614a9553044a4d99c1e7ca2566efa3b0fdd2b66c106eed`.
- [CW32F020 datasheet download V1.3](https://www.whxy.com/uploads/files/20251230/CW32F020_DataSheet_CN_V1.3.pdf), SHA-256 `1e330d800f10114c654b97cbd45b64d56a99c3cb39138d4ef20de3c12968dab0`. The PDF's own revision labels must be distinguished from the download filename.
- [CW32F020 SDK V1.2](https://www.whxy.com/uploads/files/20240115/CW32F020_StandardPeripheralLib_V1.2.zip), especially `Libraries/inc/cw32f020.h`, `cw32f020_rcc.h`, `cw32f020_gpio.h` and the corresponding drivers.
- Package provenance and physical positions: `cw32-data/pinouts/cw32f020.yaml`.
- Gate/reset field provenance: `cw32-data/clock/cw32f020.yaml`.
- Serial, CRC and numbered-AF findings: `f020-serial-compatibility.md`.

All page references below are the printed manual pages; a zero-based PDF index
may differ and should not be inferred from those references.

## HSI clock backend

The `rcc/f020.rs` wrapper explicitly selects the shared bounded HSI algorithm
only for the following independently verified subset. SYSCTRL register classes
as a whole are different and are not aliased.

| Operation | F020 evidence | Implementation constraint |
|---|---|---|
| HSI nominal/reset clocks | RM 4.3.8 p53, 4.4.2 p54, 4.7.4 p70 | HSIOSC 48 MHz; reset DIV6 gives nominal 8 MHz |
| Factory trim | RM 4.4.2 p54, 4.7.4 p70; production SDK `RCC_HSI_TRIMCODEADDR` | Read halfword 0x00012600; reject erased 0xffff; write TRIM[10:0] |
| HSI divider | RM 4.7.4 p70 | Only encodings 5,6,8,9,11–15; do not expose SDK-only /3 encoding7 |
| CR0 mux/bus dividers | RM 4.7.1 p67 | SYSCLK[2:0]=0 for HSI, PCLKPRS[4:3], HCLKPRS[7:5]; preserve unrelated bits |
| Protected system writes | RM 4.7.1–2 pp67–68 | KEY[31:16]=0x5A5A; CR1[8:6] must be written1 |
| Stopped oscillator calibration | RM 4.3.4 p47; 4.5.2 p61 | If TRIM differs, switch through stable LSI, stop HSI and await STABLE=0 before changing TRIM; restart/read back before returning |
| HSI readiness | RM 4.7.4 p70 | CR1.HSIEN bit0, HSI.STABLE bit15; bounded polling |
| Leaving a bootloader PLL | RM 4.3.7 p52, 4.5.7 pp64–65 | Switch to stable current HSI before changing its divider/trim; clear PLLEN bit2 and await PLL.STABLE=0 |
| Flash configuration gate | RM 4.7 AHBEN table p79 | AHBEN.FLASH bit1; enable/read back before CR2 write |
| Flash latency | RM 7.4 p109 and 7.9.2 p119 | CR2 key0x5A5A; WAIT[2:0], FETCH bit3, CACHE bit4; final WAIT0≤24MHz, WAIT1≤48MHz |
| Requested maximum clock | Datasheet overview and clock description | HSI, SYSCLK, HCLK and PCLK never exceed48MHz |

The initial WAIT2 encoding is explicitly documented by the manual and provides a
conservative transition setting. The manual includes a72MHz flash-table row and
PLL ranges, but the datasheet rates the F020 core/buses for48MHz. This HAL neither
exposes a72MHz configuration nor promises operation of an overclocked bootloader.

The production trim location is independently stated in the manual. The SDK's
alternate0x001007AC address is inside `#ifdef __PRIVATE_DEBUG` and is not selected.
The default is based on the verified reset-divider semantics, not on a guessed
`SystemCoreClock` variable or copied example code.

Initialization preserves unrelated peripheral clocks, reset flags, debug/remap,
HSE/LSI/LSE and backup-domain settings. It intentionally stops PLL after switching
to HSI. No HSE/PLL setup, low-power integration, runtime reclocking or time driver
is exposed. Any timeout leaves no published frequencies.

## GPIO backend

`gpio/f020.rs` uses the actual F020 GPIOA/B, GPIOC and GPIOF types. The following
subset is independently checked in RM9.3–9.6 pp141–155:

- DIR1 means input and DIR0 output; ANALOG1 disconnects the digital path.
- OPENDRAIN1 means open-drain. SPEED0/1 selects low/high speed; it is not a frequency promise.
- AFRL at0x18 and AFRH at0x14 contain four-bit fields. Only AF0–7 are documented;
  reserved AF8–15 are rejected by the F020 helper.
- PUR/PDR are independent and never enabled simultaneously during reconfiguration.
- LCKR at0x3c uses key0x5A5A. Clearing an owned pin's lock bit unlocks that pin,
  preserving the remaining pins' lock states.
- ISR0x34 and IDR0x50 are read-only (RM9.6.14 p153 and9.6.18 p154), despite the
  imported SVD omitting those access restrictions.
- ICR is R1W0. BSRR0x5c, BRR0x58 and TOG0x60 are write-one command registers;
  output methods do not read-modify-write ODR.
- Clock gate GPIOA/B/C/F bits4/5/6/9 and their real addresses match RM9.5 p148
  and the SYSCTRL.AHBEN description.

Digital configuration disables interrupts for the owned pad. EXTI remains gated
unless independently selected; a matching GPIO layout alone does not expose an
interrupt-driven driver. This backend leaves the DRIVER register unchanged.

Safe singleton pins come from exact selected-package metadata. PA13/PA14 remain
reserved for SWD, PF3/BOOT is input-only and has no bidirectional token, and NRST
is a dedicated package signal. QFN20/QFN32/QFN48 physical bonding is not expanded
to a uniform pin bank. QFN32's exposed VSS pad retains vendor position0.
Oscillator pads require the caller to resolve their board/oscillator usage;
initializing GPIO does not disable or remap oscillators.

## Explicitly excluded blocks

- ADC now exposes the separately qualified 12-bit blocking subset through its real
  F020 PAC; GTIM counter/PWM uses independently reviewed canonical v1. See
  [F020 ADC/timer implementation](f020-adc-timer-implementation.md). Unsupported
  ADC 14-bit extensions and timer capture/advanced modes remain excluded.
- DMA controller has a different layout and only two real channels. Matching
  channel register classes do not justify the existing five-channel controller.
- No ATIM peripheral is invented. F020 genuinely has three UARTs, two SPIs,
  two I2Cs, four GTIMs and three BTIMs; only audited driver subsets are enabled.
- CRC requires a family-specific algorithm surface: the manual supports only
  CRC16 modes0–7 despite copied SDK/SVD CRC32 symbols. See serial review.

## Test boundary

F020 GPIO tests use the real selected PAC against aligned RAM and independently
listed manual offsets. They check pull exclusivity, owned-pin preservation, AF
bank/width, readback, atomic command writes, all port gates, package-independent
valid-pad bounds, reserved-AF rejection and physical base addresses. The shared
HSI model suite is compiled against the actual F020 PAC, covering all documented
frequency combinations, PLL exit ordering, readback failures, timeout bounds,
invalid state rejection and retained conservative latency on failure.

Host/RAM and compile/link checks establish software behavior only. There is no
board, electrical, oscillator-startup, I2C bus or UART/SPI waveform validation.

### Integrated verification (2026-10-08)

- Family alias and all three exact parts (`cw32f020`, `cw32f020f6u7`,
  `cw32f020k6u7`, `cw32f020c6u7`) each pass 98 host unit tests and two
  compile-fail doctests, plus Cortex-M0+ release builds with `rt,defmt`.
- `tests/test_f020_hal_contracts.py` passes all three packages: 13/25/37 safe
  GPIO pads and 25/51/73 typed serial routes for QFN20/QFN32/QFN48 respectively.
  All 79 intentional compile failures are checked for their exact diagnostics,
  including SWD/BOOT/unbonded pads, absent CRC32 modes and DMA3/ATIM tokens,
  unported drivers, wrong UART pin/IRQ, retained pin borrow, and RO GPIO access.
- All 116 serial AF cells pass independent SDK/current-datasheet comparison;
  82 are implemented HAL signal routes before package/SWD filtering.
- Independent SVD/data parity passes all 13 families with the six F020 GPIO
  access corrections and the one source-qualified CRC result-field correction.
  All 135 canonical IR ledger hashes match generated metadata. Full metadata
  contracts pass 19 tests.
- Initial shared-HSI extraction regression passed F030C8T7's 156 unit tests,
  two IRQ tests and five doctests, and A030C8T7's ARM `rt,defmt` build. The CRC
  family-default test subsequently adds one x030 unit test.

Logs are in `verification-logs/f020-hal-matrix.log`,
`verification-logs/f020-hal-contracts.log` and `verification-logs/f020-staged-parity.json`.
SPI and I2C are blocking-only; UART provides the existing byte-interrupt-driven
async API. CRC exposes eight presets and defaults explicitly to CCITT (hardware
reset MODE4) on F020. F030/A030 retain their original CRC32 default.
- `tests/test_f020_hal_links.py` additionally links real Cortex-M0+ firmware for
  all three exact packages and checks its ELF, 32 KiB FLASH/8 KiB RAM layout,
  initial stack, complete interrupt vector table and non-default UART3 handler.
  Each smoke image uses 8,994 FLASH bytes and 40 static RAM bytes, excluding
  stack. The binaries are not executed. Log: `verification-logs/f020-hal-links.log`.

### Clock correctness correction

A subsequent independent audit found that the first implementation changed HSI
TRIM while HSIOSC was running. Passing host tests did not validate that sequence.
The active backend now uses a bounded temporary-LSI handover and stopped-HSI
calibration, conservative temporary /4 AHB and /8 APB dividers, and final
oscillator/mux/bus readbacks before publishing clocks or reducing flash latency.
The legal-entry and board-voltage assumptions, exact source evidence and added
failure-model coverage are documented in [HSI safe recalibration](hsi-safe-recalibration.md).
Earlier delivered archives are unchanged and must not be treated as fixed.
