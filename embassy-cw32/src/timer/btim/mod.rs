//! Private BTIM counter and overflow engine. Register maps remain actual IP-version PACs.
use super::{CounterRegisters, OverflowInstance, low_level::Timing};
use crate::{interrupt::typelevel::Handler, pac};
use core::{cell::Cell, marker::PhantomData};
use critical_section::Mutex;
use embassy_sync::waitqueue::AtomicWaker;

#[cfg(any(btim_cw32l010_v1, btim_cw32l012_v1))]
mod cw32l010_v1;
#[cfg(any(btim_cw32l031_v1, btim_cw32l052_v1))]
mod cw32l031_v1;
#[cfg(any(btim_v1, btim_cw32f002_v1))]
mod v1;

pub struct State {
    pub(super) active: Mutex<Cell<bool>>,
    pub(super) elapsed: Mutex<Cell<bool>>,
    pub(super) waker: AtomicWaker,
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

/// Acknowledge only this BTIM's overflow and retain one coalesced event.
///
/// Every enabled foreign source sharing this vector needs its own handler in
/// the same `bind_interrupts!` dispatch. This handler never accesses that source
/// or disables, unpends, or changes the priority of the complete vector.
pub struct OverflowInterruptHandler<T: OverflowInstance>(PhantomData<T>);
impl<T: OverflowInstance> Handler<T::Interrupt> for OverflowInterruptHandler<T> {
    unsafe fn on_interrupt() {
        let wake = critical_section::with(|cs| {
            let state = T::state();
            // A partner may dispatch before construction or after Drop. Do not
            // read even this bank's enable/status registers until it is active.
            if !state.active.borrow(cs).get() {
                return false;
            }
            let regs = T::OVERFLOW_REGS;
            if regs.overflow_interrupt_enabled() && regs.counter_overflow() {
                regs.counter_clear_overflow();
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

// Every member of a group shares the gate AND reset. A per-instance owner must
// never reset/gate a foreign or independently owned sibling. Keep the group
// clock enabled on drop; initialize only the selected register bank in software.
macro_rules! impl_btim {
    ($name:ident, $irq:ident) => {
        impl $crate::timer::sealed::BasicInstance for $crate::peripherals::$name {
            type CounterRegisters = $crate::pac::btim::Btim;
            const COUNTER_REGS: Self::CounterRegisters = $crate::pac::$name;
            fn overflow_state() -> Option<&'static $crate::timer::btim::State> {
                Some(<Self as $crate::timer::sealed::OverflowInstance>::state())
            }
        }
        impl $crate::timer::BasicInstance for $crate::peripherals::$name {}
        impl $crate::timer::sealed::OverflowInstance for $crate::peripherals::$name {
            const OVERFLOW_REGS: $crate::pac::btim::Btim = $crate::pac::$name;
            fn state() -> &'static $crate::timer::btim::State {
                static STATE: $crate::timer::btim::State = $crate::timer::btim::State::new();
                &STATE
            }
        }
        impl $crate::timer::OverflowInstance for $crate::peripherals::$name {
            type Interrupt = $crate::interrupt::typelevel::$irq;
        }
    };
}
pub(crate) use impl_btim;

pub(super) trait Registers: Copy {
    const FLAGS: u32;
    fn control(self) -> u32;
    fn write_control(self, value: u32);
    fn stopped_control(timing: Timing) -> u32;
    fn counter(self) -> u16;
    fn write_counter(self, value: u16);
    fn write_reload(self, value: u16);
    fn write_prescaler(self, timing: Timing);
    fn latch_prescaler(self);
    fn disable_requests(self);
    fn overflow_interrupt_enabled(self) -> bool;
    fn enable_overflow_interrupt(self);
    fn clear_inputs(self);
    fn status(self) -> u32;
    fn clear_flags(self, value: u32);
}

pub(super) fn initialize<R: Registers>(regs: R, timing: Timing) {
    regs.write_control(0);
    regs.disable_requests();
    regs.clear_inputs();
    regs.write_counter(0);
    regs.write_reload(timing.reload);
    regs.write_prescaler(timing);
    regs.write_control(R::stopped_control(timing));
    regs.latch_prescaler();
    regs.clear_flags(0); // R1W0. Clear all defined flags, reserved bits stay zero.
}

pub(super) fn reconfigure<R: Registers>(regs: R, timing: Timing) {
    let running = regs.control() & 1 != 0;
    regs.write_control(0);
    regs.write_counter(0); // ARR takes effect immediately; never reduce it below CNT.
    regs.write_reload(timing.reload);
    regs.write_prescaler(timing);
    let control = R::stopped_control(timing);
    regs.write_control(control);
    regs.latch_prescaler(); // Reset divider phase via UG on modern IP; classic IP latches on EN rising.
    regs.clear_flags(R::FLAGS & !1); // Preserve unrelated pending flags, no ISR RMW.
    if running {
        regs.write_control(control | 1);
    }
}

impl CounterRegisters for pac::btim::Btim {
    fn counter_initialize(self, timing: Timing) {
        initialize(self, timing);
    }
    fn counter_configure(self, timing: Timing) {
        reconfigure(self, timing);
    }
    fn counter_start(self, timing: Timing) {
        self.write_control(Self::stopped_control(timing) | 1);
    }
    fn counter_stop(self, timing: Timing) {
        self.write_control(Self::stopped_control(timing));
    }
    fn counter_running(self) -> bool {
        self.control() & 1 != 0
    }
    fn counter_read(self) -> u16 {
        self.counter()
    }
    fn counter_write(self, value: u16) {
        self.write_counter(value);
    }
    fn counter_overflow(self) -> bool {
        self.status() & 1 != 0
    }
    fn counter_clear_overflow(self) {
        self.clear_flags(Self::FLAGS & !1);
    }
    fn counter_disable_requests(self) {
        self.disable_requests();
    }
}
