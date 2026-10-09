//! Independent watchdog (IWDT), following Embassy's `new` / `unleash` / `pet` API.
//!
//! CW32 requires the watchdog to run before its configuration can be written.
//! Consequently, `new` only calculates settings; `unleash` starts, configures and
//! refreshes the hardware. It refuses to take over a running watchdog or one whose
//! configuration differs from the reset values. There is no implicit stop, reset, clock disable or feed
//! on drop. Although these devices document stop keys, this driver exposes none.
//!
//! Timeout calculations use the selected family's fastest published oscillator
//! frequency, rather than the SDKs' often incorrect `IWDT_FREQ = 10000` macro.
//! The requested period is rounded up in counter ticks. This accounts for the
//! published clock tolerance, but is not a measured wall-time guarantee: reset
//! propagation, asynchronous synchronization, startup and firmware scheduling
//! are additional effects. See `docs/watchdog-evidence.md` for source conditions.
//! L010/L011 use LSI; the other supported families use a dedicated RC10K clock.
//! A running watchdog continues in DeepSleep. Existing debugger-freeze settings
//! are left unchanged. No interrupt, window mode or reset-cause clearing is used.

use crate::{Peri, PeripheralType, pac};

/// Documented oscillator range, before the watchdog's prescaler.
///
/// Limits are derived from the electrical table's typical frequency and its
/// wide-temperature factory accuracy. They assume the documented supply and
/// temperature conditions, and unmodified factory calibration (especially LSI).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClockRange {
    /// Typical frequency, in Hz; not an exact/calibrated measurement.
    pub typical_hz: u32,
    /// Lowest frequency implied by the published wide-temperature accuracy.
    pub min_hz: u32,
    /// Highest frequency implied by the published wide-temperature accuracy.
    pub max_hz: u32,
}

/// Own-family oscillator envelope from source-qualified chip metadata.
pub const CLOCK: ClockRange = crate::IWDT_CLOCK;

const MAX_RELOAD: u16 = 0x0fff;
const START: u16 = 0xcccc;
const UNLOCK: u16 = 0x5555;
const LOCK: u16 = 0x6666;
const REFRESH: u16 = 0xaaaa;
const USE_LSI: bool = crate::IWDT_USES_LSI;

/// Watchdog configuration or hardware synchronization error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
pub enum Error {
    /// A zero or unrepresentable timeout was requested.
    TimeoutOutOfRange,
    /// The register-poll budget must be nonzero.
    InvalidPollLimit,
    /// Configuration clock enable did not take effect.
    ClockEnableTimeout,
    /// L010/L011 LSI did not become stable; the watchdog has not been started.
    OscillatorTimeout,
    /// Hardware was already running. It has not been stopped or reconfigured.
    AlreadyRunning,
    /// Stopped hardware differs from the documented reset configuration.
    NotReset,
    /// A prior start attempt failed after the start key. Reset before reusing it.
    StartupFailed,
    /// The start command did not produce the running status within the budget.
    StartTimeout,
    /// CR/ARR/WINR synchronization did not complete within the budget.
    UpdateTimeout,
    /// A configuration readback did not match the intended reset-mode settings.
    ConfigurationMismatch,
    /// Counter reload synchronization did not complete within the budget.
    ReloadTimeout,
    /// `pet` was called before successful `unleash`, or hardware stopped running.
    NotRunning,
}

/// Counter configuration and calculated oscillator-only timing estimates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Timing {
    prescaler_encoding: u8,
    reload: u16,
}
impl Timing {
    /// Calculate a counter period no shorter than `timeout_us` at `CLOCK.max_hz`.
    ///
    /// Uses all documented dividers, 4 through 512, and a 12-bit reload value.
    /// Rounded upward before selecting a divider; zero and overflow are errors.
    pub fn for_timeout(timeout_us: u32) -> Result<Self, Error> {
        calculate_timing(timeout_us, CLOCK.max_hz)
    }
    /// Hardware prescaler divisor (4..512).
    pub const fn prescaler(self) -> u16 {
        4u16 << self.prescaler_encoding
    }
    /// Value written to the 12-bit ARR register (counter period uses ARR + 1).
    pub const fn reload_value(self) -> u16 {
        self.reload
    }
    /// Estimated typical period, rounded down to microseconds.
    pub fn nominal_timeout_us(self) -> u64 {
        self.cycles_us() / u64::from(CLOCK.typical_hz)
    }
    /// Counter period at the published fastest clock, rounded down.
    ///
    /// This is not a measured minimum time from a Rust call to reset.
    pub fn fastest_clock_timeout_us(self) -> u64 {
        self.cycles_us() / u64::from(CLOCK.max_hz)
    }
    /// Counter period at the published slowest clock, rounded up.
    pub fn slowest_clock_timeout_us(self) -> u64 {
        self.cycles_us().div_ceil(u64::from(CLOCK.min_hz))
    }
    fn cycles_us(self) -> u64 {
        u64::from(self.prescaler()) * (u64::from(self.reload) + 1) * 1_000_000
    }
}
fn calculate_timing(timeout_us: u32, fastest_hz: u32) -> Result<Timing, Error> {
    if timeout_us == 0 || fastest_hz == 0 {
        return Err(Error::TimeoutOutOfRange);
    }
    for prescaler_encoding in 0..=7 {
        let divisor = 4u64 << prescaler_encoding;
        let ticks = (u64::from(timeout_us) * u64::from(fastest_hz)).div_ceil(divisor * 1_000_000);
        if ticks <= u64::from(MAX_RELOAD) + 1 {
            return Ok(Timing {
                prescaler_encoding,
                reload: (ticks - 1) as u16,
            });
        }
    }
    Err(Error::TimeoutOutOfRange)
}

