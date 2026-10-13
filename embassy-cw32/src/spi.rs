//! Serial Peripheral Interface (SPI), verified CW32 master backends.
//!
//! This driver follows Embassy's ownership, configuration and blocking bus API.
//! The peripheral and all three signal pins are exclusively owned through
//! [`Peri`]. Chip select is managed separately, for example by an
//! `embedded-hal-bus` device adapter and a GPIO output.
//!
//! Register behavior is from CW32x030 User Manual CN V2.5, chapter 19 (also
//! checked against EN V1.0 and the F030 SDK V2.2), and the L031/R031/W031,
//! L052 and L083 manuals/SDKs for the shared `cw32l031_v1` controller. See
//! `docs/l031-shared-serial.md` for the source audit and conservative limits.
//! F002/F003 and L010/L011/L012 have dedicated register/clock policies; see
//! `docs/remaining-spi-classic-i2c.md`. BR=7 is reserved on old IP, /256 on
//! L010/L011, and L012 has a linear even-divider field. WIDTH=bits-1,
//! ICR flags clear on writing zero, and BUSY must clear before returning.
//!
//! Blocking full-duplex master is available on all supported backends.
//! F030/A030/F020/L031/L052/L083/R031/W031 expose staged byte DMA through
//! `Spi::new_with_dma`. Independently qualified families expose interrupt-driven
//! byte masters through `Spi::new_async`, including F020/L031/L052/R031/W031.
//! The constructor fixes each owner's backend; both use the same async
//! `SpiBus<u8>` methods. A static PIO owner still uses PIO, and separate
//! controllers can use different backends. L012 SPI remains PIO-only.
//!
//! DMA never accesses caller slices. PIO ISRs never access caller memory or DR.
//! Cancellation leaves the current private DMA chunk or at most one PIO frame
//! to settle: preserve the original CS selection and successfully flush before
//! changing it. Generic device adapters that release CS on cancellation are
//! not covered. Dropping an unfinished PIO owner is destructive teardown, not
//! successful wire completion; unresolved DMA owners retain their resources.
//! See `docs/spi-dma.md` and `docs/spi-async-pio.md` for the distinct lifecycles
//! and hardware-validation limits.
#[cfg(spi_dma)]
use crate::dma;
#[cfg(any(spi_dma, spi_async_pio))]
use crate::{
    interrupt::typelevel::{Binding, Handler, Interrupt},
    mode::Async,
};
use core::marker::PhantomData;
#[cfg(any(spi_dma, spi_async_pio))]
use core::{
    cell::RefCell,
    task::{Context, Poll, Waker},
};

pub use embedded_hal::spi::{MODE_0, MODE_1, MODE_2, MODE_3, Mode, Phase, Polarity};

use crate::gpio::{Flex, Pin, Pull};
use crate::mode::{Blocking, Mode as PeriMode};
use crate::rcc::ClockBounds;
use crate::time::Hertz;
use crate::{Peri, PeripheralType, pac};

/// SPI configuration error. Invalid configurations do not touch hardware.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
pub enum ConfigError {
    /// Call [`crate::init`] successfully before constructing a driver.
    ClockNotInitialized,
    /// The requested frequency must be nonzero.
    FrequencyZero,
    /// The request exceeds the family master limit or minimum clock divisor.
    /// x030: 16 MHz; F002/F003/F020 and L031-derived IP: 12 MHz;
    /// L010/L011/L012: 24 MHz. L031-derived IP additionally requires a divisor
    /// of at least 4; all other supported IP requires a divisor of at least 2.
    /// The clock-relative request range uses nominal PCLK, separately from
    /// actual-clock safety.
    FrequencyTooHigh,
    /// The largest divider (/128 on old IP, /256 on L010/L011/L012) is too fast.
    FrequencyTooLow,
    /// This IP does not implement the requested sampling-delay semantics.
    UnsupportedSampleDelay,
    /// The requested weak pull is unavailable on the peripheral input routes.
    UnsupportedInputPull,
    /// The bus must be idle before changing its configuration.
    Busy,
    /// Invalid private DMA staging; constructor inputs are consumed on error.
    #[cfg(spi_dma)]
    DmaBuffer(dma::ConfigError),
    /// The physical DMA channel was not admitted by clean HAL initialization.
    #[cfg(spi_dma)]
    DmaChannel(dma::CopyChannelError),
    /// DMA resources were permanently retained after an unconfirmed transfer.
    #[cfg(spi_dma)]
    DmaQuarantined,
}

impl core::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::ClockNotInitialized => "SPI peripheral clock is not initialized",
            Self::FrequencyZero => "SPI frequency must be nonzero",
            Self::FrequencyTooHigh => "SPI frequency exceeds the supported maximum",
            Self::FrequencyTooLow => "SPI frequency is below the supported divider range",
            Self::UnsupportedSampleDelay => "SPI sampling delay is unsupported on this IP",
            Self::UnsupportedInputPull => "SPI input pull is unsupported on this IP",
            Self::Busy => "SPI bus is busy",
            #[cfg(spi_dma)]
            Self::DmaBuffer(_) => "invalid SPI DMA staging buffer",
            #[cfg(spi_dma)]
            Self::DmaChannel(_) => "SPI DMA channel admission failed",
            #[cfg(spi_dma)]
            Self::DmaQuarantined => "SPI DMA resources are quarantined",
        })
    }
}
impl core::error::Error for ConfigError {}

/// SPI transfer error.
///
/// PIO errors clear buffers/flags and re-enable the configured controller.
/// DMA errors permanently quarantine the paired resources without resetting or
/// flushing hardware. Earlier completed receive chunks may already be visible.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
pub enum Error {
    /// Another master was detected on the bus.
    ModeFault,
    /// A received frame overwrote unread data.
    Overrun,
    /// A transmit frame was not available (normally a slave-only error).
    Underrun,
    /// Chip select changed during a frame (normally a slave-only error).
    ChipSelectFault,
    /// A synchronous call cannot settle a cancelled/forgotten async PIO frame.
    /// Keep the original CS selected and await a successful async flush.
    #[cfg(spi_async_pio)]
    PioBusy,
    /// A DMA channel reported an error or ambiguous terminal state.
    #[cfg(spi_dma)]
    Dma(dma::Error),
    /// A blocking/configuration call cannot settle an outstanding DMA lease.
    #[cfg(spi_dma)]
    DmaBusy,
    /// The paired resources are permanently retained after an error.
    #[cfg(spi_dma)]
    DmaQuarantined,
}
impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::ModeFault => "SPI mode fault",
            Self::Overrun => "SPI receive overrun",
            Self::Underrun => "SPI transmit underrun",
            Self::ChipSelectFault => "SPI chip-select fault",
            #[cfg(spi_async_pio)]
            Self::PioBusy => "SPI async frame is still pending",
            #[cfg(spi_dma)]
            Self::Dma(_) => "SPI DMA transfer failed",
            #[cfg(spi_dma)]
            Self::DmaBusy => "SPI DMA transfer is still running",
            #[cfg(spi_dma)]
            Self::DmaQuarantined => "SPI DMA resources are quarantined",
        })
    }
}
impl core::error::Error for Error {}
impl embedded_hal::spi::Error for Error {
    fn kind(&self) -> embedded_hal::spi::ErrorKind {
        use embedded_hal::spi::ErrorKind;
        match self {
            Self::ModeFault => ErrorKind::ModeFault,
            Self::Overrun => ErrorKind::Overrun,
            Self::ChipSelectFault => ErrorKind::ChipSelectFault,
            Self::Underrun => ErrorKind::Other,
            #[cfg(spi_async_pio)]
            Self::PioBusy => ErrorKind::Other,
            #[cfg(spi_dma)]
            Self::Dma(_) | Self::DmaBusy | Self::DmaQuarantined => ErrorKind::Other,
        }
    }
}

/// SPI bit order.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum BitOrder {
    /// Least significant bit first.
    LsbFirst,
    /// Most significant bit first.
    MsbFirst,
}

/// CW32 master input sampling point.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum SampleDelay {
    /// Sample at the edge selected by CPOL and CPHA.
    None,
    /// Delay input sampling by half an SCK period (CR1.SMP=1).
    ///
    /// Use only when the slave's output timing and board timing require it.
    /// Unsupported on L010/L011/L012: their SMP bit does not promise half a
    /// period. These backends accept only [`Self::None`].
    HalfPeriod,
}

/// SPI bus configuration. Frame width is selected by the transfer's word type.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct Config {
    /// Clock polarity and phase.
    pub mode: Mode,
    /// On-wire bit order.
    pub bit_order: BitOrder,
    /// Maximum actual SCK frequency under the selected RCC source bounds. The
    /// fastest supported divisor whose entire envelope does not exceed this
    /// request is selected; unsupported limits return an error. The bounds
    /// require the documented ambient temperature, supply and factory trim.
    /// Nominal SCK may therefore be lower than with an ideal input clock.
    pub frequency: Hertz,
    /// Weak pull resistor on MISO. L012 has no pull-down on any SPI input route.
    pub input_pull: Pull,
    /// Optional CW32-specific delayed sampling.
    pub sample_delay: SampleDelay,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            mode: MODE_0,
            bit_order: BitOrder::MsbFirst,
            frequency: Hertz(1_000_000),
            input_pull: Pull::None,
            sample_delay: SampleDelay::None,
        }
    }
}

