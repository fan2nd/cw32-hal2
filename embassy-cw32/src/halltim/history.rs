use super::*;

/// Maximum number of retained software observations, not physical edges.
pub const HISTORY_CAPACITY: usize = 8;

pub(super) struct History {
    pub(super) active: bool,
    pub(super) observation: Observation,
    records: [Option<Capture>; HISTORY_CAPACITY],
    head: usize,
    len: usize,
    lost: bool,
}
impl History {
    pub(super) const fn new() -> Self {
        Self {
            active: false,
            observation: Observation::new(),
            records: [None; HISTORY_CAPACITY],
            head: 0,
            len: 0,
            lost: false,
        }
    }
    pub(super) fn push(&mut self, capture: Capture) {
        if self.lost {
            return;
        }
        if self.len == HISTORY_CAPACITY {
            self.lost = true;
            return;
        }
        self.records[(self.head + self.len) % HISTORY_CAPACITY] = Some(capture);
        self.len += 1;
    }
    fn pop(&mut self) -> Result<Option<Capture>, Error> {
        if !self.active {
            return Err(Error::HistoryNotStarted);
        }
        if self.len != 0 {
            let capture = self.records[self.head].take();
            self.head = (self.head + 1) % HISTORY_CAPACITY;
            self.len -= 1;
            return Ok(capture);
        }
        if self.lost {
            self.lost = false;
            // A subsequent delivered transition cannot span an unreported gap.
            self.observation.previous = None;
            return Err(Error::HistoryOverflow);
        }
        Ok(None)
    }
}

struct HistoryWait<T: Instance>(PhantomData<T>);
impl<T: Instance> Drop for HistoryWait<T> {
    fn drop(&mut self) {
        // Cancelling a consumer does not stop the independent IRQ collector.
        let stale = critical_section::with(|cs| {
            T::state().waker.borrow(cs).borrow_mut().take()
        });
        drop(stale);
    }
}

impl<T: Instance> HallTim<'_, T> {
    /// Start bounded IRQ recording on a running timer, discarding old history.
    /// Pending hardware CAPF is eligible; this does not define a fresh wire window.
    /// The singleton queue stores values only, never a pointer to this owner.
    /// Hardware coalescing and non-atomic WIDTH/STATE sampling still apply.
    pub fn start_history(
        &mut self,
        _irq: impl Binding<BTIM3_HALLTIM, InterruptHandler<T>>,
    ) -> Result<(), Error> {
        if !self.is_running() {
            return Err(Error::Stopped);
        }
        self.stop_history();
        retire_wait::<T>();
        critical_section::with(|cs| {
            let mut history = T::state().history.borrow(cs).borrow_mut();
            *history = History::new();
            history.observation = self.observation;
            history.active = true;
            T::regs().dier().modify(|w| w.set_capie(true));
        });
        unsafe { BTIM3_HALLTIM::enable() };
        Ok(())
    }

    /// Stop recording and discard the queue without stopping the timer or
    /// clearing hardware events. Preserve the latest observation/overflow state.
    /// The shared BTIM3 interrupt remains enabled and is never unpended here.
    pub fn stop_history(&mut self) {
        let stale = critical_section::with(|cs| {
            let state = T::state();
            let mut history = state.history.borrow(cs).borrow_mut();
            if !history.active {
                return None;
            }
            T::regs().dier().modify(|w| w.set_capie(false));
            self.observation = history.observation;
            *history = History::new();
            state.waker.borrow(cs).borrow_mut().take()
        });
        drop(stale);
    }

    /// Take the oldest retained observation. A full queue preserves its prefix,
    /// then reports HistoryOverflow after draining. Further observations are
    /// discarded until that error is consumed; the next starts a new transition
    /// sequence. This loss report does not detect or count hardware overwrites.
    pub fn try_history(&mut self) -> Result<Option<Capture>, Error> {
        critical_section::with(|cs| T::state().history.borrow(cs).borrow_mut().pop())
    }

    /// Wait for one queued observation or a loss report. Cancelling/forgetting
    /// does not stop collection; stop_history, stop, restart, ordinary capture
    /// or owner Drop does. No caller buffer is retained by the handler.
    pub async fn next_history(&mut self) -> Result<Capture, Error> {
        let _wait = HistoryWait::<T>(PhantomData);
        poll_fn(|cx| {
            let candidate = cx.waker().clone();
            let (result, stale) = critical_section::with(|cs| {
                let state = T::state();
                let result = state.history.borrow(cs).borrow_mut().pop();
                match result {
                    Ok(None) => {
                        let stale = state.waker.borrow(cs).borrow_mut().replace(candidate);
                        (Poll::Pending, stale)
                    }
                    Ok(Some(capture)) => (Poll::Ready(Ok(capture)), Some(candidate)),
                    Err(error) => (Poll::Ready(Err(error)), Some(candidate)),
                }
            });
            drop(stale);
            result
        }).await
    }
}
