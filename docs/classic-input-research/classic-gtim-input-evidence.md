# Classic GTIM capture and QEI source qualification

All listed families document four capture channels per GTIM and quadrature counting on CH1/CH2. No true missing encoder field was found in the own manuals or selected own SDK SVDs. This report is documentary evidence, not hardware validation. The implementation is bounded to capture and QEI using already qualified pin/AF routes; external ETR counting remains research-only.

| Family | Own manual | Instances | Capture | QEI | ARR rule |
|---|---|---|---|---|---|
| CW32F002 | Rev 1.4 | GTIM | §12.3.3.1 p161 | §12.3.5 p166 | §12.7.5 p180: 0xffff |
| CW32F003 | Rev 2.3 | GTIM | §12.3.3.1 p163 | §12.3.5 p168 | §12.7.5 p182: 0xffff |
| CW32F020 | Rev 1.4 | GTIM1, GTIM2, GTIM3, GTIM4 | §14.3.3.1 p224 | §14.3.5 p229 | §14.8.5 p244: 0xffff |
| CW32F030 | Rev 2.5 | GTIM1, GTIM2, GTIM3, GTIM4 | §14.3.3.1 p227 | §14.3.5 p232 | §14.8.5 p247: 0xffff |
| CW32A030 | Rev 2.5 | GTIM1, GTIM2, GTIM3, GTIM4 | §14.3.3.1 p227 | §14.3.5 p232 | §14.8.5 p247: 0xffff |
| CW32L031 | Rev 1.6 | GTIM1, GTIM2 | §14.3.3.1 p219 | §14.3.5 p224 | §14.7.6 p237: 0xffff |
| CW32L052 | Rev 1.5 | GTIM1, GTIM2, GTIM3 | §15.3.3.1 p253 | §15.3.5 p258 | §15.8.6 p274: 0xffff |
| CW32L083 | Rev 2.0 | GTIM1, GTIM2, GTIM3, GTIM4 | §15.3.3.1 p265 | §15.3.5 p270 | §15.8.6 p286: 0xffff |
| CW32R031 | Rev 1.3 | GTIM1, GTIM2 | §14.3.3.1 p221 | §14.3.5 p226 | §14.7.6 p239: 0xffff |
| CW32W031 | Rev 1.4 | GTIM1, GTIM2 | §14.3.3.1 p221 | §14.3.5 p226 | §14.7.6 p239: 0xffff |

Pages above are printed page numbers; PDF page numbers are one greater. The JSON contains exact official URLs, PDF SHA-256, SDK archive SHA-256, per-file SDK/header/SVD hashes, function line numbers, register sections, route references, and current datasheet source hashes. All cited manual PDF hashes match the locally acquired PDFs. Per-family fact_evidence entries also retain exact source-text line anchors for ARR, read-then-clear, mux0, per-edge QEI semantics, and the reset-state programming sequence.

## Common protocol

- Capture modes are CMMR nibble values 1 rising, 2 falling, 3 both. CH1–4 status is ISR bits3–6. Read CCR, then acknowledge explicitly through ICR; CCR read-clear is not documented. Every manual programming example and SDK separates these operations. No overcapture flag is documented, so polling cannot claim lossless capture or overwritten-event detection.
- ICR flags are write-zero-to-clear; one has no effect. OV bit0 and UD bit2 are distinct. DIR bit10 is read-only direction (0 up, 1 down); DIRCHANGE bit9 is separately clearable.
- All families require ARR=0xffff for QEI. ENCMODE bits16:15 select CH1 edges (1), CH2 edges (2), or both channels (3). CH3 index reset/reload is documented but outside the initial subset.
- QEI sections describe counting on every qualified CH1/CH2 transition. Every own manual encoder sequence and SDK encoder initializer leaves PRS/PSC untouched. Using reset PSC=0 / PRS=0 is consistent with those sequences; no QEI prescaler option is documented. Treat “bypass” as an inference, not an explicit quoted guarantee. Neither sequence requires CMMR capture mode.
- Explicitly select SYSCTRL_GTIMCAP/GTIMxCAP channel mux value 0 for external GPIO AF input. CH fields are three bits at shifts0/4/8/12. Existing channel pin/AF identities apply to input capture as well as PWM, while actual GPIO mode must become alternate-function input.

## Filter and family differences

- All own manuals use the same three-bit filter encoding: 0 disabled; 1/2/3 PCLK with 2/4/6 equal samples; 4/5 PCLK/4 with 4/6 samples; 6/7 PCLK/8 with 4/6 samples. Each channel CR1 nibble has filter bits2:0 and inversion bit3; ETR filter occupies bits6:4. Manuals warn inputs above half the actual sampling frequency can be missed.
- F002/F003/F020/F030/A030 use CR0.PRS power-of-two prescaling. L031/L052/L083/R031/W031 use separate PSC. For future ETR-counter mode specifically, unprescaled TRS needs PSC=1, while PCLK timer mode uses PSC=0. Do not transfer that external-counter rule to QEI.
- L031/R031/W031 manuals use base0x40000700 plus register offset0, whereas SDK/SVD uses base0x40000400 plus offset0x300. Absolute addresses agree.
- L052 adds TI1XOR and IC1RST/IC2RST/IC3RST; keep these reset for independent-channel capture and QEI. Its selected SVD calls CR0 bit4 POLARITY rather than POL.
- Internal capture and ETR source encodings differ significantly on L052/L083 and sometimes by channel or timer. External source0 is the shared subset. Refer to the JSON for the internal-source differences.

## Sources

The adjacent [machine-readable evidence](classic-gtim-input-evidence.json) identifies each primary source and exact references. Raw manuals, SVDs, and vendor drivers remain outside this repository.

## Original PDF visual cross-check

Original F002 Rev 1.4 PDF pages 181 and 183 (printed 180 and 182), and L052 Rev 1.5 PDF pages 275 and 278 (printed 274 and 277), were rendered and visually inspected. Both ARR notes visibly require 0xffff in encoder mode. Both full ISR tables show distinct OV/UD flags and no overcapture flag, agreeing with the extracted text. Temporary renders were not bundled.