/// SPI communication-mode markers.
pub mod mode {
    mod sealed {
        pub trait Sealed {}
    }
    /// Sealed SPI communication mode. Only master operation is implemented.
    pub trait CommunicationMode: sealed::Sealed {}
    /// SPI master.
    pub struct Master;
    impl sealed::Sealed for Master {}
    impl CommunicationMode for Master {}
}
use mode::{CommunicationMode, Master};

/// Owned SPI bus. SCK/MOSI are push-pull outputs using the family GPIO drive
/// setting (high speed on x030/F020; fixed hardware drive on the low-power families).
///
/// Dropping the bus disables SPI and its peripheral clock, then disconnects its
/// pins. Every successful blocking operation has already waited for bus idle.
/// A staged DMA owner instead retains all resources if its pair cannot be
/// nonblockingly confirmed complete. Cancellation may leave clocks active; keep
/// the original CS selection until a successful async flush.
pub struct Spi<'d, M: PeriMode, CM: CommunicationMode = Master> {
    peripheral: Peri<'d, AnySpi>,
    sck: Option<Flex<'d>>,
    _mosi: Option<Flex<'d>>,
    miso: Option<Flex<'d>>,
    #[cfg(spi_dma)]
    dma: Option<SpiDma>,
    #[cfg(spi_async_pio)]
    pio: Option<PioPhase>,
    sck_af: u8,
    miso_af: u8,
    word_bits: u8,
    kernel_clock: ClockBounds,
    config: Config,
    frequency_bounds: ClockBounds,
    _mode: PhantomData<(M, CM)>,
}

impl<'d> Spi<'d, Blocking, Master> {
    /// Create a blocking, full-duplex master bus.
    ///
    /// # Panics
    /// Panics if clocks have not been initialized or the requested frequency is
    /// out of range. Use [`Self::try_new_blocking`] for checked configuration.
    pub fn new_blocking<T: Instance>(
        peripheral: Peri<'d, T>,
        sck: Peri<'d, impl SckPin<T>>,
        mosi: Peri<'d, impl MosiPin<T>>,
        miso: Peri<'d, impl MisoPin<T>>,
        config: Config,
    ) -> Self {
        Self::try_new_blocking(peripheral, sck, mosi, miso, config)
            .unwrap_or_else(|error| panic!("invalid SPI configuration: {}", error))
    }

    /// Create a blocking bus, returning an error before any hardware changes if
    /// the clock or requested frequency is invalid.
    ///
    /// Pins and the peripheral are consumed even on error; pass `reborrow()`
    /// tokens if they must remain available to the caller after failure.
    pub fn try_new_blocking<T: Instance>(
        peripheral: Peri<'d, T>,
        sck: Peri<'d, impl SckPin<T>>,
        mosi: Peri<'d, impl MosiPin<T>>,
        miso: Peri<'d, impl MisoPin<T>>,
        config: Config,
    ) -> Result<Self, ConfigError> {
        Self::new_inner(peripheral, sck, mosi, miso, config)
    }
}

impl<'d, M: PeriMode> Spi<'d, M, Master> {
    fn new_inner<T: Instance>(
        peripheral: Peri<'d, T>,
        sck: Peri<'d, impl SckPin<T>>,
        mosi: Peri<'d, impl MosiPin<T>>,
        miso: Peri<'d, impl MisoPin<T>>,
        config: Config,
    ) -> Result<Self, ConfigError> {
        let kernel_clock =
            crate::rcc::bus_clock_bounds::<T>().ok_or(ConfigError::ClockNotInitialized)?;
        let sck_af = sealed::SckPin::<T>::af(&*sck);
        let mosi_af = sealed::MosiPin::<T>::af(&*mosi);
        let miso_af = sealed::MisoPin::<T>::af(&*miso);
        let peripheral: Peri<'d, AnySpi> = peripheral.into();
        let info = peripheral.info;
        let prescaler = info.validate_config(kernel_clock, &config)?;

        // A safely reborrowed token can replace a forgotten async owner. Fence
        // its static ISR before Flex changes pins or RCC resets the controller.
        #[cfg(spi_async_pio)]
        info.pio_state().deactivate(info.regs);

        // GPIO Flex begins disconnected. Configure the SPI idle level before
        // connecting the SCK alternate function to the physical output.
        let mut sck = Flex::new(sck);
        let mut mosi = Flex::new(mosi);
        let mut miso = Flex::new(miso);
        critical_section::with(|cs| info.rcc.enable_and_reset_with_cs(cs))
            .expect("SPI clock gate did not enable");
        info.configure(&config, prescaler, 8);
        sck.set_as_af(sck_af, true, config.sck_pull());
        mosi.set_as_af(mosi_af, true, Pull::None);
        miso.set_as_af(miso_af, false, config.input_pull);

        Ok(Self {
            peripheral,
            sck: Some(sck),
            _mosi: Some(mosi),
            miso: Some(miso),
            #[cfg(spi_dma)]
            dma: None,
            #[cfg(spi_async_pio)]
            pio: None,
            sck_af,
            miso_af,
            word_bits: 8,
            kernel_clock,
            config,
            frequency_bounds: prescaler.frequency_bounds,
            _mode: PhantomData,
        })
    }
}

impl<M: PeriMode> Spi<'_, M, Master> {
    /// Nominal configured SCK in whole hertz, rounded down if fractional.
    /// This is not a measured rate; see [`Self::get_current_frequency_bounds`].
    pub fn get_current_frequency(&self) -> Hertz {
        self.frequency_bounds.nominal()
    }

    /// Qualified actual SCK envelope, propagated through the selected divider.
    ///
    /// The whole-hertz endpoints round outward. The guarantee requires the
    /// factory trim and temperature/supply conditions documented by RCC; it
    /// does not certify board signal integrity or the slave's timing.
    pub fn get_current_frequency_bounds(&self) -> ClockBounds {
        self.frequency_bounds
    }

    /// Current requested configuration, including the requested frequency ceiling.
    pub fn get_current_config(&self) -> Config {
        self.config
    }

    /// Reconfigure an idle bus. Invalid settings leave the previous setup intact.
    ///
    /// Changing CPOL can change the physical SCK level. Keep all external chip
    /// selects inactive while changing configuration.
    pub fn set_config(&mut self, config: &Config) -> Result<(), ConfigError> {
        #[cfg(spi_async_pio)]
        if self.pio.is_some() {
            // Configuration cannot consume a hardware error through recovery.
            // Keep it sticky for a transfer/flush and leave configuration intact.
            if !matches!(self.poll_pio_settle(None, &mut false), Poll::Ready(Ok(()))) {
                return Err(ConfigError::Busy);
            }
        }
        #[cfg(spi_dma)]
        self.check_dma().map_err(|error| match error {
            Error::DmaBusy => ConfigError::Busy,
            _ => ConfigError::DmaQuarantined,
        })?;
        let prescaler = self
            .peripheral
            .info
            .validate_config(self.kernel_clock, config)?;
        #[cfg(spi_async_pio)]
        let status = if self.pio.is_some() {
            // Recheck after validation too: a newly observed error must remain
            // available to flush, not be erased by configure's disabled clear.
            self.pio_observe(None, PioWait::Idle)
                .map_err(|_| ConfigError::Busy)?
        } else {
            self.peripheral.info.regs.isr().read()
        };
        #[cfg(not(spi_async_pio))]
        let status = self.peripheral.info.regs.isr().read();
        if status.busy() || !status.txe() {
            return Err(ConfigError::Busy);
        }
        self.peripheral
            .info
            .configure(config, prescaler, self.word_bits);
        // Reapply only the owned pin modes; the AF number is preserved by
        // deriving it from the original generated route at construction.
        self.set_input_pulls(config);
        self.config = *config;
        self.frequency_bounds = prescaler.frequency_bounds;
        Ok(())
    }

    fn set_input_pulls(&mut self, config: &Config) {
        self.sck
            .as_mut()
            .unwrap()
            .set_as_af(self.sck_af, true, config.sck_pull());
        self.miso
            .as_mut()
            .unwrap()
            .set_as_af(self.miso_af, false, config.input_pull);
    }

    /// Write all words, receiving and discarding one frame per transmitted word.
    /// Returns only when TXE is set and BUSY is clear.
    pub fn blocking_write<W: Word>(&mut self, words: &[W]) -> Result<(), Error> {
        self.pio_transfer(&mut [], words)
    }

    /// Read words while transmitting zero-valued words to generate SCK.
    pub fn blocking_read<W: Word>(&mut self, words: &mut [W]) -> Result<(), Error> {
        self.pio_transfer(words, &[])
    }

    /// Simultaneously transmit and receive `max(read.len(), write.len())` frames.
    /// A shorter write buffer is padded with zeroes. Extra received frames are
    /// discarded, including during write-only operations.
    pub fn blocking_transfer<W: Word>(&mut self, read: &mut [W], write: &[W]) -> Result<(), Error> {
        self.pio_transfer(read, write)
    }

    /// Simultaneously transmit and replace each word with its received word.
    pub fn blocking_transfer_in_place<W: Word>(&mut self, words: &mut [W]) -> Result<(), Error> {
        self.pio_transfer_in_place(words)
    }

    /// Wait for the transmit buffer and shift register to become empty.
    pub fn blocking_flush(&mut self) -> Result<(), Error> {
        #[cfg(spi_async_pio)]
        self.check_pio()?;
        #[cfg(spi_dma)]
        self.check_dma()?;
        self.wait_idle()
    }
}

