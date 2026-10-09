//! Blocking I2C master for verified CW32 state-code controller backends.
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
//! Seven-bit addressing, repeated START, adjacent operation merging, clock
//! stretching and arbitration-loss reporting are supported. All waits are
//! bounded by a configurable **poll count**, not a wall-clock duration. A timeout
//! releases the controller, but cannot recover a slave holding a line low.
//! Transactions are never automatically retried: earlier bytes may have reached
//! the device. Multi-master scheduling, slave mode, ten-bit addressing, DMA and
//! async/cancellation are deliberately not exposed. There is no silicon-revision
//! detection or errata-specific workaround, and no on-hardware validation claim.
use core::marker::PhantomData;

use embedded_hal::i2c::NoAcknowledgeSource;
pub use embedded_hal::i2c::Operation;

use crate::gpio::{Flex, Pin, Pull};
use crate::mode::{Blocking, Mode};
use crate::rcc::ClockBounds;
use crate::time::Hertz;
use crate::{Peri, PeripheralType, pac};

use pac::i2c::vals::Status;

const MAX_FREQUENCY: u32 = crate::I2C_MAXIMUM_FREQUENCY_HZ;

/// Checked I2C configuration failure. Invalid configuration does not touch MMIO.
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
    /// Address is not an unshifted seven-bit value (0..=127).
    InvalidAddress,
    /// Empty reads are rejected before any bus activity.
    ZeroLengthTransfer,
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
            Self::InvalidAddress => "I2C address exceeds seven bits",
            Self::ZeroLengthTransfer => "I2C empty read is unsupported",
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
    /// Maximum register/line samples in each wait (including STOP/error cleanup).
    ///
    /// This is not a duration. CPU frequency, optimization and interrupt latency
    /// affect elapsed time. Increase it for slow clock-stretching devices. A long
    /// transaction may use this bound for each byte, plus START/address/STOP.
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

/// Owned I2C master. Only the [`Blocking`] mode has a constructor and bus methods.
///
/// Drop disables the peripheral and its clock and disconnects both pins. Every
/// successful transfer has already completed STOP before returning. There is no
/// future that can be cancelled while the peripheral accesses a borrowed buffer.
pub struct I2c<'d, M: Mode> {
    peripheral: Peri<'d, AnyI2c>,
    scl: Flex<'d>,
    sda: Flex<'d>,
    scl_af: u8,
    sda_af: u8,
    engine: Engine,
    kernel_clock: ClockBounds,
    config: Config,
    actual_frequency: Hertz,
    frequency_bounds: ClockBounds,
    _mode: PhantomData<M>,
}
impl<'d> I2c<'d, Blocking> {
    /// Construct a blocking master, panicking on invalid clock/configuration.
    pub fn new_blocking<T: Instance>(
        peripheral: Peri<'d, T>,
        scl: Peri<'d, impl SclPin<T>>,
        sda: Peri<'d, impl SdaPin<T>>,
        config: Config,
    ) -> Self {
        Self::try_new_blocking(peripheral, scl, sda, config)
            .unwrap_or_else(|error| panic!("invalid I2C configuration: {}", error))
    }

    /// Construct a blocking master; invalid configuration is rejected before MMIO.
    ///
    /// Tokens are consumed on error too; pass `reborrow()` when needed afterward.
    /// This does not enable the NVIC I2C interrupt. Do not independently unmask it.
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
        let mut scl = Flex::new(scl);
        let mut sda = Flex::new(sda);
        critical_section::with(|cs| info.rcc.enable_and_reset_with_cs(cs))
            .expect("I2C clock gate did not enable");
        configure(info.regs, timing);
        // CR.AA remains clear outside reads so this master does not ACK as a slave.
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
            kernel_clock,
            config,
            actual_frequency: timing.actual_frequency,
            frequency_bounds: kernel_clock.divided_by(8 * (u32::from(timing.brr) + 1)),
            _mode: PhantomData,
        })
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
        self.engine.transaction(address, operations, || {
            self.scl.is_high() && self.sda.is_high()
        })
    }
}
impl<M: Mode> I2c<'_, M> {
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
    /// Apply configuration only if the controller and sampled bus lines are idle.
    pub fn set_config(&mut self, config: &Config) -> Result<(), ConfigError> {
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
        self.scl
            .set_as_af_open_drain(self.scl_af, pull(config.scl_pullup));
        self.sda
            .set_as_af_open_drain(self.sda_af, pull(config.sda_pullup));
        self.engine.base = timing.control();
        self.engine.poll_limit = config.poll_limit;
        self.config = *config;
        self.actual_frequency = timing.actual_frequency;
        self.frequency_bounds = self
            .kernel_clock
            .divided_by(8 * (u32::from(timing.brr) + 1));
        Ok(())
    }
}
impl<M: Mode> Drop for I2c<'_, M> {
    fn drop(&mut self) {
        self.engine.regs.cr().write_value(Default::default());
        self.engine.regs.brren().write(|v| v.set_en(false));
        critical_section::with(|cs| self.peripheral.info.rcc.disable_with_cs(cs))
            .expect("I2C clock gate did not disable");
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
        address: u8,
        operations: &mut [Operation<'_>],
        mut lines_high: impl FnMut() -> bool,
    ) -> Result<(), Error> {
        // Preflight every operation: invalid input must not partially write a device.
        if address > 0x7f {
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

    fn run(&mut self, address: u8, operations: &mut [Operation<'_>]) -> Result<(), Error> {
        let mut previous = None;
        let count = operations.len();
        for index in 0..count {
            let read = matches!(operations[index], Operation::Read(_));
            if previous != Some(read) {
                let mut control = self.base;
                control.set_sta(true);
                self.regs.cr().write_value(control); // SI=0 advances START/reSTART.
                self.expect(if previous.is_none() {
                    Status::Start
                } else {
                    Status::RepeatedStart
                })?;
                self.master = true;
                self.regs
                    .dr()
                    .write(|v| v.set_dr((address << 1) | u8::from(read)));
                self.regs.cr().write_value(self.base); // Clear STA and SI after DR write.
                self.expect(if read {
                    Status::ReadAddressAck
                } else {
                    Status::WriteAddressAck
                })?;
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
/// Verified I2C peripheral instance.
#[allow(private_bounds)]
pub trait Instance: sealed::Instance {}
/// Verified, package-bonded SCL alternate-function route.
#[allow(private_bounds)]
pub trait SclPin<T: Instance>: Pin + sealed::SclPin<T> {}
/// Verified, package-bonded SDA alternate-function route.
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
