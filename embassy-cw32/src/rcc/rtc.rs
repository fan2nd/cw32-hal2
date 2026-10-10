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
    /// The frozen LSE mode, pins or current source state do not match.
    #[cfg(rcc_lse)]
    IncompatibleConfiguration,
}

#[cfg(all(not(rcc_lse), any(rtc_cw32l010_v1, rtc_cw32l011_v1, rtc_cw32l012_v1)))]
/// Calendar source on programmable-prescaler RTCs.
pub type CalendarClock<'d> = HsiOscClock<'d>;
#[cfg(all(
    not(rcc_lse),
    any(rtc_v1, rtc_cw32f020_v1, rtc_cw32l031_v1, rtc_cw32l052_v1)
))]
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
        if !Self::oscillator_ready() {
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
    pub(crate) const fn prescalers(&self) -> (u16, u32) {
        (RTC::PRESCALER_FIRST, RTC::PRESCALER_SECOND)
    }
    pub(crate) const fn calendar_divisor(&self) -> u32 {
        RTC::CALENDAR_DIVISOR
    }
    pub(crate) fn source(&self) -> crate::rtc::Source {
        RTC::SOURCE.into()
    }
    pub(crate) fn is_ready(&self) -> bool {
        Self::oscillator_ready()
    }
    fn oscillator_ready() -> bool {
        pac::SYSCTRL.cr1().read().hsien() && pac::SYSCTRL.hsi().read().stable()
    }
}

#[cfg(any(rtc_v1, rtc_cw32f020_v1, rtc_cw32l031_v1, rtc_cw32l052_v1))]
/// Shared LSI with verified, unchanged factory trim and bounded startup polling.
///
/// This capability requires factory trim already loaded. On F020/F030/A030,
/// selecting `Sysclk::LSI` during RCC initialization can establish it, as can
/// `Sysclk::LSE` on its three qualified packages; otherwise
/// board startup or a bootloader must do so. This constructor never loads trim:
/// LSIEN=0 does not prove no shared hardware user is starting the oscillator.
/// A mismatch is rejected before any write. Existing WAITCYCLE, consumers and
/// RTC state are preserved. Software
/// enable is retained on timeout and Drop so another user never loses LSI.
///
/// The own datasheet nominal is 32,800 Hz, not 32,768 Hz. Bounds require the
/// published supply/ambient interval and unchanged factory trim; readiness is
/// startup status, not a measurement or continuous loss-of-clock monitor.
/// On F020/F030/A030 these are rate-only bounds even when SYSCLK uses another
/// source; they do not qualify strict cycle durations.
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
            if Self::oscillator_ready() {
                return Ok(Self { _sysctrl: sysctrl });
            }
            core::hint::spin_loop();
        }
        Err(RtcClockError::NotReady)
    }
    pub const fn frequency(&self) -> Hertz {
        Hertz(RTC::SOURCE_NOMINAL_HZ)
    }
    /// Factory-source envelope. F020/F030/A030 qualify rate only;
    /// check `has_cycle_timing_bounds()` before requesting strict durations.
    pub const fn bounds(&self) -> ClockBounds {
        ClockBounds::rtc_source()
    }
    pub(crate) const fn calendar_divisor(&self) -> u32 {
        RTC::CALENDAR_DIVISOR
    }
    pub(crate) fn source(&self) -> crate::rtc::Source {
        RTC::SOURCE.into()
    }
    pub(crate) fn is_ready(&self) -> bool {
        Self::oscillator_ready()
    }
    fn oscillator_ready() -> bool {
        pac::SYSCTRL.cr1().read().lsien() && pac::SYSCTRL.lsi().read().stable()
    }
}

