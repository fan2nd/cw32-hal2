# Buffered timer input capture and quadrature decoding

This batch provides owned polling `timer::input_capture::InputCapture` and `timer::qei::Qei` for the buffered GTIM instances and ATIM on CW32L010, CW32L011 and CW32L012. It uses each selected family's own typed PAC and the existing RCC, timer ownership, GPIO and package-qualified AF metadata. It adds no register aliases, raw register adapter, clock-gate writes, unchecked pin routes or trigger-source enum. The existing classic timer counters/PWM are unchanged.

## Embassy correspondence

Reference: embassy-rs/embassy commit `f16efeffe37581092ec184718e6fdb1620393214`, `embassy-stm32/src/timer/input_capture.rs`, `qei.rs` and `low_level.rs`.

- `CaptureInput::from_pin` retains timer/channel/pin typing. CW32's guaranteed external pin route returns the wrapper directly, not `Option`; there is no `from_trigger`.
- `InputCapture::new` accepts four optional typed inputs, the counter tick frequency and `CountingMode`. This polling API has no IRQ binding or asynchronous waits. Its requested frequency is a ceiling selected against the qualified PCLK upper bound, using the existing power-of-two prescaler subset. ARR is fixed at 65535. `tick_frequency_bounds` reports outward-rounded qualified bounds; the nominal rate is not a measured frequency.
- `enable`, `disable`, `is_enabled`, `set_input_capture_mode`, `set_input_capture_filter`, `set_input_capture_prescaler`, `get_capture_value` and `get_input_interrupt` retain the reference names and purposes. The status flag is available with interrupts disabled. Filter and prescaler programming briefly disables the selected channel, resetting its capture divider and allowing a reconfiguration gap. No alternate/TRC capture mapping is exposed.
- `Qei`, `Config`, `QeiMode`, `Direction`, `count`, `reset` and `read_direction` retain the reference shape. Config is not generic because all selected counters are 16-bit. CH1 and CH2 parameters are separately typed. Mode1 is CW32 TI1 x2, Mode2 is TI2 x2, and Mode3 is both inputs x4; these mappings are taken from the CW32 manuals, not inferred from STM32 register values.

## Capture and position semantics

A CCR is one latest-value latch, not a queue. Reading it acknowledges that channel's CCxIF. The implementation does not then clear IF in software, which could discard a newer event. `is_overcapture_pending` exposes CCxOF, and `clear_overcapture` clears only that flag with a typed R1W0 write preserving all other defined flags, including ATIM's additional break and channel events. Separate status/CCR reads are not an atomic snapshot. Overcapture events can coalesce; no exact lost-event count or lossless delivery is promised. Disable a channel before collecting a stable diagnostic snapshot.

Capture timestamps wrap modulo 65536. `wrapping_sub` gives elapsed ticks only when fewer than 65536 ticks elapsed between samples. The overflow flag is coalescing and cannot extend the timestamp. Qei counts modulo `auto_reload + 1`, rejects ARR=0, and does not accumulate wraps into a signed software position. Direction is an independently observed hardware state and does not identify a particular wrap. `reset` writes zero while decoding continues, so concurrent edges are not synchronized to it.

All capture inputs start disabled while the counter starts running. An omitted input cannot be enabled, configured or sampled. Drop disables capture channels, stops the timer, disconnects inputs and then uses the existing owned timer drop/RCC path. Qei starts immediately with PSC=0, no inversion, no index behavior, and two required inputs. ATIM outputs and complementary outputs remain disabled.

## Electrical and package limits

Each own datasheet specifies an external timer input ceiling of `fTIMCLK / 2`; use the qualified PCLK lower endpoint when deriving a conservative signal ceiling. This ceiling is not a software service guarantee. Digital filtering reduces accepted bandwidth, and GPIO input voltage, rise/fall and operating-condition limits still apply. Input frequency cannot be measured or validated by the constructor. GPIO retains its existing pull checks, including L012's restriction of pull-down to PF3. The examples use no internal pulls.

No L010 external ATIM CH4 route is currently qualified under the existing oscillator ownership rules. L010Y8M6 only has a qualified ATIM CH3 input, so no ATIM encoder is constructible for that package. The driver does not introduce the excluded PA0/PB0/PB1 routes. See the [own-source map](timer-input-source-evidence.md) and [machine-readable evidence](timer-input-source-evidence.json) for all exact-package routes and source hashes.

## Verification and gaps

`ci/check-timer-input.sh` performs ordinary ARM release compilation for all ten buffered family/package selectors with and without defmt, ten classic-family regression builds, and real linked capture/encoder firmware for all seven buffered packages. `ci/verify-timer-input-data.py` checks source hashes and the typed PAC input-field data; it does not execute or simulate the HAL. No HAL tests, harnesses, mocks, flashing or silicon validation were performed.

Not implemented here: classic GTIM/ATIM input capture/encoder, internal triggers, timer cascades, external clock input, encoder index/error interrupts, lossless streaming, async waits, capture DMA, PWM-input reset pairing, arbitrary linear divisors, complementary PWM, protection or RF work. The data already contains all required typed fields, so this batch has no generated PAC or schema changes.
