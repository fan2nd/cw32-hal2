# Direct PAC access in the RCC backends

The F002/F003, L010/L011, L012, L031/R031/W031 and L052/L083
clock backends now use their selected PAC directly. The private `ClockIo`,
`PacIo` and register-word dispatch interfaces have been removed from those five
files. Source-specific voltage/temperature bounds and generated clock facts
remain in the data pipeline; the shared `RccInfo` peripheral clock management
remains intentional.

This change does not add a clock source or alter a clock transition. Each keyed
or unkeyed modification retains one read and one write; combined conditions
retain one register snapshot per attempt. Poll limits, failed-attempt spin hints,
DSB/ISB barriers and publication of `Clocks` only after successful initialization
are preserved. Factory trim still comes from one volatile 16-bit read and is
rejected when erased. The PAC field type decodes the trim width.

The independent review checked the selected fields against each own-family
manual and the existing source-qualified electrical/Flash policies. It includes
F002/F003's trim-match path; L010/L011 and L012 retained RTC guards;
L031/R031/W031's stopped-HSI calibration bridge; and L083's inherited
PLL-to-HSI-to-LSI ordering. Existing legal incoming-clock, board-condition and
exclusive-initialization preconditions remain necessary.

The reviewed source was then built in the separate firmware workspace for all
ten affected families with `rt` and `rt,defmt`, and ten existing comparator
firmware programs linked. Compiler success and source correspondence do not
establish silicon behavior. No firmware was executed.

The shared F020/F030/A030 `hsi_48mhz` backend remains a separate pending change.
Other driver cleanup and additional external-clock modes are not implied by
this five-file refactor. Official versions/hashes are maintained in
[`sources/evidence-sources.json`](../sources/evidence-sources.json); qualified
operating assumptions are documented in
[`rcc-operating-envelope.json`](rcc-operating-envelope.json).
