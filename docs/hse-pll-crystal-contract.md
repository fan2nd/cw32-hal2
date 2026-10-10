# Independent review: crystal HSE feeding SYSCLK PLL

2026-10-09. Read-only source/design review; no implementation, Cargo or HAL execution. The reviewed design has SHA-256 `42a505b20efe137855b4e5a581d1be9908866d859199e1a1d3c2046cb5b306dd`. Neither it nor the candidate was edited. `receipt.json` binds the exact inputs, independently rehashed original PDFs and text, page reads and visual checks.

## Verdict

**YES, with a limited functional contract, for CW32F020/F030/A030/L083.** Admit the vendor-documented crystal-HSE to PLL composition for one-time SYSCLK initialization when the existing HSE board contract, declared operating envelope and all PLL rate/configuration checks hold. Retain the PLL result as rate-only. There is sufficient positive vendor evidence for that scope; independently measuring the inaccessible internal reference is not a necessary admission gate.

This accepts the vendor's intended composition of its oscillator and PLL as an engineering premise. It does **not** derive a universal numeric duty guarantee from the startup prose, certify any unspecified board, or waive the PLL datasheet's 40–60% input condition. If the proposed API instead promised independent proof of hidden-node duty across all conditions, the supplied sources would be insufficient. That stronger promise is unnecessary for the functional mode being reviewed.

The current design correctly labels its crystal rejection as a qualification policy, but its initial bypass-only recommendation is more restrictive than the documented functional use requires. Remove that rejection and its deferred-mode error. A single `PllSource::HSE`, selected using existing `Hse.mode`, remains the appropriate API. No separate crystal/bypass PLL source schema or caller-supplied duty certificate is justified.

## Evidence and limits

All locators are one-based PDF pages; printed pages are one less. F020 uses the selected current Rev1.3 datasheet under `current-datasheets`, never the historical same-name root file.

| Evidence | F020 | F030/A030 | L083 | Meaning |
|---|---:|---:|---:|---|
| RM §4.3.3 HSE circuit/mode/drive/range | 46–47 | 48–49 | 53–54 | Crystal uses both analog oscillator pads, mode 0, selected nominal range, adjusted load and drive. |
| RM §4.3.7 source table and analog settings | 52–53 | 54–55 | 59–60 | SOURCE=0 explicitly selects HSE oscillator; SOURCE=1 selects pin input; mode must correspond. |
| RM §4.4.1 startup and Figure 4-4 | 55 | 57 | 62 | Oscillator amplitude/duty become suitable for internal sampling before the counted stable interval. No numeric duty threshold is specified here. |
| RM §4.5.8 switching example | 66 | 68 | 73 | Programs SOURCE=0, input/output ranges and multiplier, WAITCYCLE=7, enables PLL, waits STABLE, configures Flash and selects PLL SYSCLK. |
| RM §4.4.3.3 running fault | 58 | 60 | 65 | Direct HSE/LSE fallback text does not establish HSE-reference-loss behavior while SYSCLK=PLL. |

The x030 manual explicitly covers F030 and A030. §4.5.8 is a documented programming example, not merely a list of possible mux values. It assumes an already running stable HSE because its example starts from HSE SYSCLK. Initialization may retain the safer existing HSI transition path while preserving the relevant dependencies: valid HSE before PLL enable, stopped PLL before parameter writes, PLL stable before switching and suitable Flash/bus settings. It need not briefly run SYSCLK from HSE to use this evidence.

The crystal datasheet discussion and table are F020 PDF42–43, F030 PDF44–45, A030 PDF41–42 and L083 PDF53–54. They describe the 4–32MHz resonator circuit, dependence on resonator characteristics and placement/load, and a typical 2ms startup, not a worst-case deadline. The cited current PLL tables are F020 PDF45/Table7-21, F030 PDF47/Table7-21, A030 PDF44/Table7-20 and L083 PDF56/Table7-21. All state input 4–24MHz and duty 40–60%; output maximum is 48MHz on F020 and 64MHz on the others. They do not state a standalone crystal-output duty specification. Therefore the justified inference is functional compatibility of the documented internal composition under its specified use, not a newly proved 40–60% measurement or waiver of that condition.

STABLE is the documented startup handshake, not an oscilloscope, continuous lock detector or test of the truth of board declarations. The schematic's drawn comparator waveform cannot be scaled to infer 50% duty, and neither an OSC pin nor HSE_OUT is established here as a calibrated observation of the PLL reference. Those absences limit claims; they do not overturn the vendor's positive crystal-PLL use instructions.

