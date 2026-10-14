use super::*;

// Word-container APIs deliberately keep the IRQ PIO and staged DMA boundaries
// separate. No caller pointer is retained outside the operation future.
#[cfg(uart_cw32l012_v1)]
impl UartTx<'_, Async> {
    /// Queue u16 containers using ordinary PIO. L012 only; low 8 or 9 bits
    /// are sent according to Config::data_bits, and higher bits are ignored.
    /// DMA owners reject even an empty call before MMIO or lease observation.
    /// Cancellation can leave a transmitted prefix. Flush for wire completion.
    pub async fn write_u16(&mut self, buffer: &[u16]) -> Result<(), Error> {
        self.check_word_transfer()?;
        let mask = self.data_bits.mask();
        for &word in buffer {
            Wait::new(self.info.regs, &self.state.tx_waker, WaitEvent::TxReady).await;
            transmit_word(self.info.regs, word & mask);
        }
        Ok(())
    }
}

#[cfg(uart_cw32l012_v1)]
impl<M: Mode> UartTx<'_, M> {
    fn check_word_transfer(&self) -> Result<(), Error> {
        #[cfg(uart_dma_tx)]
        if self.dma.is_some() {
            return Err(Error::UnsupportedWordTransfer);
        }
        Ok(())
    }
    /// Blocking counterpart of [`UartTx::write_u16`], with the same payload
    /// masking and DMA refusal. Call blocking_flush for wire completion.
    pub fn blocking_write_u16(&mut self, buffer: &[u16]) -> Result<(), Error> {
        self.check_word_transfer()?;
        let mask = self.data_bits.mask();
        for &word in buffer {
            while !self.info.regs.isr().read().txe() {
                core::hint::spin_loop();
            }
            transmit_word(self.info.regs, word & mask);
        }
        Ok(())
    }
}

#[cfg(uart_cw32l012_v1)]
impl UartRx<'_, Async> {
    /// Receive exactly buffer.len() u16 containers using ordinary PIO.
    /// L012 only; each result contains the low configured 8 or 9 payload bits.
    /// Error/acknowledgment order and cancellation follow the ordinary byte
    /// read. An empty supported call retires an old receive wait but reads no
    /// data. DMA owners reject before that retirement, even for empty buffers.
    pub async fn read_u16(&mut self, buffer: &mut [u16]) -> Result<(), Error> {
        self.check_word_transfer()?;
        retire_receive_wait(self.info.regs);
        let mask = self.data_bits.mask();
        for word in buffer {
            loop {
                Wait::new(self.info.regs, &self.state.rx_waker, WaitEvent::Receive).await;
                let status = self.info.regs.isr().read();
                if let Some(received) = receive_observed_word(self.info.regs, status, false)? {
                    *word = received & mask;
                    break;
                }
            }
        }
        Ok(())
    }
}

#[cfg(uart_cw32l012_v1)]
impl<M: Mode> UartRx<'_, M> {
    fn check_word_transfer(&self) -> Result<(), Error> {
        #[cfg(uart_dma_rx)]
        if self.dma.is_some() {
            return Err(Error::UnsupportedWordTransfer);
        }
        Ok(())
    }
    /// Blocking counterpart of [`UartRx::read_u16`], preserving the ordinary
    /// frame-error priority and observed-event acknowledgments.
    pub fn blocking_read_u16(&mut self, buffer: &mut [u16]) -> Result<(), Error> {
        self.check_word_transfer()?;
        retire_receive_wait(self.info.regs);
        let mask = self.data_bits.mask();
        for word in buffer {
            loop {
                let status = self.info.regs.isr().read();
                if let Some(received) = receive_observed_word(self.info.regs, status, false)? {
                    *word = received & mask;
                    break;
                }
                core::hint::spin_loop();
            }
        }
        Ok(())
    }
}

#[cfg(uart_cw32l012_v1)]
impl Uart<'_, Async> {
    /// L012 PIO word-container write; see [`UartTx::write_u16`].
    pub async fn write_u16(&mut self, buffer: &[u16]) -> Result<(), Error> {
        self.tx.write_u16(buffer).await
    }
    /// L012 PIO word-container exact read; see [`UartRx::read_u16`].
    pub async fn read_u16(&mut self, buffer: &mut [u16]) -> Result<(), Error> {
        self.rx.read_u16(buffer).await
    }
}

#[cfg(uart_cw32l012_v1)]
impl<M: Mode> Uart<'_, M> {
    /// L012 PIO blocking word-container write; see [`UartTx::blocking_write_u16`].
    pub fn blocking_write_u16(&mut self, buffer: &[u16]) -> Result<(), Error> {
        self.tx.blocking_write_u16(buffer)
    }
    /// L012 PIO blocking exact word-container read; see [`UartRx::blocking_read_u16`].
    pub fn blocking_read_u16(&mut self, buffer: &mut [u16]) -> Result<(), Error> {
        self.rx.blocking_read_u16(buffer)
    }
}
