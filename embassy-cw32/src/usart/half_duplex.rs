use super::*;

/// L010/L011/L012 single-wire UART on its TXD pin, configured directly as open-drain AF.
///
/// Requires an external pull-up and application-controlled turn-taking. Hardware
/// returns to reception after transmission. Every read first flushes queued TX;
/// even a supported empty read can wait indefinitely. Nine-bit owners reject
/// byte reads before that flush, including empty reads. No arbitration, collision
/// detection, peer timing or lossless reception is promised.
///
/// This owner cannot be split: its receiver depends on the transmitter's pin.
/// Drop is destructive and can truncate output; flush explicitly when needed.
/// No DMA, ring buffering, flow control or live full-duplex conversion is exposed.
#[cfg(any(uart_cw32l010_v1, uart_cw32l012_v1))]
pub struct HalfDuplexUart<'d, M: Mode> {
    uart: Uart<'d, M>,
}

#[cfg(any(uart_cw32l010_v1, uart_cw32l012_v1))]
impl<'d> HalfDuplexUart<'d, Blocking> {
    /// Create a blocking single-wire owner. RXD remains available to the caller.
    pub fn new_blocking<T: Instance>(
        peri: Peri<'d, T>,
        tx: Peri<'d, impl TxPin<T>>,
        config: Config,
    ) -> Result<Self, ConfigError> {
        Self::new_inner(peri, tx, config)
    }
}

#[cfg(any(uart_cw32l010_v1, uart_cw32l012_v1))]
impl<'d> HalfDuplexUart<'d, Async> {
    /// Create an interrupt-driven single-wire owner with a typed IRQ binding.
    pub fn new<T: Instance>(
        peri: Peri<'d, T>,
        tx: Peri<'d, impl TxPin<T>>,
        _irq: impl Binding<T::Interrupt, T::InterruptHandler>,
        config: Config,
    ) -> Result<Self, ConfigError> {
        let uart = Self::new_inner(peri, tx, config)?;
        unsafe { T::Interrupt::enable() };
        Ok(uart)
    }
    /// Queue bytes; cancellation may leave a transmitted prefix. Flush for
    /// completion on the wire. Nine-bit owners reject byte calls, even empty.
    pub async fn write(&mut self, buffer: &[u8]) -> Result<(), Error> {
        self.uart.write(buffer).await
    }
    /// Flush queued TX, then receive exactly the requested bytes. Cancellation
    /// retains the owner, may leave a received prefix, and does not retain a
    /// caller buffer in the ISR. An unpolled future has no effects.
    pub async fn read(&mut self, buffer: &mut [u8]) -> Result<(), Error> {
        self.uart.rx.data_bits.check_byte()?;
        self.uart.flush().await?;
        self.uart.read(buffer).await
    }
    /// Queue low 8 or 9 bits of each container, according to configuration.
    pub async fn write_u16(&mut self, buffer: &[u16]) -> Result<(), Error> {
        self.uart.write_u16(buffer).await
    }
    /// Flush queued TX, then receive exactly the requested word containers.
    /// The same empty-call, cancellation and no-buffer-retention rules apply.
    pub async fn read_u16(&mut self, buffer: &mut [u16]) -> Result<(), Error> {
        self.uart.flush().await?;
        self.uart.read_u16(buffer).await
    }
    /// Wait until both TX buffer and shifter are empty. This does not wait for
    /// a peer, arbitrate the bus, or guarantee any peer turnaround interval.
    pub async fn flush(&mut self) -> Result<(), Error> {
        self.uart.flush().await
    }
}

#[cfg(any(uart_cw32l010_v1, uart_cw32l012_v1))]
impl<'d, M: Mode> HalfDuplexUart<'d, M> {
    fn new_inner<T: Instance, TX: TxPin<T>>(
        _peri: Peri<'d, T>,
        tx: Peri<'d, TX>,
        config: Config,
    ) -> Result<Self, ConfigError> {
        // Complete frame/clock/pull preflight before changing UART or GPIO.
        // The TxPin bound proves the package-specific bidirectional TXD route.
        let baud = configured_baud::<T>(config)?;
        let info = T::info();
        info.prepare::<T>(config, baud);
        let mut pin = Flex::new(tx);
        pin.set_high();
        // Do not pass through the ordinary push-pull UartTx::from_pin helper.
        pin.set_as_af_open_drain(<TX as sealed::Pin<T, sealed::Tx>>::AF, config.rx_pull);
        let state = T::state();
        state.activate(Direction::Tx);
        state.activate(Direction::Rx);
        let tx = UartTx {
            info,
            state,
            _pin: Some(pin),
            _cts: None,
            #[cfg(uart_dma_tx)]
            dma: None,
            actual_baudrate: baud.actual,
            data_bits: config.data_bits,
            _mode: PhantomData,
        };
        let rx = UartRx {
            info,
            state,
            _pin: None,
            _rts: None,
            #[cfg(uart_dma_rx)]
            dma: None,
            actual_baudrate: baud.actual,
            data_bits: config.data_bits,
            _mode: PhantomData,
        };
        critical_section::with(|_| {
            info.regs.cr1().modify(|w| {
                w.set_signal(true);
                w.set_txen(true);
                w.set_rxen(true);
            });
        });
        Ok(Self {
            uart: Uart { tx, rx },
        })
    }
    /// Baud rate after divider rounding, excluding oscillator tolerance.
    pub fn actual_baudrate(&self) -> u32 {
        self.uart.actual_baudrate()
    }
    /// Blocking byte write; flush explicitly for wire completion.
    pub fn blocking_write(&mut self, buffer: &[u8]) -> Result<(), Error> {
        self.uart.blocking_write(buffer)
    }
    /// Blocking flush followed by exact byte reception. Nine-bit mode rejects
    /// before any flush, including empty calls. Supported empty reads flush.
    pub fn blocking_read(&mut self, buffer: &mut [u8]) -> Result<(), Error> {
        self.uart.rx.data_bits.check_byte()?;
        self.uart.blocking_flush()?;
        self.uart.blocking_read(buffer)
    }
    /// Blocking low-8/9-bit container write, with ordinary PIO semantics.
    pub fn blocking_write_u16(&mut self, buffer: &[u16]) -> Result<(), Error> {
        self.uart.blocking_write_u16(buffer)
    }
    /// Blocking flush followed by exact low-8/9-bit container reception.
    pub fn blocking_read_u16(&mut self, buffer: &mut [u16]) -> Result<(), Error> {
        self.uart.blocking_flush()?;
        self.uart.blocking_read_u16(buffer)
    }
    /// Wait for queued output to leave the wire, without waiting for a peer.
    pub fn blocking_flush(&mut self) -> Result<(), Error> {
        self.uart.blocking_flush()
    }
}
