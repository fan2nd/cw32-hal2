//! Owned blocking whole-second calendars for all eleven RTC-bearing families.
//!
//! L010/L011/L012 select frozen HSIOSC or an owned, qualified LSE.
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
//! Weekly Alarm A and A/B event flags are supported; run-mode async waits are limited to L010/L011/L012.
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
    ClockNotReady,
    IncompatibleClock,
    /// L010 RTCLPM prevents synchronized writes. It is never changed here.
    LowPowerSynchronization,
    NotRunning,
    AlreadyRunning,
    AlreadyConfigured,
    /// Another ACCESS transaction was already active; it is left untouched.
    AccessInUse,
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
        if !clock.is_ready() {
            return Err(RtcError::ClockNotReady);
        }
        check_write_mode()?;
        #[cfg(any(rtc_cw32l011_v1, rtc_cw32l012_v1))]
        {
            // Own WAIT protocol excludes DATE/TIME/AWTARR access during load.
            // Read controls first, then inspect DATE only after synchronization.
            check_native_unset_controls()?;
            wait_load(config)?;
            if !clock.is_ready() {
                return Err(RtcError::ClockNotReady);
            }
            check_native_unset_controls()?;
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
            check_access_free()?;
            wait_load(config)?;
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
    /// F020/F030/A030 and exactly CW32L031C8T6/C8U6/F8U6 or CW32R031C8U6
    /// factory-LSI bounds qualify rate only under every SYSCLK.
    pub const fn source_clock_bounds(&self) -> ClockBounds {
        self.clock.bounds()
    }
    /// Healthy-source envelope of calendar second transitions. Whole-Hz getters round;
    /// the exact 32800/32768 classic LSI fraction is retained internally.
    /// F020/F030/A030 and exactly CW32L031C8T6/C8U6/F8U6 or CW32R031C8U6
    /// factory-LSI bounds remain rate-only after division under every SYSCLK;
    /// check `has_cycle_timing_bounds()` before using strict duration methods.
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
    /// ACCESS and write protection are restored on every exit. No failure is
    /// rolled back, and event timing can change. Debugger/NMI stalls are outside
    /// the documented one-second transaction bound; do not single-step a write.
    pub fn set_datetime(&mut self, datetime: DateTime) -> Result<(), RtcError> {
        check_clock(&self.clock)?;
        check_write_mode()?;
        check_access_free()?;
        wait_load(self.config)?;
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
            wait_load(self.config)?;
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
            wait_load(self.config)?;
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
fn check_write_mode() -> Result<(), RtcError> {
    #[cfg(rtc_cw32l010_v1)]
    if pac::SYSCTRL.cr2().read().rtclpm() {
        return Err(RtcError::LowPowerSynchronization);
    }
    Ok(())
}
fn check_access_free() -> Result<(), RtcError> {
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
fn wait_load(config: RtcConfig) -> Result<(), RtcError> {
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
fn check_native_unset_controls() -> Result<(), RtcError> {
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
/// initialization. L011/L012 have no ACCESS field; all writes are direct.
#[cfg(any(rtc_cw32l011_v1, rtc_cw32l012_v1))]
fn initialize_native_source(clock: &CalendarClock<'_>, config: RtcConfig) -> Result<(), RtcError> {
    let (first, second) = clock.prescalers();
    if clock
        .bounds()
        .divided_by(u32::from(first))
        .maximum_exceeds(1_000_000)
    {
        return Err(RtcError::IncompatibleClock);
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
    check_access_free()
}
fn read_calendar(clock: &CalendarClock<'_>, config: RtcConfig) -> Result<DateTime, RtcError> {
    if !pac::RTC.cr0().read().start() {
        return Err(RtcError::NotRunning);
    }
    wait_load(config)?;
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
    check_clock_source(clock)?;
    let (date, time) = encode(datetime, h24);
    pac::RTC.date().write_value(date);
    wait_load(config)?;
    check_clock_source(clock)?;
    pac::RTC.time().write_value(time);
    wait_load(config)?;
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
struct Unlocked;
impl Unlocked {
    fn new() -> Self {
        pac::RTC.key().write(|w| w.set_key(0xca));
        pac::RTC.key().write(|w| w.set_key(0x53));
        Self
    }
}
impl Drop for Unlocked {
    fn drop(&mut self) {
        pac::RTC.key().write(|w| w.set_key(0xca));
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