impl<M: PeriMode, CM: CommunicationMode> Drop for Spi<'_, M, CM> {
    fn drop(&mut self) {
        // No ISR may read this bank after its clock is gated. PIO owns no DMA
        // memory lease; unfinished wire work may be truncated by this teardown.
        #[cfg(spi_async_pio)]
        self.peripheral
            .info
            .pio_state()
            .deactivate(self.peripheral.info.regs);
        #[cfg(spi_dma)]
        if let Some(dma) = self.dma.as_mut() {
            let settled = matches!(
                dma.poll_complete(self.peripheral.info.regs, None),
                Poll::Ready(Ok(()))
            );
            dma.deactivate(self.peripheral.info.regs);
            if !settled {
                // Neither request-disable nor EN=0 proves a pending DMA access
                // drained. Retain both channels/storage, SPI clock and AF pins.
                core::mem::forget(self.dma.take());
                core::mem::forget(self.sck.take());
                core::mem::forget(self._mosi.take());
                core::mem::forget(self.miso.take());
                return;
            }
        }
        self.peripheral.info.enable(false);
        self.peripheral.info.regs.ier().write(|v| {
            v.set_txe(false);
            v.set_rxne(false);
            v.set_ssf(false);
            v.set_ssr(false);
            v.set_ud(false);
            v.set_ov(false);
            v.set_sserr(false);
            v.set_modf(false);
        });
        critical_section::with(|cs| self.peripheral.info.rcc.disable_with_cs(cs))
            .expect("SPI clock gate did not disable");
    }
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Prescaler {
    bits: u8,
    frequency_bounds: ClockBounds,
}

impl Prescaler {
    // Encodings are source-backed, not inferred from the field width.
    const MAX_BITS: u8 = if cfg!(spi_cw32l012_v1) {
        127
    } else if cfg!(spi_cw32l010_v1) {
        7
    } else {
        6
    };
    const fn divisor(bits: u8) -> u32 {
        if cfg!(spi_cw32l012_v1) {
            2 * (bits as u32 + 1)
        } else {
            2 << bits
        }
    }
}

impl Config {
    fn sck_pull(&self) -> Pull {
        match self.mode.polarity {
            #[cfg(not(any(spi_cw32l010_v1, spi_cw32l012_v1)))]
            Polarity::IdleLow => Pull::Down,
            // L010/L011 have no pull-down; L012 supports it only on PF3, which
            // has no serial route. The peripheral actively drives idle-low SCK.
            #[cfg(any(spi_cw32l010_v1, spi_cw32l012_v1))]
            Polarity::IdleLow => Pull::None,
            Polarity::IdleHigh => Pull::Up,
        }
    }
}

impl Info {
    fn validate_config(
        &self,
        clock: ClockBounds,
        config: &Config,
    ) -> Result<Prescaler, ConfigError> {
        #[cfg(spi_cw32l012_v1)]
        if config.input_pull == Pull::Down {
            return Err(ConfigError::UnsupportedInputPull);
        }
        if cfg!(any(spi_cw32l010_v1, spi_cw32l012_v1)) && config.sample_delay != SampleDelay::None {
            return Err(ConfigError::UnsupportedSampleDelay);
        }
        self.select_prescaler(clock, config.frequency)
    }
    fn select_prescaler(
        &self,
        clock: ClockBounds,
        requested: Hertz,
    ) -> Result<Prescaler, ConfigError> {
        if clock.nominal().0 == 0 {
            return Err(ConfigError::ClockNotInitialized);
        }
        if requested.0 == 0 {
            return Err(ConfigError::FrequencyZero);
        }
        let minimum_divisor = u64::from(self.minimum_divisor);
        if requested.0 > self.maximum_frequency
            || u64::from(requested.0) * minimum_divisor > u64::from(clock.nominal().0)
        {
            return Err(ConfigError::FrequencyTooHigh);
        }
        for bits in 0..=Prescaler::MAX_BITS {
            let divisor = Prescaler::divisor(bits);
            if divisor < u32::from(self.minimum_divisor) {
                continue;
            }
            // Keep the exact source numerator and cumulative divider. Rounded
            // nominal Hertz is unsuitable for a maximum actual SCK constraint.
            let frequency_bounds = clock.divided_by(divisor);
            if !frequency_bounds.maximum_exceeds(requested.0)
                && !frequency_bounds.maximum_exceeds(self.maximum_frequency)
            {
                return Ok(Prescaler {
                    bits,
                    frequency_bounds,
                });
            }
        }
        Err(ConfigError::FrequencyTooLow)
    }

    fn configure(&self, config: &Config, prescaler: Prescaler, word_bits: u8) {
        let regs = self.regs;
        let width = word_bits - 1;
        self.enable(false);
        regs.ier().write(|v| {
            v.set_txe(false);
            v.set_rxne(false);
            v.set_ssf(false);
            v.set_ssr(false);
            v.set_ud(false);
            v.set_ov(false);
            v.set_sserr(false);
            v.set_modf(false);
        });
        regs.cr2().write(|v| {
            #[cfg(not(any(spi_cw32l010_v1, spi_cw32l012_v1)))]
            v.set_hdoe(false);
            #[cfg(any(spi_cw32l010_v1, spi_cw32l012_v1))]
            v.set_en(false);
            #[cfg(spi_cw32l010_v1)]
            {
                v.set_adcrx(false);
                v.set_adctx(false);
            }
            #[cfg(spi_cw32l012_v1)]
            {
                v.set_dmarx(false);
                v.set_dmatx(false);
            }
        });
        // Low-power IP moves HDOE to CR3 and EN to CR2. Clear CR3 deliberately;
        // CR2=0 also disables DMA/ADC triggers where implemented.
        #[cfg(any(spi_cw32l010_v1, spi_cw32l012_v1))]
        regs.cr3().write(|v| v.set_hdoe(false));
        // SSM=1 makes the unused hardware CS an output controlled by SSI. No
        // physical CS pin is configured here; an external GPIO owns chip select.
        regs.ssi().write(|v| v.set_ssi(true));
        regs.cr1().write(|v| {
            v.set_mstr(true);
            v.set_ssm(true);
            v.set_cpol(config.mode.polarity == Polarity::IdleHigh);
            v.set_cpha(config.mode.phase == Phase::CaptureOnSecondTransition);
            v.set_lsbf(config.bit_order == BitOrder::LsbFirst);
            #[cfg(not(any(spi_cw32l010_v1, spi_cw32l012_v1)))]
            v.set_smp(config.sample_delay == SampleDelay::HalfPeriod);
            // The new low-power SMP bit has different semantics: leave it zero.
            v.set_br(prescaler.bits);
            v.set_width(width);
        });
        // ICR is R1W0, including FLUSH. Reset every owned flag and the
        // transmit/shift buffer only while the controller is disabled.
        self.clear();
        self.enable(true);
    }

    fn enable(&self, enable: bool) {
        #[cfg(not(any(spi_cw32l010_v1, spi_cw32l012_v1)))]
        self.regs.cr1().modify(|v| v.set_en(enable));
        #[cfg(any(spi_cw32l010_v1, spi_cw32l012_v1))]
        self.regs.cr2().modify(|v| v.set_en(enable));
    }
    fn clear(&self) {
        // This deliberately clears every SPI flag and flushes both transmit
        // buffers during disabled initialization/recovery. Begin with the
        // generated R1W0 no-op, then issue each command explicitly; reserved
        // bits retain their documented zero value. Never read-modify-write ICR.
        let mut command = pac::spi::regs::Icr::write_noop();
        command.set_flush(false);
        command.set_rxne(false);
        command.set_ssf(false);
        command.set_ssr(false);
        command.set_ud(false);
        command.set_ov(false);
        command.set_sserr(false);
        command.set_modf(false);
        self.regs.icr().write_value(command);
    }
}

