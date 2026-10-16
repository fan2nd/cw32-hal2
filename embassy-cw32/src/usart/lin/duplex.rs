use super::*;

/// One native LIN UART with both pins, configured once before either direction starts.
///
/// TX and RX retain the existing primitive contracts. External transceiver echo,
/// arbitration, response selection and deadlines belong to the application. No
/// automatic responder, DMA, buffering or conversion to ordinary UART is exposed.
/// Reads do not discard echoed bytes and writes do not pause the receiver.
pub struct LinUart<'d, M: Mode> {
    tx: LinTransmitter<'d, M>,
    rx: LinReceiver<'d, M>,
}
impl<'d> LinUart<'d, Blocking> {
    pub fn new_blocking<T: LinInstance>(
        peri: Peri<'d, T>,
        tx: Peri<'d, impl TxPin<T>>,
        rx: Peri<'d, impl RxPin<T>>,
        config: Config,
        threshold: LinBreakThreshold,
    ) -> Result<Self, ConfigError> {
        Self::new_inner(peri, tx, rx, config, threshold)
    }
}
impl<'d> LinUart<'d, Async> {
    pub fn new<T: LinInstance>(
        peri: Peri<'d, T>,
        tx: Peri<'d, impl TxPin<T>>,
        rx: Peri<'d, impl RxPin<T>>,
        _irq: impl Binding<T::Interrupt, T::InterruptHandler>,
        config: Config,
        threshold: LinBreakThreshold,
    ) -> Result<Self, ConfigError> {
        let owner = Self::new_inner(peri, tx, rx, config, threshold)?;
        unsafe { T::Interrupt::enable() };
        Ok(owner)
    }
}
impl<'d, M: Mode> LinUart<'d, M> {
    fn new_inner<T: LinInstance, TX: TxPin<T>, RX: RxPin<T>>(
        _peri: Peri<'d, T>,
        tx: Peri<'d, TX>,
        rx: Peri<'d, RX>,
        config: Config,
        threshold: LinBreakThreshold,
    ) -> Result<Self, ConfigError> {
        validate_frame(config)?;
        let baud = configured_baud::<T>(config)?;
        let info = T::info();
        info.prepare::<T>(config, baud);
        let tx = UartTx::from_pin::<T, TX>(tx, info, baud.actual, config.data_bits);
        let rx = UartRx::from_pin::<T, RX>(rx, info, baud.actual, config.rx_pull, config.data_bits);
        critical_section::with(|_| {
            info.regs.cr3().modify(|w| {
                w.set_lin(true);
                w.set_brkl(threshold == LinBreakThreshold::Bits11);
            });
            info.regs.cr1().modify(|w| {
                w.set_txen(true);
                w.set_rxen(true);
            });
        });
        Ok(Self { tx: LinTransmitter { tx }, rx: LinReceiver { rx } })
    }
    pub fn actual_baudrate(&self) -> u32 {
        self.tx.actual_baudrate()
    }
    pub fn transmitter(&mut self) -> &mut LinTransmitter<'d, M> {
        &mut self.tx
    }
    pub fn receiver(&mut self) -> &mut LinReceiver<'d, M> {
        &mut self.rx
    }
    /// Borrow independent directions for concurrent polling without mode changes.
    pub fn split_ref(&mut self) -> (&mut LinTransmitter<'d, M>, &mut LinReceiver<'d, M>) {
        (&mut self.tx, &mut self.rx)
    }
    /// Each half shuts down only its own direction. The last live half releases
    /// the clock/IRQ; forgetting either half retains its ownership indefinitely.
    pub fn split(self) -> (LinTransmitter<'d, M>, LinReceiver<'d, M>) {
        (self.tx, self.rx)
    }
}
