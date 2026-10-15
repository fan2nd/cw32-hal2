use super::*;

// The lease belongs to the driver, never to an operation future. Only Idle
// permits staging access. A forgotten waiter leaves InFlight intact.
#[cfg(uart_dma_tx)]
#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum TxDmaPhase {
    Idle,
    InFlight,
    Quarantined,
}

#[cfg(uart_dma_tx)]
pub(super) struct TxDma {
    pub(super) channel: crate::dma::Channel<'static>,
    pub(super) staging: &'static mut [u8],
    pub(super) request: crate::dma::Request,
    pub(super) phase: TxDmaPhase,
}

#[cfg(uart_dma_tx)]
impl TxDma {
    pub(super) fn validate_staging(staging: &[u8]) -> Result<(), crate::dma::ConfigError> {
        use crate::dma::ConfigError;
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

    fn poll_complete(
        &mut self,
        regs: pac::uart::Uart,
        cx: Option<&mut Context<'_>>,
    ) -> Poll<Result<(), Error>> {
        match self.phase {
            TxDmaPhase::Idle => return Poll::Ready(Ok(())),
            TxDmaPhase::Quarantined => return Poll::Ready(Err(Error::DmaQuarantined)),
            TxDmaPhase::InFlight => {}
        }
        match self.channel.poll_hardware_complete(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(error)) => {
                self.phase = TxDmaPhase::Quarantined;
                Poll::Ready(Err(Error::Dma(error)))
            }
            Poll::Ready(Ok(())) => {
                // Own manuals §18.7.1.5 (F020 §17.7.1.5, p287): close the
                // request after DMA TC. The channel helper also required
                // Complete STATUS and no TE; F020 §8.6/§8.8.3, pp133/136.
                // Four-channel terminal meaning: own L031/L052/R031/W031
                // manuals pp128/133/130/129, respectively.
                critical_section::with(|_| regs.cr2().modify(|v| v.set_dmatx(false)));
                // SAFETY: this same lease retained all resources continuously;
                // clean completion and request closure precede the fence and
                // retirement. No error path can reach Idle.
                unsafe { self.channel.retire_hardware() };
                self.phase = TxDmaPhase::Idle;
                Poll::Ready(Ok(()))
            }
        }
    }

    pub(super) async fn complete(&mut self, regs: pac::uart::Uart) -> Result<(), Error> {
        core::future::poll_fn(|cx| self.poll_complete(regs, Some(cx))).await
    }

    pub(super) fn check_complete(&mut self, regs: pac::uart::Uart) -> Result<(), Error> {
        match self.poll_complete(regs, None) {
            Poll::Ready(result) => result,
            Poll::Pending => Err(Error::DmaBusy),
        }
    }

    pub(super) async fn write(
        &mut self,
        regs: pac::uart::Uart,
        buffer: &[u8],
    ) -> Result<(), Error> {
        // Reap a cancelled/forgotten write's current chunk before touching SRAM,
        // including on an empty write. Its unscheduled remainder is discarded.
        self.complete(regs).await?;
        for chunk in buffer.chunks(self.staging.len()) {
            self.staging[..chunk.len()].copy_from_slice(chunk);
            // Install the permanent lease state before EN or any possible panic
            // after it. DMA never receives the caller's pointer.
            self.phase = TxDmaPhase::InFlight;
            // SAFETY: the typed constructor fixed the generated request and
            // consumed static UART/channel/pin/staging owners. This driver's
            // Drop retains them on any unconfirmed completion, including panic.
            let result = unsafe {
                self.channel.start_write(
                    self.request,
                    &self.staging[..chunk.len()],
                    regs.tdr().as_ptr().cast::<u8>(),
                )
            };
            if result.is_err() {
                // Validation was completed before admission. An unexpected
                // reservation/configuration failure cannot restore the lease.
                self.phase = TxDmaPhase::Quarantined;
                return Err(Error::DmaQuarantined);
            }
            // Enable the channel before the UART request (§18.7.1.5;
            // F020 §17.7.1.5, p287 also explicitly qualifies byte SRAM-to-TDR).
            // The four own byte-TDR examples cited above preserve this order.
            critical_section::with(|_| regs.cr2().modify(|v| v.set_dmatx(true)));
            self.complete(regs).await?;
        }
        Ok(())
    }
}

#[cfg(uart_dma_tx)]
impl UartTx<'static, Async> {
    /// Create a safe staged DMA transmitter on the qualified
    /// F030/A030/L083/F020/L031/L052/R031/L012 families without CTS.
    ///
    /// Consumes static UART, TX pin, compatible DMA channel and SRAM staging.
    /// Short-lived peripheral reborrows are deliberately excluded. Like other
    /// UART constructors, rejected inputs are consumed; no transfer is started.
    /// The staging slice must contain 1..=65535 bytes in qualified SRAM. Each
    /// write CPU-copies bounded chunks into it; chunk boundaries may have gaps.
    ///
    /// Admission comes only from normal HAL initialization of a reset/clean
    /// runtime, as for [`crate::dma::CopyChannel`]. No raw channel can upgrade.
    /// Cancelling/forgetting a write leaves its current chunk running. A later
    /// write or flush awaits that same clean completion before reuse. Errors
    /// permanently quarantine resources; dropping an unfinished driver retains
    /// its clocks, UART configuration, pin, channel and staging. No finite
    /// completion-time or hardware-throughput guarantee is made.
    pub fn new_with_dma<T, TX, D>(
        _peri: Peri<'static, T>,
        tx: Peri<'static, TX>,
        channel: Peri<'static, D>,
        irq: impl Binding<T::Interrupt, T::InterruptHandler>
        + Binding<D::Interrupt, crate::dma::InterruptHandler<D>>
        + 'static,
        staging: &'static mut [u8],
        config: Config,
    ) -> Result<Self, ConfigError>
    where
        T: Instance,
        TX: TxPin<T>,
        D: crate::dma::RequestRoute<T, crate::dma::signal::TX>,
    {
        let baud = configured_baud::<T>(config)?;
        require_byte_config(config)?;
        TxDma::validate_staging(staging).map_err(ConfigError::DmaBuffer)?;
        let channel = crate::dma::Channel::new_admitted(channel, irq)
            .map_err(|error| ConfigError::DmaChannel(error.error))?;
        let info = T::info();
        info.prepare::<T>(config, baud);
        let mut tx = Self::from_pin::<T, TX>(tx, info, baud.actual, config.data_bits);
        tx.dma = Some(TxDma {
            channel,
            staging,
            request: D::REQUEST,
            phase: TxDmaPhase::Idle,
        });
        critical_section::with(|_| info.regs.cr1().modify(|v| v.set_txen(true)));
        unsafe { T::Interrupt::enable() };
        Ok(tx)
    }
}
