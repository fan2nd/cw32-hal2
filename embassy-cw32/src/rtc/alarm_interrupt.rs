//! Scoped ownership of the dedicated RTC vector on direct-access RTC variants.
use super::{Alarm, Rtc, RtcError, Unlocked, alarm, check_clock, check_write_mode};
use crate::{
    interrupt::typelevel::{Binding, Handler, Interrupt, RTC},
    pac,
};
use core::{cell::Cell, future::poll_fn, task::Poll};
use critical_section::Mutex;
use embassy_sync::waitqueue::AtomicWaker;

static ACTIVE: Mutex<Cell<bool>> = Mutex::new(Cell::new(false));
static WAKER: AtomicWaker = AtomicWaker::new();

/// Bind the dedicated RTC vector for `Rtc::wait_for_alarm` on L010/L011/L012.
/// The handler masks NVIC delivery and wakes the owner; it never clears a flag.
pub struct AlarmInterruptHandler;
impl Handler<RTC> for AlarmInterruptHandler {
    unsafe fn on_interrupt() {
        let wake = critical_section::with(|cs| {
            if !ACTIVE.borrow(cs).get() {
                return false;
            }
            let enabled = pac::RTC.ier().read();
            let status = pac::RTC.isr().read();
            if (enabled.alarma() && status.alarma()) || (enabled.alarmb() && status.alarmb()) {
                RTC::disable();
                true
            } else {
                false
            }
        });
        if wake {
            WAKER.wake();
        }
    }
}

impl Rtc<'_> {
    /// Await a sticky match on an already configured and enabled alarm.
    ///
    /// Available only on L010/L011/L012, whose own access sections permit
    /// direct alarm/interrupt register access while running. All RTC IER bits
    /// must initially be disabled. The supplied binding dedicates the RTC
    /// vector to this handler for the wait; its prior NVIC state is not restored.
    ///
    /// An existing flag completes immediately. Success and cancellation both
    /// disable this wait's IER bit and RTC NVIC delivery, preserving the alarm,
    /// calendar, source and flags. Call `clear_alarm` explicitly to acknowledge.
    /// Multiple matches coalesce. This is run-mode notification, not an absolute
    /// deadline, event counter, low-power wake guarantee or monotonic clock.
    pub async fn wait_for_alarm(
        &mut self,
        alarm: Alarm,
        _irq: impl Binding<RTC, AlarmInterruptHandler>,
    ) -> Result<(), RtcError> {
        check_clock()?;
        check_write_mode()?;
        if !pac::RTC.cr0().read().start() {
            return Err(RtcError::NotRunning);
        }
        let guard = critical_section::with(|cs| {
            if pac::RTC.ier().read().0 != 0 || ACTIVE.borrow(cs).get() {
                return Err(RtcError::InterruptInUse);
            }
            if !alarm::enabled(alarm) {
                return Err(RtcError::AlarmDisabled);
            }
            RTC::disable();
            RTC::unpend();
            let guard = WaitGuard { alarm };
            let _unlock = Unlocked::new();
            pac::RTC.ier().modify(|w| match alarm {
                Alarm::A => w.set_alarma(true),
                Alarm::B => w.set_alarmb(true),
            });
            if !alarm::interrupt_enabled(alarm) {
                return Err(RtcError::WriteFailure);
            }
            ACTIVE.borrow(cs).set(true);
            unsafe {
                RTC::enable();
            }
            Ok(guard)
        })?;
        poll_fn(|cx| {
            WAKER.register(cx.waker());
            if alarm::pending(alarm) {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        })
        .await;
        drop(guard);
        Ok(())
    }
}

struct WaitGuard {
    alarm: Alarm,
}
impl Drop for WaitGuard {
    fn drop(&mut self) {
        critical_section::with(|cs| {
            RTC::disable();
            ACTIVE.borrow(cs).set(false);
            let _unlock = Unlocked::new();
            pac::RTC.ier().modify(|w| match self.alarm {
                Alarm::A => w.set_alarma(false),
                Alarm::B => w.set_alarmb(false),
            });
            RTC::unpend();
        });
    }
}
