use super::*;

/// Hardware break detection threshold, not a proof of the sender's 13-bit minimum.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum LinBreakThreshold {
    Bits10,
    Bits11,
}

/// One best-effort receive observation. Hardware flags do not timestamp ordering.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum LinRxEvent {
    /// RXBRK observed and its sampled RDR was zero, without other receive errors.
    Break,
    Data(u8),
}

/// Owned native LIN receive direction at an application-specified baud rate.
///
/// Uses the same qualified instances as LinTransmitter. LinUart may supply its
/// paired TX direction. No ring, DMA,
/// automatic baud detection or protocol parser is exposed. Service gaps can lose
/// bytes or merge events. A Break is not a recovered frame or a length measurement.
/// Polling/IRQ latency can overwrite RDR; the status/data snapshot is not atomic.
/// On any error the caller must discard its partial frame and resynchronize.
pub struct LinReceiver<'d, M: Mode> {
    pub(super) rx: UartRx<'d, M>,
}
impl<'d> LinReceiver<'d, Blocking> {
    pub fn new_blocking<T: LinInstance>(
        peri: Peri<'d, T>,
        pin: Peri<'d, impl RxPin<T>>,
        config: Config,
        threshold: LinBreakThreshold,
    ) -> Result<Self, ConfigError> {
        Self::new_inner(peri, pin, config, threshold)
    }
}
impl<'d> LinReceiver<'d, Async> {
    pub fn new<T: LinInstance>(
        peri: Peri<'d, T>,
        pin: Peri<'d, impl RxPin<T>>,
        _irq: impl Binding<T::Interrupt, T::InterruptHandler>,
        config: Config,
        threshold: LinBreakThreshold,
    ) -> Result<Self, ConfigError> {
        let owner = Self::new_inner(peri, pin, config, threshold)?;
        unsafe { T::Interrupt::enable() };
        Ok(owner)
    }
    /// Wait indefinitely for a receive event. No stale data/flags are discarded
    /// on entry. Cancellation masks RX/error/break requests without draining data.
    /// Forgetting leaves no caller-buffer pointer; a later call or Drop masks the
    /// abandoned wait. Early FE without RXBRK is reported as Framing.
    pub async fn read_event(&mut self) -> Result<LinRxEvent, Error> {
        retire_receive_wait(self.rx.info.regs);
        loop {
            Wait::new(self.rx.info.regs, &self.rx.state.rx_waker, WaitEvent::ReceiveLin).await;
            if let Some(event) = self.observe()? {
                return Ok(event);
            }
        }
    }
}
impl<'d, M: Mode> LinReceiver<'d, M> {
    fn new_inner<T: LinInstance, RX: RxPin<T>>(
        _peri: Peri<'d, T>,
        pin: Peri<'d, RX>,
        config: Config,
        threshold: LinBreakThreshold,
    ) -> Result<Self, ConfigError> {
        validate_frame(config)?;
        let baud = configured_baud::<T>(config)?;
        let info = T::info();
        info.prepare::<T>(config, baud);
        // prepare leaves both directions disabled and clears inherited events.
        // Select LIN and BRKL before enabling RX, avoiding an ordinary-UART window.
        let rx = UartRx::from_pin::<T, RX>(pin, info, baud.actual, config.rx_pull, config.data_bits);
        critical_section::with(|_| {
            info.regs.cr3().modify(|w| {
                w.set_lin(true);
                w.set_brkl(threshold == LinBreakThreshold::Bits11);
            });
            info.regs.cr1().modify(|w| w.set_rxen(true));
        });
        Ok(Self { rx })
    }
    pub fn actual_baudrate(&self) -> u32 {
        self.rx.actual_baudrate()
    }
    /// One nonwaiting observation, after retiring any abandoned receive wait.
    pub fn try_read_event(&mut self) -> Result<Option<LinRxEvent>, Error> {
        retire_receive_wait(self.rx.info.regs);
        self.observe()
    }
    /// Wait without a timeout. Partial frame assembly belongs to the caller.
    pub fn blocking_read_event(&mut self) -> Result<LinRxEvent, Error> {
        retire_receive_wait(self.rx.info.regs);
        loop {
            if let Some(event) = self.observe()? {
                return Ok(event);
            }
            core::hint::spin_loop();
        }
    }
    fn observe(&mut self) -> Result<Option<LinRxEvent>, Error> {
        let regs = self.rx.info.regs;
        let status = regs.isr().read();
        if !status.rxbrk() {
            return receive_observed(regs, status, false).map(|byte| byte.map(LinRxEvent::Data));
        }
        // RXBRK requires an RDR observation even without a sampled RC. Do not
        // pass its expected FE through the ordinary byte-error path. Read once,
        // then jointly clear RXBRK/FE and only the other sampled receive events.
        let data = regs.rdr().read().rdr();
        let mut command = regs::Icr::write_noop();
        command.set_rxbrk(false);
        command.set_fe(false);
        command.set_rc(!status.rc());
        command.set_pe(!status.pe());
        command.set_ne(!status.ne());
        command.set_ore(!status.ore());
        regs.icr().write_value(command);
        if status.ore() {
            return Err(Error::Overrun);
        }
        if status.pe() {
            return Err(Error::Parity);
        }
        if data != 0 {
            return Err(Error::Framing);
        }
        if status.ne() {
            return Err(Error::Noise);
        }
        Ok(Some(LinRxEvent::Break))
    }
}
