//! Optional global Embassy clock, nominally 1 MHz, for continuous run mode.
//!
//! Select `time-driver-gtim` (F002/F003) or `time-driver-gtim1` (other families).
//! The whole selected timer and its IRQ are reserved before peripheral exposure.
//! CH1 counts half-wraps; CH2 services the pinned Embassy software timer queue.
//!
//! Correct elapsed time and alarm timeliness require every half-boundary to be
//! serviced in strictly less than 32768 actual ticks, including critical sections,
//! higher-priority handlers, flash stalls, and queue/waker execution. The
//! conservative wall-time bound for direct HSI is <31.207619 ms for ±5% and
//! <32.125490 ms for ±2%. For an external source with cycle-timing qualification,
//! use its actual maximum tick rate to calculate 32768 ticks. New PLL-derived
//! rate-only bounds do not establish a strict wall-time budget: the obligation
//! remains fewer than 32768 actual ticks unless separately qualified by the board.
//! These are constraints, not measured latency guarantees.
//! Arbitrary IRQ masking, debugger halts, clock gating and deep sleep are not
//! supported. A monotonic clamp prevents backward timestamps after lost wraps;
//! it cannot recover elapsed time or timely alarms. The u64 epoch has no short
//! u32 wrap horizon. `now()` returns zero before initialization, without MMIO.
//!
//! NMI/HardFault `now()` returns the last published timestamp without taking a
//! critical section or reading timer registers. Scheduling from NMI/HardFault or
//! reentrantly from a Waker callback is detected and panics before aliasing queue
//! state. Normal Waker callbacks must stay within the blackout budget; they may
//! read now(). The pinned queue does not offer deferred callback delivery.
//! The default integrated queue requires Embassy wakers and exactly one such
//! queue in the application. Other executors select a generic queue feature on
//! the same pinned `embassy-time-queue-utils` dependency.
//!
//! Tick rate is nominal: selected-source accuracy and the declared voltage/ambient
//! conditions still apply. Use [`tick_bounds`] to inspect the actual envelope.
//! Embassy durations do not guarantee minimum wall-clock delays or UTC accuracy.
//! See `docs/time-driver.md` for the source/contract and race analysis.

use core::cell::{Cell, RefCell};
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use core::task::Waker;

use critical_section::CriticalSection;
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_time_driver::{Driver, TICK_HZ};
use embassy_time_queue_utils::Queue;

use crate::interrupt::typelevel::Interrupt;
use crate::rcc::{ClockBounds, SealedRccPeripheral};
use crate::{TimeDriverInterrupt, TimeDriverPeripheral};

#[cfg(gtim_classic)]
mod classic;
#[cfg(gtim_classic)]
use classic as hardware;
#[cfg(gtim_buffered)]
mod buffered;
#[cfg(gtim_buffered)]
use buffered as hardware;

const HALF: u64 = 1 << 15;
const ARM_AHEAD: u64 = 3 << 14;
const _: () = assert!(TICK_HZ == 1_000_000);

// Single-core publication: a normal-context writer holds the global critical
// section. NMI/HardFault only read the active slot, so the interrupted writer
// cannot resume until they finish. Release publishes both atomic words together.
// This avoids an NMI spinning forever on an interrupted seqlock writer.
struct PublishedTime {
    low: [AtomicU32; 2],
    high: [AtomicU32; 2],
    active: AtomicU32,
}
impl PublishedTime {
    const fn new() -> Self {
        Self {
            low: [AtomicU32::new(0), AtomicU32::new(0)],
            high: [AtomicU32::new(0), AtomicU32::new(0)],
            active: AtomicU32::new(0),
        }
    }
    // Called only under the critical section or from NMI/HardFault.
    fn read(&self) -> u64 {
        let i = self.active.load(Ordering::Acquire) as usize;
        (u64::from(self.high[i].load(Ordering::Relaxed)) << 32)
            | u64::from(self.low[i].load(Ordering::Relaxed))
    }
    fn publish(&self, _cs: CriticalSection<'_>, value: u64) {
        let i = 1 - self.active.load(Ordering::Relaxed) as usize;
        self.low[i].store(value as u32, Ordering::Relaxed);
        self.high[i].store((value >> 32) as u32, Ordering::Relaxed);
        self.active.store(i as u32, Ordering::Release);
    }
}

struct TimeDriver {
    initialized: AtomicBool,
    period: Mutex<CriticalSectionRawMutex, Cell<u64>>,
    published: PublishedTime,
    bounds: Mutex<CriticalSectionRawMutex, Cell<Option<ClockBounds>>>,
    queue: Mutex<CriticalSectionRawMutex, RefCell<Queue>>,
}

embassy_time_driver::time_driver_impl!(static DRIVER: TimeDriver = TimeDriver {
    initialized: AtomicBool::new(false),
    period: Mutex::new(Cell::new(0)),
    published: PublishedTime::new(),
    bounds: Mutex::new(Cell::new(None)),
    queue: Mutex::new(RefCell::new(Queue::new())),
});

// Cortex-M0/M0+ NMI and HardFault are the only handlers not masked by PRIMASK.
// Reading IPSR does not touch MMIO and is valid before HAL initialization.
fn unmaskable_context() -> bool {
    #[cfg(target_arch = "arm")]
    {
        let ipsr: u32;
        // SAFETY: reads a core status register, does not change processor state.
        unsafe {
            core::arch::asm!("mrs {}, IPSR", out(reg) ipsr, options(nomem, nostack, preserves_flags))
        };
        matches!(ipsr & 0x1ff, 2 | 3)
    }
    #[cfg(not(target_arch = "arm"))]
    {
        false
    }
}

