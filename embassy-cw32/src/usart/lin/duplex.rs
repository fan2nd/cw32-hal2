use super::*;

/// One native LIN UART with both pins, configured once before either direction starts.
///
/// TX and RX retain the existing primitive contracts. External transceiver echo,
/// arbitration and deadlines belong to the application. `respond_once` selects
/// from a caller-defined response table. No DMA, buffering or conversion to
/// ordinary UART is exposed.
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
    /// Wait for one matching header, transmit its response, then await TX idle.
    ///
    /// The table must contain only frames this node exclusively publishes.
    /// Duplicate IDs fail before UART access; unknown IDs are skipped. Invalid
    /// headers and UART errors return immediately without retry. An empty table
    /// waits without transmitting. Use a caller deadline to bound this wait.
    ///
    /// Cancellation discards the local parser; a later call requires a new
    /// Break. A queued TX prefix may still leave the UART after cancellation.
    /// The next call first flushes any earlier TX before accepting headers.
    /// RX remains enabled: echo is not compared, and delayed service can overrun.
    /// Success confirms UART idle, not bus acknowledgment or collision freedom.
    /// No response-space deadline or LIN schedule compliance is guaranteed.
    pub async fn respond_once(
        &mut self,
        responses: &[LinResponse],
    ) -> Result<LinIdentifier, LinRespondError> {
        let response = self.select_response(responses).await?;
        self.tx.write_response(&response).await.map_err(LinRespondError::Uart)?;
        self.tx.flush().await.map_err(LinRespondError::Uart)?;
        Ok(response.identifier())
    }

    /// Respond with per-byte echo comparison, including the checksum byte.
    ///
    /// Requires a transceiver that returns each transmitted byte on RX in order
    /// and a network that assigns this response exclusively to this node. Each
    /// byte is queued only after the preceding echo matches. A mismatch, Break
    /// or UART error ends the attempt without queuing further bytes or retrying.
    /// The already queued byte/prefix cannot be recalled. A Break consumed during
    /// echo checking is reported as an error, not reused as a new request header.
    ///
    /// Missing echo waits indefinitely unless a caller deadline cancels this
    /// future. Per-byte task scheduling adds gaps and is not a LIN timing bound.
    /// Matching bytes do not prove collision freedom or identify their sender.
    /// Stale/unrelated RX and overrun are not silently drained. Cancellation and
    /// header admission otherwise follow `respond_once`.
    pub async fn respond_once_with_echo(
        &mut self,
        responses: &[LinResponse],
    ) -> Result<LinIdentifier, LinRespondError> {
        let response = self.select_response(responses).await?;
        for (index, &expected) in response.as_bytes().iter().enumerate() {
            self.tx.write(&[expected]).await.map_err(LinRespondError::Uart)?;
            match self.rx.read_event().await.map_err(LinRespondError::Uart)? {
                LinRxEvent::Data(received) if received == expected => {}
                LinRxEvent::Data(received) => {
                    return Err(LinRespondError::EchoMismatch { index, expected, received });
                }
                LinRxEvent::Break => return Err(LinRespondError::EchoBreak { index }),
            }
        }
        self.tx.flush().await.map_err(LinRespondError::Uart)?;
        Ok(response.identifier())
    }

    async fn select_response(
        &mut self,
        responses: &[LinResponse],
    ) -> Result<LinResponse, LinRespondError> {
        let mut responder = LinResponder::new(responses)
            .map_err(LinRespondError::DuplicateIdentifier)?;
        self.tx.flush().await.map_err(LinRespondError::Uart)?;
        loop {
            let event = self.rx.read_event().await.map_err(LinRespondError::Uart)?;
            if let Some(response) = responder.push(event).map_err(LinRespondError::Header)? {
                return Ok(response);
            }
        }
    }

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
