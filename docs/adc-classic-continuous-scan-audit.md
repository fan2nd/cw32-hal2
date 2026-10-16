# Classic continuous scan coherence audit

MODE3 exists on every classic ADC family. It repeatedly visits the selected
sequence slots, writes each result to the correspondingly numbered RESULT
register, sets EOC per conversion and EOS per complete sequence, and continues
until START=0. These facts alone do not establish an atomic multi-slot snapshot.

Own-manual MODE3 sections, with printed / PDF pages:

| Family / revision | Pages | Maximum slots |
| --- | --- | --- |
| F002 CN1.4 | 311 / 312 | 4 |
| F003 CN2.3 | 371 / 372 | 4 |
| F020 CN1.4 | 382 / 383 | 4 |
| F030/A030 CN2.5 | 444 / 445 | 4 |
| L031 CN1.6 | 441 / 442 | 8 |
| L052 CN1.5 | 478 / 479 | 8 |
| L083 CN2.0 | 483 / 484 | 8 |
| R031 CN1.3 | 444 / 445 | 8 |
| W031 CN1.4 | 444 / 445 | 8 |

Official source identities are pinned in `sources/evidence-sources.json`.
W031 remains a retired target. The [single-channel audit](adc-classic-continuous.md)
lists each family's OVW and ICR register pages. OVW is described as a new
conversion completing before result data was read. The inspected MODE3 prose
warns about overflow but does not explicitly define per-slot unread state,
whether reading one RESULT acknowledges only that slot, or how EOS clearing
interacts with that state. CR1.DISCARD is specifically described as the
single-channel result retention policy and cannot supply that missing proof.

## Candidate protocol and its required invariant

A potential collector would wait for EOS, clear EOS before reading, read all
selected RESULT slots, and reject any sticky OVW observed before or after the
reads. OVW would remain uncleared for the entire requested buffer. This can
protect sequence coherence **if each RESULT slot independently retains unread
state until that particular slot is read**:

- Replacing any unread slot from the admitted sequence must latch OVW.
- Replacing a slot already read cannot change the sample saved in software.
- A later sequence completing after all its predecessor's slots were read must
  retain its own EOS for the next iteration.

Under those conditions, every accepted row contains one generation of all
selected slots. The guarantee does not follow merely from seeing EOS once or
from checking that RESULT values look plausible. For example, after EOS for
row A, hardware can write B0 before software reads RESULT0, while RESULT1 still
contains A1. Returning B0/A1 must be rejected. An aggregate acknowledgment that
retires unread state for other slots would undermine the proposed proof.

The own manuals inspected here neither explicitly establish nor disprove that
per-slot invariant. Do not label it an observed hardware defect. Obtain a
manufacturer definition of MODE3 OVW/read/EOS acknowledgment, including
simultaneous conversion and bus reads, or independently qualify a strict
service bound that prevents overlap. The native ADC audit records the stronger
absence of any OVW status on L010/L011/L012.

Single-channel MODE2 has only RESULT0 and does not require cross-slot unread
tracking. Existing MODE4 single scans wait for stopped START before reading,
so no following autonomous sequence races those reads. Neither existing API
is broadened by this audit. MODE3 APIs, periodic triggers and dual coupling
remain separate work; repeated MODE4 calls are not presented as MODE3 support.

This is source review only. No tests, firmware execution, compilation or PAC
generation were performed.
