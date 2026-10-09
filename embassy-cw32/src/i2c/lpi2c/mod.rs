//! Blocking seven-bit master for the CW32L012 command/FIFO I2C controller.
//!
//! This is a separate engine from the classic SI/STAT peripheral. Sources:
//! L012 UM CN V1.4 §§23.4, 23.8; SDK V1.0.5; DS CN V1.0 §7.3.20.
//! See `docs/l012-command-i2c.md` for the evidence and explicit limitations.
//!
//! Adjacent reads are combined into one receive command. A contiguous read run
//! may contain at most 256 bytes; all runs are checked before any bus activity.
//! Repeated START, adjacent writes, clock stretching and arbitration reporting
//! are supported. Poll limits are sample counts, not elapsed-time deadlines.
//! Transactions are never retried. Errors may follow partial wire progress.
//! Disabling MEN drains active work and generates STOP; it is not an abort.
//! Cleanup is bounded and falls back to holding the master logic in reset.
//! A reset cannot force another device to release SDA or SCL.
//!
//! Only frozen PCLK is used. Async, DMA, ten-bit addressing, slave mode and
//! disputed receive FIFO count fields are not exposed. External pull-ups and
//! board timing validation are required.
use core::marker::PhantomData;

use crate::gpio::{Flex, Pin, Pull};
use crate::mode::{Blocking, Mode};
use crate::rcc::ClockBounds;
use crate::time::Hertz;
use crate::{Peri, PeripheralType, pac};
pub use embedded_hal::i2c::Operation;

mod timing;
use timing::{Timing, select_timing};

use pac::i2c::vals::Command;

/// Configuration failures. Timing validation is completed before MMIO.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
pub enum ConfigError {
    /// Call [`crate::init`] first to establish the actual PCLK.
    ClockNotInitialized,
    /// The requested ceiling must be nonzero.
    FrequencyZero,
    /// The verified controller limit is 1 MHz.
    FrequencyTooHigh,
    /// No legal prescaler and waveform fit the requested ceiling/assumptions.
    TimingNotRepresentable,
    /// Rise-time bounds exceed the selected Standard/Fast/Fast+ mode limit.
    InvalidRiseTime,
    /// Filters must be 0..=15 cycles and SDA filtering must be at least SCL's.
    InvalidFilter,
    /// Every bounded wait needs at least one sample.
    ZeroPollLimit,
    /// The peripheral gate did not read back enabled; I2C MMIO was not accessed.
    ClockGateTimeout,
    /// The controller, FIFO, or sampled bus is not idle.
    Busy,
}
impl core::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::ClockNotInitialized => "I2C peripheral clock is not initialized",
            Self::FrequencyZero => "I2C frequency must be nonzero",
            Self::FrequencyTooHigh => "I2C frequency exceeds 1 MHz",
            Self::TimingNotRepresentable => "I2C waveform cannot satisfy the requested timing",
            Self::InvalidRiseTime => "I2C rise-time bound exceeds the selected mode limit",
            Self::InvalidFilter => "invalid I2C digital filter configuration",
            Self::ZeroPollLimit => "I2C polling limit must be nonzero",
            Self::ClockGateTimeout => "I2C peripheral clock gate did not enable",
            Self::Busy => "I2C controller or bus is busy",
        })
    }
}
impl core::error::Error for ConfigError {}

/// Transfer failure. Bytes already sent or received are not rolled back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
pub enum Error {
    /// Unexpected receive state or another controller protocol error.
    Bus,
    /// Arbitration lost. No software STOP command is issued during cleanup.
    Arbitration,
    /// Unexpected NACK. Command enqueue position cannot identify its source.
    Nack,
    /// The controller reported an illegal command/FIFO sequence.
    Fifo,
    /// The hardware detected a persistent low line.
    PinLow,
    /// A command, byte or final STOP exceeded its polling bound.
    Timeout,
    /// Bus lines or the controller remained busy before START.
    BusBusy,
    /// Address is not an unshifted seven-bit address.
    InvalidAddress,
    /// An empty read was rejected before bus activity.
    ZeroLengthTransfer,
    /// A contiguous run of read operations exceeds one 256-byte RX command.
    ReadTooLong,
    /// The held-reset state could not be confirmed; no new START was attempted.
    RecoveryFailed,
}
impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::Bus => "I2C protocol error",
            Self::Arbitration => "I2C arbitration lost",
            Self::Nack => "I2C unexpected NACK (source unknown)",
            Self::Fifo => "I2C command/FIFO error",
            Self::PinLow => "I2C line-low timeout",
            Self::Timeout => "I2C transfer timed out",
            Self::BusBusy => "I2C bus remained busy",
            Self::InvalidAddress => "I2C address exceeds seven bits",
            Self::ZeroLengthTransfer => "I2C empty read is unsupported",
            Self::ReadTooLong => "I2C contiguous read exceeds 256 bytes",
            Self::RecoveryFailed => "I2C master reset could not be confirmed",
        })
    }
}
impl core::error::Error for Error {}
impl embedded_hal::i2c::Error for Error {
    fn kind(&self) -> embedded_hal::i2c::ErrorKind {
        use embedded_hal::i2c::{ErrorKind, NoAcknowledgeSource};
        match self {
            Self::Bus | Self::Fifo => ErrorKind::Bus,
            Self::Arbitration => ErrorKind::ArbitrationLoss,
            Self::Nack => ErrorKind::NoAcknowledge(NoAcknowledgeSource::Unknown),
            _ => ErrorKind::Other,
        }
    }
}

