use super::*;

impl<M: PeriMode> embedded_hal::spi::ErrorType for Spi<'_, M, Master> {
    type Error = Error;
}
impl<W: Word> embedded_hal::spi::SpiBus<W> for Spi<'_, Blocking, Master> {
    fn read(&mut self, words: &mut [W]) -> Result<(), Error> {
        self.blocking_read(words)
    }
    fn write(&mut self, words: &[W]) -> Result<(), Error> {
        self.blocking_write(words)
    }
    fn transfer(&mut self, read: &mut [W], write: &[W]) -> Result<(), Error> {
        self.blocking_transfer(read, write)
    }
    fn transfer_in_place(&mut self, words: &mut [W]) -> Result<(), Error> {
        self.blocking_transfer_in_place(words)
    }
    fn flush(&mut self) -> Result<(), Error> {
        self.blocking_flush()
    }
}

#[cfg(any(spi_async_pio, spi_dma))]
impl<'d, W: Word> embedded_hal_async::spi::SpiBus<W> for Spi<'d, Async, Master> {
    async fn read(&mut self, words: &mut [W]) -> Result<(), Error> {
        self.read(words).await
    }
    async fn write(&mut self, words: &[W]) -> Result<(), Error> {
        self.write(words).await
    }
    async fn transfer(&mut self, read: &mut [W], write: &[W]) -> Result<(), Error> {
        self.transfer(read, write).await
    }
    async fn transfer_in_place(&mut self, words: &mut [W]) -> Result<(), Error> {
        self.transfer_in_place(words).await
    }
    async fn flush(&mut self) -> Result<(), Error> {
        self.flush().await
    }
}
