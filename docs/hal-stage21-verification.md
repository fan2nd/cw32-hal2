# Stage21: RTC alarms and events

Adds typed weekly Alarm A programming on all 11 RTC families, plus A/B status, enable control and selective acknowledgement. L010/L011/L012 expose scoped run-mode async waits using their documented direct register access. Classic variants preserve the bounded WINDOW/ACCESS transaction and do not expose async alarm waits.

Alarm B match programming is withheld because every own manual checked has a register-table polarity that conflicts with its shared A/B examples and SDK implementation. The discrepancy was verified in rendered PDF pages, not inferred from missing text. A/B event observation and acknowledgement do not depend on that disputed polarity. No low-power wake, precision wall-clock guarantee or event-count reconstruction is claimed.

Capabilities follow the normal authored data → generator → chip/PAC metadata pipeline. The HAL uses generated ICR::write_noop() rather than a literal command mask; the seven variants preserve the exact 0x7f no-op word and zero Default. Independent schema and operational reviews passed, including byte-identical allocated firmware sections for the architecture refinement.

Final combined verification passed 108 ARM library builds, 43 RTC firmware links across 36 package/legacy selections and six links alongside Embassy time. All 54 PAC selections, targeted data/schema/source checks, provenance and format/layout checks passed. Frozen inputs remained unchanged. No HAL tests were added and no firmware was executed.
