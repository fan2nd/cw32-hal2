//! Blocking and asynchronous driver mode markers.
mod sealed {
    pub trait Sealed {}
}
/// Marker trait for a driver's operating mode.
pub trait Mode: sealed::Sealed {}
/// Blocking operation.
#[derive(Clone, Copy, Debug)]
pub struct Blocking;
/// Interrupt-driven asynchronous operation.
///
/// This marker does not imply an async driver has been implemented.
#[derive(Clone, Copy, Debug)]
pub struct Async;
impl sealed::Sealed for Blocking {}
impl sealed::Sealed for Async {}
impl Mode for Blocking {}
impl Mode for Async {}
