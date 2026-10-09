//! Polling low-voltage monitor with exclusive peripheral and analog-pin ownership.
//!
//! Only an idle, unfiltered LVD with reset, interrupt and event triggers disabled
//! can be configured. Those policy bits, NVIC, BOR, reference/trim, filter clocks,
//! and shared analog owners are never changed. Low-family timer-break routing is
//! also rejected while active. The hardware has no independent RCC gate; no gate
//! or reset operation is synthesized. Drop disables only this monitor's EN bit.
//!
//! Thresholds are nominal, not calibrated voltmeter readings. L012's documented
//! table assumes Vcore=1.6 V. Allow board-qualified analog startup/settling time
//! before interpreting samples; this driver does not claim a ready indication.
//! No IRQ, wakeup, reset-policy, filter or async API is provided.
use crate::{
    Peri,
    gpio::{Flex, Pin},
    pac,
    peripherals::LVD,
};

/// A documented nominal threshold, bounded by the selected chip's source table.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Threshold(u8);
impl Threshold {
    /// Exact nominal millivolt choices documented for the selected chip.
    pub const fn available_millivolts() -> &'static [u16] {
        crate::LVD_THRESHOLDS_MV
    }
    /// Select an exact documented nominal value; no rounding or raw encoding.
    pub fn from_millivolts(millivolts: u16) -> Option<Self> {
        crate::LVD_THRESHOLDS_MV
            .iter()
            .position(|v| *v == millivolts)
            .map(|n| Self(n as u8))
    }
    /// Nominal selected threshold. L012 requires Vcore=1.6 V for this table.
    pub fn millivolts(self) -> u16 {
        crate::LVD_THRESHOLDS_MV[self.0 as usize]
    }
}

/// Current hysteretic comparator state, without any event flag acknowledgment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VoltageState {
    BelowThreshold,
    AboveThreshold,
}

/// Configuration or bounded observation failure.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    /// An existing monitor, reset/IRQ/event/filter policy or timer break is active.
    AlreadyConfigured,
    /// A requested source, threshold or enable bit did not read back correctly.
    WriteFailure,
    /// A zero register-poll budget is invalid.
    InvalidPollBudget,
    /// The requested state was not sampled within the specified poll budget.
    Timeout,
}

/// Owned, unfiltered, polling-only LVD monitor.
pub struct LowVoltageMonitor<'d> {
    _peripheral: Peri<'d, LVD>,
    _input: Option<Flex<'d>>,
    threshold: Threshold,
}
impl<'d> LowVoltageMonitor<'d> {
    /// Monitor the on-chip supply (VDD or VDDA, according to this chip).
    /// Rejects existing ownership before any write. Does not clear pending flags.
    pub fn new_supply(peripheral: Peri<'d, LVD>, threshold: Threshold) -> Result<Self, Error> {
        critical_section::with(|_| {
            check_idle()?;
            configure(0, threshold)
        })?;
        Ok(Self {
            _peripheral: peripheral,
            _input: None,
            threshold,
        })
    }
    /// Monitor one verified external input, holding its analog GPIO ownership.
    /// The input voltage must stay within the chip's specified analog pin limits.
    /// Rejects existing LVD policy before changing the GPIO or LVD registers.
    pub fn new_pin<P: InputPin>(
        peripheral: Peri<'d, LVD>,
        pin: Peri<'d, P>,
        threshold: Threshold,
    ) -> Result<Self, Error> {
        critical_section::with(|_| {
            check_idle()?;
            let mut input = Flex::new(pin);
            input.set_as_analog();
            configure(P::SELECTOR, threshold)?;
            Ok(Self {
                _peripheral: peripheral,
                _input: Some(input),
                threshold,
            })
        })
    }
    /// Selected chip supply name, for the supply constructor.
    pub const fn supply_name() -> &'static str {
        crate::LVD_SUPPLY_NAME
    }
    /// Selected nominal threshold. This is not a measured supply voltage.
    pub fn threshold(&self) -> Threshold {
        self.threshold
    }
    /// Sample the comparator level once. Startup/settling is caller-qualified.
    pub fn sample(&self) -> VoltageState {
        if pac::LVD.sr().read().fltv() {
            VoltageState::BelowThreshold
        } else {
            VoltageState::AboveThreshold
        }
    }
    /// Wait at most `polls` SR reads for a comparator state; this is not a duration.
    /// This does not debounce, acknowledge events, or ensure analog settling.
    pub fn wait_for(&self, state: VoltageState, polls: u32) -> Result<(), Error> {
        if polls == 0 {
            return Err(Error::InvalidPollBudget);
        }
        for _ in 0..polls {
            if self.sample() == state {
                return Ok(());
            }
            core::hint::spin_loop();
        }
        Err(Error::Timeout)
    }
}
impl Drop for LowVoltageMonitor<'_> {
    fn drop(&mut self) {
        critical_section::with(|_| pac::LVD.cr0().modify(|w| w.set_en(false)));
    }
}

fn check_idle() -> Result<(), Error> {
    let c0 = pac::LVD.cr0().read();
    let c1 = pac::LVD.cr1().read();
    if c0.en() || c0.action() || c1.level() || c1.rise() || c1.fall() {
        return Err(Error::AlreadyConfigured);
    }
    #[cfg(lvd_low)]
    if c1.ie() || c1.flttime() != 0 || pac::SYSCTRL.cr2().read().lvdbrken() {
        return Err(Error::AlreadyConfigured);
    }
    #[cfg(not(lvd_low))]
    if c0.ie() || c1.flten() {
        return Err(Error::AlreadyConfigured);
    }
    Ok(())
}
fn configure(source: u8, threshold: Threshold) -> Result<(), Error> {
    pac::LVD.cr0().modify(|w| {
        #[cfg(lvd_low)]
        w.set_source(source != 0);
        #[cfg(not(lvd_low))]
        w.set_source(source);
        w.set_vth(threshold.0);
        w.set_en(true);
    });
    let r = pac::LVD.cr0().read();
    #[cfg(lvd_low)]
    let actual = u8::from(r.source());
    #[cfg(not(lvd_low))]
    let actual = r.source();
    if !r.en() || r.vth() != threshold.0 || actual != source {
        // This attempt exclusively owns a previously disabled EN bit.
        pac::LVD.cr0().modify(|w| w.set_en(false));
        return Err(Error::WriteFailure);
    }
    Ok(())
}
pub(crate) trait SealedInputPin {
    const SELECTOR: u8;
}
/// A package-bonded, source-reviewed analog input of the selected LVD.
#[allow(private_bounds)]
pub trait InputPin: Pin + SealedInputPin {}
#[allow(unused_macros)]
macro_rules! impl_input_pin {
    ($pin:ident, $selector:expr) => {
        impl crate::lvd::SealedInputPin for crate::peripherals::$pin {
            const SELECTOR: u8 = $selector;
        }
        impl crate::lvd::InputPin for crate::peripherals::$pin {}
    };
}
#[allow(unused_imports)]
pub(crate) use impl_input_pin;
