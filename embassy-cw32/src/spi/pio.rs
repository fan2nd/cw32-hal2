use super::*;

#[cfg(spi_async_pio)]
impl PioWait {
    fn ready(self, status: pac::spi::regs::Isr) -> bool {
        match self {
            Self::Tx => status.txe(),
            Self::Rx => status.rxne(),
            Self::Idle => status.txe() && !status.busy(),
        }
    }
}

#[cfg(spi_async_pio)]
impl PioState {
    pub(crate) const fn new() -> Self {
        Self {
            inner: critical_section::Mutex::new(RefCell::new(PioInterruptState {
                active: false,
                error: None,
                waker: None,
            })),
        }
    }

    pub(super) fn deactivate(&self, regs: pac::spi::Spi) {
        critical_section::with(|cs| {
            let mut inner = self.inner.borrow(cs).borrow_mut();
            // An inactive bank may be clock-gated. Only a live owner authorizes
            // peripheral MMIO; a shared vector is never disabled or unpended.
            if inner.active {
                regs.ier().write(|_| {});
            }
            inner.active = false;
            inner.waker = None;
            inner.error = None;
        });
    }

    fn cancel_wait(&self, regs: pac::spi::Spi) {
        critical_section::with(|cs| {
            let mut inner = self.inner.borrow(cs).borrow_mut();
            regs.ier().write(|_| {});
            inner.waker = None;
            // Keep the first error and the owner's outstanding frame intact.
        });
    }

    fn observe(
        &self,
        regs: pac::spi::Spi,
        cx: Option<&mut Context<'_>>,
        wait: PioWait,
    ) -> Result<pac::spi::regs::Isr, Error> {
        let arm = cx.is_some();
        critical_section::with(|cs| {
            let mut inner = self.inner.borrow(cs).borrow_mut();
            // Register before either software or hardware completion checks.
            if let Some(cx) = cx {
                if !inner
                    .waker
                    .as_ref()
                    .is_some_and(|w| w.will_wake(cx.waker()))
                {
                    inner.waker = Some(cx.waker().clone());
                }
            } else {
                inner.waker = None;
            }
            // Mask before the final snapshot as well as before changing waits.
            // A critical section alone does not stop peripheral flag changes.
            regs.ier().write(|_| {});
            let mut status = regs.isr().read();
            if let Some(error) = inner.error.or_else(|| decode_error(status)) {
                inner.error = Some(error);
                return Err(error);
            }
            if arm && !wait.ready(status) {
                regs.ier().write(|v| {
                    v.set_txe(matches!(wait, PioWait::Tx));
                    v.set_rxne(matches!(wait, PioWait::Rx));
                    v.set_modf(true);
                    v.set_ov(true);
                    v.set_sserr(true);
                    v.set_ud(true);
                });
                // Recheck after arming: readiness may precede IRQ delivery.
                status = regs.isr().read();
                if let Some(error) = decode_error(status) {
                    inner.error = Some(error);
                    regs.ier().write(|_| {});
                    return Err(error);
                }
            }
            if wait.ready(status) {
                regs.ier().write(|_| {});
                inner.waker = None;
            }
            Ok(status)
        })
    }
}

#[cfg(spi_async_pio)]
pub(super) fn dispatch_pio_interrupt(regs: pac::spi::Spi, state: &PioState) {
    let waker = critical_section::with(|cs| {
        let mut inner = state.inner.borrow(cs).borrow_mut();
        if !inner.active {
            return None;
        }
        let status = regs.isr().read();
        let enabled = regs.ier().read();
        let pending = (enabled.txe() && status.txe())
            || (enabled.rxne() && status.rxne())
            || (enabled.modf() && status.modf())
            || (enabled.ov() && status.ov())
            || (enabled.sserr() && status.sserr())
            || (enabled.ud() && status.ud());
        if !pending {
            return None;
        }
        if let Some(error) = decode_error(status) {
            inner.error.get_or_insert(error);
        }
        // Retain ready flags and data for the task; one notification suffices.
        regs.ier().write(|_| {});
        inner.waker.take()
    });
    if let Some(waker) = waker {
        waker.wake();
    }
}

