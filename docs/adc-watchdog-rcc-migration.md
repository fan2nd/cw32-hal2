# ADC and watchdog central RCC migration

Source review and Cortex-M0+ compilation, 2026-10-08. This migration changes
only peripheral clock/reset control. Electrical constants, oscillator bounds,
conversion timing and watchdog counter calculations remain unchanged. There is
no hardware execution or timing certification.

## Boundary and ownership

The selected peripheral's generated `SealedRccPeripheral::RCC_INFO` now owns all
ADC/IWDT/WWDT SYSCTRL bus-gate access and reset-state queries. Classic ADC calls
the descriptor directly. Sequence ADC retains its meaningful acquired-state/BGR
release decision; L012 retains the common-group preflight. The redundant L012
`CommonIo` gate adapter and watchdog-only gate generator are removed.

The common generator and `rcc/peripheral.rs` are not changed. Their emitted
policies already match these existing driver contracts:

- Classic ADC: reset and disable allowed; enable/disable have one readback,
  and the active-low reset pulse has one readback after release.
- L010/L011 ADC: reset forbidden; conditional disable allowed. The driver still
  requires `!was_enabled && !ADC_CR.BGREN` before asking RCC to disable.
- L012 ADC1/ADC2: the same APBEN1.ADC and APBRST1.ADC descriptors; reset and
  disable both forbidden. The BGR owner acquires the common group through the
  ADC1 descriptor. It does not invent a separate BGR bus clock or reference count.
- IWDT/WWDT: reset and disable forbidden. Only explicit enable with the existing
  caller-provided poll budget is used. Drop never stops, feeds, resets or gates.

No first-user reset or last-user disable is introduced. Untracked bootloader,
comparator, OPA, BGR and sibling ADC dependencies keep their original protection.

## Source-qualified locations

All offsets below are bytes within the actual selected SYSCTRL instance.
The central descriptor stores offsets in 32-bit words, so these become
0x34/4=13, 0x38/4=14, 0x44/4=17 and 0x48/4=18.

| Driver/families | Enable | Reset | Enable write key |
|---|---|---|---|
| Classic ADC, ten families | APBEN2@0x34 bit2 | APBRST2@0x44 bit2 | none |
| L010/L011 ADC | APBEN1@0x38 bit0 | APBRST1@0x48 bit0 | bits31:16 = 0x5a5a |
| L012 ADC1/ADC2 common group | APBEN1@0x38 bit0 | APBRST1@0x48 bit0 | bits31:16 = 0x5a5a |
| IWDT, L010/L011/L012 | APBEN2@0x34 bit4 | APBRST2@0x44 bit4 | bits31:16 = 0x5a5a |
| IWDT, other ten families | APBEN1@0x38 bit5 | APBRST1@0x48 bit5 | none |
| WWDT, L012 | APBEN2@0x34 bit5 | APBRST2@0x44 bit5 | bits31:16 = 0x5a5a |
| WWDT, other ten supported families | APBEN1@0x38 bit4 | APBRST1@0x48 bit4 | none |

Enable is active-high; reset is active-low and unkeyed. L010/L011 have no WWDT.
Each location is checked against selected `rcc_control`, matching legacy `rcc`
metadata, selected SYSCTRL register IR and generated descriptor output.

Primary-source locators, already qualified independently per family:

- [Classic ADC control proof](adc-classic-family-electrical-audit.md), "Abort,
  stale results and reset proof": F002/F003/L031/R031/W031/L052/L083 own SYSCTRL
  gate/reset pages 61/64, 63/66, 78/81, 80/83, 79/82, 82/85 and 88/92.
- [F020 and x030 baseline](adc-timer-next-batch-audit.md), SYSCTRL gate/reset
  summary; [F020 implementation](f020-adc-timer-implementation.md).
- [L010/L011 sources](adc-low-sequence-audit.md): own manuals CN1.2 and current
  CN1.1, sections 4.7.12/4.7.15, printed pages 79/82 and 77/80, respectively.
  Both APBEN1 tables explicitly require the upper-halfword 0x5a5a key.
- [L012 common ownership](adc-l012-dual-ownership.md): own CN1.4 manual sections
  4.7.12/4.7.15, pp57–58/61–62, describe keyed ADC gate and active-low ADC reset;
  section 25.12.19 describes retained common BGR dependencies.
- [IWDT own-family evidence](watchdog-evidence.md), "Clock audit" and
  "Manual/SDK protocol locators", with source hashes in `watchdog-evidence.json`.
- [WWDT own-family evidence](window-watchdog-evidence.md), "Primary references",
  including every family's own SYSCTRL gate/reset sections and source hashes.

The three low-family gate/reset tables were re-read from the pinned official
manual text during this migration. No compatibility claim is inferred merely
from similar peripheral or register names.

## Exact register-operation equivalence

`Reg::modify` in the generated PAC performs one volatile read followed by one
volatile write. `Reg::write` starts from a software default and performs one
volatile write, with no implicit hardware read. Thus replacing the watchdog's
explicit read + write with central `modify` preserves the hardware operation
count and ordering.

For gate bit mask B and old register value V:

- Unkeyed enable: old and new both write `V | B`.
- Keyed enable: old and new both write `(V & 0x0000ffff) | 0x5a5a0000 | B`.
- Unkeyed disable: old and new both write `V & !B`.
- Low-ADC keyed disable: old and new both write
  `(V & 0x0000fffe) | 0x5a5a0000`.
