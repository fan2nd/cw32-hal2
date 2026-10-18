use super::*;

pub(super) struct IdleCapture {
    pub(super) active: bool,
    markers: [Option<BufferedIdle>; BUFFERED_IDLE_CAPACITY],
    head: usize,
    len: usize,
    overflow: bool,
}

impl IdleCapture {
    pub(super) fn new() -> Self {
        Self {
            active: false,
            markers: [None; BUFFERED_IDLE_CAPACITY],
            head: 0,
            len: 0,
            overflow: false,
        }
    }

    pub(super) fn push(&mut self, marker: BufferedIdle) {
        // Preserve the retained prefix, then a single explicit loss barrier.
        if self.overflow {
            return;
        }
        if self.len == BUFFERED_IDLE_CAPACITY {
            self.overflow = true;
            return;
        }
        let tail = (self.head + self.len) % BUFFERED_IDLE_CAPACITY;
        self.markers[tail] = Some(marker);
        self.len += 1;
    }

    fn pop(&mut self) -> Result<Option<BufferedIdle>, IdleCaptureError> {
        if !self.active {
            return Err(IdleCaptureError::NotCapturing);
        }
        if self.len != 0 {
            let marker = self.markers[self.head].take();
            self.head = (self.head + 1) % BUFFERED_IDLE_CAPACITY;
            self.len -= 1;
            return Ok(marker);
        }
        if self.overflow {
            self.overflow = false;
            return Err(IdleCaptureError::Overflow);
        }
        Ok(None)
    }

