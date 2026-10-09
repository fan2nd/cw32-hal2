# Safe staged F030/A030/L083 SPI DMA

F030/A030/L083 `Spi<'static, Async, Master>::new_with_dma` consumes whole static SPI,
SCK/MOSI/MISO, two admitted DMA-channel tokens, and two exclusive static SRAM
slices. Its ordinary borrowed `transfer`, `transfer_in_place`, `read`, `write`
and `flush` methods implement `embedded_hal_async::spi::SpiBus<u8>`. Caller
memory is never a DMA endpoint. This is a source/build-qualified subset, not a
silicon, throughput or generic device-adapter cancellation qualification.

## Chip select and cancellation

SPI owns SCK/MOSI/MISO, not device CS. Software NSS stays high and no physical
NSS pin is configured. Every successful awaited operation reaches wire idle.
Cancelling or forgetting an operation leaves its active chunk running in
private storage; the unstarted remainder is discarded. Keep the original
device selected and successfully await `bus.flush()` before deselecting,
selecting another device, releasing a device transaction, or changing mode or
frequency. The later flush discards that cancelled chunk's received bytes.

A generic `SpiDevice` adapter that drops CS/releases the lock on cancellation
can assert another device's CS before the next bus call has drained the old
chunk. Thus ordinary `SpiBus` compatibility does not establish cancellation-safe
device composition. No such adapter is implemented or certified here. On an
error the bus stays poisoned, and flush cannot prove that changing CS is safe.
Request-disable and SPI EN=0 are not adopted as an acknowledged early wire-stop
or outstanding-memory-access drain.

## Ownership and state

Both staging slices are validated independently: 1..=65535 bytes wholly inside
the generated `DMA_COPY_SRAM`. Their smaller length is the chunk capacity; safe
exclusive slice ownership establishes nonoverlap. RX ownership is retained as
a private pointer/capacity plus ownership marker after construction, avoiding
any destination reference while DMA may write. Only successful paired
retirement permits a temporary CPU read slice and caller-buffer copy. The
private unsafe `Channel::start_read` accepts a pointer/count for the same reason.

Channels use existing one-time `Channel::new_admitted`; no raw-channel upgrade,
new request table, RCC layer or register adapter is introduced. Constructor
errors consume inputs, and failure admitting the second channel can consume
the first admission without starting either transfer. The existing generated
request routes and SPI GLOBAL interrupt facts supply selectors and bindings.
Normal reset/clean-runtime HAL initialization is required; dirty startup is
rejected, and arbitrary active bootloader handovers are unsupported.

The persistent state belongs to the driver, not an operation future:

- Idle permits staging access, guarded PIO and configuration.
- InFlight reserves both channels, both buffers, SPI configuration, pins and
  clocks before either DMA channel is enabled.
- Finishing means both channels reported clean TC plus Complete STATUS, with
  DMARX/DMATX closed, but TXE=1/BUSY=0 still has to be observed.
- Quarantined never returns to Idle. SPI/DMA errors or unexpected launch failure
  retain the whole pair; one successful channel cannot be independently reused.

RX and TX use equal nonzero byte counts, fixed typed DR endpoints, incrementing
memory, REPEAT=1 and hardware BLOCK mode. RX is armed before TX with request
gates closed. DMARX is opened before DMATX after both descriptors are ready.
That RX-ready-first choice is a conservative inference; the manual's example
uses TX-channel, RX-channel, TX-request, RX-request order instead.

Both channels are polled even when either is pending, so both register owned
wakers and either DMA error is detected promptly. SPI error polling and the
SPI ISR check MODF/OV/SSERR/UD; the ISR latches the first error, masks owned
interrupt sources and wakes without DR access, flag clear, reset or disable.
Its active-clock guard and all state/IER operations share a critical section.
No ISR stores an owner/future/caller-buffer pointer. Sticky software and hardware
errors are checked after waker registration, including before success. The
final observation masks notification before rereading hardware/software status
in the same critical section. If wire idle is still pending, prior error
enables are restored; sticky flags raised in that interval can still wake it.

After both clean DMA results, requests close and Finishing persists through
wire completion. BUSY has no interrupt: this tail cooperatively self-wakes
while BUSY is set or TXE is clear. Once no errors and wire idle are observed,
notification is masked, both DMA retirements fence the hardware results, and
Idle becomes visible. A transient Quarantined state protects sequential
retirement against panic. Only then can the current receive chunk be copied,
with no await between successful settlement and CPU copy.

Every public blocking method, including empty calls and both blocking-trait
transfer forms, makes a single nonblocking paired-settlement observation before
PIO. Pending reports DmaBusy; quarantine reports DmaQuarantined. Configuration
similarly returns Busy/quarantine before changing registers or pins. PIO's
existing destructive error recovery and word-width/RX drain run only after
this guard proves Idle. Before staged launch, eight-bit width is restored in
case a guarded blocking operation selected u16 or another supported word width.