- Active-low assertion/release: old and new perform separate reset-register
  read-modify-writes of `V & !B`, then the fresh value `V | B`.

Neighboring gate/reset bits are preserved. The key is replaced rather than
ORed onto stale upper bits. Reset registers never receive a key.

### Classic ADC

Before and after, within the same critical section:

1. Read APBEN2, write ADC=1, read APBEN2 once.
2. Read APBRST2, write ADC=0.
3. Read APBRST2 again, write ADC=1, read APBRST2 once.
4. Leave the critical section; perform the unchanged ADC initialization.

Construction still validates initialized clocks and electrical timing before
any gate write. Initialization error still disables the gate after the same
engine error. Drop still shuts down the engine first. Both error/drop release
read APBEN2, write ADC=0, then read APBEN2 once. `Readback::Once` cannot return a
timeout; ignoring its result does not remove a formerly reported error.

### L010/L011 sequence ADC

Before and after:

1. Read APBEN1.ADC into `was_enabled`.
2. Read APBEN1, write the keyed ADC enable even if it was already enabled.
3. Read APBEN1.ADC at most the caller's `timeout` times, returning on the first
   enabled read; no spin instruction is inserted. Exhaustion returns
   `ClockEnableTimeout`, with no rollback or reset.
4. On release, short-circuit `!was_enabled` first. Only then read ADC_CR.BGREN.
   Only if both conditions hold, read APBEN1, write keyed ADC=0, and read APBEN1
   once. Inherited enable and any BGR use therefore retain the gate.

The public configuration validation and all ADC initialization, conversion,
shutdown and source-qualified timing code remain unchanged.

### L012 common ADC group

Before and after:

1. Reject a zero timeout before any hardware access (the public constructor
   retains its prior initialized-clock and timeout checks too).
2. Read active-low APBRST1.ADC. If asserted, return `CommonInReset` with no write.
3. Read APBEN1.ADC. If already enabled, return success with no gate write.
4. Otherwise read APBEN1 and write keyed ADC=1.
5. Poll APBEN1.ADC up to the caller's timeout, without a spin instruction.
   Return `ClockEnableTimeout` on exhaustion; never remove a delayed enable.

ADC1/ADC2 descriptor identity is checked; using ADC1 for common acquisition
cannot select a separate converter clock. No path changes common reset or
turns off the common gate, on success, failure or drop. Existing monotonic
BGR/temperature enables and synchronized-sibling checks are untouched.

### IWDT

Before and after, within the same startup critical section:

1. Read its own gate register and write its enabled bit, with the exact
   family key protocol above, without reset.
2. Poll the gate up to `poll_limit`; a failed read executes `spin_loop`, including
   the last failed iteration. Exhaustion returns `ClockEnableTimeout`.
3. Only after gate success, L010/L011 read CR1 and write
   `(V & !0xffff0000) | 0x5a5a0000 | LSI_ENABLE`, then poll LSI.STABLE with the same
   budget/spin behavior. LSI trim/wait controls are not written. Oscillator
   failure still returns `OscillatorTimeout`. Other families skip this step.

The oscillator enable is deliberately peripheral-specific and separate from
central bus-gate control. `IWDT_USES_LSI` and `IWDT_CLOCK` remain generated from
the unchanged source-qualified electrical metadata.

After the same gate/oscillator preflight, all startup/feed code is byte-identical:
RUN check, synchronization wait, CR/ARR/WINR reset-state rejection, Starting
marker before START, START/readback, unlock/configure/readback, unconditional
relock after START, refresh, and Running marker. Error ordering and irreversible
HAL ownership remain intact; failed startup is never implicitly reset or retried.

### WWDT

Before and after, within the same critical section:

1. Read its own active-low reset bit. If asserted, return `HeldInReset` before
   any gate read/write or watchdog-register access.
2. Read gate, write enabled bit with the exact family key protocol.
3. Poll up to `poll_limit`, spinning after every failed read. Exhaustion returns
   `ClockEnableTimeout`; no reset, disable or rollback occurs.

All subsequent start/refresh/status code is byte-identical: EN rejection,
CR0/CR1/SR reset-state preflight, CR1 configure/readback, CR0 preload/readback,
Starting marker before EN, EN-only poll, and Running marker. A failed EN readback
retains poisoned Starting state and the running gate. Refresh and read-only
status keep their previous register ordering and never reset/clear status.

## Verification boundary

Production release builds use `thumbv6m-none-eabi`, `--locked --offline`, both
`rt` and `rt,defmt` for these 13 representatives:

`cw32f030c8t7`, `cw32a030c8t7`, `cw32f020c6u7`, `cw32f002f3p7`,
`cw32f003e4p7`, `cw32l010f8p6`, `cw32l011k8t6`, `cw32l012c8t6`,
`cw32l031c8t6`, `cw32r031c8u6`, `cw32w031r8u6`, `cw32l052c8t6`,
`cw32l083mct6`.

The handoff source audit checks 38 ADC/watchdog RCC descriptors against selected
metadata/IR and compiled generator output. It also hashes nine byte-identical
ADC engine/register and watchdog start/feed/LSI regions across the before/after
snapshot. Source scans find no remaining driver-local gate/reset accessors.
This is a static source audit, not a new HAL test or behavioral harness.

The current source tree has no ADC/watchdog firmware examples. No examples,
unit/integration tests, mock register harnesses or path attributes were added.
The final integrated whole-package matrix belongs to the parent build. No
firmware was pushed, flashed or executed.
