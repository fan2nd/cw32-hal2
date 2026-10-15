use super::*;

impl<'d> Uart<'d, Blocking> {
    /// Create a blocking UART with active-low hardware RTS and CTS.
    /// RTS pauses the peer while an unread frame occupies the receive register.
    /// CTS can stall writes and flush indefinitely while the peer is not ready.
    pub fn new_blocking_with_rtscts<T: Instance, RTS: RtsPin<T>, CTS: CtsPin<T>>(
        peri: Peri<'d, T>,
        tx: Peri<'d, impl TxPin<T>>,
        rx: Peri<'d, impl RxPin<T>>,
        rts: Peri<'d, RTS>,
        cts: Peri<'d, CTS>,
        config: Config,
    ) -> Result<Self, ConfigError> {
        let uart = Self::new_inner(
            peri,
            tx,
            rx,
            Some((rts.into(), <RTS as sealed::Pin<T, sealed::Rts>>::AF)),
            Some((cts.into(), <CTS as sealed::Pin<T, sealed::Cts>>::AF)),
            config,
        )?;
        Ok(uart)
    }

    /// Create a blocking UART with owned, instance-checked TX and RX pins.
    pub fn new_blocking<T: Instance>(
        peri: Peri<'d, T>,
        tx: Peri<'d, impl TxPin<T>>,
        rx: Peri<'d, impl RxPin<T>>,
        config: Config,
    ) -> Result<Self, ConfigError> {
        Self::new_inner(peri, tx, rx, None, None, config)
    }
}
impl<'d> Uart<'d, Async> {
    /// Create an async UART with active-low hardware RTS and CTS.
    /// RTS pauses the peer while an unread frame occupies the receive register.
    /// CTS can stall writes and flush indefinitely while the peer is not ready.
    pub fn new_with_rtscts<T: Instance, RTS: RtsPin<T>, CTS: CtsPin<T>>(
        peri: Peri<'d, T>,
        tx: Peri<'d, impl TxPin<T>>,
        rx: Peri<'d, impl RxPin<T>>,
        rts: Peri<'d, RTS>,
        cts: Peri<'d, CTS>,
        _irq: impl Binding<T::Interrupt, T::InterruptHandler>,
        config: Config,
    ) -> Result<Self, ConfigError> {
        let uart = Self::new_inner(
            peri,
            tx,
            rx,
            Some((rts.into(), <RTS as sealed::Pin<T, sealed::Rts>>::AF)),
            Some((cts.into(), <CTS as sealed::Pin<T, sealed::Cts>>::AF)),
            config,
        )?;
        unsafe { T::Interrupt::enable() };
        Ok(uart)
    }

    /// Create an interrupt-driven UART without DMA or a software RX buffer.
    pub fn new<T: Instance>(
        peri: Peri<'d, T>,
        tx: Peri<'d, impl TxPin<T>>,
        rx: Peri<'d, impl RxPin<T>>,
        _irq: impl Binding<T::Interrupt, T::InterruptHandler>,
        config: Config,
    ) -> Result<Self, ConfigError> {
        let uart = Self::new_inner(peri, tx, rx, None, None, config)?;
        unsafe { T::Interrupt::enable() };
        Ok(uart)
    }

