//! L012 direct-loading DAC, with exclusive peripheral and external pin ownership.
//!
//! VDDA is the reference and must equal VDD. The supported supply is 1.7..5.5 V.
//! The DAC is unbuffered: the datasheet gives a 5 kΩ minimum resistive load and
//! 50 pF maximum capacitive load under its 3.3 V test conditions. Full-scale output
//! is not guaranteed to reach VDDA. Code readback is digital, not a measurement.
//!
//! No analog ready flag or guaranteed maximum settling time is documented. The
//! 3 µs startup figure is typical only. The application must qualify startup and
//! every output change for its board, load and temperature before relying on it.
//! See `docs/dac-opa.md` for exact limits and deferred functionality.
//!
//! This driver owns the whole DAC, disabling both channels before configuring it.
//! It never resets/gates the analog block, changes ADC/VC, or disables BGR. The
//! OPA gate is retained to safely inspect competing output drivers before use.

use crate::gpio::{Flex, Pin};
use crate::rcc::{RccPeripheral, SealedRccPeripheral};
use crate::{Peri, PeripheralType, pac, peripherals};

/// Board-declared VDDA; this is not a measurement.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct Config {
    /// VDDA in mV. Must remain in [`supply_voltage_range_mv`] and equal VDD.
    pub supply_mv: u16,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            supply_mv: crate::DAC_SUPPLY_RANGE_MV.0,
        }
    }
}
/// The source-qualified supply/reference operating range, in mV.
pub const fn supply_voltage_range_mv() -> (u16, u16) {
    crate::DAC_SUPPLY_RANGE_MV
}
/// Maximum accepted right-aligned 12-bit digital code.
pub const MAX_CODE: u16 = crate::DAC_MAX_CODE;

/// Physical output channel.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Channel {
    /// PB0 output.
    Ch1,
    /// PB1 output.
    Ch2,
}
/// Digital sample. Twelve-bit input is checked, never silently truncated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Value {
    /// Right-aligned eight-bit sample.
    Bit8(u8),
    /// Right-aligned twelve-bit sample.
    Bit12(u16),
}
/// Configuration or write failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {
    /// Board declaration outside the documented operating envelope.
    InvalidSupplyVoltage,
    /// Clock enable did not acknowledge.
    Clock,
    /// A required analog reset is asserted; it is never released implicitly.
    HeldInReset,
    /// A matching OPA is enabled and might drive the same pad.
    OutputBusy,
    /// This output was not enabled by the chosen constructor.
    ChannelDisabled,
    /// A twelve-bit sample exceeded [`MAX_CODE`].
    ValueOutOfRange,
}

