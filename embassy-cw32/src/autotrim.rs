//! Polling AUTOTRIM timer on the frozen, factory-trimmed 48 MHz HSIOSC.
//!
//! Only MD=3, AUTO=0 and SRC=HSIOSC are supported. The working source is the
//! undivided oscillator, independent of HSI/HCLK/PCLK divisors. A period is
//! `(ARR + 1) * 2^PRS` source cycles; it is not an exact physical deadline.
//! The factory ClockBounds contract still requires qualified VDD/temperature
//! and unchanged oscillator state. Debug freeze or sleep can interrupt timing.
//!
//! No calibration, FCAP frequency measurement, one-shot mode, external source,
//! interrupt ownership or deep-sleep timing is provided. AUTOTRIM_LCD is shared:
//! this driver never changes NVIC state or LCD registers. A pending UD flag
//! coalesces any number of wraps; polling cannot recover missed periods.
//! See docs/autotrim-counter-evidence.md for source conflicts and open questions.

use crate::{
    Peri, PeripheralType, pac,
    rcc::{ClockBounds, RccPeripheral},
};

pub use pac::autotrim::vals::Prescaler;
use pac::autotrim::vals::{Mode, Source};

/// Periodic down-counter settings. PRS=0 is reserved and rejected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Config {
    pub prescaler: Prescaler,
    /// Reload value. Zero is a valid one-prescaled-cycle period.
    pub reload: u16,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            prescaler: Prescaler::Div32768,
            reload: u16::MAX,
        }
    }
}
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {
    ClockNotInitialized,
    HsiNotReady,
    ClockGate,
    HeldInReset,
    InvalidPrescaler,
    /// Existing EN or AUTO ownership is not safe to reset or take over.
    AlreadyActive,
    /// A peripheral-local interrupt source is already enabled.
    InterruptInUse,
    InvalidPollBudget,
    /// The iteration budget expired. EN=0 was requested, IER masked and flags
    /// cleared. There is no documented physical stop acknowledgement.
    Timeout,
}
pub(crate) mod sealed {
    use super::*;
    pub(crate) trait Instance: RccPeripheral {
        fn regs() -> pac::autotrim::Autotrim;
    }
}
#[allow(private_bounds)]
pub trait Instance: PeripheralType + sealed::Instance + 'static {}

