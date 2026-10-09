# Safe staged UART TX DMA on F030/A030/L083

`UartTx::new_with_dma` adds a TX-only, byte-oriented DMA constructor to the
existing async UART owner. It consumes static UART and compatible DMA singleton
tokens, a matching TX pin, both real interrupt bindings and one exclusive
`&'static mut [u8]` staging buffer. Its `ConfigError` reports UART configuration,
DMA staging or DMA admission rejection before any transfer starts; rejected
owners are consumed just as with the existing UART constructors.

The existing safe `write(&mut self, &[u8]).await` and `flush().await` signatures
remain the application interface, including the ordinary async I/O trait path.
Inputs can be flash, local/stack, heap or static slices: the CPU copies them into
private staging and their addresses never reach DMA. This implementation does
not expose an arbitrary peripheral register, DMA request number or raw pointer
through a new safe API. Matching pins and `dma::RequestRoute<T, signal::TX>` come
from the existing authored/generated routing data.

## Storage, startup and completion

The staging buffer must be nonempty, no larger than the hardware's 65535-byte
count and wholly within the selected part's qualified SRAM range. Family
selections without qualified memory metadata are rejected rather than assigned
a guessed capacity. The driver owns staging permanently; there is no safe
staging accessor, split/reborrow, resource-return or recovery method. Each write
is divided into staging-sized chunks. Static RAM cost and one CPU copy per byte
are explicit; task scheduling and rearming can introduce gaps between chunks.

