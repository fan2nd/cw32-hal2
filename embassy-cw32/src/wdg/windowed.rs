//! Reset-mode CW32 window watchdog (WWDT), with optional L012 IRQ notification.
//!
//! This is separate from the independent watchdog. A valid configuration has
//! `0x40 <= window < reload <= 0x7f`; refresh is permitted only while the live
//! counter is `0x40..=window`. EN and IE cannot be cleared until reset. This
//! driver never stops the counter, pulses peripheral reset, or changes debugger, sleep or clock-tree settings.
//!
//! Timing is nominal PCLK-running time. DeepSleep stops WWDT; debugger freeze
//! can stop it too. Normal Sleep retains counting. Direct changes to global
//! clocks (including wake clock selection), reset or gates invalidate the
//! driver's frozen-clock assumptions. The application must keep PCLK and the
//! gate running and leave WWDT exclusively owned after start, including after
//! dropping this object. Drop has no hardware side effects. L012 optionally enables
//! permanent early-warning IRQ service; dropping its owner only removes the waiter.
//!
//! A checked refresh is not a guarantee against reset: the counter can reach
//! 0x3f between its read and the write. Critical sections exclude ordinary
//! interrupt preemption, but cannot stop hardware counting, NMI, debug halts,
//! bus stalls or faults. Leave scheduling margin; do not target the last tick.
//! See `docs/window-watchdog-evidence.md` for sources and validation limits.

use crate::{Peri, PeripheralType, pac, time::Hertz};

/// All eight documented divisors of PCLK, with the exact PRS encodings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(u8)]
pub enum WindowPrescaler {
    /// PCLK / 4096.
    Div4096 = 0,
    /// PCLK / 8192.
    Div8192 = 1,
    /// PCLK / 16384.
    Div16384 = 2,
    /// PCLK / 32768.
    Div32768 = 3,
    /// PCLK / 65536.
    Div65536 = 4,
    /// PCLK / 131072.
    Div131072 = 5,
    /// PCLK / 262144.
    Div262144 = 6,
    /// PCLK / 524288.
    Div524288 = 7,
}
impl WindowPrescaler {
    /// Number of PCLK cycles per counter tick.
    pub const fn divisor(self) -> u32 {
        4096 << self as u8
    }
}
const PRESCALERS: [WindowPrescaler; 8] = [
    WindowPrescaler::Div4096,
    WindowPrescaler::Div8192,
    WindowPrescaler::Div16384,
    WindowPrescaler::Div32768,
    WindowPrescaler::Div65536,
    WindowPrescaler::Div131072,
    WindowPrescaler::Div262144,
    WindowPrescaler::Div524288,
];

/// Invalid configuration or a checked hardware operation failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
pub enum WindowError {
    /// Counts must satisfy 0x40 <= window < reload <= 0x7f.
    InvalidCounts,
    /// Requested nominal intervals cannot represent a nonempty closed/open window.
    IntervalsOutOfRange,
    /// RCC has not published a nonzero frozen PCLK.
    ClockNotInitialized,
    /// A zero read budget is invalid.
    InvalidPollLimit,
    /// The clock gate did not read enabled within the read budget.
    ClockEnableTimeout,
    /// The active-low peripheral reset is asserted; this driver never releases it.
    HeldInReset,
    /// Hardware or this instance has already started; no refresh was performed.
    AlreadyRunning,
    /// An inherited non-reset configuration, IE or status was found.
    NotReset,
    /// Configuration or preload did not read back within the read budget.
    ConfigurationTimeout,
    /// EN did not read set within the read budget. The watchdog may be running.
    StartTimeout,
    /// An irreversible startup attempt failed. Reset is required before retrying.
    StartupFailed,
    /// This instance was not successfully started, or hardware EN is clear.
    NotRunning,
    /// Early-warning notification was not enabled on this running watchdog.
    WarningNotEnabled,
    /// IE did not read back set; permanent IRQ servicing remains installed.
    WarningEnableFailed,
    /// The counter is still above WINR. No refresh write was issued.
    TooEarly,
    /// The counter is below 0x40. Reset may already be in progress; no write issued.
    TooLate,
}

