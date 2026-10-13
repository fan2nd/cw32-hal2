# F020 owned DMA source evidence

This records the historical Stage84 own-family source facts for the bounded
F020 DMA extension: two native channels, finite static-owned SRAM copying, and staged byte UART TX
on `cw32f020c6u7`, `cw32f020k6u7`, and `cw32f020f6u7`. It supplies source evidence
and the native ICR command seed. Actual main software verification is
summarized in [Verification boundary](#verification-boundary), with separate
implementation and acceptance receipts. No transfer on hardware or wire
output is claimed. Stage92 separately extends finite RX and combined/split
UART DMA on the same three exact parts, using the existing runtime algorithm;
see [the current classic RX contract and verification](classic-uart-dma-rx.md).
Stage93 separately adds SPI1/SPI2 paired byte DMA on these three exact parts;
its main software verification and independent runtime and metadata/source
reviews accepted the bounded change. At main acceptance its separate final-source
two-library/six-ELF clean replay had not run. See [the current SPI contract and
main verification](spi-dma.md) and [own SPI evidence](classic-spi-dma-evidence.json).
Both native channels are
consumed by one SPI pair, so SPI1/SPI2 DMA examples are separate deployments;
clean Drop does not restore channel admission. Borrowed IRQ PIO remains
available. The Stage84 source and review records below retain their copy/TX
scope and do not verify Stage93.

The source identities, page hashes, decoded facts and remaining obligations are
in [f020-dma-owned-evidence.json](f020-dma-owned-evidence.json). The originals are
the official F020 User Manual CN Rev1.4, F020 SDK V1.2 and current F020 datasheet
CN Rev1.3. The superseded same-named datasheet outside `current-datasheets/` is
not used. All twelve bound original/member/text identities and all 27 bound
manual-page identities were reread from the existing evidence cache; the seven
selected SDK members were also checked directly through their archive chains.
No source was reacquired or substituted.

## Native ICR command seed

F020 UM §8.8.2, printed p135 (PDF viewer p136), gives DMA ICR at offset 4 with
reset `0xFFFFFFFF`. TC1/TE1 are bits 0/1 and TC2/TE2 are bits 4/5. These four
fields are R1W0: writing zero clears the corresponding flag, while writing one
has no effect. Reserved bits retain their reset values. Therefore the native
`dma_cw32f020_v1` entry in `cw32-data/register-writes.yaml` has reset and no-op
values 4294967295 and exactly those four zero-to-clear fields.

| Channel command | Encoded value |
| --- | --- |
| CH1 TC only | `0xFFFFFFFE` |
| CH1 TE only | `0xFFFFFFFD` |
| CH1 TC and TE | `0xFFFFFFFC` |
| CH2 TC only | `0xFFFFFFEF` |
| CH2 TE only | `0xFFFFFFDF` |
| CH2 TC and TE | `0xFFFFFFCF` |

These values are consequences of the source encoding, not a runtime command
table. The driver must start from the generated typed `write_noop()`, change
only its channel's fields, and use `write_value`. Zero-valued chiptool `Default`
and ICR read-modify-write do not satisfy this command contract. The documented
reset helper does not authorize a runtime controller reset. Production
generation and review of the resulting native PAC helpers remain separate.

## Controller, descriptors and independent IRQs

UM pp93–94 and134–138 establish DMA base `0x40020000`, channel bases
`0x40020020`/`0x40020040`, and exactly two physical channels. Channel1 uses IRQ9
(vector offset `0x64`); channel2 uses IRQ10 (`0x68`); IRQ11 (`0x6C`) is reserved.
The SDK names the two interrupts DMACH1 and DMACH2. Its stale Channel2/3 comment
and unused extra channel aliases do not add hardware. The existing metadata
maps each physical channel to its own GLOBAL IRQ; the controller-root DMACH1
mapping is not channel2's binding.

The descriptor offsets are CSR0, CNT4, SRCADDR8, DSTADDR12 and TRIG16.
CSR exposes EN0, TCIE1, TEIE2, TRANS3, SRCINC4, DSTINC5, SIZE[7:6] and
STATUS[10:8]. SIZE0/1/2 means byte/halfword/word, TRANS1 means BLOCK,
TYPE0/1 means software/hardware, CNT occupies bits[15:0], REPEAT[19:16]
is programmed to 1, and HARDSRC occupies bits[7:2]. Source and destination
addresses are 32-bit. ISR and all five descriptor registers reset to zero.
AHBEN.DMA bit0 enables the shared clock with 1; AHBRST.DMA bit0 asserts reset
with 0 (UM pp79/82). These facts reuse the existing central RCC metadata.

## Completion and permanent ownership

UM §8.6 p133 defines TC as all data correctly transferred. STATUS5 means
complete (p136), and software SOFTSRC clears after completion (pp126/138).
Together these support the required software success observation: recorded TC
without TE, STATUS5 and SOFTSRC0. Hardware TX requires recorded TC without TE
and STATUS5; SOFTSRC is not hardware completion evidence.

STATUS1–4 describe errors. STATUS2 describes a stop request; it does not promise
that outstanding bus activity drained. TE must take precedence over simultaneous
TC. Neither EN-clear, a closed UART request gate, count0, TXBUSY0, a timeout nor
a later favorable status establishes DMA quiescence after error. The bounded
ownership design therefore requires permanent retention of the physical
channel and static allocations on error or inconsistent completion. UART
retention also includes its endpoint, pin, configuration and clocks.

Normal reset or trustworthy clean-runtime entry remains the startup premise.
The admission checks conservatively reject missing SRAM metadata, an inherited
gate/reset state, failed clock acknowledgment or nonzero reset-register
observations. They do not reset or clear dirty hardware into admission and do
not establish arbitrary bootloader bus history. Safe acquisition must remain
one-time; raw takeover cannot later regain admission from an idle snapshot.

Cancellation/forget must leave a launched TX chunk in driver-owned static
storage; unscheduled caller bytes never become DMA memory. Reuse requires clean
retirement before overwriting staging. Copy Drop can wait indefinitely; an
unresolved/error UART Drop retains resources. None of these source facts proves
a bounded stop protocol, abort, recovery, or liveness. The existing ownership
engine and the Stage83 buffered-UART/ordinary-UART seams were accepted by
the separate actual-main runtime review within these proof limits; source
review does not establish execution of cancellation or fault cases.

## Byte TX, exact SRAM and package limits

UM p287 explicitly configures an eight-bit SRAM array, SIZE0, increasing source,
fixed UART TDR destination, BLOCK and REPEAT1. It enables the DMA channel before
DMATX, observes DMA TC before closing DMATX, then waits for TXBUSY0 to finish
wire output. This own-family example supports the scoped byte DMA endpoint
despite p30's general peripheral-width caution; CPU MMIO remains typed 32-bit.
UM pp282/292/294/295/297 separately describe the request gate, TDR at base + `0x28`,
and TXBUSY's shift-register/buffer meaning. Ordinary UART Drop does not imply
flush or wire completion.

The selected current datasheet p7 specifies 32 KiB flash and 8 KiB SRAM for C6/K6/F6;
UM p26 places SRAM at `0x20000000`, yielding `[0x20000000,0x20002000)`.
The existing exact part metadata agrees. Generic `cw32f020` has no memory
metadata and remains a `MissingMemoryMetadata` rejection for safe admission
and staging, even if its controller surface compiles. The count-field bound
65535 does not establish that such an allocation fits 8 KiB of physical SRAM.
Full-range, overflow, alignment, overlap and final linked-placement checks
remain necessary.

| Exact selection | TX owner1 | TX owner2 |
| --- | --- | --- |
| C6U7 / QFN48 | UART1 PA8/AF2, pad 29 | UART2 PA2/AF2, pad 12 |
| K6U7 / QFN32 | UART1 PA8/AF2, pad 18 | UART2 PA2/AF2, pad 8 |
| F6U7 / QFN20 | UART2 PA6/AF2, pad 9 | UART3 PA2/AF1, pad 5 |

The pad and alternate-function facts are in current datasheet pp22–24/26 and
the pinned SDK GPIO macros. UM p137 and existing metadata give UART1_TX1,
UART2_TX3 and UART3_TX5, each selectable by either physical channel. Route
metadata does not override package exclusions: F6's only UART1 TX candidate is
PA14 and remains excluded from safe TxPin. PA13/PA14/PF3 exclusions are retained.
Stage84 admitted only copy/TX. Current [Stage92 RX qualification](classic-uart-dma-rx.md)
adds finite UART RX and combined/split DMA without changing these historical
proofs. F6 UART1 has no safe RX either. Stage93 SPI1/SPI2 uses its separate
own-source qualification and separate main software acceptance; its final-source
clean replay had not run at main acceptance. Other requests,
borrowed hardware-request constructors and buffered-plus-DMA UART ownership
remain excluded.

## Verification boundary

The following results belong to Stage84 copy/TX and do not verify Stage92 RX.
The extension's separate eight-library/21-ELF main passed without warnings;
its independent runtime and metadata/source reviews accepted the bounded main result. At that main acceptance final capability
reports and the two-library/three-ELF clean replay had not run. See the separate
[Stage92 verification boundary](classic-uart-dma-rx.md#application-and-verification-boundary).

The source receipt verifies original/member/text identities, source-page
identities, current metadata correspondence, the one native seed, and reversible
append-only evidence-index references. Existing schema, register layout,
request-table extraction, generator implementation, channel enums and historical
records are unchanged. No new HAL test, synthetic probe/model, validator or
CI mechanism is introduced here.

Main verification produced 15 actual ARM libraries and 16 linked application
ELFs; all 31 commands exited zero. The main-v1 1,391-file source snapshot,
906 generated files, toolchain and Cargo configuration were unchanged
throughout that build matrix. The six new F020 ELFs cover both binaries on
each exact part. Linked-image inspection established nonempty executable
code, valid reset entry, flash/SRAM placement, independent CH1/CH2 vectors,
zero reserved IRQ11 and each TX binary's actual UART vectors. This checks
linked code and placement, not a transfer or wire observation.

Only the L010F8P6 and L011K8T6 library rows emitted warnings, each for the
three existing RCC LSE constants. All F020 rows, the other library rows
and all 16 ELF links were warning-free.

One ordinary generation produced 906 generated files from the 905-file
baseline: `cw32-data/data/register-writes/dma_cw32f020_v1.json` was added
and `cw32-metapac/src/peripherals/dma_cw32f020_v1.rs` gained the native
reset/no-op helpers. The other 904 existing files and all 54 chip JSON
files stayed byte-identical. Independent generated-delta review accepted
exactly those changes.

The capability renderer and all five intended finite source gates passed
in documented segments: DMA source projection, existing DMA source/PAC
facts, provenance, capability consistency and module layout. The initial
DMA projection attempt failed because an external cache manifest was
missing. Restoring the auxiliary manifests from the locked source index
allowed that gate to pass without changing reviewed source facts. The
next existing DMA checker attempt failed because it compared mutable
consumer backlinks as part of source-text identity. The approved
administrative correction retained stable source identity checks and the
gate then passed. Both initial failures remain in the receipts; this is
not a claim of one uninterrupted successful source-gate invocation. The
checker-only correction occurred after main-v1 and changed no HAL or
example bytes.

Independent actual-main runtime and metadata/source reviews accepted the
bounded implementation, generated delta and recorded outcomes. At main
acceptance clean had not run; final-package completion requires a separate
final-source 2-library/4-ELF clean receipt, its ordinary generation and
finite source gates. Main acceptance does not replace clean replay or
publication. No hardware execution, fault injection, measured transfer,
wire timing, throughput or finite-liveness result is claimed.
