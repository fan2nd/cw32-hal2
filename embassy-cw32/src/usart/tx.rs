use super::*;

impl<'d> UartTx<'d, Blocking> {
    /// Create a blocking transmit-only UART with active-low hardware CTS.
    pub fn new_blocking_with_cts<T: Instance, TX: TxPin<T>, CTS: CtsPin<T>>(
        _peri: Peri<'d, T>,
        tx: Peri<'d, TX>,
        cts: Peri<'d, CTS>,
        config: Config,
    ) -> Result<Self, ConfigError> {
        let tx = Self::new_inner::<T, TX>(
            tx,
            Some((cts.into(), <CTS as sealed::Pin<T, sealed::Cts>>::AF)),
            config,
        )?;
        Ok(tx)
    }

    /// Create a blocking transmit-only UART.
    pub fn new_blocking<T: Instance, TX: TxPin<T>>(
        _peri: Peri<'d, T>,
        tx: Peri<'d, TX>,
        config: Config,
    ) -> Result<Self, ConfigError> {
        Self::new_inner::<T, TX>(tx, None, config)
    }
}
impl<'d> UartTx<'d, Async> {
    /// Create an async transmit-only UART with active-low hardware CTS.
    pub fn new_with_cts<T: Instance, TX: TxPin<T>, CTS: CtsPin<T>>(
        _peri: Peri<'d, T>,
        tx: Peri<'d, TX>,
        cts: Peri<'d, CTS>,
        _irq: impl Binding<T::Interrupt, T::InterruptHandler>,
        config: Config,
    ) -> Result<Self, ConfigError> {
        let tx = Self::new_inner::<T, TX>(
            tx,
            Some((cts.into(), <CTS as sealed::Pin<T, sealed::Cts>>::AF)),
            config,
        )?;
        unsafe { T::Interrupt::enable() };
        Ok(tx)
    }

    /// Create an interrupt-driven transmit-only UART.
    pub fn new<T: Instance, TX: TxPin<T>>(
        _peri: Peri<'d, T>,
        tx: Peri<'d, TX>,
        _irq: impl Binding<T::Interrupt, T::InterruptHandler>,
        config: Config,
    ) -> Result<Self, ConfigError> {
        let tx = Self::new_inner::<T, TX>(tx, None, config)?;
        unsafe { T::Interrupt::enable() };
        Ok(tx)
    }
    /// Queue bytes, using private staging if constructed with DMA.
    /// Cancellation can leave a transmitted prefix and a running staged chunk.
    /// A later write/flush first waits for that chunk's clean completion.
    pub async fn write(&mut self, buffer: &[u8]) -> Result<(), Error> {
        self.data_bits.check_byte()?;
        #[cfg(uart_dma_tx)]
        if let Some(dma) = self.dma.as_mut() {
            return dma.write(self.info.regs, buffer).await;
        }
        for &byte in buffer {
            Wait::new(self.info.regs, &self.state.tx_waker, WaitEvent::TxReady).await;
            transmit(self.info.regs, byte);
        }
        Ok(())
    }
    /// Wait until the last queued byte has finished transmission.
    pub async fn flush(&mut self) -> Result<(), Error> {
        #[cfg(uart_dma_tx)]
        if let Some(dma) = self.dma.as_mut() {
            dma.complete(self.info.regs).await?;
        }
        Wait::new(self.info.regs, &self.state.tx_waker, WaitEvent::TxComplete).await;
        Ok(())
    }
}
impl<'d, M: Mode> UartTx<'d, M> {
    pub(super) fn from_pin<T: Instance, TX: TxPin<T>>(
        pin: Peri<'d, TX>,
        info: Info,
        actual: u32,
        data_bits: DataBits,
    ) -> Self {
        let mut pin = Flex::new(pin);
        pin.set_as_af(<TX as sealed::Pin<T, sealed::Tx>>::AF, true, Pull::None);
        T::state().activate(Direction::Tx);
        Self {
            info,
            state: T::state(),
            _pin: Some(pin),
            _cts: None,
            #[cfg(uart_dma_tx)]
            dma: None,
            actual_baudrate: actual,
            data_bits,
            _mode: PhantomData,
        }
    }

    fn new_inner<T: Instance, TX: TxPin<T>>(
        tx: Peri<'d, TX>,
        cts: Option<(Peri<'d, AnyPin>, u8)>,
        config: Config,
    ) -> Result<Self, ConfigError> {
        let baud = configured_baud::<T>(config)?;
        validate_cts_pull(cts.as_ref().map(|(pin, _)| &**pin), config.cts_pull)?;
        let info = T::info();
        info.prepare::<T>(config, baud);
        let mut tx = Self::from_pin::<T, TX>(tx, info, baud.actual, config.data_bits);
        tx._cts = cts.map(|(pin, af)| {
            let mut pin = Flex::new(pin);
            pin.set_as_af(af, false, config.cts_pull);
            pin
        });
        critical_section::with(|_| info.regs.cr2().modify(|w| w.set_ctsen(tx._cts.is_some())));
        critical_section::with(|_| info.regs.cr1().modify(|v| v.set_txen(true)));
        Ok(tx)
    }
    /// Baud rate after divider rounding.
    pub fn actual_baudrate(&self) -> u32 {
        self.actual_baudrate
    }
    /// Queue bytes with CPU writes and blocking status checks.
    /// A DMA owner first reaps clean completion, or returns DmaBusy/quarantine.
    pub fn blocking_write(&mut self, buffer: &[u8]) -> Result<(), Error> {
        self.data_bits.check_byte()?;
        #[cfg(uart_dma_tx)]
        if let Some(dma) = self.dma.as_mut() {
            dma.check_complete(self.info.regs)?;
        }
        for &byte in buffer {
            while !self.info.regs.isr().read().txe() {
                core::hint::spin_loop();
            }
            transmit(self.info.regs, byte);
        }
        Ok(())
    }
    /// Wait for the transmit buffer and shift register to empty.
    /// A DMA owner first reaps clean completion, or returns DmaBusy/quarantine.
    pub fn blocking_flush(&mut self) -> Result<(), Error> {
        #[cfg(uart_dma_tx)]
        if let Some(dma) = self.dma.as_mut() {
            dma.check_complete(self.info.regs)?;
        }
        while self.info.regs.isr().read().txbusy() {
            core::hint::spin_loop();
        }
        Ok(())
    }
}

impl<M: Mode> Drop for UartTx<'_, M> {
    fn drop(&mut self) {
        #[cfg(uart_dma_tx)]
        if let Some(dma) = self.dma.as_mut() {
            if dma.check_complete(self.info.regs).is_err() {
                // EN-clear/TE is not a memory reclamation proof, including
                // F020 STATUS2 (stop-request error, §8.8.3 p136). Retain the
                // static channel/staging/pins and leave UART/DMA clocks and
                // request configuration intact. The DMA ISR remains able to
                // record and mask its own terminal source independently.
                self.info.shutdown(self.state, Direction::Tx, true);
                core::mem::forget(self.dma.take());
                core::mem::forget(self._pin.take());
                core::mem::forget(self._cts.take());
                return;
            }
        }
        self.info.shutdown(self.state, Direction::Tx, false);
    }
}
