//! Blocking and interrupt-driven I2C masters for verified state-code controllers.
//!
//! Uses the CW32 state-code controller, not an STM32 I2C register model. The
//! peripheral and both package-checked alternate-function pins are owned through
//! [`Peri`]. External pull-ups are normally required. Register behavior follows
//! CW32x030 User Manual CN V2.5 §§20.4–20.7, checked against EN V1.0 and SDK V2.2,
//! plus the L031/R031/W031/L052/L083 manuals and SDKs. The shared
//! `cw32l031_v1` source audit is recorded in `docs/l031-shared-serial.md`.
//! F002/F003 and L010/L011 support is documented in
//! `docs/remaining-spi-classic-i2c.md`. Typed status encodings, including the
//! subsequently obtained L011 manual, are audited in `docs/i2c-typed-registers.md`.
//!
//! Seven/ten-bit addressing, repeated START, adjacent operation merging, clock
//! stretching and arbitration-loss reporting are supported. Blocking waits and
//! async idle/STOP/cleanup use finite sample budgets, not wall-clock deadlines.
//! Async SI waits have no internal deadline; an independent timeout can cancel
//! them, but cannot preempt synchronous work or destructors. Cancellation can
//! truncate a byte and leave a target holding a line or retaining earlier effects.
//! Transactions are never automatically retried or rolled back. No DMA, slave,
//! bus-recovery pulses or electrical-validation claim is made.
use core::future::poll_fn;
use core::marker::PhantomData;
use core::sync::atomic::{AtomicBool, Ordering};
use core::task::Poll;

use embassy_sync::waitqueue::AtomicWaker;

use embedded_hal::i2c::NoAcknowledgeSource;
pub use embedded_hal::i2c::Operation;

use crate::gpio::{Flex, Pin, Pull};
use crate::interrupt::typelevel::{Binding, Handler, Interrupt};
use crate::mode::{Async, Blocking, Mode};
use crate::rcc::{ClockBounds, Readback};
use crate::time::Hertz;
use crate::{Peri, PeripheralType, pac};

use pac::i2c::vals::Status;

const MAX_FREQUENCY: u32 = crate::I2C_MAXIMUM_FREQUENCY_HZ;

/// Explicit target address width, including ten-bit values below 128.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Address {
    SevenBit(u8),
    TenBit(u16),
}
impl Address {
    fn valid(self) -> bool {
        match self {
            Self::SevenBit(value) => value <= 0x7f,
            Self::TenBit(value) => value <= 0x3ff,
        }
    }
    // Tuple: generate START, byte, expected ACK state. A low address byte is
    // transmitted as controller data, but its NACK is an address failure.
    fn phases(self, read: bool, selected: bool) -> [Option<(bool, u8, Status)>; 3] {
        let ack = if read { Status::ReadAddressAck } else { Status::WriteAddressAck };
        match self {
            Self::SevenBit(value) => [Some((true, (value << 1) | u8::from(read), ack)), None, None],
            Self::TenBit(value) => {
                let high = 0xf0 | (((value >> 8) as u8 & 3) << 1);
                if read && selected {
                    [Some((true, high | 1, Status::ReadAddressAck)), None, None]
                } else {
                    [
                        Some((true, high, Status::WriteAddressAck)),
                        Some((false, value as u8, Status::WriteDataAck)),
                        if read { Some((true, high | 1, Status::ReadAddressAck)) } else { None },
                    ]
                }
            }
        }
    }
}
fn address_error(error: Error) -> Error {
    match error {
        Error::DataNack => Error::AddressNack,
        other => other,
    }
}


/// Checked I2C configuration failure. Invalid construction does not touch MMIO.
/// Async reconfiguration first retires a forgotten transfer, even for an invalid
/// candidate; the cached configuration changes only after successful setup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
pub enum ConfigError {
    /// Initialize clocks with [`crate::init`] before constructing a driver.
    ClockNotInitialized,
    /// Frequency must be nonzero.
    FrequencyZero,
    /// Requested frequency exceeds the verified 1 MHz controller limit.
    FrequencyTooHigh,
    /// Even BRR=255 at the fastest qualified PCLK exceeds the request.
    FrequencyTooLow,
    /// The per-phase polling bound must be nonzero.
    ZeroPollLimit,
    /// SDA or SCL is low, or a controller operation is pending.
    Busy,
    /// Local retirement/reset/configuration could not be confirmed. Terminal
    /// across both construction modes until chip reset; the gate is retained.
    Faulted,
}
impl core::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::ClockNotInitialized => "I2C peripheral clock is not initialized",
            Self::FrequencyZero => "I2C frequency must be nonzero",
            Self::FrequencyTooHigh => "I2C frequency exceeds 1 MHz",
            Self::FrequencyTooLow => "I2C frequency is below the fastest qualified PCLK/2048",
            Self::ZeroPollLimit => "I2C polling limit must be nonzero",
            Self::Busy => "I2C bus is busy",
            Self::Faulted => "I2C local shutdown or initialization was not confirmed",
        })
    }
}
impl core::error::Error for ConfigError {}

