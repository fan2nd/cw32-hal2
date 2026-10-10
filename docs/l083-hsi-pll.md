# CW32L083 HSI- or HSE-fed PLL

This bounded mode configures PLL once during HAL initialization using factory HSI or a board-qualified HSE crystal/bypass reference. Within the shared RCC backend it is qualified only for CW32L083, not L052. Independent PLL outputs, runtime retuning, DeepSleep restoration and guaranteed reference-loss recovery remain unqualified. The clock hardware in other families is not asserted absent.

## Own sources and admission

The authority is `sources/evidence-sources.json`. `cw32-data/pll-qualified.yaml` holds authored machine-readable facts; the adjacent `l083-hsi-pll-source-receipt.json` gives exact original URLs, revisions, SHA-256 and PDF/printed claim pages. The manual is CW32L083_UserManual_CN_V2.0.pdf, especially §4.3.7 PDF59–60/printed58–59 and §4.7.8 PDF82/printed81. The datasheet is CW32L083_DataSheet_CN_V1.9.pdf, Tables7-4,7-17,7-21 PDF47,55,56/printed46,54,55.

The HSE extension has its own [source receipt](hse-pll-source-receipt.json) and accepted [crystal composition contract](hse-pll-crystal-contract.md). The earlier HSI receipt and Stage42 build evidence remain historical records of their original scope.

Factory HSIOSC is nominal 48 MHz, qualified ±2% over −40…85°C and VDD 1.65–5.5V. PLL input must be 4–24 MHz; the datasheet output 8–64 MHz intersects the documented analog bins 12–72 MHz to 12–64 MHz. Each full actual input envelope and each multiplied output envelope must independently fit one analog bin. This conservative software qualification admits 12 nominal pairs: HSI /6 ×2,4,5,7 and HSI /10 ×3,4,6,7,8,9,11,12. Other documented hardware combinations are not claimed electrically impossible.

The qualified HSI input duty45–55% lies inside PLL's40–60% requirement. There is no separate PLL input or output divider. `Config.hsi.div` remains the HSI divider and never divides HSE; `Pll.mul` is the literal 2…12 field, then AHB/APB divide the raw PLL output. Checked u64 numerator multiplication and checked u32 conversion preserve the original denominator. Neither nominal nor outward-rounded Hertz is multiplied to reconstruct the source. The analog bins do not authorize operation above the independent raw-output or voltage-dependent bus ceilings.

## Configuration

Set `Config.pll = Some(Pll { src: PllSource::HSI, mul: PllMul::Mul7 })`, `Config.hsi.div = HsiDiv::Div6`, and `Config.sys = Sysclk::PLL`. The example declares3.0–3.6 V and −20…70°C and uses AHB/APB /1. Nominal SYSCLK/HCLK/PCLK are 56 MHz, with rate bounds54.88–57.12 MHz; WAIT2 is required. Default config retains direct HSI and PLL disabled. Configured PLL must be the requested system source; no independent-output lifecycle is exposed.

At VDD below 1.8 V, actual HCLK/PCLK must remain ≤24 MHz, so a PLL choice needs sufficient downstream division. Its raw output still must satisfy 12–64 MHz. The exact 1 MHz Embassy time driver separately checks final nominal divisibility before device acquisition; /10-based fractional-MHz rates may be unsuitable without invalidating PLL hardware qualification.

## HSE reference and board contract

For HSE, set the existing `Config.hse` declaration, `PllSource::HSE` and `Sysclk::PLL`. `HseMode::Oscillator` selects PLL SOURCE0 and `HseMode::Bypass` selects SOURCE1; no separate PLL mode declaration is needed. Missing `Config.hse` is rejected. The undivided source's original actual bounds must fit 4–24 MHz and one input bin; its multiplied output must fit one output bin and 12–64 MHz. Source conditions must cover the entire board declaration within VDD 1.65–5.5 V, VDDA=VDD and ambient −40…85°C, also preserving retained-HSI qualification.

Both HSE example features declare nominal 8 MHz, actual 7,999,600–8,000,400 Hz (±50 ppm), and ×4: nominal 32 MHz, actual 31,998,400–32,001,600 Hz. These fit input bin 6–12 MHz and output bin 24–36 MHz. The board must establish that complete source envelope across tolerance, load, temperature, aging, source supply and short-term variation. A nominal rating alone does not qualify it. The unchanged `low-voltage` choice adds AHB /4 for nominal 8 MHz buses; both bus choices permit nominal 1 MHz Embassy ticks without a wall-time guarantee.

Crystal mode uses the existing resonator, load, drive, layout and startup contract on PF0/OSC_IN and PF1/OSC_OUT. RM §4.5.8 PDF73 explicitly programs crystal HSE SOURCE0 into PLL; §4.4.1 PDF62 describes startup. Functional admission relies on that documented internal composition. PLL input duty40–60% remains a device condition; the internal reference duty is not independently certified, and no caller duty certificate is introduced. STABLE is a startup handshake, not a duty measurement. The typical 2 ms crystal startup is not a maximum.

