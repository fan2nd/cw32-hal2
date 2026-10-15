//! Polling low-voltage monitor with exclusive peripheral and analog-pin ownership.
//!
//! Constructors accept only an idle, unfiltered LVD with reset, interrupt and
//! event triggers disabled. BOR, reference/trim and shared analog owners are never
//! changed; only explicit filtered constructors select SYSCLK filtering. Low-family timer-break routing is rejected while
//! active. The hardware has no independent RCC gate; no gate or reset operation
//! is synthesized. Drop retires an owned native edge wait, then disables EN.
//!
//! Thresholds are nominal, not calibrated voltmeter readings. L012's documented
//! table assumes Vcore=1.6 V. Allow board-qualified analog startup/settling time
//! before interpreting samples; this driver does not claim a ready indication.
//! On L010/L011/L012, edge waits report a fresh interrupt event, not analog readiness.
//! L010/L011/L012 optionally select a SYSCLK digital filter at construction.
//! No wakeup or reset-policy API is provided.
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
    /// Filtered monitoring requires initialized, continuously running SYSCLK.
    #[cfg(any(cw32l010, cw32l011, cw32l012))]
    ClockNotInitialized,
    /// An existing monitor, reset/IRQ/event/filter policy or timer break is active.
    AlreadyConfigured,
    /// A requested source, threshold or enable bit did not read back correctly.
    WriteFailure,
    /// A zero register-poll budget is invalid.
    InvalidPollBudget,
    /// The requested state was not sampled within the specified poll budget.
    Timeout,
}

/// Owned LVD monitor; native families support SYSCLK filtering and edge waits.
pub struct LowVoltageMonitor<'d> {
    _peripheral: Peri<'d, LVD>,
    _input: Option<Flex<'d>>,
    threshold: Threshold,
    #[cfg(any(cw32l010, cw32l011, cw32l012))]
    filter_clock_restore: Option<bool>,
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
            #[cfg(any(cw32l010, cw32l011, cw32l012))]
            filter_clock_restore: None,
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
                #[cfg(any(cw32l010, cw32l011, cw32l012))]
                filter_clock_restore: None,
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
    /// This adds no software debounce, event acknowledgment or analog-settling proof.
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
        #[cfg(any(cw32l010, cw32l011, cw32l012))]
        drop(critical_section::with(edge_wait::retire));
        critical_section::with(|_| {
            pac::LVD.cr0().modify(|w| w.set_en(false));
            #[cfg(any(cw32l010, cw32l011, cw32l012))]
            if let Some(previous) = self.filter_clock_restore {
                pac::LVD.cr1().modify(|w| w.set_flttime(0));
                pac::LVD.cr0().modify(|w| w.set_fltclk(previous));
            }
        });
    }
}

/// L010/L011/L012 consecutive-equal-sample filter, clocked from continuously running SYSCLK.
/// DivN is the sampling divider; SamplesN is the required stable sample count.
/// Encodings follow each own manual; see `docs/native-lvd-filter.md`. They do not guarantee analog settling,
/// glitch rejection in seconds, startup output, or operation during clock loss.
#[cfg(any(cw32l010, cw32l011, cw32l012))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum DigitalFilter {
    Div1Samples2 = 1,
    Div1Samples4 = 2,
    Div1Samples8 = 3,
    Div2Samples6 = 4,
    Div2Samples8 = 5,
    Div4Samples6 = 6,
    Div4Samples8 = 7,
    Div8Samples6 = 8,
    Div8Samples8 = 9,
    Div16Samples5 = 10,
    Div16Samples6 = 11,
    Div16Samples8 = 12,
    Div32Samples5 = 13,
    Div32Samples6 = 14,
    Div32Samples8 = 15,
}

#[cfg(any(cw32l010, cw32l011, cw32l012))]
impl<'d> LowVoltageMonitor<'d> {
    /// Configure SYSCLK filtering before enabling supply monitoring. Does not
    /// select reset, level/edge actions, IRQ or timer break. SYSCLK must remain
    /// running; no low-power/source-loss or elapsed-time guarantee is provided.
    pub fn new_supply_filtered(
        peripheral: Peri<'d, LVD>,
        threshold: Threshold,
        filter: DigitalFilter,
    ) -> Result<Self, Error> {
        if crate::rcc::try_clocks().is_none() {
            return Err(Error::ClockNotInitialized);
        }
        let previous = critical_section::with(|_| {
            check_idle()?;
            configure_filtered(0, threshold, filter)
        })?;
        Ok(Self {
            _peripheral: peripheral,
            _input: None,
            threshold,
            filter_clock_restore: Some(previous),
        })
    }

    /// External-input counterpart of new_supply_filtered, retaining analog pin
    /// ownership and the same clock/filter limits. Drop disables the monitor,
    /// removes its filter and restores the inherited filter-clock selector.
    pub fn new_pin_filtered<P: InputPin>(
        peripheral: Peri<'d, LVD>,
        pin: Peri<'d, P>,
        threshold: Threshold,
        filter: DigitalFilter,
    ) -> Result<Self, Error> {
        if crate::rcc::try_clocks().is_none() {
            return Err(Error::ClockNotInitialized);
        }
        critical_section::with(|_| {
            check_idle()?;
            let mut input = Flex::new(pin);
            input.set_as_analog();
            let previous = configure_filtered(P::SELECTOR, threshold, filter)?;
            Ok(Self {
                _peripheral: peripheral,
                _input: Some(input),
                threshold,
                filter_clock_restore: Some(previous),
            })
        })
    }
}

#[cfg(any(cw32l010, cw32l011, cw32l012))]
fn configure_filtered(source: u8, threshold: Threshold, filter: DigitalFilter) -> Result<bool, Error> {
    let previous = pac::LVD.cr0().read().fltclk();
    pac::LVD.cr1().modify(|w| w.set_flttime(filter as u8));
    pac::LVD.cr0().modify(|w| w.set_fltclk(true));
    let filter_matches = pac::LVD.cr0().read().fltclk()
        && pac::LVD.cr1().read().flttime() == filter as u8;
    if !filter_matches || configure(source, threshold).is_err() {
        pac::LVD.cr0().modify(|w| w.set_en(false));
        pac::LVD.cr1().modify(|w| w.set_flttime(0));
        pac::LVD.cr0().modify(|w| w.set_fltclk(previous));
        return Err(Error::WriteFailure);
    }
    Ok(previous)
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

/// Native supply/input crossing direction, as defined by LVD RISE/FALL.
#[cfg(any(cw32l010, cw32l011, cw32l012))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Edge {
    /// Monitored voltage rises above its threshold (not the FLTV polarity).
    Rising,
    /// Monitored voltage falls below its threshold.
    Falling,
    Either,
}
#[cfg(any(cw32l010, cw32l011, cw32l012))]
pub use edge_wait::InterruptHandler;
#[cfg(any(cw32l010, cw32l011, cw32l012))]
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
#[cfg(any(cw32l010, cw32l011, cw32l012))]
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

    /// Handler for the source-qualified native LVD vector.
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
        // Own native manuals: INTF is the sole RW0 bit; FLTV is RO,
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
