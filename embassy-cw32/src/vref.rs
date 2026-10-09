//! Shared resistor reference for low-family voltage comparators.
//!
//! A `Vref` exclusively owns one divider bank. Its source and tap are immutable;
//! compatible comparators borrow it through `Comparator::new_with_vref`. L010
//! and L011 share one bank between VC1/VC2; L012 has separate VC1/VC2 and VC3/VC4
//! banks. Generated route traits reject a reference from the wrong bank.
//!
//! Construction refuses an enabled comparator already selecting this bank,
//! including a forgotten driver. Drop leaves the reference and shared VC clock
//! enabled. It never disables BGR, changes ADC state, or resets a comparator.
//! This deliberately trades retained analog power for safe sibling coexistence.
//!
//! The divider has no readiness flag or qualified maximum settling interval.
//! Board code must establish settling and meet the comparator input/supply
//! limits. Vcore is nominally 1.6 V, not an exact or calibrated reference.

use crate::rcc::{RccPeripheral, SealedRccPeripheral};
use crate::{Peri, PeripheralType, pac};

#[cfg(vref_vc2ref)]
pub(crate) use pac::vc2ref::{Vc2ref as Regs, vals};
#[cfg(vref_vcdiv)]
pub(crate) use pac::vcdiv::{Vcdiv as Regs, vals};

/// Divider input: analog supply, or the nominal 1.6 V core rail.
///
/// `Supply` means VDD on L010 and VDDA on L011/L012. Where VDDA is present,
/// the own-family datasheet's operating conditions require VDDA = VDD.
pub use vals::InputSource as Source;

/// One of the eight documented fractions of the selected input voltage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Tap {
    /// 1/8 of the input.
    OneEighth,
    /// 2/8 of the input.
    TwoEighths,
    /// 3/8 of the input.
    ThreeEighths,
    /// 4/8 of the input.
    FourEighths,
    /// 5/8 of the input.
    FiveEighths,
    /// 6/8 of the input.
    SixEighths,
    /// 7/8 of the input.
    SevenEighths,
    /// The full input voltage.
    EightEighths,
}
impl Tap {
    fn value(self) -> vals::DividerRatio {
        match self {
            Self::OneEighth => vals::DividerRatio::OneEighth,
            Self::TwoEighths => vals::DividerRatio::TwoEighths,
            Self::ThreeEighths => vals::DividerRatio::ThreeEighths,
            Self::FourEighths => vals::DividerRatio::FourEighths,
            Self::FiveEighths => vals::DividerRatio::FiveEighths,
            Self::SixEighths => vals::DividerRatio::SixEighths,
            Self::SevenEighths => vals::DividerRatio::SevenEighths,
            Self::EightEighths => vals::DividerRatio::EightEighths,
        }
    }
}

/// Fixed reference configuration. Construction does not establish settling.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct Config {
    /// Board-declared analog supply in millivolts; not a measurement.
    pub supply_mv: u16,
    /// Supply or nominal core rail.
    pub source: Source,
    /// Fraction of the selected input.
    pub tap: Tap,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            supply_mv: 3300,
            source: Source::Supply,
            tap: Tap::FourEighths,
        }
    }
}

/// Invalid board declaration or an unavailable shared analog reference.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {
    /// Declared analog supply is outside the own-family operating range.
    InvalidSupplyVoltage,
    /// The shared VC configuration clock did not enable.
    Clock,
    /// The VC module is held in reset; it is not released by this driver.
    HeldInReset,
    /// A comparator is already enabled with this divider as its negative input.
    InUse,
    /// L012 inherited a DIV code 8..15, whose meaning is unqualified.
    /// The ambiguous high bit and all reference configuration are left intact.
    UnsupportedDividerState,
}

