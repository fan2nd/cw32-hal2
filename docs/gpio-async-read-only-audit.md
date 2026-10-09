> Historical design audit. The subsequent implementation and own-manual image review are recorded in [gpio-async-expansion.md](gpio-async-expansion.md). L011's latest own manual has now been acquired. The earlier inference that sparse ICR reset values define the write-one no-op mask was incorrect: documented PIN fields define the no-op command width (8 bits for F002/F003, 16 bits for the other families). Physical/safe-pin masks remain separate. Read the implementation report for current validation status.

# GPIO asynchronous interrupt expansion: read-only next-batch audit

Date: 2026-10-08. Status: **design/evidence only; not implemented or accepted**.
The stage-5 code, generated data, feature gates, and tests were not changed or
regenerated for this audit. Existing asynchronous GPIO remains enabled only for
F030/A030. Nothing in this document claims silicon validation.

Subsequent bounded correction: the independently authorized
[GPIO ISR access correction](gpio-isr-access-corrections.md) fixes the confirmed
F002/F003/L010/L052 writable ISR metadata only. This design audit remains
unimplemented; the L011 and interrupt-mask evidence gaps below remain open.

## Findings that affect implementation

1. There are two trigger-register policies, not thirteen independent engines:
   edge plus level on F030/A030/F020, F002/F003, L052/L083; edge-only on
   L010/L011/L012/L031/R031/W031. The latter six must never touch reserved
   offsets `+0x2c` and `+0x30`.
2. IRQ ownership is a **group of banks**, not invariably one bank. L052 shares
   C/D; L083 shares C/D and E/F. An ISR must inspect software active state
   before accessing a bank, including when an IRQ fires with no HAL wait.
3. Every acquired manual specifies ICR as **R1W0**. Use a constant, bounded
   no-op mask with zeros only for the owned completed/cancelled pins. A package
   mask or bidirectional-GPIO mask is not an ICR no-op mask: it would clear
   input-only BOOT/reset pins or RF-owned flags as a side effect.
4. L011 has no acquired user manual; its SDK establishes edge-only registers,
   clear polarity, and IRQ names, but not independently audited reserved-bit
   masks for its derived GPIOB/C blocks. L012's ICR reset table conflicts with
   its full-width field table/header/pad set. These are explicit evidence gaps,
   not permission to guess a sparse mask.
5. The existing per-pin active/fired/waker engine is reusable. The current
   hardcoded `PortInterrupt::PORT`, four-bank state array, register enum
   matching, and native high/low enable writes are not portable unchanged.

## IRQ map checked against primary sources and generated metadata

Numbers below are external IRQ numbers, not vector-table slot numbers. IRQ 5,
6, 7, and 8 occupy vector slots 21, 22, 23, and 24 respectively. All **54**
currently generated chip records across **13** families were read: each
family's package records agree with its family map. This was an inventory
comparison, not execution of their firmware.

| Family | Actual IRQ groups | Trigger hardware | Current register policy |
|---|---|---|---|
| F030 | A=`GPIOA/5`, B=`GPIOB/6`, C=`GPIOC/7`, F=`GPIOF/8` | Rise/fall/high/low | `gpio_v1`, `gpioc_v1`, `gpiof_v1` |
| A030 | Same groups as F030, explicitly shared x030 manual | Rise/fall/high/low | Same; documentary shared map, no separate A030 SVD |
| F020 | A=`GPIOA/5`, B=`GPIOB/6`, C=`GPIOC/7`, F=`GPIOF/8` | Rise/fall/high/low | Same GPIO blocks as x030; independent F020 clock backend |
| F002 | A=`GPIOA/5`, B=`GPIOB/6`, C=`GPIOC/7` | Rise/fall/high/low | `gpio_cw32f002_v1`, `gpioc_cw32f002_v1` |
| F003 | A=`GPIOA/5`, B=`GPIOB/6`, C=`GPIOC/7` | Rise/fall/high/low | Same audited register versions as F002 |
| L010 | A=`GPIOA/5`, B=`GPIOB/6` | Rise/fall only | `gpio_cw32l010_v1`, `gpiob_cw32l010_v1` |
| L011 | A=`GPIOA/5`, B=`GPIOB/6`, C=`GPIOC/7` | Rise/fall only | `gpio_cw32l011_v1` |
| L012 | A=`GPIOA/5`, B=`GPIOB/6`, C=`GPIOC/7`, F=`GPIOF/8` | Rise/fall only | `gpio_cw32l012_v1` |
| L031 | A=`GPIOA/5`, B=`GPIOB/6`, C=`GPIOC/7`, F=`GPIOF/8` | Rise/fall only | `gpio_cw32l031_v1` |
| R031 | Same MCU groups as L031 | Rise/fall only | `gpio_cw32l031_v1`; separate RF ownership exclusions |
| W031 | Same MCU groups as L031 | Rise/fall only | `gpio_cw32l031_v1`; separate RF ownership exclusions |
| L052 | A=`GPIOA/5`, B=`GPIOB/6`, C+D=`GPIOC_GPIOD/7`, F=`GPIOF/8` | Rise/fall/high/low | `gpio_cw32l052_v1` |
| L083 | A=`GPIOA/5`, B=`GPIOB/6`, C+D=`GPIOC_GPIOD/7`, E+F=`GPIOE_GPIOF/8` | Rise/fall/high/low | `gpio_cw32l083_v1` |