/// Master configuration, including explicit board rise/filter assumptions.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct Config {
    /// Maximum SCL frequency over the qualified RCC oscillator envelope,
    /// including the fastest (zero-rise-time) case.
    pub frequency: Hertz,
    /// Add a weak pull-up to SCL. External pull-ups are normally still needed.
    pub scl_pullup: bool,
    /// Add a weak pull-up to SDA. External pull-ups are normally still needed.
    pub sda_pullup: bool,
    /// Maximum samples per wait, including drain, reset and recovery waits.
    /// This is a poll count, not a time duration; each byte can use this bound.
    pub poll_limit: u32,
    /// Board's worst-case SCL rise time in ns (default assumption: 100 ns).
    /// Must be <=1000/300/120 ns for Standard/Fast/Fast+ respectively.
    pub scl_rise_time_ns: u32,
    /// Board's worst-case SDA rise time in ns (default assumption: 100 ns).
    pub sda_rise_time_ns: u32,
    /// SCL digital filter width in unprescaled PCLK cycles, 0..=15; zero disables.
    pub scl_filter_cycles: u8,
    /// SDA filter width, at least `scl_filter_cycles` and at most 15.
    pub sda_filter_cycles: u8,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            frequency: Hertz(100_000),
            scl_pullup: false,
            sda_pullup: false,
            poll_limit: 100_000,
            scl_rise_time_ns: 100,
            sda_rise_time_ns: 100,
            scl_filter_cycles: 0,
            sda_filter_cycles: 0,
        }
    }
}

