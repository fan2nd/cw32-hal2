//! Blocking and interrupt-driven seven-bit CW32L012 command/FIFO I2C masters.
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
//! Async TXE/RXNE/STOP-event waits have no internal deadline. An independent
//! timeout can cancel a pending future, but cannot preempt synchronous sampling
//! or Drop. Cancellation can finish queued bytes and need not deliver received
//! bytes to the caller. A forgotten transfer is retired before the next mutable
//! operation, even when its new arguments are invalid or empty. Getters only
//! return cached settings. Unconfirmed retirement permanently faults the
//! instance until system reset, across blocking and async reconstruction.
//!
//! Only frozen PCLK is used. DMA, ten-bit addressing, slave mode and
//! disputed receive FIFO count fields are not exposed. External pull-ups and
//! board timing validation are required.
use core::future::poll_fn;
use core::marker::PhantomData;
use core::sync::atomic::{AtomicBool, Ordering};
use core::task::Poll;

use embassy_sync::waitqueue::AtomicWaker;

use crate::gpio::{Flex, Pin, Pull};
use crate::interrupt::typelevel::{Binding, Handler, Interrupt};
use crate::mode::{Async, Blocking, Mode};
use crate::rcc::{ClockBounds, Readback};
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
    /// Local retirement/reset/configuration could not be confirmed. Terminal
    /// until system reset; the clock gate is retained.
    RecoveryFailed,
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
            Self::RecoveryFailed => "I2C local recovery could not be confirmed",
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
    /// Local retirement/reset/configuration could not be confirmed. The
    /// instance is terminally faulted until system reset.
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
            Self::RecoveryFailed => "I2C local recovery could not be confirmed",
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
    /// Maximum samples per synchronous wait, including idle, drain, reset,
    /// configuration readback and Drop. Blocking byte/STOP waits also use it.
    /// Async FIFO/initial STOP-event waits suspend without an internal limit.
    /// This is not a time duration. The existing RCC helper used only to release
    /// an already asserted central reset has a separate fixed 32-sample bound.
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

/// Owned blocking or interrupt-driven master.
///
/// Drop drains with a bound, or holds master RESET before disconnecting pins.
/// Clock gating occurs only after idle or held reset has been observed.
pub struct I2c<'d, M: Mode> {
    _peripheral: Peri<'d, AnyI2c>,
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
        Self::try_new_inner(peripheral, scl, sda, config)
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
impl<'d> I2c<'d, Async> {
    /// Construct an interrupt-driven master, panicking on invalid configuration.
    pub fn new_async<T: Instance>(
        peripheral: Peri<'d, T>,
        scl: Peri<'d, impl SclPin<T>>,
        sda: Peri<'d, impl SdaPin<T>>,
        irqs: impl Binding<T::Interrupt, InterruptHandler<T>> + 'd,
        config: Config,
    ) -> Self {
        Self::try_new_async(peripheral, scl, sda, irqs, config)
            .unwrap_or_else(|error| panic!("invalid I2C configuration: {}", error))
    }

    /// Construct with this controller's dedicated GLOBAL interrupt binding.
    /// Tokens are consumed on error; use `reborrow()` if needed afterward.
    /// Inherited enabled/busy slave state and reserved clock sources are rejected
    /// before pin or master-control changes, with the gate on and vector masked.
    pub fn try_new_async<T: Instance>(
        peripheral: Peri<'d, T>,
        scl: Peri<'d, impl SclPin<T>>,
        sda: Peri<'d, impl SdaPin<T>>,
        _irqs: impl Binding<T::Interrupt, InterruptHandler<T>> + 'd,
        config: Config,
    ) -> Result<Self, ConfigError> {
        Self::try_new_inner(peripheral, scl, sda, config)
    }

    /// Read 1..=256 bytes from an unshifted seven-bit address.
    pub async fn read(&mut self, address: u8, read: &mut [u8]) -> Result<(), Error> {
        self.transaction(address, &mut [Operation::Read(read)])
            .await
    }

    /// Write; an empty buffer performs an address-only probe.
    pub async fn write(&mut self, address: u8, write: &[u8]) -> Result<(), Error> {
        self.transaction(address, &mut [Operation::Write(write)])
            .await
    }

    /// Write then read with a repeated START and one final STOP.
    pub async fn write_read(
        &mut self,
        address: u8,
        write: &[u8],
        read: &mut [u8],
    ) -> Result<(), Error> {
        self.transaction(
            address,
            &mut [Operation::Write(write), Operation::Read(read)],
        )
        .await
    }

