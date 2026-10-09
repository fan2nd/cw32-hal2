//! Reviewed vector membership; pin bindings themselves come from GLOBAL links.
use crate::interrupt::typelevel::{self, Interrupt};

mod sealed {
    pub trait GpioInterrupt {}
}

/// A GPIO interrupt vector and its bank group.
#[allow(private_bounds)]
pub trait GpioInterrupt: Interrupt + sealed::GpioInterrupt {
    /// Bank numbers served by the vector, A=0 through F=5.
    #[doc(hidden)]
    const BANKS: &'static [u8];
}

/// A GPIO interrupt dedicated to one bank.
///
/// Retained for existing single-bank generic code. Shared C/D and E/F vectors
/// implement [`GpioInterrupt`] instead of this trait.
pub trait PortInterrupt: GpioInterrupt {
    /// Bank number, A=0 through F=5.
    #[doc(hidden)]
    const PORT: u8;
}

// Normal impls generated from each GPIO peripheral's reviewed GLOBAL link.
include!(concat!(env!("OUT_DIR"), "/_generated_exti.rs"));