/// Owns the AUTOTRIM block, with polling-only periodic underflow events.
/// Creation leaves the counter disabled. Use `start` to reset and begin a period.
pub struct AutotrimTimer<'d, T: Instance> {
    _peri: Peri<'d, T>,
    config: Config,
    source: ClockBounds,
}
impl<'d, T: Instance> AutotrimTimer<'d, T> {
    pub fn new(peri: Peri<'d, T>, config: Config) -> Result<Self, Error> {
        validate(config)?;
        let source = crate::rcc::try_clocks()
            .ok_or(Error::ClockNotInitialized)?
            .hsi_osc_bounds();
        if !pac::SYSCTRL.cr1().read().hsien() || !pac::SYSCTRL.hsi().read().stable() {
            return Err(Error::HsiNotReady);
        }
        critical_section::with(|cs| {
            if T::RCC_INFO.reset_asserted() {
                return Err(Error::HeldInReset);
            }
            // Enable only the configuration clock before inspecting inherited
            // ownership. Do not reset or write CR/IER/ICR on a rejected owner.
            T::RCC_INFO
                .enable_with_cs(cs)
                .map_err(|_| Error::ClockGate)?;
            if !T::RCC_INFO.is_enabled() {
                return Err(Error::ClockGate);
            }
            let cr = T::regs().cr().read();
            if cr.en() || cr.auto() {
                return Err(Error::AlreadyActive);
            }
            let ier = T::regs().ier().read();
            if ier.end() || ier.ok() || ier.ud() || ier.ov() || ier.miss() {
                return Err(Error::InterruptInUse);
            }
            Ok(())
        })?;
        let mut this = Self {
            _peri: peri,
            config,
            source,
        };
        this.reset_and_configure()?;
        Ok(this)
    }
    fn reset_and_configure(&mut self) -> Result<(), Error> {
        critical_section::with(|cs| {
            // APBRST2.AUTOTRIM is independently owned; LCD reset is untouched.
            T::RCC_INFO
                .enable_and_reset_with_cs(cs)
                .map_err(|_| Error::ClockGate)?;
            if !T::RCC_INFO.is_enabled() {
                return Err(Error::ClockGate);
            }
            if T::RCC_INFO.reset_asserted() {
                return Err(Error::HeldInReset);
            }
            let r = T::regs();
            r.ier().write(|w| {
                w.set_end(false);
                w.set_ok(false);
                w.set_ud(false);
                w.set_ov(false);
                w.set_miss(false);
            });
            // Configure CR[11:1] while EN=0, then write ARR before enabling.
            r.cr().write(|w| {
                w.set_en(false);
                w.set_md(Mode::Timer);
                w.set_auto(false);
                w.set_prs(self.config.prescaler);
                w.set_src(Source::HsiOsc);
                w.set_ost(false);
            });
            r.arr().write(|w| w.set_arr(self.config.reload));
            clear_all::<T>();
            Ok(())
        })
    }
    /// Reset and configure the owned block, clear old flags, then set EN last.
    /// Reset establishes a new counter sequence; enable-to-first-tick latency
    /// is unqualified. A simple EN toggle has no phase-preservation guarantee.
    pub fn start(&mut self) -> Result<(), Error> {
        self.stop();
        self.reset_and_configure()?;
        T::regs().cr().modify(|w| w.set_en(true));
        Ok(())
    }
    /// Request EN=0 without consuming an underflow. This is a control request,
    /// not a claim of physical drain or a synchronized counter snapshot.
    pub fn stop(&mut self) {
        T::regs().cr().modify(|w| w.set_en(false));
    }
    /// Read EN control state. The hardware has no stop-complete status.
    pub fn is_enabled(&self) -> bool {
        T::regs().cr().read().en()
    }
    /// One raw CNT snapshot. No undocumented synchronization/coherency protocol
    /// or elapsed-period count is inferred from a live read.
    pub fn counter(&self) -> u16 {
        T::regs().cnt().read().cnt()
    }
    /// True when at least one periodic underflow is pending; does not clear it.
    pub fn is_elapsed(&self) -> bool {
        T::regs().isr().read().ud()
    }
    /// Consume a coalesced underflow event. A wrap concurrent with the clear may
    /// coalesce too. This is unsuitable for lossless elapsed-period accounting.
    pub fn try_wait(&mut self) -> bool {
        if !self.is_elapsed() {
            return false;
        }
        let mut command = pac::autotrim::regs::Icr::write_noop();
        command.set_ud(false);
        T::regs().icr().write_value(command);
        true
    }
    /// Poll at most `poll_budget` times for one event. This is an iteration
    /// bound, not a wall-clock timeout. On exhaustion stop and clear this block.
    pub fn blocking_wait(&mut self, poll_budget: u32) -> Result<(), Error> {
        if poll_budget == 0 {
            return Err(Error::InvalidPollBudget);
        }
        for _ in 0..poll_budget {
            if self.try_wait() {
                return Ok(());
            }
            core::hint::spin_loop();
        }
        self.stop();
        mask_interrupts::<T>();
        clear_all::<T>();
        Err(Error::Timeout)
    }
    /// Change ARR. While running this takes effect at the next reload and does
    /// not change current CNT. `period_source_cycles` then describes the new
    /// reload; call `start` to reset the sequence before relying on that period.
    pub fn set_reload(&mut self, reload: u16) {
        T::regs().arr().write(|w| w.set_arr(reload));
        self.config.reload = reload;
    }
    /// Stop, reset, reconfigure and leave disabled. Invalid input changes nothing.
    pub fn set_config(&mut self, config: Config) -> Result<(), Error> {
        validate(config)?;
        self.stop();
        self.config = config;
        self.reset_and_configure()
    }
    pub fn config(&self) -> Config {
        self.config
    }
    /// Exact configured period in raw HSIOSC source cycles, up to 2^31.
    pub fn period_source_cycles(&self) -> u32 {
        (u32::from(self.config.reload) + 1) << self.config.prescaler.to_bits()
    }
    /// Factory-qualified raw HSIOSC envelope; never reconstructed from HSI.
    pub fn source_clock_bounds(&self) -> ClockBounds {
        self.source
    }
    /// Outward-rounded steady-running period bounds in ns. Excludes startup,
    /// stop/restart, debug halt and sleep; a queued ARR applies after reload.
    pub fn period_bounds_ns(&self) -> (u64, u64) {
        let cycles = self.period_source_cycles();
        (
            self.source.minimum_duration_ns(cycles),
            self.source.maximum_duration_ns(cycles),
        )
    }
}
impl<T: Instance> Drop for AutotrimTimer<'_, T> {
    fn drop(&mut self) {
        critical_section::with(|cs| {
            self.stop();
            mask_interrupts::<T>();
            clear_all::<T>();
            let _ = T::RCC_INFO.disable_with_cs(cs);
        });
    }
}
fn validate(config: Config) -> Result<(), Error> {
    if config.prescaler.to_bits() == 0 {
        Err(Error::InvalidPrescaler)
    } else {
        Ok(())
    }
}
fn mask_interrupts<T: Instance>() {
    T::regs().ier().write(|w| {
        w.set_end(false);
        w.set_ok(false);
        w.set_ud(false);
        w.set_ov(false);
        w.set_miss(false);
    });
}
fn clear_all<T: Instance>() {
    let mut command = pac::autotrim::regs::Icr::write_noop();
    command.set_end(false);
    command.set_ok(false);
    command.set_ud(false);
    command.set_ov(false);
    command.set_miss(false);
    T::regs().icr().write_value(command);
}
macro_rules! impl_instance {
    ($inst:ident, $irq:ident) => {
        impl $crate::autotrim::sealed::Instance for $crate::peripherals::$inst {
            fn regs() -> $crate::pac::autotrim::Autotrim {
                $crate::pac::$inst
            }
        }
        impl $crate::autotrim::Instance for $crate::peripherals::$inst {}
    };
}
pub(crate) use impl_instance;
