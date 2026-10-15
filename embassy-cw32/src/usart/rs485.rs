use super::*;
use crate::gpio::{Level, Speed};

/// Active level of the external transceiver's driver-enable input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum DriverEnablePolarity {
    ActiveHigh,
    ActiveLow,
}

/// Native automatic DE configuration. Both delays use the same hardware value.
#[derive(Clone, Copy, Debug)]
pub struct DriverEnable {
    pub polarity: DriverEnablePolarity,
    /// 0..=31 sample periods before START and after the final STOP.
    /// One period is 1/16, 1/8 or 1/4 bit at the configured oversampling.
    /// Select this against the external transceiver's timing requirements.
    pub delay_samples: u8,
}
impl Default for DriverEnable {
    fn default() -> Self {
        Self {
            polarity: DriverEnablePolarity::ActiveHigh,
            delay_samples: 0,
        }
    }
}

/// L010/L011/L012 PIO UART with hardware DE on an owned RTS route.
///
/// Requires an external RS485 transceiver. DE is independent of receiver enable;
/// echo suppression, bus arbitration, termination and peer timing belong to the
/// application. No DMA, RTS/CTS, split or live mode conversion is exposed.
/// Flush observes UART TX completion, not expiration of the DE hold delay or
/// release of the external bus. Reads do not flush or discard echoed data.
/// Cancellation can leave a transmitted prefix; hardware still controls DE.
/// Drop can truncate output and disconnects the pins; provide an external
/// inactive bias on DE when a disconnected pin must keep the transceiver off.
pub struct Rs485Uart<'d, M: Mode> {
    uart: Uart<'d, M>,
    de: Flex<'d>,
    inactive: Level,
}

impl<'d> Rs485Uart<'d, Blocking> {
    pub fn new_blocking<T: Instance, DE: RtsPin<T>>(
        peri: Peri<'d, T>,
        tx: Peri<'d, impl TxPin<T>>,
        rx: Peri<'d, impl RxPin<T>>,
        de: Peri<'d, DE>,
        config: Config,
        driver_enable: DriverEnable,
    ) -> Result<Self, ConfigError> {
        Self::new_inner(peri, tx, rx, de, config, driver_enable)
    }
}
impl<'d> Rs485Uart<'d, Async> {
    pub fn new<T: Instance, DE: RtsPin<T>>(
        peri: Peri<'d, T>,
        tx: Peri<'d, impl TxPin<T>>,
        rx: Peri<'d, impl RxPin<T>>,
        de: Peri<'d, DE>,
        _irq: impl Binding<T::Interrupt, T::InterruptHandler>,
        config: Config,
        driver_enable: DriverEnable,
    ) -> Result<Self, ConfigError> {
        let uart = Self::new_inner(peri, tx, rx, de, config, driver_enable)?;
        unsafe { T::Interrupt::enable() };
        Ok(uart)
    }
    pub async fn write(&mut self, buffer: &[u8]) -> Result<(), Error> {
        self.uart.write(buffer).await
    }
    pub async fn read(&mut self, buffer: &mut [u8]) -> Result<(), Error> {
        self.uart.read(buffer).await
    }
    pub async fn write_u16(&mut self, buffer: &[u16]) -> Result<(), Error> {
        self.uart.write_u16(buffer).await
    }
    pub async fn read_u16(&mut self, buffer: &mut [u16]) -> Result<(), Error> {
        self.uart.read_u16(buffer).await
    }
    /// Wait for UART transmission; this does not establish DE/bus release.
    pub async fn flush(&mut self) -> Result<(), Error> {
        self.uart.flush().await
    }
}
impl<'d, M: Mode> Rs485Uart<'d, M> {
    fn new_inner<T: Instance, DE: RtsPin<T>>(
        peri: Peri<'d, T>,
        tx: Peri<'d, impl TxPin<T>>,
        rx: Peri<'d, impl RxPin<T>>,
        de: Peri<'d, DE>,
        config: Config,
        driver_enable: DriverEnable,
    ) -> Result<Self, ConfigError> {
        if driver_enable.delay_samples > 31 {
            return Err(ConfigError::InvalidDriverEnableDelay);
        }
        // Ordinary configuration completes frame/clock validation before MMIO;
        // only OVER=0/1/2 are selectable. No TDR write occurs during construction.
        let uart = Uart::new_inner(peri, tx, rx, None, None, config)?;
        let active_low = driver_enable.polarity == DriverEnablePolarity::ActiveLow;
        let inactive = if active_low { Level::High } else { Level::Low };
        let mut de = Flex::new(de);
        de.set_level(inactive);
        // Program polarity/delay while DE is disconnected, then connect its AF.
        uart.tx.info.regs.cr3().modify(|w| {
            w.set_dep(active_low);
            w.set_detime(driver_enable.delay_samples);
            w.set_dem(true);
        });
        de.set_as_af(<DE as sealed::Pin<T, sealed::Rts>>::AF, true, Pull::None);
        Ok(Self { uart, de, inactive })
    }
    pub fn actual_baudrate(&self) -> u32 {
        self.uart.actual_baudrate()
    }
    pub fn blocking_write(&mut self, buffer: &[u8]) -> Result<(), Error> {
        self.uart.blocking_write(buffer)
    }
    pub fn blocking_read(&mut self, buffer: &mut [u8]) -> Result<(), Error> {
        self.uart.blocking_read(buffer)
    }
    pub fn blocking_write_u16(&mut self, buffer: &[u16]) -> Result<(), Error> {
        self.uart.blocking_write_u16(buffer)
    }
    pub fn blocking_read_u16(&mut self, buffer: &mut [u16]) -> Result<(), Error> {
        self.uart.blocking_read_u16(buffer)
    }
    /// Wait for UART transmission; this does not establish DE/bus release.
    pub fn blocking_flush(&mut self) -> Result<(), Error> {
        self.uart.blocking_flush()
    }
}
impl<M: Mode> Drop for Rs485Uart<'_, M> {
    fn drop(&mut self) {
        // Disconnect the peripheral route before DEM=0 exposes ordinary RTS.
        // Normal Uart field Drop then retires RX/TX and releases the gate.
        self.de.set_level(self.inactive);
        self.de.set_as_output(Speed::Default);
        self.uart.tx.info.regs.cr3().modify(|w| w.set_dem(false));
    }
}