Primary IRQ references: each official SDK `Libraries/inc/cw32*.h` IRQn enum;
exact locations and download links are below. L052 UM interrupt table, printed
p95, and L083 UM interrupt table, printed p104, explicitly identify the shared
groups. No `GPIOE` group exists on L052. Its SDK GPIO header happens to contain
a `GPIOE_INTFLAG_CLR` macro, which is not evidence of an implemented bank.

Reviewed generated records: `cw32-data/data/chips/*.json`, each core's
`interrupts` and GPIO peripheral `interrupts/GLOBAL`; register definitions in
`cw32-data/data/registers/gpio*.json`; source lineage in `cw32-data/inputs/`
and `cw32-data/register-source-aliases.yaml`. Future code generation should
derive pin-to-IRQ bindings from those reviewed GLOBAL links, with explicit
assertions for this map, rather than infer an IRQ name from the port letter.

## Register semantics and distinct masks

Common offsets are RISEIE `0x24`, FALLIE `0x28`, ISR `0x34`, ICR `0x38`,
FILTER `0x40`, and IDR `0x50`. Native HIGHIE/LOWIE, where implemented, are
`0x2c`/`0x30`. Enable bits are active-high, reset zero. ISR bit one means an
enabled condition was detected; zero means no detected enabled interrupt.
The trigger types for a pin share one pending flag. ISR is read-only in the
available manuals, even where the imported SVD/PAC allows writes.

ICR bit zero clears the corresponding pending flag; bit one is a no-op.
No ISR write and no ICR read/modify/write are needed. The intended command is
`implemented_interrupt_mask & !owned_clear_mask`, with reserved bits zero.
This must preserve every implemented foreign bit, regardless of pin-token
availability. Enable-register read/modify/writes occur under the same critical
section as software state changes and change only owned pin bits.

| Family/group | Implemented interrupt no-op mask by bank | Evidence and caveat |
|---|---|---|
| x030, F020 | A/B=`ffff`, C=`e000`, F=`00cb` | UM ICR reset masks, sparse CMSIS/SVD blocks; includes PF3 |
| F002/F003 | A/B=`00ff`, C=`003f` | Own UM ICR tables and distinct C header block; F002's absent external pads do not narrow its A/B/C register fields |
| L010 | A=`01ff`, B=`00ff` | Own UM ICR reset values and narrow CMSIS fields; includes input-only PB7 |
| L011 | A/B/C fieldsets each expose `ffff`; sparse B/C implemented mask is **not independently established** | Own SDK/SVD derives B/C from A; physical GPIO masks A=`ffff`, B=`00fb`, C=`e000` are not proof of reserved-bit behavior |
| L012 | A/B full-width fields imply `ffff`; C=`e000`, F=`00cb`; A/B reset table conflict must be resolved explicitly | UM §9.6.11 prints A=`01ff`, B=`00ff`, but §§9.6.8–11 define PIN0–15 and own CMSIS fields do too; A/B pads through 15 are documented |
| L031/R031/W031 | A/B=`ffff`, C=`e000`, F=`00cb` | Each family's own UM §9.6.11; RF/package exclusions must not narrow this mask |
| L052 | A/B/C=`ffff`, D=`0004`, F=`00fb` | Own UM §9.6.13, printed p155; includes input-only PF3 |
| L083 | A/B/C/D/E=`ffff`, F=`07ff` | Own UM §9.6.13, printed p167; includes input-only PF3 and family-union F bits absent on some packages |