Bypass reserves PF0/OSC_IN and requires its accessible waveform to satisfy 40–60% duty, high 0.7×VDDIOx…VDDIOx, low VSS…0.3×VDDIOx, high/low pulses each at least 15 ns, rise/fall each at most 20 ns, and all own I/O ratings together. These digital input rules are not imposed on analog crystal pins. Keep HSE continuously within its board contract while frozen clocks are used. The existing filter-off, 262144-cycle wait, detector and pad rules remain; see the [direct-HSE contract](qualified-l083-hse.md). An inherited crystal can retain both pad reservations even when subsequently configured for bypass.

## Transition and retained owners

Entry must already have legal, stable clocks, voltage and Flash latency. Ordinary peripherals, DMA, application interrupts, MCO, dedicatedPLL_OUT and any other externally dependent consumers must be quiescent; NMI must not change clocks. A firmware jump is not reset. The board must keep the active sources available during the transition. Bounded register polls cannot make CPU execution survive an active clock loss.

Preflight checks config, selected owners and factory trim before mutation. Existing RTC/AUTOTRIM/LVD ownership checks apply even with no new HSE. Active automatic trim is rejected, RTC wake-clock ownership is retained without resetting RTC, and a required HSI retrim cannot interrupt raw-HSI owners. Retained HSE/LSE detector settings/faults and legal LSI support are preserved. Unrequested HSE pads and crystal ownership remain reserved by the existing path.

The central RCC enables Flash configuration, sets conservative WAIT2 and monotonic AHB≥/4, APB≥/8 guards. Unchanged HSI is made ready and selected to escape any inherited PLL before a possible LSI trim bridge. PLLEN is cleared and both PLLEN=0 and PLL.STABLE=0 are observed before any HSI source/divider or PLL parameter changes. Source, input bin, literal MUL, output bin and longest WAITCYCLE=7 are written by typed `modify`. Reserved/debug 19:16 must already have documented default 0x5 and are never rewritten. The upper reserved bits are preserved. CR0/CR1 and Flash writes use their own required keys; PLL has no key.

After parameters and enable readback, the actual RO PLL.STABLE latch must assert. The clearable PLLRDY event is not used as readiness, and no unrelated ICR flags are cleared. Guarded buses remain in force through PLL mux acknowledgment, then final dividers are set/read back. The final pass rechecks source, divider, HSI trim/readiness, PLL fields/readiness/enable, retained CCS, external faults and Flash. Flash is lowered only for the verified maximum of selected and retained HSI escape HCLK. Failure publishes no frozen clocks; reset before retrying, since partial hardware state may remain.

HSE-fed PLL additionally requires matching enabled, startup-stable HSE and physical pads before PLL enable and SYSCLK selection. The requested HSE mode selects the same SOURCE value in configuration and readback. The full source tree, including HSE parameters/pads and applicable faults, is checked before reducing Flash latency and again after Flash and any requested LSE startup before freezing clocks. Existing RTC/AUTOTRIM/LVD/ETR exclusions, configurable CCS and lock preservation remain in force.

STABLE is a startup latch, not continuous lock-loss detection. L083 CCS documentation describes automatic fallback from directly selected HSE/LSE, not an HSE-fed PLL; no broader recovery promise is imported. Sleep/resume has a different clock lifecycle and remains outside this API.

## Rate bounds and strict cycle-duration APIs

Table7-21 specifies maximum 300 ps cycle-to-cycle jitter. That is not an absolute period or phase-error limit. This candidate does not manufacture such a limit or assert that all multi-cycle durations are unbounded. It supports source-qualified rate propagation using the documented PLL output relation and electrical operating limits.

A PLL-derived `ClockBounds` carries a private rate-only flag through exact multiplication, bus division, centralRCC and peripheral-local division. `has_cycle_timing_bounds()` exposes that qualification. Existing `minimum_duration_ns`/`maximum_duration_ns` signatures and all old HSI/HSE behavior remain; calling them on the new PLL source explicitly panics instead of returning an unsupported hard bound. Ordinary rate consumers use nominal/boundedHertz without these strict duration helpers.

Classic ADC currently promises minimum acquisition and worst-case conversion durations. Its checked blocking/async constructors and configuration paths return `UnqualifiedCycleTiming` for PLL before ADC or RCC writes. Existing direct HSI/HSE ADC remains usable. L083 has no qualified complementary dead-time API in this baseline. AUTOTRIM uses raw HSIOSC/HSE/LSI/LSE; it does not silently inherit PLL. The RTC access-window retry delay is only a rate-derived polling interval and checks the actual WINDOW state.

## Verification boundary

Normal generator/PAC/HAL builds and ARM example links are recorded separately; no HAL tests, runtime model, adapter or hardware execution is added. Build success does not qualify board voltage, oscillator stability, external consumers or silicon behavior. Source-only delivery excludes all generated data/PAC/coverage/build trees and vendor PDF/SDK/SVD originals, retaining the existing 11 approved Apache-2.0 headers only.

Historical Stage42 combination and its fresh build evidence: [rebase receipt](l083-pll-stage42.md). The earlier source receipt retains the original Stage39-candidate validation history. Neither record establishes verification of the later HSE-PLL extension.
