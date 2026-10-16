# Classic single-channel continuous acquisition

`blocking_read_continuous` covers F002/F003/F020/F030/A030/L031/L052/L083/R031.
W031 source evidence is retained below, but its retired target is not restored.
The [x030 protocol](adc-x030-continuous.md) applies unchanged: software starts
MODE2 once, EOC is acknowledged before RESULT0 is read, OVW remains sticky,
errors invalidate the potentially written prefix, and bounded cleanup retires
the converter before reuse. Input buffering and internal channels are rejected.
No DMA or external trigger is enabled.

Each family's own manual independently specifies this protocol. The table
lists printed pages; PDF pages are printed + 1. Sources and complete hashes are
pinned in `sources/evidence-sources.json` and were checked against cached PDFs.

| Family / manual | Continuous mode | MODE encoding | DISCARD / alignment | OVW | ICR / RESULT0 |
| --- | --- | --- | --- | --- | --- |
| F002 CN1.4 | 310 | 326 | 327 | 332 | 333 |
| F003 CN2.3 | 370 | 388 | 389 | 394 | 395 |
| F020 CN1.4 | 381 | 399 | 400 | 405 | 406 |
| F030/A030 CN2.5 | 443 | 461 | 462 | 467 | 468 |
| L031 CN1.6 | 440 | 459 | 460 | 466 | 467 |
| L052 CN1.5 | 477 | 497 | 498 | 506 | 507 |
| L083 CN2.0 | 482 | 501 | 502 | 508 | 509 |
| R031 CN1.3 | 443 | 461 | 462 | 468 | 469 |
| W031 CN1.4 | 443 | 461 | 463 | 469 | 470 |

All these sources describe MODE=2 as repeated conversions of the selected
CHMUX, ending on START=0; each completion sets EOC and deposits its result in
RESULT0. DISCARD=0 overwrites unread old data. OVW reports a completion while
the old result was unread. ICR clears flags by writing zero and preserves them
by writing one. Thus the clear-before-read protocol and sticky overrun check
are based on each family's own semantics, not shared register names alone.

The common implementation uses each selected authored PAC layout. F002 has
no BGR/temperature controls or internal reference; its existing exclusions are
preserved. L031/L052/L083/R031/W031 reserve CR0 bit4 and receive no BGR write.
Their CR1 DMA controls stay disabled. R031 input handles retain the qualified
hardware mux4..12 mapping; no logical channel-number conversion is added.
L052 MODE7 and its extra common result register are unused. Existing family
supply, reference and clock bounds remain mandatory. Buffered/internal
single-shot restrictions are sourced in the classic family electrical audit.

Native L010/L011/L012 do not share this OVW protocol and remain excluded.
Continuous sequence coherence, asynchronous collection and triggered operation
remain separate pending work. This extension has source review only: PAC
regeneration, builds, tests and hardware acceptance were not run. In particular,
simultaneous completion/acknowledgment, service latency, terminal cleanup and
successive single/scan/continuous reuse still need acceptance.