/// Optional configuration of the bounded hardware polling budget.
#[derive(Clone, Copy, Debug)]
pub struct Config {
    /// Desired counter period in microseconds, calculated using the fastest clock.
    pub timeout_us: u32,
    /// Maximum reads per synchronization wait, not a wall-clock duration.
    ///
    /// No timer is required. Interrupts and debug halts may delay these reads;
    /// the hardware can reset the MCU before software returns an error.
    pub poll_limit: u32,
}
impl Config {
    /// Use a 100,000-read budget for each hardware wait.
    pub const fn new(timeout_us: u32) -> Self {
        Self {
            timeout_us,
            poll_limit: 100_000,
        }
    }
}

/// Exclusive reset-mode independent watchdog driver.
///
/// `new` has no hardware side effects. After `unleash`, pet it early enough for
/// your application's worst-case scheduling latency. Dropping it does nothing
/// to the hardware. No other code may change its clocks or registers meanwhile.
pub struct IndependentWatchdog<'d, T: Instance> {
    _peripheral: Peri<'d, T>,
    timing: Timing,
    poll_limit: u32,
    state: State,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum State {
    Prepared,
    Starting,
    Running,
}

impl<'d, T: Instance> IndependentWatchdog<'d, T> {
    /// Prepare a watchdog with the requested timeout, without starting it.
    ///
    /// Panics for zero/out-of-range timeout; see [`Timing::for_timeout`].
    pub fn new(instance: Peri<'d, T>, timeout_us: u32) -> Self {
        Self::try_new(instance, timeout_us)
            .unwrap_or_else(|e| panic!("IWDT configuration failed: {:?}", e))
    }
    /// Fallible [`Self::new`]. No MMIO is performed, including on failure.
    pub fn try_new(instance: Peri<'d, T>, timeout_us: u32) -> Result<Self, Error> {
        Self::try_new_with_config(instance, Config::new(timeout_us))
    }
    /// Prepare with a custom, nonzero per-wait register-read budget.
    pub fn try_new_with_config(instance: Peri<'d, T>, config: Config) -> Result<Self, Error> {
        if config.poll_limit == 0 {
            return Err(Error::InvalidPollLimit);
        }
        Ok(Self {
            _peripheral: instance,
            timing: Timing::for_timeout(config.timeout_us)?,
            poll_limit: config.poll_limit,
            state: State::Prepared,
        })
    }
    /// Calculated settings and oscillator-only period estimates.
    pub fn timing(&self) -> Timing {
        self.timing
    }
    /// Start, configure and refresh the watchdog. Panics on a hardware error.
    ///
    /// Repeated calls after successful start refresh without reconfiguring it.
    /// See [`Self::try_unleash`] for the hardware-specific startup requirements.
    pub fn unleash(&mut self) {
        self.try_unleash()
            .unwrap_or_else(|e| panic!("IWDT startup failed: {:?}", e));
    }
    /// Fallible start with bounded polls and explicit reset-state checks.
    ///
    /// CW32 starts counting from 0xFFF with divide-by-4 before configuration.
    /// The initial shortest oscillator-only window is 4096*4 / `CLOCK.max_hz`
    /// seconds (about 0.400 s on L011). Startup must finish inside that window;
    /// interrupts, stalled buses and debug freezes are not bounded by this API.
    /// A prescaler increase never shortens that initial window. WINR is kept at
    /// its checked 0xFFF reset value; no early window-triggered reload is issued.
    ///
    /// Failure after START leaves the watchdog potentially running. Configuration
    /// protection is re-locked, no stop/reset/feed is attempted, and future start
    /// or pet calls are rejected. Reset the device to recover. Failures before
    /// START can be retried. Bootloader watchdog takeover is intentionally absent.
    /// L010/L011 enable and retain LSI without changing its existing trim.
    pub fn try_unleash(&mut self) -> Result<(), Error> {
        if self.state == State::Running {
            return self.try_pet();
        }
        if self.state == State::Starting {
            return Err(Error::StartupFailed);
        }
        // Shared SYSCTRL read-modify-write operations and the LSI ready wait are
        // serialized. Owned IWDT operations need no long critical section.
        critical_section::with(|cs| self.prepare_clock(cs))?;
        self.start()
    }
    /// Reload the running watchdog. Panics if it was not started or a wait fails.
    pub fn pet(&mut self) {
        self.try_pet()
            .unwrap_or_else(|e| panic!("IWDT refresh failed: {:?}", e));
    }
    /// Reload and wait for completion, using bounded polls before and after KR.
    pub fn try_pet(&mut self) -> Result<(), Error> {
        if self.state != State::Running {
            return Err(Error::NotRunning);
        }
        self.refresh()
    }
}