/// Validated count-based configuration; construction performs no MMIO.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WindowConfig {
    prescaler: WindowPrescaler,
    reload: u8,
    window: u8,
    poll_limit: u32,
}
impl WindowConfig {
    /// Validate exact counts, with a default 100,000-read budget per readback.
    pub const fn from_counts(
        prescaler: WindowPrescaler,
        reload: u8,
        window: u8,
    ) -> Result<Self, WindowError> {
        if window < 0x40 || window >= reload || reload > 0x7f {
            return Err(WindowError::InvalidCounts);
        }
        Ok(Self {
            prescaler,
            reload,
            window,
            poll_limit: 100_000,
        })
    }
    /// Set a nonzero maximum number of reads per hardware verification.
    ///
    /// This bounds reads, not wall time under arbitrary preemption or stalls.
    pub const fn with_poll_limit(mut self, limit: u32) -> Result<Self, WindowError> {
        if limit == 0 {
            return Err(WindowError::InvalidPollLimit);
        }
        self.poll_limit = limit;
        Ok(self)
    }
    /// Selected prescaler.
    pub const fn prescaler(self) -> WindowPrescaler {
        self.prescaler
    }
    /// Initial and refresh counter value.
    pub const fn reload(self) -> u8 {
        self.reload
    }
    /// Inclusive upper live-count boundary at which refreshing is permitted.
    pub const fn window(self) -> u8 {
        self.window
    }
    /// Register-read budget per startup verification.
    pub const fn poll_limit(self) -> u32 {
        self.poll_limit
    }
}

/// Exact represented PCLK-cycle intervals and nominal frequency.
///
/// Each duration is the corresponding `*_pclk_cycles()` divided by `pclk().0`
/// seconds. Microsecond helpers round down and are not measured deadlines.
/// Prescaler phase at refresh, oscillator error and bus/firmware latency are
/// not included. The caller must use the actual frozen PCLK for calculations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WindowTiming {
    config: WindowConfig,
    pclk: Hertz,
}
impl WindowTiming {
    /// Calculate using a validated configuration and nominal frozen PCLK.
    pub fn from_config(config: WindowConfig, frozen_pclk: Hertz) -> Result<Self, WindowError> {
        if frozen_pclk.0 == 0 {
            return Err(WindowError::ClockNotInitialized);
        }
        Ok(Self {
            config,
            pclk: frozen_pclk,
        })
    }
    /// Select the smallest divisor representing both requested nominal intervals.
    ///
    /// Reset and closed-window ticks are separately rounded up. `closed_us` must
    /// be positive and smaller than `timeout_us`; both open and closed windows
    /// must remain nonempty after rounding. A request rounding to fewer than two
    /// ticks is rejected rather than removing the closed window. All arithmetic
    /// uses u64; even maximum u32 inputs cannot overflow the products.
    pub fn for_intervals(
        timeout_us: u32,
        closed_us: u32,
        frozen_pclk: Hertz,
    ) -> Result<Self, WindowError> {
        if frozen_pclk.0 == 0 {
            return Err(WindowError::ClockNotInitialized);
        }
        if closed_us == 0 || closed_us >= timeout_us {
            return Err(WindowError::IntervalsOutOfRange);
        }
        for prescaler in PRESCALERS {
            let denominator = u64::from(prescaler.divisor()) * 1_000_000;
            let reset_ticks =
                (u64::from(timeout_us) * u64::from(frozen_pclk.0)).div_ceil(denominator);
            let closed_ticks =
                (u64::from(closed_us) * u64::from(frozen_pclk.0)).div_ceil(denominator);
            if (2..=64).contains(&reset_ticks) && (1..reset_ticks).contains(&closed_ticks) {
                let reload = (63 + reset_ticks) as u8;
                return Self::from_config(
                    WindowConfig::from_counts(prescaler, reload, reload - closed_ticks as u8)?,
                    frozen_pclk,
                );
            }
        }
        Err(WindowError::IntervalsOutOfRange)
    }
    /// Validated represented counts, suitable for [`WindowWatchdog::new`].
    pub const fn config(self) -> WindowConfig {
        self.config
    }
    /// Nominal PCLK used in the calculation.
    pub const fn pclk(self) -> Hertz {
        self.pclk
    }
    /// Nominal PCLK cycles from reload to the window opening.
    pub const fn closed_pclk_cycles(self) -> u32 {
        self.config.prescaler.divisor() * (self.config.reload - self.config.window) as u32
    }
    /// Nominal PCLK cycles from reload to the reset threshold.
    pub const fn timeout_pclk_cycles(self) -> u32 {
        self.config.prescaler.divisor() * (self.config.reload - 0x3f) as u32
    }
    /// Nominal PCLK cycles from the window opening to the reset threshold.
    pub const fn open_pclk_cycles(self) -> u32 {
        self.config.prescaler.divisor() * (self.config.window - 0x3f) as u32
    }
    /// Nominal closed-window microseconds, rounded down.
    pub fn nominal_closed_us(self) -> u64 {
        u64::from(self.closed_pclk_cycles()) * 1_000_000 / u64::from(self.pclk.0)
    }
    /// Nominal reload-to-reset microseconds, rounded down.
    pub fn nominal_timeout_us(self) -> u64 {
        u64::from(self.timeout_pclk_cycles()) * 1_000_000 / u64::from(self.pclk.0)
    }
    /// Nominal open-window microseconds, rounded down.
    pub fn nominal_open_us(self) -> u64 {
        u64::from(self.open_pclk_cycles()) * 1_000_000 / u64::from(self.pclk.0)
    }
}