Drop makes one observation, never a hidden unbounded wait. An unresolved or
poisoned pair masks SPI IER, deactivates the static SPI ISR guard, and forgets
both DMA/storage owners and all three AF pins, returning before SPI disable or
RCC gating. Static DMA handlers may still record/mask their own terminals.
Forgetting the driver also retains all static resources. Clean Drop follows
the original disable/gate/pin-disconnect path. No public free/split/raw
conversion, buffer recovery or DMA-to-PIO conversion is offered.

## Data behavior and costs

`transfer` clocks max(read length, write length) bytes. TX is zero-padded;
excess RX is discarded. `read` transmits zeroes; `write` uses RX DMA as a private
sink. In-place transfer preserves each original chunk in TX staging before
launch and replaces the caller chunk only after successful settlement. Empty
methods still reap an outstanding pair, and all async trait methods forward
to these same inherent methods. Earlier completed chunks remain visible if a
later chunk fails or is cancelled; whole-buffer atomicity is not promised.

Costs include two SRAM buffers, two channel reservations, a TX CPU copy/fill,
and an RX CPU copy when requested. Both intra-chunk arbitration and arbitrary
scheduler gaps between wire-idle chunks affect device compatibility. A higher
priority RX channel helps contention but proves no sustained throughput;
excessive SCK or competing DMA traffic can cause overrun. Clock bounds do not
prove lossless reception. No bounded completion time or safe early abort exists.

## Primary evidence

L083 SPI1/SPI2 use their own CN V2.0 manual paired byte-DMA sequence and error
semantics, as recorded in [L083 qualification](l083-peripheral-dma.md). The
existing L083 12 MHz maximum SCK and minimum divisor 4 remain unchanged. The
x030 sources below retain their original scope.

[CW32x030 User Manual CN V2.5](https://www.whxy.com/uploads/files/20240920/CW32x030_UserManual_CN_V2.5.pdf),
SHA256 `1afd49261f0f0689af8cb8ebf1b0ac1c00e3209b20d3c722106707ff4a10bdd2`.
Printed page numbers equal zero-based PDF indices (viewer page is one higher).

- §8.4.1 p127: descriptors before channel EN; no descriptor reset as abort.
- §8.4.4 pp130–132: request pacing/routes and hardware block triggering.
- §8.5 p134: matching byte widths, 1..65535 count and REPEAT reload.
- §8.6 p135 and §8.8.3 p139: TC means all data correctly transferred; Complete
  STATUS is 101. TE/STOP/EN-clear alone is not adopted as memory quiescence.
- §19.3.8 p375: RXNE data availability, BUSY covers queued/shift data, wait for
  zero before CS high.
- §19.3.9 p376: MODF disables SPI; overrun overwrites old RX data. Those errors
  can prevent the paired DMA counts completing, hence a separate error wake.
- §19.4 p377, §19.8.4 p391: interrupt sources; no BUSY-clear interrupt.
- §19.5 p378: independent TXE/RXNE DMA requests.
- §19.6.1.3 pp381–382, steps 5/12–13/16–44: eight-bit paired descriptors, both
  terminals, close both requests, wait BUSY zero, then SSI high.
- §19.8.1–7 pp388–393: SSM/SSI/EN, request gates, status, R1W0 ICR/FLUSH and DR.

[Official F030 SDK V2.2](https://www.whxy.com/uploads/files/20241111/CW32F030_StandardPeripheralLib_V2.2.zip),
SHA256 `7c431df43d7075b817a51d818ea4c9aba55f83b7c9c77bf0065976758954b780`.
`Libraries/src/cw32f030_spi.c:51–68,326–338` corroborates typed DR and request
bit operations; those operations provide no drain handshake. The unchanged
`Examples/SPI/SPI_DMA/USER/src/main.c:237–263` corroborates paired byte/fixed-DR/
incrementing-memory descriptors. That SDK example is polling master with DMA
slave; the manual's paired-master sequence is the master evidence here.

Authored `cw32-data/registers/spi_v1.yaml`, `cw32-data/dma/`, package metadata and
existing build hooks produce typed PAC endpoints, DMA routes and GLOBAL IRQs.
Generated PAC/chip outputs are build evidence, excluded from source archives.
The initial x030 implementation/source identities and source locators are recorded in
`docs/spi-dma-evidence.json`; external run receipts separately identify the
frozen build inputs and actual firmware ELFs. The real example is
[examples/spi-dma](../examples/spi-dma/README.md). No firmware was executed.
