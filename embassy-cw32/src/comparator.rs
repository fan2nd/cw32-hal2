//! Voltage comparators with Embassy peripheral, pin and reference ownership.
//!
//! Each instance owns its analog pins, and borrows an internal divider when used.
//! Only its CR0/CR1 (and CR2 on
//! L012) are configured. Divider/reference registers, ADC configuration, shared
//! resets and sibling comparators are never written. The shared VC clock gate is
//! enabled and left enabled, including on drop. Hardware may automatically enable
//! a shared bandgap when a comparator is enabled; this driver never disables it.
//!
//! `new` enables the comparator without claiming its analog startup is complete.
//! Classic IP exposes a hardware READY flag; L010/L011/L012 do not. Their 0.5 us
//! startup specification is typical only, so no invented fixed delay is used.
//! Each output observation includes its readiness status. Even after startup,
//! input changes require the selected response time and adequate overdrive.
//!
//! The board must meet its own datasheet's analog-supply, input/common-mode,
//! temperature and offset limits. Inputs must stay between analog ground and the
//! analog supply (VDD on F002/F003/L010, VDDA on the other families). The analog
//! supply range is 1.65..5.5 V for classic non-radio families, 2.2..3.6 V on R031,
//! 1.8..3.6 V on W031 (at least 2.0 V in RF DCDC mode), 1.62..5.5 V on L010 and
//! 1.7..5.5 V on L011/L012. These are board requirements, not measurements.
//! Hysteresis values are typical, and electrical response limits are characterized
//! rather than production-tested. Consult `docs/comparator.md` and its evidence.
//!
//! This polling subset disables filtering, window mode, interrupts and timer
//! blanking/routing. On L010/L011/L012, `new_with_vref` accepts a generated,
//! bank-qualified immutable reference borrow. Fixed BGR references, DAC input,
//! output-pin routing and asynchronous operation remain outside this subset.

use crate::gpio::{Flex, Level, Pin};
use crate::pac::vc::vals;
use crate::rcc::RccPeripheral;
use crate::{Peri, PeripheralType, pac};

/// Analog response-speed/power setting; no exact propagation time is promised.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ResponseSpeed {
    /// Lowest power and slowest response on classic comparators.
    #[cfg(not(vc_two_speed))]
    UltraLow,
    /// Low-speed/low-power response.
    Low,
    /// Medium-speed response on classic comparators.
    #[cfg(not(vc_two_speed))]
    Medium,
    /// Highest speed and power.
    High,
}

/// Positive-feedback hysteresis; voltage values are typical, not exact thresholds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Hysteresis {
    /// Disable hysteresis.
    None,
    /// Approximately 10 mV on classic IP.
    #[cfg(not(vc_two_speed))]
    Low,
    /// Approximately 20 mV on classic IP.
    #[cfg(not(vc_two_speed))]
    Medium,
    /// Approximately 30 mV on classic IP.
    #[cfg(not(vc_two_speed))]
    High,
    /// Enable the fixed hysteresis of L010/L011/L012; see the own datasheet.
    #[cfg(vc_two_speed)]
    Enabled,
}

/// Output polarity after the analog comparison.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum OutputPolarity {
    /// Positive input above negative input produces a high output.
    NotInverted,
    /// Invert the comparison result.
    Inverted,
}

/// Comparator configuration for external inputs and unfiltered polling.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct Config {
    /// Board-declared analog supply in mV, not a measurement.
    ///
    /// Must remain within [`supply_voltage_range_mv`]. Inputs must independently
    /// stay between analog ground and that rail; VDDA must equal VDD where used.
    /// W031 RF DCDC operation separately requires at least 2000 mV.
    pub supply_mv: u16,
    /// Analog speed/power trade-off.
    pub response_speed: ResponseSpeed,
    /// Input hysteresis setting.
    pub hysteresis: Hysteresis,
    /// Comparison output polarity.
    pub output_polarity: OutputPolarity,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            supply_mv: crate::COMPARATOR_SUPPLY_RANGE_MV.0,
            response_speed: ResponseSpeed::High,
            hysteresis: Hysteresis::None,
            output_polarity: OutputPolarity::NotInverted,
        }
    }
}

/// Selected family's comparator analog-supply operating range, in mV.
///
/// This does not replace package temperature, input voltage, or whole-device
/// constraints. It is sourced independently from the family's own datasheet.
pub const fn supply_voltage_range_mv() -> (u16, u16) {
    crate::COMPARATOR_SUPPLY_RANGE_MV
}

/// Whether comparator inputs are referred to VDDA rather than VDD.
/// Where VDDA exists, the own-family general conditions require VDDA = VDD.
pub const fn input_uses_vdda() -> bool {
    crate::COMPARATOR_INPUT_USES_VDDA
}