/// A polling snapshot. Separate register reads are not an atomic hardware sample.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WindowStatus {
    /// Live count observed in CR0.
    pub counter: u8,
    /// EN observed in the same CR0 read.
    pub running: bool,
    /// Whether the observed counter was in the configured refresh window.
    /// It can become stale before the next instruction; use [`WindowWatchdog::try_pet`].
    pub window_open: bool,
    /// Sticky POV: the counter reached 0x40 at least once since reset/clear.
    /// Without IRQ service this is sticky. L012 early-warning IRQ service clears
    /// it; the separate coalesced software notification is not reflected here.
    pub preoverflow: bool,
}

/// Exclusively owned reset-mode window watchdog, with optional L012 notification.
///
/// Embassy's `new` / `unleash` / `pet` vocabulary is retained, but preparation
/// never starts hardware. Unlike IWDT, repeated `unleash` returns AlreadyRunning
/// without touching hardware and never implicitly refreshes inside a closed window.
pub struct WindowWatchdog<'d, T: WindowInstance> {
    _peripheral: Peri<'d, T>,
    timing: WindowTiming,
    state: State,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum State {
    Prepared,
    Starting,
    Running,
}
impl<'d, T: WindowInstance> WindowWatchdog<'d, T> {
    /// Prepare from exact counts, returning configuration errors to the caller.
    /// Only reads the RAM-backed frozen RCC snapshot; no MMIO is performed.
    pub fn new(instance: Peri<'d, T>, config: WindowConfig) -> Result<Self, WindowError> {
        let pclk = crate::rcc::try_clocks()
            .ok_or(WindowError::ClockNotInitialized)?
            .pclk;
        Ok(Self {
            _peripheral: instance,
            timing: WindowTiming::from_config(config, pclk)?,
            state: State::Prepared,
        })
    }
    /// Exact represented counts and nominal timing at the frozen clock frequency.
    pub const fn timing(&self) -> WindowTiming {
        self.timing
    }
    /// Start once. Panics on failure, including a repeated call; never feeds.
    pub fn unleash(&mut self) {
        self.try_unleash()
            .unwrap_or_else(|e| panic!("WWDT startup failed: {:?}", e));
    }
    /// Enable the gate, verify reset hardware, configure and start irreversibly.
    ///
    /// Refuses inherited EN, IE, modified counts or status. No takeover, reset
    /// pulse, interrupt enable or initial post-start refresh is attempted. Only
    /// EN is polled after activation; the live counter is allowed to decay.
    /// Failure after the EN write leaves this instance poisoned, potentially
    /// running, with its gate retained. Reset to recover. Earlier failures can
    /// leave non-reset configuration and are not rolled back. Read limits are
    /// not wall-time limits; the hardware may reset before this function returns.
    pub fn try_unleash(&mut self) -> Result<(), WindowError> {
        self.state.check_start()?;
        critical_section::with(|cs| self.prepare_clock(cs))?;
        self.start()
    }
    /// Refresh in the live open window; panics on a detected error.
    pub fn pet(&mut self) {
        self.try_pet()
            .unwrap_or_else(|e| panic!("WWDT refresh failed: {:?}", e));
    }
    /// Check the live counter and write EN|reload in one transaction if allowed.
    ///
    /// TooEarly, TooLate and NotRunning perform no refresh write. Does not spin
    /// waiting for a window. Ordinary interrupts are masked over read/check/write,
    /// but counter progress and unmaskable delays can still cause reset. In
    /// particular, success at 0x40 is not proof the write beat the next tick.
    pub fn try_pet(&mut self) -> Result<(), WindowError> {
        if self.state != State::Running {
            return Err(WindowError::NotRunning);
        }
        critical_section::with(|_| self.refresh())
    }
    /// Observe count and sticky POV without clearing or feeding anything.
    ///
    /// Only available after this instance starts successfully. It is a snapshot,
    /// not permission to refresh later without a new live-count check.
    pub fn try_status(&self) -> Result<WindowStatus, WindowError> {
        if self.state != State::Running {
            return Err(WindowError::NotRunning);
        }
        Ok(self.status())
    }
}
trait SealedWindowInstance {
    fn regs() -> pac::wwdt::Wwdt;
}
/// WWDT instance, sealed to the generated hardware singleton.
#[allow(private_bounds)]
pub trait WindowInstance: SealedWindowInstance + PeripheralType {}
impl SealedWindowInstance for crate::peripherals::WWDT {
    fn regs() -> pac::wwdt::Wwdt {
        pac::WWDT
    }
}
impl WindowInstance for crate::peripherals::WWDT {}

