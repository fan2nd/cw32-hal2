# CW32L083 staged UART and SPI DMA qualification

This extension qualifies UART1–6 byte TX, finite-chunk byte RX, combined split
TX/RX, and SPI1–2 paired full-duplex master byte DMA. It reuses the static-owner
contracts in [UART TX](uart-dma-tx.md), [UART RX](uart-dma-rx.md) and
[SPI DMA](spi-dma.md). It does not infer peripheral support from the previously
accepted L083 software-copy implementation or from a shared register layout.

## Qualification and ownership boundary

`embassy-cw32/build.rs` contains one bounded staged profile policy. It verifies
native DMA/DMACHANNEL `v1`, UART `cw32l083_v1` and SPI `cw32l031_v1`, and requires
one unremapped, unrestricted DMA RX and TX route for each admitted instance.
The UART/SPI route sets generate `uart_dma` and `spi_dma`; internal request
launch/retirement machinery requires either capability. These mark qualified
driver support, not hardware absence on other families. The typed PAC versions
and metadata-derived clock, pin and interrupt facts are preserved. No adapter,
family list throughout runtime, or new register/schema representation is used.

Only these generated L083 constants and sealed routes are exposed. Selectors
come from `cw32-data/dma/cw32l083.yaml`, not a second runtime request table:

| Peripheral | RX | TX | Compatible channels |
| --- | ---: | ---: | --- |
| UART1 | 0 | 1 | DMA_CH1–5 |
| UART2 | 2 | 3 | DMA_CH1–5 |
| UART3 | 4 | 5 | DMA_CH1–5 |
| UART4 | 43 | 44 | DMA_CH1–5 |
| UART5 | 45 | 46 | DMA_CH1–5 |
| UART6 | 47 | 48 | DMA_CH1–5 |
| SPI1 | 6 | 7 | DMA_CH1–5 |
| SPI2 | 8 | 9 | DMA_CH1–5 |

The existing x030 route inventory is unchanged. L083 ADC SEQ/SINGLE, LCD FRAME,
timer and other peripheral requests remain outside this HAL admission scope.
Public unsafe borrowed `Transfer::new_read/new_write` remain x030-only; L083
gets no borrowed hardware-request upgrade.

CW32L083RBT6, RCS6, RCT6, MCT6 and VCT6 have 24 KiB qualified SRAM at
0x20000000..0x20006000. Static staging must contain 1..=65535 bytes and fit
entirely within that selected-part extent. Generic `cw32l083` keeps no memory
extent and rejects safe construction with `MissingMemoryMetadata`. Channels
come only from one-time normal clean-startup admission; a raw takeover cannot
restore eligibility. No fresh hardware snapshot creates a safe capability.

Successful hardware retirement requires recorded TC without TE and Complete
STATUS (5), then request closure and the existing fence/retirement protocol.
SOFTSRC is not evidence of hardware completion. EN-clear, TE, gate closure,
SPI EN-clear or a count reaching zero is not a documented drain acknowledgment.
Cancelled/forgotten operations retain their current static leases. Ambiguous
completion and DMA errors permanently quarantine resources. No timeout, early
abort, recovery, returned resources, circular mode or bounded liveness is added.

## UART and SPI consequences

UART1/4, UART2/5 and UART3/6 retain the existing `SharedInterruptHandler` and
per-instance active guards. Constructors keep `T::InterruptHandler`; they do
not substitute an x030-specific handler. Shared NVIC lines are never disabled
or unpended by a partner's preparation or teardown. Inactive clock-gated
partners are skipped before MMIO. Retained/live TX and RX ownership still
protects each direction's requests, configuration, pins and clocks.

RX keeps RC masked while DMA owns RDR. FE/PE can wake and report promptly but
cannot release the running chunk; copy or discard follows clean completion.
There is no ORE flag. RDR overwrite and copy/rearm gaps can lose frames without
a loss report. No DMA RTS/CTS, continuous/lossless RX or throughput claim is
made. TX completion retires memory-to-TDR work; `flush` also waits TXBUSY=0.

