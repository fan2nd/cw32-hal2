use super::*;

#[cfg(spi_dma)]
impl State {
    pub(crate) const fn new() -> Self {
        Self {
            inner: critical_section::Mutex::new(RefCell::new(InterruptState {
                active: false,
                error: None,
                waker: None,
            })),
        }
    }

    fn observe(
        &self,
        regs: pac::spi::Spi,
        cx: Option<&mut Context<'_>>,
        finish: bool,
    ) -> Result<pac::spi::regs::Isr, Error> {
        critical_section::with(|cs| {
            let mut inner = self.inner.borrow(cs).borrow_mut();
            if let Some(cx) = cx {
                if !inner
                    .waker
                    .as_ref()
                    .is_some_and(|w| w.will_wake(cx.waker()))
                {
                    inner.waker = Some(cx.waker().clone());
                }
            }
            // Register before reading sticky software/hardware state. For
            // final settlement, mask first and then take the final hardware
            // snapshot; a critical section alone cannot stop hardware flags.
            let enabled = if finish {
                let enabled = regs.ier().read();
                regs.ier().write(|_| {});
                Some(enabled)
            } else {
                None
            };
            let status = regs.isr().read();
            if let Some(error) = inner.error.or_else(|| decode_error(status)) {
                inner.error = Some(error);
                regs.ier().write(|_| {});
                return Err(error);
            }
            if finish && status.txe() && !status.busy() {
                inner.waker = None;
            } else if let Some(enabled) = enabled {
                // Still finishing: restore the previous error enables. A flag
                // raised after the snapshot remains sticky and will wake us.
                regs.ier().write_value(enabled);
            }
            Ok(status)
        })
    }

    fn mask(&self, regs: pac::spi::Spi, deactivate: bool) {
        critical_section::with(|cs| {
            let mut inner = self.inner.borrow(cs).borrow_mut();
            regs.ier().write(|_| {});
            inner.waker = None;
            if deactivate {
                inner.active = false;
            }
        });
    }
}

#[cfg(spi_dma)]
pub(super) fn dispatch_dma_interrupt(regs: pac::spi::Spi, state: &State) {
    let waker = critical_section::with(|cs| {
        let mut inner = state.inner.borrow(cs).borrow_mut();
        if !inner.active {
            return None;
        }
        let status = regs.isr().read();
        let enabled = regs.ier().read();
        let error = if enabled.modf() && status.modf() {
            Some(Error::ModeFault)
        } else if enabled.ov() && status.ov() {
            Some(Error::Overrun)
        } else if enabled.sserr() && status.sserr() {
            Some(Error::ChipSelectFault)
        } else if enabled.ud() && status.ud() {
            Some(Error::Underrun)
        } else {
            None
        };
        if let Some(error) = error {
            inner.error.get_or_insert(error);
            regs.ier().write(|_| {});
            inner.waker.take()
        } else {
            None
        }
    });
    if let Some(waker) = waker {
        waker.wake();
    }
}

// Native request gates live in CR2. Modify only the requested gate bits,
// preserving the sampled EN and unrelated bits; closure is not a drain proof.
// Hardware MODF can independently clear EN, so this RMW is no atomic guarantee
// against hardware events. Sticky errors still quarantine the paired lease.
#[cfg(spi_dma)]
fn close_dma_requests(regs: pac::spi::Spi) {
    #[cfg(spi_cw32l012_v1)]
    regs.cr2().modify(|v| {
        v.set_dmarx(false);
        v.set_dmatx(false);
    });
    #[cfg(not(spi_cw32l012_v1))]
    regs.cr1().modify(|v| {
        v.set_dmarx(false);
        v.set_dmatx(false);
    });
}

#[cfg(spi_dma)]
fn open_dma_rx_request(regs: pac::spi::Spi) {
    #[cfg(spi_cw32l012_v1)]
    regs.cr2().modify(|v| v.set_dmarx(true));
    #[cfg(not(spi_cw32l012_v1))]
    regs.cr1().modify(|v| v.set_dmarx(true));
}