impl<M: PeriMode> Spi<'_, M, Master> {
    fn status(&mut self) -> Result<pac::spi::regs::Isr, Error> {
        let status = self.peripheral.info.regs.isr().read();
        let error = decode_error(status);
        if let Some(error) = error {
            self.peripheral.info.enable(false);
            self.peripheral.info.clear();
            self.peripheral.info.enable(true);
            return Err(error);
        }
        Ok(status)
    }

    fn wait_idle(&mut self) -> Result<(), Error> {
        loop {
            let status = self.status()?;
            if status.txe() && !status.busy() {
                return Ok(());
            }
            core::hint::spin_loop();
        }
    }

    fn prepare<W: Word>(&mut self) -> Result<(), Error> {
        self.wait_idle()?;
        if self.word_bits != W::BITS {
            self.peripheral.info.enable(false);
            self.peripheral
                .info
                .regs
                .cr1()
                .modify(|v| v.set_width(W::BITS - 1));
            self.peripheral.info.enable(true);
            self.word_bits = W::BITS;
        }
        // There is exactly one receive buffer. Read only when it is nonempty;
        // do not clear all ICR flags and accidentally hide a pending error.
        if self.status()?.rxne() {
            let _ = self.peripheral.info.regs.dr().read().dr();
        }
        Ok(())
    }

    fn exchange<W: Word>(&mut self, word: W) -> Result<W, Error> {
        while !self.status()?.txe() {
            core::hint::spin_loop();
        }
        self.peripheral
            .info
            .regs
            .dr()
            .write(|v| v.set_dr(word.into_u16()));
        while !self.status()?.rxne() {
            core::hint::spin_loop();
        }
        Ok(W::from_u16(self.peripheral.info.regs.dr().read().dr()))
    }

    fn pio_transfer<W: Word>(&mut self, read: &mut [W], write: &[W]) -> Result<(), Error> {
        #[cfg(spi_async_pio)]
        self.check_pio()?;
        #[cfg(spi_dma)]
        self.check_dma()?;
        let count = read.len().max(write.len());
        if count == 0 {
            return Ok(());
        }
        self.prepare::<W>()?;
        for index in 0..count {
            let received = self.exchange(write.get(index).copied().unwrap_or_default())?;
            if let Some(word) = read.get_mut(index) {
                *word = received;
            }
        }
        self.wait_idle()
    }

    fn pio_transfer_in_place<W: Word>(&mut self, words: &mut [W]) -> Result<(), Error> {
        #[cfg(spi_async_pio)]
        self.check_pio()?;
        #[cfg(spi_dma)]
        self.check_dma()?;
        if words.is_empty() {
            return Ok(());
        }
        self.prepare::<W>()?;
        for word in words {
            *word = self.exchange(*word)?;
        }
        self.wait_idle()
    }
}

fn decode_error(status: pac::spi::regs::Isr) -> Option<Error> {
    if status.modf() {
        Some(Error::ModeFault)
    } else if status.ov() {
        Some(Error::Overrun)
    } else if status.sserr() {
        Some(Error::ChipSelectFault)
    } else if status.ud() {
        Some(Error::Underrun)
    } else {
        None
    }
}

// IRQ PIO is positively qualified independently of the existing DMA path.
// Only the task accesses DR or caller memory. An abandoned future leaves this
// owner-local phase, never an address into that future, for the next entry.
#[cfg(spi_async_pio)]
#[derive(Clone, Copy, Eq, PartialEq)]
enum PioPhase {
    Idle,
    AwaitRx,
    Finishing,
}

#[cfg(spi_async_pio)]
struct PioInterruptState {
    active: bool,
    error: Option<Error>,
    waker: Option<Waker>,
}

#[cfg(spi_async_pio)]
pub(crate) struct PioState {
    inner: critical_section::Mutex<RefCell<PioInterruptState>>,
}

#[cfg(spi_async_pio)]
#[derive(Clone, Copy)]
enum PioWait {
    Tx,
    Rx,
    Idle,
}

#[cfg(spi_async_pio)]
impl PioWait {
    fn ready(self, status: pac::spi::regs::Isr) -> bool {
        match self {
            Self::Tx => status.txe(),
            Self::Rx => status.rxne(),
            Self::Idle => status.txe() && !status.busy(),
        }
    }
}

#[cfg(spi_async_pio)]
impl PioState {
    pub(crate) const fn new() -> Self {
        Self {
            inner: critical_section::Mutex::new(RefCell::new(PioInterruptState {
                active: false,
                error: None,
                waker: None,
            })),
        }
    }

    fn deactivate(&self, regs: pac::spi::Spi) {
        critical_section::with(|cs| {
            let mut inner = self.inner.borrow(cs).borrow_mut();
            // An inactive bank may be clock-gated. Only a live owner authorizes
            // peripheral MMIO; a shared vector is never disabled or unpended.
            if inner.active {
                regs.ier().write(|_| {});
            }
            inner.active = false;
            inner.waker = None;
            inner.error = None;
        });
    }

    fn cancel_wait(&self, regs: pac::spi::Spi) {
        critical_section::with(|cs| {
            let mut inner = self.inner.borrow(cs).borrow_mut();
            regs.ier().write(|_| {});
            inner.waker = None;
            // Keep the first error and the owner's outstanding frame intact.
        });
    }

    fn observe(
        &self,
        regs: pac::spi::Spi,
        cx: Option<&mut Context<'_>>,
        wait: PioWait,
    ) -> Result<pac::spi::regs::Isr, Error> {
        let arm = cx.is_some();
        critical_section::with(|cs| {
            let mut inner = self.inner.borrow(cs).borrow_mut();
            // Register before either software or hardware completion checks.
            if let Some(cx) = cx {
                if !inner
                    .waker
                    .as_ref()
                    .is_some_and(|w| w.will_wake(cx.waker()))
                {
                    inner.waker = Some(cx.waker().clone());
                }
            } else {
                inner.waker = None;
            }
            // Mask before the final snapshot as well as before changing waits.
            // A critical section alone does not stop peripheral flag changes.
            regs.ier().write(|_| {});
            let mut status = regs.isr().read();
            if let Some(error) = inner.error.or_else(|| decode_error(status)) {
                inner.error = Some(error);
                return Err(error);
            }
            if arm && !wait.ready(status) {
                regs.ier().write(|v| {
                    v.set_txe(matches!(wait, PioWait::Tx));
                    v.set_rxne(matches!(wait, PioWait::Rx));
                    v.set_modf(true);
                    v.set_ov(true);
                    v.set_sserr(true);
                    v.set_ud(true);
                });
                // Recheck after arming: readiness may precede IRQ delivery.
                status = regs.isr().read();
                if let Some(error) = decode_error(status) {
                    inner.error = Some(error);
                    regs.ier().write(|_| {});
                    return Err(error);
                }
            }
            if wait.ready(status) {
                regs.ier().write(|_| {});
                inner.waker = None;
            }
            Ok(status)
        })
    }
}

/// Dedicated SPI interrupt handler for the constructor-selected async backend.
/// DMA owners also require both DMA channel handlers. Each backend checks its
/// own active state before MMIO; an inactive backend never touches the bank.
/// L012 SPI2/SPI3 constructors require the complete `SharedInterruptHandler`
/// instead. The ISR never reads DR or stores a caller-buffer/future pointer.
#[cfg(any(spi_async_pio, spi_dma))]
pub struct InterruptHandler<T: Instance>(PhantomData<T>);

#[cfg(any(spi_async_pio, spi_dma))]
impl<T: Instance> Handler<T::Interrupt> for InterruptHandler<T> {
    unsafe fn on_interrupt() {
        #[cfg(spi_async_pio)]
        dispatch_pio_interrupt(T::regs(), T::pio_state());
        #[cfg(spi_dma)]
        dispatch_dma_interrupt(T::regs(), T::state());
    }
}

/// Complete L012 SPI2/SPI3 handler. Bind it once to the actual `SPI23` vector;
/// either instance's constructor accepts that binding. Inactive members are
/// skipped before MMIO, and one owner's teardown never disables the peer's IRQ.
/// An externally owned inactive peer must have its interrupt source quiescent.
#[cfg(all(spi_async_pio, spi_cw32l012_v1))]
pub struct SharedInterruptHandler<I: Interrupt>(PhantomData<I>);

#[cfg(all(spi_async_pio, spi_cw32l012_v1))]
impl Handler<crate::interrupt::typelevel::SPI23>
    for SharedInterruptHandler<crate::interrupt::typelevel::SPI23>
{
    unsafe fn on_interrupt() {
        dispatch_pio_interrupt(
            pac::SPI2,
            <crate::peripherals::SPI2 as sealed::Instance>::pio_state(),
        );
        dispatch_pio_interrupt(
            pac::SPI3,
            <crate::peripherals::SPI3 as sealed::Instance>::pio_state(),
        );
    }
}

#[cfg(spi_async_pio)]
fn dispatch_pio_interrupt(regs: pac::spi::Spi, state: &PioState) {
    let waker = critical_section::with(|cs| {
        let mut inner = state.inner.borrow(cs).borrow_mut();
        if !inner.active {
            return None;
        }
        let status = regs.isr().read();
        let enabled = regs.ier().read();
        let pending = (enabled.txe() && status.txe())
            || (enabled.rxne() && status.rxne())
            || (enabled.modf() && status.modf())
            || (enabled.ov() && status.ov())
            || (enabled.sserr() && status.sserr())
            || (enabled.ud() && status.ud());
        if !pending {
            return None;
        }
        if let Some(error) = decode_error(status) {
            inner.error.get_or_insert(error);
        }
        // Retain ready flags and data for the task; one notification suffices.
        regs.ier().write(|_| {});
        inner.waker.take()
    });
    if let Some(waker) = waker {
        waker.wake();
    }
}

