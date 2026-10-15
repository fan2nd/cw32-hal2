use super::*;

impl RxStorage {
    fn new(buffer: &mut [u8]) -> Self {
        assert!(!buffer.is_empty());
        Self {
            ptr: buffer.as_mut_ptr(),
            capacity: buffer.len(),
        }
    }
}
// SAFETY: constructors guarantee exclusive, valid storage until shutdown
// detaches this descriptor (forever if a safe static owner is forgotten).
// Borrowed constructors require the caller to uphold this even if Drop is bypassed.
// Moving the descriptor between threads does not change that obligation.
// All ring metadata and byte writes are serialized by State's critical-section mutex; live shared
// prefixes cover only occupied slots, which writers never modify. No Sync impl.
unsafe impl Send for RxStorage {}

impl RxRing {
    fn new(buffer: &mut [u8]) -> Self {
        Self {
            storage: RxStorage::new(buffer),
            read: 0,
            write: 0,
            len: 0,
            exposed: 0,
            error: None,
            waiting: false,
        }
    }

    // Called only under the per-instance critical section. Hardware events are
    // still drained/acknowledged before this helper applies the error barrier.
    pub(super) fn observe(&mut self, observation: Result<Option<u8>, Error>) {
        if self.error.is_some() {
            return;
        }
        match observation {
            Ok(Some(byte)) if self.len < self.storage.capacity => {
                // SAFETY: write is in bounds and names a free slot, disjoint
                // from every occupied prefix returned by fill_buf. Only this
                // helper writes storage, and all its calls are serialized.
                unsafe { self.storage.ptr.add(self.write).write(byte) };
                self.write = if self.write == self.storage.capacity - 1 {
                    0
                } else {
                    self.write + 1
                };
                self.len += 1;
            }
            Ok(Some(_)) => self.error = Some(Error::BufferOverflow),
            Ok(None) => {}
            Err(error) => self.error = Some(error),
        }
    }

    // Release occupied bytes only after the owner's mutable borrow guarantees
    // no returned prefix remains in use. count is bounded by len by callers.
    fn advance(&mut self, count: usize) {
        assert!(count <= self.len);
        self.read = if count < self.storage.capacity - self.read {
            self.read + count
        } else {
            count - (self.storage.capacity - self.read)
        };
        self.len -= count;
    }

    // Snapshot only an occupied contiguous segment, not the whole allocation.
    // The caller must tie a resulting reference to the RX owner's mutable borrow.
    fn readable(&mut self) -> Result<(*const u8, usize), Error> {
        self.exposed = self.len.min(self.storage.capacity - self.read);
        if self.exposed != 0 {
            self.waiting = false;
        } else if let Some(error) = self.error.take() {
            self.waiting = false;
            return Err(error);
        }
        // SAFETY: read is always in bounds of the consumed static allocation.
        Ok((unsafe { self.storage.ptr.add(self.read) }, self.exposed))
    }

    fn consume(&mut self, count: usize) {
        assert!(
            count <= self.exposed,
            "consume exceeds the last fill_buf prefix"
        );
        self.advance(count);
        self.exposed -= count;
    }

    // Called only while the per-instance critical section is held. The ISR
    // cannot append or change the error barrier while this batch is copied.
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, Error> {
        if buffer.is_empty() {
            return Ok(0);
        }
        self.exposed = 0;
        // Bound global interrupt masking independently of application ring size.
        // Partial reads may return less than the available data.
        let count = buffer.len().min(self.len).min(64);
        if count != 0 {
            let first = count.min(self.storage.capacity - self.read);
            // SAFETY: both source ranges contain initialized occupied bytes;
            // the destination cannot alias the consumed static allocation in
            // safe code. No owner-returned slice is live during a mutable read.
            unsafe {
                core::ptr::copy_nonoverlapping(
                    self.storage.ptr.add(self.read),
                    buffer.as_mut_ptr(),
                    first,
                );
                core::ptr::copy_nonoverlapping(
                    self.storage.ptr,
                    buffer.as_mut_ptr().add(first),
                    count - first,
                );
            }
            self.advance(count);
            self.waiting = false;
            return Ok(count);
        }
        if let Some(error) = self.error.take() {
            self.waiting = false;
            return Err(error);
        }
        Ok(0)
    }
}

