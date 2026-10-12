//! Owned blocking whole-second calendars for all eleven RTC-bearing families.
//!
//! L010/L011/L012 select frozen HSIOSC or an owned, qualified LSE. Exactly
//! qualified packages also admit a live factory-LSI source independent of SYSCLK,
//! with fixed /32800 division and rate-only bounds. Acquisition never starts LSI.
//! Prescalers and tick bounds follow the actual held source. The eight
//! classic families support a verified factory-trim LSI capability; its
//! nominal rate is 32800/32768 calendar ticks per SI second, with the own RC
//! tolerance retained in ClockBounds. This is not a precision wall clock.
//! Exact source-qualified packages also accept a held board-qualified LSE source.
//! Bounds describe a healthy source, with no automatic LSI fallback after failure.
//! Native StartupOnly readiness cannot detect later source loss: STABLE may latch
//! indefinitely with CCS clear. A successful read does not prove elapsed time.
//! Attach and reads preserve retained calendar/event state. Drop never stops,
//! resets or gates the RTC or its oscillator. Cold initialization is explicit.
//! On L011/L012, activating or changing the RTC source also affects RTC_OUT,
//! RTC_1Hz and their downstream recipients. Supported functional handover
//! requires these recipients to be inactive/disconnected or within the RTC
//! owner's explicitly requested scope. These roots are not all runtime-checked;
//! owning the RTC and oscillator pads alone does not own other timer or output
//! recipients. This functional limit adds no safe-Rust memory-safety obligation.
//! Weekly Alarm A and A/B event flags are supported; run-mode async waits are
//! limited to HSIOSC/LSE on L010/L011/L012. Native LSI refuses async waiting and
//! L010 running calendar writes; stopped-unset initialization remains supported.
//! No compensation, subseconds, low-power or battery guarantee.
//! Sources and unsupported boundaries: docs/rtc-remaining-evidence.json.
mod alarm;
#[cfg(rtc_alarm_direct_access)]
mod alarm_interrupt;
mod datetime;
use crate::{
    Peri, pac,
    peripherals::RTC,
    rcc::{CalendarClock, ClockBounds},
};
pub use alarm::{Alarm, AlarmAConfig, AlarmDays, AlarmStatus};
#[cfg(rtc_alarm_direct_access)]
pub use alarm_interrupt::AlarmInterruptHandler;
pub use datetime::{DateTime, DayOfWeek, Error as DateTimeError};
#[cfg(any(rtc_cw32l010_v1, rtc_cw32l011_v1, rtc_cw32l012_v1))]
pub(crate) type Source = u8;
#[cfg(not(any(rtc_cw32l010_v1, rtc_cw32l011_v1, rtc_cw32l012_v1)))]
pub(crate) type Source = pac::rtc::vals::Source;

