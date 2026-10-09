//! Blocking I2C drivers selected by the verified peripheral IP version.
#[cfg(not(i2c_cw32l012_v1))]
mod classic;
#[cfg(not(i2c_cw32l012_v1))]
pub use classic::*;
#[cfg(not(i2c_cw32l012_v1))]
pub(crate) use classic::{impl_instance, impl_pin, sealed};

#[cfg(i2c_cw32l012_v1)]
mod lpi2c;
#[cfg(i2c_cw32l012_v1)]
pub use lpi2c::*;
#[cfg(i2c_cw32l012_v1)]
pub(crate) use lpi2c::{impl_instance, impl_pin, sealed};
