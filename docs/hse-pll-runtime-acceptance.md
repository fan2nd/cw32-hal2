# Stage55 independent HSE-fed PLL runtime review

2026-10-09. Read-only review of the Stage55 candidate. No candidate edits, Cargo invocation, HAL tests/harnesses or silicon execution were performed by this reviewer.

## Runtime verdict

PASS for the exact V2 runtime/source/generated freeze identified below, with no runtime blocker. Eight library and eighteen ordinary ELF receipts have been checked. The requested stale PAC/source-comment descriptions are corrected. This verdict covers runtime/data integration and source/build correspondence; it is not whole-package acceptance. A subsequently surfaced inherited L011 canonical-ledger hash failure remains a separate data gate requiring a narrow correction and final-manifest correspondence supplement.

The accepted scope is the independent crystal functional review at `docs/hse-pll-crystal-contract.md`, overriding the earlier bypass-only design restriction. One `PllSource::HSE` reuses `Config.hse`; oscillator/bypass select SOURCE 0/1. The accepted crystal path relies on vendor-documented composition, without a hidden-node duty certificate. Bypass retains the 40–60% waveform requirement. Both paths remain init-only and rate-only, without individual-cycle, reference-loss recovery, continued CPU progress or low-power restoration guarantees.

## Reviewed source identities

- Stage54 ZIP SHA-256: `e376ff9b217e3b7dcade9c0367f3fee83e366525541898f9170961a178ac7015`.
- Initial runtime manifest SHA-256: `6689ff2d0ed3b21cb8b2fcc83b67907f23ac5b965d70df5ab0430c5185f90d86`.
- Initial runtime delta SHA-256: `bb90d91debf7c21151c22a0160d9c53bb91a0a1752fd2fe832a4d9c7b106c4d5`.
- All seven original PDF hashes and all seven extracted-text hashes independently match `docs/hse-pll-source-receipt.json`. F020 uses `current-datasheets/CW32F020_DataSheet_CN_V1.3.pdf`, never the historical same-name root copy.
- Read own RM source/bin/mode tables and stop/start rules at F020 PDF52–53, x030 PDF54–55, L083 PDF59–60; HSE-to-PLL examples at PDF66/68/73. Read own PLL datasheet tables at F020 PDF45, F030 PDF47, A030 PDF44, L083 PDF56. These agree with the runtime electrical limits, source encodings, multiplier range and longest startup setting.

## Runtime and arithmetic

`hsi_48mhz.rs` and `l052_l083.rs` validate the full `Hse::bounds()` envelope, not reconstructed nominal Hertz. Board conditions must be covered by the source declaration, both source and PLL envelopes must be qualified, and exact bonded pads must exist. The `hse_supported` value is emitted from generated PLL metadata and checked explicitly.

Input must be within 4–24 MHz and fit one closed input bin: 4–6, 6–12, 12–20 or 20–24 MHz. Multiplication by literal 2–12 uses checked wide arithmetic on all three source numerators, preserving the original denominator and setting rate-only qualification. Output must independently fit 12–48 MHz on F020 or 12–64 MHz on F030/A030/L083 and one output bin: 12–18, 18–24, 24–36, 36–48 or 48–72 MHz. First matching bin consistently handles a shared exact endpoint. A nonzero-width envelope crossing a boundary is rejected. Bus division cannot legalize excessive raw PLL output.

`ClockBounds::minimum_below/maximum_exceeds` compare rational endpoints without inward rounding. HCLK/PCLK operating admission retains the 24 MHz ceiling below 1.8 V and own-family higher-voltage ceiling, plus the retained HSI escape checks. Flash uses outward-ceiled actual HCLK and the retained HSI envelope at final divisors: `(upper_hclk - 1) / 24_000_000`. Incoming legal/stable clock and Flash state remain required. Monotonic temporary AHB/APB guards and conservative initial Flash latency precede source changes; final dividers follow source-switch acknowledgment.

Both backends leave inherited PLL through unchanged HSI, stop PLL and observe its true STABLE clear before changing its source/fields. Configured HSE is started/reused under unchanged owner/pad policy before the dependent PLL enables. Reference validation immediately precedes enable and SYSCLK switch; mode/pad/source matching is source-aware. HSE longest wait/filter-off and PLL WAITCYCLE=7 remain. Parameter writes preserve reserved fields, reject nondefault PLL debug nibble, and match the selected source, bins, multiplier, wait and debug value. SOURCE=2 is never selected.

Full tree readbacks occur before Flash reduction, after Flash acknowledgment, and after requested LSE startup. Requested HSE enable/STABLE/parameters/pads and relevant sticky faults are checked; no sticky flags are cleared. The L083 path retains configurable CCS; classic families retain mandatory write-one controls. LSE startup may change only its own admitted enable/detector policy. RCC clock publication occurs only after configure succeeds. As already documented in the unchanged public initializer, a later time-driver hardware failure can occur after successfully verified RCC clocks have been published; this is not broadened into a promise that every possible HAL error leaves clocks unpublished.

## Preservation and scope

The Stage54 comparison shows no changes to `ClockBounds`, operating admission, time-driver arithmetic, ADC/complementary-PWM strict cycle checks, HSE pin generation, LSE implementation, or LSE ownership/qualification files. All 23 exact-package LSE contracts remain byte-identical. All authored clock-limit facts are identical after removing the new `hse_supported: true` on F020/F030/A030/L083.

The original HSI divider/source formulas and default HSI /6, PLL None and SYSCLK HSI are retained. Independent rational arithmetic reproduces the existing nine F020 HSI pairs: /6 ×2,4,5 and /10 ×3,4,6,7,8,9. The other three retain twelve: /6 ×2,4,5,7 and /10 ×3,4,6,7,8,9,11,12. Added readbacks check invariants established by the existing transition; they do not introduce a new HSI electrical admission rule. The shared L052 final checks similarly validate the successful existing tree, without admitting PLL there.