#[cfg(rcc_lse)]
/// Source ownership for the explicitly qualified exact LSE packages.
/// Selecting LSE keeps its board-qualified envelope conditional on source health.
/// No RTC fallback to LSI, elapsed-time continuity or fault recovery is promised.
pub enum CalendarClock<'d> {
    #[cfg(not(any(rtc_cw32l010_v1, rtc_cw32l011_v1, rtc_cw32l012_v1)))]
    Lsi(LsiClock<'d>),
    #[cfg(any(rtc_cw32l010_v1, rtc_cw32l011_v1, rtc_cw32l012_v1))]
    HsiOsc(HsiOscClock<'d>),
    Lse(LseClock<'d>),
}
#[cfg(rcc_lse)]
impl<'d> CalendarClock<'d> {
    /// Existing factory-trim LSI acquisition path.
    #[cfg(not(any(rtc_cw32l010_v1, rtc_cw32l011_v1, rtc_cw32l012_v1)))]
    pub fn new(sysctrl: Peri<'d, SYSCTRL>, poll_budget: u32) -> Result<Self, RtcClockError> {
        LsiClock::new(sysctrl, poll_budget).map(Self::Lsi)
    }
    /// Existing native HSIOSC acquisition; same one-argument constructor.
    #[cfg(any(rtc_cw32l010_v1, rtc_cw32l011_v1, rtc_cw32l012_v1))]
    pub fn new(sysctrl: Peri<'d, SYSCTRL>) -> Result<Self, RtcClockError> {
        HsiOscClock::new(sysctrl).map(Self::HsiOsc)
    }
    pub const fn frequency(&self) -> Hertz {
        match self {
            #[cfg(not(any(rtc_cw32l010_v1, rtc_cw32l011_v1, rtc_cw32l012_v1)))]
            Self::Lsi(c) => c.frequency(),
            #[cfg(any(rtc_cw32l010_v1, rtc_cw32l011_v1, rtc_cw32l012_v1))]
            Self::HsiOsc(c) => c.frequency(),
            Self::Lse(c) => c.frequency(),
        }
    }
    /// Declared healthy-source envelope, not a guarantee after oscillator faults.
    /// Preserves the selected source's rate-only or cycle-timing qualification.
    pub const fn bounds(&self) -> ClockBounds {
        match self {
            #[cfg(not(any(rtc_cw32l010_v1, rtc_cw32l011_v1, rtc_cw32l012_v1)))]
            Self::Lsi(c) => c.bounds(),
            #[cfg(any(rtc_cw32l010_v1, rtc_cw32l011_v1, rtc_cw32l012_v1))]
            Self::HsiOsc(c) => c.bounds(),
            Self::Lse(c) => c.bounds(),
        }
    }
    pub(crate) const fn calendar_divisor(&self) -> u32 {
        match self {
            #[cfg(not(any(rtc_cw32l010_v1, rtc_cw32l011_v1, rtc_cw32l012_v1)))]
            Self::Lsi(c) => c.calendar_divisor(),
            #[cfg(any(rtc_cw32l010_v1, rtc_cw32l011_v1, rtc_cw32l012_v1))]
            Self::HsiOsc(c) => c.calendar_divisor(),
            Self::Lse(c) => c.calendar_divisor(),
        }
    }
    #[cfg(any(rtc_cw32l010_v1, rtc_cw32l011_v1, rtc_cw32l012_v1))]
    pub(crate) const fn prescalers(&self) -> (u16, u32) {
        match self {
            Self::HsiOsc(c) => c.prescalers(),
            Self::Lse(_) => (
                crate::RCC_LSE_RTC_FIRST_DIVISOR,
                crate::RCC_LSE_RTC_SECOND_DIVISOR,
            ),
        }
    }
    pub(crate) fn source(&self) -> crate::rtc::Source {
        match self {
            #[cfg(not(any(rtc_cw32l010_v1, rtc_cw32l011_v1, rtc_cw32l012_v1)))]
            Self::Lsi(c) => c.source(),
            #[cfg(any(rtc_cw32l010_v1, rtc_cw32l011_v1, rtc_cw32l012_v1))]
            Self::HsiOsc(c) => c.source(),
            Self::Lse(c) => c.source(),
        }
    }
    pub(crate) fn is_ready(&self) -> bool {
        match self {
            #[cfg(not(any(rtc_cw32l010_v1, rtc_cw32l011_v1, rtc_cw32l012_v1)))]
            Self::Lsi(c) => c.is_ready(),
            #[cfg(any(rtc_cw32l010_v1, rtc_cw32l011_v1, rtc_cw32l012_v1))]
            Self::HsiOsc(c) => c.is_ready(),
            Self::Lse(c) => c.is_ready(),
        }
    }
}
#[cfg(all(rcc_lse, not(any(rtc_cw32l010_v1, rtc_cw32l011_v1, rtc_cw32l012_v1))))]
impl<'d> From<LsiClock<'d>> for CalendarClock<'d> {
    fn from(clock: LsiClock<'d>) -> Self {
        Self::Lsi(clock)
    }
}
#[cfg(all(rcc_lse, any(rtc_cw32l010_v1, rtc_cw32l011_v1, rtc_cw32l012_v1)))]
impl<'d> From<HsiOscClock<'d>> for CalendarClock<'d> {
    fn from(clock: HsiOscClock<'d>) -> Self {
        Self::HsiOsc(clock)
    }
}
#[cfg(rcc_lse)]
impl<'d> From<LseClock<'d>> for CalendarClock<'d> {
    fn from(clock: LseClock<'d>) -> Self {
        Self::Lse(clock)
    }
}

