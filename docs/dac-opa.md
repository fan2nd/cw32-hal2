# CW32L012 analog output

Source-qualified software support, not hardware validation. The driver uses direct typed PAC access, central `RCC_INFO`, generated instances and package-qualified pins. Both exact L012 parts and its common-package alias are supported. Pinned Embassy revision `f16efeffe37581092ec184718e6fdb1620393214` supplies the reference ownership pattern; no STM32 register assumptions are imported.

## DAC

`dac::Dac::new` owns the whole DAC and both PB0/PB1 outputs; `new_ch1` / `new_ch2` own the whole DAC and only their selected output. Both channels are first disabled and DHR initialized to zero. `set(Channel, Value)` supports checked 12-bit right-aligned and 8-bit samples; direct loading transfers to DOR in one dac_pclk cycle. No-trigger mode needs only its independent PCLK gate, not ADC clocks/configuration. `read` returns digital DOR, never voltage or analog readiness. Drop disconnects both DAC output switches then disables its channels, leaving clock gates enabled.

VDDA is supply and reference, equals VDD, and must be 1.7–5.5 V. Reference selection is not exposed. This is an unbuffered output: datasheet Table 7-30 characterizes typical output resistance 4180 Ω, at least 5 kΩ resistive load and at most 50 pF capacitive load under specified 3.3 V conditions. Full-scale output does not promise the rail. Startup 3 µs is typical, not a maximum. Board loads, tolerances, signal changes and temperature need independent settling qualification.

## OPA

`opamp::OpAmp::new` validates declared VDDA, enables the shared OPA configuration clock and additively enables BGR while preserving TSEN, then leaves only its own amplifier disabled. The application must establish BGR stability before `buffer_ext`, `pga_ext` or `standalone_ext`. The first two modes own exactly one positive input and output, with every external negative switch disabled. Standalone also owns a negative input and requires an external stable feedback network. The scoped `OpAmpOutput` owns these pins and borrows the controller; dropping it disables only its amplifier. Gain choices are 2, 4, 8, 16, 32; follower is unity. BIAS is explicitly 8 µA. No driver operation resets a shared block or disables BGR.

Input must stay within 0–VDDA, linear output within 0.1–(VDDA−0.1) V, load at least 8 kΩ for less than 1% deviation, and output current at most 6 mA under the source's stated conditions. VDDA=VDD must be 1.7–5.5 V. Datasheet 2.5 µs startup is a **minimum**, not a maximum; BGR ~30 µs is approximate. No hardware-ready flag exists. These APIs deliberately promise neither calibrated accuracy nor settled output at return.

## Ownership and inherited state

PB0 is DAC1/OPA1 output; PB1 is DAC2/OPA2 output. All participating pins remain exclusively owned. Before touching an output pin, the DAC enables the actual OPA gate without reset, inspects the matching OPA EN and rejects an active peer. Before enabling an OPA output, its constructor has enabled the DAC gate, and the mode method rejects matching DAC EN+CxOUT. Clock-disabled does not mean analog-disabled. Counterpart gates stay enabled: no untracked owner is disrupted by reset or gate removal. Held resets are reported and never released. Invalid supply/sample data are rejected before associated writes.

`Dac` owns both internal channels. This initial slice therefore offers no safe internal DAC/OPA/ADC/VC sharing. Independent DAC channel splitting would need a separate whole-peripheral ownership model.

## Precisely deferred

- DAC timer/software triggers, DMA/interrupts, generated noise/triangle waveforms, independent channel splitting, internal output-only paths, selectable reference, and output calibration.
- OPA auto/level calibration, asynchronous triggered calibration, adjustable bias or low-power control, internal DAC input and ADC output integration.
- Readiness/settling maxima, guaranteed analog voltage from code, measured offset/gain, and electrical validation.

No guessed LPMODE exists: datasheet mentions it but the current manual/header/SVD do not define it. Current CN manual/SDK reset 0xE000 supersedes older EN 0x0E00; code explicitly writes BIAS=7. Calibration period terminology differs (manual 8×2^n cycles and AZRUN twice period; SDK 16×2^n labels), so the API does not conflate them. SDK claims about OPA-to-ADC channel12/13 are not used because the manual identifies those as DAC channels. DAC Figure26-1's ADCCLK label is recorded as inconsistent with the clock tree and explicit dac_pclk text.

## Evidence and verification

See [own-source review](dac-opa-source-review.md), [artifact/member hashes](dac-opa-evidence.json), and authored `cw32-data/dac-opa.yaml`. Raw vendor PDFs, SDK code, and rendered source pages are not distributed. PAC semantic enums precede HAL use; DAC DOR mixed RO/W0C semantics have independent write-no-op seeds. HAL never writes DOR, SWTRGR or calibration trigger commands. Register default values are not misrepresented as hardware reset values.

Normal ARM build receipts and real linked firmware examples are delivered separately. No HAL unit tests, runtime harnesses, flashing or silicon verification are included.