/// Invalid board declaration or unavailable shared peripheral clock/reset.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {
    /// Declared supply is outside the own-family operating range.
    InvalidSupplyVoltage,
    /// Clock gate did not acknowledge enabling.
    Clock,
    /// The shared VC reset is asserted; this driver will not release it.
    HeldInReset,
    /// The comparator and its shared reference declare different analog supplies.
    #[cfg(vref)]
    ReferenceSupplyMismatch,
}

/// Startup status of the same hardware observation as the output level.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Readiness {
    /// Classic hardware reports its comparator is stable.
    Ready,
    /// Classic hardware has not asserted READY. Do not use the level yet.
    Starting,
    /// This IP has no readiness flag or source-backed maximum startup delay.
    /// The board/application must establish sufficient settling independently.
    Unknown,
}

/// Observed post-polarity comparator output and startup qualification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Output {
    /// The instantaneous hardware FLTV value. It is unqualified while starting,
    /// when readiness is unknown, or while the analog input is still settling.
    pub level: Level,
    /// Analog startup information supplied by this peripheral version.
    pub readiness: Readiness,
}

/// Polling comparator retaining exclusive ownership of the instance and inputs.
pub struct Comparator<'d, T: Instance> {
    _peri: Peri<'d, T>,
    _positive: Flex<'d>,
    _negative: Option<Flex<'d>>,
    #[cfg(vref)]
    _reference: Option<&'d crate::vref::Config>,
}
impl<'d, T: Instance> Comparator<'d, T> {
    /// Configure external inputs and enable this comparator.
    ///
    /// Startup is deliberately not treated as complete on return. Check the
    /// returned [`Output::readiness`] before relying on a level. For low-family
    /// IP, the application must qualify its own startup settling interval.
    /// Pins are disconnected from digital input/output and weak pulls. Shared
    /// references and sibling registers are left alone; no peripheral reset runs.
    /// Invalid supply is rejected before any register or pin write. A held shared
    /// reset returns an error after enabling the gate, without releasing reset.
    pub fn new<P: NonInvertingPin<T>, N: InvertingPin<T>>(
        peri: Peri<'d, T>,
        positive: Peri<'d, P>,
        negative: Peri<'d, N>,
        config: Config,
    ) -> Result<Self, Error> {
        enable_clock::<T>(&config)?;
        let r = T::regs();
        // Quiesce only this instance before changing its input selection.
        r.cr0().write(|_| {});
        let mut positive = Flex::new(positive);
        let mut negative = Flex::new(negative);
        positive.set_as_analog();
        negative.set_as_analog();
        configure::<T>(P::INP_MUX, N::INN_MUX, &config);
        Ok(Self {
            _peri: peri,
            _positive: positive,
            _negative: Some(negative),
            #[cfg(vref)]
            _reference: None,
        })
    }

    /// Compare an external positive input with an immutable shared divider.
    ///
    /// The reference bank must have a generated route to this comparator. This
    /// driver retains its borrow, so the bank cannot be reconfigured through safe
    /// code while in use. Other comparators can borrow the same unchanged bank.
    /// The board supply declaration must match the reference's declaration.
    /// Startup and subsequent analog settling remain unqualified on return.
    #[cfg(vref)]
    pub fn new_with_vref<P: NonInvertingPin<T>, R: crate::vref::Reference<T>>(
        peri: Peri<'d, T>,
        positive: Peri<'d, P>,
        reference: &'d crate::vref::Vref<'_, R>,
        config: Config,
    ) -> Result<Self, Error> {
        if config.supply_mv != reference.config.supply_mv {
            return Err(Error::ReferenceSupplyMismatch);
        }
        enable_clock::<T>(&config)?;
        T::regs().cr0().write(|_| {});
        let mut positive = Flex::new(positive);
        positive.set_as_analog();
        configure::<T>(P::INP_MUX, R::MUX, &config);
        Ok(Self {
            _peri: peri,
            _positive: positive,
            _negative: None,
            _reference: Some(&reference.config),
        })
    }

    /// Read the unfiltered, post-polarity output and hardware startup status.
    ///
    /// This operation never waits. It is a single SR read and has no side effects.
    /// Readiness does not indicate that a recently changed input has settled.
    pub fn output(&self) -> Output {
        let status = T::regs().sr().read();
        #[cfg(vc_ready)]
        let readiness = if status.ready() {
            Readiness::Ready
        } else {
            Readiness::Starting
        };
        #[cfg(not(vc_ready))]
        let readiness = Readiness::Unknown;
        Output {
            level: status.fltv().into(),
            readiness,
        }
    }
}
impl<T: Instance> Drop for Comparator<'_, T> {
    fn drop(&mut self) {
        // Never reset or gate the shared block, nor disable any shared bandgap.
        T::regs().cr0().modify(|w| w.set_en(false));
    }
}