    pub(super) fn advance(&mut self, count: usize) {
        // Four fixed slots: bounded interrupt masking, no lifetime byte counter.
        for marker in self.markers.iter_mut().flatten() {
            marker.buffered_bytes = marker.buffered_bytes.saturating_sub(count);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn marker(count: usize) -> BufferedIdle {
        BufferedIdle {
            buffered_bytes: count,
            error: Some(Error::Framing),
        }
    }
    #[test]
    fn idle_positions_wrap_and_saturate_without_changing_returned_snapshots() {
        let mut queue = IdleCapture::new();
        assert_eq!(queue.pop(), Err(IdleCaptureError::NotCapturing));
        queue.active = true;
        for count in 1..=4 {
            queue.push(marker(count));
        }
        let saved = queue.pop().unwrap().unwrap();
        queue.push(marker(5));
        queue.advance(3);
        assert_eq!(saved, marker(1));
        for count in [0, 0, 1, 2] {
            assert_eq!(queue.pop(), Ok(Some(marker(count))));
        }
        assert_eq!(queue.pop(), Ok(None));
    }
    #[test]
    fn idle_overflow_is_a_barrier_until_observed() {
        let mut queue = IdleCapture::new();
        queue.active = true;
        for count in 0..5 {
            queue.push(marker(count));
        }
        for count in 0..4 {
            assert_eq!(queue.pop(), Ok(Some(marker(count))));
        }
        queue.push(marker(99));
        assert_eq!(queue.pop(), Err(IdleCaptureError::Overflow));
        assert_eq!(queue.pop(), Ok(None));
        queue.push(marker(6));
        assert_eq!(queue.pop(), Ok(Some(marker(6))));
    }
}

impl BufferedUartRx<'_> {
    /// Start continuous capture of up to four native idle markers.
    ///
    /// Clears old idle state/markers but preserves receive data and errors.
    /// Markers track unread queue positions as bytes are consumed; an already
    /// returned marker is no longer adjusted. Error fields describe observation
    /// history and may repeat an error already consumed. No wire framing or
    /// lossless history is promised. The fifth queued observation sets Overflow;
    /// later markers are discarded until retained markers and Overflow are read.
    pub fn start_idle_capture(&mut self) {
        critical_section::with(|cs| {
            let mut ring = self.state.rx_ring.borrow(cs).borrow_mut();
            let ring = ring.as_mut().expect("buffered UART receiver has no ring");
            self.info.regs.ier().modify(|w| w.set_rxidle(false));
            let mut command = regs::Icr::write_noop();
            command.set_rxidle(false);
            self.info.regs.icr().write_value(command);
            ring.idle_snapshot = None;
            ring.idle_capture = IdleCapture::new();
            ring.idle_capture.active = true;
            self.info.regs.ier().modify(|w| w.set_rxidle(true));
        });
    }

    /// Stop idle capture and discard its markers; RX collection and TX stay live.
    pub fn stop_idle_capture(&mut self) {
        critical_section::with(|cs| {
            self.info.regs.ier().modify(|w| w.set_rxidle(false));
            let mut ring = self.state.rx_ring.borrow(cs).borrow_mut();
            let ring = ring.as_mut().expect("buffered UART receiver has no ring");
            ring.idle_snapshot = None;
            ring.idle_capture = IdleCapture::new();
        });
    }

    /// Take the oldest adjusted marker, or None without waiting.
    /// Reading markers never consumes receive bytes or errors.
    pub fn try_take_idle(&mut self) -> Result<Option<BufferedIdle>, IdleCaptureError> {
        critical_section::with(|cs| {
            self.state
                .rx_ring
                .borrow(cs)
                .borrow_mut()
                .as_mut()
                .expect("buffered UART receiver has no ring")
                .idle_capture
                .pop()
        })
    }

    /// Wait for the next queued marker or capture error. Start capture first.
    /// Cancellation/forgetting does not stop capture or consume a pending marker;
    /// call stop_idle_capture explicitly. No caller output pointer is retained.
    pub async fn next_idle_marker(&mut self) -> Result<BufferedIdle, IdleCaptureError> {
        poll_fn(|cx| {
            self.state.rx_waker.register(cx.waker());
            match self.try_take_idle() {
                Ok(Some(marker)) => Poll::Ready(Ok(marker)),
                Ok(None) => Poll::Pending,
                Err(error) => Poll::Ready(Err(error)),
            }
        })
        .await
    }

    /// Wait for a new coalesced native idle observation while RX keeps collecting.
    ///
    /// First poll clears only stale RXIDLE and opens a new observation window.
    /// It also stops continuous idle capture and discards its queued markers.
    /// Returns the queued byte count and retained error at ISR observation time,
    /// including any bytes queued before the wait began. Read exactly this prefix
    /// before using its error snapshot; later bytes may already have been appended.
    /// The position is relative to the unread queue and becomes stale after reads,
    /// consume or replacement. It does not imply error-free data or a wire packet
    /// boundary; delayed service and repeated events can coalesce. The ring may overflow
    /// while waiting. Hardware needs a received character before detecting idle,
    /// so a silent line need not complete this wait.
    ///
    /// Cancellation masks only idle notification, leaving collection and TX live.
    /// Forgetting can leave one idle notification armed; the next idle retires
    /// it, or a later idle wait/RX Drop cleans it up. The wait retains no additional
    /// buffer beyond the receiver's existing ring.
    #[cfg(any(uart_cw32l010_v1, uart_cw32l012_v1))]
    pub async fn wait_for_idle(&mut self) -> BufferedIdle {
        let guard = BufferedIdleWait {
            info: self.info,
            state: self.state,
        };
        let mut started = false;
        let snapshot = poll_fn(|cx| {
            self.state.rx_waker.register(cx.waker());
            critical_section::with(|cs| {
                let mut ring = self.state.rx_ring.borrow(cs).borrow_mut();
                let ring = ring.as_mut().expect("buffered UART receiver has no ring");
                if !started {
                    self.info.regs.ier().modify(|w| w.set_rxidle(false));
                    let mut command = regs::Icr::write_noop();
                    command.set_rxidle(false);
                    self.info.regs.icr().write_value(command);
                    ring.idle_snapshot = None;
                    ring.idle_capture = IdleCapture::new();
                    self.info.regs.ier().modify(|w| w.set_rxidle(true));
                    started = true;
                }
                if let Some(snapshot) = ring.idle_snapshot {
                    Poll::Ready(snapshot)
                } else {
                    Poll::Pending
                }
            })
        })
        .await;
        drop(guard);
        snapshot
    }
}

impl BufferedUart<'_> {
    /// See [`BufferedUartRx::start_idle_capture`].
    pub fn start_idle_capture(&mut self) {
        self.rx.start_idle_capture();
    }

    /// See [`BufferedUartRx::stop_idle_capture`].
    pub fn stop_idle_capture(&mut self) {
        self.rx.stop_idle_capture();
    }

    /// See [`BufferedUartRx::try_take_idle`].
    pub fn try_take_idle(&mut self) -> Result<Option<BufferedIdle>, IdleCaptureError> {
        self.rx.try_take_idle()
    }

    /// See [`BufferedUartRx::next_idle_marker`].
    pub async fn next_idle_marker(&mut self) -> Result<BufferedIdle, IdleCaptureError> {
        self.rx.next_idle_marker().await
    }
}

#[cfg(any(uart_cw32l010_v1, uart_cw32l012_v1))]
struct BufferedIdleWait {
    info: Info,
    state: &'static State,
}

#[cfg(any(uart_cw32l010_v1, uart_cw32l012_v1))]
impl Drop for BufferedIdleWait {
    fn drop(&mut self) {
        critical_section::with(|cs| {
            self.info.regs.ier().modify(|w| w.set_rxidle(false));
            if let Some(ring) = self.state.rx_ring.borrow(cs).borrow_mut().as_mut() {
                ring.idle_snapshot = None;
            }
        });
    }
}
