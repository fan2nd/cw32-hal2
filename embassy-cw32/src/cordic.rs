//! L012 blocking CORDIC operations in signed Q1.31, with checked input domains.
//!
//! Angles for circular functions use units of pi. Hyperbolic arguments do not.
//! Every method documents the hardware's scaled output; no floating-point
//! conversion, software fallback, range extension or post-correction is hidden.
//! The maximum ITER table encoding is selected. The manual's ITER prose conflicts
//! with its table; no execution-time or numeric accuracy guarantee is made.
//!
//! Budgets count CSR reads, not time. Timeout does not abort. A subsequent call
//! refuses busy hardware; once idle, it may discard an unread timed-out result.
//! Drop never waits or resets and retains the gate while busy. DMA/IRQ APIs and
//! Q1.15 are outside this driver's scope. See `docs/l012-math-evidence.md`.

use crate::{Peri, PeripheralType, pac, rcc::RccPeripheral};
use pac::cordic::vals::{Comp, Format, Func, Iter, Scale};
mod domains {
    include!(concat!(env!("OUT_DIR"), "/_cordic_domains.rs"));
}

/// Signed Q1.31 bit pattern: real value = `to_bits() / 2^31`, in [-1, 1).
/// No implicit saturation or floating-point conversion is performed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(transparent)]
pub struct Q1_31(i32);
impl Q1_31 {
    /// Construct from the exact two's-complement hardware bit pattern.
    pub const fn from_bits(bits: i32) -> Self {
        Self(bits)
    }
    /// Extract the exact two's-complement hardware bit pattern.
    pub const fn to_bits(self) -> i32 {
        self.0
    }
}

/// Startup, input-domain or bounded-wait failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
pub enum Error {
    /// Peripheral reset is asserted; no reset is released or pulsed.
    HeldInReset,
    /// Clock enable did not read back as enabled.
    ClockNotEnabled,
    /// An earlier operation is busy; no control or operand was written.
    Busy,
    /// Status-read budget expired; the operation may still be running.
    Timeout,
    /// Operand is outside this API's documented domain; no operation was started.
    OutOfDomain,
}

/// Circular sine/cosine pair, both in signed Q1.31.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct SinCos {
    /// sin(angle * pi), from Y.
    pub sin: Q1_31,
    /// cos(angle * pi), from X.
    pub cos: Q1_31,
}
/// Scaled hyperbolic outputs, both in signed Q1.31.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct SinhCosh {
    /// sinh(2 * input) / 2, from Y.
    pub sinh_half: Q1_31,
    /// cosh(2 * input) / 2, from X.
    pub cosh_half: Q1_31,
}

/// Input multiplier for hardware arctangent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum AtanScale {
    X1,
    X2,
    X4,
    X8,
    X16,
    X32,
    X64,
    X128,
}
impl AtanScale {
    fn value(self) -> Scale {
        match self {
            Self::X1 => Scale::Scale0,
            Self::X2 => Scale::Scale1,
            Self::X4 => Scale::Scale2,
            Self::X8 => Scale::Scale3,
            Self::X16 => Scale::Scale4,
            Self::X32 => Scale::Scale5,
            Self::X64 => Scale::Scale6,
            Self::X128 => Scale::Scale7,
        }
    }
}
/// Natural-logarithm input domain and output scaling, as specified by hardware.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum LogScale {
    /// x in [0.0535, 0.5): return ln(2*x)/4.
    X2Over4,
    /// x in [0.25, 0.75): return ln(4*x)/2.
    X4Over2,
    /// x in [0.375, 0.875): return ln(8*x)/2.
    X8Over2,
    /// x in [0.4375, 0.584): return ln(16*x)/4.
    X16Over4,
}
impl LogScale {
    fn parameters(self) -> (Scale, (i32, i32)) {
        match self {
            Self::X2Over4 => (Scale::Scale1, domains::LN1),
            Self::X4Over2 => (Scale::Scale2, domains::LN2),
            Self::X8Over2 => (Scale::Scale3, domains::LN3),
            Self::X16Over4 => (Scale::Scale4, domains::LN4),
        }
    }
}
/// Fixed-point square-root input domain and output scaling.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum SqrtScale {
    /// x in [0.027, 0.75): return sqrt(x).
    X1,
    /// x in [0.375, 0.875): return sqrt(2*x)/2.
    X2Over2,
    /// x in [0.4375, 0.585]: return sqrt(4*x)/2.
    X4Over2,
}
impl SqrtScale {
    fn parameters(self) -> (Scale, (i32, i32)) {
        match self {
            Self::X1 => (Scale::Scale0, domains::SQRT0),
            Self::X2Over2 => (Scale::Scale1, domains::SQRT1),
            Self::X4Over2 => (Scale::Scale2, domains::SQRT2),
        }
    }
}