#[cfg(spi_dma)]
fn open_dma_tx_request(regs: pac::spi::Spi) {
    #[cfg(spi_cw32l012_v1)]
    regs.cr2().modify(|v| v.set_dmatx(true));
    #[cfg(not(spi_cw32l012_v1))]
    regs.cr1().modify(|v| v.set_dmatx(true));
}

#[cfg(spi_dma)]
#[derive(Clone, Copy, Eq, PartialEq)]
enum DmaPhase {
    Idle,
    InFlight,
    Finishing,
    Quarantined,
}

// Own the original static RX allocation without retaining a Rust reference
// while DMA may write it. Moving this owner never moves the allocation.
#[cfg(spi_dma)]
struct OwnedRxStaging {
    ptr: *mut u8,
    _ownership: PhantomData<&'static mut [u8]>,
}

#[cfg(spi_dma)]
impl OwnedRxStaging {
    fn new(staging: &'static mut [u8]) -> Self {
        Self {
            ptr: staging.as_mut_ptr(),
            _ownership: PhantomData,
        }
    }

    // Returning the address does not grant access to the bytes. Callers must
    // retain the paired DMA lease and observe its existing access protocol.
    fn as_ptr(&self) -> *mut u8 {
        self.ptr
    }
}

// SAFETY: construction consumes exclusive static byte storage. This non-Copy
// owner contains no thread-affine state; moving it preserves the allocation's
// address and ownership, including while the paired DMA lease is outstanding.
#[cfg(spi_dma)]
unsafe impl Send for OwnedRxStaging {}
// SAFETY: shared access only copies the raw address; it cannot read or mutate
// the allocation through a safe API. SpiDma permits CPU RX access only through
// an exclusive borrow after fenced pair retirement. Its ISRs never dereference
// this pointer. No Rust reference to the bytes is retained during DMA writes.
#[cfg(spi_dma)]
unsafe impl Sync for OwnedRxStaging {}

#[cfg(spi_dma)]
pub(super) struct SpiDma {
    tx: dma::Channel<'static>,
    rx: dma::Channel<'static>,
    tx_staging: &'static mut [u8],
    rx_staging: OwnedRxStaging,
    capacity: usize,
    tx_request: dma::Request,
    rx_request: dma::Request,
    state: &'static State,
    phase: DmaPhase,
}

#[cfg(spi_dma)]
impl SpiDma {
    fn validate_staging(staging: &[u8]) -> Result<(), dma::ConfigError> {
        use dma::ConfigError;
        if staging.is_empty() || staging.len() > u16::MAX as usize {
            return Err(ConfigError::InvalidLength);
        }
        let (base, size) = crate::DMA_COPY_SRAM.ok_or(ConfigError::MissingMemoryMetadata)?;
        let start = staging.as_ptr() as usize;
        let end = start
            .checked_add(staging.len())
            .ok_or(ConfigError::AddressOutOfRange)?;
        let limit = (base as usize)
            .checked_add(size as usize)
            .ok_or(ConfigError::AddressOutOfRange)?;
        if start < base as usize || end > limit {
            return Err(ConfigError::NotInSram);
        }
        Ok(())
    }

    fn quarantine(&mut self, regs: pac::spi::Spi, error: Error) -> Error {
        self.phase = DmaPhase::Quarantined;
        self.state.mask(regs, false);
        error
    }

    pub(super) fn deactivate(&self, regs: pac::spi::Spi) {
        self.state.mask(regs, true);
    }

