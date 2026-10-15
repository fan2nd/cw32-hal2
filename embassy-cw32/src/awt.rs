//! Automatic wake-up timer, using frozen HSIOSC or an owned LSE in run mode.
//!
//! This driver does not configure low-speed oscillators or deep-sleep wake.
//! HSIOSC remains enabled by RCC for the whole boot. No oscillator, reset or
//! shared bus clock is changed on drop. Periodic events coalesce; this is not
//! an Embassy monotonic time driver. See `docs/low-power-timers.md`.
//!
//! LSE is available only on CW32F020C6U7, CW32F030C8T7, CW32A030C8T7,
//! CW32L031C8T6/C8U6/F8U6, CW32R031C8U6. Its existing
//! source capability owns SYSCTRL and the oscillator pads. Source acquisition
//! or reacquisition may temporarily open the whole GPIOC gate to inspect pads;
//! retained filters and armed edges can advance even before an error. AWT's
//! readiness query does not perform that inspection. An active RTC consumes
//! its own clock capability; this API does not add shared RTC/AWT ownership.
//!
//! LSE health is sampled at constructor admission, not continuously monitored
//! by this driver. Source loss does not wake a pending wait, and retained
//! underflows remain consumable after loss. A deadline needs an independently
//! waking source. Counting-source loss alone does not disable PCLK register
//! access, but no CPU progress or system-clock fault recovery is promised.
//!
//! On L031/R031, retained GPIOA/B/C/F filters can select AWT underflow.
//! Reconfiguring, starting, stopping or dropping this producer, or losing its
//! source, can change or stall those filters and their interrupt delivery.
//! The driver neither inspects nor owns those recipients.
use crate::{
    Peri, PeripheralType,
    interrupt::typelevel::{Binding, Handler, Interrupt},
    mode::{Async, Blocking, Mode},
    pac,
    rcc::{ClockBounds, RccPeripheral},
    time::Hertz,
};
use core::cell::Cell;
use core::{future::poll_fn, marker::PhantomData, task::Poll};
use critical_section::Mutex;
use embassy_sync::waitqueue::AtomicWaker;

/// Hardware prescaler. The reserved zero encoding is rejected by constructors.
pub use pac::awt::vals::Prescaler;

/// Timer configuration. A period contains `reload + 1` counter clocks.
#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub prescaler: Prescaler,
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
    /// RCC initialization has not succeeded.
    ClockNotInitialized,
    /// The timer's configuration clock did not enable.
    ClockGate,
    /// Reset is asserted. The driver never releases foreign reset ownership.
    HeldInReset,
    /// AWT prescaler encoding zero is reserved.
    InvalidPrescaler,
    /// The owned LSE source was observed unhealthy at constructor admission.
    #[cfg(awt_lse)]
    ClockNotReady,
}

pub(crate) struct State {
    active: Mutex<Cell<bool>>,
    elapsed: Mutex<Cell<bool>>,
    waker: AtomicWaker,
}
impl State {
    pub(crate) const fn new() -> Self {
        Self {
            active: Mutex::new(Cell::new(false)),
            elapsed: Mutex::new(Cell::new(false)),
            waker: AtomicWaker::new(),
        }
    }
}
pub(crate) mod sealed {
    use super::*;
    pub(crate) trait Instance: RccPeripheral {
        fn regs() -> pac::awt::Awt;
        fn state() -> &'static State;
    }
}
/// An owned AWT instance with a source-reviewed interrupt route.
#[allow(private_bounds)]
pub trait Instance: PeripheralType + sealed::Instance + 'static {
    type Interrupt: Interrupt;
}

/// Bind the AWT vector to this handler for asynchronous waits.
pub struct InterruptHandler<T: Instance>(PhantomData<T>);
impl<T: Instance> Handler<T::Interrupt> for InterruptHandler<T> {
    unsafe fn on_interrupt() {
        let wake = critical_section::with(|cs| {
            let state = T::state();
            if !state.active.borrow(cs).get() {
                return false;
            }
            if T::regs().ier().read().ud() && T::regs().isr().read().ud() {
                clear_event::<T>();
                state.elapsed.borrow(cs).set(true);
                true
            } else {
                false
            }
        });
        if wake {
            T::state().waker.wake();
        }
    }
}