impl State {
    fn check_start(self) -> Result<(), WindowError> {
        match self {
            State::Prepared => Ok(()),
            State::Starting => Err(WindowError::StartupFailed),
            State::Running => Err(WindowError::AlreadyRunning),
        }
    }
}

impl<'d, T: WindowInstance> WindowWatchdog<'d, T> {
    fn poll(&self, ready: impl Fn() -> bool, error: WindowError) -> Result<(), WindowError> {
        for _ in 0..self.timing.config.poll_limit {
            if ready() {
                return Ok(());
            }
            core::hint::spin_loop();
        }
        Err(error)
    }

    fn prepare_clock(&self, cs: critical_section::CriticalSection<'_>) -> Result<(), WindowError> {
        use crate::rcc::{Readback, SealedRccPeripheral};
        let rcc = crate::peripherals::WWDT::RCC_INFO;
        if rcc.reset_asserted() {
            return Err(WindowError::HeldInReset);
        }
        rcc.enable_with_cs_readback(
            cs,
            Readback::Poll {
                attempts: self.timing.config.poll_limit,
                spin: true,
            },
        )
        .map_err(|_| WindowError::ClockEnableTimeout)
    }

    fn start(&mut self) -> Result<(), WindowError> {
        self.state.check_start()?;
        let regs = T::regs();
        let config = self.timing.config;
        let cr0 = regs.cr0().read();
        if cr0.en() {
            return Err(WindowError::AlreadyRunning);
        }
        let mut reset_cr0 = pac::wwdt::regs::Cr0(0);
        reset_cr0.set_wcnt(0x7f);
        let mut reset_cr1 = pac::wwdt::regs::Cr1(0);
        reset_cr1.set_winr(0x7f);
        // Whole-register comparisons retain refusal of reserved/non-reset bits.
        if cr0 != reset_cr0
            || regs.cr1().read() != reset_cr1
            || regs.sr().read() != pac::wwdt::regs::Sr(0)
        {
            return Err(WindowError::NotReset);
        }
        // Full zero-seeded writes keep IE and reserved bits clear, without reads.
        let mut cr1 = pac::wwdt::regs::Cr1(0);
        cr1.set_winr(config.window);
        cr1.set_prs(config.prescaler as u8);
        debug_assert!(!cr1.ie());
        regs.cr1().write_value(cr1);
        self.poll(
            || regs.cr1().read() == cr1,
            WindowError::ConfigurationTimeout,
        )?;
        let mut preload = pac::wwdt::regs::Cr0(0);
        preload.set_wcnt(config.reload);
        regs.cr0().write_value(preload);
        self.poll(
            || regs.cr0().read() == preload,
            WindowError::ConfigurationTimeout,
        )?;
        // Once EN has possibly reached hardware, no implicit restart/feed is safe.
        self.state = State::Starting;
        let mut running = preload;
        running.set_en(true);
        regs.cr0().write_value(running);
        self.poll(|| regs.cr0().read().en(), WindowError::StartTimeout)?;
        self.state = State::Running;
        Ok(())
    }

