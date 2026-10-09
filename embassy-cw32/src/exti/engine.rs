//! Per-bank state and per-pin one-shot waits using the proven selected-family PAC view.
use super::Trigger;
use crate::{gpio_block, pac};
use core::{
    cell::Cell,
    future::Future,
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
};
use critical_section::Mutex;
use embassy_sync::waitqueue::AtomicWaker;

pub(super) struct PortState {
    pub(super) active: Mutex<Cell<u16>>,
    pub(super) fired: Mutex<Cell<u16>>,
    pub(super) wakers: [AtomicWaker; 16],
}
impl PortState {
    pub(super) const fn new() -> Self {
        Self {
            active: Mutex::new(Cell::new(0)),
            fired: Mutex::new(Cell::new(0)),
            wakers: [const { AtomicWaker::new() }; 16],
        }
    }
}

static STATES: [PortState; crate::GPIO_BANK_COUNT] =
    [const { PortState::new() }; crate::GPIO_BANK_COUNT];

pub(super) fn state(bank: u8) -> &'static PortState {
    // Generated from real bank GLOBAL links; absent banks have no state slot.
    &STATES[crate::GPIO_STATE_INDEX[usize::from(bank)]]
}

pub(super) fn dispatch(banks: &[u8], mut service: impl FnMut(u8)) {
    for &bank in banks {
        service(bank);
    }
}

pub(super) fn on_interrupt(port: u8, state: &PortState) {
    let ready = critical_section::with(|cs| {
        let active = state.active.borrow(cs).get();
        if active == 0 {
            return 0;
        }
        // Construct/access the bank only after the RAM guard. An inactive
        // partner of a shared vector may have its working clock gated.
        let registers = gpio_block(port);
        let ready = registers.isr().read().0 as u16
            & active
            & crate::GPIO_SERVICED_MASKS[usize::from(port)];
        if ready != 0 {
            disable(registers, ready, cs);
            clear(registers, port, ready);
            state.active.borrow(cs).set(active & !ready);
            state
                .fired
                .borrow(cs)
                .set(state.fired.borrow(cs).get() | ready);
        }
        ready
    });
    for pin in 0..16 {
        if ready & (1 << pin) != 0 {
            state.wakers[pin].wake();
        }
    }
}

pub(super) struct WaitFuture<'a> {
    registers: pac::gpio::Gpio,
    port: u8,
    state: &'a PortState,
    pin: u8,
    trigger: Trigger,
    completed: bool,
    // Public async wrappers call new only on first poll and exclusively borrow
    // their ExtiInput until completion or cancellation.
    _borrow: PhantomData<&'a mut ()>,
}
impl<'a> WaitFuture<'a> {
    pub(super) fn new(port: u8, state: &'a PortState, pin: u8, trigger: Trigger) -> Self {
        let registers = gpio_block(port);
        let mask = 1u16 << pin;
        assert!(crate::GPIO_SERVICED_MASKS[usize::from(port)] & mask != 0);
        critical_section::with(|cs| {
            disable(registers, mask, cs);
            clear(registers, port, mask);
            state
                .fired
                .borrow(cs)
                .set(state.fired.borrow(cs).get() & !mask);
            state
                .active
                .borrow(cs)
                .set(state.active.borrow(cs).get() | mask);
            enable(registers, mask, trigger, cs);
        });
        Self {
            registers,
            port,
            state,
            pin,
            trigger,
            completed: false,
            _borrow: PhantomData,
        }
    }

    fn poll_inner(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        if self.completed {
            return Poll::Ready(());
        }
        self.state.wakers[self.pin as usize].register(cx.waker());
        let mask = 1u16 << self.pin;
        let fired = critical_section::with(|cs| self.state.fired.borrow(cs).get() & mask != 0);
        // A matching captured edge remains sufficient if its pulse has ended.
        let level_matches = !fired
            && match self.trigger {
                Trigger::High => self.registers.idr().read().0 as u16 & mask != 0,
                Trigger::Low => self.registers.idr().read().0 as u16 & mask == 0,
                _ => false,
            };
        if fired || level_matches {
            // Cleanup on Ready itself, even if an internal caller retains a
            // completed future. Native held-level sources disable before W0C.
            cancel(self.port, self.state, self.pin);
            self.completed = true;
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}
impl Future for WaitFuture<'_> {
    type Output = ();
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        self.get_mut().poll_inner(cx)
    }
}
impl Drop for WaitFuture<'_> {
    fn drop(&mut self) {
        if !self.completed {
            cancel(self.port, self.state, self.pin);
        }
    }
}

pub(super) fn cancel(port: u8, state: &PortState, pin: u8) {
    let registers = gpio_block(port);
    let mask = 1u16 << pin;
    critical_section::with(|cs| {
        disable(registers, mask, cs);
        clear(registers, port, mask);
        state
            .active
            .borrow(cs)
            .set(state.active.borrow(cs).get() & !mask);
        state
            .fired
            .borrow(cs)
            .set(state.fired.borrow(cs).get() & !mask);
    });
}

// Direct selected-family PAC accesses. The build proves offsets, widths, access
// and used fields against each real bank before constructing this common view.
fn disable(registers: pac::gpio::Gpio, mask: u16, _cs: critical_section::CriticalSection<'_>) {
    let mask = u32::from(mask);
    registers.riseie().modify(|v| v.0 &= !mask);
    registers.fallie().modify(|v| v.0 &= !mask);
    #[cfg(gpio_irq_level)]
    {
        registers.highie().modify(|v| v.0 &= !mask);
        registers.lowie().modify(|v| v.0 &= !mask);
    }
}

fn enable(
    registers: pac::gpio::Gpio,
    mask: u16,
    trigger: Trigger,
    _cs: critical_section::CriticalSection<'_>,
) {
    let mask = u32::from(mask);
    match trigger {
        Trigger::Rising => registers.riseie().modify(|v| v.0 |= mask),
        Trigger::Falling => registers.fallie().modify(|v| v.0 |= mask),
        Trigger::AnyEdge => {
            registers.riseie().modify(|v| v.0 |= mask);
            registers.fallie().modify(|v| v.0 |= mask);
        }
        #[cfg(gpio_irq_level)]
        Trigger::High => registers.highie().modify(|v| v.0 |= mask),
        #[cfg(gpio_irq_level)]
        Trigger::Low => registers.lowie().modify(|v| v.0 |= mask),
        #[cfg(not(gpio_irq_level))]
        Trigger::High => registers.riseie().modify(|v| v.0 |= mask),
        #[cfg(not(gpio_irq_level))]
        Trigger::Low => registers.fallie().modify(|v| v.0 |= mask),
    }
}

fn clear(registers: pac::gpio::Gpio, port: u8, mask: u16) {
    // R1W0: bounded ones preserve every documented, unowned pending flag.
    // The manual command domain is independent of sparse PAC fields, serviced
    // flags, reset values and package pads. Never read ICR or write ISR.
    let command = u32::from(crate::GPIO_CLEAR_NOOP_MASKS[usize::from(port)] & !mask);
    registers.icr().write(|v| v.0 = command);
}