    /// Execute a seven-bit transaction, combining adjacent same-direction runs.
    /// Each read run is one 1..=256-byte receive command with one final NACK.
    /// All read runs are validated before any new bus activity; empty writes
    /// retain their address phase and empty reads are rejected.
    ///
    /// Dropping a polled future performs bounded retirement and leaves the
    /// master disabled or held in RESET. Queued work can still reach the target.
    /// Forgetting it leaves a durable Active marker; the next mutable operation
    /// retires that old work even if the new arguments are invalid or empty.
    /// TXE/RXNE/initial STOP waits have no internal deadline. Once STOP is seen,
    /// idle confirmation is synchronous and bounded by `Config::poll_limit`.
    pub async fn transaction(
        &mut self,
        address: u8,
        operations: &mut [Operation<'_>],
    ) -> Result<(), Error> {
        self.engine.prepare(address, operations, || {
            self.scl.is_high() && self.sda.is_high()
        })?;
        if operations.is_empty() {
            return Ok(());
        }
        // Both the owned marker and guard precede the first possible await or
        // command. No static state points into this future or its operations.
        self.engine.lifecycle = Lifecycle::Active { stop_queued: false };
        let mut guard = AsyncGuard {
            engine: &mut self.engine,
            armed: true,
        };
        let result = guard.run(address, operations).await;
        guard.finish(result)
    }
}

impl embedded_hal_async::i2c::I2c for I2c<'_, Async> {
    async fn transaction(
        &mut self,
        address: u8,
        operations: &mut [Operation<'_>],
    ) -> Result<(), Error> {
        I2c::transaction(self, address, operations).await
    }
}

impl<'d, M: Mode> I2c<'d, M> {
    fn try_new_inner<T: Instance>(
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
        // Fence, qualify inherited state and restore the disabled master before
        // Flex::new can change a pin. Busy admission leaves the gate on.
        let mut engine = Engine::construct(peripheral.info, timing, config.poll_limit)?;
        let mut scl = Flex::new(scl);
        let mut sda = Flex::new(sda);
        scl.set_as_af_open_drain(scl_af, pull(config.scl_pullup));
        sda.set_as_af_open_drain(sda_af, pull(config.sda_pullup));
        engine
            .enable_configured(timing, config.poll_limit)
            .map_err(|_| ConfigError::RecoveryFailed)?;
        Ok(Self {
            _peripheral: peripheral,
            scl,
            sda,
            scl_af,
            sda_af,
            engine,
            kernel_clock,
            config,
            _mode: PhantomData,
        })
    }
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
    /// A forgotten active transfer is retired before validating the candidate.
    /// Busy/invalid candidates preserve cached settings and any stopped state.
    pub fn set_config(&mut self, config: &Config) -> Result<(), ConfigError> {
        self.engine.retire_abandoned().map_err(config_error)?;
        let timing = select_timing(self.kernel_clock, config)?;
        if self.engine.lifecycle == Lifecycle::Ready {
            let status = self.engine.status().map_err(config_error)?;
            if status.mstbusy()
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
            if self.engine.retire(true).map_err(config_error)? {
                return Err(ConfigError::Busy);
            }
        }
        if !self
            .engine
            .wait_lines(&mut || self.scl.is_high() && self.sda.is_high())
        {
            return Err(ConfigError::Busy);
        }
        self.engine
            .restore_disabled(timing, config.poll_limit)
            .map_err(config_error)?;
        self.engine
            .enable_configured(timing, config.poll_limit)
            .map_err(config_error)?;
        // Publish pull choices and the cache only after every I2C readback.
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
        self.engine.close();
        // Flex drops disconnect pins even after terminal fault. Local retirement
        // cannot promise external bus recovery; no caller buffer is retained.
    }
}
impl<M: Mode> embedded_hal::i2c::ErrorType for I2c<'_, M> {
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

fn config_error(error: Error) -> ConfigError {
    if error == Error::RecoveryFailed {
        ConfigError::RecoveryFailed
    } else {
        ConfigError::Busy
    }
}

fn configure_disabled(regs: pac::i2c::I2c, timing: Timing) {
    // A confirmed master RESET was released with MEN=0. MCR0 is restored
    // explicitly by the caller because master RESET excludes that register.
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
#[derive(Clone, Copy, PartialEq, Eq)]
enum Lifecycle {
    Ready,
    // Sequencing only: this never proves ownership of the physical bus.
    Active { stop_queued: bool },
    Disabled,
    ResetHeld,
    Faulted,
}

struct Engine {
    info: &'static Info,
    timing: Timing,
    poll_limit: u32,
    lifecycle: Lifecycle,
}
impl Engine {
    fn construct(
        info: &'static Info,
        timing: Timing,
        poll_limit: u32,
    ) -> Result<Self, ConfigError> {
        info.constructor_fence()?;
        let mut engine = Self {
            info,
            timing,
            poll_limit,
            // No idle assumption is made about a forgotten or foreign owner.
            lifecycle: Lifecycle::Active { stop_queued: false },
        };
        engine.check_fault().map_err(config_error)?;
        if critical_section::with(|cs| info.rcc.enable_with_cs_readback(cs, Readback::None))
            .is_err()
            || !engine.wait_gate(true)
        {
            engine.fault();
            return Err(ConfigError::ClockGateTimeout);
        }
        if info.rcc.reset_asserted() {
            // I2C2 can power up held by its own central reset. The helper's
            // existing fixed 32-sample gate loop is the only in-section polling
            // exception. Never pulse a reset observed released in this section.
            let released = critical_section::with(|cs| {
                if !info.rcc.is_enabled() {
                    return false;
                }
                !info.rcc.reset_asserted() || info.rcc.enable_and_reset_with_cs(cs).is_ok()
            });
            if !released {
                return Err(config_error(engine.fault()));
            }
            let mut eligible = false;
            for _ in 0..poll_limit {
                if info.rcc.is_enabled() && !info.rcc.reset_asserted() {
                    eligible = true;
                    break;
                }
                core::hint::spin_loop();
            }
            if !eligible {
                return Err(config_error(engine.fault()));
            }
        }
        // Master RESET cannot retire the independent slave. Qualify it before
        // any master control/reset or pin write, while SYSCTRL permits MMIO.
        let slave_disabled = engine
            .access(|regs| {
                !regs.scr0().read().sen()
                    && regs.sier().read().0 == 0
                    && regs.sder().read().0 == 0
                    && !regs.sisr().read().slvbusy()
            })
            .map_err(config_error)?;
        if !slave_disabled {
            return Err(ConfigError::Busy);
        }
        let (control, zero_watermarks) = engine
            .access(|regs| (regs.mcr0().read(), regs.mfifocr().read().0 == 0))
            .map_err(config_error)?;
        if control.clksrc() == 1 {
            // Reserved encoding: leave the gate and master state untouched.
            return Err(ConfigError::Busy);
        }
        // An entire zero MFIFOCR word is required; no disputed width is used.
        engine.retire(zero_watermarks).map_err(config_error)?;
        engine
            .restore_disabled(timing, poll_limit)
            .map_err(config_error)?;
        Ok(engine)
    }

    fn fault(&mut self) -> Error {
        self.lifecycle = Lifecycle::Faulted;
        self.info.state.faulted.store(true, Ordering::Release);
        self.info.fence();
        Error::RecoveryFailed
    }

    fn check_fault(&mut self) -> Result<(), Error> {
        if self.lifecycle == Lifecycle::Faulted || self.info.state.faulted.load(Ordering::Acquire) {
            return Err(self.fault());
        }
        Ok(())
    }

    fn check_access(&mut self) -> Result<(), Error> {
        self.check_fault()?;
        if !self.info.rcc.is_enabled() || self.info.rcc.reset_asserted() {
            return Err(self.fault());
        }
        Ok(())
    }

    // Bracket every peripheral observation/write with actual gate and central
    // reset readbacks. Inaccessible zeroes must never establish local idle.
    fn access<R>(&mut self, f: impl FnOnce(pac::i2c::I2c) -> R) -> Result<R, Error> {
        self.check_access()?;
        let result = f(self.info.regs);
        self.check_access()?;
        Ok(result)
    }

    fn status(&mut self) -> Result<pac::i2c::regs::Misr, Error> {
        self.access(|regs| regs.misr().read())
    }

    fn wait_gate(&self, enabled: bool) -> bool {
        for _ in 0..self.poll_limit {
            if self.info.rcc.is_enabled() == enabled {
                return true;
            }
            core::hint::spin_loop();
        }
        false
    }

    fn wait_control(&mut self, expected: pac::i2c::regs::Mcr0, limit: u32) -> Result<(), Error> {
        for _ in 0..limit {
            let control = self.access(|regs| regs.mcr0().read())?;
            if ordinary_control_matches(control, expected) {
                return Ok(());
            }
            core::hint::spin_loop();
        }
        Err(self.fault())
    }

    fn hold_reset(&mut self, control: pac::i2c::regs::Mcr0, limit: u32) -> Result<(), Error> {
        let mut held = ordinary_control(control);
        held.set_men(false);
        held.set_reset(true);
        self.access(|regs| regs.mcr0().write_value(held))?;
        self.wait_control(held, limit)?;
        self.lifecycle = Lifecycle::ResetHeld;
        Ok(())
    }

    fn restore_disabled(&mut self, timing: Timing, limit: u32) -> Result<(), Error> {
        self.check_fault()?;
        self.info.fence();
        let inherited = self.access(|regs| regs.mcr0().read())?;
        // Preserve CLKSRC/DBGEN/SLEEPEN until held RESET is freshly confirmed.
        self.hold_reset(inherited, limit)?;
        let mut held = pac::i2c::regs::Mcr0::default();
        held.set_reset(true);
        self.access(|regs| regs.mcr0().write_value(held))?;
        self.wait_control(held, limit)?; // PCLK selected only while held.
        let released = pac::i2c::regs::Mcr0::default();
        self.access(|regs| regs.mcr0().write_value(released))?;
        self.wait_control(released, limit)?;
        self.access(|regs| configure_disabled(regs, timing))?;
        self.verify_configuration(timing, false, limit)?;
        // Reset has discarded master commands/FIFOs. Clear only observed flags;
        // in particular this is never an early FIFO error clear during drain.
        let status = self.status()?;
        self.access(|regs| clear_observed(regs, status))?;
        self.lifecycle = Lifecycle::Disabled;
        Ok(())
    }

    fn verify_configuration(
        &mut self,
        timing: Timing,
        enabled: bool,
        limit: u32,
    ) -> Result<(), Error> {
        for _ in 0..limit {
            let valid = self.access(|regs| configuration_matches(regs, timing, enabled))?;
            if valid {
                return Ok(());
            }
            core::hint::spin_loop();
        }
        Err(self.fault())
    }

    fn enable_configured(&mut self, timing: Timing, limit: u32) -> Result<(), Error> {
        self.access(|regs| regs.mcr0().write(|v| v.set_men(true)))?;
        self.verify_configuration(timing, true, limit)?;
        self.lifecycle = Lifecycle::Ready;
        Ok(())
    }

    // Returns whether arbitration was observed during retirement, before any
    // diagnostic clear or reset. Recovery failure always has higher priority.
    fn retire(&mut self, zero_watermarks: bool) -> Result<bool, Error> {
        self.check_fault()?;
        self.info.fence();
        let (control, status, watermarks_zero) = self.access(|regs| {
            regs.mier().write_value(Default::default());
            regs.mder().write_value(Default::default());
            (
                regs.mcr0().read(),
                regs.misr().read(),
                regs.mfifocr().read().0 == 0,
            )
        })?;
        let mut arbitration = status.arbi();
        if control.clksrc() == 1 {
            return Err(self.fault());
        }
        let mut disabled = ordinary_control(control);
        disabled.set_men(false);
        // MEN alone is cleared, with source/debug/sleep and an inherited RESET
        // retained. There are no FIFO pulses or explicit cleanup STOP commands.
        self.access(|regs| regs.mcr0().write_value(disabled))?;
        if disabled.reset() || !zero_watermarks || !watermarks_zero {
            self.hold_reset(disabled, self.poll_limit)?;
            return Ok(arbitration);
        }
        let mut idle = false;
        for _ in 0..self.poll_limit {
            let (control, status) = self.access(|regs| (regs.mcr0().read(), regs.misr().read()))?;
            arbitration |= status.arbi();
            if ordinary_control_matches(control, disabled) && !status.mstbusy() && status.txe() {
                idle = true;
                break;
            }
            core::hint::spin_loop();
        }
        if idle {
            // Only confirmed disabled local idle permits a separate FIFO reset.
            // Receive data may have accumulated while MEN-disable drained work.
            let mut flush = disabled;
            flush.set_txfiforst(true);
            flush.set_rxfiforst(true);
            self.access(|regs| regs.mcr0().write_value(flush))?;
            for _ in 0..self.poll_limit {
                let (control, status, zero) = self.access(|regs| {
                    (
                        regs.mcr0().read(),
                        regs.misr().read(),
                        regs.mfifocr().read().0 == 0,
                    )
                })?;
                arbitration |= status.arbi();
                // R0W1 pulse readback is not used as an acknowledgement.
                if ordinary_control_matches(control, disabled)
                    && zero
                    && !status.mstbusy()
                    && status.txe()
                    && !status.rxne()
                {
                    self.access(|regs| clear_observed(regs, status))?;
                    self.lifecycle = Lifecycle::Disabled;
                    return Ok(arbitration);
                }
                core::hint::spin_loop();
            }
        }
        // ARBI/NACK can block pending STARTs; never clear them to restart work.
        // Holding RESET isolates the local master, not the target or its wires.
        self.hold_reset(disabled, self.poll_limit)?;
        Ok(arbitration)
    }

    fn retire_abandoned(&mut self) -> Result<(), Error> {
        self.check_fault()?;
        if matches!(self.lifecycle, Lifecycle::Active { .. }) && self.retire(true)? {
            return Err(Error::Arbitration);
        }
        Ok(())
    }

    fn finish_error(&mut self, original: Error) -> Error {
        match self.retire(true) {
            Err(_) => Error::RecoveryFailed,
            Ok(true) => Error::Arbitration,
            Ok(false) => original,
        }
    }

    fn close(&mut self) {
        if self.retire(true).is_err() {
            return;
        }
        // Reverify the stopped state immediately before authorizing gate-off.
        let held = self.lifecycle == Lifecycle::ResetHeld;
        let stopped = self.access(|regs| {
            let control = regs.mcr0().read();
            let slave_disabled = !regs.scr0().read().sen()
                && regs.sier().read().0 == 0
                && regs.sder().read().0 == 0
                && !regs.sisr().read().slvbusy();
            if held {
                slave_disabled && !control.men() && control.reset()
            } else {
                let status = regs.misr().read();
                slave_disabled
                    && !control.men()
                    && !control.reset()
                    && !status.mstbusy()
                    && status.txe()
                    && !status.rxne()
                    && regs.mfifocr().read().0 == 0
            }
        });
        if !matches!(stopped, Ok(true)) {
            self.fault();
            return;
        }
        let info = self.info;
        let disabled = critical_section::with(|cs| {
            info.rcc.is_enabled()
                && !info.rcc.reset_asserted()
                && info
                    .rcc
                    .disable_with_cs_readback(cs, Readback::None)
                    .is_ok()
        });
        if !disabled || !self.wait_gate(false) {
            // Do not panic or retry an unconfirmed gate change.
            self.fault();
        }
    }

    fn prepare(
        &mut self,
        address: u8,
        operations: &[Operation<'_>],
        mut lines_high: impl FnMut() -> bool,
    ) -> Result<(), Error> {
        self.retire_abandoned()?;
        preflight(address, operations)?;
        if operations.is_empty() {
            return Ok(());
        }
        if matches!(self.lifecycle, Lifecycle::Disabled | Lifecycle::ResetHeld) {
            if !self.wait_lines(&mut lines_high) {
                return Err(Error::BusBusy);
            }
            self.restore_disabled(self.timing, self.poll_limit)?;
            self.enable_configured(self.timing, self.poll_limit)?;
        }
        for _ in 0..self.poll_limit {
            let status = self.status()?;
            if let Some(error) = status_error(status) {
                return Err(self.finish_error(error));
            }
            if status.rxne() {
                return Err(self.finish_error(Error::Bus));
            }
            if !status.mstbusy() && !status.busbusy() && status.txe() && lines_high() {
                self.access(|regs| {
                    let mut clear = pac::i2c::regs::Micr::write_noop();
                    clear.set_stop(!status.stop());
                    clear.set_packet(!status.packet());
                    clear.set_match_(!status.match_());
                    regs.micr().write_value(clear);
                })?;
                return Ok(());
            }
            core::hint::spin_loop();
        }
        Err(Error::BusBusy)
    }

    fn transaction(
        &mut self,
        address: u8,
        operations: &mut [Operation<'_>],
        lines_high: impl FnMut() -> bool,
    ) -> Result<(), Error> {
        self.prepare(address, operations, lines_high)?;
        if operations.is_empty() {
            return Ok(());
        }
        self.lifecycle = Lifecycle::Active { stop_queued: false };
        match self.run(address, operations) {
            Ok(()) => Ok(()),
            Err(error) => Err(self.finish_error(error)),
        }
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
        self.confirm_completion()
    }

    fn confirm_completion(&mut self) -> Result<(), Error> {
        for _ in 0..self.poll_limit {
            let status = self.status()?;
            if let Some(error) = status_error(status) {
                return Err(error);
            }
            // TXE is FIFO capacity, not an ACK. BUSBUSY can already belong to
            // another master after our STOP. PACKET also covers repeated START.
            if self.lifecycle == (Lifecycle::Active { stop_queued: true })
                && status.stop()
                && status.txe()
                && !status.mstbusy()
            {
                self.info.fence();
                self.access(|regs| {
                    regs.mier().write_value(Default::default());
                    regs.mder().write_value(Default::default());
                    let mut clear = pac::i2c::regs::Micr::write_noop();
                    clear.set_stop(!status.stop());
                    clear.set_packet(!status.packet());
                    regs.micr().write_value(clear);
                })?;
                self.lifecycle = Lifecycle::Ready;
                return Ok(());
            }
            core::hint::spin_loop();
        }
        Err(Error::Timeout)
    }

    fn enqueue(&mut self, command: Command, data: u8) -> Result<(), Error> {
        for _ in 0..self.poll_limit {
            let status = self.status()?;
            if let Some(error) = status_error(status) {
                return Err(error);
            }
            if status.txe() {
                if command == Command::Stop {
                    self.lifecycle = Lifecycle::Active { stop_queued: true };
                }
                self.access(|regs| {
                    regs.mtdr().write(|v| {
                        v.set_cmd(command);
                        v.set_data(data);
                    })
                })?;
                return Ok(());
            }
            core::hint::spin_loop();
        }
        Err(Error::Timeout)
    }

    fn read_byte(&mut self) -> Result<u8, Error> {
        for _ in 0..self.poll_limit {
            let status = self.status()?;
            if let Some(error) = status_error(status) {
                return Err(error);
            }
            if status.rxne() {
                let value = self.access(|regs| regs.mrdr().read())?;
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
}

#[derive(Clone, Copy)]
enum WaitFor {
    Tx,
    Rx,
    Stop,
}
impl WaitFor {
    fn ready(self, status: pac::i2c::regs::Misr) -> bool {
        match self {
            Self::Tx => status.txe(),
            Self::Rx => status.rxne(),
            Self::Stop => status.stop(),
        }
    }

    fn mask(self) -> pac::i2c::regs::Mier {
        let mut mask = pac::i2c::regs::Mier::default();
        mask.set_arbi(true);
        mask.set_pinlow(true);
        mask.set_fifo(true);
        mask.set_nack(true);
        match self {
            Self::Tx => mask.set_txe(true),
            Self::Rx => mask.set_rxne(true),
            Self::Stop => mask.set_stop(true),
        }
        // PACKET/MATCH never mean completion; TXE stays off for RX/STOP waits.
        mask
    }
}

impl Engine {
    async fn wait(&mut self, phase: WaitFor) -> Result<pac::i2c::regs::Misr, Error> {
        let info = self.info;
        poll_fn(|cx| {
            info.state.waker.register(cx.waker());
            let result = critical_section::with(|_| {
                (info.disable_irq)();
                info.state.armed.store(false, Ordering::Release);
                let status = self.access(|regs| {
                    regs.mier().write_value(Default::default());
                    regs.misr().read()
                })?;
                if let Some(error) = status_error(status) {
                    return Err(error);
                }
                if phase.ready(status) {
                    return Ok(Poll::Ready(status));
                }
                (info.unpend_irq)();
                self.access(|regs| regs.mier().write_value(phase.mask()))?;
                info.state.armed.store(true, Ordering::Release);
                (info.enable_irq)();
                // Retained status closes both event-before-arm and
                // event-before-sleep windows. No diagnostic flag is cleared.
                let status = self.status()?;
                let error = status_error(status);
                if error.is_some() || phase.ready(status) {
                    (info.disable_irq)();
                    info.state.armed.store(false, Ordering::Release);
                    self.access(|regs| regs.mier().write_value(Default::default()))?;
                    if let Some(error) = error {
                        return Err(error);
                    }
                    return Ok(Poll::Ready(status));
                }
                Ok(Poll::Pending)
            });
            match result {
                Ok(poll) => poll.map(Ok),
                Err(error) => Poll::Ready(Err(error)),
            }
        })
        .await
    }

    async fn enqueue_async(&mut self, command: Command, data: u8) -> Result<(), Error> {
        loop {
            self.wait(WaitFor::Tx).await?;
            let written = critical_section::with(|_| {
                // Recheck every error immediately before touching MTDR. Neither
                // this section nor FIFO capacity promises an address/data ACK.
                let status = self.status()?;
                if let Some(error) = status_error(status) {
                    return Err(error);
                }
                if !status.txe() {
                    return Ok(false);
                }
                if command == Command::Stop {
                    self.lifecycle = Lifecycle::Active { stop_queued: true };
                }
                self.access(|regs| {
                    regs.mtdr().write(|v| {
                        v.set_cmd(command);
                        v.set_data(data);
                    })
                })?;
                Ok(true)
            })?;
            if written {
                return Ok(());
            }
        }
    }

    async fn read_byte_async(&mut self) -> Result<u8, Error> {
        loop {
            self.wait(WaitFor::Rx).await?;
            let byte = critical_section::with(|_| {
                let status = self.status()?;
                if let Some(error) = status_error(status) {
                    return Err(error);
                }
                if !status.rxne() {
                    return Ok(None);
                }
                // DATA and EMPTY are from this one read; a stale RXNE must not
                // store a spurious byte or advance the caller-buffer cursor.
                let value = self.access(|regs| regs.mrdr().read())?;
                Ok((!value.empty()).then(|| value.data()))
            })?;
            if let Some(byte) = byte {
                return Ok(byte);
            }
        }
    }
}

struct AsyncGuard<'a> {
    engine: &'a mut Engine,
    armed: bool,
}
impl AsyncGuard<'_> {
    async fn run(&mut self, address: u8, operations: &mut [Operation<'_>]) -> Result<(), Error> {
        let mut index = 0;
        while index < operations.len() {
            let read = matches!(operations[index], Operation::Read(_));
            let mut end = index + 1;
            while end < operations.len() && matches!(operations[end], Operation::Read(_)) == read {
                end += 1;
            }
            self.engine
                .enqueue_async(Command::StartExpectAck, (address << 1) | u8::from(read))
                .await?;
            if read {
                let count: usize = operations[index..end]
                    .iter()
                    .map(|op| match op {
                        Operation::Read(bytes) => bytes.len(),
                        Operation::Write(_) => 0,
                    })
                    .sum();
                // Exactly one RX command: another queued Receive would change
                // the previous command's last-byte NACK into an ACK.
                self.engine
                    .enqueue_async(Command::Receive, (count - 1) as u8)
                    .await?;
                for op in &mut operations[index..end] {
                    if let Operation::Read(bytes) = op {
                        for byte in bytes.iter_mut() {
                            *byte = self.engine.read_byte_async().await?;
                        }
                    }
                }
            } else {
                for op in &operations[index..end] {
                    if let Operation::Write(bytes) = op {
                        for &byte in *bytes {
                            self.engine.enqueue_async(Command::Transmit, byte).await?;
                        }
                    }
                }
            }
            index = end;
        }
        self.engine.enqueue_async(Command::Stop, 0).await?;
        // Master-generated final-read NACK is not MISR.NACK. Keep checking the
        // received-response NACK fault through STOP and final idle confirmation.
        self.engine.wait(WaitFor::Stop).await?;
        // No MSTBUSY-clear IRQ exists: retain STOP and use a finite sample loop.
        self.engine.confirm_completion()
    }

    fn finish(mut self, result: Result<(), Error>) -> Result<(), Error> {
        let result = result.map_err(|error| self.engine.finish_error(error));
        self.armed = false;
        result
    }
}
impl Drop for AsyncGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            let _ = self.engine.retire(true);
        }
    }
}

// Never replay a raw MCR0 read: reserved bits and R0W1 FIFO strobes stay zero.
fn ordinary_control(control: pac::i2c::regs::Mcr0) -> pac::i2c::regs::Mcr0 {
    let mut value = pac::i2c::regs::Mcr0::default();
    value.set_men(control.men());
    value.set_reset(control.reset());
    value.set_sleepen(control.sleepen());
    value.set_dbgen(control.dbgen());
    value.set_clksrc(control.clksrc());
    value
}

fn ordinary_control_matches(control: pac::i2c::regs::Mcr0, expected: pac::i2c::regs::Mcr0) -> bool {
    control.men() == expected.men()
        && control.reset() == expected.reset()
        && control.sleepen() == expected.sleepen()
        && control.dbgen() == expected.dbgen()
        && control.clksrc() == expected.clksrc()
}

fn clear_observed(regs: pac::i2c::I2c, status: pac::i2c::regs::Misr) {
    // R1W0; write_noop preserves reserved bit2 and every unobserved flag.
    let mut clear = pac::i2c::regs::Micr::write_noop();
    clear.set_packet(!status.packet());
    clear.set_stop(!status.stop());
    clear.set_nack(!status.nack());
    clear.set_arbi(!status.arbi());
    clear.set_fifo(!status.fifo());
    clear.set_pinlow(!status.pinlow());
    clear.set_match_(!status.match_());
    regs.micr().write_value(clear);
}

fn configuration_matches(regs: pac::i2c::I2c, timing: Timing, enabled: bool) -> bool {
    let control = regs.mcr0().read();
    let mut expected = pac::i2c::regs::Mcr0::default();
    expected.set_men(enabled);
    let input = regs.insel().read();
    let mcr1 = regs.mcr1().read();
    let mcr2 = regs.mcr2().read();
    let mcr3 = regs.mcr3().read();
    let clocks = regs.mccr().read();
    let matches = regs.mmatch().read();
    let status = regs.misr().read();
    ordinary_control_matches(control, expected)
        && regs.mier().read().0 == 0
        && regs.mder().read().0 == 0
        && regs.scr0().read().0 == 0
        && regs.sier().read().0 == 0
        && regs.sder().read().0 == 0
        && !regs.sisr().read().slvbusy()
        && input.sclinsel() == 0
        && input.sdainsel() == 0
        && !mcr1.cirfifo()
        && !mcr1.rdmo()
        && mcr2.prescale() == timing.prescale
        && mcr2.pincfg() == 0
        && !mcr2.autostop()
        && !mcr2.ignack()
        && !mcr2.timecfg()
        && !mcr2.trgsel()
        && mcr2.matcfg() == 0
        && mcr3.busidle() == timing.busidle
        && mcr3.fltscl() == timing.scl_filter
        && mcr3.fltsda() == timing.sda_filter
        && regs.mcr4().read().pinlow() == 0
        && clocks.clklo() == timing.clklo
        && clocks.clkhi() == timing.clkhi
        && clocks.sethold() == timing.sethold
        && clocks.datavd() == timing.datavd
        && matches.match0() == 0
        && matches.match1() == 0
        && regs.mfifocr().read().0 == 0
        && status.txe()
        && !status.rxne()
        && !status.mstbusy()
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
    pub(crate) state: &'static State,
    pub(crate) enable_irq: fn(),
    pub(crate) disable_irq: fn(),
    pub(crate) unpend_irq: fn(),
}
impl Info {
    fn constructor_fence(&self) -> Result<(), ConfigError> {
        critical_section::with(|_| {
            (self.disable_irq)();
            self.state.armed.store(false, Ordering::Release);
            (self.unpend_irq)();
            if self.state.faulted.load(Ordering::Acquire) {
                Err(ConfigError::RecoveryFailed)
            } else {
                Ok(())
            }
        })
    }

    fn fence(&self) {
        critical_section::with(|_| {
            (self.disable_irq)();
            self.state.armed.store(false, Ordering::Release);
            (self.unpend_irq)();
        });
    }
}

/// Static state holds an owned waker and flags, never a driver or caller buffer.
pub(crate) struct State {
    waker: AtomicWaker,
    armed: AtomicBool,
    faulted: AtomicBool,
}
impl State {
    pub(crate) const fn new() -> Self {
        Self {
            waker: AtomicWaker::new(),
            armed: AtomicBool::new(false),
            faulted: AtomicBool::new(false),
        }
    }
}

/// Dedicated GLOBAL handler. Mask the vector, then wake an armed waiter.
/// It performs no I2C MMIO, so a delayed invocation is safe after clock gating.
pub struct InterruptHandler<T: Instance>(PhantomData<T>);
impl<T: Instance> Handler<T::Interrupt> for InterruptHandler<T> {
    unsafe fn on_interrupt() {
        let info = T::info();
        let wake = critical_section::with(|_| {
            (info.disable_irq)();
            let armed = info.state.armed.load(Ordering::Acquire);
            info.state.armed.store(false, Ordering::Release);
            armed
        });
        if wake {
            info.state.waker.wake();
        }
    }
}

pub(crate) fn enable_interrupt<I: Interrupt>() {
    unsafe { I::enable() };
}
pub(crate) fn disable_interrupt<I: Interrupt>() {
    I::disable();
}
pub(crate) fn unpend_interrupt<I: Interrupt>() {
    I::unpend();
}
pub(crate) mod sealed {
    use super::*;
    pub(crate) trait Instance:
        crate::rcc::RccPeripheral + PeripheralType + Into<AnyI2c>
    {
        fn info() -> &'static Info;
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
pub trait Instance: sealed::Instance {
    /// Dedicated GLOBAL vector resolved from this native instance's metadata.
    type Interrupt: Interrupt;
}
/// Verified, bonded SCL alternate-function route for this instance.
#[allow(private_bounds)]
pub trait SclPin<T: Instance>: Pin + sealed::SclPin<T> {}
/// Verified, bonded SDA alternate-function route for this instance.
#[allow(private_bounds)]
pub trait SdaPin<T: Instance>: Pin + sealed::SdaPin<T> {}
macro_rules! impl_instance {
    ($name:ident, $irq:ident) => {
        impl $crate::i2c::sealed::Instance for $crate::peripherals::$name {
            fn info() -> &'static $crate::i2c::Info {
                static STATE: $crate::i2c::State = $crate::i2c::State::new();
                static INFO: $crate::i2c::Info = $crate::i2c::Info {
                    regs: $crate::pac::$name,
                    rcc: <$crate::peripherals::$name as $crate::rcc::SealedRccPeripheral>::RCC_INFO,
                    state: &STATE,
                    enable_irq: $crate::i2c::enable_interrupt::<$crate::interrupt::typelevel::$irq>,
                    disable_irq: $crate::i2c::disable_interrupt::<$crate::interrupt::typelevel::$irq>,
                    unpend_irq: $crate::i2c::unpend_interrupt::<$crate::interrupt::typelevel::$irq>,
                };
                &INFO
            }
        }
        impl $crate::i2c::Instance for $crate::peripherals::$name {
            type Interrupt = $crate::interrupt::typelevel::$irq;
        }
        impl From<$crate::peripherals::$name> for $crate::i2c::AnyI2c {
            fn from(_: $crate::peripherals::$name) -> Self {
                Self { info: <$crate::peripherals::$name as $crate::i2c::sealed::Instance>::info() }
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
