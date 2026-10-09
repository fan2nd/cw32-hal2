> Historical software-verification references: HAL tests and fixture harnesses cited below were deleted on 2026-10-08. Their reported results apply to the dated source snapshot only; source facts remain unchanged. See [current HAL layout and build scope](hal-production-layout.md).

# Classic ADC family expansion

This isolated batch extends the reviewed F020/x030 single-shot ADC engine to
CW32F002, CW32F003, CW32L031, CW32R031, CW32W031, CW32L052 and CW32L083.
It does not change timer or clock drivers, canonical register templates, or
vendor register identities. Software checks do not establish silicon accuracy.

## Hardware-qualified implementation

- The public ADC module is selected by exact reviewed ADC register-version cfgs.
  The private adapter uses the actual selected PAC, including the relocated
  IER/ICR/ISR on eight-result parts. It never pointer-casts another family's IP.
- F002 only exposes supply reference and supply/3 as its internal channel.
  Internal1V5, Internal2V5, Temperature, VrefInt and their constructors are absent.
  Copied SDK CR0 BGREN/TSEN fields do not authorize writes to reserved bits5:4.
- F003 has the documented BGREN4/TSEN5 controls. The five L/R/W families retain
  TSEN5 and never set reserved bit4. No hypothetical BGR register is invented.
- All eight-result adapters leave sequence/result extensions untouched. L052's
  common RESULT, ALTR and additional conversion modes remain unexposed. F020's
  SDK-only 14-bit path remains untouched. F002/F003 have no DMA request fields;
  initialization and per-read CR1 writes only select the proven 4-bit mux.
- All classic families retain BIAS reset0, MODE0, right alignment, no accumulation,
  no triggers, and no interrupt/DMA requests. Every wait has a finite poll budget.
  Success requires EOC and stopped START; overwrite discards the sample. Timeout
  and Drop stop conversion and power down; a later read reinitializes and waits
  for READY. Configuration/timing validation errors preserve ADC registers and
  active state. Constructor and configuration-setter validation precedes hardware
  writes, but blocking reads prepare the channel first: external GPIO can already
  be configured as analog when a later timing check rejects the read.
- ADC clock selection uses exact rational comparisons before division, the own
  supply/reference bands, buffer maximum200kSPS and TS minimum5us acquisition.
  TS startup waits50 nominal us against each own datasheet's maximum45us.
  The BGR25us guard where BGREN exists is based on approximate20us prose, not a
  characterized maximum. READY is always checked. Clocks include no tolerance
  guarantee; external-source settling remains a board responsibility.
- Supply limits are1.65–5.5V except R0312.2–3.6V and W0311.8–3.6V. W031 RF DCDC
  additionally requires2.0V. L/R/W general operating tables require VDDA=VDD.
  The declaration describes guaranteed minimum supply; the board must also
  independently meet maximum supply and input/reference full-scale ratings.
- Analog sources are exclusively managed by the ADC owner. Even where no BGREN
  field exists, comparators may consume ADC-selected references/temperature;
  independent raw comparator use is not a supported ownership arrangement.

## Pin and metadata contract

External channels are generated only from qualified package routes; the HAL
retains Embassy Peri ownership, sealed instance/channel traits and borrowed
channel erasure. There are no manually implemented per-pad ADC traits.

The optional adc_mux field records a hardware selector independently of a
source signal label. In particular R031 PA4 remains signal IN0 with adc_mux4,
not a relabeled IN4. Existing non-ADC pins omit this field in JSON and carry None
in static metadata. x030/F020 routes are migrated to explicit selectors too.
The build script rejects ADC pin routes without a mux and analog routes with AF.
Generated host tests construct tokens without MMIO and compare every sealed
channel selector with the explicit metadata.

See [route qualification](classic-adc-route-qualification.md) for all21 new exact
packages, seven family intersections, per-route sources and SDK disputes.
See [electrical audit](adc-classic-family-electrical-audit.md) for28 pinned own
manual/datasheet/SDK sources and exact printed-page references. The intentional
schema extension is recorded in cw32-data/DIVERGENCES.md.

## Verification scope

Receipts are under verification-logs/classic-adc/. The final summary records
commands, feature sets, result counts and source-input hashes. In particular:

- All44 classic ADC feature selections receive whole-HAL host tests, including
  the seven new family aliases, every exact package and x030/F020 regressions.
- ARM compile contracts test every bonded pad, borrowed/owned channel erasure,
  peripheral reuse after Drop, sealed traits and unsupported capabilities.
  F002 additionally rejects both internal references and both forbidden internal
  channel types/constructors. Negative controls must fail for exact diagnostics.
- Actual release ELF links exercise external and supply/3 conversion paths on
  all21 new exact packages, using each package's memory and interrupt metadata.
- Source/PDF-grid qualification, package intersection/mutation, schema round-trip,
  static metadata, access, generated parity and existing route regressions run
  independently of the HAL engine tests.

No firmware is flashed or executed. Known-voltage inputs, electrical corners,
clock tolerance, source impedance, internal settling and shared analog behavior
still require on-silicon validation. No calibrated temperature units are claimed.

## Explicit remaining scope

L010/L011 require their own sequence ADC engine: different sample times, limits,
start/settling and EOS protocol, with no READY/OVW. L011's own PA8–11 routes must
not inherit L010's PB pins. L012 additionally needs coordinated dual-ADC common
clock/reset and analog-source ownership. Those three families are deliberately
not enabled by this batch. External-reference pin ownership, multi-channel/scan,
DMA, asynchronous conversion, trigger modes, calibration and physical-unit
conversion are also absent.

When integrating, preserve the main tree's newer FLASH permission/version
corrections and other unrelated work. Apply only analog additions to input
manifests, merge the exact ADC build-script additions, then regenerate on main.
Do not replace main input manifests or generated trees from this isolated copy.