Normal reset or trustworthy clean-runtime entry remains the HAL platform
premise. The existing one-time DMA admission records a clean startup before
issuing a safe channel capability. This constructor uses that admission, not a
fresh register snapshot or a raw-to-safe channel conversion. Raw channel
acquisition permanently revokes safe eligibility. Startup observations can
reject dirty state but cannot establish that an arbitrary bootloader's prior
bus accesses drained. The raw PAC remains a technically unsafe ownership escape
within the project's existing chiptool model. See
[the startup boundary](dma-safe-owned-copy.md#supported-platform-boundary).

A chunk is copied while idle, then its persistent reservation is established
before DMA starts. The HAL programs the typed UART TDR endpoint, incrementing
SRAM source, fixed destination, equal byte widths, bounded count, REPEAT=1 and
hardware BLOCK request. It enables the channel before UART DMATX, following the
own manual's sequence. The selected-channel ISR records its result using the
existing critical-section/waker protocol and R1W0 flag-clear seed. TE takes
precedence over TC. Successful reuse requires recorded TC without TE and
STATUS=Complete; the driver closes DMATX and orders memory before copying the
next chunk. SOFTSRC=0 is not positive completion evidence for a hardware request.

`write` completes after clean memory-to-TDR transfer. `flush` first settles any
retained DMA chunk, then waits for UART wire completion. UART TXBUSY=0 is the
serial idle condition, not a general DMA cancellation/drain acknowledgment.
Neither interrupt delivery nor successful completion has a finite-time promise.

## Cancellation, errors and teardown

The state belongs to the UART owner, not only to an async future's destructor:

- Dropping an unpolled write launches nothing.
- Cancelling or forgetting a polled write retains the current private chunk
  in flight. A prefix may already have been transmitted; the unscheduled
  remainder is not sent. The caller's input never became a DMA address.
- A later write or flush waits for the retained chunk's existing outcome.
  Clean completion allows reuse; a replacement cannot overwrite staging while
  that chunk remains active. Even an empty async write settles previous work.
  This is normal-completion reaping, not abort.
- TE, TE+TC or inconsistent completion permanently quarantines the DMA lease.
  A later TC does not rehabilitate it. Transfer/flush calls return an error;
  no staging reuse, channel restart or safe resource return is available.
- Dropping an idle owner performs normal UART shutdown. Drop may first reap a
  currently observable clean TC with Complete status. Otherwise an owner with
  unconfirmed in-flight work or a quarantine retains the static staging,
  channel, UART/pin ownership and required clocks. UART waiter interrupts are
  masked; a combined owner's live RX peer keeps UART dispatch active. The UART
  DMA request is not disabled on this abandoned/error path. The retained DMA state can still
  record a terminal outcome. Resource and power retention is intentional.
  It does not promise early bus drain.

Blocking methods on the same owner make one nonblocking terminal observation,
servicing the selected channel's flags even if CPU interrupts are masked.
Pending work returns `Error::DmaBusy`; clean completion permits the operation.
Once idle, `blocking_write` performs ordinary CPU writes. The first observed
DMA failure returns `Error::Dma(error)`; subsequent transfer/flush calls return
`Error::DmaQuarantined`. No blocking entry bypasses the retained lease.
A cancelled flush after clean DMA completion has the
ordinary UART wire-wait semantics; it does not make already-reclaimed staging
unsafe. No destructor is required to keep separately borrowed caller memory
alive. Rust permits [forgotten futures](https://doc.rust-lang.org/core/mem/fn.forget.html),
so static private storage is the lifetime boundary even when cleanup is skipped.

This TX API introduces no CTS constructor, circular mode, borrowed zero-copy or
safe early-abort guarantee. [Staged RX and combined TX/RX](uart-dma-rx.md) extend
the same ownership model with per-direction live/retained state; an abandoned
half preserves its live peer and both retained halves preserve UART clocks. L083 UART1–6 use the same staged contract with independently reviewed own-family
request and terminal evidence; see [L083 qualification](l083-peripheral-dma.md).
The existing non-DMA UART constructors and low-level unsafe DMA APIs retain
their separate scope.

## Own-source evidence

L083 uses its own CN V2.0 manual and exact-part 24 KiB SRAM metadata, documented
in [the L083 source qualification](l083-peripheral-dma.md). The x030 sources and
page references below retain their original family scope.

The selected authoritative behavior source is the shared F030/A030
[CW32x030 User Manual CN V2.5](https://www.whxy.com/uploads/files/20240920/CW32x030_UserManual_CN_V2.5.pdf),
cover date 2024-09 and revision-history date 2024-07-24. Its locked identities
are recorded in `sources/evidence-sources.json`:

- PDF SHA-256: `1afd49261f0f0689af8cb8ebf1b0ac1c00e3209b20d3c722106707ff4a10bdd2`
- Extracted text SHA-256: `23935a54e976c25acfacd689818f272db88b579fc8f15363c2732c12da8773fe`

The retained selected revision was reviewed during the design pass. This change
does not claim a fresh vendor download or a latest-revision comparison. Raw
vendor documents and extracted text are not redistributed in the source tree.
For this PDF, the printed page equals the zero-based PDF index; a conventional
one-based viewer page number is one greater.

| Source location | Supported fact |
|---|---|
| §8.4.1, printed p127 / viewer p128 | Configure descriptors, width, count, increments and request before EN. |
| §8.4.4, printed pp130–132 / viewer pp131–133 | Each hardware BLOCK request transfers one block until the count completes; Table 8-2 supplies the UART request mapping. |
| §8.5, printed p134 / viewer p135 | Matching source/destination widths, count 1–65535 and REPEAT reloaded with 1. |
| §8.6, printed p135 / viewer p136 | TC denotes all data correctly transferred; TE includes address, stop-request and access errors. |
| §8.8.2, printed p138 / viewer p139 | DMA ICR uses R1W0 with reset/no-op seed 0xffffffff; only selected flags are cleared. |
| §8.8.3, printed p139 / viewer p140 | STATUS=101 is complete; STATUS=010 is stop-request abort. EN is enable/disable without a documented drain acknowledgment. |
| §18.6, printed p344 / viewer p345 | UART DMA handshakes until the count completes; DMA TC follows; TX shutdown additionally waits TXBUSY=0. |
| §18.7.1.5, printed p349 / viewer p350, steps 9, 12–25 | Official byte TX example enables the DMA channel, enables DMATX, waits TC and clears DMATX. |

The selected [F030 SDK V2.2](https://www.whxy.com/uploads/files/20241111/CW32F030_StandardPeripheralLib_V2.2.zip)
corroborates this setup: `Libraries/src/cw32f030_uart.c:450–475` controls CR2
DMATX/DMARX; `Libraries/src/cw32f030_dma.c:63–81` configures the channel and
`:119–133` sets/clears EN. Its DeInit at `:44–54` rewrites descriptors after
clearing EN without a drain wait, so it supplies no early-memory-release proof.
The STOPREQ status name at `Libraries/inc/cw32f030_dma.h:234` does not establish
that all outstanding memory accesses have drained. Neither EN-clear nor every
TE cause is treated as clean reclamation evidence.

Existing source-qualified metadata remains unchanged:

- `cw32-data/dma/cw32f030.yaml` and `cw32a030.yaml` give UART2 TX request 3 on
  any of the five physical channels, with DMA_CH1 on DMACH1. The HAL consumes
  generated compatibility instead of copying selector numbers into the example.
- `cw32-data/af/cw32f030.yaml` and `cw32a030.yaml` give PA2 AF2 for UART2 TX;
  their package pinouts bond PA2 on the two C8T7 selections used by the example.
- `cw32-data/parts.yaml` provides exact-part memory capacities. SRAM accessibility
  and byte transfer evidence are retained in `dma-owned-copy-evidence.json`.
  Both example parts have 8 KiB SRAM; no new memory schema is needed.

Pinned Embassy revision `f16efeffe37581092ec184718e6fdb1620393214` supplies the
safe peripheral API/ownership comparison:
[UART constructor and write](https://github.com/embassy-rs/embassy/blob/f16efeffe37581092ec184718e6fdb1620393214/embassy-stm32/src/usart/mod.rs#L541)
and [its low-level DMA teardown](https://github.com/embassy-rs/embassy/blob/f16efeffe37581092ec184718e6fdb1620393214/embassy-stm32/src/dma/dma_bdma/mod.rs#L915).
STM32's reset/suspend behavior is not evidence for equivalent CW32 cancellation.

## Production verification boundary

[The real UART TX example](../examples/uart-dma-tx/README.md) consumes a singleton
staging allocation, sends flash and ordinary local slices longer than staging,
and calls write/flush repeatedly. Its local metadata-derived linker script and
the exact x030/L083 package selections are included in `ci/check-hal.sh`. The linked ELF
must retain actual vectors, an entry/stack and nonempty firmware text; a compile
or empty image is not equivalent to a useful firmware link.

Affected x030/L083 builds, other no-DMA UART profiles and
the existing safe CopyChannel firmware remain regression scope. Source review
must also cover persistent state, error precedence, cancelled/forgotten writes,
all blocking/async/trait entry points, and teardown. Build receipts belong to
the exact source snapshot that was checked; this document is not a declaration
that those checks or any silicon run have passed. No HAL test/harness, synthetic
register model, firmware execution, throughput or liveness result is supplied.