/// Transfer failure. Already transferred bytes are not rolled back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
pub enum Error {
    /// Bus error or an unexpected controller state.
    Bus,
    /// Arbitration lost. No STOP is generated and the transaction is not retried.
    Arbitration,
    /// Address was not acknowledged.
    AddressNack,
    /// A transmitted data byte was not acknowledged.
    DataNack,
    /// A phase or STOP did not complete within the configured polling bound.
    Timeout,
    /// Lines did not become high before beginning a transaction.
    BusBusy,
    /// Address exceeds its explicit seven-bit or ten-bit range.
    InvalidAddress,
    /// Empty reads are rejected before START; an old forgotten transfer may
    /// first require retirement on an async owner.
    ZeroLengthTransfer,
    /// Exceptional async retirement or checked initialization readback failure.
    /// Terminal until chip reset; does not claim a completed STOP or bus recovery.
    Faulted,
}
impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::Bus => "I2C bus error",
            Self::Arbitration => "I2C arbitration lost",
            Self::AddressNack => "I2C address not acknowledged",
            Self::DataNack => "I2C data not acknowledged",
            Self::Timeout => "I2C transfer timed out",
            Self::BusBusy => "I2C bus remained busy",
            Self::InvalidAddress => "I2C address exceeds its selected width",
            Self::ZeroLengthTransfer => "I2C empty read is unsupported",
            Self::Faulted => "I2C local shutdown or initialization was not confirmed",
        })
    }
}
impl core::error::Error for Error {}
impl embedded_hal::i2c::Error for Error {
    fn kind(&self) -> embedded_hal::i2c::ErrorKind {
        use embedded_hal::i2c::ErrorKind;
        match self {
            Self::Bus => ErrorKind::Bus,
            Self::Arbitration => ErrorKind::ArbitrationLoss,
            Self::AddressNack => ErrorKind::NoAcknowledge(NoAcknowledgeSource::Address),
            Self::DataNack => ErrorKind::NoAcknowledge(NoAcknowledgeSource::Data),
            _ => ErrorKind::Other,
        }
    }
}

/// Master configuration. Weak pull-ups do not replace board-level rise-time design.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct Config {
    /// SCL ceiling over the qualified RCC clock envelope, capped at 1 MHz.
    /// The fastest legal BRR satisfying this bound is used.
    pub frequency: Hertz,
    /// Enable the SCL pin's weak pull-up in addition to any external pull-up.
    pub scl_pullup: bool,
    /// Enable the SDA pin's weak pull-up in addition to any external pull-up.
    pub sda_pullup: bool,
    /// Maximum samples per synchronous idle, STOP, cleanup or setup phase.
    ///
    /// This is not a duration. CPU frequency, optimization and interrupt latency
    /// affect elapsed time. Increase it for slow clock-stretching devices. A long
    /// blocking transaction may use this bound for each byte and START/address.
    /// Async SI waits are interrupt-driven and have no internal timeout.
    pub poll_limit: u32,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            frequency: Hertz(100_000),
            scl_pullup: false,
            sda_pullup: false,
            poll_limit: 100_000,
        }
    }
}

/// Owned seven-bit I2C master, retaining the peripheral and both pins.
///
/// The ISR only masks its dedicated vector and wakes; it never holds a caller
/// buffer pointer or advances a byte. Dropping a pending future retires locally
/// with finite synchronous work. Forgetting it leaves an owner-local marker that
/// the next transaction or reconfiguration retires before new work. A terminal
/// local readback failure retains the gate and blocks reconstruction until reset.
pub struct I2c<'d, M: Mode> {
    peripheral: Peri<'d, AnyI2c>,
    scl: Flex<'d>,
    sda: Flex<'d>,
    scl_af: u8,
    sda_af: u8,
    engine: Engine,
    lifecycle: Lifecycle,
    asynchronous: bool,
    kernel_clock: ClockBounds,
    config: Config,
    actual_frequency: Hertz,
    divider: u8,
    frequency_bounds: ClockBounds,
    _mode: PhantomData<M>,
}
impl<'d> I2c<'d, Blocking> {
    /// Construct a blocking master; invalid configuration is rejected before MMIO.
    ///
    /// Tokens are consumed on error too; pass `reborrow()` when needed afterward.
    /// This does not enable the NVIC I2C interrupt. Do not independently unmask it.
    pub fn new_blocking<T: Instance>(
        peripheral: Peri<'d, T>,
        scl: Peri<'d, impl SclPin<T>>,
        sda: Peri<'d, impl SdaPin<T>>,
        config: Config,
    ) -> Result<Self, ConfigError> {
        Self::new_inner(peripheral, scl, sda, config, false)
    }