    /// Queue all bytes. Nine-bit owners reject byte APIs, even empty calls.
    /// Cancellation can leave an already-transmitted prefix.
    /// Use `flush` to wait for the final stop bit to leave the wire.
    pub async fn write(&mut self, buffer: &[u8]) -> Result<(), Error> {
        self.tx.write(buffer).await
    }
    /// Receive exactly `buffer.len()` bytes.
    /// Cancellation may leave a filled prefix. Without DMA, unread hardware
    /// data stays in place; with DMA, the active private chunk is retained for
    /// a later read. Receive/rearm/copy gaps may still lose frames. See
    /// [`UartRx::read`] for the receive and cancellation contract.
    pub async fn read(&mut self, buffer: &mut [u8]) -> Result<(), Error> {
        self.rx.read(buffer).await
    }
    /// Receive a finite prefix until the buffer fills or native RXIDLE is observed.
    /// Available on L010/L011/L012. Nine-bit owners reject this byte API.
    /// On an eight-bit owner, an empty slice returns `Ok(0)` without MMIO;
    /// a nonempty call discards stale idle once on its first poll and can return
    /// zero on a later idle event. This is an inherent API, not the embedded-io
    /// short-read contract. See [`UartRx::read_until_idle`] for the observation
    /// window, error priority, cancellation and possible indefinite-wait limits.
    #[cfg(any(uart_cw32l010_v1, uart_cw32l012_v1))]
    pub async fn read_until_idle(&mut self, buffer: &mut [u8]) -> Result<usize, Error> {
        self.rx.read_until_idle(buffer).await
    }
    /// Wait until both the transmit buffer and shift register are empty.
    pub async fn flush(&mut self) -> Result<(), Error> {
        self.tx.flush().await
    }
}

impl<'d, M: Mode> Uart<'d, M> {
    pub(super) fn new_inner<T: Instance, TX: TxPin<T>, RX: RxPin<T>>(
        _peri: Peri<'d, T>,
        tx: Peri<'d, TX>,
        rx: Peri<'d, RX>,
        rts: Option<(Peri<'d, AnyPin>, u8)>,
        cts: Option<(Peri<'d, AnyPin>, u8)>,
        config: Config,
    ) -> Result<Self, ConfigError> {
        let baud = configured_baud::<T>(config)?;
        validate_cts_pull(cts.as_ref().map(|(pin, _)| &**pin), config.cts_pull)?;
        let info = T::info();
        info.prepare::<T>(config, baud);
        let mut tx = UartTx::from_pin::<T, TX>(tx, info, baud.actual, config.data_bits);
        let mut rx =
            UartRx::from_pin::<T, RX>(rx, info, baud.actual, config.rx_pull, config.data_bits);
        tx._cts = cts.map(|(pin, af)| {
            let mut pin = Flex::new(pin);
            pin.set_as_af(af, false, config.cts_pull);
            pin
        });
        rx._rts = rts.map(|(pin, af)| {
            let mut pin = Flex::new(pin);
            pin.set_as_af(af, true, Pull::None);
            pin
        });
        critical_section::with(|_| {
            info.regs.cr2().modify(|w| {
                w.set_ctsen(tx._cts.is_some());
                w.set_rtsen(rx._rts.is_some());
            });
            info.regs.cr1().modify(|w| {
                w.set_txen(true);
                w.set_rxen(true);
            });
        });
        Ok(Self { tx, rx })
    }
    /// Split into independent transmitter and receiver ownership.
    pub fn split(self) -> (UartTx<'d, M>, UartRx<'d, M>) {
        (self.tx, self.rx)
    }
    /// Borrow both halves to use concurrently without consuming the UART.
    pub fn split_ref(&mut self) -> (&mut UartTx<'d, M>, &mut UartRx<'d, M>) {
        (&mut self.tx, &mut self.rx)
    }
    /// Baud rate after divider rounding, excluding oscillator tolerance.
    pub fn actual_baudrate(&self) -> u32 {
        self.tx.actual_baudrate
    }
    /// Queue bytes using blocking status checks. Call `blocking_flush` afterward
    /// if completion on the wire is required.
    pub fn blocking_write(&mut self, buffer: &[u8]) -> Result<(), Error> {
        self.tx.blocking_write(buffer)
    }
    /// Read exactly the requested number of bytes using blocking status checks.
    pub fn blocking_read(&mut self, buffer: &mut [u8]) -> Result<(), Error> {
        self.rx.blocking_read(buffer)
    }
    pub(super) fn check_read(&mut self) -> Result<(), Error> {
        self.rx.check_read()
    }
    /// Wait for all queued data to leave the wire.
    pub fn blocking_flush(&mut self) -> Result<(), Error> {
        self.tx.blocking_flush()
    }
}