SPI still uses two channels for every byte operation, including write-only and
read-only methods. Both clean terminals precede closing DMARX/DMATX, and wire
idle precedes success or RX copying. MODF/OV/SSERR/UD poison the whole pair.
No unresolved path reads DR, flushes, resets or recovers the peripheral. The
existing L083 SCK maximum of 12 MHz and minimum divisor 4 remain unchanged;
[the clock policy](spi-clock-source-policy.json) retains the 12/16 MHz source
conflict. Cancellation requires preserving the original device selection and
successfully awaiting `flush` before changing CS. Generic adapters that release
CS on cancellation remain outside the guarantee.

## Locked own-family sources

The authority is [CW32L083 User Manual CN V2.0](https://www.whxy.com/uploads/files/20240920/CW32L083_UserManual_CN_V2.0.pdf),
PDF SHA-256 `9930bf1755f3bbf8933163c2d0da57fd9a4f3250a358a4c0bfc75ed4eda3a0a3`,
extracted text SHA-256 `6abc933b3ed02659347ee557f47549e4c93c5de890530b950b20535b56a56564`.
Printed pages equal zero-based PDF indices; viewer pages are one greater.

| Manual location | Fact used |
| --- | --- |
| §§8.4.4/8.5 pp140/142 | One block per request, equal byte widths, REPEAT=1, bounded count. |
| §§8.6/8.8.3–4 pp143/147–149 | Correct-transfer TC, STATUS=5 and the common five-channel HARDSRC table. |
| §5.4 pp104–105 | DMA shared channel vectors, paired UART vectors and independent SPI vectors. |
| §§19.6/19.7.1.5–6 pp384/389–390 | Byte TX/RX, channel EN before request gate, TC then gate closure, separate TXBUSY wait. |
| §§19.3.3.4/19.5/19.9.9 pp378/383/400 | FE/PE/RC, receive overwrite, R1W0 clearing and no overrun indication. |
| §§20.5/20.6.1.3 pp417/420–421 | Paired byte descriptors, both TCs, both gates closed, then BUSY=0 before selection release. |
| §§20.3.9/20.4/20.8.5–7 pp415–416/431–432 | Sticky SPI errors, MODF disables SPI, destructive RX/flush operations are not drain proofs. |

[CW32L083 Datasheet CN V1.9](https://www.whxy.com/uploads/files/20251229/CW32L083_DataSheet_CN_V1.9.pdf),
PDF SHA-256 `852f772e9174cb76bf0f475f31f1e275254f8fe176bd3e7ad60d00b41db9509e`,
Table 5-2 pp26–32 and Tables 5-3–8 pp33–37 supply the existing package/AF
inventory. `parts.yaml` and generated memory retain exact-part SRAM sizes.
LQFP100 is not assumed to be a strict pin superset of LQFP80 or LQFP64.

The selected [SDK V2.2](https://www.whxy.com/uploads/files/20240821/CW32L083_StandardPeripheralLib_V2.2.zip)
has SHA-256 `2d58765568d8dd8a218b52e4650aa6e5bd4f2e4b5f386196bae637dfc0b3fe73`.
Its already-locked `Libraries/inc/cw32l083_dma.h` member corroborates selectors
(SHA-256 `e76079e457c5488e348170a05129b560af0f3098cfe368bec00cf3d976a93a61`).
The exact PDF/text/archive/member identities are selected in
`sources/evidence-sources.json`; no new unpinned SDK example is an evidence
dependency. Vendor PDFs, extracted text and SDK contents outside the existing
approved redistribution subset remain external acquisition inputs.

## Validation boundary

The existing UART TX, standalone RX, combined UART and SPI applications have
all five exact L083 package selections. The additional shared-vector UART
application keeps UART1 and UART4 active together and uses real asynchronous
packet exchange; SPI examples cover both instances. Bindings, pins and channel
choices remain checked by the same production constructors and linker.

Ordinary ARM library compilation and actual ELF/vector inspection provide only
source/build qualification. Run receipts must identify their exact source
snapshot; this document alone claims no executed check, hardware transaction,
error injection, throughput result or liveness guarantee. No HAL test harness,
register model or synthetic compile-contract engine is introduced.