fn enable_clock<T: Instance>(config: &Config) -> Result<(), Error> {
    let (min, max) = supply_voltage_range_mv();
    if !(min..=max).contains(&config.supply_mv) {
        return Err(Error::InvalidSupplyVoltage);
    }
    critical_section::with(|cs| {
        T::RCC_INFO.enable_with_cs(cs).map_err(|_| Error::Clock)?;
        if !T::RCC_INFO.is_enabled() {
            return Err(Error::Clock);
        }
        if T::RCC_INFO.reset_asserted() {
            return Err(Error::HeldInReset);
        }
        Ok(())
    })?;
    Ok(())
}

fn configure<T: Instance>(positive_mux: u8, negative_mux: u8, config: &Config) {
    let r = T::regs();
    // PCLK selects the running peripheral bus, avoiding an implicit LSI
    // request on low-family IP even when filtering is bypassed.
    r.cr1().write(|w| w.set_fltclk(vals::FilterClock::Pclk));
    #[cfg(vc_cr2)]
    r.cr2().write(|_| {});
    r.cr0().write(|w| {
        w.set_inp(positive_mux);
        w.set_inn(negative_mux);
        w.set_resp(match config.response_speed {
            #[cfg(not(vc_two_speed))]
            ResponseSpeed::UltraLow => vals::ResponseSpeed::UltraLow,
            ResponseSpeed::Low => vals::ResponseSpeed::Low,
            #[cfg(not(vc_two_speed))]
            ResponseSpeed::Medium => vals::ResponseSpeed::Medium,
            ResponseSpeed::High => vals::ResponseSpeed::High,
        });
        w.set_hys(match config.hysteresis {
            Hysteresis::None => vals::Hysteresis::None,
            #[cfg(not(vc_two_speed))]
            Hysteresis::Low => vals::Hysteresis::Low,
            #[cfg(not(vc_two_speed))]
            Hysteresis::Medium => vals::Hysteresis::Medium,
            #[cfg(not(vc_two_speed))]
            Hysteresis::High => vals::Hysteresis::High,
            #[cfg(vc_two_speed)]
            Hysteresis::Enabled => vals::Hysteresis::Enabled,
        });
        w.set_pol(match config.output_polarity {
            OutputPolarity::NotInverted => vals::Polarity::Normal,
            OutputPolarity::Inverted => vals::Polarity::Inverted,
        });
        w.set_en(true);
    });
}

pub(crate) trait SealedInstance {
    fn regs() -> pac::vc::Vc;
}
/// Comparator instance generated from the selected chip's verified metadata.
#[allow(private_bounds)]
pub trait Instance: PeripheralType + RccPeripheral + SealedInstance + 'static {}
pub(crate) trait SealedNonInvertingPin<T> {
    const INP_MUX: u8;
}
pub(crate) trait SealedInvertingPin<T> {
    const INN_MUX: u8;
}
/// Package-qualified external positive input to this comparator.
#[allow(private_bounds)]
pub trait NonInvertingPin<T>: Pin + SealedNonInvertingPin<T> {}
/// Package-qualified external negative input to this comparator.
#[allow(private_bounds)]
pub trait InvertingPin<T>: Pin + SealedInvertingPin<T> {}

macro_rules! impl_instance {
    ($instance:ident) => {
        impl $crate::comparator::SealedInstance for $crate::peripherals::$instance {
            fn regs() -> $crate::pac::vc::Vc {
                $crate::pac::$instance
            }
        }
        impl $crate::comparator::Instance for $crate::peripherals::$instance {}
    };
}
pub(crate) use impl_instance;
macro_rules! impl_pin {
    ($instance:ident, $pin:ident, INP, $mux:literal) => {
        impl $crate::comparator::SealedNonInvertingPin<$crate::peripherals::$instance>
            for $crate::peripherals::$pin
        {
            const INP_MUX: u8 = $mux;
        }
        impl $crate::comparator::NonInvertingPin<$crate::peripherals::$instance>
            for $crate::peripherals::$pin
        {
        }
    };
    ($instance:ident, $pin:ident, INN, $mux:literal) => {
        impl $crate::comparator::SealedInvertingPin<$crate::peripherals::$instance>
            for $crate::peripherals::$pin
        {
            const INN_MUX: u8 = $mux;
        }
        impl $crate::comparator::InvertingPin<$crate::peripherals::$instance>
            for $crate::peripherals::$pin
        {
        }
    };
}
pub(crate) use impl_pin;
