# Init-only LSE SYSCLK on CW32L083

This bounded mode selects nominal32768Hz LSE as SYSCLK on exactly CW32L083RBT6, CW32L083RCT6, CW32L083RCS6, CW32L083MCT6 and CW32L083VCT6. Set `Config.lse=Some(...)`, `Config.sys=Sysclk::LSE` and leave `Config.pll=None`. Family aliases and other parts are not qualified. The existing board contract for crystal/bypass electrical behavior, per-cycle source bounds and PC14/PC15 ownership remains in [qualified-l083-lse.md](qualified-l083-lse.md).

This is an initialization mode. It does not add public LSI SYSCLK, runtime retuning, low-power restoration, source-loss recovery, independent PLL output ownership or RTC migration. Software correspondence and build results are separate from hardware validation; no silicon or board execution is claimed here.

## Own sources and electrical admission

The authoritative URLs and hashes remain in `sources/evidence-sources.json`. The target-only qualification is [l083-lse-sysclk-qualification.json](l083-lse-sysclk-qualification.json); the existing active-LSE source/member receipts remain authoritative for source identity.

- [CN manual V2.0](https://www.whxy.com/uploads/files/20240920/CW32L083_UserManual_CN_V2.0.pdf), SHA-256 `9930bf1755f3bbf8933163c2d0da57fd9a4f3250a358a4c0bfc75ed4eda3a0a3`.
- [CN datasheet V1.9](https://www.whxy.com/uploads/files/20251229/CW32L083_DataSheet_CN_V1.9.pdf), SHA-256 `852f772e9174cb76bf0f475f31f1e275254f8fe176bd3e7ad60d00b41db9509e`.
- [SDK V2.2](https://www.whxy.com/uploads/files/20240821/CW32L083_StandardPeripheralLib_V2.2.zip), SHA-256 `2d58765568d8dd8a218b52e4650aa6e5bd4f2e4b5f386196bae637dfc0b3fe73`. Its encodings corroborate the native PAC; its full startup sequence is not the HAL contract.

Page references below are1-based PDF/printed. Manual65/64 and76/75 specify128 LSE edges during256 LSI clocks. Factory32800Hz LSI is qualified±3% over−40…85°C, giving31,816…33,784Hz (datasheet55/54). The separate sufficient software margin is `256 * declared_LSE_min_hz > 129 * 33784`, with u64 products. The smallest integer minimum passing this inequality is17024Hz; the declared envelope must still contain nominal32768Hz. The extra edge does not certify per-cycle LSI jitter. Poll budgets count attempts, not elapsed time or a crystal startup maximum.

Manual65/64 gives the same CLKCCS behavior for directly selected HSE and LSE: with CLKCCS=1, source failure selects HSI; with CLKCCS=0, no switch occurs. Manual75/74,76/75 and78/77 do not explicitly promise post-fault divider retention or the fixed-/6 rewrite documented for another family. This new LSE target conservatively covers raw factory HSI's48.96MHz upper bound, giving no HSI/AHB/APB divider credit. That is a software qualification policy, not a claim that L083 hardware resets its dividers or that the existing direct-HSE policy is disproved.

Consequently the complete board declaration must have VDD≥1.8V, VDD≤5.5V, VDDA=VDD and ambient−40…85°C. Even HSI/16, large bus divisors or CLKCCS=0 do not admit a declaration below1.8V. The historical auxiliary-LSE1.65…5.5V source envelope remains unchanged. Datasheet47/46 limits HCLK/PCLK to64MHz at VDD≥1.8V and24MHz below1.8V. Manual121/120 and131/130 require WAIT2 above48MHz, so this target retains Flash WAIT2 even while running32768Hz. A Flash rating up to72MHz does not increase the64MHz bus limit.

Pure validation proves requested LSE bus bounds, configured HSI with the final bus divisors, and the undivided48.96MHz fallback ceiling before peripheral acquisition. Returned clocks describe healthy intended LSE operation; fallback is electrical headroom, not a second operational timing envelope.

## Entry, references and observers

The incoming clock tree, voltage and Flash latency must already be legal. The unchanged HSI used for escape must independently be in its documented safe calibration regime even if HSI was idle or disabled at entry. Manual62/61 describes the32–48MHz calibration range; datasheet55/54 separately qualifies factory48MHz tolerance. STABLE and a legal DIV encoding do not establish an arbitrary inherited trim's rate, and the64MHz bus ceiling does not authorize arbitrarily trimmed HSI up to64MHz. The new factory bound applies only after factory trim is established.

Ordinary peripherals, DMA, application interrupts, MCO, dedicated PLL_OUT and external consumers of changing clock outputs must be quiescent under the initialization handoff. Stopping PLL stops those outputs despite preserving its configuration. NMI must not mutate the tree, and sources carrying CPU execution must remain available. A firmware jump is not equivalent to reset.

Gate-preserving inspection can briefly run whole GPIOA/B/C/D/E/F banks where present. Sampling, filters and armed events can advance even before an error; restoring gates and preserving configuration cannot undo progress. Starting a matching stopped LSI without a TRIM/WAIT write can resume parked UART1…6 SOURCE3, permitted manual AUTOTRIM timer SRC1, GPIO FLTCLK5, MCO SOURCE4 and bonded LSI output routes. PC4 AF6 is present on all five parts; PF2 AF4 additionally on MCT6/VCT6; PD5 AF6 additionally on VCT6. Enabled work-ungated LPTIM ICLKSRC3 and LCD CLKCS0 can also resume; closed work gates remain closed. The handoff must permit this progress. Cold LSE still requires the full own reset-like RTC image with SOURCE0, while reused LSE already requires a ready monitor; these admitted combinations do not newly resume RTC SOURCE2.

The unconditional native AUTOTRIM guard precedes any matching-trim shortcut and rejects automatic or active calibration and unqualified retained modes/sources. Existing RTC/AUTOTRIM/LVD/ETR ownership checks remain. Enabled HSECCS or LSECCS requires already enabled, factory-matching, stable LSI with unchanged WAIT before any source mutation. Contradictory live monitor state is rejected rather than repaired. Only mismatching stopped, nonstable, unselected LSI with no detector or ready history/IRQ owner can reach the existing two-pass idle-consumer admission for a masked trim write.

An active inherited PLL needs valid native source/MUL/debug state and stable enable state. HSI-fed PLL consumes post-DIV HSI (manual59/58 and82/81), so both HSI TRIM and DIV remain unchanged through escape and both PLL stop acknowledgments. HSE-fed PLL SOURCE0/1 must match crystal/bypass HSE.MODE and retain its enabled, stable reference and physical pads. An inherited enabled HSE remains unchanged and reserved even when `Config.hse=None`; a requested live HSE must match exactly. PLL input/output register bins do not measure inherited frequency.

## Transition and failure contract

The L083 target uses a separate module; the existing L052 target and the old auxiliary/None paths retain their own behavior.

1. Admit retained state, owners and monitor policy; enable the Flash configuration gate and establish WAIT2 while the legal incoming source still runs.
2. Make unchanged HSI ready. Read back preservation of inherited enables, CCS, LSELOCK and unrelated CR1 controls. The first CR0 write explicitly selects HSI and installs monotonic guards of at least AHB/4 and APB/8, preserving stronger inherited divisors. An explicit target avoids replaying a stale HSE/LSE selector after asynchronous fallback.
3. Acknowledge HSI and guarded buses, then stop inherited PLL and observe both PLLEN=0 and PLL.STABLE=0. Preserve PLL fields and its reference through those acknowledgments. Manual67/66 only permits PLL switches through HSI/HSE; no direct PLL→LSI/LSE switch occurs.
4. Run the unchanged stopped-LSI helper now that SYSCLK is HSI. Factory-matching live LSI is never retuned; the stopped-mismatch path retains both complete consumer passes. Enable and retain the qualified monitor. If HSI needs factory retrim, use a private ready-LSI bridge, stop HSI, modify only TRIM, restart and return to HSI. The documented live DIV-only operation then installs the configured divider with TRIM unchanged.
5. Preserve/reuse inherited HSE, or start a newly requested cold HSE under its own existing contract. Start or exactly reuse LSE with the real single MODE/DRIVER/AMP/WAITCYCLE bank. L083 has no startup PDRIVER/PAMP bank. Preserve CLKCCS/HSECCS/LSELOCK; fresh LSE enables its own detector.
6. Recheck the complete protected tree after owner/pad work. Install final bus divisors while still on configured HSI and WAIT2, then perform the final intentional LSE mux write. No CR0 mutation follows it. Keep WAIT2; verify source, buses, HSI, monitor, stopped unchanged PLL, external sources/pads and Flash before publishing clocks.

Failure publishes neither frozen clocks nor the target-success record. Partial hardware changes, reservations and diagnostic flags can remain; no rollback or live retry guarantee is provided. Reset before retry; POR-retained LSE may need a power cycle. No flags are cleared, IRQ owners masked, clocks recovered or RTC sources migrated to hide failure. Bounded polling requires CPU progress and does not guarantee survival after loss of the running source.

After source loss or automatic fallback, previously frozen LSE and PCLK-derived timing is invalid. Drivers do not silently recalculate baud rates or timers. Existing ADC/SPI/I2C/UART/timer bounds and RTC capability checks remain in force. The fixed1MHz Embassy time driver cannot use32768Hz with these integer bus dividers; its existing pure rejection remains before acquisition. Use the blocking example without a time-driver feature rather than treating low-speed SYSCLK as a new tick-rate configuration.