    fn refresh(&self) -> Result<(), WindowError> {
        let regs = T::regs();
        let config = self.timing.config;
        let cr0 = regs.cr0().read();
        if !cr0.en() {
            return Err(WindowError::NotRunning);
        }
        let counter = cr0.wcnt();
        if counter > config.window {
            return Err(WindowError::TooEarly);
        }
        if counter < 0x40 {
            return Err(WindowError::TooLate);
        }
        // Never modify a read copy of the decaying counter; always write EN|reload.
        regs.cr0().write(|v| {
            v.set_en(true);
            v.set_wcnt(config.reload);
        });
        Ok(())
    }

    fn status(&self) -> WindowStatus {
        let regs = T::regs();
        let cr0 = regs.cr0().read();
        let counter = cr0.wcnt();
        let running = cr0.en();
        WindowStatus {
            counter,
            running,
            window_open: running && (0x40..=self.timing.config.window).contains(&counter),
            preoverflow: regs.sr().read().pov(),
        }
    }
}

/// Permanent L012 WWDT source handler on the shared WDT vector.
/// Bind any independently enabled IWDT source separately on the same vector.
#[cfg(cw32l012)]
pub use warning::InterruptHandler;

#[cfg(cw32l012)]
impl<T: WindowInstance> WindowWatchdog<'_, T> {
    /// Enable irreversible early-warning IRQ service on an already running WWDT.
    ///
    /// The static handler remains active until reset, including after an error,
    /// cancelled/forgotten wait or owner Drop. Repeated enable is idempotent and
    /// does not discard notifications. This never feeds the watchdog. One tick
    /// remains after POV; neither IRQ latency nor task delivery before reset is
    /// guaranteed. Keep the running clock/gate and this static binding intact.
    pub fn enable_early_warning(
        &mut self,
        _irq: impl crate::interrupt::typelevel::Binding<
            crate::interrupt::typelevel::WDT,
            InterruptHandler,
        >,
    ) -> Result<(), WindowError> {
        if self.state != State::Running {
            return Err(WindowError::NotRunning);
        }
        warning::enable()
    }

    /// Consume one coalesced warning, or wait for one after enabling the service.
    ///
    /// Notifications are latched even without a waiter, not counted. Cancelling
    /// a pending wait preserves the latch and only removes its waker. A completed
    /// wait consumes it. Forgetting a future cannot leave borrowed memory in the
    /// ISR; the next wait or owner Drop retires the old owned waker. No automatic
    /// refresh occurs and this cannot guarantee time to react before reset.
    pub async fn wait_early_warning(&mut self) -> Result<(), WindowError> {
        if self.state != State::Running {
            return Err(WindowError::NotRunning);
        }
        warning::wait().await
    }
}
impl<T: WindowInstance> Drop for WindowWatchdog<'_, T> {
    fn drop(&mut self) {
        #[cfg(cw32l012)]
        if self.state == State::Running {
            drop(critical_section::with(warning::retire_waiter));
        }
    }
}