/// Periodic downcounter. Creating it configures and stops this owned instance.
/// Drop stops only the owned timer and masks its event. Forgetting the owner
/// leaves hardware running; the ISR holds no reference into this object.
/// Source enables and pad reservations survive Drop, errors and forgetting.
/// A successful reconstruction deliberately discards the old pending event.
pub struct Awt<'d, T: Instance, M: Mode = Blocking> {
    _peri: Peri<'d, T>,
    _mode: PhantomData<M>,
    config: Config,
    tick: ClockBounds,
    #[cfg(awt_lse)]
    lse: Option<crate::rcc::LseClock<'d>>,
}
impl<'d, T: Instance> Awt<'d, T, Blocking> {
    pub fn new(peri: Peri<'d, T>, config: Config) -> Result<Self, Error> {
        Self::new_inner(
            peri,
            config,
            false,
            #[cfg(awt_lse)]
            None,
        )
    }
    /// Configure a stopped AWT while retaining the qualified LSE owner.
    /// Readiness is sampled once before AWT/gate writes; successful return does
    /// not guarantee that the source is still healthy. No source is repaired,
    /// stopped or released on error. Reborrow tokens when retry is required.
    #[cfg(awt_lse)]
    pub fn new_lse(
        peri: Peri<'d, T>,
        clock: crate::rcc::LseClock<'d>,
        config: Config,
    ) -> Result<Self, Error> {
        Self::new_inner(peri, config, false, Some(clock))
    }
}
impl<'d, T: Instance> Awt<'d, T, Async> {
    pub fn new_async(
        peri: Peri<'d, T>,
        _irq: impl Binding<T::Interrupt, InterruptHandler<T>>,
        config: Config,
    ) -> Result<Self, Error> {
        let this = Self::new_inner(
            peri,
            config,
            true,
            #[cfg(awt_lse)]
            None,
        )?;
        // Never unpend or disable a shared vector.
        unsafe {
            T::Interrupt::enable();
        }
        Ok(this)
    }
    /// Configure a stopped IRQ-driven AWT, retaining the qualified LSE owner.
    /// Readiness is sampled once before AWT/gate writes, not guaranteed at
    /// return. Source loss itself does not wake `wait`; an independent wake is
    /// needed for a deadline. Errors retain the source, pads and fault flags.
    #[cfg(awt_lse)]
    pub fn new_async_lse(
        peri: Peri<'d, T>,
        _irq: impl Binding<T::Interrupt, InterruptHandler<T>>,
        clock: crate::rcc::LseClock<'d>,
        config: Config,
    ) -> Result<Self, Error> {
        let this = Self::new_inner(peri, config, true, Some(clock))?;
        // Never unpend or disable a shared vector.
        unsafe {
            T::Interrupt::enable();
        }
        Ok(this)
    }
    /// Wait for a retained underflow event. Multiple underflows coalesce.
    /// Cancellation leaves the timer and pending event intact.
    /// A retained event remains consumable after source loss. With no event,
    /// the wait can remain pending indefinitely; loss itself does not wake it.
    pub async fn wait(&mut self) {
        poll_fn(|cx| {
            T::state().waker.register(cx.waker());
            if self.take_elapsed() {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        })
        .await
    }
}
impl<'d, T: Instance, M: Mode> Awt<'d, T, M> {
    fn new_inner(
        peri: Peri<'d, T>,
        config: Config,
        asynchronous: bool,
        #[cfg(awt_lse)] lse: Option<crate::rcc::LseClock<'d>>,
    ) -> Result<Self, Error> {
        let tick = validate(
            config,
            #[cfg(awt_lse)]
            lse.as_ref(),
        )?;
        let source = pac::awt::vals::Source::Hsiosc;
        #[cfg(awt_lse)]
        let source = match &lse {
            Some(clock) => {
                if !clock.is_ready() {
                    return Err(Error::ClockNotReady);
                }
                pac::awt::vals::Source::Lse
            }
            None => source,
        };
        critical_section::with(|cs| {
            T::RCC_INFO
                .enable_with_cs(cs)
                .map_err(|_| Error::ClockGate)?;
            if T::RCC_INFO.reset_asserted() {
                return Err(Error::HeldInReset);
            }
            let r = T::regs();
            r.cr().modify(|w| w.set_en(false));
            r.ier().write(|w| w.set_ud(false));
            write_config::<T>(config, source);
            clear_event::<T>();
            T::state().elapsed.borrow(cs).set(false);
            T::state().active.borrow(cs).set(asynchronous);
            r.ier().write(|w| w.set_ud(asynchronous));
            Ok(())
        })?;
        Ok(Self {
            _peri: peri,
            _mode: PhantomData,
            config,
            tick,
            #[cfg(awt_lse)]
            lse,
        })
    }
    /// Restart from ARR, discarding a previous pending event.
    pub fn start(&mut self) {
        critical_section::with(|cs| {
            T::regs().cr().modify(|w| w.set_en(false));
            clear_event::<T>();
            T::state().elapsed.borrow(cs).set(false);
            T::regs().cr().modify(|w| w.set_en(true));
        });
    }
    /// Stop counting without clearing a pending underflow.
    pub fn stop(&mut self) {
        T::regs().cr().modify(|w| w.set_en(false));
    }
    /// Read EN, not source health or counting progress.
    pub fn is_running(&self) -> bool {
        T::regs().cr().read().en()
    }
    /// Stop and change period/divider. Call `start` to restart from the new ARR.
    /// Retains the selected source and clears old events, even if unhealthy.
    pub fn set_config(&mut self, config: Config) -> Result<(), Error> {
        let tick = validate(
            config,
            #[cfg(awt_lse)]
            self.lse.as_ref(),
        )?;
        let source = pac::awt::vals::Source::Hsiosc;
        #[cfg(awt_lse)]
        let source = if self.lse.is_some() {
            pac::awt::vals::Source::Lse
        } else {
            source
        };
        critical_section::with(|cs| {
            self.stop();
            write_config::<T>(config, source);
            clear_event::<T>();
            T::state().elapsed.borrow(cs).set(false);
        });
        self.config = config;
        self.tick = tick;
        Ok(())
    }
    /// Read the current downcounter. This is a sample, not elapsed wall time.
    pub fn counter(&self) -> u16 {
        T::regs().cnt().read().cnt()
    }
    /// Consume one coalesced event, including an event delivered to the ISR.
    /// An event does not establish current source health or elapsed wall time.
    pub fn take_elapsed(&mut self) -> bool {
        critical_section::with(|cs| {
            let latched = T::state().elapsed.borrow(cs).replace(false);
            let hardware = T::regs().isr().read().ud();
            if hardware {
                clear_event::<T>();
            }
            latched || hardware
        })
    }
    /// Snapshot the selected source's readiness, without changing timer/events.
    /// LSE uses the held capability's enabled/stable/configuration/fault checks;
    /// HSIOSC samples its enable and STABLE bits. This is not a frequency
    /// measurement or promise of a later edge. No GPIO gate is opened, source
    /// fault flag cleared, fault interrupt installed or pending wait awakened.
    #[cfg(awt_lse)]
    pub fn is_clock_ready(&self) -> bool {
        match &self.lse {
            Some(clock) => clock.is_ready(),
            None => pac::SYSCTRL.cr1().read().hsien() && pac::SYSCTRL.hsi().read().stable(),
        }
    }
    /// Declared nominal counter clock, not a measured rate.
    /// HSIOSC is not the divided HSI/PCLK clock.
    pub fn frequency(&self) -> Hertz {
        self.tick.nominal()
    }
    /// Declared healthy-source envelope; source loss invalidates progress claims.
    pub fn clock_bounds(&self) -> ClockBounds {
        self.tick
    }
    /// Qualified steady-state period envelope, excluding interrupt latency.
    /// Conditional on the source remaining healthy and its board/electrical
    /// qualifications; this does not bound a wait after clock loss.
    pub fn period_bounds_ns(&self) -> (u64, u64) {
        let cycles = u32::from(self.config.reload) + 1;
        (
            self.tick.minimum_duration_ns(cycles),
            self.tick.maximum_duration_ns(cycles),
        )
    }
}
impl<T: Instance, M: Mode> Drop for Awt<'_, T, M> {
    fn drop(&mut self) {
        critical_section::with(|cs| {
            self.stop();
            T::regs().ier().write(|w| w.set_ud(false));
            T::state().active.borrow(cs).set(false);
        });
    }
}
fn validate(
    config: Config,
    #[cfg(awt_lse)] lse: Option<&crate::rcc::LseClock<'_>>,
) -> Result<ClockBounds, Error> {
    if config.prescaler.to_bits() == 0 {
        return Err(Error::InvalidPrescaler);
    }
    crate::rcc::try_clocks().ok_or(Error::ClockNotInitialized)?;
    let source = ClockBounds::hsi(1);
    #[cfg(awt_lse)]
    let source = lse.map_or(source, |clock| clock.bounds());
    Ok(source.divided_by(1 << config.prescaler.to_bits()))
}
fn write_config<T: Instance>(config: Config, source: pac::awt::vals::Source) {
    T::regs().cr().write(|w| {
        w.set_src(source);
        w.set_md(pac::awt::vals::Mode::Timer);
        w.set_prs(config.prescaler);
        w.set_en(false);
    });
    T::regs().arr().write(|w| w.set_arr(config.reload));
}
fn clear_event<T: Instance>() {
    let mut value = pac::awt::regs::Icr::write_noop();
    value.set_ud(false);
    T::regs().icr().write_value(value);
}
macro_rules! impl_instance {
    ($inst:ident, $irq:ident) => {
        impl $crate::awt::sealed::Instance for $crate::peripherals::$inst {
            fn regs() -> $crate::pac::awt::Awt {
                $crate::pac::$inst
            }
            fn state() -> &'static $crate::awt::State {
                static STATE: $crate::awt::State = $crate::awt::State::new();
                &STATE
            }
        }
        impl $crate::awt::Instance for $crate::peripherals::$inst {
            type Interrupt = $crate::interrupt::typelevel::$irq;
        }
    };
}
pub(crate) use impl_instance;