#[cfg(spi_async_pio)]
impl<M: PeriMode> Spi<'_, M, Master> {
    fn pio_observe(
        &self,
        cx: Option<&mut Context<'_>>,
        wait: PioWait,
    ) -> Result<pac::spi::regs::Isr, Error> {
        let info = self.peripheral.info;
        info.pio_state().observe(info.regs, cx, wait)
    }

    fn recover_pio(&mut self, error: Error) -> Error {
        let info = self.peripheral.info;
        critical_section::with(|cs| {
            let mut inner = info.pio_state().inner.borrow(cs).borrow_mut();
            info.regs.ier().write(|_| {});
            inner.waker = None;
            // Capture the error before destructive recovery. This neither
            // retries the byte nor proves the failed transaction completed.
            self.pio = Some(PioPhase::Idle);
            info.enable(false);
            info.clear();
            info.enable(true);
            inner.error = None;
        });
        error
    }

    fn poll_pio_settle(
        &mut self,
        mut cx: Option<&mut Context<'_>>,
        read_used: &mut bool,
    ) -> Poll<Result<(), Error>> {
        if self.pio == Some(PioPhase::AwaitRx) {
            let status = self.pio_observe(cx.as_deref_mut(), PioWait::Rx)?;
            if !status.rxne() {
                return Poll::Pending;
            }
            if *read_used {
                if let Some(cx) = cx {
                    cx.waker().wake_by_ref();
                }
                return Poll::Pending;
            }
            let _ = self.peripheral.info.regs.dr().read().dr();
            *read_used = true;
            self.pio = Some(PioPhase::Finishing);
        }
        let status = self.pio_observe(cx.as_deref_mut(), PioWait::Idle)?;
        if status.txe() && !status.busy() {
            self.pio = Some(PioPhase::Idle);
            Poll::Ready(Ok(()))
        } else {
            // BUSY has no completion IRQ. No finite-liveness or sleep claim.
            if let Some(cx) = cx {
                cx.waker().wake_by_ref();
            }
            Poll::Pending
        }
    }

    fn check_pio(&mut self) -> Result<(), Error> {
        if self.pio.is_none() {
            return Ok(());
        }
        match self.poll_pio_settle(None, &mut false) {
            Poll::Ready(Ok(())) => Ok(()),
            Poll::Ready(Err(error)) => Err(self.recover_pio(error)),
            Poll::Pending => Err(Error::PioBusy),
        }
    }
}

#[cfg(spi_async_pio)]
struct PioGuard<'a, 'd> {
    driver: &'a mut Spi<'d, Async, Master>,
}

#[cfg(spi_async_pio)]
impl Drop for PioGuard<'_, '_> {
    fn drop(&mut self) {
        let info = self.driver.peripheral.info;
        info.pio_state().cancel_wait(info.regs);
    }
}

#[cfg(spi_async_pio)]
enum PioBuffers<'a> {
    Separate { read: &'a mut [u8], write: &'a [u8] },
    InPlace(&'a mut [u8]),
}

#[cfg(spi_async_pio)]
impl PioBuffers<'_> {
    fn len(&self) -> usize {
        match self {
            Self::Separate { read, write } => read.len().max(write.len()),
            Self::InPlace(words) => words.len(),
        }
    }
    fn transmit(&self, index: usize) -> u8 {
        match self {
            Self::Separate { write, .. } => write.get(index).copied().unwrap_or(0),
            Self::InPlace(words) => words[index],
        }
    }
    fn receive(&mut self, index: usize, byte: u8) {
        match self {
            Self::Separate { read, .. } => {
                if let Some(destination) = read.get_mut(index) {
                    *destination = byte;
                }
            }
            Self::InPlace(words) => words[index] = byte,
        }
    }
}

#[cfg(spi_async_pio)]
impl<'d> Spi<'d, Async, Master> {
    /// Create a qualified interrupt-driven byte master, panicking on invalid
    /// configuration. Use `try_new_async` for the checked constructor.
    /// The owner uses PIO even when its lifetime is static or DMA is available.
    pub fn new_async<T: Instance>(
        peripheral: Peri<'d, T>,
        sck: Peri<'d, impl SckPin<T>>,
        mosi: Peri<'d, impl MosiPin<T>>,
        miso: Peri<'d, impl MisoPin<T>>,
        irqs: impl Binding<T::Interrupt, T::InterruptHandler> + 'd,
        config: Config,
    ) -> Self {
        Self::try_new_async(peripheral, sck, mosi, miso, irqs, config)
            .unwrap_or_else(|error| panic!("invalid SPI configuration: {}", error))
    }

    /// Create a byte master without DMA or static transfer storage. L012 SPI2
    /// and SPI3 require the complete `SharedInterruptHandler<SPI23>` binding.
    /// Tokens are consumed on error; preflight failure leaves hardware intact.
    /// Borrowed owners remain supported on families that also qualify for DMA.
    ///
    /// Cancellation/forget can leave one frame outstanding. Preserve the
    /// original CS selection and successfully await `flush` before changing it.
    /// Dropping the driver is bounded destructive teardown and may truncate a
    /// frame. Neither an error nor Drop is successful wire completion.
    pub fn try_new_async<T: Instance>(
        peripheral: Peri<'d, T>,
        sck: Peri<'d, impl SckPin<T>>,
        mosi: Peri<'d, impl MosiPin<T>>,
        miso: Peri<'d, impl MisoPin<T>>,
        _irqs: impl Binding<T::Interrupt, T::InterruptHandler> + 'd,
        config: Config,
    ) -> Result<Self, ConfigError> {
        let mut spi = Self::new_inner(peripheral, sck, mosi, miso, config)?;
        spi.pio = Some(PioPhase::Idle);
        critical_section::with(|cs| {
            let mut inner = T::pio_state().inner.borrow(cs).borrow_mut();
            inner.error = None;
            inner.waker = None;
            inner.active = true;
        });
        // Shared vectors stay enabled. Never clear the partner's pending IRQ.
        unsafe { T::Interrupt::enable() };
        Ok(spi)
    }

    async fn flush_pio(&mut self) -> Result<(), Error> {
        let guard = PioGuard { driver: self };
        core::future::poll_fn(
            |cx| match guard.driver.poll_pio_settle(Some(cx), &mut false) {
                Poll::Ready(Err(error)) => Poll::Ready(Err(guard.driver.recover_pio(error))),
                other => other,
            },
        )
        .await
    }

