//! Low-power timer, using qualified PCLK in run mode.
//!
//! Owns the complete timer but never disables/unpends its NVIC vector: L052
//! and L083 share that vector with BTIM2. Bind every enabled source on a shared
//! vector. IER is configured only while EN=0, as required by CW32. The handler
//! clears only ARRM and latches a coalesced event; it never changes IER.
//!
//! No low-speed oscillator, external trigger, pin output, deep-sleep wake or
//! Embassy time-driver support is implied. Startup has documented clock-domain
//! delays; counter samples are accepted only after two equal consecutive reads.
use crate::{
    Peri, PeripheralType,
    interrupt::typelevel::{Binding, Handler, Interrupt},
    mode::{Async, Blocking, Mode},
    pac,
    rcc::{ClockBounds, RccPeripheral},
    time::Hertz,
};
use core::{cell::Cell, future::poll_fn, marker::PhantomData, task::Poll};
use critical_section::Mutex;
use embassy_sync::waitqueue::AtomicWaker;

pub use pac::lptim::vals::Prescaler;
/// Raw timer period configuration. No wall-clock deadline is inferred from ARR.
#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub prescaler: Prescaler,
    /// Nonzero match value. Zero-period behavior is intentionally unsupported.
    pub reload: u16,
    /// Maximum reads for each synchronization or stable-counter operation.
    /// This is a polling budget, not a duration.
    pub poll_budget: u32,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            prescaler: Prescaler::Div128,
            reload: u16::MAX,
            poll_budget: 100_000,
        }
    }
}
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {
    ClockNotInitialized,
    ClockGate,
    HeldInReset,
    InvalidConfig,
    /// Register update or counter reset failed to acknowledge. Timer is stopped.
    SynchronizationTimeout,
    /// Two equal consecutive counter reads were not observed within the budget.
    UnstableCounter,
    /// An earlier synchronization error requires reconstructing this instance.
    Faulted,
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
        fn regs() -> pac::lptim::Lptim;
        fn state() -> &'static State;
    }
}
#[allow(private_bounds)]
pub trait Instance: PeripheralType + sealed::Instance + 'static {
    type Interrupt: Interrupt;
}
/// Clears only this timer's ARRM event. Service foreign shared-vector sources
/// with their own handlers in the same `bind_interrupts!` invocation.
pub struct InterruptHandler<T: Instance>(PhantomData<T>);
impl<T: Instance> Handler<T::Interrupt> for InterruptHandler<T> {
    unsafe fn on_interrupt() {
        let wake = critical_section::with(|cs| {
            let state = T::state();
            if !state.active.borrow(cs).get() {
                return false;
            }
            if T::regs().ier().read().arrm() && T::regs().isr().read().arrm() {
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
/// PCLK-backed upward counter with periodic or hardware single-shot starts.
pub struct Lptim<'d, T: Instance, M: Mode = Blocking> {
    _peri: Peri<'d, T>,
    _mode: PhantomData<M>,
    config: Config,
    tick: ClockBounds,
    faulted: bool,
}
impl<'d, T: Instance> Lptim<'d, T, Blocking> {
    pub fn new(peri: Peri<'d, T>, config: Config) -> Result<Self, Error> {
        Self::new_inner(peri, config, false)
    }
}
impl<'d, T: Instance> Lptim<'d, T, Async> {
    pub fn new_async(
        peri: Peri<'d, T>,
        _irq: impl Binding<T::Interrupt, InterruptHandler<T>>,
        config: Config,
    ) -> Result<Self, Error> {
        let this = Self::new_inner(peri, config, true)?;
        unsafe {
            T::Interrupt::enable();
        }
        Ok(this)
    }
    /// Consume a retained match event. Multiple events coalesce. Cancelling the
    /// wait leaves counting, interrupt enable and a pending event unchanged.
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
impl<'d, T: Instance, M: Mode> Lptim<'d, T, M> {
    fn new_inner(peri: Peri<'d, T>, config: Config, asynchronous: bool) -> Result<Self, Error> {
        let tick = validate::<T>(config)?;
        critical_section::with(|cs| {
            // RCC_INFO resets only this independently owned LPTIM field, never
            // the BTIM/ADC/AWT resources that can share another reset or IRQ.
            T::RCC_INFO
                .enable_and_reset_with_cs(cs)
                .map_err(|_| Error::ClockGate)?;
            if T::RCC_INFO.reset_asserted() {
                return Err(Error::HeldInReset);
            }
            stop::<T>();
            T::regs().ier().write(|w| w.set_arrm(false));
            T::state().active.borrow(cs).set(false);
            T::state().elapsed.borrow(cs).set(false);
            Ok(())
        })?;
        let mut this = Self {
            _peri: peri,
            _mode: PhantomData,
            config,
            tick,
            faulted: false,
        };
        this.configure()?;
        critical_section::with(|cs| {
            // configure() returns with EN=0, so this IER write is legal.
            T::regs().ier().write(|w| w.set_arrm(asynchronous));
            T::state().active.borrow(cs).set(asynchronous);
        });
        Ok(this)
    }
    fn configure(&mut self) -> Result<(), Error> {
        stop::<T>();
        let r = T::regs();
        r.cfgr().write(|w| {
            w.set_iclksrc(pac::lptim::vals::Source::Pclk);
            w.set_prs(self.config.prescaler);
            w.set_trigen(pac::lptim::vals::Trigger::Software);
            w.set_cksel(false);
            w.set_countmd(false);
            w.set_preload(false);
        });
        self.enable();
        let mut value = pac::lptim::regs::Icr::write_noop();
        value.set_arrok(false);
        r.icr().write_value(value);
        r.arr().write(|w| w.set_arr(self.config.reload));
        let result = self.poll(|| r.isr().read().arrok());
        stop::<T>();
        if result.is_err() {
            self.faulted = true;
        }
        result
    }
    fn enable(&self) {
        let r = T::regs();
        #[cfg(lptim_cr0)]
        let cr = r.cr0();
        #[cfg(not(lptim_cr0))]
        let cr = r.cr();
        #[cfg(lptim_cr0)]
        let mut value = pac::lptim::regs::Cr0::write_noop();
        #[cfg(not(lptim_cr0))]
        let mut value = pac::lptim::regs::Cr::write_noop();
        value.set_en(true);
        cr.write_value(value);
        // Own manuals require two counter-clock cycles after EN. Endpoints
        // are conservative for the common frozen source and integer divisors.
        // On L083, HCLK and PCLK-derived LPTIM retain the same PLL reference;
        // this relative cycle wait makes no absolute nanosecond/jitter claim.
        // Keep exact source fractions: a valid divided tick can be below 1 Hz.
        let cpu = crate::rcc::try_clocks().unwrap().hclk_bounds();
        let cycles = cpu.cycles_for(self.tick, 2);
        cortex_m::asm::delay(cycles as u32);
    }
    fn poll(&self, mut ready: impl FnMut() -> bool) -> Result<(), Error> {
        for _ in 0..self.config.poll_budget {
            if ready() {
                return Ok(());
            }
            core::hint::spin_loop();
        }
        Err(Error::SynchronizationTimeout)
    }
    fn start_inner(&mut self, single: bool) -> Result<(), Error> {
        if self.faulted {
            return Err(Error::Faulted);
        }
        self.stop();
        self.enable();
        let r = T::regs();
        #[cfg(lptim_cr0)]
        let cr = r.cr0();
        #[cfg(not(lptim_cr0))]
        let cr = r.cr();
        let reset = (|| {
            self.poll(|| cr.read().srst() && cr.read().arst())?;
            #[cfg(lptim_cr0)]
            let mut value = pac::lptim::regs::Cr0::write_noop();
            #[cfg(not(lptim_cr0))]
            let mut value = pac::lptim::regs::Cr::write_noop();
            value.set_en(true);
            value.set_srst(false);
            cr.write_value(value);
            self.poll(|| cr.read().srst())
        })();
        if let Err(error) = reset {
            self.stop();
            self.faulted = true;
            return Err(error);
        }
        critical_section::with(|cs| {
            clear_event::<T>();
            T::state().elapsed.borrow(cs).set(false);
            #[cfg(lptim_cr0)]
            let mut value = pac::lptim::regs::Cr0::write_noop();
            #[cfg(not(lptim_cr0))]
            let mut value = pac::lptim::regs::Cr::write_noop();
            value.set_en(true);
            value.set_sngstart(single);
            value.set_cntstart(!single);
            cr.write_value(value);
        });
        Ok(())
    }
    /// Restart from counter zero in continuous mode, clearing a prior event.
    /// The documented start synchronization latency is not part of ARR.
    pub fn start(&mut self) -> Result<(), Error> {
        self.start_inner(false)
    }
    /// Restart from zero and stop counting in hardware on the ARR match.
    /// The timer core stays enabled until `stop`; the match flag is retained.
    pub fn start_once(&mut self) -> Result<(), Error> {
        self.start_inner(true)
    }
    /// Disable the timer core without consuming a pending match event.
    pub fn stop(&mut self) {
        stop::<T>();
    }
    /// Core enable state, not a proof that single-shot counting is still active.
    pub fn is_enabled(&self) -> bool {
        #[cfg(lptim_cr0)]
        let cr = T::regs().cr0();
        #[cfg(not(lptim_cr0))]
        let cr = T::regs().cr();
        cr.read().en()
    }
    /// Stop, reconfigure and leave stopped. Synchronization errors latch a
    /// fault; drop and reconstruct using a reborrowed token to recover by reset.
    pub fn set_config(&mut self, config: Config) -> Result<(), Error> {
        if self.faulted {
            return Err(Error::Faulted);
        }
        let tick = validate::<T>(config)?;
        self.config = config;
        self.tick = tick;
        self.configure()?;
        critical_section::with(|cs| {
            clear_event::<T>();
            T::state().elapsed.borrow(cs).set(false);
        });
        Ok(())
    }
    pub fn take_elapsed(&mut self) -> bool {
        critical_section::with(|cs| {
            let latched = T::state().elapsed.borrow(cs).replace(false);
            let hardware = T::regs().isr().read().arrm();
            if hardware {
                clear_event::<T>();
            }
            latched || hardware
        })
    }
    /// Bounded synchronized read; PCLK/div1 may never yield equal samples while
    /// running. Choose a slower divider or stop before reading in that case.
    pub fn counter(&self) -> Result<u16, Error> {
        let mut previous = T::regs().cnt().read().cnt();
        for _ in 0..self.config.poll_budget {
            let next = T::regs().cnt().read().cnt();
            if next == previous {
                return Ok(next);
            }
            previous = next;
        }
        Err(Error::UnstableCounter)
    }
    pub fn frequency(&self) -> Hertz {
        self.tick.nominal()
    }
    pub fn clock_bounds(&self) -> ClockBounds {
        self.tick
    }
}
impl<T: Instance, M: Mode> Drop for Lptim<'_, T, M> {
    fn drop(&mut self) {
        critical_section::with(|cs| {
            stop::<T>();
            T::regs().ier().write(|w| w.set_arrm(false));
            T::state().active.borrow(cs).set(false);
        });
    }
}
fn validate<T: Instance>(config: Config) -> Result<ClockBounds, Error> {
    if config.reload == 0 || config.poll_budget == 0 {
        return Err(Error::InvalidConfig);
    }
    let source = crate::rcc::bus_clock_bounds::<T>().ok_or(Error::ClockNotInitialized)?;
    Ok(source.divided_by(1 << config.prescaler.to_bits()))
}
fn stop<T: Instance>() {
    #[cfg(lptim_cr0)]
    T::regs()
        .cr0()
        .write_value(pac::lptim::regs::Cr0::write_noop());
    #[cfg(not(lptim_cr0))]
    T::regs()
        .cr()
        .write_value(pac::lptim::regs::Cr::write_noop());
}
fn clear_event<T: Instance>() {
    let mut value = pac::lptim::regs::Icr::write_noop();
    value.set_arrm(false);
    T::regs().icr().write_value(value);
}
macro_rules! impl_instance {
    ($inst:ident, $irq:ident) => {
        impl $crate::lptim::sealed::Instance for $crate::peripherals::$inst {
            fn regs() -> $crate::pac::lptim::Lptim {
                $crate::pac::$inst
            }
            fn state() -> &'static $crate::lptim::State {
                static STATE: $crate::lptim::State = $crate::lptim::State::new();
                &STATE
            }
        }
        impl $crate::lptim::Instance for $crate::peripherals::$inst {
            type Interrupt = $crate::interrupt::typelevel::$irq;
        }
    };
}
pub(crate) use impl_instance;