impl<'d> BufferedUartRx<'d> {
    /// Replace an empty IRQ ring without stopping reception, returning its storage.
    ///
    /// A queued byte or pending error rejects replacement. On failure, the
    /// original ring is unchanged and the supplied static buffer is returned.
    /// On success, storage is switched under the ISR's critical section without
    /// copying or clearing bytes, changing registers, or touching the TX half.
    ///
    /// The returned old buffer has the owner's lifetime. The new buffer must
    /// be static so forgetting this owner cannot leave the ISR with freed storage.
    /// A byte still in hardware can enter the new ring; this is not an idle or
    /// packet boundary, and continuous traffic may prevent replacement.
    pub fn try_replace_buffer(
        &mut self,
        buffer: &'static mut [u8],
    ) -> Result<&'d mut [u8], (BufferReplaceError, &'static mut [u8])> {
        if buffer.is_empty() {
            return Err((BufferReplaceError::Empty, buffer));
        }
        let storage = critical_section::with(|cs| {
            let mut ring = self.state.rx_ring.borrow(cs).borrow_mut();
            let ring = ring.as_mut().expect("buffered UART receiver has no ring");
            if ring.len != 0 || ring.error.is_some() {
                return Err((BufferReplaceError::PendingReceive, buffer));
            }
            Ok(core::mem::replace(ring, RxRing::new(buffer)).storage)
        })?;
        // SAFETY: the exclusive owner borrow excludes active read/fill_buf
        // borrows. The ISR uses the same critical section and can now access
        // only the new static allocation. Construction reserved the detached
        // initialized allocation for 'd (replacement requires static storage);
        // its descriptor is never used again.
        Ok(unsafe { core::slice::from_raw_parts_mut(storage.ptr, storage.capacity) })
    }

    /// Stop reception, consume this owner and return the entire current ring.
    ///
    /// Unread bytes and pending errors are discarded; the returned storage is
    /// not in receive order and is not cleared. Shutdown follows normal Drop,
    /// including RX/RTS disable and ISR pointer detachment. A live TX half keeps
    /// its existing ownership. This does not return pins or the peripheral.
    ///
    /// The slice has the owner's lifetime, even for a static constructor:
    /// a `BufferedUartRx<'static>` returns `&'static mut [u8]`. Borrowed
    /// constructors retain their unsafe no-leak contract until shutdown.
    pub fn into_buffer(self) -> &'d mut [u8] {
        let (ptr, capacity) = critical_section::with(|cs| {
            let ring = self.state.rx_ring.borrow(cs).borrow();
            let ring = ring.as_ref().expect("buffered UART receiver has no ring");
            (ring.storage.ptr, ring.storage.capacity)
        });
        // Do not form a reference while the ISR can still write. Run the full
        // owner/pin teardown exactly once, before reconstructing the slice.
        drop(self);
        // SAFETY: construction or replacement reserved this initialized slice
        // exclusively for 'd. Consuming the owner excludes live read/fill_buf
        // borrows. Drop has disabled collection and detached the only ISR storage descriptor;
        // the TX half never accesses it. No reference escapes if Drop panics.
        unsafe { core::slice::from_raw_parts_mut(ptr, capacity) }
    }

    /// Start continuous IRQ reception using a consumed nonempty static ring.
    /// Rejected inputs, including the ring, are consumed. See [`BufferedUartRx`].
    pub fn new<T: Instance, RX: RxPin<T>>(
        _peri: Peri<'d, T>,
        rx: Peri<'d, RX>,
        _irq: impl Binding<T::Interrupt, T::InterruptHandler>,
        buffer: &'static mut [u8],
        config: Config,
    ) -> Result<Self, ConfigError> {
        let rx = Self::new_inner::<T, RX>(rx, None, buffer, config)?;
        unsafe { T::Interrupt::enable() };
        Ok(rx)
    }

    /// Start continuous IRQ reception using a borrowed, nonempty receive ring.
    /// Normal RX Drop stops collection and releases the buffer borrow. On a
    /// constructor error no pointer to the supplied buffer is retained.
    ///
    /// # Safety
    /// Until reception is stopped and the ISR ring is detached (normally by
    /// RX Drop), its buffer must remain allocated at the same address and
    /// exclusively accessible through the driver. Leaking or forgetting the
    /// RX owner, a split RX half, or a future/container holding it does not
    /// stop reception. Keep storage valid and reserved until shutdown, even
    /// after a leak or panic; without shutdown that reservation lasts forever.
    /// Cancelling an operation is fine if the owner is still dropped normally.
    /// A borrowed buffer in an async task must outlive collection even if that
    /// task/future is leaked; an ordinary borrow alone cannot guarantee this.
    /// Prefer the safe static constructor when that obligation cannot be met.
    pub unsafe fn new_borrowed<T: Instance, RX: RxPin<T>>(
        _peri: Peri<'d, T>,
        rx: Peri<'d, RX>,
        _irq: impl Binding<T::Interrupt, T::InterruptHandler>,
        buffer: &'d mut [u8],
        config: Config,
    ) -> Result<Self, ConfigError> {
        let rx = Self::new_inner::<T, RX>(rx, None, buffer, config)?;
        unsafe { T::Interrupt::enable() };
        Ok(rx)
    }

    /// Start continuous IRQ reception with active-low hardware RTS.
    /// RTS follows hardware RDR occupancy, not software ring fullness. Rejected
    /// inputs, including the static ring, are consumed. See [`BufferedUartRx`].
    pub fn new_with_rts<T: Instance, RX: RxPin<T>, RTS: RtsPin<T>>(
        _peri: Peri<'d, T>,
        rx: Peri<'d, RX>,
        rts: Peri<'d, RTS>,
        _irq: impl Binding<T::Interrupt, T::InterruptHandler>,
        buffer: &'static mut [u8],
        config: Config,
    ) -> Result<Self, ConfigError> {
        let rx = Self::new_inner::<T, RX>(
            rx,
            Some((rts.into(), <RTS as sealed::Pin<T, sealed::Rts>>::AF)),
            buffer,
            config,
        )?;
        unsafe { T::Interrupt::enable() };
        Ok(rx)
    }

    /// Start borrowed IRQ-buffered RX with active-low hardware RTS.
    /// RTS follows RDR occupancy, not software ring fullness.
    /// Normal RX Drop stops collection and releases the buffer borrow. On a
    /// constructor error no pointer to the supplied buffer is retained.
    ///
    /// # Safety
    /// Until reception is stopped and the ISR ring is detached (normally by
    /// RX Drop), its buffer must remain allocated at the same address and
    /// exclusively accessible through the driver. Leaking or forgetting the
    /// RX owner, a split RX half, or a future/container holding it does not
    /// stop reception. Keep storage valid and reserved until shutdown, even
    /// after a leak or panic; without shutdown that reservation lasts forever.
    /// Cancelling an operation is fine if the owner is still dropped normally.
    /// A borrowed buffer in an async task must outlive collection even if that
    /// task/future is leaked; an ordinary borrow alone cannot guarantee this.
    /// Prefer the safe static constructor when that obligation cannot be met.
    pub unsafe fn new_borrowed_with_rts<T: Instance, RX: RxPin<T>, RTS: RtsPin<T>>(
        _peri: Peri<'d, T>,
        rx: Peri<'d, RX>,
        rts: Peri<'d, RTS>,
        _irq: impl Binding<T::Interrupt, T::InterruptHandler>,
        buffer: &'d mut [u8],
        config: Config,
    ) -> Result<Self, ConfigError> {
        let rx = Self::new_inner::<T, RX>(
            rx,
            Some((rts.into(), <RTS as sealed::Pin<T, sealed::Rts>>::AF)),
            buffer,
            config,
        )?;
        unsafe { T::Interrupt::enable() };
        Ok(rx)
    }

    fn from_pin<T: Instance, RX: RxPin<T>>(
        pin: Peri<'d, RX>,
        rts: Option<(Peri<'d, AnyPin>, u8)>,
        info: Info,
        actual: u32,
        pull: Pull,
    ) -> Self {
        let mut pin = Flex::new(pin);
        pin.set_as_af(<RX as sealed::Pin<T, sealed::Rx>>::AF, false, pull);
        let rts = rts.map(|(pin, af)| {
            let mut pin = Flex::new(pin);
            pin.set_as_af(af, true, Pull::None);
            pin
        });
        // Ownership is published only after every pin and owner value exists.
        Self {
            info,
            state: T::state(),
            _pin: pin,
            _rts: rts,
            actual_baudrate: actual,
            _buffer: PhantomData,
        }
    }

    fn install_ring(&self, cs: critical_section::CriticalSection<'_>, buffer: &'d mut [u8]) {
        *self.state.rx_ring.borrow(cs).borrow_mut() = Some(RxRing::new(buffer));
        let mut owners = self.state.lifecycle.borrow(cs).get();
        owners.rx_live = true;
        self.state.lifecycle.borrow(cs).set(owners);
    }

    fn new_inner<T: Instance, RX: RxPin<T>>(
        rx: Peri<'d, RX>,
        rts: Option<(Peri<'d, AnyPin>, u8)>,
        buffer: &'d mut [u8],
        config: Config,
    ) -> Result<Self, ConfigError> {
        // Entry points either consume static storage or require the caller to
        // keep borrowed storage valid and exclusive until RX shutdown.
        let baud = configured_baud::<T>(config)?;
        require_byte_config(config)?;
        if buffer.is_empty() {
            return Err(ConfigError::RxBufferEmpty);
        }
        let info = T::info();
        info.prepare::<T>(config, baud);
        let rx = Self::from_pin::<T, RX>(rx, rts, info, baud.actual, config.rx_pull);
        critical_section::with(|cs| {
            rx.install_ring(cs, buffer);
            info.regs.cr2().modify(|w| w.set_rtsen(rx._rts.is_some()));
            info.regs.cr1().modify(|w| w.set_rxen(true));
            info.regs
                .ier()
                .modify(|w| WaitEvent::Receive.set_enabled(w, true));
        });
        Ok(rx)
    }

    fn poll_read(&mut self, cx: &mut Context<'_>, buffer: &mut [u8]) -> Poll<Result<usize, Error>> {
        // Register before checking: arrivals before this point are retained;
        // arrivals after the empty check see waiting=true and this task's waker.
        self.state.rx_waker.register(cx.waker());
        critical_section::with(|cs| {
            let mut ring = self.state.rx_ring.borrow(cs).borrow_mut();
            let ring = ring.as_mut().expect("buffered UART receiver has no ring");
            match ring.read(buffer) {
                Ok(0) => {
                    ring.waiting = true;
                    Poll::Pending
                }
                result => Poll::Ready(result),
            }
        })
    }

    /// Receive the currently queued clean prefix, waiting for at least one byte.
    ///
    /// Returns the number copied, up to 64 bytes per call to bound each
    /// interrupt-masked copy. It may be less than `buffer.len()`. A
    /// pending discontinuity error is reported only after its clean prefix has
    /// been returned; bytes and an error are never consumed by the same call.
    /// Empty reads return zero without consuming data/errors or registering a
    /// waker. An unpolled or cancelled pending read consumes nothing. Collection
    /// continues and one stale task wake may remain after cancellation.
    pub async fn read(&mut self, buffer: &mut [u8]) -> Result<usize, Error> {
        if buffer.is_empty() {
            return Ok(0);
        }
        poll_fn(|cx| self.poll_read(cx, buffer)).await
    }

    /// Wait for a nonempty contiguous clean prefix without copying or consuming it.
    ///
    /// Call [`Self::consume`] to release bytes. The returned slice holds a mutable
    /// borrow of this owner, while IRQ collection may append only into free slots.
    /// At a wrap, another call after consumption exposes the next segment. A full
    /// ring preserves this prefix and records overflow, not backpressure. Errors
    /// are reported only after all earlier clean bytes have been consumed.
    /// Cancelling a pending call consumes neither bytes nor errors.
    pub async fn fill_buf(&mut self) -> Result<&[u8], Error> {
        let (ptr, len) = poll_fn(|cx| {
            self.state.rx_waker.register(cx.waker());
            critical_section::with(|cs| {
                let mut ring = self.state.rx_ring.borrow(cs).borrow_mut();
                let ring = ring.as_mut().expect("buffered UART receiver has no ring");
                match ring.readable() {
                    Ok((_, 0)) => {
                        ring.waiting = true;
                        Poll::Pending
                    }
                    result => Poll::Ready(result),
                }
            })
        })
        .await?;
        // SAFETY: readable snapshots initialized occupied bytes in static owned
        // storage. They remain occupied until a mutable owner operation, which
        // this returned borrow prevents. The ISR only writes disjoint free slots;
        // drop/reconstruction likewise cannot occur while this borrow is used.
        Ok(unsafe { core::slice::from_raw_parts(ptr, len) })
    }

    /// Release bytes from the last [`Self::fill_buf`] prefix.
    ///
    /// Panics if `amount` exceeds that prefix's unconsumed length. Copying reads
    /// invalidate that allowance (except empty reads). Zero is always a no-op.
    pub fn consume(&mut self, amount: usize) {
        critical_section::with(|cs| {
            self.state
                .rx_ring
                .borrow(cs)
                .borrow_mut()
                .as_mut()
                .expect("buffered UART receiver has no ring")
                .consume(amount);
        });
    }

    /// Fill the requested slice, waiting across partial reads as necessary.
    ///
    /// Cancellation or error may leave a filled prefix. Those bytes are not
    /// returned to the ring, and errors carry no partial count. Use `read` when
    /// the application must know how many clean bytes precede an error.
    pub async fn read_exact(&mut self, mut buffer: &mut [u8]) -> Result<(), Error> {
        while !buffer.is_empty() {
            let count = self.read(buffer).await?;
            buffer = &mut buffer[count..];
        }
        Ok(())
    }

    /// Copy queued bytes without waiting, matching Embassy's buffered API.
    ///
    /// Returns zero when no bytes/error are queued, not end-of-stream. Clean
    /// data is returned before a pending error. Empty slices have no effect.
    pub fn blocking_read(&mut self, buffer: &mut [u8]) -> Result<usize, Error> {
        if buffer.is_empty() {
            return Ok(0);
        }
        critical_section::with(|cs| {
            self.state
                .rx_ring
                .borrow(cs)
                .borrow_mut()
                .as_mut()
                .expect("buffered UART receiver has no ring")
                .read(buffer)
        })
    }

    /// Whether a nonempty read can complete immediately with data or an error.
    /// This does not consume the pending error or queued bytes.
    pub fn read_ready(&mut self) -> Result<bool, Error> {
        critical_section::with(|cs| {
            let ring = self.state.rx_ring.borrow(cs).borrow();
            let ring = ring.as_ref().expect("buffered UART receiver has no ring");
            Ok(ring.len != 0 || ring.error.is_some())
        })
    }

    /// Baud rate after divider rounding, excluding oscillator tolerance.
    pub fn actual_baudrate(&self) -> u32 {
        self.actual_baudrate
    }
}

impl<'d> BufferedUart<'d> {
    /// Create ordinary async TX and continuously IRQ-buffered RX.
    /// Rejected inputs, including the static ring, are consumed. See [`BufferedUartRx`].
    pub fn new<T: Instance, TX: TxPin<T>, RX: RxPin<T>>(
        peri: Peri<'d, T>,
        tx: Peri<'d, TX>,
        rx: Peri<'d, RX>,
        _irq: impl Binding<T::Interrupt, T::InterruptHandler>,
        buffer: &'static mut [u8],
        config: Config,
    ) -> Result<Self, ConfigError> {
        let uart = Self::new_inner(peri, tx, rx, None, None, buffer, config)?;
        unsafe { T::Interrupt::enable() };
        Ok(uart)
    }

    /// Create ordinary async TX and borrowed continuous IRQ-buffered RX.
    /// RX Drop stops collection; reuse also requires the shared TX/RX lifetime
    /// to end. Constructor errors retain no pointer to the supplied buffer.
    ///
    /// # Safety
    /// Until reception is stopped and the ISR ring is detached (normally by
    /// RX Drop), its buffer must remain allocated at the same address and
    /// exclusively accessible through the driver. Leaking or forgetting the
    /// RX owner, a split RX half, or a future/container holding it does not
    /// stop reception. Keep storage valid and reserved until shutdown, even
    /// after a leak or panic; without shutdown that reservation lasts forever.
    /// Cancelling an operation is fine if the owner is still dropped normally.
    /// A borrowed buffer in an async task must outlive collection even if that
    /// task/future is leaked; an ordinary borrow alone cannot guarantee this.
    /// Prefer the safe static constructor when that obligation cannot be met.
    pub unsafe fn new_borrowed<T: Instance, TX: TxPin<T>, RX: RxPin<T>>(
        peri: Peri<'d, T>,
        tx: Peri<'d, TX>,
        rx: Peri<'d, RX>,
        _irq: impl Binding<T::Interrupt, T::InterruptHandler>,
        buffer: &'d mut [u8],
        config: Config,
    ) -> Result<Self, ConfigError> {
        let uart = Self::new_inner(peri, tx, rx, None, None, buffer, config)?;
        unsafe { T::Interrupt::enable() };
        Ok(uart)
    }

    /// Create buffered RX and ordinary async TX with active-low RTS and CTS.
    /// RTS does not reflect software ring fullness. CTS can stall TX forever.
    /// Rejected inputs, including the static ring, are consumed. See [`BufferedUartRx`].
    pub fn new_with_rtscts<
        T: Instance,
        TX: TxPin<T>,
        RX: RxPin<T>,
        RTS: RtsPin<T>,
        CTS: CtsPin<T>,
    >(
        peri: Peri<'d, T>,
        tx: Peri<'d, TX>,
        rx: Peri<'d, RX>,
        rts: Peri<'d, RTS>,
        cts: Peri<'d, CTS>,
        _irq: impl Binding<T::Interrupt, T::InterruptHandler>,
        buffer: &'static mut [u8],
        config: Config,
    ) -> Result<Self, ConfigError> {
        let uart = Self::new_inner(
            peri,
            tx,
            rx,
            Some((rts.into(), <RTS as sealed::Pin<T, sealed::Rts>>::AF)),
            Some((cts.into(), <CTS as sealed::Pin<T, sealed::Cts>>::AF)),
            buffer,
            config,
        )?;
        unsafe { T::Interrupt::enable() };
        Ok(uart)
    }

    /// Create ordinary async TX and borrowed IRQ-buffered RX with RTS/CTS.
    /// RTS follows RDR occupancy, not ring fullness; CTS can stall TX forever.
    /// RX Drop stops collection; reuse also requires the shared TX/RX lifetime
    /// to end. Constructor errors retain no pointer to the supplied buffer.
    ///
    /// # Safety
    /// Until reception is stopped and the ISR ring is detached (normally by
    /// RX Drop), its buffer must remain allocated at the same address and
    /// exclusively accessible through the driver. Leaking or forgetting the
    /// RX owner, a split RX half, or a future/container holding it does not
    /// stop reception. Keep storage valid and reserved until shutdown, even
    /// after a leak or panic; without shutdown that reservation lasts forever.
    /// Cancelling an operation is fine if the owner is still dropped normally.
    /// A borrowed buffer in an async task must outlive collection even if that
    /// task/future is leaked; an ordinary borrow alone cannot guarantee this.
    /// Prefer the safe static constructor when that obligation cannot be met.
    pub unsafe fn new_borrowed_with_rtscts<
        T: Instance,
        TX: TxPin<T>,
        RX: RxPin<T>,
        RTS: RtsPin<T>,
        CTS: CtsPin<T>,
    >(
        peri: Peri<'d, T>,
        tx: Peri<'d, TX>,
        rx: Peri<'d, RX>,
        rts: Peri<'d, RTS>,
        cts: Peri<'d, CTS>,
        _irq: impl Binding<T::Interrupt, T::InterruptHandler>,
        buffer: &'d mut [u8],
        config: Config,
    ) -> Result<Self, ConfigError> {
        let uart = Self::new_inner(
            peri,
            tx,
            rx,
            Some((rts.into(), <RTS as sealed::Pin<T, sealed::Rts>>::AF)),
            Some((cts.into(), <CTS as sealed::Pin<T, sealed::Cts>>::AF)),
            buffer,
            config,
        )?;
        unsafe { T::Interrupt::enable() };
        Ok(uart)
    }

    fn new_inner<T: Instance, TX: TxPin<T>, RX: RxPin<T>>(
        _peri: Peri<'d, T>,
        tx: Peri<'d, TX>,
        rx: Peri<'d, RX>,
        rts: Option<(Peri<'d, AnyPin>, u8)>,
        cts: Option<(Peri<'d, AnyPin>, u8)>,
        buffer: &'d mut [u8],
        config: Config,
    ) -> Result<Self, ConfigError> {
        // Entry points either consume static storage or require the caller to
        // keep borrowed storage valid and exclusive until RX shutdown.
        let baud = configured_baud::<T>(config)?;
        require_byte_config(config)?;
        if buffer.is_empty() {
            return Err(ConfigError::RxBufferEmpty);
        }
        validate_cts_pull(cts.as_ref().map(|(pin, _)| &**pin), config.cts_pull)?;
        let info = T::info();
        info.prepare::<T>(config, baud);
        let rx = BufferedUartRx::from_pin::<T, RX>(rx, rts, info, baud.actual, config.rx_pull);
        let mut tx = UartTx::from_pin::<T, TX>(tx, info, baud.actual, config.data_bits);
        tx._cts = cts.map(|(pin, af)| {
            let mut pin = Flex::new(pin);
            pin.set_as_af(af, false, config.cts_pull);
            pin
        });
        critical_section::with(|cs| {
            rx.install_ring(cs, buffer);
            info.regs.cr2().modify(|w| {
                w.set_rtsen(rx._rts.is_some());
                w.set_ctsen(tx._cts.is_some());
            });
            info.regs.cr1().modify(|w| {
                w.set_rxen(true);
                w.set_txen(true);
            });
            info.regs
                .ier()
                .modify(|w| WaitEvent::Receive.set_enabled(w, true));
        });
        Ok(Self { tx, rx })
    }

    /// Replace an empty RX ring. See [`BufferedUartRx::try_replace_buffer`].
    pub fn try_replace_buffer(
        &mut self,
        buffer: &'static mut [u8],
    ) -> Result<&'d mut [u8], (BufferReplaceError, &'static mut [u8])> {
        self.rx.try_replace_buffer(buffer)
    }

    /// Receive an available prefix. See [`BufferedUartRx::read`].
    pub async fn read(&mut self, buffer: &mut [u8]) -> Result<usize, Error> {
        self.rx.read(buffer).await
    }
    /// Fill the slice. Cancellation or error may leave a filled prefix.
    pub async fn read_exact(&mut self, buffer: &mut [u8]) -> Result<(), Error> {
        self.rx.read_exact(buffer).await
    }
    /// Copy queued data without waiting. Zero means no data, not end-of-stream.
    pub fn blocking_read(&mut self, buffer: &mut [u8]) -> Result<usize, Error> {
        self.rx.blocking_read(buffer)
    }
    /// Whether a nonempty read can complete immediately with data or an error.
    pub fn read_ready(&mut self) -> Result<bool, Error> {
        self.rx.read_ready()
    }
    /// Borrow a contiguous clean prefix. See [`BufferedUartRx::fill_buf`].
    pub async fn fill_buf(&mut self) -> Result<&[u8], Error> {
        self.rx.fill_buf().await
    }
    /// Release bytes from the last prefix. See [`BufferedUartRx::consume`].
    pub fn consume(&mut self, amount: usize) {
        self.rx.consume(amount)
    }
    /// Queue bytes through ordinary async TX; cancellation may leave a sent prefix.
    pub async fn write(&mut self, buffer: &[u8]) -> Result<(), Error> {
        self.tx.write(buffer).await
    }
    /// Wait until the transmit buffer and shift register are empty.
    pub async fn flush(&mut self) -> Result<(), Error> {
        self.tx.flush().await
    }
    /// Queue bytes with blocking status checks; call `blocking_flush` for wire completion.
    pub fn blocking_write(&mut self, buffer: &[u8]) -> Result<(), Error> {
        self.tx.blocking_write(buffer)
    }
    /// Wait for all queued transmit data to leave the wire.
    pub fn blocking_flush(&mut self) -> Result<(), Error> {
        self.tx.blocking_flush()
    }
    /// Baud rate after divider rounding, excluding oscillator tolerance.
    pub fn actual_baudrate(&self) -> u32 {
        self.tx.actual_baudrate()
    }
    /// Split into independent ordinary TX and buffered RX ownership.
    pub fn split(self) -> (UartTx<'d, Async>, BufferedUartRx<'d>) {
        (self.tx, self.rx)
    }
    /// Borrow the disjoint TX and buffered RX halves to use concurrently.
    pub fn split_ref(&mut self) -> (&mut UartTx<'d, Async>, &mut BufferedUartRx<'d>) {
        (&mut self.tx, &mut self.rx)
    }
}

impl Drop for BufferedUartRx<'_> {
    fn drop(&mut self) {
        self.info.shutdown(self.state, Direction::Rx, false);
    }
}
