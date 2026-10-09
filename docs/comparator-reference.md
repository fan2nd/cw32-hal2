# Shared low-family comparator references

`vref::Vref` configures CW32L010/L011 VCDIV and CW32L012 VC12REF/VC34REF.
`Comparator::new_with_vref` compares a package-qualified external positive input
with its bank's internal divider. The reference bank, each comparator instance
and each external input retain normal Embassy peripheral ownership. The driver
uses typed PAC fields directly; no register adapter or HAL test model is added.

## Qualified subset

| Family | Bank | Consumers | Supply input | Alternative | Taps |
|---|---|---|---|---|---|
| L010 | VCDIV | VC1, VC2 | VDD | nominal 1.6 V Vcore | 1/8 through 8/8 |
| L011 | VCDIV | VC1, VC2 | VDDA | nominal 1.6 V Vcore | 1/8 through 8/8 |
| L012 | VC12REF | VC1, VC2 | VDDA | nominal 1.6 V Vcore | 1/8 through 8/8 |
| L012 | VC34REF | VC3, VC4 | VDDA | nominal 1.6 V Vcore | 1/8 through 8/8 |

DIV codes 0..7 implement `(DIV + 1)/8`. INN=3 selects the corresponding bank.
VDDA must equal VDD where present. Supply declarations are checked against each
family's own comparator limits: L010 1620..5500 mV; L011/L012 1700..5500 mV.
The comparator and its reference must declare the same board analog supply.
Neither declaration measures voltage, and Vcore is not a calibrated reference.
The board remains responsible for input, common-mode, temperature, offset,
hysteresis, response and settling requirements in its own datasheet.

The new `reference_divider` generated metadata records each bank's clock owner,
consumer pair, negative-input mux and electrical bounds. These facts originate
in `cw32-data/reference-dividers.yaml`, not a HAL-side chip table. Source/tap
encodings are authored PAC enums. Other chip families receive no Vref capability.

## Ownership and sequencing

The bank is immutable while owned. Compatible comparators borrow it for their
whole driver lifetime; generated `Reference<T>` implementations admit only the
bank's real consumers. A bank's borrow prevents safe reconfiguration, and both
comparators may share that unchanged selection. External-only comparator
construction still does not write reference registers.

Construction validates supply before any register write. It enables the existing
generated VC clock owner's configuration gate through central RCC, verifies the
gate and refuses a held VC reset. It never resets or releases reset. No standalone
divider kernel frequency or reset domain is fabricated: current clock metadata
correctly leaves those helper facts unestablished. The shared VC module gate is
explicitly described as its configuration clock and DIV/REF are listed in its
register block. Enabling the owner is a conservative register-access policy.

Before a reference write, L012 rejects an inherited unsupported DIV value, then
all variants inspect both consumers and reject any enabled comparator selecting
that reference (INN=3). This also protects a comparator whose Rust driver was
forgotten. A free bank is disabled, its source/tap are programmed, and then it is
enabled, always through typed read-modify-write fields. Unrelated/reserved bits
are preserved. Consumer constructors set only their own input/control registers
and configure their owned external input as analog before enabling comparison.

Reference drop leaves its divider and shared clock enabled. It performs no
register writes, so an enabled consumer cannot lose its reference on a forgotten
borrower's behalf. A future owner still checks active consumers before changing
it. Comparator drop only clears its own EN. Neither path changes ADC, BGR,
sibling comparators, another divider bank or shared reset. Retained analog power
is intentional; no low-power disable/reconfiguration API is exposed here.

## L012 field disagreement

Own CN manual Rev 1.4 printed p627 (physical PDF p653), sections 27.7.1/27.7.2,
visibly labels both divider fields DIV[3:0]. Its functional description on
printed p620 (physical p646) specifies only values 0..7. SDK V1.0.5's
`cw32l012.h` has a 3-bit member and `VCREF_DIV_DIV_Msk = 0x7`; its `VC_Divider`
enum also lists only 0..7. These sources disagree about the high bit.

The L012 authored PAC field now follows the own manual's four-bit register
shape. Its enum names only the documented eight ratios. The HAL exposes exactly
eight legal taps and returns `UnsupportedDividerState` for inherited DIV=8..15
before any reference write. It does not clear uncertain bit3 or assign a ratio
to undocumented values. L010/L011 remain separate three-bit fields with bit3
reserved. The rendered own-family pages were inspected during this change;
source PDFs/images are retained outside the source distribution.

## Evidence and limits

Canonical source URLs, revisions, byte counts and SHA256 pins remain in
`sources/evidence-sources.json`; new evidence pointers lead to the authored
reference catalog. Each catalog profile cites its own manual, datasheet and
pinned SDK/header/VC implementation, with exact printed/physical pages.

- L010 manual Rev1.2: §21.3.1 p528, §21.5 p533, §21.6 p534 and §21.7.1 p535;
  physical pages are one higher. VC gate/reset: §§4.7.12/4.7.15 pp79/82.
  Datasheet Rev1.3 §7.3.15 Table7-31 p51 (physical52).
- L011 manual Rev1.1: §21.3.1 p530, §21.5 p535, §21.6 p536 and §21.7.1 p537;
  physical pages are one higher. VC gate/reset: §§4.7.12/4.7.15 pp77/80.
  Datasheet Rev1.1 §7.3.15 Table7-30 p55 (physical58).
- L012 manual Rev1.4: §27.3.1 pp619–620, §27.5 p625, §27.6 p626 and
  §§27.7.1–27.7.2 p627; physical pages are 26 higher. VC gate/reset:
  §§4.7.12/4.7.15 pp57–58/61. Datasheet Rev1.0 §7.3.16 Table7-32 p63 (physical66).

No independent divider-ready flag or guaranteed divider settling maximum was
found. Low-family comparator startup is typical, not a guaranteed maximum;
`Output::readiness` remains `Unknown`. This driver does not invent a fixed delay,
make an exact threshold-voltage promise, or claim measured electrical behavior.
Fixed 1.2 V BGR input, DAC references, output pins, interrupts, filtering, window
mode and timer routing are outside this addition. No RF register is touched.