#[cfg(cw32l012)]
mod warning {
    use super::*;
    use crate::interrupt::typelevel::{Handler, Interrupt, WDT as Irq};
    use core::{
        cell::RefCell,
        future::poll_fn,
        task::{Poll, Waker},
    };
    struct WarningState {
        servicing: bool,
        pending: bool,
        waker: Option<Waker>,
    }
    static WARNING: critical_section::Mutex<RefCell<WarningState>> =
        critical_section::Mutex::new(RefCell::new(WarningState {
            servicing: false,
            pending: false,
            waker: None,
        }));

    pub struct InterruptHandler;
    impl Handler<Irq> for InterruptHandler {
        unsafe fn on_interrupt() {
            let wake = critical_section::with(|cs| {
                let mut state = WARNING.borrow(cs).borrow_mut();
                if !state.servicing {
                    return None;
                }
                if !pac::WWDT.sr().read().pov() {
                    return None;
                }
                // L012 UM §20.6.3: sole W0C POV, reserved bits zero.
                pac::WWDT.sr().write_value(pac::wwdt::regs::Sr(0));
                state.pending = true;
                state.waker.take()
            });
            if let Some(waker) = wake {
                waker.wake();
            }
        }
    }
    pub(super) fn enable() -> Result<(), WindowError> {
        use crate::rcc::SealedRccPeripheral;
        critical_section::with(|cs| {
            let rcc = crate::peripherals::WWDT::RCC_INFO;
            if rcc.reset_asserted() {
                return Err(WindowError::HeldInReset);
            }
            if !rcc.is_enabled() {
                return Err(WindowError::NotRunning);
            }
            if !pac::WWDT.cr0().read().en() {
                return Err(WindowError::NotRunning);
            }
            // Publish permanent ownership BEFORE the irreversible write. Do not
            // undo this marker on readback failure: IE may have reached hardware.
            WARNING.borrow(cs).borrow_mut().servicing = true;
            unsafe {
                Irq::enable();
            }
            if !pac::WWDT.cr1().read().ie() {
                pac::WWDT.cr1().modify(|w| w.set_ie(true));
            }
            if pac::WWDT.cr1().read().ie() {
                Ok(())
            } else {
                Err(WindowError::WarningEnableFailed)
            }
        })
    }
    pub(super) fn retire_waiter(cs: critical_section::CriticalSection<'_>) -> Option<Waker> {
        WARNING.borrow(cs).borrow_mut().waker.take()
    }
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            drop(critical_section::with(retire_waiter));
        }
    }
    pub(super) async fn wait() -> Result<(), WindowError> {
        drop(critical_section::with(retire_waiter));
        if !critical_section::with(|cs| WARNING.borrow(cs).borrow().servicing) {
            return Err(WindowError::WarningNotEnabled);
        }
        let _guard = Guard;
        poll_fn(|cx| {
            let mut next = Some(cx.waker().clone());
            let (ready, old) = critical_section::with(|cs| {
                let mut state = WARNING.borrow(cs).borrow_mut();
                if state.pending {
                    state.pending = false;
                    (true, state.waker.take())
                } else {
                    (false, core::mem::replace(&mut state.waker, next.take()))
                }
            });
            drop(old);
            if ready {
                Poll::Ready(Ok(()))
            } else {
                Poll::Pending
            }
        })
        .await
    }
}
