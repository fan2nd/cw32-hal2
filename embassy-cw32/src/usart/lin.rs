use super::*;
use crate::peripherals::UART1;

/// LIN dominant break length in bit periods, bounded by the nine-bit TDR field.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct LinBreakBits(u16);
impl LinBreakBits {
    /// Reject fewer than 13 or more than 511 dominant bit periods.
    pub const fn new(bits: u16) -> Option<Self> {
        if bits >= 13 && bits <= 511 {
            Some(Self(bits))
        } else {
            None
        }
    }
    pub const fn bits(self) -> u16 {
        self.0
    }
}
impl Default for LinBreakBits {
    fn default() -> Self {
        Self(13)
    }
}

/// L010/L011 UART1 LIN transmit primitives with an exclusively owned TX pin.
///
/// Requires an external LIN transceiver. This is not a LIN protocol stack:
/// scheduling, baud/oscillator limits, delimiter timing, protected identifiers,
/// checksums, responses and bus ownership belong to the application. Byte writes
/// queue data; flush waits for UART TXBUSY to clear, not external bus release.
/// No receiver, DMA, flow control, split, or conversion into an ordinary UART is
/// exposed. Cancellation can leave a transmitted prefix or pending command;
/// later operations preserve UART ordering and do not replay that command.
/// Drop can truncate transmission and disconnects TX; qualify external bias.
pub struct LinTransmitter<'d, M: Mode> {
    tx: UartTx<'d, M>,
}
impl<'d> LinTransmitter<'d, Blocking> {
    pub fn new_blocking(
        peri: Peri<'d, UART1>,
        tx: Peri<'d, impl TxPin<UART1>>,
        config: Config,
    ) -> Result<Self, ConfigError> {
        Self::new_inner(peri, tx, config)
    }
}
impl<'d> LinTransmitter<'d, Async> {
    pub fn new(
        peri: Peri<'d, UART1>,
        tx: Peri<'d, impl TxPin<UART1>>,
        _irq: impl Binding<<UART1 as Instance>::Interrupt, <UART1 as Instance>::InterruptHandler>,
        config: Config,
    ) -> Result<Self, ConfigError> {
        let owner = Self::new_inner(peri, tx, config)?;
        unsafe { <UART1 as Instance>::Interrupt::enable() };
        Ok(owner)
    }
    /// Queue bytes in 8N1 format. Call flush for wire completion.
    pub async fn write(&mut self, bytes: &[u8]) -> Result<(), Error> {
        self.tx.write(bytes).await
    }
    pub async fn flush(&mut self) -> Result<(), Error> {
        self.tx.flush().await
    }
    /// Queue a dominant break and wait for UART transmission to finish.
    pub async fn send_break(&mut self, bits: LinBreakBits) -> Result<(), Error> {
        Wait::new(self.tx.info.regs, &self.tx.state.tx_waker, WaitEvent::TxReady).await;
        self.command(bits.bits(), true, false);
        self.tx.flush().await
    }
    /// Send one hardware idle frame (all ones, configured 8N1 frame length).
    /// This is not a programmable one-bit delimiter or a scheduling deadline.
    pub async fn send_idle(&mut self) -> Result<(), Error> {
        Wait::new(self.tx.info.regs, &self.tx.state.tx_waker, WaitEvent::TxReady).await;
        self.command(0, false, true);
        self.tx.flush().await
    }
}
impl<'d, M: Mode> LinTransmitter<'d, M> {
    fn new_inner<TX: TxPin<UART1>>(
        _peri: Peri<'d, UART1>,
        pin: Peri<'d, TX>,
        config: Config,
    ) -> Result<Self, ConfigError> {
        if config.data_bits != DataBits::DataBits8
            || config.parity != Parity::ParityNone
            || config.stop_bits != StopBits::STOP1
        {
            return Err(ConfigError::InvalidLinFrame);
        }
        // Existing validation precedes MMIO. Reset configuration supplies LSB,
        // SIGNAL=SYNC=0; no TDR write or RX enable occurs during construction.
        let tx = UartTx::new_inner::<UART1, TX>(pin, None, config)?;
        critical_section::with(|_| {
            tx.info.regs.cr1().modify(|w| w.set_txen(false));
            tx.info.regs.cr3().modify(|w| w.set_lin(true));
            tx.info.regs.cr1().modify(|w| w.set_txen(true));
        });
        Ok(Self { tx })
    }
    pub fn actual_baudrate(&self) -> u32 {
        self.tx.actual_baudrate()
    }
    pub fn blocking_write(&mut self, bytes: &[u8]) -> Result<(), Error> {
        self.tx.blocking_write(bytes)
    }
    pub fn blocking_flush(&mut self) -> Result<(), Error> {
        self.tx.blocking_flush()
    }
    pub fn blocking_send_break(&mut self, bits: LinBreakBits) -> Result<(), Error> {
        self.wait_ready();
        self.command(bits.bits(), true, false);
        self.tx.blocking_flush()
    }
    /// Send one all-ones hardware idle frame and wait for completion.
    pub fn blocking_send_idle(&mut self) -> Result<(), Error> {
        self.wait_ready();
        self.command(0, false, true);
        self.tx.blocking_flush()
    }
    fn wait_ready(&self) {
        while !self.tx.info.regs.isr().read().txe() {
            core::hint::spin_loop();
        }
    }
    fn command(&mut self, payload: u16, send_break: bool, idle: bool) {
        let mut clear = regs::Icr::write_noop();
        clear.set_tc(false);
        self.tx.info.regs.icr().write_value(clear);
        self.tx.info.regs.tdr().write(|w| {
            w.set_tdr(payload);
            w.set_break(send_break);
            w.set_idle(idle);
        });
    }
}
