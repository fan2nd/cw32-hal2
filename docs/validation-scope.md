# Validation scope

Local tests, builds and data/PAC generation are authorized. No CI is configured
or required. Run these entry points locally with Python 3.11+ and the required
development dependencies installed:

- `./d test`: generator, schema and data/PAC contracts.
- `./d test-hal`: host behavior, compile-time ownership, 52 HAL selections and release examples.
- `./d audit-current`: official source facts, projections and distribution checks.
- `./d check`: regeneration, determinism and chip-feature builds.
- `./d lint`: module layout.

Source audits, compilation and hardware validation are separate results.
Missing evidence or a failed command is not a pass. Software checks do not
establish electrical behavior, runtime timing or cancellation safety.

Current software acceptance (2026-10-18): Rust 1.99.0, thumbv6m-none-eabi,
Python 3.12; pinned official inputs and Poppler 25.03.0 for evidence text.
The host matrix executes 172 tests across 12 supported families. The 67 real
ARM API probes check positive controls and specific borrow/lifetime diagnostics.
The release example matrix covers 164 feature selections, including async ADC,
native UART idle and timed LIN. Classic ADC uses size optimization to fit F002.
Per-command logs and statuses are emitted under `build/acceptance/`.

| Area | Software evidence |
|---|---|
| UART / LIN | Ring prefix/error ordering, retained idle markers, protocol/PID/checksum, dispatch, schedule bounds/cancellation; buffer-lifetime probes; source IRQ audits and ELF links. |
| I²C / ADC | Address/timing/retry boundaries and invalid-input preflight; live owner/output-borrow probes; typed-register/source audits; blocking and async ELF links. |
| DAC / WWDT | Split-owner exclusion, watchdog count/rounding boundaries and wait-lifetime probes; source register audits and ELF links. |
| Timers | Quadrature wrap/loss/overflow and Hall-history barriers; capture/command/shared-vector source audits and ELF links. |
| LVD / comparator | Exact threshold table, live/cancelled wait-borrow probes, source filter/edge/shared-vector review and ELF links. |

Cancellation and shared IRQ MMIO paths are source-reviewed and compiled:
I²C/ADC guards request bounded retirement and latch failed cleanup; native edge
waits retire local selectors; comparator handlers preserve shared vectors; DAC
channel Drop serializes shared control updates; WWDT Drop retains irreversible
watchdog service. Host tests do not execute these physical IRQ/MMIO paths.
Hardware acceptance remains a separate TODO, including error injection,
cancellation/reset timing, analog settling and real LIN waveforms.