    fn poll_pio_prepare(
        &mut self,
        cx: &mut Context<'_>,
        read_used: &mut bool,
    ) -> Poll<Result<(), Error>> {
        // Do not call prepare::<u8>(): its wait_idle loop is unbounded. Width
        // changes and stale RX draining share this poll's one-read budget.
        let status = self.pio_observe(Some(cx), PioWait::Idle)?;
        if !status.txe() || status.busy() {
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        if self.word_bits != 8 {
            self.peripheral.info.enable(false);
            self.peripheral.info.regs.cr1().modify(|v| v.set_width(7));
            self.peripheral.info.enable(true);
            self.word_bits = 8;
        }
        let status = self.pio_observe(Some(cx), PioWait::Idle)?;
        if !status.txe() || status.busy() {
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        if status.rxne() {
            if *read_used {
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
            let _ = self.peripheral.info.regs.dr().read().dr();
            *read_used = true;
        }
        Poll::Ready(Ok(()))
    }

    async fn run_pio(&mut self, mut buffers: PioBuffers<'_>) -> Result<(), Error> {
        let guard = PioGuard { driver: self };
        let count = buffers.len();
        let mut settled = false;
        let mut prepared = false;
        let mut index = 0;
        core::future::poll_fn(|cx| {
            let result = (|| {
                // This one budget covers abandoned RX, stale preparation RX,
                // and the current operation. A poll writes at most one frame.
                let mut read_used = false;
                if !settled {
                    match guard.driver.poll_pio_settle(Some(cx), &mut read_used) {
                        Poll::Ready(Ok(())) => settled = true,
                        other => return other,
                    }
                }
                if count == 0 {
                    return Poll::Ready(Ok(()));
                }
                if !prepared {
                    match guard.driver.poll_pio_prepare(cx, &mut read_used) {
                        Poll::Ready(Ok(())) => prepared = true,
                        other => return other,
                    }
                }
                if guard.driver.pio == Some(PioPhase::AwaitRx) {
                    let status = guard.driver.pio_observe(Some(cx), PioWait::Rx)?;
                    if !status.rxne() {
                        return Poll::Pending;
                    }
                    if read_used {
                        cx.waker().wake_by_ref();
                        return Poll::Pending;
                    }
                    let byte = guard.driver.peripheral.info.regs.dr().read().dr() as u8;
                    read_used = true;
                    guard.driver.pio = Some(PioPhase::Finishing);
                    buffers.receive(index, byte);
                    index += 1;
                }
                if index == count {
                    return guard.driver.poll_pio_settle(Some(cx), &mut read_used);
                }
                let status = guard.driver.pio_observe(Some(cx), PioWait::Tx)?;
                if !status.txe() {
                    return Poll::Pending;
                }
                let byte = buffers.transmit(index);
                // Retained marker precedes DR. No await exists in this command.
                guard.driver.pio = Some(PioPhase::AwaitRx);
                guard
                    .driver
                    .peripheral
                    .info
                    .regs
                    .dr()
                    .write(|v| v.set_dr(u16::from(byte)));
                let status = guard.driver.pio_observe(Some(cx), PioWait::Rx)?;
                if status.rxne() {
                    // Already ready, but never consume a second frame/read or
                    // launch another byte in this poll. The ISR is one-shot.
                    cx.waker().wake_by_ref();
                }
                Poll::Pending
            })();
            match result {
                Poll::Ready(Err(error)) => Poll::Ready(Err(guard.driver.recover_pio(error))),
                other => other,
            }
        })
        .await
    }
}

#[cfg(any(spi_async_pio, spi_dma))]
impl<'d> Spi<'d, Async, Master> {
    /// Clock `max(read.len(), write.len())` bytes, padding TX with zeroes and
    /// discarding excess RX. The constructor-selected backend remains fixed.
    /// PIO exposes received bytes individually; DMA exposes completed chunks.
    /// Earlier results can remain visible on failure or cancellation.
    ///
    /// Empty calls also settle prior work. After cancellation, preserve the
    /// original CS selection and successfully flush before changing it.
    pub async fn transfer(&mut self, read: &mut [u8], write: &[u8]) -> Result<(), Error> {
        #[cfg(spi_dma)]
        if self.dma.is_some() {
            return self.transfer_dma(read, write).await;
        }
        #[cfg(spi_async_pio)]
        if self.pio.is_some() {
            return self.run_pio(PioBuffers::Separate { read, write }).await;
        }
        unreachable!("async SPI owner has no installed backend")
    }

    /// Transmit each original byte and replace it with received data. PIO
    /// replaces one byte at a time; DMA replaces each chunk only after clean
    /// paired completion and wire idle. Earlier results may remain visible on
    /// failure or cancellation. Empty calls also settle prior work.
    /// Preserve the original CS selection until a successful flush.
    pub async fn transfer_in_place(&mut self, words: &mut [u8]) -> Result<(), Error> {
        #[cfg(spi_dma)]
        if self.dma.is_some() {
            return self.transfer_in_place_dma(words).await;
        }
        #[cfg(spi_async_pio)]
        if self.pio.is_some() {
            return self.run_pio(PioBuffers::InPlace(words)).await;
        }
        unreachable!("async SPI owner has no installed backend")
    }

    /// Transmit bytes and drain every received frame, settling prior work even
    /// when empty. After cancellation, keep the original CS selection until a
    /// successful `flush`.
    pub async fn write(&mut self, words: &[u8]) -> Result<(), Error> {
        self.transfer(&mut [], words).await
    }

    /// Receive bytes while transmitting zeroes, settling prior work even when
    /// empty. After cancellation, keep the original CS selection until a
    /// successful `flush`.
    pub async fn read(&mut self, words: &mut [u8]) -> Result<(), Error> {
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

#[cfg(any(spi_async_pio, spi_dma))]
impl<'d> embedded_hal_async::spi::SpiBus<u8> for Spi<'d, Async, Master> {
    async fn read(&mut self, words: &mut [u8]) -> Result<(), Error> {
        self.read(words).await
    }
    async fn write(&mut self, words: &[u8]) -> Result<(), Error> {
        self.write(words).await
    }
    async fn transfer(&mut self, read: &mut [u8], write: &[u8]) -> Result<(), Error> {
        self.transfer(read, write).await
    }
    async fn transfer_in_place(&mut self, words: &mut [u8]) -> Result<(), Error> {
        self.transfer_in_place(words).await
    }
    async fn flush(&mut self) -> Result<(), Error> {
        self.flush().await
    }
}

#[cfg(spi_dma)]
struct InterruptState {
    active: bool,
    error: Option<Error>,
    waker: Option<Waker>,
}

// No pointer into an owner or operation future is retained by an interrupt.
#[cfg(spi_dma)]
pub(crate) struct State {
    inner: critical_section::Mutex<RefCell<InterruptState>>,
}

#[cfg(spi_dma)]
impl State {
    pub(crate) const fn new() -> Self {
        Self {
            inner: critical_section::Mutex::new(RefCell::new(InterruptState {
                active: false,
                error: None,
                waker: None,
            })),
        }
    }

    fn observe(
        &self,
        regs: pac::spi::Spi,
        cx: Option<&mut Context<'_>>,
        finish: bool,
    ) -> Result<pac::spi::regs::Isr, Error> {
        critical_section::with(|cs| {
            let mut inner = self.inner.borrow(cs).borrow_mut();
            if let Some(cx) = cx {
                if !inner
                    .waker
                    .as_ref()
                    .is_some_and(|w| w.will_wake(cx.waker()))
                {
                    inner.waker = Some(cx.waker().clone());
                }
            }
            // Register before reading sticky software/hardware state. For
            // final settlement, mask first and then take the final hardware
            // snapshot; a critical section alone cannot stop hardware flags.
            let enabled = if finish {
                let enabled = regs.ier().read();
                regs.ier().write(|_| {});
                Some(enabled)
            } else {
                None
            };
            let status = regs.isr().read();
            if let Some(error) = inner.error.or_else(|| decode_error(status)) {
                inner.error = Some(error);
                regs.ier().write(|_| {});
                return Err(error);
            }
            if finish && status.txe() && !status.busy() {
                inner.waker = None;
            } else if let Some(enabled) = enabled {
                // Still finishing: restore the previous error enables. A flag
                // raised after the snapshot remains sticky and will wake us.
                regs.ier().write_value(enabled);
            }
            Ok(status)
        })
    }

    fn mask(&self, regs: pac::spi::Spi, deactivate: bool) {
        critical_section::with(|cs| {
            let mut inner = self.inner.borrow(cs).borrow_mut();
            regs.ier().write(|_| {});
            inner.waker = None;
            if deactivate {
                inner.active = false;
            }
        });
    }
}

#[cfg(spi_dma)]
fn dispatch_dma_interrupt(regs: pac::spi::Spi, state: &State) {
    let waker = critical_section::with(|cs| {
        let mut inner = state.inner.borrow(cs).borrow_mut();
        if !inner.active {
            return None;
        }
        let status = regs.isr().read();
        let enabled = regs.ier().read();
        let error = if enabled.modf() && status.modf() {
            Some(Error::ModeFault)
        } else if enabled.ov() && status.ov() {
            Some(Error::Overrun)
        } else if enabled.sserr() && status.sserr() {
            Some(Error::ChipSelectFault)
        } else if enabled.ud() && status.ud() {
            Some(Error::Underrun)
        } else {
            None
        };
        if let Some(error) = error {
            inner.error.get_or_insert(error);
            regs.ier().write(|_| {});
            inner.waker.take()
        } else {
            None
        }
    });
    if let Some(waker) = waker {
        waker.wake();
    }
}

#[cfg(spi_dma)]
#[derive(Clone, Copy, Eq, PartialEq)]
enum DmaPhase {
    Idle,
    InFlight,
    Finishing,
    Quarantined,
}

// Own the original static RX allocation without retaining a Rust reference
// while DMA may write it. Moving this owner never moves the allocation.
#[cfg(spi_dma)]
struct OwnedRxStaging {
    ptr: *mut u8,
    _ownership: PhantomData<&'static mut [u8]>,
}

#[cfg(spi_dma)]
impl OwnedRxStaging {
    fn new(staging: &'static mut [u8]) -> Self {
        Self {
            ptr: staging.as_mut_ptr(),
            _ownership: PhantomData,
        }
    }

    // Returning the address does not grant access to the bytes. Callers must
    // retain the paired DMA lease and observe its existing access protocol.
    fn as_ptr(&self) -> *mut u8 {
        self.ptr
    }
}

// SAFETY: construction consumes exclusive static byte storage. This non-Copy
// owner contains no thread-affine state; moving it preserves the allocation's
// address and ownership, including while the paired DMA lease is outstanding.
#[cfg(spi_dma)]
unsafe impl Send for OwnedRxStaging {}
// SAFETY: shared access only copies the raw address; it cannot read or mutate
// the allocation through a safe API. SpiDma permits CPU RX access only through
// an exclusive borrow after fenced pair retirement. Its ISRs never dereference
// this pointer. No Rust reference to the bytes is retained during DMA writes.
#[cfg(spi_dma)]
unsafe impl Sync for OwnedRxStaging {}

#[cfg(spi_dma)]
struct SpiDma {
    tx: dma::Channel<'static>,
    rx: dma::Channel<'static>,
    tx_staging: &'static mut [u8],
    rx_staging: OwnedRxStaging,
    capacity: usize,
    tx_request: dma::Request,
    rx_request: dma::Request,
    state: &'static State,
    phase: DmaPhase,
}

#[cfg(spi_dma)]
impl SpiDma {
    fn validate_staging(staging: &[u8]) -> Result<(), dma::ConfigError> {
        use dma::ConfigError;
        if staging.is_empty() || staging.len() > u16::MAX as usize {
            return Err(ConfigError::InvalidLength);
        }
        let (base, size) = crate::DMA_COPY_SRAM.ok_or(ConfigError::MissingMemoryMetadata)?;
        let start = staging.as_ptr() as usize;
        let end = start
            .checked_add(staging.len())
            .ok_or(ConfigError::AddressOutOfRange)?;
        let limit = (base as usize)
            .checked_add(size as usize)
            .ok_or(ConfigError::AddressOutOfRange)?;
        if start < base as usize || end > limit {
            return Err(ConfigError::NotInSram);
        }
        Ok(())
    }

    fn quarantine(&mut self, regs: pac::spi::Spi, error: Error) -> Error {
        self.phase = DmaPhase::Quarantined;
        self.state.mask(regs, false);
        error
    }

    fn deactivate(&self, regs: pac::spi::Spi) {
        self.state.mask(regs, true);
    }

    fn poll_complete(
        &mut self,
        regs: pac::spi::Spi,
        mut cx: Option<&mut Context<'_>>,
    ) -> Poll<Result<(), Error>> {
        if self.phase == DmaPhase::Quarantined {
            return Poll::Ready(Err(Error::DmaQuarantined));
        }
        if self.phase == DmaPhase::InFlight {
            // Poll both even if either is pending: both waiters must register,
            // and a fault in the other channel must be observed promptly.
            let tx = self.tx.poll_hardware_complete(cx.as_deref_mut());
            let rx = self.rx.poll_hardware_complete(cx.as_deref_mut());
            if let Err(error) = self.state.observe(regs, cx.as_deref_mut(), false) {
                return Poll::Ready(Err(self.quarantine(regs, error)));
            }
            if let Poll::Ready(Err(error)) = tx {
                return Poll::Ready(Err(self.quarantine(regs, Error::Dma(error))));
            }
            if let Poll::Ready(Err(error)) = rx {
                return Poll::Ready(Err(self.quarantine(regs, Error::Dma(error))));
            }
            if tx.is_pending() || rx.is_pending() {
                return Poll::Pending;
            }
            // Both clean TC+Complete proofs precede request closure. The pair
            // stays reserved through wire completion and any cancellation.
            regs.cr1().modify(|v| {
                v.set_dmarx(false);
                v.set_dmatx(false);
            });
            self.phase = DmaPhase::Finishing;
        }
        let status = match self.state.observe(regs, cx.as_deref_mut(), true) {
            Ok(status) => status,
            Err(error) => return Poll::Ready(Err(self.quarantine(regs, error))),
        };
        if status.busy() || !status.txe() {
            // BUSY has no interrupt. After DMA terminals, cooperatively poll
            // this finite-in-healthy-hardware tail instead of losing the wake.
            // No finite-liveness guarantee applies on faulty hardware.
            if let Some(cx) = cx {
                cx.waker().wake_by_ref();
            }
            return Poll::Pending;
        }
        if self.phase == DmaPhase::Finishing {
            // SAFETY: both clean terminals, closed requests, no SPI error and
            // wire idle were observed. Poison before sequential retirements;
            // a panic cannot expose or reuse either half of the reservation.
            self.phase = DmaPhase::Quarantined;
            unsafe {
                self.tx.retire_hardware();
                self.rx.retire_hardware();
            }
            self.phase = DmaPhase::Idle;
        }
        Poll::Ready(Ok(()))
    }

    fn start(&mut self, regs: pac::spi::Spi, count: usize) -> Result<(), Error> {
        debug_assert!(self.phase == DmaPhase::Idle);
        debug_assert!(count > 0 && count <= self.capacity);
        self.phase = DmaPhase::InFlight;
        critical_section::with(|cs| {
            let mut inner = self.state.inner.borrow(cs).borrow_mut();
            inner.error = None;
            regs.ier().write(|v| {
                v.set_modf(true);
                v.set_ov(true);
                v.set_sserr(true);
                v.set_ud(true);
            });
        });
        // SAFETY: static endpoint/pins/clocks and both admitted channels and
        // private allocations are installed on the driver before either EN.
        // The pointer covers the validated RX capacity; no reference is formed
        // until both clean terminals and wire idle have retired this pair.
        let result = unsafe {
            self.rx
                .start_read(
                    self.rx_request,
                    regs.dr().as_ptr().cast::<u8>(),
                    self.rx_staging.as_ptr(),
                    count,
                )
                .and_then(|()| {
                    self.tx.start_write(
                        self.tx_request,
                        &self.tx_staging[..count],
                        regs.dr().as_ptr().cast::<u8>(),
                    )
                })
        };
        if result.is_err() {
            // Includes a second-channel failure after RX was armed. Even that
            // case retains the entire pair; no partial channel recovery.
            return Err(self.quarantine(regs, Error::DmaQuarantined));
        }
        // Both descriptors are armed with gates closed. RX-ready-first avoids
        // opening a receive window; this ordering is a conservative choice.
        regs.cr1().modify(|v| v.set_dmarx(true));
        regs.cr1().modify(|v| v.set_dmatx(true));
        Ok(())
    }

    fn copy_rx(&mut self, destination: &mut [u8]) {
        debug_assert!(self.phase == DmaPhase::Idle);
        debug_assert!(destination.len() <= self.capacity);
        // SAFETY: successful paired retirement fenced all writes and closed
        // requests. CPU owns these bytes again, with no yield before copying.
        let received =
            unsafe { core::slice::from_raw_parts(self.rx_staging.as_ptr(), destination.len()) };
        destination.copy_from_slice(received);
    }
}

#[cfg(spi_dma)]
impl<M: PeriMode> Spi<'_, M, Master> {
    fn check_dma(&mut self) -> Result<(), Error> {
        if let Some(dma) = self.dma.as_mut() {
            match dma.poll_complete(self.peripheral.info.regs, None) {
                Poll::Ready(result) => result,
                Poll::Pending => Err(Error::DmaBusy),
            }
        } else {
            Ok(())
        }
    }
}

#[cfg(spi_dma)]
impl Spi<'static, Async, Master> {
    /// Create a qualified full-duplex byte bus with safe staged DMA.
    /// Available on F030/A030/F020/L031/L052/L083/R031/W031; where PIO is also
    /// qualified, this constructor selects DMA for this owner permanently.
    ///
    /// Consumes whole static SPI/pin/channel owners and two nonoverlapping
    /// static SRAM slices of 1..=65535 bytes. Inputs remain consumed on error;
    /// a rejected second channel may leave the first admission consumed. Normal
    /// reset/clean-runtime HAL initialization is required. Raw channels cannot
    /// regain safe admission. There is no buffer/channel recovery or split API.
    ///
    /// Caller slices are CPU-copied through the smaller staging capacity. All
    /// successful operations wait for both DMA terminals and wire idle. Errors
    /// permanently retain the pair. Chunk gaps and receive overrun under load
    /// are possible; no throughput or hardware-validation claim is made.
    ///
    /// # Cancellation and chip select
    /// Cancellation/forget leaves the current chunk running, but never accesses
    /// caller memory. Keep the original device selected and successfully await
    /// [`Self::flush`] before deasserting CS, selecting another device or changing
    /// configuration. On error, wire completion is unproven. Generic `SpiDevice`
    /// adapters that release CS on cancellation do not satisfy this contract.
    /// Dropping an unresolved/poisoned bus retains its pins, clocks and pair.
    pub fn new_with_dma<T, SCK, MOSI, MISO, TXD, RXD>(
        peripheral: Peri<'static, T>,
        sck: Peri<'static, SCK>,
        mosi: Peri<'static, MOSI>,
        miso: Peri<'static, MISO>,
        tx_channel: Peri<'static, TXD>,
        rx_channel: Peri<'static, RXD>,
        irq: impl Binding<T::Interrupt, InterruptHandler<T>>
        + Binding<TXD::Interrupt, dma::InterruptHandler<TXD>>
        + Binding<RXD::Interrupt, dma::InterruptHandler<RXD>>
        + 'static,
        tx_staging: &'static mut [u8],
        rx_staging: &'static mut [u8],
        config: Config,
    ) -> Result<Self, ConfigError>
    where
        T: Instance,
        SCK: SckPin<T>,
        MOSI: MosiPin<T>,
        MISO: MisoPin<T>,
        TXD: dma::RequestRoute<T, dma::signal::TX>,
        RXD: dma::RequestRoute<T, dma::signal::RX>,
    {
        SpiDma::validate_staging(tx_staging).map_err(ConfigError::DmaBuffer)?;
        SpiDma::validate_staging(rx_staging).map_err(ConfigError::DmaBuffer)?;
        let tx = dma::Channel::new_admitted(tx_channel, irq)
            .map_err(|error| ConfigError::DmaChannel(error.error))?;
        let rx = dma::Channel::new_admitted(rx_channel, irq)
            .map_err(|error| ConfigError::DmaChannel(error.error))?;
        let mut spi = Self::new_inner(peripheral, sck, mosi, miso, config)?;
        let capacity = tx_staging.len().min(rx_staging.len());
        spi.dma = Some(SpiDma {
            tx,
            rx,
            tx_staging,
            rx_staging: OwnedRxStaging::new(rx_staging),
            capacity,
            tx_request: TXD::REQUEST,
            rx_request: RXD::REQUEST,
            state: T::state(),
            phase: DmaPhase::Idle,
        });
        critical_section::with(|cs| {
            let mut inner = T::state().inner.borrow(cs).borrow_mut();
            inner.error = None;
            inner.waker = None;
            inner.active = true;
        });
        unsafe { T::Interrupt::enable() };
        Ok(spi)
    }
}

#[cfg(spi_dma)]
impl<'d> Spi<'d, Async, Master> {
    async fn transfer_dma(&mut self, read: &mut [u8], write: &[u8]) -> Result<(), Error> {
        self.flush_dma().await?;
        let count = read.len().max(write.len());
        let capacity = self.dma.as_ref().unwrap().capacity;
        for offset in (0..count).step_by(capacity) {
            let len = (count - offset).min(capacity);
            self.prepare_dma()?;
            let dma = self.dma.as_mut().unwrap();
            let write_len = write.len().saturating_sub(offset).min(len);
            dma.tx_staging[..len].fill(0);
            if write_len != 0 {
                dma.tx_staging[..write_len].copy_from_slice(&write[offset..offset + write_len]);
            }
            dma.start(self.peripheral.info.regs, len)?;
            self.flush_dma().await?;
            let read_len = read.len().saturating_sub(offset).min(len);
            if read_len != 0 {
                self.dma
                    .as_mut()
                    .unwrap()
                    .copy_rx(&mut read[offset..offset + read_len]);
            }
        }
        Ok(())
    }

    async fn transfer_in_place_dma(&mut self, words: &mut [u8]) -> Result<(), Error> {
        self.flush_dma().await?;
        let capacity = self.dma.as_ref().unwrap().capacity;
        for chunk in words.chunks_mut(capacity) {
            self.prepare_dma()?;
            let dma = self.dma.as_mut().unwrap();
            dma.tx_staging[..chunk.len()].copy_from_slice(chunk);
            dma.start(self.peripheral.info.regs, chunk.len())?;
            self.flush_dma().await?;
            self.dma.as_mut().unwrap().copy_rx(chunk);
        }
        Ok(())
    }

    async fn flush_dma(&mut self) -> Result<(), Error> {
        core::future::poll_fn(|cx| {
            self.dma
                .as_mut()
                .unwrap()
                .poll_complete(self.peripheral.info.regs, Some(cx))
        })
        .await
    }

    fn prepare_dma(&mut self) -> Result<(), Error> {
        // Previous completion proved Idle. PIO may have selected another word
        // width, so restore eight bits and drain only stale, idle RX data.
        if let Err(error) = self.prepare::<u8>() {
            return Err(self
                .dma
                .as_mut()
                .unwrap()
                .quarantine(self.peripheral.info.regs, error));
        }
        self.peripheral.info.regs.cr1().modify(|v| {
            v.set_dmarx(false);
            v.set_dmatx(false);
        });
        Ok(())
    }
}

/// Supported SPI frame types: `u8`, `u16`, and [`word`] wrappers for 4–15 bits.
///
/// The trait is sealed, so an unsupported frame width cannot reach hardware.
///
#[allow(private_bounds)]
pub trait Word: sealed::Word + Copy + Default + 'static {}

impl sealed::Word for u8 {
    const BITS: u8 = 8;
    fn into_u16(self) -> u16 {
        u16::from(self)
    }
    fn from_u16(value: u16) -> Self {
        value as Self
    }
}
impl Word for u8 {}
impl sealed::Word for u16 {
    const BITS: u8 = 16;
    fn into_u16(self) -> u16 {
        self
    }
    fn from_u16(value: u16) -> Self {
        value
    }
}
impl Word for u16 {}

/// Non-native word widths. Only the low declared number of bits is transferred.
/// These tuple wrappers follow Embassy's `dma::word::U4` etc. convention.
pub mod word {
    macro_rules! word {
        ($name:ident, $storage:ty, $bits:literal) => {
            #[doc = concat!(stringify!($bits), "-bit SPI frame, stored in the low bits.")]
            #[repr(transparent)]
            #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
            #[cfg_attr(feature = "defmt", derive(defmt::Format))]
            pub struct $name(pub $storage);
            impl super::sealed::Word for $name {
                const BITS: u8 = $bits;
                fn into_u16(self) -> u16 {
                    (self.0 as u16) & ((1 << $bits) - 1)
                }
                fn from_u16(value: u16) -> Self {
                    Self((value & ((1 << $bits) - 1)) as $storage)
                }
            }
            impl super::Word for $name {}
        };
    }
    word!(U4, u8, 4);
    word!(U5, u8, 5);
    word!(U6, u8, 6);
    word!(U7, u8, 7);
    word!(U9, u16, 9);
    word!(U10, u16, 10);
    word!(U11, u16, 11);
    word!(U12, u16, 12);
    word!(U13, u16, 13);
    word!(U14, u16, 14);
    word!(U15, u16, 15);
}

#[derive(Clone, Copy)]
pub(crate) struct AnySpi {
    pub(crate) info: &'static Info,
}
impl PeripheralType for AnySpi {}
pub(crate) struct Info {
    pub(crate) regs: pac::spi::Spi,
    pub(crate) maximum_frequency: u32,
    pub(crate) minimum_divisor: u16,
    pub(crate) rcc: crate::rcc::RccInfo,
    #[cfg(spi_async_pio)]
    pub(crate) pio_state: fn() -> &'static PioState,
}

#[cfg(spi_async_pio)]
impl Info {
    fn pio_state(&self) -> &'static PioState {
        (self.pio_state)()
    }
}

pub(crate) mod sealed {
    use super::*;
    pub(crate) trait Instance:
        crate::rcc::RccPeripheral + PeripheralType + Into<AnySpi>
    {
        const MAX_FREQUENCY: u32;
        const MIN_DIVISOR: u16;
        #[cfg(any(spi_dma, spi_async_pio))]
        fn regs() -> pac::spi::Spi;
        #[cfg(spi_dma)]
        fn state() -> &'static State;
        #[cfg(spi_async_pio)]
        fn pio_state() -> &'static PioState;
    }
    pub trait SckPin<T: super::Instance> {
        fn af(&self) -> u8;
    }
    pub trait MosiPin<T: super::Instance> {
        fn af(&self) -> u8;
    }
    pub trait MisoPin<T: super::Instance> {
        fn af(&self) -> u8;
    }
    pub trait Word {
        const BITS: u8;
        fn into_u16(self) -> u16;
        fn from_u16(value: u16) -> Self;
    }
}

/// A verified SPI instance. Implementations are generated only for supported chips.
#[allow(private_bounds)]
pub trait Instance: sealed::Instance {
    /// Metadata-derived SPI GLOBAL vector for the qualified async path.
    #[cfg(any(spi_dma, spi_async_pio))]
    type Interrupt: Interrupt;
    /// Complete required PIO handler; L012 SPI2/SPI3 use the shared SPI23 group.
    #[cfg(spi_async_pio)]
    type InterruptHandler: Handler<Self::Interrupt>;
}
/// Verified SCK alternate-function route for an SPI instance.
#[allow(private_bounds)]
pub trait SckPin<T: Instance>: Pin + sealed::SckPin<T> {}
/// Verified MOSI alternate-function route for an SPI instance.
#[allow(private_bounds)]
pub trait MosiPin<T: Instance>: Pin + sealed::MosiPin<T> {}
/// Verified MISO alternate-function route for an SPI instance.
#[allow(private_bounds)]
pub trait MisoPin<T: Instance>: Pin + sealed::MisoPin<T> {}

// Generated hooks take only SDK/datasheet-verified instance and package routes.
macro_rules! impl_instance {
    ($name:ident, $maximum_frequency:literal, $minimum_divisor:literal, $irq:ident) => {
        impl $crate::spi::sealed::Instance for $crate::peripherals::$name {
            const MAX_FREQUENCY: u32 = $maximum_frequency;
            const MIN_DIVISOR: u16 = $minimum_divisor;
            #[cfg(any(spi_dma, spi_async_pio))]
            fn regs() -> $crate::pac::spi::Spi {
                $crate::pac::$name
            }
            #[cfg(spi_dma)]
            fn state() -> &'static $crate::spi::State {
                static STATE: $crate::spi::State = $crate::spi::State::new();
                &STATE
            }
            #[cfg(spi_async_pio)]
            fn pio_state() -> &'static $crate::spi::PioState {
                static STATE: $crate::spi::PioState = $crate::spi::PioState::new();
                &STATE
            }
        }
        impl $crate::spi::Instance for $crate::peripherals::$name {
            #[cfg(any(spi_dma, spi_async_pio))]
            type Interrupt = $crate::interrupt::typelevel::$irq;
            #[cfg(spi_async_pio)]
            $crate::spi::pio_interrupt_handler!($irq);
        }
        impl From<$crate::peripherals::$name> for $crate::spi::AnySpi {
            fn from(_: $crate::peripherals::$name) -> Self {
                use $crate::spi::sealed::Instance;
                static INFO: $crate::spi::Info = $crate::spi::Info {
                    regs: $crate::pac::$name,
                    maximum_frequency: <$crate::peripherals::$name>::MAX_FREQUENCY,
                    minimum_divisor: <$crate::peripherals::$name>::MIN_DIVISOR,
                    rcc: <$crate::peripherals::$name as $crate::rcc::SealedRccPeripheral>::RCC_INFO,
                    #[cfg(spi_async_pio)]
                    pio_state: <$crate::peripherals::$name>::pio_state,
                };
                Self { info: &INFO }
            }
        }
    };
}
pub(crate) use impl_instance;

// Match the actual metadata vector, not a register version or instance alias.
#[cfg(spi_async_pio)]
macro_rules! pio_interrupt_handler {
    (SPI23) => {
        type InterruptHandler =
            $crate::spi::SharedInterruptHandler<$crate::interrupt::typelevel::SPI23>;
    };
    ($irq:ident) => {
        type InterruptHandler = $crate::spi::InterruptHandler<Self>;
    };
}
#[cfg(spi_async_pio)]
pub(crate) use pio_interrupt_handler;

macro_rules! impl_pin {
    ($signal:ident, $instance:ident, $pin:ident, $af:literal) => {
        impl $crate::spi::sealed::$signal<$crate::peripherals::$instance>
            for $crate::peripherals::$pin
        {
            fn af(&self) -> u8 {
                $af
            }
        }
        impl $crate::spi::$signal<$crate::peripherals::$instance> for $crate::peripherals::$pin {}
    };
}
pub(crate) use impl_pin;