## Required scope and assumptions

- The board uses the selected part/package's bonded OSC_IN/OSC_OUT pads and a suitable resonator, load network, layout and drive. Its frequency declaration accounts for the conditions already required by `Hse`, including tolerance, load, temperature, aging and short-term variation. This review does not establish that declaration for any real board.
- The board's supply/ambient envelope fits both HSE and PLL qualification: VDD 1.65–5.5V, VDDA=VDD; existing conservative ambient limits −40…105°C for F020/F030/A030 and −40…85°C for L083. General conditions are F020 PDF36, F030 PDF38, A030 PDF35 and L083 PDF47. Retained HSI and all existing thermal/bus/Flash restrictions still apply.
- Full actual HSE bounds fit 4–24MHz and one existing PLL input bin. The checked multiplied bounds fit an output bin and the independent family raw ceiling, with the existing qualified output floor 12MHz. Multiplier remains 2–12. Bin-straddling rejection and the 12MHz floor are conservative software qualification limits, not assertions of absent hardware.
- Keep filter disabled, longest accepted HSE wait, PLL WAITCYCLE=7, matching mode/selector readback, HSE-before-PLL dependency and all existing ownership, pad, startup/fault, Flash and final-tree checks. The review does not broaden retained-owner or package admission.
- The oscillator and operating conditions must continue to meet their contract while frozen clocks are used. No guaranteed HSE-fed-PLL fallback, surviving CPU progress, runtime retuning, low-power restoration or post-loss rate is added.
- All multiplied and divided PLL clocks stay rate-only. No individual-cycle period/high/low duration, numeric PLL output duty or wall-time guarantee follows from a valid crystal, STABLE, the input duty condition or the cycle-to-cycle jitter entry. Existing strict ADC/complementary-PWM refusals remain.

## Exact replacement design text

The following replacements use the design's existing S1–S7 references. They are proposed wording only; the reviewed design remains unchanged.

### Replace the entire “Recommendation and the crystal evidence boundary” section

Extend the two existing PLL backends for one-time SYSCLK initialization using a board-qualified HSE reference in either oscillator or bypass mode, retaining all accepted factory-HSI behavior. Add one public `PllSource::HSE` choice, require the existing `Config.hse`, and derive the PAC selector from its mode. Admit `HseMode::Oscillator` through the vendor-documented crystal-to-PLL composition. No new source object, alternate initialization API, adapter layer, PLL output owner, duty certificate or test harness is needed.

The own manuals explicitly identify HSE oscillator clock at SOURCE=0 and give a §4.5.8 programming example that configures SOURCE=0, FREQIN/MUL/FREQOUT, longest PLL WAITCYCLE, enables PLL, waits STABLE, configures Flash and selects PLL SYSCLK [S1–S3]. Their startup discussion describes oscillator amplitude and duty becoming suitable for internal sampling before counted stability [S1–S3]. Together with the own crystal and PLL electrical specifications [S4–S7], this is sufficient source evidence to admit normal functional crystal-PLL operation under the existing HSE board contract and the bounded PLL conditions below. This design explicitly relies on the vendor's documented internal composition.

All four PLL datasheets specify 40–60% input duty [S4–S7]. This remains a device condition. The supplied sources do not independently quantify the internal crystal-reference duty across every board and operating point; this design does not claim they do. In oscillator mode, functional admission relies on a vendor-conforming oscillator implementation feeding the vendor-documented PLL path. It does not require callers to measure an inaccessible internal node or add an unsupported `crystal_duty_is_valid` assertion. In bypass mode, the board must satisfy the accessible OSC_IN waveform requirements, including 40–60% duty. STABLE provides the documented startup handshake and does not measure duty or prove individual-cycle timing. The PLL output remains rate-only in both modes.

This admission adds no HSE-reference-loss recovery guarantee while SYSCLK=PLL. Neither source mode establishes automatic fallback, a surviving CPU clock, continuous lock status, a post-fault PLL envelope, runtime retuning or low-power restoration. Frozen rate bounds apply only while the board/reference contract remains true.

### Replace the public API opening paragraph and HSE parameter bullet

