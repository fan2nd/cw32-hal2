# Inherited LSE pad ownership

HAL initialization preserves inherited LSE and reserves its required bonded pads
before it returns peripheral tokens. This is ownership protection on all eleven
LSE-bearing families, independent of the selected system clock and HSE support.
It does not initialize, stop, reset, reconfigure or qualify LSE, RTC or board locks.
F002/F003 have no LSE and perform no LSE register access.

## Capture and GPIO entry

After `Peripherals::take`, common RCC initialization reads native typed
`SYSCTRL.CR1` and `SYSCTRL.LSE` before calling a backend or performing any MMIO
write. One critical section spans the capture and the existing backend sequence.
The platform entry contract requires quiescent DMA and no competing NMI or
other clock/pad writes. Both reads are ordinary reads with no documented
read-to-clear behavior. No GPIO, RTC or FLASH gate is needed for them.

The reservation uses the inherited software enable and external-input mode:

| LSE enable | Applicable pad lock | Mode | Reserved pads |
|---|---|---|---|
| 0 | 0 | either | none |
| 1 | 0 | crystal/resonator | OSC32_IN and OSC32_OUT |
| 1 | 0 | external digital input | OSC32_IN |
| either | 1 | either | OSC32_IN and OSC32_OUT |

The applicable pad lock is `LSE.PINLOCK` on L010 and
`LSE.PINLOCK && CR1.LSELOCK` on L011/L012. Other families have no pad-lock field;
`LSELOCK` alone is an enable lock and does not reserve a disabled source's pads.
The reservation does not require `LSEEN=1` when a pad lock applies. It does not
shrink on `STABLE=0`, clock faults, RTC stop/source changes or later oscillator
changes. Faults can suspend hardware lock protection on L011/L012; this software
policy conservatively keeps the reservation and does not claim that every
hardware pad write is blocked in all fault states.

The common state retains the union of captured ownership for the boot, including
when initialization later fails. Initialization failure returns no tokens and
retains the existing reset-before-retry requirement. `Flex::new` checks that
frozen state before any GPIO gate, unlock or pad write, and panics for a reserved
pad. Safe digital, analog and peripheral pin construction through Flex therefore
cannot repurpose a reserved pad. Ordinary disconnected/disabled LSE at POR does
not reserve pads. Bypass without an applicable lock leaves OSC32_OUT usable.

## Package facts and limits

L010 uses PB1 for OSC32_IN and PB0 for OSC32_OUT. All other LSE-bearing families
use PC14 and PC15. Generation projects the existing own-datasheet `OSC32_IN` and
`OSC32_OUT` rows only where the selected package bonds them. Package-less aliases
receive only pads shared by all their applicable packages. Unbonded pads remain
absent, including F020 QFN20/QFN32, smaller F030 packages and several L031 packages.
L031 QFN20 does bond both LSE pads even though it has no qualified HSE route.

`clock_limits.lse` records LSE presence and only the two pad-lock applicability
facts. It is separate from `hse`; L012 is protected while its inherited-HSE
rejection remains intact. Generation checks real CR1/LSE registers, readable
access, field widths/positions and native getters against the selected PAC IR.
No raw peripheral addresses, register access adapters or chip-fact cfg predicates
are added.

Direct public PAC writes, unsafe stolen pins, DMA/NMI mutation and enabling LSE
after a GPIO driver already owns its pad remain outside HAL ownership guarantees.
Future supported LSE start or mode APIs must acquire any additional pads before
changing hardware; there is no such API here. No source-loss recovery,
low-power-resume or hardware-execution validation is claimed.

## Own-source evidence

[`cw32-data/lse-ownership.yaml`](../cw32-data/lse-ownership.yaml) records each
family's own RM/DS pages and the three lock policies, including explicit F002/F003
absence. [`sources/SOURCES.md`](../sources/SOURCES.md) identifies the original
manuals. The exact original identities, 37 catalog selectors, 54 generated
selectors, SDK field corroboration and independently read PDF tables are retained
in [`lse-pad-source-evidence.json`](lse-pad-source-evidence.json). R031/W031 use
their own MCU manuals; no RF qualification is added. A030's shared x030 RM
explicitly names both F030/A030, and A030 uses its own datasheet for pad facts.

`python3 ci/verify-lse-ownership-data.py` checks original source bytes and the
source/data/package projection. It does not execute HAL code or hardware.
