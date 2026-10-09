# External-input comparator polling

The `comparator` module provides a pin-owning `Comparator<'d, T>` on all 13 verified families. It follows the pinned upstream Embassy `comp.rs` peripheral/pin ownership boundary, with CW32 register semantics sourced independently. The HAL is one ordinary flat module. Generated instance and pin traits use explicit hardware mux metadata rather than treating a source signal number as an encoding.

## Exact scope

- CW32F030/A030/F020/F002/F003/L031/R031/W031/L052/L083: VC1 and VC2, external positive/negative inputs, four response modes, off/low/medium/high hysteresis, polarity, one-read polling with hardware READY qualification.
- CW32L010/L011: VC1 and VC2, external positive CH0–3 and negative CH0–1, two response modes, hysteresis enable/disable, polarity, one-read polling with unknown startup readiness.
- CW32L012: the same low-family subset on VC1–VC4.
- Each part exposes only its bonded, ordinary analog-capable GPIO routes. Package-neutral aliases expose the common physical-package intersection. R031 VC1 source CH0–3 selects mux 4–7. The mux is unrelated to a GPIO digital alternate-function number.
- L010/L011/L012 additionally accept immutable, bank-qualified shared dividers through `new_with_vref`; see [shared references](comparator-reference.md). Fixed BGR/DAC input routes, output-pin routing, digital filtering, blanking, window comparisons, timer connections, interrupts, async operation and DMA remain outside this subset. Radio peripherals remain out of scope; R031/W031 comparator support is independent of RF.

## Use

Initialize the HAL, then pass one comparator token and its two input-pin tokens to `Comparator::new`. For F030, VC1 PA0 positive and PA1 negative are one valid pair. Set `Config::supply_mv` to the board's analog-supply declaration, with `Config::default()` for high response speed, no hysteresis and non-inverted output. `new` returns `Result` and does not wait for analog startup. Keep the comparator alive while the inputs are in use.

`output()` reads SR exactly once. Its `Output` contains `Level` and `Readiness`. Classic IP returns the contemporaneous READY flag as Ready/Starting. L010/L011/L012 always return Unknown because their manuals have no readiness bit and their 0.5 µs startup specification is typical only. Applications must qualify their own startup settling on these parts. A recently changed input still needs analog propagation/overdrive settling even if READY is set. The driver does not establish READY deassertion latency when disabling/reconfiguring a previously active comparator.

The source-backed `supply_voltage_range_mv()` and `input_uses_vdda()` functions report selected-family board requirements. Configuration validates the declared voltage before any register/pin writes; it cannot measure the board or establish input/common-mode compliance. Inputs must stay between analog ground and VDD (F002/F003/L010) or VDDA (other families). Where VDDA exists, VDDA = VDD is required by the own-family general conditions. W031 RF DCDC operation separately requires at least 2.0 V. Package temperature limits, offset, overdrive and whole-device limits still apply.

Response speed names express a power/speed trade-off. Hysteresis voltages are typical, not exact thresholds. Classic datasheet maxima are characterized, not production-tested. The low-family datasheets specify typical hysteresis 20/30/26 mV for L010/L011/L012, while some manual/SDK descriptions differ; the API intentionally uses Enabled rather than a numerical threshold. Manuals' approximate classic response prose also differs from newer datasheet tables, so no exact time is promised.

## Register ownership and side effects

The constructor enables the shared VC gate through generated `RCC_INFO`, which inserts KEY=0x5A5A for low-family APBEN1 writes and preserves unrelated gate bits. It verifies the gate and rejects an already asserted shared reset without releasing it. It never resets a comparator group. Once enabled the shared gate is retained, including on constructor reset rejection and on drop.

Only the owned instance's CR0/CR1 (plus L012 CR2) are configured. CR0 is written disabled before analog pin configuration, and then written with typed INP/INN setters and RESP/HYS/POL enums. Interrupt/filter/window/blanking/timer fields remain disabled. CR1 explicitly selects PCLK; this avoids the low-family implicit LSI request associated with FLTCLK=LSI. No ADC, BGR, DIV, VCREF, DAC, sibling instance, NVIC, or pin-output AF register is written. L012 comparator enable/high-speed hardware can automatically start the shared BGR; the driver neither disables it nor promises that external-input low-speed operation cannot affect it. Drop clears only the owned CR0.EN, then releases its owned pins and any reference borrow.

SR.FLTV and classic SR.READY are read-only status fields. SR.INTF is write-zero-to-clear. The HAL does not write SR or acknowledge any interrupt. PAC SR `write_noop` is explicitly 1 so an unrelated command write does not clear INTF; reset default remains zero. This is a command seed, not a status value to write from read-modify-write.

## Evidence and verification

- `cw32-data/af/*-comparator.yaml`: all 13 own-source route profiles with pin/mux table printed/PDF pages, source hashes/URLs, canonical physical-pinout hash, exact package positions, supply limits and frozen review-policy hashes.
- `docs/comparator-classic-evidence.json`: nine own manuals covering ten families, own datasheet electrical/clock/control/startup/status references and qualifications. F020 uses `current-datasheets/CW32F020_DataSheet_CN_V1.3.pdf`, the actual printed Rev 1.3, not the older same-named root artifact.
- `docs/comparator-low-evidence.json`: L010/L011/L012 own manuals, datasheets and SDK member hashes; route, keyed clock, register, startup, shared analog and source-conflict analysis.
- `tests/verify_comparator_evidence.py`: source/data/PAC audit only. It verifies canonical source fingerprints, actual manual pin/mux cells, physical-package projections, electrical limits, typed control layouts and R1W0 seeds. It is not a HAL test/harness and never executes firmware.
- `docs/comparator-coverage.json`: exact feature counts and subset ledger; production ARM compilation is recorded separately in the handoff manifest.

Builds and source review are not silicon validation. No firmware was flashed or executed and no external push was performed.
