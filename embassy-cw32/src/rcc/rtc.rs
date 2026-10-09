//! Lifetime-held calendar sources. Acquisition never changes oscillator trim.
use super::ClockBounds;
use crate::{
    Peri, pac,
    peripherals::{RTC, SYSCTRL},
    rtc::sealed::Instance,
    time::Hertz,
};

#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum RtcClockError {
    NotInitialized,
    /// Enabled oscillator did not report stable within the poll budget.
    NotReady,
    InvalidTimeout,
    /// Factory calibration storage is erased or current LSI trim differs.
    /// No clock or trim write is performed on this error.
    IncompatibleCalibration,
}

#[cfg(any(rtc_cw32l010_v1, rtc_cw32l011_v1, rtc_cw32l012_v1))]
/// Calendar source on programmable-prescaler RTCs.
pub type CalendarClock<'d> = HsiOscClock<'d>;
#[cfg(any(rtc_v1, rtc_cw32f020_v1, rtc_cw32l031_v1, rtc_cw32l052_v1))]
/// Calendar source on fixed-/32768 RTCs.
pub type CalendarClock<'d> = LsiClock<'d>;

#[cfg(any(rtc_cw32l010_v1, rtc_cw32l011_v1, rtc_cw32l012_v1))]
/// Shared frozen factory HSIOSC: 48 MHz on L010, 96 MHz on L011/L012.
/// Consumes SYSCTRL to prevent duplicate safe clock capabilities. The supported
/// RCC tree keeps HSIOSC alive after Drop; deep sleep is outside this contract.
pub struct HsiOscClock<'d> {
    _sysctrl: Peri<'d, SYSCTRL>,
}
#[cfg(any(rtc_cw32l010_v1, rtc_cw32l011_v1, rtc_cw32l012_v1))]
impl<'d> HsiOscClock<'d> {
    pub fn new(sysctrl: Peri<'d, SYSCTRL>) -> Result<Self, RtcClockError> {
        if super::try_clocks().is_none() {
            return Err(RtcClockError::NotInitialized);
        }
        if !Self::is_ready() {
            return Err(RtcClockError::NotReady);
        }
        Ok(Self { _sysctrl: sysctrl })
    }
    pub const fn frequency(&self) -> Hertz {
        Hertz(RTC::SOURCE_NOMINAL_HZ)
    }
    pub const fn bounds(&self) -> ClockBounds {
        ClockBounds::rtc_source()
    }
    pub(crate) fn is_ready() -> bool {
        pac::SYSCTRL.cr1().read().hsien() && pac::SYSCTRL.hsi().read().stable()
    }
}

#[cfg(any(rtc_v1, rtc_cw32f020_v1, rtc_cw32l031_v1, rtc_cw32l052_v1))]
/// Shared LSI with verified, unchanged factory trim and bounded startup polling.
///
/// This capability requires factory trim already loaded by board startup or a
/// bootloader. It never loads trim itself: LSIEN=0 does not prove no shared
/// hardware user is starting the oscillator. A mismatch is rejected before any
/// write. Existing WAITCYCLE, consumers and RTC state are preserved. Software
/// enable is retained on timeout and Drop so another user never loses LSI.
///
/// The own datasheet nominal is 32,800 Hz, not 32,768 Hz. Bounds require the
/// published supply/ambient interval and unchanged factory trim; readiness is
/// startup status, not a measurement or continuous loss-of-clock monitor.
pub struct LsiClock<'d> {
    _sysctrl: Peri<'d, SYSCTRL>,
}
#[cfg(any(rtc_v1, rtc_cw32f020_v1, rtc_cw32l031_v1, rtc_cw32l052_v1))]
impl<'d> LsiClock<'d> {
    pub fn new(sysctrl: Peri<'d, SYSCTRL>, poll_budget: u32) -> Result<Self, RtcClockError> {
        if super::try_clocks().is_none() {
            return Err(RtcClockError::NotInitialized);
        }
        if poll_budget == 0 {
            return Err(RtcClockError::InvalidTimeout);
        }
        // Factory address is a reviewed readable halfword, not a caller pointer.
        let factory = unsafe { core::ptr::read_volatile(RTC::FACTORY_TRIM_ADDRESS as *const u16) };
        let mut calibration = pac::sysctrl::regs::Lsi::default();
        calibration.set_trim(factory);
        if factory == u16::MAX || pac::SYSCTRL.lsi().read().trim() != calibration.trim() {
            return Err(RtcClockError::IncompatibleCalibration);
        }
        critical_section::with(|_| {
            pac::SYSCTRL.cr1().modify(|w| {
                w.set_key(0x5a5a);
                w.set_lsien(true);
            })
        });
        for _ in 0..poll_budget {
            if Self::is_ready() {
                return Ok(Self { _sysctrl: sysctrl });
            }
            core::hint::spin_loop();
        }
        Err(RtcClockError::NotReady)
    }
    pub const fn frequency(&self) -> Hertz {
        Hertz(RTC::SOURCE_NOMINAL_HZ)
    }
    pub const fn bounds(&self) -> ClockBounds {
        ClockBounds::rtc_source()
    }
    pub(crate) fn is_ready() -> bool {
        pac::SYSCTRL.cr1().read().lsien() && pac::SYSCTRL.lsi().read().stable()
    }
}
