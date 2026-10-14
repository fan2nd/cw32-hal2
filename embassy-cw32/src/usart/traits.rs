use super::*;

macro_rules! impl_error {
    ($($name:ident),+ $(,)?) => { $(
        impl<M: Mode> embedded_io::ErrorType for $name<'_, M> { type Error = Error; }
    )+ };
}
impl_error!(Uart, UartTx, UartRx);
macro_rules! impl_write {
    ($($name:ident),+ $(,)?) => { $(
        impl<M: Mode> embedded_io::Write for $name<'_, M> {
            fn write(&mut self, buf: &[u8]) -> Result<usize, Error> { self.blocking_write(buf)?; Ok(buf.len()) }
            fn flush(&mut self) -> Result<(), Error> { self.blocking_flush() }
        }
        impl embedded_io_async::Write for $name<'_, Async> {
            async fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
                // The trait requires an empty write to return without waiting.
                // This performs one DMA lease observation, never an await.
                if buf.is_empty() { self.blocking_write(buf)?; return Ok(0); }
                self.write(buf).await?;
                Ok(buf.len())
            }
            async fn flush(&mut self) -> Result<(), Error> { self.flush().await }
        }
    )+ };
}
impl_write!(Uart, UartTx);
// embedded-io Read must return after at least one byte, unlike the inherent
// Embassy-style exact-length read. Receive one byte rather than waiting to fill.
macro_rules! impl_read {
    ($($name:ident),+ $(,)?) => { $(
        impl<M: Mode> embedded_io::Read for $name<'_, M> {
            fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
                if buf.is_empty() { self.check_read()?; return Ok(0); }
                self.blocking_read(&mut buf[..1])?;
                Ok(1)
            }
        }
        impl embedded_io_async::Read for $name<'_, Async> {
            async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
                self.check_read()?;
                if buf.is_empty() { return Ok(0); }
                self.read(&mut buf[..1]).await?;
                Ok(1)
            }
        }
    )+ };
}
impl_read!(Uart, UartRx);

impl embedded_io::ErrorType for BufferedUartRx<'_> {
    type Error = Error;
}
impl embedded_io::ErrorType for BufferedUart<'_> {
    type Error = Error;
}
impl embedded_io_async::Read for BufferedUartRx<'_> {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        self.read(buf).await
    }
}
impl embedded_io_async::Read for BufferedUart<'_> {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        self.rx.read(buf).await
    }
}
impl embedded_io_async::BufRead for BufferedUartRx<'_> {
    async fn fill_buf(&mut self) -> Result<&[u8], Self::Error> {
        BufferedUartRx::fill_buf(self).await
    }
    fn consume(&mut self, amount: usize) {
        BufferedUartRx::consume(self, amount)
    }
}
impl embedded_io_async::BufRead for BufferedUart<'_> {
    async fn fill_buf(&mut self) -> Result<&[u8], Self::Error> {
        BufferedUart::fill_buf(self).await
    }
    fn consume(&mut self, amount: usize) {
        BufferedUart::consume(self, amount)
    }
}
impl embedded_io::ReadReady for BufferedUartRx<'_> {
    fn read_ready(&mut self) -> Result<bool, Error> {
        self.read_ready()
    }
}
impl embedded_io::ReadReady for BufferedUart<'_> {
    fn read_ready(&mut self) -> Result<bool, Error> {
        self.rx.read_ready()
    }
}
impl embedded_io::Write for BufferedUart<'_> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        embedded_io::Write::write(&mut self.tx, buf)
    }
    fn flush(&mut self) -> Result<(), Error> {
        embedded_io::Write::flush(&mut self.tx)
    }
}
impl embedded_io_async::Write for BufferedUart<'_> {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        embedded_io_async::Write::write(&mut self.tx, buf).await
    }
    async fn flush(&mut self) -> Result<(), Error> {
        embedded_io_async::Write::flush(&mut self.tx).await
    }
}