/// Owned master. Only [`Blocking`] exposes constructors and transfer methods.
///
/// Drop drains with a bound, or holds master RESET before disconnecting pins.
/// Clock gating occurs only after idle or held reset has been observed.
pub struct I2c<'d, M: Mode> {
    peripheral: Peri<'d, AnyI2c>,
    scl: Flex<'d>,
    sda: Flex<'d>,
    scl_af: u8,
    sda_af: u8,
    engine: Engine,
    kernel_clock: ClockBounds,
    config: Config,
    _mode: PhantomData<M>,
}
impl<'d> I2c<'d, Blocking> {
    /// Construct a blocking master, panicking if clock/configuration is invalid.
    pub fn new_blocking<T: Instance>(
        peripheral: Peri<'d, T>,
        scl: Peri<'d, impl SclPin<T>>,
        sda: Peri<'d, impl SdaPin<T>>,
        config: Config,
    ) -> Self {
        Self::try_new_blocking(peripheral, scl, sda, config)
            .unwrap_or_else(|error| panic!("invalid I2C configuration: {}", error))
    }
    /// Construct after checking all timing assumptions, before touching MMIO.
    /// Tokens are consumed on error; use `reborrow()` when needed afterwards.
    /// No NVIC line is enabled. Do not independently enable this I2C interrupt.
    pub fn try_new_blocking<T: Instance>(
        peripheral: Peri<'d, T>,
        scl: Peri<'d, impl SclPin<T>>,
        sda: Peri<'d, impl SdaPin<T>>,
        config: Config,
    ) -> Result<Self, ConfigError> {
        let kernel_clock =
            crate::rcc::bus_clock_bounds::<T>().ok_or(ConfigError::ClockNotInitialized)?;
        let timing = select_timing(kernel_clock, &config)?;
        let scl_af = sealed::SclPin::<T>::af(&*scl);
        let sda_af = sealed::SdaPin::<T>::af(&*sda);
        let peripheral: Peri<'d, AnyI2c> = peripheral.into();
        let info = peripheral.info;
        critical_section::with(|cs| info.rcc.enable_and_reset_with_cs(cs))
            .map_err(|_| ConfigError::ClockGateTimeout)?;
        let mut scl = Flex::new(scl);
        let mut sda = Flex::new(sda);
        configure(info.regs, timing);
        scl.set_as_af_open_drain(scl_af, pull(config.scl_pullup));
        sda.set_as_af_open_drain(sda_af, pull(config.sda_pullup));
        Ok(Self {
            peripheral,
            scl,
            sda,
            scl_af,
            sda_af,
            engine: Engine {
                regs: info.regs,
                timing,
                poll_limit: config.poll_limit,
                reset_held: false,
            },
            kernel_clock,
            config,
            _mode: PhantomData,
        })
    }
    /// Read 1..=256 bytes from a seven-bit address.
    pub fn blocking_read(&mut self, address: u8, read: &mut [u8]) -> Result<(), Error> {
        self.blocking_transaction(address, &mut [Operation::Read(read)])
    }
    /// Write; an empty buffer performs an address-only probe.
    pub fn blocking_write(&mut self, address: u8, write: &[u8]) -> Result<(), Error> {
        self.blocking_transaction(address, &mut [Operation::Write(write)])
    }
    /// Write then read 1..=256 bytes, using a repeated START and one STOP.
    pub fn blocking_write_read(
        &mut self,
        address: u8,
        write: &[u8],
        read: &mut [u8],
    ) -> Result<(), Error> {
        self.blocking_transaction(
            address,
            &mut [Operation::Write(write), Operation::Read(read)],
        )
    }
    /// Execute an embedded-hal transaction. All inputs are checked before START.
    /// Adjacent same-direction operations share an address phase. Each contiguous
    /// read run is one receive command of at most 256 bytes, with one final NACK.
    /// Empty reads fail, empty writes are retained, and an empty list does nothing.
    /// Success requires observed STOP, an empty command FIFO and idle master.
    pub fn blocking_transaction(
        &mut self,
        address: u8,
        operations: &mut [Operation<'_>],
    ) -> Result<(), Error> {
        self.engine.transaction(address, operations, || {
            self.scl.is_high() && self.sda.is_high()
        })
    }
}
impl<M: Mode> I2c<'_, M> {
    /// Nominal zero-rise SCL rate in whole hertz, rounded down, not measured.
    /// See [`Self::get_current_frequency_bounds`] for the configured envelope.
    pub fn get_current_frequency(&self) -> Hertz {
        self.engine.timing.actual_frequency
    }
    /// Generated SCL frequency interval, rounded outward to whole hertz.
    /// Includes RCC oscillator uncertainty and the configured SCL rise bound.
    /// It requires RCC's qualified supply/temperature range and unchanged clocks.
    /// Clock stretching, FIFO service stalls, or other masters can make the bus
    /// slower than the lower endpoint; this is not a throughput guarantee.
    pub fn get_current_frequency_bounds(&self) -> (Hertz, Hertz) {
        self.engine.timing.frequency_bounds
    }
    /// Current requested configuration and board timing assumptions.
    pub fn get_current_config(&self) -> Config {
        self.config
    }
    /// Reconfigure only while the controller, FIFOs and sampled lines are idle.
    pub fn set_config(&mut self, config: &Config) -> Result<(), ConfigError> {
        let timing = select_timing(self.kernel_clock, config)?;
        let status = self.engine.regs.misr().read();
        if self.engine.reset_held
            || status.mstbusy()
            || status.busbusy()
            || status.rxne()
            || status.packet()
            || status.stop()
            || status.nack()
            || status.arbi()
            || status.fifo()
            || status.pinlow()
            || status.match_()
            || !status.txe()
            || !self.scl.is_high()
            || !self.sda.is_high()
        {
            return Err(ConfigError::Busy);
        }
        configure(self.engine.regs, timing);
        self.scl
            .set_as_af_open_drain(self.scl_af, pull(config.scl_pullup));
        self.sda
            .set_as_af_open_drain(self.sda_af, pull(config.sda_pullup));
        self.engine.timing = timing;
        self.engine.poll_limit = config.poll_limit;
        self.config = *config;
        Ok(())
    }
}
impl<M: Mode> Drop for I2c<'_, M> {
    fn drop(&mut self) {
        if self.engine.quiesce() {
            critical_section::with(|cs| self.peripheral.info.rcc.disable_with_cs(cs))
                .expect("I2C clock gate did not disable");
        }
        // If neither idle nor held reset could be read back, leave the clock on.
        // Flex drops disconnect both pins; no user buffer is retained by hardware.
    }
}
impl embedded_hal::i2c::ErrorType for I2c<'_, Blocking> {
    type Error = Error;
}
impl embedded_hal::i2c::I2c for I2c<'_, Blocking> {
    fn transaction(&mut self, address: u8, operations: &mut [Operation<'_>]) -> Result<(), Error> {
        self.blocking_transaction(address, operations)
    }
}
fn pull(enabled: bool) -> Pull {
    if enabled { Pull::Up } else { Pull::None }
}