    pub(super) fn poll_complete(
        &mut self,
        regs: pac::spi::Spi,
        mut cx: Option<&mut Context<'_>>,
    ) -> Poll<Result<(), Error>> {
        if self.phase == DmaPhase::Quarantined {
            return Poll::Ready(Err(Error::DmaQuarantined));
        }
        if self.phase == DmaPhase::InFlight {
            // Poll both even if either is pending: both waiters must register,
            // and a fault in the other channel must be observed promptly.
            let tx = self.tx.poll_hardware_complete(cx.as_deref_mut());
            let rx = self.rx.poll_hardware_complete(cx.as_deref_mut());
            if let Err(error) = self.state.observe(regs, cx.as_deref_mut(), false) {
                return Poll::Ready(Err(self.quarantine(regs, error)));
            }
            if let Poll::Ready(Err(error)) = tx {
                return Poll::Ready(Err(self.quarantine(regs, Error::Dma(error))));
            }
            if let Poll::Ready(Err(error)) = rx {
                return Poll::Ready(Err(self.quarantine(regs, Error::Dma(error))));
            }
            if tx.is_pending() || rx.is_pending() {
                return Poll::Pending;
            }
            // Both clean TC+Complete proofs precede request closure. The pair
            // stays reserved through wire completion and any cancellation.
            close_dma_requests(regs);
            self.phase = DmaPhase::Finishing;
        }
        let status = match self.state.observe(regs, cx.as_deref_mut(), true) {
            Ok(status) => status,
            Err(error) => return Poll::Ready(Err(self.quarantine(regs, error))),
        };
        if status.busy() || !status.txe() {
            // BUSY has no interrupt. After DMA terminals, cooperatively poll
            // this finite-in-healthy-hardware tail instead of losing the wake.
            // No finite-liveness guarantee applies on faulty hardware.
            if let Some(cx) = cx {
                cx.waker().wake_by_ref();
            }
            return Poll::Pending;
        }
        if self.phase == DmaPhase::Finishing {
            // SAFETY: both clean terminals, closed requests, no SPI error and
            // wire idle were observed. Poison before sequential retirements;
            // a panic cannot expose or reuse either half of the reservation.
            self.phase = DmaPhase::Quarantined;
            unsafe {
                self.tx.retire_hardware();
                self.rx.retire_hardware();
            }
            self.phase = DmaPhase::Idle;
        }
        Poll::Ready(Ok(()))
    }

    fn start(&mut self, regs: pac::spi::Spi, count: usize) -> Result<(), Error> {
        debug_assert!(self.phase == DmaPhase::Idle);
        debug_assert!(count > 0 && count <= self.capacity);
        self.phase = DmaPhase::InFlight;
        critical_section::with(|cs| {
            let mut inner = self.state.inner.borrow(cs).borrow_mut();
            inner.error = None;
            regs.ier().write(|v| {
                v.set_modf(true);
                v.set_ov(true);
                v.set_sserr(true);
                v.set_ud(true);
            });
        });
        // SAFETY: static endpoint/pins/clocks and both admitted channels and
        // private allocations are installed on the driver before either EN.
        // The pointer covers the validated RX capacity; no reference is formed
        // until both clean terminals and wire idle have retired this pair.
        // Native DR pointers carry fixed aligned typed-register addresses, not
        // byte-MMIO permissions. Native SIZE selects only SRAM element width.
        let result = unsafe {
            self.rx
                .start_read(
                    self.rx_request,
                    regs.dr().as_ptr().cast::<u8>(),
                    self.rx_staging.as_ptr(),
                    count,
                )
                .and_then(|()| {
                    self.tx.start_write(
                        self.tx_request,
                        &self.tx_staging[..count],
                        regs.dr().as_ptr().cast::<u8>(),
                    )
                })
        };
        if result.is_err() {
            // Includes a second-channel failure after RX was armed. Even that
            // case retains the entire pair; no partial channel recovery.
            return Err(self.quarantine(regs, Error::DmaQuarantined));
        }
        // Both descriptors are armed with gates closed. RX-ready-first avoids
        // opening a receive window; this ordering is a conservative choice.
        open_dma_rx_request(regs);
        open_dma_tx_request(regs);
        Ok(())
    }

