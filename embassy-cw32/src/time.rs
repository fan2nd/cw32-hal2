//! Frequency units, following the Embassy STM32 HAL's `Hertz` API.

use core::fmt::Display;
use core::ops::{Div, Mul};

/// Frequency in whole hertz.
#[derive(Eq, PartialEq, Ord, PartialOrd, Clone, Copy, Debug, Default)]
pub struct Hertz(pub u32);

impl Hertz {
    /// Construct a frequency in hertz.
    pub const fn hz(hertz: u32) -> Self {
        Self(hertz)
    }
    /// Construct a frequency in kilohertz.
    pub const fn khz(kilohertz: u32) -> Self {
        Self(kilohertz * 1_000)
    }
    /// Construct a frequency in megahertz.
    pub const fn mhz(megahertz: u32) -> Self {
        Self(megahertz * 1_000_000)
    }
}

/// Construct a frequency in hertz.
pub const fn hz(hertz: u32) -> Hertz {
    Hertz::hz(hertz)
}
/// Construct a frequency in kilohertz.
pub const fn khz(kilohertz: u32) -> Hertz {
    Hertz::khz(kilohertz)
}
/// Construct a frequency in megahertz.
pub const fn mhz(megahertz: u32) -> Hertz {
    Hertz::mhz(megahertz)
}

impl Display for Hertz {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} Hz", self.0)
    }
}

#[cfg(feature = "defmt")]
impl defmt::Format for Hertz {
    fn format(&self, f: defmt::Formatter) {
        defmt::write!(f, "{=u32} Hz", self.0);
    }
}

macro_rules! frequency_ops {
    ($($ty:ty),*) => {$(
        impl Mul<$ty> for Hertz {
            type Output = Hertz;
            fn mul(self, rhs: $ty) -> Hertz { Hertz(self.0 * u32::from(rhs)) }
        }
        impl Div<$ty> for Hertz {
            type Output = Hertz;
            fn div(self, rhs: $ty) -> Hertz { Hertz(self.0 / u32::from(rhs)) }
        }
    )*};
}
frequency_ops!(u8, u16, u32);

impl Div<Hertz> for Hertz {
    type Output = u32;
    fn div(self, rhs: Hertz) -> u32 {
        self.0 / rhs.0
    }
}