trait SealedInstance {
    fn regs() -> pac::cordic::Cordic;
}
/// A source-qualified CORDIC instance.
#[allow(private_bounds)]
pub trait Instance: SealedInstance + PeripheralType + RccPeripheral {}
impl SealedInstance for crate::peripherals::CORDIC {
    fn regs() -> pac::cordic::Cordic {
        pac::CORDIC
    }
}
impl Instance for crate::peripherals::CORDIC {}

/// Exclusive blocking CORDIC owner.
pub struct Cordic<'d, T: Instance> {
    _peripheral: Peri<'d, T>,
}
impl<'d, T: Instance> Cordic<'d, T> {
    /// Enable the clock while preserving resets and shared resources.
    /// Busy startup leaves the clock running without writing CORDIC controls.
    pub fn new(peripheral: Peri<'d, T>) -> Result<Self, Error> {
        if T::RCC_INFO.reset_asserted() {
            return Err(Error::HeldInReset);
        }
        critical_section::with(|cs| T::RCC_INFO.enable_with_cs(cs))
            .map_err(|_| Error::ClockNotEnabled)?;
        if !T::RCC_INFO.is_enabled() {
            return Err(Error::ClockNotEnabled);
        }
        if T::regs().csr().read().busy() {
            return Err(Error::Busy);
        }
        // No operand is written; disable inherited request sources while idle.
        Self::configure(Func::Cos, Scale::Scale0);
        Ok(Self {
            _peripheral: peripheral,
        })
    }
    /// Read BUSY without consuming a result or modifying hardware.
    pub fn is_busy(&self) -> bool {
        T::regs().csr().read().busy()
    }