    fn copy_rx(&mut self, destination: &mut [u8]) {
        debug_assert!(self.phase == DmaPhase::Idle);
        debug_assert!(destination.len() <= self.capacity);
        // SAFETY: successful paired retirement fenced all writes and closed
        // requests. CPU owns these bytes again, with no yield before copying.
        let received =
            unsafe { core::slice::from_raw_parts(self.rx_staging.as_ptr(), destination.len()) };
        destination.copy_from_slice(received);
    }
}

#[cfg(spi_dma)]
impl<M: PeriMode> Spi<'_, M, Master> {
    pub(super) fn check_dma(&mut self) -> Result<(), Error> {
        if let Some(dma) = self.dma.as_mut() {
            match dma.poll_complete(self.peripheral.info.regs, None) {
                Poll::Ready(result) => result,
                Poll::Pending => Err(Error::DmaBusy),
            }
        } else {
            Ok(())
        }
    }
}

#[cfg(spi_dma)]
impl Spi<'static, Async, Master> {
    /// Create a qualified full-duplex byte bus with safe staged DMA.
    /// Available on F030/A030/F020/L031/L052/L083/R031/W031 and native L012
    /// SPI1/SPI2/SPI3; where PIO is also qualified, this constructor selects DMA
    /// for this owner permanently. L012 safe staging and channel admission
    /// require the exact C8T6/C8U6 memory map; the generic profile rejects them.
    /// L012 includes the existing DMA variants of [`ConfigError`] and [`Error`].
    ///
    /// Bind the complete [`Instance::InterruptHandler`] and both DMA channels.
    /// L012 SPI2/SPI3 require `SharedInterruptHandler<SPI23>`, even if only one
    /// peer is used. A dedicated `InterruptHandler<SPI2>` or
    /// `InterruptHandler<SPI3>` is not the complete shared binding. Generic
    /// wrappers must use the associated bound.
    ///
    /// Consumes whole static SPI/pin/channel owners and two nonoverlapping
    /// static SRAM slices of 1..=65535 bytes. Inputs remain consumed on error;
    /// both DMA channels are checked before either admission is consumed. A
    /// channel rejection leaves both admission states unchanged, but a later SPI
    /// configuration error does not roll back successful pair admission. Normal
    /// reset/clean-runtime HAL initialization is required. Raw channels cannot
    /// regain safe admission. There is no buffer/channel recovery or split API.
    ///
    /// Caller slices are CPU-copied through the smaller staging capacity. All
    /// successful operations wait for both DMA terminals and wire idle. Errors
    /// permanently retain the pair. Chunk gaps and receive overrun under load
    /// are possible; no throughput or hardware-validation claim is made.
    ///
    /// # Cancellation and chip select
    /// Cancellation/forget leaves the current chunk running, but never accesses
    /// caller memory. Keep the original device selected and successfully await
    /// [`Self::flush`] before deasserting CS, selecting another device or changing
    /// configuration. On error, wire completion is unproven. Generic `SpiDevice`
    /// adapters that release CS on cancellation do not satisfy this contract.
    /// Dropping an unresolved/poisoned bus retains its pins, clocks and pair.
    pub fn new_with_dma<T, SCK, MOSI, MISO, TXD, RXD>(
        peripheral: Peri<'static, T>,
        sck: Peri<'static, SCK>,
        mosi: Peri<'static, MOSI>,
        miso: Peri<'static, MISO>,
        tx_channel: Peri<'static, TXD>,
        rx_channel: Peri<'static, RXD>,
        irq: impl Binding<T::Interrupt, T::InterruptHandler>
        + Binding<TXD::Interrupt, dma::InterruptHandler<TXD>>
        + Binding<RXD::Interrupt, dma::InterruptHandler<RXD>>
        + 'static,
        tx_staging: &'static mut [u8],
        rx_staging: &'static mut [u8],
        config: Config,
    ) -> Result<Self, ConfigError>
    where
        T: Instance,
        SCK: SckPin<T>,
        MOSI: MosiPin<T>,
        MISO: MisoPin<T>,
        TXD: dma::RequestRoute<T, dma::signal::TX>,
        RXD: dma::RequestRoute<T, dma::signal::RX>,
    {
        SpiDma::validate_staging(tx_staging).map_err(ConfigError::DmaBuffer)?;
        SpiDma::validate_staging(rx_staging).map_err(ConfigError::DmaBuffer)?;
        let (tx, rx) = dma::Channel::new_admitted_pair(tx_channel, rx_channel, irq)
            .map_err(ConfigError::DmaChannel)?;
        let mut spi = Self::new_inner(peripheral, sck, mosi, miso, config)?;
        let capacity = tx_staging.len().min(rx_staging.len());
        spi.dma = Some(SpiDma {
            tx,
            rx,
            tx_staging,
            rx_staging: OwnedRxStaging::new(rx_staging),
            capacity,
            tx_request: TXD::REQUEST,
            rx_request: RXD::REQUEST,
            state: T::state(),
            phase: DmaPhase::Idle,
        });
        critical_section::with(|cs| {
            let mut inner = T::state().inner.borrow(cs).borrow_mut();
            inner.error = None;
            inner.waker = None;
            inner.active = true;
        });
        unsafe { T::Interrupt::enable() };
        Ok(spi)
    }
}

