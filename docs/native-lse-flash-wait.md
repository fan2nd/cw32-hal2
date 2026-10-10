# Native auxiliary LSE and owned FLASH WAIT changes

This correction applies to the existing auxiliary-LSE configurations on
CW32L010F8P6/F8U6/Y8M6, CW32L011K8T6/K8U6 and CW32L012C8T6/C8U6.
It adds no package, clock source, public configuration field or hardware mode.

## Documented cause and affected entry

Each family's own manual identifies SYSCTRL_CR2.FLASHWAIT[6:4] and
FLASH_CR2.WAIT[2:0] as the same function, with reciprocal register notes.
Both views reset to encoded WAIT0. The native LSE preflight captured the complete
SYSCTRL.CR2 before the enclosing RCC initialization raised FLASH latency.
The later native start compared that unchanged snapshot against current CR2.
Under the documented shared-field model, the HAL's own WAIT change could cause
LseClockInUse even though all source and fault-route settings were preserved.
This is a source-level finding, not a report of a hardware reproduction or a
measurement of cross-interface readback timing.

The affected path requests Config.lse=Some while selecting HSI or HSE SYSCLK,
passes all earlier admission checks, and reaches native start after changing
WAIT0 to WAIT1 on L010, or WAIT0/1/2 to WAIT3 on L011/L012. Fresh and retained LSE,
crystal and bypass, and both existing fault-detection modes use that comparison.
An originally unready monitored LSI still fails its earlier admission.
Config.lse=None is outside this path. Existing L010/L011 LSE SYSCLK paths already
have their own guarded WAIT refresh and retain that implementation.

## Narrow correction

The original opaque Admission is retained. Immediately before auxiliary native
start, its associated operation checks authoritative FLASH.WAIT against the
known maximum just installed by the enclosing transition. It constructs the
expected SYSCTRL.CR2 from the original saved word by changing only the typed
FLASHWAIT field, then requires full current CR2 equality with that value.
Original IER and LSE words must still match. For a monitored admission, the
original LSI TRIM/WAIT tuple, current stability and the existing own-family
factory qualification must still hold. L010 retains its inherited-legal LSI
policy; L011/L012 retain their existing factory-matching policies.

Only the saved routes word changes. The original reused classification and every
other Admission field remain intact. The operation performs no peripheral
writes, source requests, gate changes, resets or fresh preflight. Native start
keeps all its exact full-word, source, consumer, GPIO, fault and readiness checks.
A wrong authoritative WAIT returns the existing latency error; unrelated route
changes and invalid monitor state remain errors. A mirror that does not show the
expected value fails closed; no propagation delay is newly promised.

L010 RTCLPM bit7 and all non-WAIT route/reserved bits remain protected. The
existing L012 FLASH write continues preserving FETCH, CACHE and CACHEINVALID.
No change is made to the existing HSI/HSE transition or its error recovery rules.

## Failure and verification boundaries

An initialization error after transition work is still a partial initialization:
FLASH latency/gating, guarded buses, HSI calibration and possibly requested HSE
startup may already have changed. No clocks or peripheral tokens are published;
reset before retrying remains required. This correction is not a rollback,
source-loss recovery, low-power resume or L012 LSE SYSCLK feature.

The independent implementation review is recorded in
[native-lse-flash-wait-review.json](native-lse-flash-wait-review.json).
Ordinary library and representative firmware builds establish buildability.
They do not demonstrate execution of the source-review state cases, oscillator
startup, board qualification or fault-injection behavior. No HAL tests, models,
probes or harnesses are added.

## Own source references

Exact original identities and official acquisition URLs remain in
[sources/evidence-sources.json](../sources/evidence-sources.json).

- L010 CN Rev1.2: section4.7.3, PDF70/printed69 and section7.10.2,
  PDF114/printed113; WAIT0..1 and reciprocal field descriptions.
- L011 CN Rev1.1: section4.7.3, PDF68/printed67 and section7.10.2,
  PDF113/printed112; WAIT0..3 and reciprocal field descriptions.
- L012 CN Rev1.4: section4.7.3, PDF73/printed47 and section7.10.2,
  PDF127/printed101; WAIT0..3 and reciprocal field descriptions.
- L012 EN Rev1.0 corroboration: PDF77/printed51 and PDF135/printed109.

The original PDFs and their locked text derivatives were checked against those
identities. Vendor originals and rendered page images are acquired separately;
this source-only package does not add them to its redistribution contents.
