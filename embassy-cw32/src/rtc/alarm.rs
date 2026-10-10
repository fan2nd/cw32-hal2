//! Weekly alarm matches and retained event flags.
use super::{DayOfWeek, Rtc, RtcConfig, RtcError, Unlocked, bcd, check_clock, check_write_mode};
use crate::pac;

/// One of the two independently enabled weekly comparators.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Alarm {
    A,
    B,
}

/// Days on which an alarm may match. Sunday is translated to hardware bit zero.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct AlarmDays(u8);
impl AlarmDays {
    pub const EVERY_DAY: Self = Self(0x7f);
    pub const WEEKDAYS: Self = Self(0x3e);
    pub const fn only(day: DayOfWeek) -> Self {
        Self(1 << day.hardware())
    }
    pub const fn and(self, day: DayOfWeek) -> Self {
        Self(self.0 | (1 << day.hardware()))
    }
    pub const fn contains(self, day: DayOfWeek) -> bool {
        self.0 & (1 << day.hardware()) != 0
    }
}

/// Alarm A recurring weekly match, interpreted in the RTC's retained 12/24-hour format.
///
/// Values use ordinary binary 24-hour time; `None` ignores that component.
/// This is not an absolute deadline. Matches recur while the alarm is enabled.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct AlarmAConfig {
    pub days: AlarmDays,
    /// `Some(0..=23)` matches that hour; `None` matches every hour.
    pub hour: Option<u8>,
    /// `Some(0..=59)` matches that minute; `None` matches every minute.
    pub minute: Option<u8>,
    /// `Some(0..=59)` matches that second; `None` matches every second.
    pub second: Option<u8>,
}
impl AlarmAConfig {
    fn validate(self) -> Result<Self, RtcError> {
        if self.hour.is_some_and(|v| v > 23)
            || self.minute.is_some_and(|v| v > 59)
            || self.second.is_some_and(|v| v > 59)
        {
            return Err(RtcError::InvalidAlarm);
        }
        Ok(self)
    }
}

/// One bounded observation; an event can arrive immediately after the read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct AlarmStatus {
    pub enabled: bool,
    pub interrupt_enabled: bool,
    /// Sticky hardware flag. Repeated matches coalesce, rather than being counted.
    pub pending: bool,
}
impl Rtc<'_> {
    /// Replace disabled Alarm A's match without changing its enable or flag.
    ///
    /// An enabled comparator or its interrupt must first be disabled by its
    /// owner. Existing 12/24-hour format, the other alarm and other events are
    /// preserved. A failed write is not rolled back.
    pub fn set_alarm_a(&mut self, config: AlarmAConfig) -> Result<(), RtcError> {
        let config = config.validate()?;
        let alarm = Alarm::A;
        alarm_access(&self.clock, self.config, || {
            if interrupt_enabled(alarm) {
                return Err(RtcError::InterruptInUse);
            }
            if enabled(alarm) {
                return Err(RtcError::AlarmAlreadyEnabled);
            }
            let hour = config.hour.unwrap_or(0);
            let hour = if pac::RTC.cr0().read().h24() {
                bcd(hour)
            } else {
                bcd(if hour % 12 == 0 { 12 } else { hour % 12 }) | if hour >= 12 { 0x20 } else { 0 }
            };
            // F020's source-derived PAC field names differ; all own ALARMA
            // tables consistently specify 1=ignore. ALARMB is contradictory
            // and deliberately has no configuration API.
            let mut value = pac::rtc::regs::Alarma::default();
            value.set_hour(hour);
            value.set_minute(bcd(config.minute.unwrap_or(0)));
            value.set_second(bcd(config.second.unwrap_or(0)));
            #[cfg(rtc_cw32f020_v1)]
            {
                value.set_hourmask(config.hour.is_none());
                value.set_minutemask(config.minute.is_none());
                value.set_secondmask(config.second.is_none());
                value.set_week(config.days.0);
            }
            #[cfg(not(rtc_cw32f020_v1))]
            {
                value.set_houren(config.hour.is_none());
                value.set_minuteen(config.minute.is_none());
                value.set_seconden(config.second.is_none());
                value.set_weekmask(config.days.0);
            }
            pac::RTC.alarma().write_value(value);
            if pac::RTC.alarma().read() != value {
                return Err(RtcError::WriteFailure);
            }
            Ok(())
        })
    }

    /// Enable or disable configured Alarm A, preserving its pending flag.
    ///
    /// An already enabled peripheral interrupt is treated as foreign ownership.
    /// Enabling a matching alarm may immediately produce an event. Disabling
    /// does not acknowledge a previous event or disconnect an RTC_OUT route.
    pub fn set_alarm_a_enabled(&mut self, enable: bool) -> Result<(), RtcError> {
        let alarm = Alarm::A;
        alarm_access(&self.clock, self.config, || {
            if interrupt_enabled(alarm) {
                return Err(RtcError::InterruptInUse);
            }
            pac::RTC.cr2().modify(|w| w.set_alarmaen(enable));
            if enabled(alarm) == enable {
                Ok(())
            } else {
                Err(RtcError::WriteFailure)
            }
        })
    }

    /// Read one alarm's enable, interrupt enable and sticky match flag.
    /// Classic families use a bounded WINDOW/ACCESS transaction even for status.
    pub fn alarm_status(&mut self, alarm: Alarm) -> Result<AlarmStatus, RtcError> {
        alarm_access(&self.clock, self.config, || {
            Ok(AlarmStatus {
                enabled: enabled(alarm),
                interrupt_enabled: interrupt_enabled(alarm),
                pending: pending(alarm),
            })
        })
    }

    /// Acknowledge only the selected sticky match flag.
    ///
    /// Repeated matches coalesce. A match concurrent with acknowledgement can
    /// be lost; a match after the write may already have reasserted the flag.
    /// Success means the bounded access completed, not that the flag stayed low.
    pub fn clear_alarm(&mut self, alarm: Alarm) -> Result<(), RtcError> {
        alarm_access(&self.clock, self.config, || {
            // Every own manual specifies ICR reset/read-one seed 0x0000_007f.
            // Preserve reserved bit 5 at its documented reset value, and write
            // one to every unselected R1W0 flag. Never read/modify/write ISR.
            let mut clear = pac::rtc::regs::Icr::write_noop();
            match alarm {
                Alarm::A => clear.set_alarma(false),
                Alarm::B => clear.set_alarmb(false),
            }
            pac::RTC.icr().write_value(clear);
            Ok(())
        })
    }
}