/// Immutable, exclusively owned shared reference bank.
///
/// Compatible comparators may borrow this bank simultaneously. There is no
/// reconfiguration or disable method; drop deliberately retains its setting.
pub struct Vref<'d, T: Instance> {
    _peri: Peri<'d, T>,
    pub(crate) config: Config,
}
impl<'d, T: Instance> Vref<'d, T> {
    /// Configure and enable one shared reference bank.
    ///
    /// Invalid supply is rejected before any register write. Clock/reset,
    /// inherited divider-state and active-consumer checks run before changing
    /// the reference. Only the central generated VC clock owner is enabled;
    /// there is no invented divider kernel clock or reset domain.
    pub fn new(peri: Peri<'d, T>, config: Config) -> Result<Self, Error> {
        if !(T::SUPPLY_RANGE_MV.0..=T::SUPPLY_RANGE_MV.1).contains(&config.supply_mv) {
            return Err(Error::InvalidSupplyVoltage);
        }
        critical_section::with(|cs| {
            T::Clock::RCC_INFO
                .enable_with_cs(cs)
                .map_err(|_| Error::Clock)?;
            if !T::Clock::RCC_INFO.is_enabled() {
                return Err(Error::Clock);
            }
            if T::Clock::RCC_INFO.reset_asserted() {
                return Err(Error::HeldInReset);
            }
            #[cfg(vref_vcdiv)]
            let control = T::regs().div();
            #[cfg(vref_vc2ref)]
            let control = T::regs().ref_();
            // L012's own register table describes four DIV bits; its functional
            // text and SDK qualify only 0..7. Never silently clear bit 3.
            #[cfg(vref_vc2ref)]
            if control.read().div().to_bits() > 7 {
                return Err(Error::UnsupportedDividerState);
            }
            for consumer in T::consumers() {
                let config = consumer.cr0().read();
                if config.en() && config.inn() == T::NEGATIVE_MUX {
                    return Err(Error::InUse);
                }
            }
            // No active consumer selects this bank. Modify keeps all unrelated
            // and reserved bits; program the source/tap before enabling it.
            control.modify(|w| w.set_en(false));
            control.modify(|w| {
                w.set_vin(config.source);
                w.set_div(config.tap.value());
            });
            control.modify(|w| w.set_en(true));
            Ok(())
        })?;
        Ok(Self {
            _peri: peri,
            config,
        })
    }

    /// The board declaration and immutable programmed selection.
    pub fn config(&self) -> Config {
        self.config
    }

    /// Own-family analog-supply bounds; board voltage must remain in this range.
    pub const fn supply_voltage_range_mv() -> (u16, u16) {
        T::SUPPLY_RANGE_MV
    }

    /// Whether `Source::Supply` uses VDDA (rather than VDD).
    pub const fn input_uses_vdda() -> bool {
        T::INPUT_USES_VDDA
    }
}

pub(crate) trait SealedInstance {
    type Clock: RccPeripheral;
    fn regs() -> Regs;
    fn consumers() -> [pac::vc::Vc; 2];
    const NEGATIVE_MUX: u8;
    const SUPPLY_RANGE_MV: (u16, u16);
    const INPUT_USES_VDDA: bool;
}
/// Source-qualified reference-bank singleton.
#[allow(private_bounds)]
pub trait Instance: PeripheralType + SealedInstance + 'static {}
pub(crate) trait SealedReference<T> {
    const MUX: u8;
}
/// Source-qualified route from this bank to comparator `T`.
#[allow(private_bounds)]
pub trait Reference<T: crate::comparator::Instance>: Instance + SealedReference<T> {}

macro_rules! impl_instance {
    ($instance:ident, $clock:ident, $first:ident, $second:ident, $mux:literal, $range:expr, $vdda:literal) => {
        impl $crate::vref::SealedInstance for $crate::peripherals::$instance {
            type Clock = $crate::peripherals::$clock;
            fn regs() -> $crate::vref::Regs {
                $crate::pac::$instance
            }
            fn consumers() -> [$crate::pac::vc::Vc; 2] {
                [$crate::pac::$first, $crate::pac::$second]
            }
            const NEGATIVE_MUX: u8 = $mux;
            const SUPPLY_RANGE_MV: (u16, u16) = $range;
            const INPUT_USES_VDDA: bool = $vdda;
        }
        impl $crate::vref::Instance for $crate::peripherals::$instance {}
        impl $crate::vref::SealedReference<$crate::peripherals::$first>
            for $crate::peripherals::$instance
        {
            const MUX: u8 = $mux;
        }
        impl $crate::vref::Reference<$crate::peripherals::$first>
            for $crate::peripherals::$instance
        {
        }
        impl $crate::vref::SealedReference<$crate::peripherals::$second>
            for $crate::peripherals::$instance
        {
            const MUX: u8 = $mux;
        }
        impl $crate::vref::Reference<$crate::peripherals::$second>
            for $crate::peripherals::$instance
        {
        }
    };
}
pub(crate) use impl_instance;