impl TimeDriver {
    fn now_with_cs(&self, cs: CriticalSection<'_>) -> u64 {
        if !self.initialized.load(Ordering::Acquire) {
            return 0;
        }
        let period = self.period.borrow(cs).get();
        let counter = crate::TIME_DRIVER_REGS.cnt().read().cnt();
        // A single pending half-boundary is inferred by counter/period parity.
        // Saturation matters only after the full u64 timestamp horizon (~584k y).
        let base = period.saturating_mul(HALF);
        let candidate = base.saturating_add(u64::from(counter ^ (((period & 1) as u16) << 15)));
        let now = candidate.max(self.published.read());
        self.published.publish(cs, now);
        now
    }

    fn set_alarm(&self, cs: CriticalSection<'_>, at: u64) -> bool {
        hardware::alarm_irq(false);
        if at == u64::MAX {
            return true;
        }
        let now = self.now_with_cs(cs);
        if at <= now {
            return false;
        }
        // Clear before programming, preserving all other pending flags. A flag
        // arriving afterwards is harmless: the IRQ checks absolute queue times.
        hardware::clear_alarm();
        hardware::write_alarm(at as u16);
        hardware::alarm_irq(at - now < ARM_AHEAD);
        if at <= self.now_with_cs(cs) {
            hardware::alarm_irq(false);
            return false;
        }
        true
    }

    fn service_queue(&self, cs: CriticalSection<'_>, queue: &mut Queue) {
        let mut next = queue.next_expiration(self.now_with_cs(cs));
        while !self.set_alarm(cs, next) {
            next = queue.next_expiration(self.now_with_cs(cs));
        }
    }
}

impl Driver for TimeDriver {
    fn now(&self) -> u64 {
        if unmaskable_context() {
            return self.published.read();
        }
        critical_section::with(|cs| self.now_with_cs(cs))
    }

    fn schedule_wake(&self, at: u64, waker: &Waker) {
        assert!(
            !unmaskable_context(),
            "Embassy wake scheduling is unavailable in NMI/HardFault"
        );
        critical_section::with(|cs| {
            // The pinned Queue invokes callbacks under its mutable borrow, and
            // has no drain/deferred-wake API. Reentry therefore fails closed,
            // like upstream, instead of recursively waking the caller again.
            let mut queue = self
                .queue
                .borrow(cs)
                .try_borrow_mut()
                .expect("reentrant Embassy wake scheduling is unavailable");
            if queue.schedule_wake(at, waker) && self.initialized.load(Ordering::Acquire) {
                self.service_queue(cs, &mut queue);
            }
            // Pre-init entries survive in the queue; init services them after
            // starting the clock. No timer registers are accessed here yet.
        });
    }
}

/// Actual tick-rate envelope after successful initialization; `None` before it
/// or from NMI/HardFault, where the PRIMASK-protected state cannot be inspected.
/// Requires unchanged clocks, factory trim and the declared qualified conditions.
pub fn tick_bounds() -> Option<ClockBounds> {
    if unmaskable_context() {
        return None;
    }
    critical_section::with(|cs| DRIVER.bounds.borrow(cs).get())
}

pub(crate) fn validate_clock(pclk: ClockBounds) -> Result<u32, crate::rcc::Error> {
    let divisor = pclk
        .exact_divisor_for(TICK_HZ as u32)
        .ok_or(crate::rcc::Error::UnsupportedTimeDriverClock)?;
    #[cfg(any(gtim_v1, gtim_cw32f002_v1))]
    let supported = divisor.is_power_of_two() && divisor <= 32768;
    #[cfg(not(any(gtim_v1, gtim_cw32f002_v1)))]
    let supported = divisor <= 65536;
    if !supported {
        return Err(crate::rcc::Error::UnsupportedTimeDriverClock);
    }
    Ok(divisor)
}

pub(crate) fn init(divisor: u32) -> Result<(), crate::rcc::Error> {
    critical_section::with(|cs| {
        TimeDriverInterrupt::disable();
        TimeDriverPeripheral::RCC_INFO
            .enable_and_reset_with_cs(cs)
            .map_err(|_| crate::rcc::Error::TimeDriverClockFailure)?;
        if !TimeDriverPeripheral::RCC_INFO.is_enabled()
            || TimeDriverPeripheral::RCC_INFO.reset_asserted()
        {
            return Err(crate::rcc::Error::TimeDriverClockFailure);
        }
        hardware::initialize(divisor);
        TimeDriverInterrupt::unpend();
        TimeDriverInterrupt::set_priority_with_cs(cs, crate::interrupt::Priority::P0);
        DRIVER
            .bounds
            .borrow(cs)
            .set(crate::rcc::try_clocks().map(|c| c.pclk_bounds().divided_by(divisor)));
        hardware::start();
        DRIVER.initialized.store(true, Ordering::Release);
        DRIVER.service_queue(cs, &mut DRIVER.queue.borrow(cs).borrow_mut());
        // SAFETY: the generated vector is installed and every shared state is
        // initialized; selected timer/IRQ ownership is exclusive for this image.
        unsafe { TimeDriverInterrupt::enable() };
        Ok(())
    })
}

pub(crate) fn on_interrupt() {
    critical_section::with(|cs| {
        if !DRIVER.initialized.load(Ordering::Acquire) {
            return;
        }
        let boundaries = hardware::take_interrupt();
        let period = DRIVER.period.borrow(cs);
        period.set(period.get().saturating_add(u64::from(boundaries)));
        DRIVER.service_queue(cs, &mut DRIVER.queue.borrow(cs).borrow_mut());
    });
}