pub(crate) mod sealed {
    pub trait Instance: crate::rcc::RccPeripheral {
        const SOURCE: u8;
        const SOURCE_IS_LSI: bool;
        const SOURCE_NOMINAL_HZ: u32;
        const SOURCE_MINIMUM_HZ: u32;
        const SOURCE_MAXIMUM_HZ: u32;
        const TEMPERATURE_C: (i16, i16);
        const SUPPLY_MV: (u16, u16);
        #[cfg(not(any(rtc_cw32l010_v1, rtc_cw32l011_v1, rtc_cw32l012_v1)))]
        const FACTORY_TRIM_ADDRESS: usize;
        const CALENDAR_DIVISOR: u32;
        #[cfg(any(rtc_cw32l010_v1, rtc_cw32l011_v1, rtc_cw32l012_v1))]
        const PRESCALER_FIRST: u16;
        #[cfg(any(rtc_cw32l010_v1, rtc_cw32l011_v1, rtc_cw32l012_v1))]
        const PRESCALER_SECOND: u32;
    }
}
/// A failed deliberate write can be partial; no rollback is promised.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum RtcError {
    InvalidDateTime(DateTimeError),
    InvalidTimeout,
    /// An alarm hour, minute or second is outside its binary range.
    InvalidAlarm,
    /// Disable the selected comparator before replacing its match.
    AlarmAlreadyEnabled,
    /// Enable the selected comparator before awaiting a match.
    AlarmDisabled,
    /// A retained peripheral interrupt is already enabled; it is left untouched.
    InterruptInUse,
    ClockGateTimeout,
    /// The new native-LSI RTC domain was observed gated or held in reset.
    /// Hardware may remain unlocked or partially configured; no repair occurs.
    ClockDomainUnavailable,
    /// The held source lost readiness, including native LSI factory-trim compatibility.
    ClockNotReady,
    IncompatibleClock,
    /// L010 RTCLPM prevents synchronized writes. It is never changed here.
    LowPowerSynchronization,
    NotRunning,
    AlreadyRunning,
    AlreadyConfigured,
    /// Another ACCESS transaction was already active; it is left untouched.
    AccessInUse,
    /// L010 native-LSI calendar writes lack a qualified one-second ACCESS bound.
    UnqualifiedWriteTiming,
    /// Native live-LSI supports synchronous alarm operations only.
    UnsupportedAsyncAlarmSource,
    SynchronizationTimeout,
    ReadFailure,
    WriteFailure,
}
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RtcConfig {
    /// Register polls per synchronization step, not a time duration.
    pub timeout: u32,
    /// Complete snapshot attempts; each masks ordinary interrupts briefly.
    pub read_retries: u32,
}
impl Default for RtcConfig {
    fn default() -> Self {
        Self {
            timeout: 100_000,
            read_retries: 8,
        }
    }
}
impl RtcConfig {
    fn validate(self) -> Result<Self, RtcError> {
        if self.timeout == 0 || self.read_retries == 0 {
            Err(RtcError::InvalidTimeout)
        } else {
            Ok(self)
        }
    }
}
/// Unique RTC plus a retained source capability; no cloneable provider.
pub struct Rtc<'d> {
    _rtc: Peri<'d, RTC>,
    clock: CalendarClock<'d>,
    config: RtcConfig,
}
impl<'d> Rtc<'d> {
    /// Enable only the APB configuration gate, then validate the retained source,
    /// format, prescalers and calendar. No reset, unlock, stop, trim, compensation,
    /// event or global reset-flag write is performed. Both hour formats work.
    pub fn attach_preserving_state(
        rtc: Peri<'d, RTC>,
        clock: impl Into<CalendarClock<'d>>,
        config: RtcConfig,
    ) -> Result<Self, RtcError> {
        let clock = clock.into();
        let config = config.validate()?;
        enable_gate(config)?;
        #[cfg(rtc_native_live_lsi)]
        check_native_lsi_rtc_domain(matches!(&clock, CalendarClock::Lsi(_)))?;
        check_clock(&clock)?;
        read_calendar(&clock, config)?;
        check_clock(&clock)?;
        Ok(Self {
            _rtc: rtc,
            clock,
            config,
        })
    }
    /// Explicitly initialize a stopped, DATE=0, inactive calendar only.
    ///
    /// Existing compensation/event/interrupt configuration causes an error.
    /// L011/L012 also reject pending event flags and drain WAIT before reading
    /// DATE; retained flags are never cleared to manufacture an unset calendar.
    /// Native LSI on L010 also drains WAIT before DATE, and verifies SOURCE=2
    /// before lowering its first prescaler. Native LSI must already be live at
    /// factory trim; initialization never starts or calibrates that oscillator.
    /// Inactive alarm matches and AWT reload are preserved. Source and format
    /// are configured without peripheral reset or clearing flags. Failure can
    /// leave partial configuration; use borrowed tokens if retry is required.
    /// Classic LSI must already carry factory trim before obtaining its clock
    /// capability; this constructor does not calibrate shared oscillators.
    ///
    /// L011/L012 source/prescaler writes can change RTC_OUT and RTC_1Hz before
    /// START is set. Functional handover must cover external output recipients,
    /// BTIM trigger/reset roots, GTIM/ATIM inputs, LPTIM RTC event roots and their
    /// downstream cascades throughout this operation and later RTC use. L011
    /// RTC_OUT uses PA01/PA03 AF3; L012 additionally uses PB14/PB15/PC13 AF4.
    /// L012's disputed BTIM/ATIM selector mappings must be covered across all
    /// documented alternatives. START=0, reserved RTC1HZ=0, stopped counters or
    /// closed working gates do not establish that these recipients are absent.
    /// This API does not inspect or disconnect them. Independent ownership must
    /// already be absent, or those effects must belong to the requested RTC
    /// operation. This is an unverified functional condition, not an additional
    /// memory-safety precondition for safe Rust callers.
    ///
    /// L011/L012 stage the first prescaler without increasing the incoming
    /// RTCCLKD, verify the source change, then install the requested divisors.
    /// Incoming clocking must already be legal; initialization cannot repair an
    /// earlier overclock retroactively. Failure can leave a staged divider or
    /// the requested source selected, without an elapsed-time continuity claim.
    pub fn initialize_if_unset(
        rtc: Peri<'d, RTC>,
        clock: impl Into<CalendarClock<'d>>,
        config: RtcConfig,
        datetime: DateTime,
    ) -> Result<Self, RtcError> {
        let clock = clock.into();
        let config = config.validate()?;
        enable_gate(config)?;
        #[cfg(rtc_native_live_lsi)]
        if matches!(&clock, CalendarClock::Lsi(_)) {
            check_native_lsi_rtc_domain(true)?;
            let ready = clock.is_ready();
            check_native_lsi_rtc_domain(true)?;
            if !ready {
                return Err(RtcError::ClockNotReady);
            }
            check_write_mode()?;
            #[cfg(any(rtc_cw32l011_v1, rtc_cw32l012_v1))]
            {
                check_native_unset_controls(true)?;
                wait_load(config, true)?;
                check_native_lsi_rtc_domain(true)?;
                let ready = clock.is_ready();
                check_native_lsi_rtc_domain(true)?;
                if !ready {
                    return Err(RtcError::ClockNotReady);
                }
                check_native_unset_controls(true)?;
            }
            #[cfg(rtc_cw32l010_v1)]
            {
                check_native_lsi_rtc_domain(true)?;
                let running = pac::RTC.cr0().read().start();
                check_native_lsi_rtc_domain(true)?;
                if running {
                    return Err(RtcError::AlreadyRunning);
                }
                check_native_lsi_rtc_domain(true)?;
                let (mut controls, cr2, compensation, ier) = (
                    pac::RTC.cr0().read(),
                    pac::RTC.cr2().read(),
                    compensation(),
                    pac::RTC.ier().read(),
                );
                check_native_lsi_rtc_domain(true)?;
                controls.set_h24(false);
                if controls.0 != 0 || cr2.0 != 0 || compensation != 0 || ier.0 != 0 {
                    return Err(RtcError::AlreadyConfigured);
                }
                check_access_free(true)?;
                wait_load(config, true)?;
                check_native_lsi_rtc_domain(true)?;
                let ready = clock.is_ready();
                check_native_lsi_rtc_domain(true)?;
                if !ready {
                    return Err(RtcError::ClockNotReady);
                }
            }
            // All three own WAIT protocols exclude DATE during an active load,
            // including L010 while stopped and therefore outside ACCESS.
            check_native_lsi_rtc_domain(true)?;
            let date = pac::RTC.date().read();
            check_native_lsi_rtc_domain(true)?;
            if date.0 != 0 {
                return Err(RtcError::AlreadyConfigured);
            }
            let mut guard = Unlocked::new_native_lsi()?;
            let result = (|| {
                initialize_native_source(&clock, config)?;
                check_native_lsi_rtc_domain(true)?;
                pac::RTC.cr0().write(|w| w.set_h24(true));
                check_native_lsi_rtc_domain(true)?;
                check_clock(&clock)?;
                check_native_lsi_rtc_domain(true)?;
                let h24 = pac::RTC.cr0().read().h24();
                check_native_lsi_rtc_domain(true)?;
                if !h24 {
                    return Err(RtcError::WriteFailure);
                }
                write_stopped(&clock, config, datetime, true)?;
                check_native_lsi_rtc_domain(true)?;
                let mut control = pac::RTC.cr0().read();
                check_native_lsi_rtc_domain(true)?;
                control.set_start(true);
                check_native_lsi_rtc_domain(true)?;
                pac::RTC.cr0().write_value(control);
                check_native_lsi_rtc_domain(true)?;
                let mut domain_error = None;
                let result = poll(
                    config.timeout,
                    || {
                        if let Err(error) = check_native_lsi_rtc_domain(true) {
                            domain_error = Some(error);
                            return true;
                        }
                        let running = pac::RTC.cr0().read().start();
                        if let Err(error) = check_native_lsi_rtc_domain(true) {
                            domain_error = Some(error);
                            return true;
                        }
                        running
                    },
                    RtcError::WriteFailure,
                );
                let final_domain = check_native_lsi_rtc_domain(true);
                if let Some(error) = domain_error {
                    return Err(error);
                }
                final_domain?;
                result
            })();
            if matches!(result, Err(RtcError::ClockDomainUnavailable)) {
                guard.suppress_relock = true;
                return Err(RtcError::ClockDomainUnavailable);
            }
            guard.finish_native_lsi()?;
            result?;
            check_clock(&clock)?;
            return Ok(Self {
                _rtc: rtc,
                clock,
                config,
            });
        }
        if !clock.is_ready() {
            return Err(RtcError::ClockNotReady);
        }
        check_write_mode()?;
        #[cfg(any(rtc_cw32l011_v1, rtc_cw32l012_v1))]
        {
            // Own WAIT protocol excludes DATE/TIME/AWTARR access during load.
            // Read controls first, then inspect DATE only after synchronization.
            check_native_unset_controls(false)?;
            wait_load(config, false)?;
            if !clock.is_ready() {
                return Err(RtcError::ClockNotReady);
            }
            check_native_unset_controls(false)?;
            if pac::RTC.date().read().0 != 0 {
                return Err(RtcError::AlreadyConfigured);
            }
        }
        #[cfg(not(any(rtc_cw32l011_v1, rtc_cw32l012_v1)))]
        {
            if pac::RTC.cr0().read().start() {
                return Err(RtcError::AlreadyRunning);
            }
            let mut controls = pac::RTC.cr0().read();
            controls.set_h24(false);
            if pac::RTC.date().read().0 != 0
                || controls.0 != 0
                || pac::RTC.cr2().read().0 != 0
                || compensation() != 0
                || pac::RTC.ier().read().0 != 0
            {
                return Err(RtcError::AlreadyConfigured);
            }
            check_access_free(false)?;
            wait_load(config, false)?;
            if !clock.is_ready() {
                return Err(RtcError::ClockNotReady);
            }
        }
        let guard = Unlocked::new();
        #[cfg(any(rtc_cw32l011_v1, rtc_cw32l012_v1))]
        initialize_native_source(&clock, config)?;
        #[cfg(not(any(rtc_cw32l011_v1, rtc_cw32l012_v1)))]
        {
            // Fresh typed write excludes WINDOW/WAIT status bits.
            pac::RTC.cr1().write(|w| w.set_source(clock.source()));
            #[cfg(rtc_cw32l010_v1)]
            pac::RTC.psc().write(|w| {
                let (first, second) = clock.prescalers();
                w.set_psc1((first - 1) as u8);
                w.set_psc2(second - 1);
            });
        }
        pac::RTC.cr0().write(|w| w.set_h24(true));
        check_clock(&clock)?;
        if !pac::RTC.cr0().read().h24() {
            return Err(RtcError::WriteFailure);
        }
        write_stopped(&clock, config, datetime, true)?;
        pac::RTC.cr0().modify(|w| w.set_start(true));
        poll(
            config.timeout,
            || pac::RTC.cr0().read().start(),
            RtcError::WriteFailure,
        )?;
        drop(guard);
        check_clock(&clock)?;
        Ok(Self {
            _rtc: rtc,
            clock,
            config,
        })
    }
    /// Bounded repeated complete pairs, using the own-manual fast-read exception.
    /// No full-calendar hardware latch is documented; rollover needs board QA.
    /// Invalid BCD, impossible Gregorian dates and weekday mismatch are errors.
    pub fn now(&self) -> Result<DateTime, RtcError> {
        check_clock(&self.clock)?;
        let value = read_calendar(&self.clock, self.config)?;
        check_clock(&self.clock)?;
        Ok(value)
    }
    /// Declared healthy-source envelope before the calendar divider.
    /// This getter does not guarantee continuity after a source fault.
    /// F020/F030/A030 and exactly CW32L031C8T6/C8U6/F8U6, CW32R031C8U6,
    /// CW32W031R8U6, CW32L052C8T6/R8S6/R8T6 or CW32L083RBT6/RCT6/RCS6/MCT6/VCT6
    /// factory-LSI bounds qualify rate only under every SYSCLK.
    /// The exact native live-LSI packages have the same rate-only limitation.
    pub const fn source_clock_bounds(&self) -> ClockBounds {
        self.clock.bounds()
    }
    /// Healthy-source envelope of calendar second transitions. Whole-Hz getters round;
    /// the exact 32800/32768 factory-LSI fraction is retained internally.
    /// F020/F030/A030 and exactly CW32L031C8T6/C8U6/F8U6, CW32R031C8U6,
    /// CW32W031R8U6, CW32L052C8T6/R8S6/R8T6 or CW32L083RBT6/RCT6/RCS6/MCT6/VCT6
    /// factory-LSI bounds remain rate-only after division under every SYSCLK;
    /// check `has_cycle_timing_bounds()` before using strict duration methods.
    /// Native live-LSI uses /32800 for nominal 1 Hz, retaining its own fractional
    /// rate envelope. Whole-Hz endpoints round outward to 0 and 2; these are not
    /// individual-period bounds and strict duration methods remain unavailable.
    pub const fn calendar_tick_bounds(&self) -> ClockBounds {
        self.clock
            .bounds()
            .divided_by(self.clock.calendar_divisor())
    }
    /// Deliberate time jump preserving current 12/24-hour format and events.
    ///
    /// Classic and L010 use their own ACCESS transaction without stopping time.
    /// DATE/TIME are written and read back in a fixed-size critical section.
    /// L011/L012 explicitly stop START, drain WAIT and verify both writes before
    /// restarting; an error can leave them stopped. TIME clears subseconds.
    /// L010 native live-LSI returns `UnqualifiedWriteTiming` before unlock or
    /// ACCESS; its running transaction lacks a reviewed execution-duration proof.
    /// Native LSI checks its gate/reset domain around direct operations and
    /// explicit relocking. Observed loss returns `ClockDomainUnavailable` and
    /// can leave hardware unlocked or partially written. Other sources retain
    /// their existing ACCESS and write-protection cleanup. No failure is rolled
    /// back, and event timing can change. Debugger/NMI stalls are outside
    /// the documented one-second transaction bound; do not single-step a write.
    pub fn set_datetime(&mut self, datetime: DateTime) -> Result<(), RtcError> {
        check_clock(&self.clock)?;
        check_write_mode()?;
        #[cfg(rtc_native_live_lsi)]
        let native_lsi = matches!(&self.clock, CalendarClock::Lsi(_));
        #[cfg(not(rtc_native_live_lsi))]
        let native_lsi = false;
        check_access_free(native_lsi)?;
        #[cfg(all(rtc_cw32l010_v1, rtc_native_live_lsi))]
        if native_lsi {
            return Err(RtcError::UnqualifiedWriteTiming);
        }
        #[cfg(all(any(rtc_cw32l011_v1, rtc_cw32l012_v1), rtc_native_live_lsi))]
        if native_lsi {
            wait_load(self.config, true)?;
            check_clock(&self.clock)?;
            let mut guard = Unlocked::new_native_lsi()?;
            let result = (|| {
                check_native_lsi_rtc_domain(true)?;
                let old = pac::RTC.cr0().read();
                check_native_lsi_rtc_domain(true)?;
                check_native_lsi_rtc_domain(true)?;
                let mut control = pac::RTC.cr0().read();
                check_native_lsi_rtc_domain(true)?;
                control.set_start(false);
                check_native_lsi_rtc_domain(true)?;
                pac::RTC.cr0().write_value(control);
                check_native_lsi_rtc_domain(true)?;
                let mut domain_error = None;
                let result = poll(
                    self.config.timeout,
                    || {
                        if let Err(error) = check_native_lsi_rtc_domain(true) {
                            domain_error = Some(error);
                            return true;
                        }
                        let running = pac::RTC.cr0().read().start();
                        if let Err(error) = check_native_lsi_rtc_domain(true) {
                            domain_error = Some(error);
                            return true;
                        }
                        !running
                    },
                    RtcError::WriteFailure,
                );
                let final_domain = check_native_lsi_rtc_domain(true);
                if let Some(error) = domain_error {
                    return Err(error);
                }
                final_domain?;
                result?;
                wait_load(self.config, true)?;
                check_clock(&self.clock)?;
                write_stopped(&self.clock, self.config, datetime, old.h24())?;
                check_native_lsi_rtc_domain(true)?;
                let mut control = pac::RTC.cr0().read();
                check_native_lsi_rtc_domain(true)?;
                control.set_start(true);
                check_native_lsi_rtc_domain(true)?;
                pac::RTC.cr0().write_value(control);
                check_native_lsi_rtc_domain(true)?;
                let mut domain_error = None;
                let result = poll(
                    self.config.timeout,
                    || {
                        if let Err(error) = check_native_lsi_rtc_domain(true) {
                            domain_error = Some(error);
                            return true;
                        }
                        let running = pac::RTC.cr0().read().start();
                        if let Err(error) = check_native_lsi_rtc_domain(true) {
                            domain_error = Some(error);
                            return true;
                        }
                        running
                    },
                    RtcError::WriteFailure,
                );
                let final_domain = check_native_lsi_rtc_domain(true);
                if let Some(error) = domain_error {
                    return Err(error);
                }
                final_domain?;
                result?;
                check_clock(&self.clock)
            })();
            if matches!(result, Err(RtcError::ClockDomainUnavailable)) {
                guard.suppress_relock = true;
                return Err(RtcError::ClockDomainUnavailable);
            }
            guard.finish_native_lsi()?;
            return result;
        }
        wait_load(self.config, false)?;
        check_clock(&self.clock)?;
        #[cfg(any(rtc_cw32l011_v1, rtc_cw32l012_v1))]
        {
            let _guard = Unlocked::new();
            let old = pac::RTC.cr0().read();
            pac::RTC.cr0().modify(|w| w.set_start(false));
            poll(
                self.config.timeout,
                || !pac::RTC.cr0().read().start(),
                RtcError::WriteFailure,
            )?;
            wait_load(self.config, false)?;
            check_clock(&self.clock)?;
            write_stopped(&self.clock, self.config, datetime, old.h24())?;
            pac::RTC.cr0().modify(|w| w.set_start(true));
            poll(
                self.config.timeout,
                || pac::RTC.cr0().read().start(),
                RtcError::WriteFailure,
            )?;
            check_clock(&self.clock)
        }
        #[cfg(not(any(rtc_cw32l011_v1, rtc_cw32l012_v1)))]
        {
            if !pac::RTC.cr0().read().start() {
                return Err(RtcError::NotRunning);
            }
            #[cfg(not(rtc_cw32l010_v1))]
            {
                wait_classic_window(self.config)?;
                check_clock(&self.clock)?;
            }
            #[cfg(rtc_cw32l010_v1)]
            critical_section::with(|_| {
                check_clock(&self.clock)?;
                let _guard = Unlocked::new();
                let _access = Access::new(self.clock.source());
                // Limit ACCESS-held work independently of a large caller budget.
                // No normal interrupt or user callback can extend this window.
                // A slow/missing domain can be retried after the guard releases.
                poll(
                    self.config.timeout.min(32),
                    || pac::RTC.cr1().read().window(),
                    RtcError::SynchronizationTimeout,
                )?;
                check_clock_source(&self.clock)?;
                write_pair(&self.clock, datetime, pac::RTC.cr0().read().h24())
            })?;
            #[cfg(not(rtc_cw32l010_v1))]
            critical_section::with(|_| {
                check_clock(&self.clock)?;
                // The pre-wait had interrupts enabled. Recheck before unlocking.
                if !pac::RTC.cr1().read().window() {
                    return Err(RtcError::SynchronizationTimeout);
                }
                let _guard = Unlocked::new();
                let _access = Access::new(self.clock.source());
                check_clock_source(&self.clock)?;
                write_pair(&self.clock, datetime, pac::RTC.cr0().read().h24())
            })?;
            wait_load(self.config, false)?;
            check_clock(&self.clock)
        }
    }
}
fn poll(budget: u32, mut ready: impl FnMut() -> bool, error: RtcError) -> Result<(), RtcError> {
    for _ in 0..budget {
        if ready() {
            return Ok(());
        }
        core::hint::spin_loop();
    }
    Err(error)
}
#[cfg(not(any(rtc_cw32l010_v1, rtc_cw32l011_v1, rtc_cw32l012_v1)))]
fn wait_classic_window(config: RtcConfig) -> Result<(), RtcError> {
    let cycles = crate::rcc::clocks().hclk_bounds().delay_cycles_us(10_000) as u32;
    for _ in 0..config.timeout.min(1000) {
        if pac::RTC.cr1().read().window() {
            return Ok(());
        }
        cortex_m::asm::delay(cycles);
    }
    Err(RtcError::SynchronizationTimeout)
}
fn enable_gate(config: RtcConfig) -> Result<(), RtcError> {
    use crate::rcc::{Readback, SealedRccPeripheral};
    let info = RTC::RCC_INFO;
    critical_section::with(|cs| {
        info.enable_with_cs_readback(cs, Readback::None)
            .expect("unpolled RTC gate cannot fail")
    });
    poll(
        config.timeout,
        || info.is_enabled(),
        RtcError::ClockGateTimeout,
    )
}
fn check_native_lsi_rtc_domain(native_lsi: bool) -> Result<(), RtcError> {
    if native_lsi {
        use crate::rcc::SealedRccPeripheral;
        let info = RTC::RCC_INFO;
        let enabled = info.is_enabled();
        let reset_asserted = info.reset_asserted();
        if !enabled || reset_asserted {
            return Err(RtcError::ClockDomainUnavailable);
        }
    }
    Ok(())
}
fn check_write_mode() -> Result<(), RtcError> {
    #[cfg(rtc_cw32l010_v1)]
    if pac::SYSCTRL.cr2().read().rtclpm() {
        return Err(RtcError::LowPowerSynchronization);
    }
    Ok(())
}
fn check_access_free(native_lsi: bool) -> Result<(), RtcError> {
    #[cfg(rtc_native_live_lsi)]
    if native_lsi {
        #[cfg(rtc_cw32l010_v1)]
        {
            check_native_lsi_rtc_domain(true)?;
            let access = pac::RTC.cr1().read().access();
            check_native_lsi_rtc_domain(true)?;
            if access {
                return Err(RtcError::AccessInUse);
            }
        }
        return Ok(());
    }
    let _ = native_lsi;
    #[cfg(not(any(rtc_cw32l011_v1, rtc_cw32l012_v1)))]
    if pac::RTC.cr1().read().access() {
        return Err(RtcError::AccessInUse);
    }
    Ok(())
}
fn load_ready() -> bool {
    #[cfg(any(rtc_cw32l010_v1, rtc_cw32l011_v1, rtc_cw32l012_v1))]
    {
        !pac::RTC.cr1().read().wait()
    }
    #[cfg(not(any(rtc_cw32l010_v1, rtc_cw32l011_v1, rtc_cw32l012_v1)))]
    {
        true
    }
}
fn wait_load(config: RtcConfig, native_lsi: bool) -> Result<(), RtcError> {
    #[cfg(rtc_native_live_lsi)]
    if native_lsi {
        let mut domain_error = None;
        let result = poll(
            config.timeout,
            || {
                if let Err(error) = check_native_lsi_rtc_domain(true) {
                    domain_error = Some(error);
                    return true;
                }
                let ready = !pac::RTC.cr1().read().wait();
                if let Err(error) = check_native_lsi_rtc_domain(true) {
                    domain_error = Some(error);
                    return true;
                }
                ready
            },
            RtcError::SynchronizationTimeout,
        );
        let final_domain = check_native_lsi_rtc_domain(true);
        if let Some(error) = domain_error {
            return Err(error);
        }
        final_domain?;
        return result;
    }
    let _ = native_lsi;
    poll(config.timeout, load_ready, RtcError::SynchronizationTimeout)
}
fn compensation() -> u32 {
    #[cfg(any(rtc_cw32l010_v1, rtc_cw32l011_v1, rtc_cw32l012_v1))]
    {
        pac::RTC.compcfr1().read().0
    }
    #[cfg(not(any(rtc_cw32l010_v1, rtc_cw32l011_v1, rtc_cw32l012_v1)))]
    {
        pac::RTC.compen().read().0
    }
}
#[cfg(any(rtc_cw32l011_v1, rtc_cw32l012_v1))]
fn check_native_unset_controls(native_lsi: bool) -> Result<(), RtcError> {
    #[cfg(rtc_native_live_lsi)]
    if native_lsi {
        check_native_lsi_rtc_domain(true)?;
        let (mut controls, cr2, compensation, ier, isr) = (
            pac::RTC.cr0().read(),
            pac::RTC.cr2().read(),
            compensation(),
            pac::RTC.ier().read(),
            pac::RTC.isr().read(),
        );
        check_native_lsi_rtc_domain(true)?;
        if controls.start() {
            return Err(RtcError::AlreadyRunning);
        }
        controls.set_h24(false);
        if controls.0 != 0 || cr2.0 != 0 || compensation != 0 || ier.0 != 0 || isr.0 != 0 {
            return Err(RtcError::AlreadyConfigured);
        }
        return Ok(());
    }
    let _ = native_lsi;
    let mut controls = pac::RTC.cr0().read();
    if controls.start() {
        return Err(RtcError::AlreadyRunning);
    }
    controls.set_h24(false);
    if controls.0 != 0
        || pac::RTC.cr2().read().0 != 0
        || compensation() != 0
        || pac::RTC.ier().read().0 != 0
        || pac::RTC.isr().read().0 != 0
    {
        return Err(RtcError::AlreadyConfigured);
    }
    Ok(())
}
/// Called only while the RTC is unlocked, stopped and explicitly owned for
/// initialization. L011/L012 have no ACCESS field. L010's native-LSI call is
/// stopped, so its own direct-access exception needs no ACCESS transaction.
#[cfg(any(
    rtc_cw32l011_v1,
    rtc_cw32l012_v1,
    all(rtc_cw32l010_v1, rtc_native_live_lsi)
))]
fn initialize_native_source(clock: &CalendarClock<'_>, config: RtcConfig) -> Result<(), RtcError> {
    let (first, second) = clock.prescalers();
    if clock
        .bounds()
        .divided_by(u32::from(first))
        .maximum_exceeds(1_000_000)
    {
        return Err(RtcError::IncompatibleClock);
    }
    #[cfg(rtc_native_live_lsi)]
    if matches!(clock, CalendarClock::Lsi(_)) {
        check_native_lsi_rtc_domain(true)?;
        let mut staged = pac::RTC.psc().read();
        check_native_lsi_rtc_domain(true)?;
        let staged_first = first.max(u16::from(staged.psc1()) + 1);
        staged.set_psc1((staged_first - 1) as u8);
        check_native_lsi_rtc_domain(true)?;
        pac::RTC.psc().write_value(staged);
        check_native_lsi_rtc_domain(true)?;
        let mut domain_error = None;
        let result = poll(
            config.timeout,
            || {
                if let Err(error) = check_native_lsi_rtc_domain(true) {
                    domain_error = Some(error);
                    return true;
                }
                let value = pac::RTC.psc().read();
                if let Err(error) = check_native_lsi_rtc_domain(true) {
                    domain_error = Some(error);
                    return true;
                }
                value.psc1() == staged.psc1()
            },
            RtcError::WriteFailure,
        );
        let final_domain = check_native_lsi_rtc_domain(true);
        if let Some(error) = domain_error {
            return Err(error);
        }
        final_domain?;
        result?;
        check_native_lsi_rtc_domain(true)?;
        let ready = clock.is_ready();
        check_native_lsi_rtc_domain(true)?;
        if !ready {
            return Err(RtcError::ClockNotReady);
        }
        check_native_lsi_rtc_domain(true)?;
        pac::RTC.cr1().write(|w| w.set_source(clock.source()));
        check_native_lsi_rtc_domain(true)?;
        let mut domain_error = None;
        let result = poll(
            config.timeout,
            || {
                if let Err(error) = check_native_lsi_rtc_domain(true) {
                    domain_error = Some(error);
                    return true;
                }
                let source = pac::RTC.cr1().read().source();
                if let Err(error) = check_native_lsi_rtc_domain(true) {
                    domain_error = Some(error);
                    return true;
                }
                source == clock.source()
            },
            RtcError::WriteFailure,
        );
        let final_domain = check_native_lsi_rtc_domain(true);
        if let Some(error) = domain_error {
            return Err(error);
        }
        final_domain?;
        result?;
        check_native_lsi_rtc_domain(true)?;
        let ready = clock.is_ready();
        check_native_lsi_rtc_domain(true)?;
        if !ready {
            return Err(RtcError::ClockNotReady);
        }
        // SOURCE=2 has acknowledged before the final first divisor can fall.
        check_native_lsi_rtc_domain(true)?;
        pac::RTC.psc().write(|w| {
            w.set_psc1((first - 1) as u8);
            w.set_psc2(second - 1);
        });
        check_native_lsi_rtc_domain(true)?;
        let mut domain_error = None;
        let result = poll(
            config.timeout,
            || {
                if let Err(error) = check_native_lsi_rtc_domain(true) {
                    domain_error = Some(error);
                    return true;
                }
                let prescalers = pac::RTC.psc().read();
                if let Err(error) = check_native_lsi_rtc_domain(true) {
                    domain_error = Some(error);
                    return true;
                }
                u16::from(prescalers.psc1()) + 1 == first && prescalers.psc2() + 1 == second
            },
            RtcError::WriteFailure,
        );
        let final_domain = check_native_lsi_rtc_domain(true);
        if let Some(error) = domain_error {
            return Err(error);
        }
        final_domain?;
        result?;
        return check_clock_source(clock);
    }
    let mut staged = pac::RTC.psc().read();
    let staged_first = first.max(u16::from(staged.psc1()) + 1);
    // Increasing division cannot overclock the incoming source. It also bounds
    // the requested source before SOURCE changes, without assuming which source
    // was inherited or reducing its existing division. Preserve PSC2 here.
    staged.set_psc1((staged_first - 1) as u8);
    pac::RTC.psc().write_value(staged);
    poll(
        config.timeout,
        || pac::RTC.psc().read().psc1() == staged.psc1(),
        RtcError::WriteFailure,
    )?;
    if !clock.is_ready() {
        return Err(RtcError::ClockNotReady);
    }
    pac::RTC.cr1().write(|w| w.set_source(clock.source()));
    poll(
        config.timeout,
        || pac::RTC.cr1().read().source() == clock.source(),
        RtcError::WriteFailure,
    )?;
    if !clock.is_ready() {
        return Err(RtcError::ClockNotReady);
    }
    // SOURCE is now verified before reducing division for the LSE path.
    pac::RTC.psc().write(|w| {
        w.set_psc1((first - 1) as u8);
        w.set_psc2(second - 1);
    });
    poll(
        config.timeout,
        || {
            let p = pac::RTC.psc().read();
            u16::from(p.psc1()) + 1 == first && p.psc2() + 1 == second
        },
        RtcError::WriteFailure,
    )?;
    check_clock_source(clock)
}
fn check_clock_source(clock: &CalendarClock<'_>) -> Result<(), RtcError> {
    #[cfg(rtc_native_live_lsi)]
    if matches!(clock, CalendarClock::Lsi(_)) {
        check_native_lsi_rtc_domain(true)?;
        let (ready, source, prescalers, compensation) = (
            clock.is_ready(),
            pac::RTC.cr1().read().source(),
            pac::RTC.psc().read(),
            pac::RTC.compcfr1().read(),
        );
        check_native_lsi_rtc_domain(true)?;
        if !ready {
            return Err(RtcError::ClockNotReady);
        }
        let first = u32::from(prescalers.psc1()) + 1;
        let second = prescalers.psc2() + 1;
        if source != clock.source()
            || (first, second)
                != (
                    u32::from(crate::RCC_NATIVE_LSI_RTC_FIRST_DIVISOR),
                    crate::RCC_NATIVE_LSI_RTC_SECOND_DIVISOR,
                )
            || clock.bounds().divided_by(first).maximum_exceeds(1_000_000)
            || compensation.en()
        {
            return Err(RtcError::IncompatibleClock);
        }
        return Ok(());
    }
    if !clock.is_ready() {
        return Err(RtcError::ClockNotReady);
    }
    if pac::RTC.cr1().read().source() != clock.source() {
        return Err(RtcError::IncompatibleClock);
    }
    #[cfg(any(rtc_cw32l010_v1, rtc_cw32l011_v1, rtc_cw32l012_v1))]
    {
        let p = pac::RTC.psc().read();
        let first = u32::from(p.psc1()) + 1;
        let second = p.psc2() + 1;
        if clock.bounds().divided_by(first).maximum_exceeds(1_000_000)
            || u64::from(first) * u64::from(second) * 2 != u64::from(clock.frequency().0)
        {
            return Err(RtcError::IncompatibleClock);
        }
        // Native LSE qualification fixes PSC1=0, PSC2=0x3fff. Preserve the
        // prior HSIOSC attach rule allowing any safe exact nominal factor pair.
        #[cfg(all(rcc_lse, any(rtc_cw32l010_v1, rtc_cw32l011_v1, rtc_cw32l012_v1)))]
        if matches!(clock, CalendarClock::Lse(_))
            && (first, second)
                != (
                    u32::from(crate::RCC_LSE_RTC_FIRST_DIVISOR),
                    crate::RCC_LSE_RTC_SECOND_DIVISOR,
                )
        {
            return Err(RtcError::IncompatibleClock);
        }
        if pac::RTC.compcfr1().read().en() {
            return Err(RtcError::IncompatibleClock);
        }
    }
    #[cfg(not(any(rtc_cw32l010_v1, rtc_cw32l011_v1, rtc_cw32l012_v1)))]
    if pac::RTC.compen().read().en() {
        return Err(RtcError::IncompatibleClock);
    }
    Ok(())
}
fn check_clock(clock: &CalendarClock<'_>) -> Result<(), RtcError> {
    check_clock_source(clock)?;
    #[cfg(rtc_native_live_lsi)]
    if matches!(clock, CalendarClock::Lsi(_)) {
        return check_access_free(true);
    }
    check_access_free(false)
}
fn read_calendar(clock: &CalendarClock<'_>, config: RtcConfig) -> Result<DateTime, RtcError> {
    #[cfg(rtc_native_live_lsi)]
    if matches!(clock, CalendarClock::Lsi(_)) {
        check_native_lsi_rtc_domain(true)?;
        let running = pac::RTC.cr0().read().start();
        check_native_lsi_rtc_domain(true)?;
        if !running {
            return Err(RtcError::NotRunning);
        }
        wait_load(config, true)?;
        check_clock(clock)?;
        for _ in 0..config.read_retries {
            let snapshot = critical_section::with(|_| -> Result<_, RtcError> {
                check_native_lsi_rtc_domain(true)?;
                let control = pac::RTC.cr0().read();
                let t1 = pac::RTC.time().read();
                let d1 = pac::RTC.date().read();
                let t2 = pac::RTC.time().read();
                let d2 = pac::RTC.date().read();
                let t3 = pac::RTC.time().read();
                let ready = load_ready();
                check_native_lsi_rtc_domain(true)?;
                Ok(
                    (ready && control.start() && t1 == t2 && t2 == t3 && d1 == d2).then_some((
                        d1,
                        t1,
                        control.h24(),
                    )),
                )
            })?;
            if let Some((date, time, h24)) = snapshot {
                let value = decode(date, time, h24)?;
                check_clock(clock)?;
                return Ok(value);
            }
        }
        check_native_lsi_rtc_domain(true)?;
        return Err(RtcError::ReadFailure);
    }
    if !pac::RTC.cr0().read().start() {
        return Err(RtcError::NotRunning);
    }
    wait_load(config, false)?;
    check_clock(clock)?;
    for _ in 0..config.read_retries {
        let snapshot = critical_section::with(|_| {
            let control = pac::RTC.cr0().read();
            let t1 = pac::RTC.time().read();
            let d1 = pac::RTC.date().read();
            let t2 = pac::RTC.time().read();
            let d2 = pac::RTC.date().read();
            let t3 = pac::RTC.time().read();
            (load_ready() && control.start() && t1 == t2 && t2 == t3 && d1 == d2).then_some((
                d1,
                t1,
                control.h24(),
            ))
        });
        if let Some((d, t, h24)) = snapshot {
            let value = decode(d, t, h24)?;
            check_clock(clock)?;
            return Ok(value);
        }
    }
    Err(RtcError::ReadFailure)
}
fn write_stopped(
    clock: &CalendarClock<'_>,
    config: RtcConfig,
    datetime: DateTime,
    h24: bool,
) -> Result<(), RtcError> {
    #[cfg(rtc_native_live_lsi)]
    if matches!(clock, CalendarClock::Lsi(_)) {
        check_clock_source(clock)?;
        let (date, time) = encode(datetime, h24);
        check_native_lsi_rtc_domain(true)?;
        pac::RTC.date().write_value(date);
        check_native_lsi_rtc_domain(true)?;
        wait_load(config, true)?;
        check_clock_source(clock)?;
        check_native_lsi_rtc_domain(true)?;
        pac::RTC.time().write_value(time);
        check_native_lsi_rtc_domain(true)?;
        wait_load(config, true)?;
        check_clock_source(clock)?;
        let mut domain_error = None;
        let result = poll(
            config.timeout,
            || {
                if let Err(error) = check_native_lsi_rtc_domain(true) {
                    domain_error = Some(error);
                    return true;
                }
                let (read_date, read_time) = (pac::RTC.date().read(), pac::RTC.time().read());
                if let Err(error) = check_native_lsi_rtc_domain(true) {
                    domain_error = Some(error);
                    return true;
                }
                read_date == date && read_time == time
            },
            RtcError::WriteFailure,
        );
        let final_domain = check_native_lsi_rtc_domain(true);
        if let Some(error) = domain_error {
            return Err(error);
        }
        final_domain?;
        result?;
        return check_clock_source(clock);
    }
    check_clock_source(clock)?;
    let (date, time) = encode(datetime, h24);
    pac::RTC.date().write_value(date);
    wait_load(config, false)?;
    check_clock_source(clock)?;
    pac::RTC.time().write_value(time);
    wait_load(config, false)?;
    check_clock_source(clock)?;
    poll(
        config.timeout,
        || pac::RTC.date().read() == date && pac::RTC.time().read() == time,
        RtcError::WriteFailure,
    )?;
    check_clock_source(clock)
}
#[cfg(not(any(rtc_cw32l011_v1, rtc_cw32l012_v1)))]
fn write_pair(clock: &CalendarClock<'_>, datetime: DateTime, h24: bool) -> Result<(), RtcError> {
    check_clock_source(clock)?;
    let (date, time) = encode(datetime, h24);
    pac::RTC.date().write_value(date);
    check_clock_source(clock)?;
    pac::RTC.time().write_value(time);
    if pac::RTC.date().read() != date || pac::RTC.time().read() != time {
        Err(RtcError::WriteFailure)
    } else {
        check_clock_source(clock)
    }
}
struct Unlocked {
    native_lsi: bool,
    suppress_relock: bool,
}
impl Unlocked {
    fn new() -> Self {
        pac::RTC.key().write(|w| w.set_key(0xca));
        pac::RTC.key().write(|w| w.set_key(0x53));
        Self {
            native_lsi: false,
            suppress_relock: false,
        }
    }
    #[cfg(rtc_native_live_lsi)]
    fn new_native_lsi() -> Result<Self, RtcError> {
        check_native_lsi_rtc_domain(true)?;
        pac::RTC.key().write(|w| w.set_key(0xca));
        check_native_lsi_rtc_domain(true)?;
        pac::RTC.key().write(|w| w.set_key(0x53));
        check_native_lsi_rtc_domain(true)?;
        Ok(Self {
            native_lsi: true,
            suppress_relock: false,
        })
    }
    #[cfg(rtc_native_live_lsi)]
    fn finish_native_lsi(&mut self) -> Result<(), RtcError> {
        self.suppress_relock = true;
        check_native_lsi_rtc_domain(true)?;
        pac::RTC.key().write(|w| w.set_key(0xca));
        check_native_lsi_rtc_domain(true)?;
        pac::RTC.key().write(|w| w.set_key(0));
        check_native_lsi_rtc_domain(true)
    }
}
impl Drop for Unlocked {
    fn drop(&mut self) {
        if self.suppress_relock {
            return;
        }
        if self.native_lsi && check_native_lsi_rtc_domain(true).is_err() {
            return;
        }
        pac::RTC.key().write(|w| w.set_key(0xca));
        if self.native_lsi && check_native_lsi_rtc_domain(true).is_err() {
            return;
        }
        pac::RTC.key().write(|w| w.set_key(0));
    }
}
#[cfg(not(any(rtc_cw32l011_v1, rtc_cw32l012_v1)))]
struct Access {
    source: Source,
}
#[cfg(not(any(rtc_cw32l011_v1, rtc_cw32l012_v1)))]
impl Access {
    fn new(source: Source) -> Self {
        pac::RTC.cr1().write(|w| {
            w.set_source(source);
            w.set_access(true);
        });
        Self { source }
    }
}
#[cfg(not(any(rtc_cw32l011_v1, rtc_cw32l012_v1)))]
impl Drop for Access {
    fn drop(&mut self) {
        pac::RTC.cr1().write(|w| w.set_source(self.source));
    }
}
fn unbcd(value: u8) -> Result<u8, RtcError> {
    if value & 15 > 9 || value >> 4 > 9 {
        Err(RtcError::InvalidDateTime(DateTimeError::InvalidBcd))
    } else {
        Ok((value >> 4) * 10 + (value & 15))
    }
}
fn bcd(value: u8) -> u8 {
    value / 10 * 16 + value % 10
}
fn decode(
    date: pac::rtc::regs::Date,
    time: pac::rtc::regs::Time,
    h24: bool,
) -> Result<DateTime, RtcError> {
    let raw = time.hour();
    let hour = if h24 {
        unbcd(raw)?
    } else {
        let h = unbcd(raw & 0x1f)?;
        if !(1..=12).contains(&h) {
            return Err(RtcError::InvalidDateTime(DateTimeError::InvalidHour));
        }
        h % 12 + if raw & 0x20 != 0 { 12 } else { 0 }
    };
    DateTime::from(
        2000 + u16::from(unbcd(date.year())?),
        unbcd(date.month())?,
        unbcd(date.day())?,
        DayOfWeek::from_hardware(date.week()).map_err(RtcError::InvalidDateTime)?,
        hour,
        unbcd(time.minute())?,
        unbcd(time.second())?,
        0,
    )
    .map_err(RtcError::InvalidDateTime)
}
fn encode(dt: DateTime, h24: bool) -> (pac::rtc::regs::Date, pac::rtc::regs::Time) {
    let mut date = pac::rtc::regs::Date::default();
    let mut time = pac::rtc::regs::Time::default();
    date.set_day(bcd(dt.day()));
    date.set_month(bcd(dt.month()));
    date.set_year(bcd((dt.year() - 2000) as u8));
    date.set_week(dt.day_of_week().hardware());
    let hour = if h24 {
        bcd(dt.hour())
    } else {
        bcd(if dt.hour() % 12 == 0 {
            12
        } else {
            dt.hour() % 12
        }) | if dt.hour() >= 12 { 0x20 } else { 0 }
    };
    time.set_hour(hour);
    time.set_minute(bcd(dt.minute()));
    time.set_second(bcd(dt.second()));
    (date, time)
}
