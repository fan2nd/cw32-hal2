# Native four-channel owned DMA source evidence

This records the historical Stage85 own-source qualification for
CW32L031/L052/R031/W031: finite, permanently owned SRAM copying and staged byte UART TX on eleven exact
parts. The four generic aliases have no exact memory bounds and remain refused
for safe admission. The Stage85 evidence does not qualify UART DMA RX or
combined RX/TX.
Stage92 separately extends [finite RX and combined/split DMA](classic-uart-dma-rx.md)
on these eleven parts, using positive own-source RX evidence and the existing
runtime algorithm. Its eight-library/21-ELF main passed without warnings;
independent runtime and metadata/source reviews accepted the bounded main result. At that main acceptance final capability
reports and the separate two-library/three-ELF clean replay had not run.
SPI, RF access and other DMA consumers remain outside this qualification.

The native ICR seed, generated helpers and finite source checks are complete.
Main software verification and independent actual-implementation reviews are
accepted within the [verification boundary](#verification-boundary). Clean had
not run at main acceptance and requires its own final-source receipt. No hardware
transfer, wire output, fault injection, drain timing or RF traffic has been measured.

The [evidence ledger](four-channel-dma-owned-evidence.json) binds the exact
originals, text pages, own SVD geometry, SDK symbols, channel topology, memory
and selected package routes. Its project paths are relative to the source root;
vendor paths are relative to `CW32_SOURCES`. The locked originals are external
and are not redistributed. All 54 selected original/member/text identities were
reread during preparation and matched their indexed sizes and SHA-256 values;
selected SDK members also matched their exact archive-member chains. No source
was reacquired, substituted or repinned.

## Own authorities and page conventions

Printed pages below equal zero-based PDF page indices. PDF viewer page numbers
are one greater. Each extracted page has its own UTF-8 hash in the ledger.
These conventions were checked separately for each own manual and datasheet.

| Family | Own user manual | Own datasheet | Own SDK |
| --- | --- | --- | --- |
| L031 | CN V1.6 | CN V1.9 | V1.4 |
| L052 | CN V1.5 | CN V1.3 | V1.4 |
| R031 | CN V1.3 | CN V1.2 | V1.1 |
| W031 | CN V1.4 | CN V1.3 | V1.3 |

| Fact | L031 UM | L052 UM | R031 UM | W031 UM |
| --- | --- | --- | --- | --- |
| SRAM / main Flash | 28 / 29 | 31 / 32 | 30 / 31 | 29 / 30 |
| General access-width and gated-read limits | 32 | 35 | 34 | 33 |
| DMA gate / active-low reset | 76 / 79 | 80 / 83 | 78 / 81 | 77 / 80 |
| DMA IRQs | 90 | 95 | 92 | 91 |
| Four channels / arbitration | 119–120 | 124–125 | 121–122 | 120–121 |
| Software BLOCK / SOFTSRC completion | 123 | 128 | 125 | 124 |
| Equal width / REPEAT / count | 127 | 132 | 129 | 128 |
| Terminal flags | 128 | 133 | 130 | 129 |
| Register geometry / ISR / ICR / CSR | 129–132 | 134–137 | 131–134 | 130–133 |
| Trigger selectors and type | 133 | 138–139 | 135 | 134 |
| Count and address registers | 134 | 139 | 136 | 135 |
| Three UARTs / UARTx DMA | 319 / 339 | 356 / 376 | 322 / 342 | 322 / 342 |
| Byte SRAM-to-TDR example | 344 | 381 | 347 | 347 |
| UART bases / CR2 / TDR / ISR | 349 / 352 / 354 / 356 | 386 / 389 / 391 / 393 | 352 / 355 / 357 / 359 | 352 / 355 / 357 / 359 |

L052's HARDSRC request table is on p138; SOFTSRC and TYPE continue on p139.
Both pages are required. The selected L031 SVD is its indexed EWARM member;
the L052 SDK has a nested `CW32L052_StandardPeripheralLib_V1.4` directory.
Similar file names elsewhere in an SDK do not replace those pinned identities.

Each family supplies its own hardware facts. Matching canonical register IR is
corroboration, not permission to infer hardware from another family. The prior
F020 implementation supplies an ownership contract, not four-channel geometry,
vector, SRAM, byte-TDR or package-pin authority.

## Native ICR command seed

The four own ICR pages establish register offset `0x04`, reset `0xFFFFFFFF`,
R1W0 semantics, W0 clearing, W1 doing nothing and reserved bits retaining their
defaults. The single appended `dma_cw32l031_v1` entry in
[register-writes.yaml](../cw32-data/register-writes.yaml) therefore uses
4294967295 for both reset and no-op, with TC1/TE1 through TC4/TE4 as its eight
zero-to-clear fields. Existing authored seed bytes are preserved as a prefix.

| Channel | TC / TE bits | TC only | TE only | Both |
| --- | --- | --- | --- | --- |
| 1 | 0 / 1 | `0xFFFFFFFE` | `0xFFFFFFFD` | `0xFFFFFFFC` |
| 2 | 4 / 5 | `0xFFFFFFEF` | `0xFFFFFFDF` | `0xFFFFFFCF` |
| 3 | 8 / 9 | `0xFFFFFEFF` | `0xFFFFFDFF` | `0xFFFFFCFF` |
| 4 | 12 / 13 | `0xFFFFEFFF` | `0xFFFFDFFF` | `0xFFFFCFFF` |

These encodings are consequences of the typed no-op seed and selected field
setters, not a runtime mask table. Writes must begin with the generated typed
`write_noop()`, change only the selected channel fields and preserve sibling
flags and reserved ones. Zero `Default` and ICR read-modify-write do not satisfy
this command contract. A reset-value helper does not authorize runtime reset.

Ordinary generation added only
`cw32-data/data/register-writes/dma_cw32l031_v1.json` and appended native ICR
helpers to `cw32-metapac/src/peripherals/dma_cw32l031_v1.rs`, increasing the
906-file generated inventory to 907. Independent actual generated-delta review
accepted exactly these two paths; 905 existing generated files and all 54 chip
JSONs remained byte-identical.
Register layouts, canonical reuse, chip JSON, pins, vectors, memory, clock
metadata, request inventories and generator implementation need no change.

## Channels, descriptors and shared vector

DMA base is `0x40020000`; ISR is at +0 and ICR at +4. For physical channel y=1..4,
CSR is at base+32*y, CNT at +4, SRCADDR at +8, DSTADDR at +12 and TRIG at +16.
The native `dma_cw32l031_v1` controller and common `dmachannel_v1` descriptor
already represent these facts. No fifth channel or raw pointer-stride adapter
is justified.

| Physical channel | Singleton / index | Base | IRQ / number | Vector offset |
| --- | --- | --- | --- | --- |
| 1 | DMA_CH1 / 0 | `0x40020020` | DMACH1 / 9 | `0x64` |
| 2 | DMA_CH2 / 1 | `0x40020040` | DMACH23 / 10 | `0x68` |
| 3 | DMA_CH3 / 2 | `0x40020060` | DMACH23 / 10 | `0x68` |
| 4 | DMA_CH4 / 3 | `0x40020080` | DMACH4 / 11 | `0x6C` |

Each own SVD, CMSIS header and manual IRQ table agrees. DMA's controller-root
SVD interrupt lists only DMACH1 and cannot substitute for the four physical
channel bindings. CH2/3 share an NVIC vector while retaining independent
ownership, descriptors, flags and waiters. Servicing one sibling must not
consume another's completion or globally clear, disable or unpend its vector.
UART1/2/3 have individual IRQ27/28/29 at vector offsets `0xAC/0xB0/0xB4`.

CSR is EN0, TCIE1, TEIE2, TRANS3, SRCINC4, DSTINC5, SIZE[7:6], STATUS[10:8].
TRIG is TYPE0, SOFTSRC1, HARDSRC[7:2]. CNT is 16 bits and REPEAT is four bits
at bit16. STATUS0 is initial, 1 address range error, 2 stop-request error,
3 source error, 4 destination error and 5 complete; values6/7 are unknown.
The existing shared typed enums already encode these facts.

## Completion, admission and permanent lifetime

Normal reset or trustworthy clean-runtime entry supplies the startup premise.
Exact SRAM bounds, initially disabled shared work/config clock, released reset,
bounded gate acknowledgment and subsequent reset-register observations are
required for admission. Reads while configuration clocks are off are not valid
state evidence. Dirty inherited state is rejected, never reset or cleared into
admission. After enabling the gate, a rejected admission leaves it enabled.
One-time ownership cannot be reconstructed from a later idle snapshot.

Copy transfers require complete, overflow-free, aligned and disjoint static
SRAM extents, equal widths, both increments, BLOCK mode, REPEAT1 and count
1..65535. Ownership becomes in-flight before launch or a panic boundary;
descriptors are configured before EN and software trigger. Own manuals describe
SOFTSRC clearing after successful software BLOCK completion. EN-clear is not
completion evidence.

Success requires recorded own TC, no TE, STATUS5 and, for software copying,
SOFTSRC0, followed by the ownership engine's memory fence before returning the
same channel and original allocations. TC before the first poll must still be
recorded. TE wins over simultaneous TC. Inconsistent completion remains
unconfirmed even if a later observation looks favorable.

A stop request, disabled channel, closed request gate, reset, clock-off, count0,
timeout or TXBUSY0 does not establish bus drain after an error. Error or unresolved
cancellation retains/quarantines the physical channel and static allocations
permanently. Staged TX additionally retains the UART, pin/configuration and
clocks. No post-error reclamation, reset recovery or finite-liveness guarantee
is established by these sources. CPU and lower-numbered channels have priority.

## Staged byte UART TX and exact package routes

Each own general MMIO warning limits most peripherals to 32-bit access. Each
own UART example separately demonstrates an SRAM `uint8_t` array sent to
UART1_TDR using SIZE0, source increment, fixed destination, BLOCK, REPEAT1,
then EN1 before DMATX1. This narrow peripheral-DMA exception does not authorize
arbitrary byte endpoints; CPU MMIO remains typed 32-bit.

UART2/3 qualification follows each own manual's three-UART statement, UARTx
DMA/CR2/TDR contract and base table, its own SVD's UART2/3 inheritance from
UART1, and its own three TX request selectors. Matching addresses alone would
not establish that qualification.

| Endpoint | TDR | HARDSRC | SDK shifted value | Channels |
| --- | --- | --- | --- | --- |
| UART1_TX | `0x40013828` | 1 | 4 | CH1..4 |
| UART2_TX | `0x40004428` | 3 | 12 | CH1..4 |
| UART3_TX | `0x40004828` | 5 | 20 | CH1..4 |

TX owns exclusive SRAM byte staging. Caller slices are CPU-copied into finite
staging-sized chunks; a caller pointer never becomes a DMA address. Clean
recorded TC/noTE plus STATUS5 permits closing DMATX, fencing and retiring the
staging lease. Hardware TX does not require SOFTSRC. Flush additionally waits
for TXBUSY0; UART TC supplies wakeup, not DMA completion. Clean Drop need not
flush the last frame. Unresolved/error Drop retains the endpoint and staging.

All eleven exact parts have Flash `[0,0x10000)` and SRAM
`[0x20000000,0x20002000)`. Own datasheet product tables are L031 p9, L052 p9,
R031 p10 and W031 p8, corroborated by own manual memory chapters, selected PDSC
identities and exact metadata. Two equal disjoint copy allocations can total
at most 8 KiB before stack and other allocations; the 65535 count ceiling does
not enlarge SRAM.

| Exact feature | Package | UART1 / CH4 | UART2 / CH2 | UART3 / CH3 |
| --- | --- | --- | --- | --- |
| cw32l031c8t6 | LQFP48 | PA8 / AF2 / pad29 | PA6 / AF2 / pad16 | PA10 / AF6 / pad31 |
| cw32l031c8u6 | QFN48 | PA8 / AF2 / pad29 | PA6 / AF2 / pad16 | PA10 / AF6 / pad31 |
| cw32l031f8p6 | TSSOP20 | No safe TX pin | PA6 / AF2 / pad12 | PA10 / AF6 / pad17 |
| cw32l031f8u6 | QFN20 | No safe TX pin | PA6 / AF2 / pad9 | PA10 / AF6 / pad15 |
| cw32l031k8u6 | QFN32 5×5mm | PA8 / AF2 / pad18 | PA6 / AF2 / pad12 | PA10 / AF6 / pad20 |
| cw32l031k8v6 | QFN32 4×4mm | PA8 / AF2 / pad18 | PA6 / AF2 / pad12 | PA10 / AF6 / pad20 |
| cw32l052c8t6 | LQFP48 | PA8 / AF2 / pad29 | PA6 / AF2 / pad16 | PA9 / AF1 / pad30 |
| cw32l052r8s6 | LQFP64 7×7mm | PA8 / AF2 / pad41 | PA6 / AF2 / pad22 | PA9 / AF1 / pad42 |
| cw32l052r8t6 | LQFP64 10×10mm | PA8 / AF2 / pad41 | PA6 / AF2 / pad22 | PA9 / AF1 / pad42 |
| cw32r031c8u6 | QFN48 | PA8 / AF2 / pad32 | PA6 / AF2 / pad19 | PA10 / AF6 / pad34 |
| cw32w031r8u6 | QFN64 | PA8 / AF2 / pad22 | PA6 / AF2 / pad10 | PA10 / AF6 / pad24 |

These are 31 existing safe routes. Own package/AF pages are L031 DS26–27/28,
L052 DS26–27/30, R031 DS28–29/31 and W031 DS26–27/30. The L031/R031/W031
SDK spelling `PA10_AFx_SUART3TXD` writes AFR10=6; the own datasheet identifies
UART3_TXD there. The vendor spelling is retained as evidence, not renamed into
a different peripheral. L052 uses its own PA9/AF1 route.

PA13/PA14/PF3 stay excluded. The two L031F8 packages have no safe UART1 TX pin;
PA14 cannot be introduced through a generic trait or raw-pin escape. The generic
`cw32l031`, `cw32l052`, `cw32r031`, `cw32w031` selections have empty memory and
package metadata and cannot receive safe DMA admission. This cohort grants no
new factory-LSI or other clock-source qualification.

## Clocks, RF boundaries and inventory-only requests

DMA uses one shared HCLK work/config gate at SYSCTRL.AHBEN bit0 (+0x30) and
active-low AHBRST bit0 (+0x40). GPIOA uses bit4. UART1 uses APBEN2/APBRST2 bit9
(+0x34/+0x44); UART2/3 use APBEN1/APBRST1 bits7/8 (+0x38/+0x48). Channel
recovery cannot reset or gate siblings. Existing scoped RCC ownership remains
necessary. UART CR2.SOURCE is peripheral-local; a clock-sidecar `partial` mux
status does not permit addressing it through SYSCTRL. This TX profile uses PCLK.

R031 has its own 16MHz RF oscillator (UM49/56), host GPIO/SPI signals on PA0..3
(DS15), and RF access/state-machine descriptions (UM497–498). Chosen PA6/8/10
are distinct, but enabling GPIOA can resume retained whole-bank sampling,
filters and events. Existing functional handover and immutable-clock limits
still apply; leaving RFCLK alone does not prove RF idleness or packet continuity.

W031 has its own 32MHz RF oscillator (UM48/54), internal host connections on
PB3/PB4/PB5/PB6/PB13 (DS13), an SPI1/RF exclusion (UM362), PCLK-derived SPI
timing (UM365), and RF register/FIFO access (UM508). Selected GPIOA routes need
no new RF/SPI path. Existing whole-GPIOB/RCC limits remain. Additional DMA bus
traffic can change CPU/host service despite unchanged RFCLK. No RF page, register,
command, mode, power, reset or IRQ/event read/clear is part of this qualification.

Stage85 left UART_RX inventory-only; [Stage92 classic RX](classic-uart-dma-rx.md)
separately qualifies finite UART RX-only and combined/split DMA. L031F8U6
UART1 supports RX-only on PA9/AF2 or PC15/AF6 despite its absent TX route;
L031F8P6 UART1 has neither safe direction. SPI_RX/TX, timers, ADC and L052's
extra SPI2/GTIM3/LCD requests remain inventory-only. No SPI DMA, low-power DMA,
circular mode, borrowed-buffer DMA, direct caller-Flash DMA or arbitrary endpoint
is qualified. Existing x030/L083 RX/TX/SPI, idle and buffered-RX guards retain
their independent scope.

## Verification boundary

These results belong to Stage85 copy/TX, including its recorded source-gate
failures and generated delta. They do not verify Stage92 RX; its separate
[main results and remaining verification](classic-uart-dma-rx.md#application-and-verification-boundary)
retain their own receipts.

The existing [source checker](../tests/verify_dma_owned_evidence.py) retains its
previous checks and adds finite current facts: exact own source bytes and archive
chains, hashed pages, ICR geometry/semantics, one authored/generated seed, typed
PAC helper values, channel/vector topology, own byte-TDR examples and start order,
eleven exact SRAM/Flash records, four empty aliases and all 31 package/AF routes.
Only mutable `text.evidence` backlinks are ignored when comparing embedded text
identity; paths, URLs, digests, sizes and other bound identity values remain strict.
Current scoped geometry and canonical reuse are checked without freezing unrelated
historical files or entire source-index hashes.

The source index appends references to these two documents, their actual source
checker and the [metadata review](four-channel-dma-metadata-review.json), which
independently consumed all 54 locked source nodes. The
[runtime review](four-channel-dma-runtime-review.json) consumed selected project
inputs and source-binding evidence, so it is linked here without claiming direct
consumption of raw vendor nodes. Original identities, previous backlinks and
unrelated records are unchanged. Reversing the administrative append separately
restores the prior index; reversing every Stage85 append restores all 403,846
baseline bytes. Both exact reversal receipts are retained externally.

Main verification produced six standalone ARM libraries and 21 linked firmware
ELFs; all 27 commands exited zero and emitted zero warnings. The 21 retained
HAL dependency archives are recorded separately, not counted as extra standalone
commands. All 1,402 source files, 907 generated files, toolchain and Cargo
configuration remained unchanged throughout the matrix. Independent inspection
established actual executable text, reset/Thumb entry, initial stack, selected
Flash/SRAM ranges, exact generated memory.x and active vector/handler bindings.
The four generic libraries retained SRAM None; the eleven TX selections and four
copy selections supplied the new package and channel coverage recorded in the
actual review JSONs. Remaining ELFs preserve the selected earlier behavior.

Seven finite generation/source commands passed in documented segments. The first
attempt completed ordinary generation and DMA projection, then the existing DMA
fact checker failed on a parent vendor identity referenced by a text child. Its
second attempt exposed an intentionally projected clock record being compared to
notes/evidence as well as facts. Its third attempt encountered L052's lsb/msb field
encoding. The narrow corrections retained complete source identity, all hardware
clock fields and exact normalized SVD geometry. All three original failures remain
in the receipts. The successful five-command tail then covered DMA source facts,
provenance, capability rendering, capability consistency and module layout. This
is not one uninterrupted successful invocation.

The parent-identity and clock-projection corrections preceded main. Only the final
SVD parser correction followed the completed ARM matrix, changing the checker
from 9d749aa5584a7a229cabe9230e712d3ef9332259cf66570db6ec991b17b42ee1 to
77c4591ffad1574fbf67ce0d10921e026212b084eece4b86ad9723ffbee863b1. It changed no
compiled HAL/example code, locks, configuration or generated files. The final
finite gates ran against that corrected checker. This evidence/status update,
actual review receipts, source consumer references and receipt-retention rule
are subsequent administrative changes, not part of the frozen main inputs.

Both actual independent reviews accept the bounded main implementation and
compiler/linker results. At main acceptance clean had not run. Final-package
acceptance requires a separate final-source reconstruction with ordinary
generation, finite gates, two standalone ARM libraries, four linked ELFs and
independent review. Main acceptance does not replace that clean receipt or
publication approval. Source and software evidence does not establish hardware
transfer, wire timing, error drain, RF continuity, electrical validity or finite
liveness.
