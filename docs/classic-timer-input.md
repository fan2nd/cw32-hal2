# Classic GTIM external input capture and QEI

This adds polling `timer::input_capture::InputCapture` and `timer::qei::Qei` for the source-qualified classic GTIM instances on CW32A030, F002, F003, F020, F030, L031, L052, L083, R031 and W031. The same owners continue to support buffered L010/L011/L012 GTIM and ATIM. There are no per-family driver copies or register casts.

The pinned upstream is Embassy `f16efeffe37581092ec184718e6fdb1620393214`. Module placement, `CaptureInput::from_pin`, channel methods, `Qei`, `Config`, `QeiMode`, `Direction`, `count`, `reset` and `read_direction` follow its timer API organization. CW32-specific limits remain explicit: four direct GTIM input channels, 16-bit counters, a three-bit classic filter, and no classic capture prescaler or overcapture API. The additional `CapturePin` capability prevents output-only pin qualification from silently authorizing input routes.

## Ownership and clock configuration

Both drivers retain the exclusive timer token through the existing low-level `Timer`, and own every connected `Flex` input. All construction uses the existing generated `RCC_INFO` gate/reset path and qualified `ClockBounds`. The per-timer SYSCTRL capture mux is explicitly set to external source zero. GPIO connects in digital alternate-function input mode, so output drivers stay disabled. Pin traits come from generated `CAP1`–`CAP4` metadata, cross-checked against existing own-family pin/AF evidence and actual package bonding.

Reserved Embassy-time GTIM tokens, timer capabilities and input pin implementations are excluded by the existing generator boundary. Reusing a reserved timer is not offered. Both drivers stop peripheral activity and disconnect pins before the inner timer drops its gate. No interrupt, DMA, async capture, or low-power timing promise is added.

## Capture semantics and latency

Construction starts an up-counter at zero with ARR=65535 and all capture channels disabled. The requested frequency is a maximum tick rate: the qualified PCLK upper bound selects a representable power-of-two divider. A zero/unrepresentable rate fails before timer configuration. Enable each owned capture explicitly.

Classic CMMR values 1, 2 and 3 capture rising, falling and both edges. `get_capture_value` reads the latest 16-bit latch and does not acknowledge its status. `clear_input_interrupt` performs an explicit R1W0 command from the generated `Icr::write_noop` seed, retaining unrelated flags and reserved reset-one bits. Buffered IP keeps its existing CCR read-clear and overcapture behavior.

There is one latest-value latch per channel and no documented classic overcapture flag, FIFO, capture event prescaler, or lossless delivery guarantee. Captures can overwrite each other without detection. Reading then clearing while acquisition continues can acknowledge an edge that arrived between those operations. The firmware example intentionally disables that channel before reading and clearing, then re-enables it; this creates an acquisition gap. Mode/filter changes also briefly disable the channel. No bounded software service latency or maximum lossless event rate is claimed.

The filters require consecutive equal samples from PCLK or PCLK/4 or PCLK/8, independently of CNT prescaling. Filter encodings come from each own manual, and filtered transitions are delayed. External signals must satisfy the selected device's voltage, pulse-width, sampling and timing limits; the manual's half-sampling-frequency warning is not a guarantee that a particular application can service every capture. Unsigned timestamp subtraction is meaningful only when the application knows fewer than 65536 ticks elapsed. Overflow flags coalesce and cannot reconstruct missed wraps.

## QEI semantics and limitations

Both external phases are required for every mode. Mode1 counts both CH1 edges, Mode2 both CH2 edges, and Mode3 both channels; direction comes from their phase relationship. All classic own manuals require ARR=0xffff in encoder mode. Generated per-instance metadata enforces this before configuration; custom classic reloads are rejected. Buffered devices retain their existing permitted reload range.

Initialization retains non-inverted inputs, keeps CH3 index reset/reload and L052 XOR/capture-reset controls disabled, and leaves CMMR capture modes disabled. The start command modifies only EN, preserving ENCMODE. PRS=0 or PSC=0 follows each manual's reset-state encoder sequence. The manuals describe counting on each qualified transition and omit prescaler programming from this sequence; no programmable QEI prescaling or explicit hardware bypass guarantee is claimed.

Classic overflow (ARR→0) and underflow (0→ARR) are distinct coalescing flags, exposed and acknowledged separately. `count`, direction and wrap flags are independent, non-atomic observations. They do not establish a signed unbounded position or the ordering/number of missed wraps. `reset` writes zero while decoding continues; concurrent edges are not synchronized to that write. Input filtering does not recover skipped or illegal quadrature transitions and does not guarantee mechanical debounce.

## Remaining hardware and scope

The classic manuals also document ETR external counting and CH3 index functions. Those require their own routing and ownership qualification and are not exposed by this patch. Separate-PSC devices use PSC rather than PSC+1 for external TRS division, which must not be inferred from timer-clock or QEI setup. Classic ATIM uses paired A/B input channels and remains outside this API; it is not represented as four STM32-style capture channels. RF remains deferred.

## Evidence and verification

[The source review](classic-input-research/classic-gtim-input-evidence.md) and its adjacent JSON contain official URLs, printed revisions, exact sections/pages, original document hashes, SDK function references and explicit inferences. The authored pin catalog is `cw32-data/classic-timer-input.yaml`. No raw manuals, SDK sources or rendered vendor tables are bundled with the patch.

`ci/verify-classic-timer-input-data.py` audits original source hashes, instance facts and generated input routes. `ci/check-classic-timer-input.py` performs ordinary ARM library builds and real external-input firmware links. These do not run the HAL on hardware, emulate registers, use a harness, or establish electrical/silicon behavior. Build receipts, source hashes, ELF hashes and source-review findings accompany the handoff.
