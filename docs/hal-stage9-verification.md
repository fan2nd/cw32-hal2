> Historical software-verification references: HAL tests and fixture harnesses were deleted on 2026-10-08. Counts and commands below apply to the dated source snapshot. See [current HAL layout and build scope](hal-production-layout.md).

# Stage 9: seven additional classic GTIM families

This checkpoint extends polling general timers and simple PWM to F002/F003,
L031/R031/W031 and L052/L083. Together with the previous F020/F030/A030 paths,
classic GTIM support covers ten families. L010/L011/L012 buffered GTIM and RTC
calendar work remain separate pending batches. No hardware was executed.

## Hardware-specific implementation

Actual generated GTIM/GTIMn instances and versioned PACs determine instance traits
and pin bindings. F002/F003 expose their single GTIM and never access absent DMA
or PSC registers. Linear-prescaler variants adapt PSC+1 while exposing the same
conservative power-of-two divisor subset. Configuration follows documented
stopped-counter/enable-edge prescaler behavior, preserves unrelated gate/reset
bits, and implements duty endpoints with forced output modes. Split PWM handles
retain their peripheral/pin ownership. Reconfiguration is not promised glitch-free.

The seven-family source audit qualifies 215 routing entries using own manuals,
datasheets, SDK macros and exact package grids. Unsupported/debug/oscillator/radio
pads and SDK-only routes remain excluded. Generation changed only GTIM pin
metadata on 28 intended chip selections; every other chip field and all 135 PAC
register modules remain byte-identical to Stage 8. See `docs/classic-gtim.md`,
`docs/classic-pwm-routes.md` and their machine-readable source evidence.

## Measured verification

The main-tree combined matrix completed at 2026-10-08 11:52:15 UTC:

- 54 host selections across 13 families and 108 optimized ARM builds
- 12,527 unit + 24 interrupt-binding + 1 API + 144 documentation test executions
- Zero warnings; all 909 scoped inputs unchanged
- Manifest SHA-256: `e981838695eae66f1c0022901f0ebf92cdeec134366f1590450b3144c4e41c8d`

Focused merged-source qualification passed 2,988 unit + 4 IRQ + 32 doctest
executions across all 13 representative families, 28 new GTIM selections with
756 route controls and 411 diagnostic-specific negative cases, and 42 inspected
ELFs across 21 exact parts with/without defmt. Legacy timer, all-family BTIM and
modified family API contracts passed. Full `./d test` and `./d check` passed,
including all 54 PAC selections, metadata and generated/source parity checks.

An independent source/driver review accepted the frozen candidate. All 29 owned
files matched it at integration. A separate review checked the shared build/lib
hooks and preservation of the existing FLASH cfg category. Logs, hashes, generated
metadata deltas and receipts are in `docs/verification-logs/classic-gtim-merged`
and `docs/verification-logs/stage9-classic-gtim`.

## Explicit test-boundary corrections

The first data run rejected new PWM metadata because the RTC correction verifier
intentionally hashed its entire historical input manifest. The verifier now
subtracts only explicitly listed, exact-value and SHA-pinned subsequent PWM
additions before checking the original RTC boundary. Original hashes remain
unchanged. The two-file test/evidence delta changed no runtime or PAC register
code; the full data/PAC suites passed afterward. A separate legacy-suite launch
lacked cargo on PATH; the explicit project-toolchain retry passed. Failed attempts
are retained and excluded from acceptance claims.

## Remaining scope

ADC remains blocking single-conversion only. Timer capture/encoder, cascades,
master/slave routing, TRGO, hardware-triggered ADC and safe DMA remain incomplete.
Per-destination trigger mappings cannot be represented by a universal selector
enum; source contradictions remain quarantined pending qualified typed APIs.
RTC, comparator, advanced/low-power timers, additional Flash variants and other
accelerator/radio peripherals remain listed in `docs/hal-coverage.json`.

Stage 8's low-power/dual-ADC and reserved-region Flash limits continue to apply.
Host models, source review and compile/link tests do not establish silicon timing,
waveform quality, electrical accuracy, endurance, RF coexistence or retention.
All official evidence is acquired reproducibly and is not redistributed here.
