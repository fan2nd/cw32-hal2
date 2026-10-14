//! Polling low-voltage monitor with exclusive peripheral and analog-pin ownership.
//!
//! Constructors accept only an idle, unfiltered LVD with reset, interrupt and
//! event triggers disabled. BOR, reference/trim, filter clocks and shared analog
//! owners are never changed. Low-family timer-break routing is rejected while
//! active. The hardware has no independent RCC gate; no gate or reset operation
//! is synthesized. Drop retires an owned L012 edge wait, then disables EN.
//!
//! Thresholds are nominal, not calibrated voltmeter readings. L012's documented
//! table assumes Vcore=1.6 V. Allow board-qualified analog startup/settling time
//! before interpreting samples; this driver does not claim a ready indication.
//! On L012 only, edge waits report a fresh interrupt event, not analog readiness.
//! No wakeup, reset-policy or filter API is provided.
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

/// Owned, unfiltered LVD monitor; L012 additionally supports edge notifications.
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
        #[cfg(cw32l012)]
        drop(critical_section::with(edge_wait::retire));
        critical_section::with(|_| {
            pac::LVD.cr0().modify(|w| w.set_en(false));
        });
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

/// L012 supply/input crossing direction, as defined by LVD RISE/FALL.
#[cfg(cw32l012)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Edge {
    /// Monitored voltage rises above its threshold (not the FLTV polarity).
    Rising,
    /// Monitored voltage falls below its threshold.
    Falling,
    Either,
}
#[cfg(cw32l012)]
pub use edge_wait::InterruptHandler;
#[cfg(cw32l012)]
impl LowVoltageMonitor<'_> {
    /// Start a fresh edge window on first poll, discarding any previous event.
    ///
    /// Completion means a sticky edge event was observed, not a settled voltage
    /// or a count of crossings. Cancellation masks this monitor's request and
    /// clears its event, without disabling monitoring. No wakeup guarantee.
    /// Forgetting the future retains no buffer pointers; Drop of the monitor or
    /// the next wait retires the abandoned request. An unpolled wait does nothing.
    pub async fn wait_for_edge(
        &mut self,
        edge: Edge,
        _irq: impl crate::interrupt::typelevel::Binding<
            crate::interrupt::typelevel::LVD,
            InterruptHandler,
        >,
    ) {
        edge_wait::wait(edge).await;
    }
}
#[cfg(cw32l012)]
mod edge_wait {
    use super::*;
    use crate::interrupt::typelevel::{Handler, Interrupt, LVD as Irq};
    use core::{
        cell::RefCell,
        future::poll_fn,
        task::{Poll, Waker},
    };
    struct State {
        active: bool,
        waker: Option<Waker>,
    }
    static STATE: critical_section::Mutex<RefCell<State>> =
        critical_section::Mutex::new(RefCell::new(State {
            active: false,
            waker: None,
        }));

    /// Handler for the source-qualified L012 LVD vector.
    pub struct InterruptHandler;
    impl Handler<Irq> for InterruptHandler {
        unsafe fn on_interrupt() {
            let wake = critical_section::with(|cs| {
                let mut state = STATE.borrow(cs).borrow_mut();
                // Inactive dispatch must not even read peripheral registers.
                if !state.active {
                    return None;
                }
                if pac::LVD.cr1().read().ie() && pac::LVD.sr().read().intf() {
                    pac::LVD.cr1().modify(|w| w.set_ie(false));
                    state.waker.take()
                } else {
                    None
                }
            });
            if let Some(waker) = wake {
                waker.wake();
            }
        }
    }
    fn clear_event() {
        // L012 UM 1.4 §28.7: INTF is the sole RW0 bit; FLTV is RO,
        // reserved bits require zero. Typed command, never status RMW.
        pac::LVD.sr().write_value(pac::lvd::regs::Sr(0));
    }
    pub(super) fn retire(cs: critical_section::CriticalSection<'_>) -> Option<Waker> {
        let mut state = STATE.borrow(cs).borrow_mut();
        if state.active {
            pac::LVD.cr1().modify(|w| {
                w.set_ie(false);
                w.set_rise(false);
                w.set_fall(false);
            });
            clear_event();
        }
        state.active = false;
        state.waker.take()
    }
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            drop(critical_section::with(retire));
        }
    }
    pub(super) async fn wait(edge: Edge) {
        drop(critical_section::with(retire));
        critical_section::with(|cs| {
            pac::LVD.cr1().modify(|w| {
                w.set_ie(false);
                w.set_level(false);
                w.set_rise(edge != Edge::Falling);
                w.set_fall(edge != Edge::Rising);
            });
            clear_event();
            STATE.borrow(cs).borrow_mut().active = true;
        });
        let _guard = Guard;
        poll_fn(|cx| {
            // RawWaker callbacks may be arbitrary code: clone/drop outside CS.
            let mut next_waker = Some(cx.waker().clone());
            let (poll, previous) = critical_section::with(|cs| {
                let mut state = STATE.borrow(cs).borrow_mut();
                if pac::LVD.sr().read().intf() {
                    return (Poll::Ready(()), None);
                }
                let previous = core::mem::replace(&mut state.waker, next_waker.take());
                pac::LVD.cr1().modify(|w| w.set_ie(true));
                unsafe {
                    Irq::enable();
                }
                (Poll::Pending, previous)
            });
            drop(previous);
            poll
        })
        .await;
    }
}