fn configure(regs: pac::i2c::I2c, timing: Timing) {
    // Caller guarantees idle or reset. MEN=0 by itself is NOT such a guarantee.
    regs.mcr0().write_value(Default::default()); // CLKSRC=0: frozen PCLK.
    regs.mier().write_value(Default::default());
    regs.mder().write_value(Default::default());
    regs.scr0().write_value(Default::default()); // No slave/shared-clock override.
    regs.sier().write_value(Default::default());
    regs.sder().write_value(Default::default());
    regs.insel().write_value(Default::default()); // GPIO, not comparator inputs.
    regs.mcr1().write_value(Default::default()); // No circular/match-only FIFO.
    regs.mcr2().write(|v| v.set_prescale(timing.prescale)); // Open drain; no auto STOP/ignore ACK.
    regs.mcr3().write(|v| {
        v.set_busidle(timing.busidle);
        v.set_fltscl(timing.scl_filter);
        v.set_fltsda(timing.sda_filter);
    });
    regs.mcr4().write_value(Default::default()); // No SMBus timeout claim.
    regs.mmatch().write_value(Default::default());
    regs.mccr().write(|v| {
        v.set_clklo(timing.clklo);
        v.set_clkhi(timing.clkhi);
        v.set_sethold(timing.sethold);
        v.set_datavd(timing.datavd);
    });
    regs.mfifocr().write_value(Default::default()); // One-entry FIFO watermarks 0.
    regs.mcr0().write(|v| {
        v.set_txfiforst(true);
        v.set_rxfiforst(true);
    });
    // MICR is R1W0. Its reserved bit 2 must retain the documented reset value.
    // Never use zero Default or a read-modify-write for this command register.
    let mut clear = pac::i2c::regs::Micr::write_noop();
    clear.set_packet(false);
    clear.set_stop(false);
    clear.set_nack(false);
    clear.set_arbi(false);
    clear.set_fifo(false);
    clear.set_pinlow(false);
    clear.set_match_(false);
    regs.micr().write_value(clear); // TX reset precedes FIFO error clear.
    regs.mcr0().write(|v| v.set_men(true));
}
fn status_error(status: pac::i2c::regs::Misr) -> Option<Error> {
    // Arbitration dominates simultaneous faults: cleanup must never assume ownership.
    if status.arbi() {
        Some(Error::Arbitration)
    } else if status.pinlow() {
        Some(Error::PinLow)
    } else if status.fifo() {
        Some(Error::Fifo)
    } else if status.nack() {
        Some(Error::Nack)
    } else {
        None
    }
}
struct Engine {
    regs: pac::i2c::I2c,
    timing: Timing,
    poll_limit: u32,
    reset_held: bool,
}
impl Engine {
    fn transaction(
        &mut self,
        address: u8,
        operations: &mut [Operation<'_>],
        mut lines_high: impl FnMut() -> bool,
    ) -> Result<(), Error> {
        preflight(address, operations)?;
        if operations.is_empty() {
            return Ok(());
        }
        if self.reset_held {
            if !self.reset_confirmed() {
                return Err(Error::RecoveryFailed);
            }
            if !self.wait_lines(&mut lines_high) {
                return Err(Error::BusBusy);
            }
            // RESET erased all queued work; restore the full timing/configuration.
            configure(self.regs, self.timing);
            self.reset_held = false;
        }
        let mut idle = false;
        for _ in 0..self.poll_limit {
            let status = self.regs.misr().read();
            if let Some(error) = status_error(status) {
                self.recover();
                return Err(error);
            }
            if status.rxne() {
                self.recover();
                return Err(Error::Bus);
            }
            if !status.mstbusy() && !status.busbusy() && status.txe() && lines_high() {
                let mut clear = pac::i2c::regs::Micr::write_noop();
                clear.set_stop(!status.stop());
                clear.set_packet(!status.packet());
                clear.set_match_(!status.match_());
                self.regs.micr().write_value(clear);
                idle = true;
                break;
            }
            core::hint::spin_loop();
        }
        if !idle {
            return Err(Error::BusBusy);
        }
        let result = self.run(address, operations);
        if result.is_err() {
            self.recover();
        }
        result
    }
    fn run(&mut self, address: u8, operations: &mut [Operation<'_>]) -> Result<(), Error> {
        let mut index = 0;
        while index < operations.len() {
            let read = matches!(operations[index], Operation::Read(_));
            let mut end = index + 1;
            while end < operations.len() && matches!(operations[end], Operation::Read(_)) == read {
                end += 1;
            }
            self.enqueue(Command::StartExpectAck, (address << 1) | u8::from(read))?;
            if read {
                // Preflight established 1..=256, even across adjacent buffers.
                let count: usize = operations[index..end]
                    .iter()
                    .map(|op| match op {
                        Operation::Read(bytes) => bytes.len(),
                        Operation::Write(_) => 0,
                    })
                    .sum();
                self.enqueue(Command::Receive, (count - 1) as u8)?;
                for op in &mut operations[index..end] {
                    if let Operation::Read(bytes) = op {
                        for byte in bytes.iter_mut() {
                            *byte = self.read_byte()?;
                        }
                    }
                }
            } else {
                for op in &operations[index..end] {
                    if let Operation::Write(bytes) = op {
                        for &byte in *bytes {
                            self.enqueue(Command::Transmit, byte)?;
                        }
                    }
                }
            }
            index = end;
        }
        self.enqueue(Command::Stop, 0)?;
        for _ in 0..self.poll_limit {
            let status = self.regs.misr().read();
            if let Some(error) = status_error(status) {
                return Err(error);
            }
            // TXE alone says only that the command FIFO is empty. STOP+idle are mandatory.
            // BUSBUSY may already belong to another master after our successful STOP.
            if status.stop() && status.txe() && !status.mstbusy() {
                let mut clear = pac::i2c::regs::Micr::write_noop();
                clear.set_stop(!status.stop());
                clear.set_packet(!status.packet());
                self.regs.micr().write_value(clear);
                return Ok(());
            }
            core::hint::spin_loop();
        }
        Err(Error::Timeout)
    }
    fn enqueue(&mut self, command: Command, data: u8) -> Result<(), Error> {
        for _ in 0..self.poll_limit {
            let status = self.regs.misr().read();
            if let Some(error) = status_error(status) {
                return Err(error);
            }
            if status.txe() {
                self.regs.mtdr().write(|v| {
                    v.set_cmd(command);
                    v.set_data(data);
                });
                return Ok(());
            }
            core::hint::spin_loop();
        }
        Err(Error::Timeout)
    }
    fn read_byte(&mut self) -> Result<u8, Error> {
        for _ in 0..self.poll_limit {
            let status = self.regs.misr().read();
            if let Some(error) = status_error(status) {
                return Err(error);
            }
            if status.rxne() {
                let value = self.regs.mrdr().read();
                if !value.empty() {
                    return Ok(value.data());
                }
            }
            core::hint::spin_loop();
        }
        Err(Error::Timeout)
    }
    fn wait_lines(&self, lines_high: &mut impl FnMut() -> bool) -> bool {
        for _ in 0..self.poll_limit {
            if lines_high() {
                return true;
            }
            core::hint::spin_loop();
        }
        false
    }
    fn reset_confirmed(&self) -> bool {
        for _ in 0..self.poll_limit {
            let control = self.regs.mcr0().read();
            if control.reset() && !control.men() {
                return true;
            }
            core::hint::spin_loop();
        }
        false
    }
    fn recover(&mut self) {
        if self.quiesce() && !self.reset_held {
            // Re-enable only the empty, idle controller. Any persistent PINLOW
            // will be observed at the next transaction, before a new START.
            self.regs.mcr0().write(|v| v.set_men(true));
        }
    }
    fn quiesce(&mut self) -> bool {
        self.regs.mier().write_value(Default::default());
        self.regs.mder().write_value(Default::default());
        if self.reset_held {
            return self.reset_confirmed();
        }
        // First discard queued commands. An active command may still complete.
        // The single R0W1 write also clears MEN: §23.4.4 drains active work,
        // stops waiting for FIFO servicing, and automatically emits STOP if owned.
        // Do not enqueue STOP after ARBI: hardware has already released ownership.
        self.regs.mcr0().write(|v| {
            v.set_txfiforst(true);
            v.set_rxfiforst(true);
        });
        for _ in 0..self.poll_limit {
            let status = self.regs.misr().read();
            if !status.mstbusy() && status.txe() {
                // Hardware can have filled RX while draining an active receive.
                self.regs.mcr0().write(|v| {
                    v.set_txfiforst(true);
                    v.set_rxfiforst(true);
                });
                let mut clear = pac::i2c::regs::Micr::write_noop();
                clear.set_packet(!status.packet());
                clear.set_stop(!status.stop());
                clear.set_nack(!status.nack());
                clear.set_arbi(!status.arbi());
                clear.set_fifo(!status.fifo());
                clear.set_pinlow(!status.pinlow());
                clear.set_match_(!status.match_());
                self.regs.micr().write_value(clear); // Reset TX before clearing FIFO.
                return true;
            }
            core::hint::spin_loop();
        }
        // RESET resets all master logic/registers except MCR0 (§23.4.1.1).
        // Keep it asserted; no claim that external lines were recovered.
        self.regs.mcr0().write(|v| v.set_reset(true));
        self.reset_held = true;
        self.reset_confirmed()
    }
}
fn preflight(address: u8, operations: &[Operation<'_>]) -> Result<(), Error> {
    if address > 0x7f {
        return Err(Error::InvalidAddress);
    }
    let mut run = 0usize;
    for op in operations {
        match op {
            Operation::Read(bytes) => {
                if bytes.is_empty() {
                    return Err(Error::ZeroLengthTransfer);
                }
                run = run.checked_add(bytes.len()).ok_or(Error::ReadTooLong)?;
                if run > 256 {
                    return Err(Error::ReadTooLong);
                }
            }
            Operation::Write(_) => run = 0,
        }
    }
    Ok(())
}

/// Type-erased owned I2C peripheral, constructible only from verified instances.
pub struct AnyI2c {
    pub(crate) info: &'static Info,
}
embassy_hal_internal::impl_peripheral!(AnyI2c);
pub(crate) struct Info {
    pub(crate) regs: pac::i2c::I2c,
    pub(crate) rcc: crate::rcc::RccInfo,
}
pub(crate) mod sealed {
    use super::*;
    pub(crate) trait Instance:
        crate::rcc::RccPeripheral + PeripheralType + Into<AnyI2c>
    {
    }
    pub trait SclPin<T: super::Instance> {
        fn af(&self) -> u8;
    }
    pub trait SdaPin<T: super::Instance> {
        fn af(&self) -> u8;
    }
}
/// Verified CW32L012 command/FIFO I2C instance.
#[allow(private_bounds)]
pub trait Instance: sealed::Instance {}
/// Verified, bonded SCL alternate-function route for this instance.
#[allow(private_bounds)]
pub trait SclPin<T: Instance>: Pin + sealed::SclPin<T> {}
/// Verified, bonded SDA alternate-function route for this instance.
#[allow(private_bounds)]
pub trait SdaPin<T: Instance>: Pin + sealed::SdaPin<T> {}
macro_rules! impl_instance {
    ($name:ident, $number:literal) => {
        impl $crate::i2c::sealed::Instance for $crate::peripherals::$name {}
        impl $crate::i2c::Instance for $crate::peripherals::$name {}
        impl From<$crate::peripherals::$name> for $crate::i2c::AnyI2c {
            fn from(_: $crate::peripherals::$name) -> Self {
                static INFO: $crate::i2c::Info = $crate::i2c::Info {
                    regs: $crate::pac::$name,
                    rcc: <$crate::peripherals::$name as $crate::rcc::SealedRccPeripheral>::RCC_INFO,
                };
                Self { info: &INFO }
            }
        }
    };
}
pub(crate) use impl_instance;
macro_rules! impl_pin {
    ($signal:ident, $instance:ident, $pin:ident, $af:literal) => {
        impl $crate::i2c::sealed::$signal<$crate::peripherals::$instance>
            for $crate::peripherals::$pin
        {
            fn af(&self) -> u8 {
                $af
            }
        }
        impl $crate::i2c::$signal<$crate::peripherals::$instance> for $crate::peripherals::$pin {}
    };
}
pub(crate) use impl_pin;