#[cfg(spi_async_pio)]
impl<M: PeriMode> Spi<'_, M, Master> {
    pub(super) fn pio_observe(
        &self,
        cx: Option<&mut Context<'_>>,
        wait: PioWait,
    ) -> Result<pac::spi::regs::Isr, Error> {
        let info = self.peripheral.info;
        info.pio_state().observe(info.regs, cx, wait)
    }

    fn recover_pio(&mut self, error: Error) -> Error {
        let info = self.peripheral.info;
        critical_section::with(|cs| {
            let mut inner = info.pio_state().inner.borrow(cs).borrow_mut();
            info.regs.ier().write(|_| {});
            inner.waker = None;
            // Capture the error before destructive recovery. This neither
            // retries the byte nor proves the failed transaction completed.
            self.pio = Some(PioPhase::Idle);
            info.enable(false);
            info.clear();
            info.enable(true);
            inner.error = None;
        });
        error
    }

    pub(super) fn poll_pio_settle(
        &mut self,
        mut cx: Option<&mut Context<'_>>,
        read_used: &mut bool,
    ) -> Poll<Result<(), Error>> {
        if self.pio == Some(PioPhase::AwaitRx) {
            let status = self.pio_observe(cx.as_deref_mut(), PioWait::Rx)?;
            if !status.rxne() {
                return Poll::Pending;
            }
            if *read_used {
                if let Some(cx) = cx {
                    cx.waker().wake_by_ref();
                }
                return Poll::Pending;
            }
            let _ = self.peripheral.info.regs.dr().read().dr();
            *read_used = true;
            self.pio = Some(PioPhase::Finishing);
        }
        let status = self.pio_observe(cx.as_deref_mut(), PioWait::Idle)?;
        if status.txe() && !status.busy() {
            self.pio = Some(PioPhase::Idle);
            Poll::Ready(Ok(()))
        } else {
            // BUSY has no completion IRQ. No finite-liveness or sleep claim.
            if let Some(cx) = cx {
                cx.waker().wake_by_ref();
            }
            Poll::Pending
        }
    }

    pub(super) fn check_pio(&mut self) -> Result<(), Error> {
        if self.pio.is_none() {
            return Ok(());
        }
        match self.poll_pio_settle(None, &mut false) {
            Poll::Ready(Ok(())) => Ok(()),
            Poll::Ready(Err(error)) => Err(self.recover_pio(error)),
            Poll::Pending => Err(Error::PioBusy),
        }
    }
}

#[cfg(spi_async_pio)]
struct PioGuard<'a, 'd> {
    driver: &'a mut Spi<'d, Async, Master>,
}

#[cfg(spi_async_pio)]
impl Drop for PioGuard<'_, '_> {
    fn drop(&mut self) {
        let info = self.driver.peripheral.info;
        info.pio_state().cancel_wait(info.regs);
    }
}

#[cfg(spi_async_pio)]
pub(super) enum PioBuffers<'a, W: Word> {
    Separate { read: &'a mut [W], write: &'a [W] },
    InPlace(&'a mut [W]),
}

#[cfg(spi_async_pio)]
impl<W: Word> PioBuffers<'_, W> {
    fn len(&self) -> usize {
        match self {
            Self::Separate { read, write } => read.len().max(write.len()),
            Self::InPlace(words) => words.len(),
        }
    }
    fn transmit(&self, index: usize) -> W {
        match self {
            Self::Separate { write, .. } => write.get(index).copied().unwrap_or_default(),
            Self::InPlace(words) => words[index],
        }
    }
    fn receive(&mut self, index: usize, word: W) {
        match self {
            Self::Separate { read, .. } => {
                if let Some(destination) = read.get_mut(index) {
                    *destination = word;
                }
            }
            Self::InPlace(words) => words[index] = word,
        }
    }
}

#[cfg(spi_async_pio)]
impl<'d> Spi<'d, Async, Master> {
    /// Create a 4–16-bit master without DMA or static transfer storage. L012 SPI2
    /// and SPI3 require the complete `SharedInterruptHandler<SPI23>` binding.
    /// Tokens are consumed on error; preflight failure leaves hardware intact.
    /// Borrowed owners remain supported on families that also qualify for DMA.
    ///
    /// Cancellation/forget can leave one frame outstanding. Preserve the
    /// original CS selection and successfully await `flush` before changing it.
    /// Dropping the driver is bounded destructive teardown and may truncate a
    /// frame. Neither an error nor Drop is successful wire completion.
    pub fn new_async<T: Instance>(
        peripheral: Peri<'d, T>,
        sck: Peri<'d, impl SckPin<T>>,
        mosi: Peri<'d, impl MosiPin<T>>,
        miso: Peri<'d, impl MisoPin<T>>,
        _irqs: impl Binding<T::Interrupt, T::InterruptHandler> + 'd,
        config: Config,
    ) -> Result<Self, ConfigError> {
        let mut spi = Self::new_inner(peripheral, sck, mosi, miso, config)?;
        spi.pio = Some(PioPhase::Idle);
        critical_section::with(|cs| {
            let mut inner = T::pio_state().inner.borrow(cs).borrow_mut();
            inner.error = None;
            inner.waker = None;
            inner.active = true;
        });
        // Shared vectors stay enabled. Never clear the partner's pending IRQ.
        unsafe { T::Interrupt::enable() };
        Ok(spi)
    }