#[cfg(spi_dma)]
impl<'d> Spi<'d, Async, Master> {
    pub(super) async fn transfer_dma(
        &mut self,
        read: &mut [u8],
        write: &[u8],
    ) -> Result<(), Error> {
        self.flush_dma().await?;
        let count = read.len().max(write.len());
        let capacity = self.dma.as_ref().unwrap().capacity;
        for offset in (0..count).step_by(capacity) {
            let len = (count - offset).min(capacity);
            self.prepare_dma()?;
            let dma = self.dma.as_mut().unwrap();
            let write_len = write.len().saturating_sub(offset).min(len);
            dma.tx_staging[..len].fill(0);
            if write_len != 0 {
                dma.tx_staging[..write_len].copy_from_slice(&write[offset..offset + write_len]);
            }
            dma.start(self.peripheral.info.regs, len)?;
            self.flush_dma().await?;
            let read_len = read.len().saturating_sub(offset).min(len);
            if read_len != 0 {
                self.dma
                    .as_mut()
                    .unwrap()
                    .copy_rx(&mut read[offset..offset + read_len]);
            }
        }
        Ok(())
    }

    pub(super) async fn transfer_in_place_dma(&mut self, words: &mut [u8]) -> Result<(), Error> {
        self.flush_dma().await?;
        let capacity = self.dma.as_ref().unwrap().capacity;
        for chunk in words.chunks_mut(capacity) {
            self.prepare_dma()?;
            let dma = self.dma.as_mut().unwrap();
            dma.tx_staging[..chunk.len()].copy_from_slice(chunk);
            dma.start(self.peripheral.info.regs, chunk.len())?;
            self.flush_dma().await?;
            self.dma.as_mut().unwrap().copy_rx(chunk);
        }
        Ok(())
    }

    pub(super) async fn flush_dma(&mut self) -> Result<(), Error> {
        core::future::poll_fn(|cx| {
            self.dma
                .as_mut()
                .unwrap()
                .poll_complete(self.peripheral.info.regs, Some(cx))
        })
        .await
    }

    fn prepare_dma(&mut self) -> Result<(), Error> {
        // Previous completion proved Idle. PIO may have selected another word
        // width, so restore eight bits and drain only stale, idle RX data.
        if let Err(error) = self.prepare::<u8>() {
            return Err(self
                .dma
                .as_mut()
                .unwrap()
                .quarantine(self.peripheral.info.regs, error));
        }
        close_dma_requests(self.peripheral.info.regs);
        Ok(())
    }
}