/// Whole-DAC owner with one or two owned output pins.
pub struct Dac<'d, T: Instance> {
    _peri: Peri<'d, T>,
    _ch1: Option<Flex<'d>>,
    _ch2: Option<Flex<'d>>,
}
impl<'d, T: Instance> Dac<'d, T> {
    /// Enable both outputs at zero code. Return does not imply analog settling.
    pub fn new<P1: OutputPin<T, 1>, P2: OutputPin<T, 2>>(
        peri: Peri<'d, T>,
        ch1: Peri<'d, P1>,
        ch2: Peri<'d, P2>,
        config: Config,
    ) -> Result<Self, Error> {
        Self::prepare(config, true, true)?;
        let mut ch1 = Flex::new(ch1);
        let mut ch2 = Flex::new(ch2);
        ch1.set_as_analog();
        ch2.set_as_analog();
        Self::start(true, true);
        Ok(Self {
            _peri: peri,
            _ch1: Some(ch1),
            _ch2: Some(ch2),
        })
    }
    /// Enable only channel 1 at zero code. Channel 2 remains disabled.
    pub fn new_ch1<P: OutputPin<T, 1>>(
        peri: Peri<'d, T>,
        pin: Peri<'d, P>,
        config: Config,
    ) -> Result<Self, Error> {
        Self::prepare(config, true, false)?;
        let mut pin = Flex::new(pin);
        pin.set_as_analog();
        Self::start(true, false);
        Ok(Self {
            _peri: peri,
            _ch1: Some(pin),
            _ch2: None,
        })
    }
    /// Enable only channel 2 at zero code. Channel 1 remains disabled.
    pub fn new_ch2<P: OutputPin<T, 2>>(
        peri: Peri<'d, T>,
        pin: Peri<'d, P>,
        config: Config,
    ) -> Result<Self, Error> {
        Self::prepare(config, false, true)?;
        let mut pin = Flex::new(pin);
        pin.set_as_analog();
        Self::start(false, true);
        Ok(Self {
            _peri: peri,
            _ch1: None,
            _ch2: Some(pin),
        })
    }
    fn prepare(config: Config, ch1: bool, ch2: bool) -> Result<(), Error> {
        let (min, max) = supply_voltage_range_mv();
        if !(min..=max).contains(&config.supply_mv) {
            return Err(Error::InvalidSupplyVoltage);
        }
        critical_section::with(|cs| {
            T::RCC_INFO.enable_with_cs(cs).map_err(|_| Error::Clock)?;
            // OPA1 and OPA2 have the same actual gate. Never reset that group.
            peripherals::OPA1::RCC_INFO
                .enable_with_cs(cs)
                .map_err(|_| Error::Clock)?;
            if T::RCC_INFO.reset_asserted() || peripherals::OPA1::RCC_INFO.reset_asserted() {
                return Err(Error::HeldInReset);
            }
            if (ch1 && pac::OPA1.cr().read().en()) || (ch2 && pac::OPA2.cr().read().en()) {
                return Err(Error::OutputBusy);
            }
            Ok(())
        })?;
        // Whole-DAC ownership permits quiescing both channels. No status/command writes.
        T::regs().cr0().write(|_| {});
        T::regs().cr1().write(|_| {});
        T::regs().dhr12rd().write(|w| {
            w.set_c1data(0);
            w.set_c2data(0);
        });
        Ok(())
    }
    fn start(ch1: bool, ch2: bool) {
        T::regs().cr0().write(|w| {
            w.set_wave1(pac::dac::vals::Wave::Disabled);
            w.set_wave2(pac::dac::vals::Wave::Disabled);
            w.set_ten1(false);
            w.set_ten2(false);
            w.set_en1(ch1);
            w.set_en2(ch2);
        });
        T::regs().cr1().write(|w| {
            w.set_c1out(ch1);
            w.set_c2out(ch2);
        });
    }
    /// Load a sample directly; hardware transfers DHR to DOR after one dac_pclk.
    /// This is not an analog-settling wait and performs no trigger/DMA operation.
    pub fn set(&mut self, channel: Channel, value: Value) -> Result<(), Error> {
        if matches!(value,Value::Bit12(v) if v>MAX_CODE) {
            return Err(Error::ValueOutOfRange);
        }
        let out = T::regs().cr1().read();
        let en = T::regs().cr0().read();
        if !match channel {
            Channel::Ch1 => out.c1out() && en.en1(),
            Channel::Ch2 => out.c2out() && en.en2(),
        } {
            return Err(Error::ChannelDisabled);
        }
        match (channel, value) {
            (Channel::Ch1, Value::Bit8(v)) => T::regs().dhr8r1().write(|w| w.set_data(v)),
            (Channel::Ch2, Value::Bit8(v)) => T::regs().dhr8r2().write(|w| w.set_data(v)),
            (Channel::Ch1, Value::Bit12(v)) => T::regs().dhr12r1().write(|w| w.set_data(v)),
            (Channel::Ch2, Value::Bit12(v)) => T::regs().dhr12r2().write(|w| w.set_data(v)),
        }
        Ok(())
    }
    /// Read the 12-bit digital output register. It may lag a just-written sample;
    /// it does not qualify analog voltage, calibration, startup or settling.
    pub fn read(&self, channel: Channel) -> u16 {
        match channel {
            Channel::Ch1 => T::regs().dor1().read().data(),
            Channel::Ch2 => T::regs().dor2().read().data(),
        }
    }
}
impl<T: Instance> Drop for Dac<'_, T> {
    fn drop(&mut self) {
        T::regs().cr1().modify(|w| {
            w.set_c1out(false);
            w.set_c2out(false);
        });
        T::regs().cr0().modify(|w| {
            w.set_en1(false);
            w.set_en2(false);
        });
    }
}
pub(crate) trait SealedInstance {
    fn regs() -> pac::dac::Dac;
}
/// DAC instance generated from the selected chip's metadata.
#[allow(private_bounds)]
pub trait Instance: PeripheralType + RccPeripheral + SealedInstance + 'static {}
pub(crate) trait SealedOutputPin<T, const C: u8> {}
/// Package-qualified DAC channel output, retaining physical pin exclusivity.
#[allow(private_bounds)]
pub trait OutputPin<T, const C: u8>: Pin + SealedOutputPin<T, C> {}
macro_rules! impl_instance {
    ($p:ident) => {
        impl $crate::dac::SealedInstance for $crate::peripherals::$p {
            fn regs() -> $crate::pac::dac::Dac {
                $crate::pac::$p
            }
        }
        impl $crate::dac::Instance for $crate::peripherals::$p {}
    };
}
pub(crate) use impl_instance;
macro_rules! impl_pin {
    ($p:ident,$pin:ident,$ch:literal) => {
        impl $crate::dac::SealedOutputPin<$crate::peripherals::$p, $ch>
            for $crate::peripherals::$pin
        {
        }
        impl $crate::dac::OutputPin<$crate::peripherals::$p, $ch> for $crate::peripherals::$pin {}
    };
}
pub(crate) use impl_pin;
