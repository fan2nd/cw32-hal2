> Historical software-verification references: HAL tests and fixture harnesses cited below were deleted on 2026-10-08. Their reported results apply to the dated source snapshot only; source facts remain unchanged. See [current HAL layout and build scope](hal-production-layout.md).

# Typed asynchronous GPIO expansion

Date: 2026-10-08. This implementation is software-validated; no board, RF
coexistence, low-power wake or clock-gated bus behavior has been physically tested.

## Enabled scope and resolved source questions

The EXTI engine supports all thirteen families: F030/A030/F020, F002/F003,
L010/L011/L012, L031/R031/W031, L052 and L083. Earlier L011/L012 withholding
was removed only after inspecting their own register-table PDF images.

L011's own latest [User Manual CN Rev 1.1](https://www.whxy.com/uploads/files/20260602/CW32L011_UserManual_CN_V1.1.pdf),
section 8.6.10, printed p128 (PDF page 129), explicitly gives bits 15:0 as
PINy, y=0..15, R1W0 with W1 having no effect; only bits 31:16 are RFU and must
stay zero. SHA-256: `b245887e9037caf267f739e25bd3c354c2579d23315fc06389e055f371159f4f`.
The previously acquired September 2025 copy has identical GPIO text. The
manual also independently marks ISR read-only in 8.6.9. No L010 manual or
cross-family alias is used to establish this policy.

L012's own [User Manual CN V1.4](https://www.whxy.com/uploads/files/20260603/CW32L012_UserManual_CN_V1.4.pdf),
section 9.6.11, printed p136 (PDF page 162), specifies the same 16-bit W1
no-op command field for A/B/C/F. SHA-256:
`a9e54694a26f03c1f3e3041f40844900168ca2b8e6142f3328671b92e6b7a340`.
Its independently acquired English V1.0 also retains this field definition
and the same sparse reset values.

Both manuals still print ICR reset A=`01ff`, B=`00ff`, C=`e000`; L012 also
prints F=`00cb`. These values are preserved as vendor evidence, not silently
changed or presented as corrected values. A reset value is not a W1 no-op
command mask. The explicit field semantics permit `0xffff & !owned` on each
listed bank, with upper reserved bits zero. This preserves every documented
foreign flag, including A9..15/B8..15, without claiming every field represents
a physical or bonded pad. SDK full-complement clear macros corroborate W0C,
but are not the reason for writing reserved upper bits: this HAL writes none.

The [read-only audit](gpio-async-read-only-audit.md) remains historical. Its
unresolved L011/L012 status and reset-derived command-mask table are superseded here; it retains the other exact
primary URLs, manual sections, SDK versions and silicon-validation caveats.
The confirmed ISR read-only metadata corrections are maintained by the source
pipeline; this implementation performs no ISR writes on any backend.

## Register and IRQ policies

| Family | W0C no-op mask per bank | GPIO IRQ groups | Level trigger |
|---|---|---|---|
| F030/A030/F020 | all banks ffff | A, B, C, F | native |
| F002/F003 | all banks 00ff | A, B, C | native |
| L010 | all banks ffff | A, B | matching edge + IDR |
| L011 | A/B/C ffff | A, B, C | matching edge + IDR |
| L012 | A/B/C/F ffff | A, B, C, F | matching edge + IDR |
| L031/R031/W031 | all banks ffff | A, B, C, F | matching edge + IDR |
| L052 | all banks ffff | A, B, C+D, F | native |
| L083 | all banks ffff | A, B, C+D, E+F | native |

Each command mask follows its reviewed ICR no-op fields, independently of
reset values, physical pad availability, package bonding, output capability,
public safe-pin masks and RF exclusions. Serviced pending-source masks and clear no-op masks have separate named
policies. In particular, sparse pending masks must not narrow the W1 no-op
command field. ICR is
written with `clear_noop_mask & !owned`; it is never read/modified, and ISR is
never written. This preserves input-only PF3, L010 PB7 and RF-owned flags,
including W031 PB6, even though their safe tokens are unavailable. Reserved
bits receive zero. Trigger modifications touch only owned pin bits under a
critical section. Edge-only backends never read/write reserved HIGHIE/LOWIE
locations at offsets 0x2c and 0x30.

Independent review found that the reset-vs-command distinction also applies
outside L011/L012. All twelve separately published own-family manuals define
ICR PIN0..15 W1 as no-op, except F002/F003 which define PIN0..7; x030 expressly
covers A030 as well. This correction is applied consistently to every bank,
including x030 C/F, F002/F003 C, L010 A/B, L031-family C/F, L052 D/F and L083 F.
The sparse serviced-source masks remain distinct and never grant extra safe pin
tokens. Foreign flags outside those sparse masks still receive W1, not W0.
No generated sparse PAC accessor width is treated as permission to override
an explicit own-manual command field. The full command is written through the
selected bank's real raw PAC register wrapper; reserved upper fields stay zero.

