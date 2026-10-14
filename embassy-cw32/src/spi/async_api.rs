use super::*;

#[cfg(any(spi_async_pio, spi_dma))]
impl<'d> Spi<'d, Async, Master> {
    /// Clock `max(read.len(), write.len())` frames, padding TX with zeroes and
    /// discarding excess RX. The constructor-selected backend remains fixed.
    /// PIO supports every [`Word`] and exposes received frames individually;
    /// DMA supports only `u8` and exposes completed chunks. Other word types
    /// return `Error::UnsupportedWordSize` before any MMIO or settlement
    /// on DMA owners, even for empty buffers. No backend fallback occurs.
    /// Earlier results can remain visible on failure or cancellation.
    ///
    /// Supported-word empty calls also settle prior work. After cancellation,
    /// preserve the original CS selection and successfully flush before changing it.
    pub async fn transfer<W: Word>(&mut self, read: &mut [W], write: &[W]) -> Result<(), Error> {
        #[cfg(spi_dma)]
        if self.dma.is_some() {
            let read = W::as_bytes_mut(read).ok_or(Error::UnsupportedWordSize)?;
            let write = W::as_bytes(write).ok_or(Error::UnsupportedWordSize)?;
            return self.transfer_dma(read, write).await;
        }
        #[cfg(spi_async_pio)]
        if self.pio.is_some() {
            return self.run_pio(PioBuffers::Separate { read, write }).await;
        }
        unreachable!("async SPI owner has no installed backend")
    }

    /// Transmit each original frame and replace it with received data. PIO
    /// replaces one frame at a time; DMA replaces each chunk only after clean
    /// paired completion and wire idle. Earlier results may remain visible on
    /// failure or cancellation. Supported-word empty calls also settle prior work.
    /// Preserve the original CS selection until a successful flush.
    /// Word support and DMA rejection follow [`Self::transfer`].
    pub async fn transfer_in_place<W: Word>(&mut self, words: &mut [W]) -> Result<(), Error> {
        #[cfg(spi_dma)]
        if self.dma.is_some() {
            let words = W::as_bytes_mut(words).ok_or(Error::UnsupportedWordSize)?;
            return self.transfer_in_place_dma(words).await;
        }
        #[cfg(spi_async_pio)]
        if self.pio.is_some() {
            return self.run_pio(PioBuffers::InPlace(words)).await;
        }
        unreachable!("async SPI owner has no installed backend")
    }

    /// Transmit words and drain every received frame, settling prior work even
    /// when empty and the word type is supported. After cancellation, keep the
    /// original CS selection until a successful `flush`.
    /// Word support and DMA rejection follow [`Self::transfer`].
    pub async fn write<W: Word>(&mut self, words: &[W]) -> Result<(), Error> {
        self.transfer(&mut [], words).await
    }

    /// Receive words while transmitting zeroes, settling prior work even when
    /// empty and the word type is supported. After cancellation, keep the
    /// original CS selection until a successful `flush`.
    /// Word support and DMA rejection follow [`Self::transfer`].
    pub async fn read<W: Word>(&mut self, words: &mut [W]) -> Result<(), Error> {
        self.transfer(words, &[]).await
    }

    /// Discard abandoned RX and await error-free TXE with BUSY clear using the
    /// installed backend. PIO error recovery may truncate failed work; DMA
    /// errors permanently quarantine the paired resources. Neither error nor
    /// Drop proves that CS can safely change. There is no timeout or guarantee
    /// of finite completion.
    pub async fn flush(&mut self) -> Result<(), Error> {
        #[cfg(spi_dma)]
        if self.dma.is_some() {
            return self.flush_dma().await;
        }
        #[cfg(spi_async_pio)]
        if self.pio.is_some() {
            return self.flush_pio().await;
        }
        unreachable!("async SPI owner has no installed backend")
    }
}