trait SealedInstance {
    fn regs() -> pac::iwdt::Iwdt;
}
/// IWDT peripheral instance, sealed to the generated hardware singleton.
#[allow(private_bounds)]
pub trait Instance: SealedInstance + PeripheralType {}
impl SealedInstance for crate::peripherals::IWDT {
    fn regs() -> pac::iwdt::Iwdt {
        pac::IWDT
    }
}
impl Instance for crate::peripherals::IWDT {}

#[cfg(wwdt)]
mod windowed;
#[cfg(wwdt)]
pub use windowed::{
    WindowConfig, WindowError, WindowInstance, WindowPrescaler, WindowStatus, WindowTiming,
    WindowWatchdog,
};

impl<'d, T: Instance> IndependentWatchdog<'d, T> {
    fn poll(&self, ready: impl Fn() -> bool, error: Error) -> Result<(), Error> {
        for _ in 0..self.poll_limit {
            if ready() {
                return Ok(());
            }
            core::hint::spin_loop();
        }
        Err(error)
    }

    fn prepare_clock(&self, cs: critical_section::CriticalSection<'_>) -> Result<(), Error> {
        use crate::rcc::{Readback, SealedRccPeripheral};
        crate::peripherals::IWDT::RCC_INFO
            .enable_with_cs_readback(
                cs,
                Readback::Poll {
                    attempts: self.poll_limit,
                    spin: true,
                },
            )
            .map_err(|_| Error::ClockEnableTimeout)?;
        // L010/L011's oscillator enable is separate from the peripheral bus gate.
        if USE_LSI {
            pac::SYSCTRL.cr1().modify(|v| {
                v.set_key(0x5a5a);
                v.set_lsien(true);
            });
            self.poll(
                || pac::SYSCTRL.lsi().read().stable(),
                Error::OscillatorTimeout,
            )?;
        }
        Ok(())
    }

    fn start(&mut self) -> Result<(), Error> {
        let regs = T::regs();
        if regs.sr().read().run() {
            return Err(Error::AlreadyRunning);
        }
        self.poll(
            || {
                let sr = regs.sr().read();
                !sr.crf() && !sr.arrf() && !sr.winrf() && !sr.reload()
            },
            Error::UpdateTimeout,
        )?;
        let mut reset_arr = pac::iwdt::regs::Arr(0);
        reset_arr.set_arr(MAX_RELOAD);
        let mut reset_winr = pac::iwdt::regs::Winr(0);
        reset_winr.set_winr(MAX_RELOAD);
        // Compare whole registers, including reserved bits, just as at takeover.
        if regs.cr().read() != pac::iwdt::regs::Cr(0)
            || regs.arr().read() != reset_arr
            || regs.winr().read() != reset_winr
        {
            return Err(Error::NotReset);
        }
        // Mark the attempt before issuing the start key. Even a timeout does not
        // establish that the hardware has stopped; no subsequent implicit retry.
        self.state = State::Starting;
        regs.kr().write(|v| v.set_kr(START));
        let result = (|| {
            self.poll(|| regs.sr().read().run(), Error::StartTimeout)?;
            self.poll(
                || {
                    let sr = regs.sr().read();
                    !sr.crf() && !sr.arrf() && !sr.winrf() && !sr.reload()
                },
                Error::UpdateTimeout,
            )?;
            regs.kr().write(|v| v.set_kr(UNLOCK));
            // Zero seeds deliberately retain reset mode, no IRQ and run in sleep.
            let mut cr = pac::iwdt::regs::Cr(0);
            cr.set_prs(self.timing.prescaler_encoding);
            let mut arr = pac::iwdt::regs::Arr(0);
            arr.set_arr(self.timing.reload);
            regs.cr().write_value(cr);
            regs.arr().write_value(arr);
            self.poll(
                || {
                    let sr = regs.sr().read();
                    !sr.crf() && !sr.arrf() && !sr.winrf()
                },
                Error::UpdateTimeout,
            )?;
            if regs.cr().read() != cr
                || regs.arr().read() != arr
                || regs.winr().read() != reset_winr
            {
                return Err(Error::ConfigurationMismatch);
            }
            Ok(())
        })();
        regs.kr().write(|v| v.set_kr(LOCK));
        result?;
        self.refresh()?;
        self.state = State::Running;
        Ok(())
    }

    fn refresh(&self) -> Result<(), Error> {
        let regs = T::regs();
        if !regs.sr().read().run() {
            return Err(Error::NotRunning);
        }
        self.poll(
            || {
                let sr = regs.sr().read();
                !sr.crf() && !sr.arrf() && !sr.winrf() && !sr.reload()
            },
            Error::ReloadTimeout,
        )?;
        regs.kr().write(|v| v.set_kr(REFRESH));
        self.poll(|| !regs.sr().read().reload(), Error::ReloadTimeout)
    }
}