For L012, using the printed narrow A/B reset values as the W0C command mask
would write zero to every PA9–PA15/PB8–PB15 flag whenever another pin clears.
The bit-field definition, CMSIS, and pad list support a full-width candidate;
record that reconciliation and corroborate the contradictory reset table
before claiming the mask fully verified. Do not fabricate a corrected vendor
reset value. L011 needs its own documented reconciliation or vendor evidence
for B/C reserved masks; neither L010 nor L012 is its user manual.

The SDK `GPIOx_INTFLAG_CLR` macros for all twelve separately acquired SDKs
write an unrestricted 32-bit complement. They corroborate W0C polarity but
are not permission to write ones into reserved bits. In particular, a generic
`!mask` is not the proposed implementation.

Four different concepts must remain separate:

- Implemented IRQ bits: for pending masking and ICR no-op writes.
- Externally usable family pads: excludes RF-internal/nonexistent pads.
- Selected-package pads: controls which singleton tokens exist.
- Safe public pads: excludes default SWD/reset/input-only pads under today's
  `Pin` contract. EXTI expansion does not itself broaden that contract.

## GPIO clocks, filters, and special pads

Manual programming sections require enabling a bank's configuration/working
clock before GPIO configuration. The existing `Input` constructor already
owns the pin, enables its bank clock, and leaves that clock enabled. Reuse
that lifecycle; do not reset or disable a shared bank at construction/drop.

