# Buffered timer input qualification

Read-only qualification from the locally acquired official sources, 2026-10-08. No HAL tests, hardware harnesses, mocks, or silicon validation. Every family below was checked against its own manual; matching semantics are a finding, not an assumed cross-family alias.

## Source identity

### CW32L010

Manual: [CW32L010_UserManual_CN_V1.2.pdf](https://www.whxy.com/uploads/files/20260626/CW32L010_UserManual_CN_V1.2.pdf)
- SHA256: `b66ae2b2837cf22aede7f19312b82659ea10f96960bfe7965de8733bb72513fa`
- Version: 1.2; local bytes match source catalog.
Datasheet: [CW32L010_DataSheet_CN_V1.3.pdf](https://www.whxy.com/uploads/files/20260416/CW32L010_DataSheet_CN_V1.3.pdf)
- SHA256: `6fbefd334a86a1fafbec8ead5b6bd44dc99dfe564b604f69ec1edaa4ec789b86`
SDK: [CW32L010_StandardPeripheralLib_V1.0.9.zip](https://www.whxy.com/uploads/files/20260806/CW32L010_StandardPeripheralLib_V1.0.9.zip)
- SHA256: `84dbbeb8b684d0435ef9f926df8b899ceeb1d7bfd7767145b1e4f7e22210c716`

### CW32L011

Manual: [CW32L011_UserManual_CN_V1.1.pdf](https://www.whxy.com/uploads/files/20260602/CW32L011_UserManual_CN_V1.1.pdf)
- SHA256: `b245887e9037caf267f739e25bd3c354c2579d23315fc06389e055f371159f4f`
- Version: 1.1; local bytes match source catalog.
Datasheet: [CW32L011_DataSheet_CN_V1.1.pdf](https://www.whxy.com/uploads/files/20250724/CW32L011_DataSheet_CN_V1.1.pdf)
- SHA256: `0b7414049824881920fc38f829029e3ba0af88feb4536fb27351df0d60f688a5`
SDK: [CW32L011_StandardPeripheralLib_V1.0.3.zip](https://www.whxy.com/uploads/files/20251016/CW32L011_StandardPeripheralLib_V1.0.3.zip)
- SHA256: `76adfe39360eb1d05ef58c25f26a8c1f99f2bc2f8fef677214aaca85cffc679e`

### CW32L012

Manual: [CW32L012_UserManual_CN_V1.4.pdf](https://www.whxy.com/uploads/files/20260603/CW32L012_UserManual_CN_V1.4.pdf)
- SHA256: `a9e54694a26f03c1f3e3041f40844900168ca2b8e6142f3328671b92e6b7a340`
- Version: 1.4; local bytes match source catalog.
Datasheet: [CW32L012_DataSheet_CN_V1.0.pdf](https://www.whxy.com/uploads/files/20250717/CW32L012_DataSheet_CN_V1.0.pdf)
- SHA256: `08a605fe8843d1eefa2b1115474b5a74df2833068f0ad6f35225e4f39377da76`
SDK: [CW32L012_StandardPeripheralLib_V1.0.5.zip](https://www.whxy.com/uploads/files/20260701/CW32L012_StandardPeripheralLib_V1.0.5.zip)
- SHA256: `8b0a4c0ee865642d08f353aec98906c900a8b10f672120e649e6b1bf5e29c18f`

## Verified register behavior

- Direct external input: clear CR2.TI1S (avoid XOR), set TISEL/TISEL1 TIxSEL=0, and CCxS=01 for x=1..4. CH1/2 use CCMR1CAP; CH3/4 use CCMR2CAP. Complementary CHxN is not a capture input substitute.
- CCER NP/P: 00 rising, 01 falling, 11 both; 10 reserved. Capture requires CCxE=1. CCxS is writable only while the channel is disabled. CCxE=0 immediately resets that channel’s input prescaler.
- ICxPSC encodings 0,1,2,3 capture every 1,2,4,8 events. ICxF encoding 0 means no filter, sampled at fDTS. Codes 1,2,3 sample at fPCLK with N=2,4,8. Codes 4..15 are fDTS/2:N6,N8; /4:N6,N8; /8:N6,N8; /16:N5,N6,N8; /32:N5,N6,N8. CR1.CKD=0,1,2 gives fDTS=fPCLK/1,/2,/4. Do not use undocumented CKD=3.
- Capture latches latest CNT into CCRx and sets IF. A second capture while IF is already set also sets OF. A CCRx read clears IF in input-capture mode; it does not clear OF. ICR is R1W0: zero clears, one preserves. CH1..4 IF bits are 1..4, OF bits are 9..12.
- Encoder SMS=1 counts both TI1FP1 edges (x2), SMS=2 both TI2FP2 edges (x2), SMS=3 both channels (x4). CC1NP/CC2NP must stay zero; P controls inversion. Both inputs remain required even in x2 mode because the other input supplies direction.
- Encoder DIR is hardware-maintained on transitions of either input, including transitions not counted in a selected x2 mode. For TI1: when TI2 is low, rising counts up and falling down; TI2 high reverses that. For TI2: when TI1 high, rising counts up and falling down; TI1 low reverses that. CNT is 16-bit and wraps between zero and ARR.

## Exact manual anchors

Pages below are printed pages; JSON also includes one-based PDF pages and text line anchors. L010/L011 PDF page = printed+1; L012 CN V1.4 PDF page = printed+26.

| Family / bank | Capture | Encoder | CCMR1 / CCMR2 | CCER | ISR / ICR | SMCR | TISEL | Examples encoder / capture |
|---|---|---|---|---|---|---|---|---|
| CW32L010 GTIM | §13.3.3 p231 | §13.3.2.8 p226 | §13.9.8 p269 / §13.9.10 p273 | §13.9.12 p275 | §13.9.5 p265 / §13.9.6 p267 | §13.9.3 p260 | §13.9.21 p280 | §13.7.6 p252 / §13.7.7.1 p253 |
| CW32L010 ATIM | §14.3.3 p308 | §14.3.2.8 p303 | §14.9.8 p356 / §14.9.10 p361 | §14.9.14 p365 | §14.9.5 p351 / §14.9.6 p353 | §14.9.3 p345 | §14.9.28 p376 | §14.7.6 p336 / §14.7.7.1 p337 |
| CW32L011 GTIM | §13.3.3 p231 | §13.3.2.8 p226 | §13.9.8 p269 / §13.9.10 p273 | §13.9.12 p275 | §13.9.5 p265 / §13.9.6 p267 | §13.9.3 p260 | §13.9.21 p280 | §13.7.6 p252 / §13.7.7.1 p253 |
| CW32L011 ATIM | §14.3.3 p309 | §14.3.2.8 p304 | §14.9.8 p357 / §14.9.10 p362 | §14.9.14 p366 | §14.9.5 p352 / §14.9.6 p354 | §14.9.3 p346 | §14.9.28 p378 | §14.7.6 p337 / §14.7.7.1 p338 |
| CW32L012 GTIM | §16.3.3 p259 | §16.3.2.8 p255 | §16.10.8 p296 / §16.10.10 p300 | §16.10.12 p302 | §16.10.5 p292 / §16.10.6 p294 | §16.10.3 p287 | §16.10.21 p308 | §16.8.6 p280 / §16.8.7.1 p281 |
| CW32L012 ATIM | §17.3.3 p338 | §17.3.2.8 p334 | §17.10.8 p386 / §17.10.10 p390 | §17.10.14 p394 | §17.10.5 p380 / §17.10.6 p383 | §17.10.3 p374 | §17.10.28 p406 | §17.8.6 p365 / §17.8.7.1 p366 |

## Input rate and runtime limits

- Each own datasheet specifies fEXT ≤ fTIMCLK/2 (24 MHz only when fTIMCLK is actually 48 MHz). L010 §7.3.17 table 7-33 p53; L011 §7.3.17 table 7-32 p56; L012 §7.3.19 table 7-35 p66. These are generic timer-input electrical/design limits, not a software capture throughput guarantee. Filter settings lower accepted bandwidth; GPIO electrical input limits also apply.
- The manual PCLK/4 text belongs to ETR external clock mode 2. It is not a standalone CH capture/encoder rate specification.
- Each capture channel has one latest-value latch, not a FIFO. Overcapture detects overwrite but does not reconstruct lost samples. Reading CCR then explicitly clearing IF can discard a newer event. There is no documented atomic IF/OF/CCR snapshot or lossless high-rate service guarantee.
- Inferred safe initialization: own timer exclusively; mask interrupts/DMA, disable CEN/channels, reset old configuration, select AF input and direct source, write CCxS only while disabled, configure PSC/ARR/CNT, load buffered PSC via UG if needed and clear resulting flags before starting. Encoder should use PSC=0, input PSC=0, edge-aligned mode, SMSH=0, ECE=0, no index/reset extension, and explicit ARR/CNT.
- Inferred cancellation/drop safety: disable interrupt/DMA enables and capture channels, then stop CEN before releasing peripheral/pins. CEN=0 alone is not a documented capture-input disable; CCxE is the capture gate.
- SDK caveat: GTIM_EncodeInit is disabled under #if 0 and uses an older CR0/CMMR architecture in all three SDKs. Do not use it as a buffered-timer recipe. GTIM API polarity enums 1/2/3 are translated by a switch; ATIM enums are 0/1/3.

## Example routes for all seven exact packages

Each cell is GPIO / AF / physical package pin. Selected from qualified route files and checked against each exact package. A dash means no route qualified under the present oscillator ownership policy.

| Package | GTIM1 CH1 | GTIM1 CH2 | GTIM1 CH3 | GTIM1 CH4 | ATIM CH1 | ATIM CH2 | ATIM CH3 | ATIM CH4 |
|---|---|---|---|---|---|---|---|---|
| CW32L010F8P6 | PA6 / 6 / 16 | PA5 / 6 / 15 | PA4 / 6 / 14 | PB3 / 6 / 20 | PB4 / 7 / 1 | PB2 / 7 / 19 | PA3 / 7 / 13 | — |
| CW32L010F8U6 | PA6 / 6 / 13 | PA5 / 6 / 12 | PA4 / 6 / 11 | PB3 / 6 / 17 | PB4 / 7 / 18 | PB2 / 7 / 16 | PA3 / 7 / 10 | — |
| CW32L010Y8M6 | PA6 / 6 / 14 | PA5 / 6 / 13 | PA4 / 6 / 12 | PA3 / 6 / 11 | — | — | PA3 / 7 / 11 | — |
| CW32L011K8T6 | PA6 / 6 / 12 | PA7 / 6 / 13 | PB0 / 6 / 14 | PB1 / 6 / 15 | PA8 / 7 / 18 | PA9 / 7 / 19 | PA10 / 7 / 20 | PA11 / 7 / 21 |
| CW32L011K8U6 | PA6 / 6 / 12 | PA7 / 6 / 13 | PB0 / 6 / 14 | PB1 / 6 / 15 | PA8 / 7 / 18 | PA9 / 7 / 19 | PA10 / 7 / 20 | PA11 / 7 / 21 |
| CW32L012C8T6 | PA6 / 6 / 16 | PA7 / 6 / 17 | PB0 / 6 / 18 | PB1 / 6 / 19 | PA8 / 7 / 29 | PA9 / 7 / 30 | PA10 / 7 / 31 | PA11 / 7 / 32 |
| CW32L012C8U6 | PA6 / 6 / 16 | PA7 / 6 / 17 | PB0 / 6 / 18 | PB1 / 6 / 19 | PA8 / 7 / 29 | PA9 / 7 / 30 | PA10 / 7 / 31 | PA11 / 7 / 32 |

- CW32L010Y8M6 does not bond PB4 or PB2. Its only currently qualified ATIM input in CH1–4 is CH3 on PA3 pin11. It shares that pad with GTIM1 CH4, so those two particular routes cannot be used simultaneously.
- L010 F8P6/F8U6 examples above use GTIM1 CH4=PB3 to avoid overlap with ATIM CH3=PA3.
- L010 SDK and own AF table document PA0 AF7=ATIM_CH4; its OSC_IN alias is why it is excluded. PB0/PB1 ATIM_CH1/2 are excluded for OSC32 ownership. These are policy/ownership exclusions, not claims that the silicon has no channel.
- Full alternative routes for GTIM1/2/3/4, per-channel/per-package physical bonding, source macro lines, own datasheet/manual cells, source hashes, and selected-route evidence are in buffered-timer-input-qualification.json. Do not expand complementary or unqualified routes from raw SDK candidates.

## Evidence files

- buffered-timer-input-qualification.json: machine-readable source hashes, official URLs, precise section anchors, behavior values, all qualified package routes, and clearly labeled implementation inferences.
- Only this evidence directory was written; the main and timer-input checkouts were not modified.