    /// Read with an unshifted seven-bit address. An empty buffer is rejected.
    pub fn blocking_read(&mut self, address: u8, read: &mut [u8]) -> Result<(), Error> {
        self.blocking_transaction(address, &mut [Operation::Read(read)])
    }
    /// Write with an unshifted seven-bit address. Empty writes perform an address probe.
    pub fn blocking_write(&mut self, address: u8, write: &[u8]) -> Result<(), Error> {
        self.blocking_transaction(address, &mut [Operation::Write(write)])
    }
    /// Write then read with a repeated START between the directions and one STOP.
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
    /// Execute an embedded-hal transaction with one START and one final STOP.
    ///
    /// Adjacent operations of the same direction share a single address phase.
    /// Only the last byte of each contiguous read run is NACKed. Direction changes
    /// generate repeated START. Empty writes are retained; empty reads cause an
    /// error before any bus activity. An empty operations list does nothing.
    pub fn blocking_transaction(
        &mut self,
        address: u8,
        operations: &mut [Operation<'_>],
    ) -> Result<(), Error> {
        self.blocking_transaction_with_address(Address::SevenBit(address), operations)
    }
    /// Explicit width; low ten-bit values remain ten-bit targets. The complete
    /// address is validated before START. Low-address NACK is AddressNack.
    pub fn blocking_transaction_with_address(
        &mut self,
        address: Address,
        operations: &mut [Operation<'_>],
    ) -> Result<(), Error> {
        self.engine.transaction(address, operations, || {
            self.scl.is_high() && self.sda.is_high()
        })
    }
}
impl<'d, M: Mode> I2c<'d, M> {
    fn new_inner<T: Instance>(
        peripheral: Peri<'d, T>,
        scl: Peri<'d, impl SclPin<T>>,
        sda: Peri<'d, impl SdaPin<T>>,
        config: Config,
        asynchronous: bool,
    ) -> Result<Self, ConfigError> {
        let kernel_clock =
            crate::rcc::bus_clock_bounds::<T>().ok_or(ConfigError::ClockNotInitialized)?;
        let timing = select_timing(kernel_clock, &config)?;
        let scl_af = sealed::SclPin::<T>::af(&*scl);
        let sda_af = sealed::SdaPin::<T>::af(&*sda);
        let peripheral: Peri<'d, AnyI2c> = peripheral.into();
        let info = peripheral.info;
        // Flex::new disconnects pins: reject sticky faults and fence a forgotten
        // owner's IRQ first, before any pin mutation or peripheral MMIO.
        info.constructor_fence()?;
        let mut scl = Flex::new(scl);
        let mut sda = Flex::new(sda);
        info.initialize(timing, config.poll_limit)?;
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
                base: timing.control(),
                poll_limit: config.poll_limit,
                master: false,
            },
            lifecycle: Lifecycle::Idle,
            asynchronous,
            kernel_clock,
            config,
            actual_frequency: timing.actual_frequency,
            divider: timing.brr,
            frequency_bounds: kernel_clock.divided_by(8 * (u32::from(timing.brr) + 1)),
            _mode: PhantomData,
        })
    }

    /// Nominal programmed SCL rate, truncated to whole hertz, not measured.
    /// See [`Self::get_current_frequency_bounds`] for oscillator uncertainty.
    pub fn get_current_frequency(&self) -> Hertz {
        self.actual_frequency
    }
    /// Selected-source bounds on the divider-generated SCL rate, rounded outward.
    /// The interval assumes unchanged clocks and RCC's qualified temperature
    /// and supply range. Rise time, clock stretching, and SI service delays can
    /// make the physical bus slower; the lower endpoint does not cover them.
    pub fn get_current_frequency_bounds(&self) -> (Hertz, Hertz) {
        (
            self.frequency_bounds.minimum(),
            self.frequency_bounds.maximum(),
        )
    }
    /// Current requested configuration.
    pub fn get_current_config(&self) -> Config {
        self.config
    }
    /// Apply a candidate only after idle checks and successful configuration.
    /// Async owners first retire any forgotten transfer. Invalid/busy candidates
    /// retain the old cached configuration and any confirmed disabled state.
    pub fn set_config(&mut self, config: &Config) -> Result<(), ConfigError> {
        if self.check_fault().is_err() {
            return Err(ConfigError::Faulted);
        }
        if self.asynchronous {
            if matches!(self.lifecycle, Lifecycle::Active { .. }) {
                if self.retire().is_err_and(|error| error == Error::Faulted) {
                    return Err(ConfigError::Faulted);
                }
            }
            let timing = select_timing(self.kernel_clock, config)?;
            let control_busy = if self.lifecycle == Lifecycle::Idle {
                critical_section::with(|_| {
                    let control = self.engine.regs.cr().read();
                    control.sta() || control.sto() || control.si()
                })
            } else {
                false
            };
            if control_busy || !self.scl.is_high() || !self.sda.is_high() {
                return Err(ConfigError::Busy);
            }
            if self.lifecycle == Lifecycle::Idle {
                self.disable_local().map_err(|_| ConfigError::Faulted)?;
            }
            self.reinitialize(timing, config.poll_limit)
                .map_err(|_| ConfigError::Faulted)?;
            self.commit_config(config, timing);
            return Ok(());
        }
        // Preserve the ordinary blocking configuration path and preconditions.
        let timing = select_timing(self.kernel_clock, config)?;
        let control = self.engine.regs.cr().read();
        if control.sta()
            || control.sto()
            || control.si()
            || !self.scl.is_high()
            || !self.sda.is_high()
        {
            return Err(ConfigError::Busy);
        }
        configure(self.engine.regs, timing);
        self.commit_config(config, timing);
        Ok(())
    }

    fn commit_config(&mut self, config: &Config, timing: Timing) {
        self.scl
            .set_as_af_open_drain(self.scl_af, pull(config.scl_pullup));
        self.sda
            .set_as_af_open_drain(self.sda_af, pull(config.sda_pullup));
        self.engine.base = timing.control();
        self.engine.poll_limit = config.poll_limit;
        self.config = *config;
        self.actual_frequency = timing.actual_frequency;
        self.divider = timing.brr;
        self.frequency_bounds = self
            .kernel_clock
            .divided_by(8 * (u32::from(timing.brr) + 1));
    }

    fn check_fault(&mut self) -> Result<(), Error> {
        if self.lifecycle == Lifecycle::Faulted
            || self.peripheral.info.state.faulted.load(Ordering::Acquire)
        {
            self.lifecycle = Lifecycle::Faulted;
            Err(Error::Faulted)
        } else {
            Ok(())
        }
    }

    fn disable_local(&mut self) -> Result<(), Error> {
        if self
            .peripheral
            .info
            .disable_checked(self.engine.base, self.engine.poll_limit)
        {
            self.lifecycle = Lifecycle::NeedsReinitialize;
            Ok(())
        } else {
            self.peripheral.info.latch_fault();
            self.lifecycle = Lifecycle::Faulted;
            Err(Error::Faulted)
        }
    }

    fn reinitialize(&mut self, timing: Timing, attempts: u32) -> Result<(), Error> {
        self.check_fault()?;
        if self
            .peripheral
            .info
            .reset_and_configure(timing, attempts)
            .is_err()
        {
            self.lifecycle = Lifecycle::Faulted;
            return Err(Error::Faulted);
        }
        self.lifecycle = Lifecycle::Idle;
        Ok(())
    }

    // The single bounded error/cancellation/forgotten-owner retirement path.
    // Only Active may authorize a STOP from a fresh held master status.
    fn retire(&mut self) -> Result<(), Error> {
        let info = self.peripheral.info;
        info.mask();
        self.check_fault()?;
        let Lifecycle::Active { stop_attempted } = self.lifecycle else {
            return if self.lifecycle == Lifecycle::Idle {
                self.disable_local()
            } else {
                Ok(()) // NeedsReinitialize is already confirmed disabled.
            };
        };
        let mut arbitration = false;
        let mut requested_stop = false;
        critical_section::with(|_| {
            let control = info.regs.cr().read();
            if control.si() {
                let status = info.regs.stat().read().stat();
                if is_arbitration(status) {
                    arbitration = true;
                    info.regs.cr().write_value(self.engine.base);
                } else if !stop_attempted
                    && (is_master_boundary(status) || status == Status::BusError)
                {
                    self.lifecycle = Lifecycle::Active {
                        stop_attempted: true,
                    };
                    let mut command = self.engine.base;
                    command.set_sto(true);
                    info.regs.cr().write_value(command);
                    requested_stop = true;
                }
            }
        });
        let stop_result = if requested_stop {
            self.wait_stop()
        } else {
            Ok(())
        };
        arbitration |= stop_result == Err(Error::Arbitration);
        if stop_result.is_err() {
            // One failed STOP/error clear consumes the allowance. Reclassify
            // live arbitration before disable, never issue another STOP.
            critical_section::with(|_| {
                if info.regs.cr().read().si() && is_arbitration(info.regs.stat().read().stat()) {
                    arbitration = true;
                    info.regs.cr().write_value(self.engine.base);
                }
            });
        }
        self.disable_local()?;
        if arbitration {
            Err(Error::Arbitration)
        } else {
            Ok(())
        }
    }

    fn wait_stop(&self) -> Result<(), Error> {
        for _ in 0..self.engine.poll_limit {
            let result = critical_section::with(|_| {
                let control = self.engine.regs.cr().read();
                if control.si() {
                    Some(Err(
                        if is_arbitration(self.engine.regs.stat().read().stat()) {
                            Error::Arbitration
                        } else {
                            Error::Bus
                        },
                    ))
                } else if !control.sto() {
                    Some(Ok(()))
                } else {
                    None
                }
            });
            if let Some(result) = result {
                return result;
            }
            core::hint::spin_loop();
        }
        Err(Error::Timeout)
    }
}
impl<M: Mode> Drop for I2c<'_, M> {
    fn drop(&mut self) {
        let info = self.peripheral.info;
        info.mask();
        if self.check_fault().is_err() {
            return;
        }
        if self.asynchronous {
            match self.lifecycle {
                Lifecycle::Idle => {
                    let _ = self.disable_local();
                }
                Lifecycle::Active { .. } => {
                    let _ = self.retire();
                }
                Lifecycle::NeedsReinitialize => {} // No stale STAT or EN=1 write.
                Lifecycle::Faulted => return,
            }
            // Cleanup can itself become Faulted; never gate an unconfirmed owner.
            if self.check_fault().is_err() {
                return;
            }
        } else {
            // Existing blocking teardown, with the common static fault/IRQ fence.
            self.engine.regs.cr().write_value(Default::default());
            self.engine.regs.brren().write(|v| v.set_en(false));
        }
        critical_section::with(|cs| info.rcc.disable_with_cs(cs))
            .expect("I2C clock gate did not disable");
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
#[derive(Clone, Copy, PartialEq, Eq)]
enum Lifecycle {
    Idle,
    Active { stop_attempted: bool },
    NeedsReinitialize,
    Faulted,
}

impl<'d> I2c<'d, Async> {
    /// Construct with the instance's dedicated GLOBAL interrupt binding.
    /// Tokens are consumed on error; use `reborrow()` if needed afterward.
    pub fn new_async<T: Instance>(
        peripheral: Peri<'d, T>,
        scl: Peri<'d, impl SclPin<T>>,
        sda: Peri<'d, impl SdaPin<T>>,
        _irqs: impl Binding<T::Interrupt, InterruptHandler<T>> + 'd,
        config: Config,
    ) -> Result<Self, ConfigError> {
        Self::new_inner(peripheral, scl, sda, config, true)
    }

    /// Read using an unshifted seven-bit address; empty reads are rejected.
    pub async fn read(&mut self, address: u8, read: &mut [u8]) -> Result<(), Error> {
        self.transaction(address, &mut [Operation::Read(read)])
            .await
    }

    /// Write using an unshifted seven-bit address; an empty write probes it.
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

    /// Transfer with adjacent same-direction operations sharing an address phase.
    /// Only the last byte in a contiguous read run receives NACK. Direction
    /// changes repeat START; successful return includes local STOP completion.
    ///
    /// Invalid input starts no new transfer. A forgotten transfer is retired
    /// before validation, so that retirement can still affect the old bus work.
    /// Cancellation is bounded local cleanup, not atomicity or target recovery.
    pub async fn transaction(
        &mut self,
        address: u8,
        operations: &mut [Operation<'_>],
    ) -> Result<(), Error> {
        self.transaction_with_address(Address::SevenBit(address), operations).await
    }
    /// Explicit seven/ten-bit transfer; retires forgotten work before preflight.
    /// Cancellation and STOP/fault handling match the seven-bit transaction.
    pub async fn transaction_with_address(
        &mut self,
        address: Address,
        operations: &mut [Operation<'_>],
    ) -> Result<(), Error> {
        self.check_fault()?;
        if matches!(self.lifecycle, Lifecycle::Active { .. }) {
            self.retire()?;
        }
        if !address.valid() {
            return Err(Error::InvalidAddress);
        }
        if operations
            .iter()
            .any(|op| matches!(op, Operation::Read(bytes) if bytes.is_empty()))
        {
            return Err(Error::ZeroLengthTransfer);
        }
        if operations.is_empty() {
            return Ok(());
        }
        if self.lifecycle == Lifecycle::NeedsReinitialize {
            let timing = Timing {
                brr: self.divider,
                actual_frequency: self.actual_frequency,
            };
            self.reinitialize(timing, self.engine.poll_limit)?;
        }
        self.wait_idle()?;
        // The guard and owner marker precede the first START. Only the guard
        // borrows caller operations; the interrupt state contains no pointers.
        self.lifecycle = Lifecycle::Active {
            stop_attempted: false,
        };
        let mut guard = AsyncGuard {
            driver: self,
            armed: true,
        };
        let result = guard.run(address, operations).await;
        guard.finish(result)
    }

    fn wait_idle(&mut self) -> Result<(), Error> {
        for _ in 0..self.engine.poll_limit {
            let (control, status) = critical_section::with(|_| {
                let control = self.engine.regs.cr().read();
                let status = control.si().then(|| self.engine.regs.stat().read().stat());
                (control, status)
            });
            if let Some(status) = status {
                // Idle/inherited state never authorizes a master STOP.
                let error = if is_arbitration(status) {
                    Error::Arbitration
                } else {
                    Error::Bus
                };
                self.peripheral.info.mask();
                self.disable_local()?;
                return Err(error);
            }
            if !control.sta() && !control.sto() && self.scl.is_high() && self.sda.is_high() {
                return Ok(());
            }
            core::hint::spin_loop();
        }
        Err(Error::BusBusy)
    }

    fn command(&self, control: pac::i2c::regs::Cr, data: Option<u8>) {
        let info = self.peripheral.info;
        critical_section::with(|_| {
            info.state.armed.store(false, Ordering::Release);
            (info.disable_irq)();
            if let Some(byte) = data {
                info.regs.dr().write(|v| v.set_dr(byte));
            }
            info.regs.cr().write_value(control);
        });
    }

    async fn expect(&self, expected: Status) -> Result<(), Error> {
        let info = self.peripheral.info;
        let status = poll_fn(|cx| {
            info.state.waker.register(cx.waker());
            critical_section::with(|_| {
                info.state.armed.store(false, Ordering::Release);
                (info.disable_irq)();
                if !info.regs.cr().read().si() {
                    info.state.armed.store(true, Ordering::Release);
                    (info.enable_irq)();
                    if !info.regs.cr().read().si() {
                        return Poll::Pending;
                    }
                    info.state.armed.store(false, Ordering::Release);
                    (info.disable_irq)();
                }
                // SI is still held. Decode only after observing it; ISR never
                // clears SI, copies DR, or changes this expected protocol state.
                Poll::Ready(info.regs.stat().read().stat())
            })
        })
        .await;
        match status {
            Status::WriteAddressNack | Status::ReadAddressNack => Err(Error::AddressNack),
            Status::WriteDataNack => Err(Error::DataNack),
            status if is_arbitration(status) => Err(Error::Arbitration),
            status if status == expected => Ok(()),
            _ => Err(Error::Bus),
        }
    }

    fn stop_async(&mut self) -> Result<(), Error> {
        let info = self.peripheral.info;
        critical_section::with(|_| {
            let control = info.regs.cr().read();
            if !control.si() {
                return Err(Error::Bus);
            }
            let status = info.regs.stat().read().stat();
            if is_arbitration(status) {
                return Err(Error::Arbitration);
            }
            if !is_master_boundary(status) {
                return Err(Error::Bus);
            }
            self.lifecycle = Lifecycle::Active {
                stop_attempted: true,
            };
            let mut command = self.engine.base;
            command.set_sto(true);
            info.regs.cr().write_value(command);
            Ok(())
        })?;
        self.wait_stop()
    }
}

struct AsyncGuard<'a, 'd> {
    driver: &'a mut I2c<'d, Async>,
    armed: bool,
}
impl AsyncGuard<'_, '_> {
    async fn run(&mut self, address: Address, operations: &mut [Operation<'_>]) -> Result<(), Error> {
        let mut previous = None;
        let count = operations.len();
        for index in 0..count {
            let read = matches!(operations[index], Operation::Read(_));
            if previous != Some(read) {
                for (phase_index, phase) in address
                    .phases(read, previous == Some(false))
                    .into_iter()
                    .enumerate()
                {
                    let Some((start, byte, expected)) = phase else {
                        continue;
                    };
                    if start {
                        let mut control = self.driver.engine.base;
                        control.set_sta(true);
                        self.driver.command(control, None);
                        self.driver.expect(if previous.is_none() && phase_index == 0 {
                            Status::Start
                        } else {
                            Status::RepeatedStart
                        }).await?;
                    }
                    self.driver.command(self.driver.engine.base, Some(byte));
                    let result = self.driver.expect(expected).await;
                    if start {
                        result?;
                    } else {
                        result.map_err(address_error)?;
                    }
                }
            }
            let read_continues =
                index + 1 < count && matches!(operations[index + 1], Operation::Read(_));
            match &mut operations[index] {
                Operation::Write(bytes) => {
                    for &byte in *bytes {
                        self.driver.command(self.driver.engine.base, Some(byte));
                        self.driver.expect(Status::WriteDataAck).await?;
                    }
                }
                Operation::Read(bytes) => {
                    let len = bytes.len();
                    for (offset, byte) in bytes.iter_mut().enumerate() {
                        let ack = offset + 1 < len || read_continues;
                        let mut control = self.driver.engine.base;
                        control.set_aa(ack);
                        self.driver.command(control, None);
                        self.driver
                            .expect(if ack {
                                Status::ReadDataAck
                            } else {
                                Status::ReadDataNack
                            })
                            .await?;
                        *byte =
                            critical_section::with(|_| self.driver.engine.regs.dr().read().dr());
                    }
                }
            }
            previous = Some(read);
        }
        self.driver.stop_async()
    }

    fn finish(mut self, result: Result<(), Error>) -> Result<(), Error> {
        let result = match result {
            Ok(()) => {
                self.driver.lifecycle = Lifecycle::Idle;
                Ok(())
            }
            Err(original) => match self.driver.retire() {
                Err(Error::Faulted) => Err(Error::Faulted),
                Err(Error::Arbitration) => Err(Error::Arbitration),
                _ => Err(original),
            },
        };
        self.armed = false;
        result
    }
}
impl Drop for AsyncGuard<'_, '_> {
    fn drop(&mut self) {
        if self.armed {
            let _ = self.driver.retire();
        }
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

fn is_arbitration(status: Status) -> bool {
    matches!(
        status,
        Status::ArbitrationLost
            | Status::ArbitrationLostSlaveWrite
            | Status::ArbitrationLostGeneralCall
            | Status::ArbitrationLostSlaveRead
    )
}
fn is_master_boundary(status: Status) -> bool {
    matches!(
        status,
        Status::Start
            | Status::RepeatedStart
            | Status::WriteAddressAck
            | Status::WriteAddressNack
            | Status::WriteDataAck
            | Status::WriteDataNack
            | Status::ReadAddressAck
            | Status::ReadAddressNack
            | Status::ReadDataAck
            | Status::ReadDataNack
    )
}

fn pull(enabled: bool) -> Pull {
    if enabled { Pull::Up } else { Pull::None }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Timing {
    brr: u8,
    actual_frequency: Hertz,
}
impl Timing {
    fn control(self) -> pac::i2c::regs::Cr {
        let mut control = pac::i2c::regs::Cr::default();
        control.set_en(true);
        control.set_flt(self.brr <= 9);
        control
    }
}
fn select_timing(clock: ClockBounds, config: &Config) -> Result<Timing, ConfigError> {
    if clock.minimum().0 == 0 {
        return Err(ConfigError::ClockNotInitialized);
    }
    if config.frequency.0 == 0 {
        return Err(ConfigError::FrequencyZero);
    }
    if config.frequency.0 > MAX_FREQUENCY {
        return Err(ConfigError::FrequencyTooHigh);
    }
    if config.poll_limit == 0 {
        return Err(ConfigError::ZeroPollLimit);
    }
    // Round the fastest qualified PCLK UP, so oscillator error cannot exceed
    // the requested ceiling. Whole-Hz rounding is conservative; reporting keeps
    // the exact source fraction through the additional hardware divisor.
    // BRR=0 is explicitly excluded by CN V2.5 §20.4.2, despite SDK accepting it.
    let divisor = u64::from(clock.maximum().0)
        .div_ceil(8 * u64::from(config.frequency.0))
        .max(2);
    if divisor > 256 {
        return Err(ConfigError::FrequencyTooLow);
    }
    Ok(Timing {
        brr: (divisor - 1) as u8,
        actual_frequency: clock.divided_by(8 * divisor as u32).nominal(),
    })
}
fn configure(regs: pac::i2c::I2c, timing: Timing) {
    regs.cr().write_value(Default::default());
    regs.brren().write(|v| v.set_en(false));
    regs.brr().write(|v| v.set_brr(timing.brr));
    // Master targeting is sent through DR, never through own-address registers.
    // General-call is reset-disabled; AA stays clear outside a master read.
    regs.brren().write(|v| v.set_en(true));
    // L010/L011 SCLINSRC/SDAINSRC are zero: select GPIO, not comparators.
    // Engine base/control writes retain this deliberate zero selection.
    regs.cr().write_value(timing.control());
}

// Each command starts from the complete typed CR baseline, never a CR read.
// Clearing SI advances the controller, so flags must change in one MMIO write.
struct Engine {
    regs: pac::i2c::I2c,
    base: pac::i2c::regs::Cr,
    poll_limit: u32,
    master: bool,
}
impl Engine {
    fn transaction(
        &mut self,
        address: Address,
        operations: &mut [Operation<'_>],
        mut lines_high: impl FnMut() -> bool,
    ) -> Result<(), Error> {
        // Preflight every operation: invalid input must not partially write a device.
        if !address.valid() {
            return Err(Error::InvalidAddress);
        }
        if operations
            .iter()
            .any(|op| matches!(op, Operation::Read(bytes) if bytes.is_empty()))
        {
            return Err(Error::ZeroLengthTransfer);
        }
        if operations.is_empty() {
            return Ok(());
        }
        let mut idle = false;
        for _ in 0..self.poll_limit {
            let control = self.regs.cr().read();
            if control.si() {
                // Noise can latch a bus fault even while no call is active.
                // Clear it through the documented recovery path instead of
                // reporting BusBusy forever. No user bytes are sent on this call.
                let error = if matches!(
                    self.regs.stat().read().stat(),
                    Status::ArbitrationLost
                        | Status::ArbitrationLostSlaveWrite
                        | Status::ArbitrationLostGeneralCall
                        | Status::ArbitrationLostSlaveRead
                ) {
                    Error::Arbitration
                } else {
                    Error::Bus
                };
                self.abort(error);
                return Err(error);
            }
            if lines_high() && !control.sta() && !control.sto() {
                idle = true;
                break;
            }
            core::hint::spin_loop();
        }
        if !idle {
            return Err(Error::BusBusy);
        }
        let result = self.run(address, operations);
        if let Err(error) = result {
            self.abort(error);
        }
        result
    }

    fn run(&mut self, address: Address, operations: &mut [Operation<'_>]) -> Result<(), Error> {
        let mut previous = None;
        let count = operations.len();
        for index in 0..count {
            let read = matches!(operations[index], Operation::Read(_));
            if previous != Some(read) {
                for (phase_index, phase) in address
                    .phases(read, previous == Some(false))
                    .into_iter()
                    .enumerate()
                {
                    let Some((start, byte, expected)) = phase else {
                        continue;
                    };
                    if start {
                        let mut control = self.base;
                        control.set_sta(true);
                        self.regs.cr().write_value(control);
                        self.expect(if previous.is_none() && phase_index == 0 {
                            Status::Start
                        } else {
                            Status::RepeatedStart
                        })?;
                        self.master = true;
                    }
                    self.regs.dr().write(|v| v.set_dr(byte));
                    self.regs.cr().write_value(self.base);
                    let result = self.expect(expected);
                    if start {
                        result?;
                    } else {
                        result.map_err(address_error)?;
                    }
                }
            }
            let read_continues =
                index + 1 < count && matches!(operations[index + 1], Operation::Read(_));
            match &mut operations[index] {
                Operation::Write(bytes) => {
                    for &byte in *bytes {
                        self.regs.dr().write(|v| v.set_dr(byte));
                        self.regs.cr().write_value(self.base);
                        self.expect(Status::WriteDataAck)?;
                    }
                }
                Operation::Read(bytes) => {
                    let len = bytes.len();
                    for (offset, byte) in bytes.iter_mut().enumerate() {
                        let ack = offset + 1 < len || read_continues;
                        let mut control = self.base;
                        control.set_aa(ack);
                        self.regs.cr().write_value(control);
                        self.expect(if ack {
                            Status::ReadDataAck
                        } else {
                            Status::ReadDataNack
                        })?;
                        *byte = self.regs.dr().read().dr();
                    }
                }
            }
            previous = Some(read);
        }
        self.stop()
    }

    fn expect(&mut self, expected: Status) -> Result<(), Error> {
        for _ in 0..self.poll_limit {
            // STAT=F8 alone never means idle: it also appears between active states.
            if self.regs.cr().read().si() {
                let status = self.regs.stat().read().stat();
                if status == expected {
                    return Ok(());
                }
                return Err(match status {
                    Status::WriteAddressNack | Status::ReadAddressNack => Error::AddressNack,
                    Status::WriteDataNack => Error::DataNack,
                    Status::ArbitrationLost
                    | Status::ArbitrationLostSlaveWrite
                    | Status::ArbitrationLostGeneralCall
                    | Status::ArbitrationLostSlaveRead => {
                        self.master = false;
                        Error::Arbitration
                    }
                    _ => {
                        // Bus-error/slave states release mastership in hardware.
                        // Retain conservative ownership handling for undocumented
                        // upper status encodings too; do not normalize reserved states.
                        if status == Status::BusError
                            || status.to_bits() >= Status::SlaveWriteAddress.to_bits()
                        {
                            self.master = false;
                        }
                        Error::Bus
                    }
                });
            }
            core::hint::spin_loop();
        }
        Err(Error::Timeout)
    }

    fn stop(&mut self) -> Result<(), Error> {
        let mut control = self.base;
        control.set_sto(true);
        self.regs.cr().write_value(control); // STA=0, AA=0, SI=0 in one write.
        for _ in 0..self.poll_limit {
            let control = self.regs.cr().read();
            if !control.sto() && !control.si() {
                self.master = false;
                return Ok(());
            }
            if control.si()
                && matches!(
                    self.regs.stat().read().stat(),
                    Status::ArbitrationLost
                        | Status::ArbitrationLostSlaveWrite
                        | Status::ArbitrationLostGeneralCall
                        | Status::ArbitrationLostSlaveRead
                )
            {
                self.master = false;
                return Err(Error::Arbitration);
            }
            core::hint::spin_loop();
        }
        Err(Error::Timeout)
    }

    fn abort(&mut self, error: Error) {
        if error == Error::Arbitration {
            // Hardware handed the bus to another master. Never generate STOP or
            // a new START, and do not ACK if subsequently addressed as a slave.
            self.regs.cr().write_value(self.base);
            self.master = false;
            return;
        }
        // §20.4.10: STO clears state 00 without putting a STOP on the wires.
        // A timeout after a confirmed START gets a bounded attempt to finish STOP.
        if self.master
            || (self.regs.cr().read().si() && self.regs.stat().read().stat() == Status::BusError)
        {
            if self.stop().is_ok() {
                return;
            }
        }
        // §20.4.10 fallback: EN=0, EN=1, SI=0. No retransmission or GPIO pulses.
        // This releases the controller but cannot force a slave to release lines.
        let mut control = self.base;
        control.set_en(false);
        self.regs.cr().write_value(control);
        control = self.base;
        control.set_si(true);
        self.regs.cr().write_value(control); // Preserve SI while re-enabling.
        self.regs.cr().write_value(self.base);
        self.master = false;
    }
}

/// Type-erased owned I2C peripheral. Constructed only from a verified instance.
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
    fn mask(&self) {
        critical_section::with(|_| {
            self.state.armed.store(false, Ordering::Release);
            (self.disable_irq)();
        });
    }

    fn constructor_fence(&self) -> Result<(), ConfigError> {
        critical_section::with(|_| {
            self.state.armed.store(false, Ordering::Release);
            (self.disable_irq)();
            if self.state.faulted.load(Ordering::Acquire) {
                Err(ConfigError::Faulted)
            } else {
                Ok(())
            }
        })
    }

    fn latch_fault(&self) -> ConfigError {
        critical_section::with(|_| {
            self.state.armed.store(false, Ordering::Release);
            (self.disable_irq)();
            self.state.faulted.store(true, Ordering::Release);
        });
        ConfigError::Faulted
    }

    fn initialize(&self, timing: Timing, attempts: u32) -> Result<(), ConfigError> {
        if !self.rcc.is_enabled() {
            critical_section::with(|cs| self.rcc.enable_with_cs_readback(cs, Readback::None))
                .map_err(|_| self.latch_fault())?;
            let mut enabled = false;
            for _ in 0..attempts {
                if self.rcc.is_enabled() {
                    enabled = true;
                    break;
                }
                core::hint::spin_loop();
            }
            if !enabled {
                return Err(self.latch_fault());
            }
        }
        // Ownership of inherited hardware is unknown. Never emit STOP here.
        if !self.disable_checked(timing.control(), attempts) {
            return Err(self.latch_fault());
        }
        self.reset_and_configure(timing, attempts)
    }

    fn disable_checked(&self, base: pac::i2c::regs::Cr, attempts: u32) -> bool {
        let mut disabled = base;
        disabled.set_en(false);
        disabled.set_sta(false);
        disabled.set_aa(false);
        disabled.set_si(true); // W1 preserves any current event; it does not set SI.
        critical_section::with(|_| {
            self.regs.cr().write_value(disabled);
            self.regs.brren().write(|v| v.set_en(false));
        });
        for _ in 0..attempts {
            // One combined sample: CR once, BRREN once. STO=0 is separately
            // mandatory: writing STO=0 never cancels a previously pending STOP.
            let retired = critical_section::with(|_| {
                let control = self.regs.cr().read();
                let divider = self.regs.brren().read();
                stable_control(control, disabled) && !control.sto() && !divider.en()
            });
            if retired {
                return true;
            }
            core::hint::spin_loop();
        }
        false
    }

    fn reset_and_configure(&self, timing: Timing, attempts: u32) -> Result<(), ConfigError> {
        // The exclusive metadata proof and confirmed local disable/STO=0 are
        // prerequisites. Failed reset acknowledgement forbids all later I2C MMIO.
        self.rcc
            .reset_checked(attempts)
            .map_err(|_| self.latch_fault())?;
        let mut reset = false;
        for _ in 0..attempts {
            let ready = critical_section::with(|_| {
                let control = self.regs.cr().read();
                let divider_enabled = self.regs.brren().read().en();
                let divider = self.regs.brr().read().brr();
                stable_control(control, Default::default())
                    && !control.sto()
                    && !divider_enabled
                    && divider == 0
            });
            if ready {
                reset = true;
                break;
            }
            core::hint::spin_loop();
        }
        if !reset {
            return Err(self.latch_fault());
        }
        critical_section::with(|_| configure(self.regs, timing));
        for _ in 0..attempts {
            let configured = critical_section::with(|_| {
                let control = self.regs.cr().read();
                let divider_enabled = self.regs.brren().read().en();
                let divider = self.regs.brr().read().brr();
                let gate_enabled = self.rcc.is_enabled();
                let reset_asserted = self.rcc.reset_asserted();
                stable_control(control, timing.control())
                    && !control.sto()
                    && divider_enabled
                    && divider == timing.brr
                    && gate_enabled
                    && !reset_asserted
            });
            if configured {
                // Dedicated pending state may be cleared only after retirement
                // and successful reset/configuration, with the IRQ still masked.
                critical_section::with(|_| (self.unpend_irq)());
                return Ok(());
            }
            core::hint::spin_loop();
        }
        Err(self.latch_fault())
    }
}

// SI is asynchronous and deliberately excluded; STO is checked separately.
// These typed fields describe masks 0x65 / 0x3f65, without raw-word writes.
fn stable_control(control: pac::i2c::regs::Cr, expected: pac::i2c::regs::Cr) -> bool {
    let stable = control.en() == expected.en()
        && control.sta() == expected.sta()
        && control.aa() == expected.aa()
        && control.flt() == expected.flt();
    #[cfg(any(i2c_cw32l010_v1, i2c_cw32l011_v1))]
    let stable = stable && control.sclinsrc() == 0 && control.sdainsrc() == 0;
    stable
}

/// Static state contains no driver, future, pin or caller-buffer address.
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

/// Dedicated GLOBAL handler: retain SI, mask the vector, then wake the task.
/// No controller command, DR access or caller-buffer access occurs in the ISR.
pub struct InterruptHandler<T: Instance>(PhantomData<T>);
impl<T: Instance> Handler<T::Interrupt> for InterruptHandler<T> {
    unsafe fn on_interrupt() {
        let info = T::info();
        let wake = critical_section::with(|_| {
            if !info.state.armed.load(Ordering::Acquire) {
                (info.disable_irq)();
                return false; // Inactive handlers never read a gated peripheral.
            }
            if !info.regs.cr().read().si() {
                return false;
            }
            info.state.armed.store(false, Ordering::Release);
            (info.disable_irq)();
            true
        });
        if wake {
            info.state.waker.wake();
        }
    }
}

pub(crate) fn enable_interrupt<I: Interrupt>() {
    unsafe {
        I::enable();
    }
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
/// Verified I2C peripheral instance.
#[allow(private_bounds)]
pub trait Instance: sealed::Instance {
    /// Dedicated GLOBAL vector resolved from this instance's metadata.
    type Interrupt: Interrupt;
}
/// Verified, package-bonded SCL alternate-function route.
#[allow(private_bounds)]
pub trait SclPin<T: Instance>: Pin + sealed::SclPin<T> {}
/// Verified, package-bonded SDA alternate-function route.
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

impl embedded_hal::i2c::I2c<embedded_hal::i2c::TenBitAddress> for I2c<'_, Blocking> {
    fn transaction(&mut self, address: u16, operations: &mut [Operation<'_>]) -> Result<(), Error> {
        self.blocking_transaction_with_address(Address::TenBit(address), operations)
    }
}
impl embedded_hal_async::i2c::I2c<embedded_hal::i2c::TenBitAddress> for I2c<'_, Async> {
    async fn transaction(&mut self, address: u16, operations: &mut [Operation<'_>]) -> Result<(), Error> {
        self.transaction_with_address(Address::TenBit(address), operations).await
    }
}
