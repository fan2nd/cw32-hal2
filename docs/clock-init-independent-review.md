# Clock initialization source review

Review in progress, 2026-10-08. This complements, rather than substitutes for,
protocol-model tests. No silicon execution has been performed.

## L012

Reviewed `rcc/l012.rs` against official CW32L012 UM CN V1.4:

- §4.5 p38 requires the documented safe source-switch sequence. §4.5.1 p39
  requires stable destination before SYSCLK selection and adequate FLASH WAIT
  before switching. The implementation polls LSI STABLE, reads back SYSCLK=3,
  then clears HSIEN and polls both enable and STABLE clear before trim.
- §4.5 permits live HSI divider changes preserving trim. The existing matching-
  trim path only changes DIV, preserving TRIM. The changed-trim path uses LSI.
- §4.5 p38 requires WAIT=0/1/2/3 at up to 24/48/72/96 MHz respectively. The code
  raises WAIT to 3 before a transition and lowers it only after final HCLK
  configuration readback. A legal incoming clock is limited to rated 96 MHz;
  this does not promise recovery from an already overclocked bootloader.
- Datasheet CN V1.0 standard operating table lists VDD 1.7–5.5 V; its clock-tree
  section specifies both AHB/APB maxima of 96 MHz at VDD ≥1.8 V and 24 MHz
  below 1.8 V. Independent review found a transient risk when the final bus
  divider is greater than the incoming divider; repaired by setting conservative
  bus dividers before HSI changes. A second independent review caught the
  documented 90–100 MHz HSIOSC calibration range: the temporary AHB divisor
  is /8, not /4, to keep even 100 MHz source input below 24 MHz.
  Final clocks still require appropriate VDD. Board voltage, temperature and oscillator tolerance must
  meet datasheet conditions; the HAL does not measure supply voltage.
- L012 has no PLL; source selector 2 is rejected. Other documented entry sources
  0/1/3/4 are accepted. LSI parameters/security/debug settings are preserved.
- A second independent review identified a clock-security race in the matching-
  trim fast path: external-source failure can force HSI to 4 MHz after DIV was
  written but before the HSI mux. The repaired code rechecks DIV/TRIM and STABLE
  after confirmed HSI selection; an injected-fallback test verifies rejection
  rather than publishing stale frequencies.
- Every hardware wait has a finite poll budget. Failure does not publish frozen
  clocks and may leave conservative latency and temporary LSI state; reset is
  required before retrying. No rollback of arbitrary external state is promised.

Other families' repaired sequencing and source citations are recorded in their
family reports; the final integrated verification will supersede older counts.

## F002/F003 independent re-review

A separate reviewer rechecked the repaired implementation against F002 UM V1.4
§4.5.2/§4.5.5 and F003 UM V2.3 corresponding sections. Changed trim now requires
stable LSI handover and HSIEN=0/STABLE=0; matching trim uses the permitted
DIV-only path. Transitional bus guarding precedes source changes, final HSI
DIV/TRIM/STABLE are rechecked, FLASH latency follows the documented thresholds,
and LSI software state is restored without a physical-stop assumption. No
remaining blocker was found in this focused source review. Final frequencies
are not validated against measured VDD; electrical conditions remain a caller
obligation, including the applicable 24 MHz low-voltage ceiling.

## Shared F030/A030/F020 and L010/L011

A separate family reviewer cross-checked the shared 48 MHz implementation:
legal PLL exit through HSI, stable LSI bridge, stopped-trim calibration,
DIV-only exception, voltage guard, FLASH ordering and bounded failure behavior.
Post-mux DIV/TRIM/STABLE and mandatory control readbacks were also reviewed.
L010/L011 preserve hardware-requested LSI operation by restoring only the
software enable bit and clock-security setting. Their datasheet Table 7-4
voltage limits and stopped-HSI rules are cited in `l010-l011-hal.md`.

These are source reviews, not guarantees under arbitrary asynchronous clock
loss. All backends require valid electrical conditions and a stable legal entry
clock during initialization; detailed results remain in family reports.