Retained RTC/AWT owners in the classic backend and RTC/AUTOTRIM/LVD/ETR concerns in L083 remain under their inherited admission and gate-restoring inspection policy. No owner is reset to make an HSE request succeed. The existing exact-reuse checks remain. L083 retains inherited oscillator pad reservations for the boot, including conservative PF1 retention after inherited crystal use. These are retained ownership policies, not newly broadened claims about otherwise unqualified inherited states.

## Ordinary example audit

The eight package selections expose PF0/PF1 and PA8/PB0 in generated metadata. The unchanged pad generator obtains oscillator routes from exact SYSCTRL signal metadata, not register-layout inference. UART uses PA8 and activity output PB0, separate from HSE PF0/PF1. Crystal reserves both oscillator pads; bypass uses PF0, subject to retained L083 PF1 ownership.

Both HSE features declare 8,000,000 Hz nominal with actual 7,999,600–8,000,400 Hz, HSI /6, PLL ×4, SOURCE=0 for crystal or 1 for bypass. Thus raw SYSCLK is 31,998,400–32,001,600 Hz, inside input bin 6–12 MHz and output bin 24–36 MHz, below every raw cap. Default HCLK/PCLK are nominal 32 MHz, requiring one Flash wait. Low-voltage AHB /4 gives 7,999,600–8,000,400 Hz buses and zero wait. Board declaration is −20…70°C, normally 3.0–3.6 V or low-voltage 1.65–1.79 V; retained HSI remains independently safe.

Both modes divide exactly by 32 for nominal 1 MHz Embassy ticks, or by 8 at low voltage, satisfying classic power-of-two and L083 integer prescalers. Fractional HSI examples remain excluded from time-driver admission when not exactly divisible; simultaneous HSE modes and HSE+fractional are compile errors. Bounds stay rate-only through timer division; a nominal tick is not guaranteed one microsecond. README explicitly makes ±50 ppm and waveform/startup conditions board obligations, not measured hardware results.

## Evidence boundary

Successful compiler exit and strong linked symbols prove build integration only, not electrical operation. Initial missing-SVD and existing-output-conflict generation failures remain recorded beside their successful authorized retries. No build or runtime assertion certifies any physical board.

## V2 freeze update

The requested stale descriptions are corrected. All 1,195 source and 902 generated files match the V2 manifests when independently rehashed. Source manifest SHA-256 is `2962944178a3981a9ffaa094974aea5161e88eb7e8ac7383b5dcaf6e34021a27`; delta is `f26ce39bc21a6349292a9d7c3f95e2d278b186f1cf8903d6afa2a6de4b09ff41`; generated manifest is `d93172e39c8aa1e0d5d8b6f17d8d17b74eb5a1968394db5fa0fb12dc5b76b54d`.

The eight library build receipts each report exit 0 and independently matching log SHA-256. Their V1-to-V2 correspondence is verified rather than assumed: the sole runtime source change is the opening module comment, and all twelve generated file changes are exactly the two source-enum description replacements. Build.rs, the L052/L083 backend and example configuration/Cargo feature choices are byte-identical. Other source changes are documentation, source-verifier, accepted schema receipt and provenance/description ledger updates. Eighteen final ELFs have now been independently hashed and checked as described below.


## Final runtime/build correspondence

All eighteen retained ELF files match their manifest sizes and SHA-256 values; their command receipts select the recorded exact package, HSI/crystal/bypass feature and binary, and each exit-0 build log rehashes correctly. Direct `readelf` inspection confirms ELF32 little-endian ARM EXEC and strong defined Reset/main in every file. The reviewed layout receipt checks Flash vectors, aligned RAM stack, Thumb entry/reset identity, executable entry and bounded Flash/RAM load segments. For all six HSE Embassy-time files, this reviewer additionally checked a strong non-default GTIM1 handler, its exact device IRQ vector, and a strong Embassy wake symbol.

The eighteen combinations are HSI/crystal/bypass UART on F020/F030/A030/L083 (twelve), plus crystal/bypass Embassy-time on F020/F030/L083 (six). The absence of an A030 time ELF is explicit; its admitted configuration arithmetic is reviewed, while this matrix is not evidence of a compiled A030 time image. Low-voltage mode arithmetic is reviewed; these eighteen builds do not claim low-voltage hardware execution.

ELF manifest SHA-256: `a451f16ec7845ca1bd020de80bfce4169da01eed97d5ec2114e8f14c4eed543b`. ELF layout receipt: `73345cbf31bd5d75e90d1b8e6c9525438abd571a1f0b0b58da7e46112122ac04`. Library V1/V2 correspondence: `b944a57283df243c5a42b4617fd8d8f5219639f48e76c1768537fae280f21420`. This review independently confirmed the comment-only generated/runtime delta underlying that library correspondence.

The source/generated V2 manifests were rehashed again when writing this receipt. The external source/data suite later reported one canonical-ledger hash failure for `sysctrl_cw32l011_v1.yaml`; both L011/L012 original Stage54 register files are byte-identical, and the implementer identified the analogous L012 ledger omission too. That retained-ledger correction and shared documentation remain separate. Preserve this runtime freeze and use a narrow independent correspondence supplement for subsequent final-package manifests; do not silently treat this runtime PASS as acceptance of an unreviewed later source state.

Matrix acceptance receipt SHA-256: `6eb85c7def453e71adc16a602e71874f1c44dd2874dccf81362ba88c2defaf6f`; the eight-library/eighteen-ELF scope matches this reviewer’s independent receipt checks.
