//! Shared per-bank state for qualified coalesced counter-overflow events.
use super::{CounterRegisters, OverflowInstance};
use crate::interrupt::typelevel::Handler;
use core::{cell::Cell, marker::PhantomData};
use critical_section::Mutex;
use embassy_sync::waitqueue::AtomicWaker;

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

/// Acknowledge only this timer's overflow and retain one coalesced event.
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
            let regs = T::COUNTER_REGS;
            if regs.counter_overflow_interrupt_enabled() && regs.counter_overflow() {
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