Source-image checks: x030 §9.6.15 printed p156; F020 §9.6.15 p153;
F002 §8.6.12 p112; F003 §8.6.12 p114; L010 §8.6.10 p128;
L011 §8.6.10 p128; L012 §9.6.11 p136; L031 §9.6.11 p148;
R031 §9.6.11 p150; W031 §9.6.11 p149; L052 §9.6.13 p155;
L083 §9.6.13 p167. Exact source hashes and URLs are recorded in
[the evidence inventory](gpio-async-evidence.json). The source audit
`tests/check_gpio_irq_sources.py` checks the retained PDFs/text against it.

The original typed PAC enum has been removed by the
[metadata-driven EXTI refactor](gpio-exti-metadata.md). The PAC still retains
its actual C/F/B block types. The HAL now uses its selected family's direct
GPIO view only after proving each real bank's accessed offsets, widths,
permissions and serviced fields against that view. A shared register-version
name alone remains insufficient evidence.

Pin `ExtiPin::Interrupt` implementations are generated from reviewed metadata
GLOBAL links, not guessed from port letters. C/D pins require the complete
`InterruptHandler<GPIOC_GPIOD>` binding; L083 E/F requires the complete
`InterruptHandler<GPIOE_GPIOF>` binding. Equal pin numbers in different banks
have separate active/fired masks and wakers. Every handler checks RAM active
state before constructing or accessing that bank's MMIO. Idle or absent-package
partners can remain clock-gated. The handler never enables another bank's clock.

## Ownership and lifecycle

`ExtiInput::new` retains `Peri` ownership and requires a typed binding. `AnyPin`
cannot satisfy the safe constructor. Public waits exclusively borrow the input
and are lazy: dropping an unpolled async future does not arm a source. First
execution disables and clears only the owned pin, discards stale completion,
marks it active, and enables the chosen trigger last. The waker is registered
before completion is checked. Interrupt completion is latched before waking.

Native high/low waits use level enable registers; edge-only high/low waits use
only the matching edge. Both read IDR after arming for an already-present level.
A matching pulse that ends before the next poll still resolves the wait. Edge
waits never resolve solely from IDR. Multiple edges may coalesce; this API is
not a pulse counter or minimum pulse-width guarantee.

Completion disables the pin before clearing its flag; this prevents a native
held level from immediately reasserting its owned source. Returning Ready
cleans up even if the internal future is retained. Cancellation and driver
drop disarm/clear only that pin. The constructor also clears its own stale
source before enabling the bound vector. NVIC pending state, enable state of
partners, priorities and vector ownership are otherwise preserved. The old
single-bank `PortInterrupt` trait remains available for existing generic code;
new shared vectors use `GpioInterrupt`.

The board must keep each owned bank's configuration/working clock available.
Inherited per-pin filtering and shared FILTER selector are preserved. The
selected filter source must run, or the board must explicitly configure/disable
that pin's filter. There is no implicit reset, clock remap, SWD/reset/oscillator
remap, shared filter clock change or deep-sleep management.

Typed tokens/bindings cannot establish exclusive ownership against a bootloader,
debugger, relocated vector table or raw PAC code. At handover, every enabled
source must be serviced in the installed application vector or disabled by its
owner. A foreign/RF handler may share a vector using a composite `bind_interrupts!`
binding if it owns disjoint pins and preserves HAL clocks/state/flags. Unknown
sources are neither adopted nor cleared; an unserviced foreign source can cause
an interrupt storm. This is an explicit integration obligation, not a promise
that the driver repairs unsupported bootloader/RF configurations.

## Validation layers

- Host engine tests use an instrumented W0C model with clock-gated access
  rejection, foreign arrivals around clear, matching short pulses, all sixteen
  slots, cancellation/rearming, replacement wakers, and IRQ injection before
  registration, after registration and after the completion check.
- Shared-group tests cover either/both active banks, identical bit indices,
  simultaneous completion and a cancelled/clock-gated idle partner.
- Real typed PAC-on-aligned-RAM tests run every implemented bank of the selected
  feature. They check pending/IDR offsets, enable/disable offsets, clear command
  values, untouched ISR, retained-completed-future cleanup, untouched reserved
  level-register locations and preservation of unrelated memory words. RAM
  checks cannot simulate W0C or interrupt delivery; the model is separate.
- L011/L012 tests exercise every documented bit on every bank, including
  upper-bit foreign pending preservation across arm, completion and cancellation.
- `tests/test_exti_hal_contracts.py` compiles actual Cortex-M0+ APIs for selected
  chip/package features, correct group bindings and every safe package pin,
  rt/defmt variants, ownership failures, erased pins, wrong bindings and
  incomplete shared-group handlers. It also checks the independent IRQ map
  against all generated chip records, including L011/L012 full-width command
  policies and the L012 PF3 positive constructor.

`tests/test_exti_example_links.py` links fifteen exact-package firmware
selections with rt and rt+defmt, instantiates one input per bonded bank, and
checks each installed GPIO vector against its ELF symbol and all loadable
segments against FLASH/RAM limits. It does not execute the binaries.

These checks establish source/API/model/build behavior only. Linking and board
execution, where performed, must be reported separately; neither a library
build nor a pending-bit RAM test proves silicon interrupt delivery.
