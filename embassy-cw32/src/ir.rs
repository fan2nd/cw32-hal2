//! Owned infrared signal-combination controller and optional output pin.
//!
//! Source selectors describe this exact destination's timer/UART wiring. They
//! are not universal trigger sources. Configure and retain the required source
//! signals separately; this driver never reconfigures or clocks timers/UARTs.
//! Routing alone does not generate a carrier, encode a protocol, or receive IR.
//!
//! Classic families use the real SYSCTRL.IRMOD register through an independent
//! generated IR ownership token. Low families use IRMOD.CR. Neither has an
//! independent RCC gate. Drop disconnects the owned output pin, preserving the
//! controller state and all signal-source peripherals.
//!
//! L012's current manual and SDK disagree on MOD encodings: only attachment,
//! software control and inversion are exposed there, preserving existing MOD.
//! R031's only documented output is debug PA13, currently withheld by the HAL.
#[cfg(ir_sysctrl)]
use crate::peripherals::IR as Peripheral;
#[cfg(not(ir_sysctrl))]
use crate::peripherals::IRMOD as Peripheral;
use crate::{
    Peri,
    gpio::{Flex, Pin, Pull},
    pac,
};

#[cfg(all(ir_modes, not(ir_sysctrl)))]
pub use pac::irmod::vals::IrMode as Mode;
/// The selected chip's exact IR boolean combinations and internal source signals.
#[cfg(all(ir_modes, ir_sysctrl))]
pub use pac::sysctrl::vals::IrMode as Mode;

/// Owned IR controller. The optional output pin is retained for its lifetime.
pub struct IrModulator<'d> {
    _peripheral: Peri<'d, Peripheral>,
    output: Option<Flex<'d>>,
}
impl<'d> IrModulator<'d> {
    /// Acquire the controller without changing mode, software control or inversion.
    /// No GPIO is enabled until [`Self::with_output`] is used.
    pub fn attach(peripheral: Peri<'d, Peripheral>) -> Self {
        Self {
            _peripheral: peripheral,
            output: None,
        }
    }
    /// Select a documented boolean combination, preserving other control bits.
    /// Inputs are live signals from externally configured timer/UART drivers.
    /// Changing a running mode can glitch the output; disconnect it first when
    /// the receiver requires a glitch-free transition. No atomic waveform claim.
    #[cfg(ir_modes)]
    pub fn set_mode(&mut self, mode: Mode) {
        critical_section::with(|_| {
            #[cfg(ir_sysctrl)]
            pac::SYSCTRL.irmod().modify(|w| w.set_mod_(mode));
            #[cfg(not(ir_sysctrl))]
            pac::IRMOD.cr().modify(|w| w.set_mod_(mode));
        });
    }
    /// Read the currently selected, documented combination.
    #[cfg(ir_modes)]
    pub fn mode(&self) -> Mode {
        #[cfg(ir_sysctrl)]
        {
            pac::SYSCTRL.irmod().read().mod_()
        }
        #[cfg(not(ir_sysctrl))]
        {
            pac::IRMOD.cr().read().mod_()
        }
    }
    /// Attach a package-bonded IR output. Replacing an output disconnects the old
    /// pin first. Uses push-pull with no weak pulls; external LED circuitry and
    /// current limiting are required. Debug/reset pad remapping is not performed.
    pub fn with_output<P: OutputPin>(mut self, pin: Peri<'d, P>) -> Self {
        self.output = None;
        let mut pin = Flex::new(pin);
        pin.set_as_af(P::AF, true, Pull::None);
        self.output = Some(pin);
        self
    }
    /// Disconnect the owned GPIO while retaining controller ownership/settings.
    pub fn disconnect_output(&mut self) {
        self.output = None;
    }
    /// Set IRSW's boolean input. Its AND/OR effect depends on the selected mode;
    /// `false` is not a universal output-disable operation.
    #[cfg(ir_software)]
    pub fn set_software_input(&mut self, high: bool) {
        critical_section::with(|_| {
            #[cfg(ir_sysctrl)]
            pac::SYSCTRL.irmod().modify(|w| w.set_irsw(high));
            #[cfg(not(ir_sysctrl))]
            pac::IRMOD.cr().modify(|w| w.set_irsw(high));
        });
    }
    /// Invert the modulated output after its documented signal combination.
    #[cfg(ir_invert)]
    pub fn set_inverted(&mut self, inverted: bool) {
        critical_section::with(|_| pac::IRMOD.cr().modify(|w| w.set_inv(inverted)));
    }
}
pub(crate) trait SealedOutputPin {
    const AF: u8;
}
/// A reviewed IR output route bonded on the selected chip and safe GPIO policy.
#[allow(private_bounds)]
pub trait OutputPin: Pin + SealedOutputPin {}
#[allow(unused_macros)]
macro_rules! impl_output_pin {
    ($pin:ident, $af:expr) => {
        impl crate::ir::SealedOutputPin for crate::peripherals::$pin {
            const AF: u8 = $af;
        }
        impl crate::ir::OutputPin for crate::peripherals::$pin {}
    };
}
#[allow(unused_imports)]
pub(crate) use impl_output_pin;