Keep `Pll { src, mul }`, `Config`, `Hse`, and `Clocks` structurally unchanged. Add `HSE` beside `HSI` in the existing `PllSource` enums under `rcc_pll`. Document that it uses the undivided HSE declared in `Config.hse`, supports the existing oscillator and bypass modes under their respective board contracts, and provides rate-only PLL bounds. Preserve `HSI`, its configured divider, all existing default values and the nine F020 / twelve F030/A030/L083 accepted HSI pairs.

- HSE requires `config.hse` or returns existing `HseNotConfigured`. Use its original validated `Hse::bounds()`; do not construct bounds from nominal `Hertz`. Require the explicit generated HSE-PLL qualification. Derive PAC `HseCrystal` (0) from `HseMode::Oscillator` and PAC `HseBypass` (1) from `HseMode::Bypass`. Use the derived source consistently in writes and every matcher. No crystal-deferred qualification error is added.

### Replace the metadata paragraph

The smallest explicit metadata delta is `hse_supported: bool` on the existing PLL qualification record, true only for F020/F030/A030/L083. It admits both existing HSE modes for the bounded functional contract; it is not a measured internal-duty guarantee. Preserve `hsi_supported`. Carry the support fact through the existing authored profiles, PLL qualification, serde, metapac input/template/generated metadata and `build.rs::generate_pll`, and emit `RCC_PLL_HSE_SUPPORTED` under `rcc_pll`. Do not infer it from `rcc_hse` or register layout. Record the numeric PLL input-duty condition (40–60%) and the crystal mode's documented-composition basis in the source policy/receipt, separately from the existing bypass waveform obligations. A new runtime duty field is unnecessary: there is no new measurement or arithmetic use for it. Keep one HSE support fact rather than separate crystal and bypass schema branches, the four-family whitelist and exact selected own-source identities. Existing PAC encodings already express the mode difference.

### Replace the crystal board-obligation paragraph

Crystal PLL uses the existing direct-HSE board obligations for resonator suitability, nominal range, drive, load, placement and startup across the declared conditions. These obligations and the bounded PLL checks support the vendor-documented functional composition; bypass voltage/edge rules are not imposed on the analog crystal pins. Typical crystal startup of 2ms is not a maximum. Board declaration and startup readback do not independently certify internal reference duty or PLL individual-cycle timing. No extra caller assertion about the internal comparator node is required.

### Replace the broader-claims paragraph

Limits to broader claims remain: independently certified crystal-reference numeric duty; bin-crossing behavior and 8–12MHz analog settings; undocumented SOURCE2; absolute period/pulse/jitter guarantees; PLL-reference-loss behavior, runtime retuning and low-power restoration. None prevents the stated functional oscillator-or-bypass init-only design. No individual board has been electrically certified by this work.

### Exact API documentation for the new HSE variant

“Undivided HSE from `Config.hse`, in oscillator or bypass mode. The existing HSE board and operating-condition contract and all PLL input/output limits apply. Oscillator mode follows the vendor-documented crystal-to-PLL path; admission does not independently certify internal reference duty. Bypass requires the specified OSC_IN waveform, including 40–60% duty. PLL bounds describe rates only; individual-cycle timing and recovery after reference loss are not guaranteed.”

### Consistency edits accompanying those replacements

Change the source table's oscillator admission from “Documented hardware; deferred qualification” to “Vendor-documented functional path”. Use “Admitted bounded path” for bypass. Delete the extra deferred-mode error from the error paragraph. Replace “crystal deferral” in the implementation-check paragraph with “crystal and bypass admission with matching selectors and mode/pad checks”. Keep all other source-aware lifecycle, ownership, numerical admission and rate-only consumer requirements. Update the final design/source-policy receipt to identify this explicit acceptance of documented composition; do not present it as a newly obtained numeric duty specification.

## Source identity and verification

The independent receipt retains each original and text hash, official URL and exact pages. All seven PDF/text hash pairs match the qualification receipt. The source table, startup, crystal, operating-condition and PLL electrical text was extracted directly from those PDFs for this review. All three §4.5.8 pages and all three §4.4.1 pages were independently rendered and visually inspected; the four existing PLL electrical renders were also visually inspected against the directly extracted original values. No vendor asset is included in this authored-review directory.

This verdict is tied to the seven selected own sources. It is not new vendor correspondence, a broader errata audit, silicon testing or evidence that any board is qualified. The Stage54 candidate was only inspected for the existing `Hse`/`Pll` documentation and structure. Its implementation, generator parity and full lifecycle acceptance still require the separately authorized implementation review.
