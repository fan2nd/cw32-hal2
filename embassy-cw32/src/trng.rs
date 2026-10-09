//! L083 bounded polling access to raw hardware random-source output.
//!
//! The manual documents no analog-ready, clock-error, seed-error, repetition
//! test, or health-status indication. Completion means only START cleared after
//! submission. Output has not passed an entropy assessment or statistical health
//! test; no `CryptoRng`/`RngCore` implementation or entropy-strength claim is made.
//! Use a qualified entropy collection/conditioning policy for security uses.
//! Feedback-only deterministic generation is deliberately not exposed here.
//!
//! Budgets count CR1 reads, not time. Timeout never reads data, aborts, resets,
//! disables the analog source, or gates off. Subsequent calls refuse active
//! hardware. Drop retains analog power/gate if busy; after `wait_idle` succeeds,
//! Drop disables both. No IRQ, DMA, async or arbitrary-length fill API exists.
//! See `docs/l083-aes-trng-evidence.md`.

use crate::{Peri, PeripheralType, crypto_geometry::TRNG_OUTPUT_WORDS, pac, rcc::RccPeripheral};
/// Documented shift counts. Count alone is not a promised entropy amount.
pub use pac::trng::vals::Shift;
use pac::trng::vals::{Operation, Source as PacSource};

/// Only input sources that include the analog random source are offered.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Source {
    Analog,
    AnalogXorFeedback,
}
impl Source {
    fn pac(self) -> PacSource {
        match self {
            Self::Analog => PacSource::Analog,
            Self::AnalogXorFeedback => PacSource::AnalogXorFeedback,
        }
    }
}
/// Explicit hardware sampling configuration; this is not a health policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Config {
    pub source: Source,
    pub shift: Shift,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            source: Source::Analog,
            shift: Shift::Cycles256,
        }
    }
}
/// Unassessed hardware output: DATA0 bits31:0, then DATA1 bits63:32.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Sample {
    pub words: [u32; TRNG_OUTPUT_WORDS],
}
/// Startup, configuration or bounded-completion failure. Hardware does not
/// expose a health/error status, so no invented health result appears here.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {
    HeldInReset,
    ClockNotEnabled,
    /// Active hardware or an already-enabled analog source at construction.
    Busy,
    InvalidPollBudget,
    /// Reserved PAC shift encoding was supplied. No hardware was changed.
    InvalidShift,
    /// Configuration or analog-enable readback failed; no result was read.
    WriteFailure,
    /// Hardware may still be active. No output was read or returned.
    Timeout,
}
trait SealedInstance {
    fn regs() -> pac::trng::Trng;
}
/// A source-qualified TRNG register block and its actual RCC owner.
#[allow(private_bounds)]
pub trait Instance: SealedInstance + PeripheralType + RccPeripheral {}
impl SealedInstance for crate::peripherals::TRNG {
    fn regs() -> pac::trng::Trng {
        pac::TRNG
    }
}
impl Instance for crate::peripherals::TRNG {}

/// Exclusive raw-output hardware owner. This type makes no cryptographic RNG guarantee.
pub struct Trng<'d, T: Instance> {
    _peripheral: Peri<'d, T>,
    config: Config,
}
impl<'d, T: Instance> Trng<'d, T> {
    /// Enable the dedicated gate, preserving reset and active hardware. The
    /// analog source remains off until generation. Reserved SHIFT encodings
    /// are rejected even when obtained through the public PAC enum conversion.
    pub fn new(peripheral: Peri<'d, T>, config: Config) -> Result<Self, Error> {
        if !matches!(
            config.shift,
            Shift::Cycles8
                | Shift::Cycles16
                | Shift::Cycles32
                | Shift::Cycles64
                | Shift::Cycles128
                | Shift::Cycles256
        ) {
            return Err(Error::InvalidShift);
        }
        if T::RCC_INFO.reset_asserted() {
            return Err(Error::HeldInReset);
        }
        critical_section::with(|cs| T::RCC_INFO.enable_with_cs(cs))
            .map_err(|_| Error::ClockNotEnabled)?;
        if !T::RCC_INFO.is_enabled() {
            return Err(Error::ClockNotEnabled);
        }
        let c = T::regs().cr1().read();
        if c.start() == Operation::Active || c.en() {
            return Err(Error::Busy);
        }
        Ok(Self {
            _peripheral: peripheral,
            config,
        })
    }
    /// Observe only the computation status. There is no analog-ready flag.
    pub fn is_busy(&self) -> bool {
        T::regs().cr1().read().start() == Operation::Active
    }
    /// Wait at most `polls` status reads without reading an old result. The
    /// analog source remains enabled until Drop or the next successful sample.
    pub fn wait_idle(&mut self, polls: u32) -> Result<(), Error> {
        if polls == 0 {
            return Err(Error::InvalidPollBudget);
        }
        for _ in 0..polls {
            if !self.is_busy() {
                return Ok(());
            }
            core::hint::spin_loop();
        }
        Err(Error::Timeout)
    }
    /// Submit one operation and return raw words only after its completion.
    /// An older timed-out result is discarded only once the engine is idle.
    /// No timer-based analog settling assumption or software entropy test is
    /// silently substituted for a hardware-ready/health indication.
    pub fn generate(&mut self, polls: u32) -> Result<Sample, Error> {
        if polls == 0 {
            return Err(Error::InvalidPollBudget);
        }
        if self.is_busy() {
            return Err(Error::Busy);
        }
        let r = T::regs();
        // Configure only while idle. Zero START here is not an abort command.
        r.cr1().write(|w| w.set_en(true));
        r.cr2().write(|w| {
            w.set_source(self.config.source.pac());
            w.set_shift(self.config.shift);
        });
        let c = r.cr2().read();
        if !r.cr1().read().en()
            || c.source() != self.config.source.pac()
            || c.shift() != self.config.shift
        {
            r.cr1().write(|w| w.set_en(false));
            return Err(Error::WriteFailure);
        }
        r.cr1().write(|w| {
            w.set_en(true);
            w.set_start(Operation::Active);
        });
        self.wait_idle(polls)?;
        let sample = Sample {
            words: [r.data0().read().data0(), r.data1().read().data1()],
        };
        r.cr1().write(|w| w.set_en(false));
        Ok(sample)
    }
}
impl<T: Instance> Drop for Trng<'_, T> {
    fn drop(&mut self) {
        if self.is_busy() {
            return;
        }
        T::regs().cr1().write(|w| w.set_en(false));
        critical_section::with(|cs| {
            let _ = T::RCC_INFO.disable_with_cs(cs);
        });
    }
}