#[cfg(rcc_lse)]
/// Owns the source and its physical pads for an RCC-initialized LSE calendar.
/// Acquisition only verifies the frozen init record and live state. It never
/// starts, stops or reconfigures an oscillator or pad. Drop retains the source.
/// Bounds require the board's complete per-cycle and electrical guarantees;
/// EN/STABLE and sticky fault flags are checked by every RTC operation.
/// On L010/L011/L012, `LseFaultDetection::StartupOnly` leaves CCS clear: later
/// source loss can be invisible because STABLE latches and no fault is raised.
/// Monitored operation retains the already selected brake/timer fault routes.
/// On L011/L012, acquisition temporarily opens GPIOC to inspect the oscillator
/// pads. Whole-bank sampling, filters and armed edges may advance, including
/// before an error; restoring the gate does not undo that progress. Supported
/// functional handover must permit it. Acquisition does not establish ownership
/// of RTC_OUT/RTC_1Hz recipients for later calendar activation; see
/// [`crate::rtc::Rtc::initialize_if_unset`].
/// Drop, forgetting the capability, and failed acquisition never release pads.
pub struct LseClock<'d> {
    _sysctrl: Peri<'d, SYSCTRL>,
    _input: Peri<'d, crate::gpio::AnyPin>,
    _output: Option<Peri<'d, crate::gpio::AnyPin>>,
    config: super::Lse,
    bounds: ClockBounds,
}
#[cfg(rcc_lse)]
impl<'d> LseClock<'d> {
    pub fn new(
        sysctrl: Peri<'d, SYSCTRL>,
        input: Peri<'d, impl crate::gpio::Pin>,
        output: Peri<'d, impl crate::gpio::Pin>,
    ) -> Result<Self, RtcClockError> {
        Self::acquire(
            sysctrl,
            input.into(),
            Some(output.into()),
            super::LseMode::Oscillator,
        )
    }
    pub fn new_bypass(
        sysctrl: Peri<'d, SYSCTRL>,
        input: Peri<'d, impl crate::gpio::Pin>,
    ) -> Result<Self, RtcClockError> {
        Self::acquire(sysctrl, input.into(), None, super::LseMode::Bypass)
    }
    fn acquire(
        sysctrl: Peri<'d, SYSCTRL>,
        input: Peri<'d, crate::gpio::AnyPin>,
        output: Option<Peri<'d, crate::gpio::AnyPin>>,
        mode: super::LseMode,
    ) -> Result<Self, RtcClockError> {
        let clocks = super::try_clocks().ok_or(RtcClockError::NotInitialized)?;
        let (config, bounds) = clocks.lse.ok_or(RtcClockError::NotInitialized)?;
        if config.mode != mode
            || Some(input.pin_port) != crate::RCC_LSE_PINS.0
            || output.as_ref().map(|p| p.pin_port)
                != if mode == super::LseMode::Oscillator {
                    crate::RCC_LSE_PINS.1
                } else {
                    None
                }
        {
            return Err(RtcClockError::IncompatibleConfiguration);
        }
        critical_section::with(|cs| super::lse::verify(config, cs))
            .map_err(|_| RtcClockError::IncompatibleConfiguration)?;
        Ok(Self {
            _sysctrl: sysctrl,
            _input: input,
            _output: output,
            config,
            bounds,
        })
    }
    pub const fn frequency(&self) -> Hertz {
        self.bounds.nominal()
    }
    /// Healthy-source envelope; average ppm alone does not establish this bound.
    pub const fn bounds(&self) -> ClockBounds {
        self.bounds
    }
    /// Frozen native monitoring semantics; startup-only checks cannot prove a
    /// source is still running. No continuous frequency measurement is made.
    #[cfg(any(rtc_cw32l010_v1, rtc_cw32l011_v1, rtc_cw32l012_v1))]
    pub const fn fault_detection(&self) -> super::LseFaultDetection {
        self.config.fault_detection
    }
    pub(crate) const fn calendar_divisor(&self) -> u32 {
        #[cfg(any(rtc_cw32l010_v1, rtc_cw32l011_v1, rtc_cw32l012_v1))]
        {
            crate::RCC_LSE_RTC_CALENDAR_DIVISOR
        }
        #[cfg(not(any(rtc_cw32l010_v1, rtc_cw32l011_v1, rtc_cw32l012_v1)))]
        {
            RTC::CALENDAR_DIVISOR
        }
    }
    pub(crate) fn source(&self) -> crate::rtc::Source {
        crate::RCC_LSE_RTC_SOURCE
    }
    pub(crate) fn is_ready(&self) -> bool {
        super::lse::healthy(self.config)
    }
}
