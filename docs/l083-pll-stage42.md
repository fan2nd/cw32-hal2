# L083 HSI-fed PLL on the Stage42 source baseline

This isolated source candidate combines delivered Stage42 with the accepted CW32L083 factory-HSI-fed PLL addition. It preserves Stage40 optional HSE ranges and L010/L011 controls, Stage41 L012 inherited-HSE rejection before writes, and Stage42 common inherited-LSE capture, GPIO guards, metadata and source protection. It does not configure LSE or expand PLL qualification to another source or family.

## Exact combination and review

The five accepted PLL HAL source bodies, including rational bounds, classic ADC rejection, LPTIM and the time driver, are unchanged. The shared RCC runtime is exact Stage42; its changes are module documentation only. The build script adds only the metadata-derived PLL cfg, accepted PLL projection function and call. The three data models append PLL facts while retaining current LSE and optional HSE fields. Exact inverse checks reproduce both delivered Stage42 and the old accepted PLL model, with the shared base also reproduced.

The current [combined schema review](pll-stage42-schema-review.json) and [identity verification](pll-stage42-identity-note.json) own the new serde identity. The complete Stage42 review object remains its immediate previous review. Adapted metapac ancestry and the existing legal caveat remain unchanged. The independent ClockBounds/consumer review separately accepted all frozen runtime hashes.

## Capability boundary

[The PLL contract](l083-hsi-pll.md) remains limited to the 12 conservative pairs HSI /6 ×2,4,5,7 and /10 ×3,4,6,7,8,9,11,12. This policy checks entire actual envelopes against one input and output analog bin each, the independent 12–64 MHz qualified raw-output range and voltage-dependent bus limits. It does not claim the hardware's other documented configurations are absent or impossible.

PLL bounds qualify rates; the documented 300 ps cycle-to-cycle jitter does not create an absolute period or phase bound. The marker survives all propagation and division. Classic ADC strict acquisition/conversion timing rejects the new rate-only PLL before ADC-register or ADC-RCC writes. Earlier pin/channel preparation can separately touch GPIO. Existing HSI/HSE ADC APIs and timing behavior remain unchanged. Runtime retuning, HSE-fed PLL, low-power restoration and fault recovery remain unqualified.

Classic I2C remains a rate/BRR configuration API. Own L083 DS Rev1.9 §7.3.20 PDF69/printed68 explicitly guarantees correctly configured I2C timing by design; own RM Rev2.0 PDF440–441 supplies the BRR formula/range and filter controls. SPI uses the documented /4…128 rates and relative HalfPeriod sampling; its DS PDF70/printed69 characterized waveform and external-device input conditions do not become additional HAL nanosecond promises. Neither driver converts the PLL rate envelope into per-pulse high/low or setup-duration bounds. Their published frequency intervals remain source-qualified divided rates, while board loading and connected-device constraints remain applicable. The separate waveform-solving LPI2C backend is L012-only and cannot receive this PLL configuration. The [independent serial addendum](pll-stage42-serial-addendum.json) closes this classification at unchanged runtime hashes.

## Current verification

Current combined generation and 31 ordinary ARM library builds passed: ten L052/L083 family/package selections in both rt and rt+defmt modes, plus eleven other-family representatives. Twenty actual ELF links passed: ten PLL examples, eight existing L052/L083/L010/L011 HSE crystal/bypass examples and two L010/L012 retained-calendar examples. All images were inspected for exact-package SP and reset vectors, Thumb executable code, every declared IRQ vector, nonempty strong entry functions and memory/load extents. PLL time images also have a strong GTIM1 handler.

Own L083 raw manuals/datasheets, the 12-pair rational envelope calculation, all 54 generated clock records and the shared-runtime inverse were checked. Only six L083 profiles contain PLL facts. Removing them reproduces all earlier Stage42 clock limits, including HSE and LSE. Official locked source bytes and current distribution checks were verified. [The current source/build receipt](l083-pll-stage42-source-receipt.json) records exact inputs, commands and hashes.

The existing metadata-contract suite also passes all 22 checks. A reviewed correction to `tests/reviewed_metadata.py` extends the exact package oscillator expectation with OSC32/LSE roles only when own LSE metadata is present. It preserves equality, bonded-pad intersection and evidence gating; the original Stage42 helper omitted the new inherited LSE roles. This correction changes no hardware fact or runtime body and adds no HAL test or runtime harness.

The older `l083-hsi-pll-source-receipt.json` and `pll-schema-review.json` are historical Stage39-candidate evidence. Their 31-library/14-ELF counts and data/PAC checks are not current combined results. No HAL tests, runtime models, adapters, mocks, negative harnesses, silicon execution or board timing qualification were added or run for this rebase.

Delivery remains source-only. Generated data/PAC/coverage/build products, retained ELFs and raw manufacturer PDF/SDK/SVD files are excluded. The existing eleven approved headers and licensing boundary are retained; no independent legal clearance is claimed.