| Family | AHBEN bank gate bits | Write policy |
|---|---|---|
| F030/A030/F020 | A4/B5/C6/F9 | Unkeyed |
| F002/F003 | A4/B5/C6 | Unkeyed |
| L010 | A4/B5 | Upper-half `5a5a` key |
| L011 | A4/B5/C6 | Upper-half `5a5a` key, own SDK evidence |
| L012 | A4/B5/C6/**F7** | Upper-half `5a5a` key; UM §4.7.11 |
| L031/R031/W031 | A4/B5/C6/F9 | Unkeyed |
| L052 | A4/B5/C6/D7/F9 | Unkeyed; E8 is not a GPIO bank |
| L083 | A4/B5/C6/D7/E8/F9 | Unkeyed |

An L052 D-only wait must not read clock-gated C just because C/D share an IRQ;
likewise either side of L083 E/F. Software-active bits are checked in RAM
first, under a critical section, before constructing/accessing that bank's
MMIO. No-active-bank and no-active-pin paths return without GPIO reads or
writes. A newly created `Input` without an active wait is not an excuse to
scan every other bank. Bank clock lifetime is guaranteed by owned drivers;
raw PAC users must not gate/reset them behind the driver. A live AHBEN check
can diagnose a violated contract but does not make concurrent external clock
changes safe, and the handler must not silently enable a foreign bank.

Do not rewrite shared FILTER.FLTCLK or silently assume reset filter state.
The manuals' GPIO interrupt sections explain that all pins within one bank
share that clock selector. Preserve existing per-pin filter controls and the
shared selector in the initial expansion, documenting that inherited filter
settings affect latency/pulse acceptance. A stopped timer-selected filter
can prevent events; initialization must require a working configured source
or the board owner must explicitly disable/configure that pin's filter.
A future filtering API requires separate per-bank clock ownership. Hardware
deep-sleep wake capability is documented, but this expansion does not promise
deep-sleep clock management or an Embassy low-power executor integration.

Keep existing safe-pin exclusions:

- F002/F003: PA2/PA5 are SWD, PC5/NRST needs explicit reset remapping.
  The GPIO backend deliberately avoids SYSCTRL.CR2; any CR2 write locks its
  reset-remapping choice until POR on these families.
- L010: PA7/PA8 are SWD; PB7/NRST remains input-only even when remapped.
- L011/L012: PA13/PA14 are SWD; NRST is dedicated. L012 PF3/BOOT is genuinely
  bidirectional and already has a safe token; only PF3 supports Pull::Down.
- x030/F020/L031/R031/W031/L052/L083: PA13/PA14 are SWD and PF3/BOOT is
  input-only under the current GPIO contract. Its pending flag still exists
  and must be preserved by ICR commands for other pins.
- Oscillator pads require board-level exclusivity. GPIO waits must not stop
  or remap HSE/LSE, SWD, reset, or BOOT functions.
- R031 PA0–PA3 are internal RF SPI connections (DS V1.2 §4.4.5, Table 4-4,
  printed p15). W031 PB3/PB4/PB5/PB13 are internal RF SPI and **PB6 is RF
  IRQ** (DS V1.3 §4.4.4, Table 4-2, printed p13). No generic `ExtiPin` is
  generated for them. RF GPIO1/3/10/11 labels are not MCU port numbers.

For example, clearing W031 PB7 must leave PB6 pending intact, even though no
safe PB6 token exists. If an RF handler owns PB6, it must be explicitly bound
alongside the GPIO handler to the same `GPIOB` vector and service that bit.
Merely ignoring PB6 does not prevent an interrupt storm when RF leaves it
enabled and pending.

## Proposed Embassy API and implementation split

Keep `ExtiInput::new(Peri<'d, T>, Pull, Binding<...>)` and its exclusive
`&mut self` waits. Generate `ExtiPin::Interrupt` from each selected pin's
reviewed bank GLOBAL link. PC and PD on L052/L083 must require
`Binding<GPIOC_GPIOD, InterruptHandler<GPIOC_GPIOD>>`; PE/PF on L083 require
the analogous `GPIOE_GPIOF` binding. AnyPin intentionally loses that static
association; do not add a safe erased-pin constructor that bypasses binding.

Replace the one-bank assumption with a sealed `GpioInterrupt`/group policy
that provides a bounded set of bank IDs. Keep one active/fired mask and one
waker slot per pin **per bank**, not one combined 16-bit mask per IRQ. A C2
wait and D2 wait are different resources. A group handler dispatches every
active member bank and performs no MMIO for inactive ones. Current
`bind_interrupts!` already permits multiple typed handlers on one vector;
do not generate duplicate vector symbols for each bank.

Use the existing reviewed GPIO register backend for pin configuration/gating
and small typed IRQ access operations. Keep common future/state algorithms
separate from trigger-register and group/mask policies. Appropriate proposed
module paths, all following the requested module-directory convention:

- `embassy-cw32/src/exti/mod.rs`: public API and binding traits.
- `embassy-cw32/src/exti/engine/mod.rs`: active/fired/waker and cancellation.
- `embassy-cw32/src/exti/registers/mod.rs`: reviewed PAC dispatch and ICR masks.
- `embassy-cw32/src/exti/irq_groups/mod.rs`: derived/checked group membership.
- `embassy-cw32/src/exti/tests/mod.rs`: behavioral and MMIO tests; any split
  test modules also use their own `.../mod.rs`.

Reuse hardware-meaningful `gpio_exti` and existing `gpio_*_v1` backend cfgs;
if capability cfgs are needed, `gpio_irq_level` and `gpio_irq_shared_banks`
describe actual hardware. Avoid `hal_*`, acceptance-stage names, or vague
compatibility labels. Software can emulate level waits when
`gpio_irq_level` is absent; that must not pretend native level registers exist.
An enabled capability requires reviewed map/mask evidence and contract tests,
not merely a PAC block with a familiar name.

### Per-pin wait state machine

1. Public async methods are lazy: constructing and dropping an unpolled
   future must not arm anything. On first execution, under one critical
   section, disable only this pin's supported trigger enables, clear its
   stale pending flag, discard its old fired bit, mark it active, and enable
   the selected hardware triggers last. Preserve all partners throughout.
2. Register the task waker before testing completion. Latch fired state
   before waking, so an interrupt between arming and waker registration or
   between registering and checking cannot be lost. Wake outside the
   state-mutating critical section.
3. Native high/low-capable policy uses HIGHIE/LOWIE and an after-arm IDR
   check for immediate completion. Edge-only policy uses rising for high,
   falling for low, plus the same after-arm IDR check. Rise/fall/any-edge
   waits never complete solely because the input is already at a level.
4. ISR completion is `pending & active & implemented_interrupt_mask` for
   that bank. Disable only completed pins before clearing W0C flags, remove
   them from active, add to fired, then wake the corresponding wakers.
   For a native level source, disabling before clearing prevents a held
   level from immediately retriggering this owned wait.
5. Cancellation and drop disable/clear only that pin, remove both its active
   and fired state, and permit clean rearming. Completion cleanup must occur
   even for the immediate-level path. The public async wrapper currently
   drops its inner future on completion; retain that guarantee or clean up
   directly when returning Ready. A retained completed internal future must
   not leave its trigger armed by accident.

The pinned `embedded-hal-async` 1.0.0 `digital::Wait` documentation explicitly
allows a high/low pulse to have ended before the awakened task runs: a latched
matching edge must still resolve the wait. Do not recheck the current level
and discard that completion. Conversely an any-edge pending flag cannot by
itself establish which level was reached; high/low emulation arms only the
matching edge. Tests must inject edges at each arm/register/check boundary.
This is a one-shot condition wait, not an edge counter or pulse-width meter.
Multiple edges coalesce in one hardware pending bit. Minimum pulse width,
IDR synchronizer delay, and inherited digital filtering remain hardware limits.

## Bootloader and external-owner boundary

Typed `Peri` and `Binding` protect safe Rust use, but do not establish ownership
against an already-running bootloader, a relocated vector table, a debugger,
or raw/unsafe PAC users. Before application handover, every GPIO IRQ source
must either be owned by a handler in the installed application vector or be
disabled by its owner. A legacy enabled flag outside the HAL active set is
preserved, not adopted, and can keep the group IRQ asserted forever.

Constructors may disable/clear their **owned pin** and enable the already-bound
NVIC line. They must not unpend the group, disable it on cancellation/drop,
reset a port, clear all pending flags, change partner priorities, or replace
an unrelated handler to hide that situation. Preserve priorities unless the
application explicitly configures them. An already-pending shared vector
may fire immediately after enable; the guard must tolerate no active waits.

An external/RF handler sharing a bank or group must follow disjoint pin
ownership, service its own enabled sources, leave HAL state/flags untouched,
and keep shared clocks/filter selectors valid. The GPIO handler does not
promise to manage or recover unsupported external owners. Leaving unknown
sources intact is an ownership guarantee, not a guarantee against storms.
Board startup must arrange NVIC/vector/clock handover before concurrently
running application handlers; `Binding` alone cannot prove this hardware state.

## Required next-batch test matrix

No new tests were added or run for this read-only audit. These are acceptance
requirements for a later implementation, after the current freeze is lifted.

| Layer | Required cases |
|---|---|
| Generated map | All 54 current chip records / 13 families; GLOBAL links and external IRQ numbers equal primary evidence; no nonexistent E on L052; preserve A030 shared-source disclosure |
| Typed API | Every safe selected-package pin constructs with its real group binding; wrong vector/handler fails for the intended diagnostic; erased AnyPin cannot satisfy ExtiPin; repeated/mutable Peri ownership conflicts fail |
| Package exclusions | SWD/reset/input-only/RF/unbonded pins remain absent; L012 PF3 positive; all current package feature selections, including sparse L083 VCT6/MCT6 layouts |
| Edge/level policy | Rising, falling, both; immediate high/low; high/low pulse ends before poll; no writes at +2c/+30 on every edge-only PAC; no level-only completion for edge waits |
| State races | IRQ before first waker registration, after registration/before fired check, after check/before Pending return; replaced waker; unrelated flag; stale flag at arm; cancellation before/after IRQ; repeat arm/cancel; unpolled future drop |
| Same bank | Two/many active pins, different triggers; completing or cancelling one retains all partners, pending flags, enable bits, and wakers |
| Shared banks | L052 C-only, D-only, C+D; L083 C/D and E/F separately and together; equal bit index across banks; simultaneous completions; cancelled bank while partner stays active |
| Clock guard | Instrumented MMIO rejects *any* inactive-bank read/write; all banks inactive, only unbonded group partner, stale NVIC pending, externally clock-gated idle partner; no implicit clock enabling inside ISR |
| Clear protocol | W0C simulator with per-family masks, same-bank foreign pending arriving around clear; preserve PF3/PB7/RF pending; reserved upper/lower bits zero; no ISR writes or W1C assumptions |
| Configuration preservation | No group NVIC unpend/disable; no reset, priority changes, FILTER selector changes, partner enables, SWD/NRST/oscillator remap; preserve GPIO latch/pulls of other pins |
| Foreign owner | Deliberately foreign enabled/pending source is not cleared/disabled by HAL; composite binding invokes foreign handler; unsupported bootloader owner documented as a storm boundary rather than silently repaired |
| Actual PAC | Aligned-RAM accesses for each unique GPIO block prove offsets and command values; access-log/protocol model proves W0C and gating behavior, since RAM cannot simulate those silicon effects |
| ARM/link | All selected features, `rt` and `rt,defmt` library/API checks; representative exact-package executable per IRQ/register policy, ELF vector slot identity and memory limits; existing full stage regression after changes |
| Board validation | At least one device per register/IRQ policy; rising/falling/short-pulse capture; held-level reassertion; concurrent shared-bank waits; cancel/rearm; filter timing; legal bootloader handover; separate RF coexistence trial before claiming RF integration |

Test evidence must distinguish software-model/PAC-on-RAM, cross-compiled,
linked, and physically executed results. A build or pending-bit RAM test is
not a hardware interrupt-delivery or low-power wake test.

## Exact primary references

The manual links below identify the acquired local documents under
`/workspace/shared/cw32-sources/`; page numbers are printed pages.

| Family | Primary behavioral source and sections |
|---|---|
| F030/A030 | [x030 UM CN V2.5](https://www.whxy.com/uploads/files/20240920/CW32x030_UserManual_CN_V2.5.pdf), §§9.3.6 (p148), 9.4, 9.6.10–15 (pp155–156); common F030/A030 scope explicitly stated |
| F020 | [UM CN V1.4](https://www.whxy.com/uploads/files/20240920/CW32F020_UserManual_CN_V1.4.pdf), §§9.3.6 (p145), 9.4, 9.6.10–15 (pp152–153) |
| F002 | [UM CN V1.4](https://www.whxy.com/uploads/files/20240920/CW32F002_UserManual_CN_V1.4.pdf), §§8.3.6 (p105), 8.4, 8.6.7–12 (pp111–112) |
| F003 | [UM CN V2.3](https://www.whxy.com/uploads/files/20240920/CW32F003_UserManual_CN_V2.3.pdf), §§8.3.6, 8.4, 8.6.7–12 (pp113–114) |
| L010 | [UM CN V1.2](https://www.whxy.com/uploads/files/20260626/CW32L010_UserManual_CN_V1.2.pdf), §§8.3.6 (p123), 8.4, 8.6.7–10 (pp127–128) |
| L011 | [DS CN V1.1](https://www.whxy.com/uploads/files/20250724/CW32L011_DataSheet_CN_V1.1.pdf), §4.7 and Table 5-2; own SDK GPIO source/header and CMSIS/SVD; no acquired UM |
| L012 | [UM CN V1.4](https://www.whxy.com/uploads/files/20260603/CW32L012_UserManual_CN_V1.4.pdf), §§9.3.6 (p130), 9.4, 9.6.8–11 (pp135–136), §4.7.11 (p56); contradictory ICR reset table expressly retained above |
| L031 | [UM CN V1.6](https://www.whxy.com/uploads/files/20240920/CW32L031_UserManual_CN_V1.6.pdf), §§9.3.6 (p141), 9.4, 9.6.8–11 (pp147–148) |
| R031 | [UM CN V1.3](https://www.whxy.com/uploads/files/20240920/CW32R031_UserManual_CN_V1.3.pdf), §§9.3.6 (p143), 9.4, 9.6.8–11 (pp149–150); [DS CN V1.2](https://www.whxy.com/uploads/files/20251230/CW32R031_DataSheet_CN_V1.2.pdf), §4.4.5/Table 4-4 |
| W031 | [UM CN V1.4](https://www.whxy.com/uploads/files/20240920/CW32W031_UserManual_CN_V1.4.pdf), §§9.3.6 (p142), 9.4, 9.6.8–11 (pp148–149); [DS CN V1.3](https://www.whxy.com/uploads/files/20251230/CW32W031_DataSheet_CN_V1.3.pdf), §4.4.4/Table 4-2 |
| L052 | [UM CN V1.5](https://www.whxy.com/uploads/files/20240920/CW32L052_UserManual_CN_V1.5.pdf), interrupt table p95; §§9.3.6 (p147), 9.4, 9.6.8–13 (pp153–155) |
| L083 | [UM CN V2.0](https://www.whxy.com/uploads/files/20240920/CW32L083_UserManual_CN_V2.0.pdf), interrupt table p104; §§9.3.6 (p159), 9.4, 9.6.8–13 (pp165–167) |

Each SDK path below is relative to its extracted `Libraries/` directory. IRQ
line numbers refer to `inc/cw32{family}.h`; clear macro lines refer to
`inc/cw32{family}_gpio.h`. Corresponding `src/cw32{family}_gpio.c`
`GPIO_Init` paths independently show the supported trigger writes.

| Family | SDK source | IRQ enum lines | Clear macro lines |
|---|---|---:|---:|
| F030/A030 | [V2.2](https://www.whxy.com/uploads/files/20241111/CW32F030_StandardPeripheralLib_V2.2.zip) | 72–75 | 98–101 |
| F020 | [V1.2](https://www.whxy.com/uploads/files/20240115/CW32F020_StandardPeripheralLib_V1.2.zip) | 72–75 | 98–101 |
| F002 | [V1.2](https://www.whxy.com/uploads/files/20240115/CW32F002_StandardPeripheralLib_V1.2.zip) | 48–50 | 88–90 |
| F003 | [V1.7](https://www.whxy.com/uploads/files/20250606/CW32F003_StandardPeripheralLib_V1.7.zip) | 70–72 | 89–91 |
| L010 | [V1.0.9](https://www.whxy.com/uploads/files/20260806/CW32L010_StandardPeripheralLib_V1.0.9.zip) | 71–72 | 93–94 |
| L011 | [V1.0.3](https://www.whxy.com/uploads/files/20251016/CW32L011_StandardPeripheralLib_V1.0.3.zip) | 70–72 | 93–95 |
| L012 | [V1.0.5](https://www.whxy.com/uploads/files/20260701/CW32L012_StandardPeripheralLib_V1.0.5.zip) | 70–73 | 93–96 |
| L031 | [V1.4](https://www.whxy.com/uploads/files/20250721/CW32L031_StandardPeripheralLib_V1.4.zip) | 71–74 | 111–114 |
| R031 | [V1.1](https://www.whxy.com/uploads/files/20240115/CW32R031_StandardPeripheralLib_V1.1.zip) | 71–74 | 111–114 |
| W031 | [V1.3](https://www.whxy.com/uploads/files/20240119/CW32W031_StandardPeripheralLib_V1.3.zip) | 66–69 | 111–114 |
| L052 | [V1.4](https://www.whxy.com/uploads/files/20260309/CW32L052_StandardPeripheralLib_V1.4.zip) | 71–74 | 111–116 |
| L083 | [V2.2](https://www.whxy.com/uploads/files/20240821/CW32L083_StandardPeripheralLib_V2.2.zip) | 71–74 | 112–117 |

SDK archive/source hashes are already recorded in
`/workspace/shared/cw32-sources/CW32*-manifest.json` and repository source
inventories. The L011 SVD examined is
`cw32l011/IDEsupport/MDK/WHXY.CW32L011_DFP.1.0.1/SVD/CW32L011.svd`:
GPIOB and GPIOC are `derivedFrom="GPIOA"`, with 16-bit IRQ fieldsets and no
independent bank-specific mask documentation there.

## Source-quality items for the later batch

- At audit time, imported `gpio_cw32f002_v1`, `gpioc_cw32f002_v1`,
  `gpio_cw32l010_v1`, `gpiob_cw32l010_v1`, and
  `gpio_cw32l052_v1` ISR entries defaulted to ReadWrite despite their own
  manuals marking ISR RO. The subsequent bounded correction linked above
  fixes these through the reviewed input/override pipeline. L011's imported
  ISR remains ReadWrite; its missing UM prevents extending that correction
  without its own explicit evidence rationale. Never write ISR.
- L011 B/C reserved masks and L012 A/B reset values require reconciliation
  before promoting complete all-family interrupt-mask verification.
- Generic wide PAC fieldsets on L031/L012/L052/L083 do not establish sparse
  bank masks. Preserve the explicit family/bank policy derived above.
- The current x030 handler reads ISR before checking active state. Retain
  its tested per-pin algorithm but move the RAM-active guard before MMIO
  in the future refactor, including the original x030 regression coverage.
- No clock-gated MMIO fault behavior, filter timing, IRQ wake propagation,
  special-pad remapping, or RF coexistence has been physically verified here.

Suggested order: refactor and regress x030; add F020/F002/F003 with explicit
mask policies; add manual-supported edge-only L010/L031/R031/W031; add shared
L052/L083 with guarded group dispatch; complete L011/L012 after their source
ambiguities are documented/resolved. This is sequencing advice, not an
authorization to modify the frozen acceptance candidate.