pub(super) fn enabled(alarm: Alarm) -> bool {
    let value = pac::RTC.cr2().read();
    match alarm {
        Alarm::A => value.alarmaen(),
        Alarm::B => value.alarmben(),
    }
}
pub(super) fn interrupt_enabled(alarm: Alarm) -> bool {
    let value = pac::RTC.ier().read();
    match alarm {
        Alarm::A => value.alarma(),
        Alarm::B => value.alarmb(),
    }
}
pub(super) fn pending(alarm: Alarm) -> bool {
    let value = pac::RTC.isr().read();
    match alarm {
        Alarm::A => value.alarma(),
        Alarm::B => value.alarmb(),
    }
}

/// Only fixed-size driver operations enter this closure, never user code.
fn alarm_access<T>(
    clock: &crate::rcc::CalendarClock<'_>,
    config: RtcConfig,
    f: impl FnOnce() -> Result<T, RtcError>,
) -> Result<T, RtcError> {
    check_clock(clock)?;
    check_write_mode()?;
    #[cfg(not(rtc_alarm_direct_access))]
    {
        let running = pac::RTC.cr0().read().start();
        if running {
            super::wait_classic_window(config)?;
        }
        check_clock(clock)?;
        let result = critical_section::with(|_| {
            check_clock(clock)?;
            if running && !pac::RTC.cr1().read().window() {
                return Err(RtcError::SynchronizationTimeout);
            }
            let _unlock = Unlocked::new();
            let _access = running.then(|| super::Access::new(clock.source()));
            super::check_clock_source(clock)?;
            let value = f()?;
            super::check_clock_source(clock)?;
            Ok(value)
        })?;
        check_clock(clock)?;
        Ok(result)
    }
    #[cfg(rtc_alarm_direct_access)]
    {
        let _ = config;
        // Own access sections exclude ALARMx/CR2/IER/ISR/ICR from the
        // DATE/TIME/AWTARR synchronization protocol, including on L010.
        let result = critical_section::with(|_| {
            check_clock(clock)?;
            let _unlock = Unlocked::new();
            super::check_clock_source(clock)?;
            let value = f()?;
            super::check_clock_source(clock)?;
            Ok(value)
        })?;
        check_clock(clock)?;
        Ok(result)
    }
}