    pub(super) async fn flush_pio(&mut self) -> Result<(), Error> {
        let guard = PioGuard { driver: self };
        core::future::poll_fn(
            |cx| match guard.driver.poll_pio_settle(Some(cx), &mut false) {
                Poll::Ready(Err(error)) => Poll::Ready(Err(guard.driver.recover_pio(error))),
                other => other,
            },
        )
        .await
    }

    fn poll_pio_prepare<W: Word>(
        &mut self,
        cx: &mut Context<'_>,
        read_used: &mut bool,
    ) -> Poll<Result<(), Error>> {
        // Do not call prepare::<W>(): its wait_idle loop is unbounded. Width
        // changes and stale RX draining share this poll's one-read budget.
        let status = self.pio_observe(Some(cx), PioWait::Idle)?;
        if !status.txe() || status.busy() {
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        if self.word_bits != W::BITS {
            self.peripheral.info.enable(false);
            self.peripheral
                .info
                .regs
                .cr1()
                .modify(|v| v.set_width(W::BITS - 1));
            self.peripheral.info.enable(true);
            self.word_bits = W::BITS;
        }
        let status = self.pio_observe(Some(cx), PioWait::Idle)?;
        if !status.txe() || status.busy() {
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        if status.rxne() {
            if *read_used {
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
            let _ = self.peripheral.info.regs.dr().read().dr();
            *read_used = true;
        }
        Poll::Ready(Ok(()))
    }

    pub(super) async fn run_pio<W: Word>(
        &mut self,
        mut buffers: PioBuffers<'_, W>,
    ) -> Result<(), Error> {
        let guard = PioGuard { driver: self };
        let count = buffers.len();
        let mut settled = false;
        let mut prepared = false;
        let mut index = 0;
        core::future::poll_fn(|cx| {
            let result = (|| {
                // This one budget covers abandoned RX, stale preparation RX,
                // and the current operation. A poll writes at most one frame.
                let mut read_used = false;
                if !settled {
                    match guard.driver.poll_pio_settle(Some(cx), &mut read_used) {
                        Poll::Ready(Ok(())) => settled = true,
                        other => return other,
                    }
                }
                if count == 0 {
                    return Poll::Ready(Ok(()));
                }
                if !prepared {
                    match guard.driver.poll_pio_prepare::<W>(cx, &mut read_used) {
                        Poll::Ready(Ok(())) => prepared = true,
                        other => return other,
                    }
                }
                if guard.driver.pio == Some(PioPhase::AwaitRx) {
                    let status = guard.driver.pio_observe(Some(cx), PioWait::Rx)?;
                    if !status.rxne() {
                        return Poll::Pending;
                    }
                    if read_used {
                        cx.waker().wake_by_ref();
                        return Poll::Pending;
                    }
                    let word = W::from_u16(guard.driver.peripheral.info.regs.dr().read().dr());
                    read_used = true;
                    guard.driver.pio = Some(PioPhase::Finishing);
                    buffers.receive(index, word);
                    index += 1;
                }
                if index == count {
                    return guard.driver.poll_pio_settle(Some(cx), &mut read_used);
                }
                let status = guard.driver.pio_observe(Some(cx), PioWait::Tx)?;
                if !status.txe() {
                    return Poll::Pending;
                }
                let word = buffers.transmit(index);
                // Retained marker precedes DR. No await exists in this command.
                guard.driver.pio = Some(PioPhase::AwaitRx);
                guard
                    .driver
                    .peripheral
                    .info
                    .regs
                    .dr()
                    .write(|v| v.set_dr(word.into_u16()));
                let status = guard.driver.pio_observe(Some(cx), PioWait::Rx)?;
                if status.rxne() {
                    // Already ready, but never consume a second frame/read or
                    // launch another frame in this poll. The ISR is one-shot.
                    cx.waker().wake_by_ref();
                }
                Poll::Pending
            })();
            match result {
                Poll::Ready(Err(error)) => Poll::Ready(Err(guard.driver.recover_pio(error))),
                other => other,
            }
        })
        .await
    }
}