    /// sin/cos of `angle * pi`, for angle in [-1, 1).
    /// Hardware quantization of exact +1 endpoints is not specified; this method
    /// returns the hardware bits without promising saturation or rounding.
    /// At most `polls` CSR reads follow submission; zero submits nothing.
    pub fn sin_cos(&mut self, angle: Q1_31, polls: u32) -> Result<SinCos, Error> {
        self.prepare(Func::Cos, Scale::Scale0, polls)?;
        let r = T::regs();
        r.z().write(|w| w.set_z(angle.0 as u32));
        self.wait(polls)?;
        let cos = Q1_31(r.x().read().x() as i32);
        let sin = Q1_31(r.y().read().y() as i32);
        Ok(SinCos { sin, cos })
    }
    /// atan(y/x)/pi, using the hardware phase function. Requires x > 0.
    /// This intentionally excludes zero/negative-x quadrant and origin cases,
    /// whose branch-cut behavior is not specified in the manual's atan(y/x) table.
    pub fn phase(&mut self, x: Q1_31, y: Q1_31, polls: u32) -> Result<Q1_31, Error> {
        if x.0 <= 0 {
            return Err(Error::OutOfDomain);
        }
        self.prepare(Func::Atan2, Scale::Scale0, polls)?;
        let r = T::regs();
        r.x().write(|w| w.set_x(x.0 as u32));
        r.y().write(|w| w.set_y(y.0 as u32));
        self.wait(polls)?;
        Ok(Q1_31(r.z().read().z() as i32))
    }
    /// sqrt(x*x + y*y)/2, for both inputs in [-1, 1), with hardware K compensation.
    /// The factor 1/2 is part of the documented hardware result.
    pub fn hypot_half(&mut self, x: Q1_31, y: Q1_31, polls: u32) -> Result<Q1_31, Error> {
        self.prepare(Func::Hypot, Scale::Scale0, polls)?;
        let r = T::regs();
        r.x().write(|w| w.set_x(x.0 as u32));
        r.y().write(|w| w.set_y(y.0 as u32));
        self.wait(polls)?;
        Ok(Q1_31(r.x().read().x() as i32))
    }
    /// atan(multiplier*y)/pi, for y in [-1, 1), with multiplier chosen by `scale`.
    pub fn atan(&mut self, y: Q1_31, scale: AtanScale, polls: u32) -> Result<Q1_31, Error> {
        self.prepare(Func::Atan, scale.value(), polls)?;
        let r = T::regs();
        r.y().write(|w| w.set_y(y.0 as u32));
        self.wait(polls)?;
        Ok(Q1_31(r.z().read().z() as i32))
    }
    /// Return sinh(2*z)/2 and cosh(2*z)/2; z must be in [-0.559, 0.559].
    pub fn sinh_cosh_half(&mut self, z: Q1_31, polls: u32) -> Result<SinhCosh, Error> {
        check_domain(z, domains::HYPERBOLIC)?;
        self.prepare(Func::Cosh, Scale::Scale1, polls)?;
        let r = T::regs();
        r.z().write(|w| w.set_z(z.0 as u32));
        self.wait(polls)?;
        let cosh_half = Q1_31(r.x().read().x() as i32);
        let sinh_half = Q1_31(r.y().read().y() as i32);
        Ok(SinhCosh {
            sinh_half,
            cosh_half,
        })
    }
    /// atanh(2*y)/2, for y in [-0.403, 0.403].
    pub fn atanh_half(&mut self, y: Q1_31, polls: u32) -> Result<Q1_31, Error> {
        check_domain(y, domains::ATANH)?;
        self.prepare(Func::Atanh, Scale::Scale1, polls)?;
        let r = T::regs();
        r.y().write(|w| w.set_y(y.0 as u32));
        self.wait(polls)?;
        Ok(Q1_31(r.z().read().z() as i32))
    }
    /// Natural logarithm with the exact input domain/output scaling in [`LogScale`].
    pub fn ln(&mut self, x: Q1_31, scale: LogScale, polls: u32) -> Result<Q1_31, Error> {
        let (scale, domain) = scale.parameters();
        check_domain(x, domain)?;
        self.prepare(Func::Ln, scale, polls)?;
        let r = T::regs();
        r.x().write(|w| w.set_x(x.0 as u32));
        self.wait(polls)?;
        Ok(Q1_31(r.z().read().z() as i32))
    }
    /// Fixed-point root with [`SqrtScale`] domain/scaling and hardware K compensation.
    /// For full-range unsigned integer roots, use [`crate::eau::Eau::square_root`].
    pub fn square_root(&mut self, x: Q1_31, scale: SqrtScale, polls: u32) -> Result<Q1_31, Error> {
        let (scale, domain) = scale.parameters();
        check_domain(x, domain)?;
        self.prepare(Func::Sqrt, scale, polls)?;
        let r = T::regs();
        r.x().write(|w| w.set_x(x.0 as u32));
        self.wait(polls)?;
        Ok(Q1_31(r.x().read().x() as i32))
    }
    fn prepare(&mut self, function: Func, scale: Scale, polls: u32) -> Result<(), Error> {
        if polls == 0 {
            return Err(Error::Timeout);
        }
        if self.is_busy() {
            return Err(Error::Busy);
        }
        Self::configure(function, scale);
        Ok(())
    }
    fn configure(function: Func, scale: Scale) {
        // CSR reset seed is zero. RO flags are never acknowledged by writes;
        // the documented trigger clears EOC. All DMA/IRQ sources remain disabled.
        T::regs().csr().write(|w| {
            w.set_func(function);
            w.set_scale(scale);
            w.set_format(Format::Q131);
            w.set_iter(Iter::Iter32);
            w.set_comp(Comp::Compensated);
            w.set_ie(false);
            w.set_dmaeoc(false);
            w.set_dmaidle(false);
        });
    }
    fn wait(&self, polls: u32) -> Result<(), Error> {
        for _ in 0..polls {
            let status = T::regs().csr().read();
            if !status.busy() && status.eoc() {
                return Ok(());
            }
            core::hint::spin_loop();
        }
        Err(Error::Timeout)
    }
}
fn check_domain(value: Q1_31, (minimum, maximum): (i32, i32)) -> Result<(), Error> {
    if (minimum..=maximum).contains(&value.0) {
        Ok(())
    } else {
        Err(Error::OutOfDomain)
    }
}
impl<T: Instance> Drop for Cordic<'_, T> {
    fn drop(&mut self) {
        if !self.is_busy() {
            critical_section::with(|cs| {
                let _ = T::RCC_INFO.disable_with_cs(cs);
            });
        }
    }
}
