//! CW32L012 one-time factory-HSI and qualified direct-HSE clocks.
//!
//! Own-source qualification and retained-owner rules: docs/qualified-l012-hse.md.
//! HSI defaults to /12 (8 MHz); the distinct hardware CCS fallback is /24 (4 MHz).
//! Incoming oscillator frequencies, buses and Flash latency must already be legal.
//! Other bus-dependent peripherals, DMA and application interrupts must be quiescent.
//! No source frequency or board condition is measured. STABLE is a startup latch;
//! later source loss can halt execution or invalidate frozen clocks after fallback.
//! Poll limits count CPU iterations while execution continues, not elapsed time.
//! Failure publishes no clocks and requires reset before retrying. No PLL exists;
//! runtime retuning, automatic HEXEN, sleep/resume and recovery are not provided.
//! Common RCC captures inherited LSE ownership before this backend runs.

use crate::{pac, time::Hertz};
use core::cell::Cell;
use critical_section::Mutex;
use pac::sysctrl::vals::Sysclk as ClockSource;

/// Nominal calibrated internal high-speed oscillator frequency.
pub const HSI_FREQ: Hertz = Hertz(crate::RCC_HSI_FREQUENCY_HZ);

/// Documented divider between HSIOSC and HSI. Values are hardware encodings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(u8)]
pub enum HsiDiv {
    /// Divide by 1.
    Div1 = 1,
    /// Divide by 2.
    Div2 = 2,
    /// Divide by 3.
    Div3 = 3,
    /// Divide by 4.
    Div4 = 4,
    /// Divide by 5.
    Div5 = 5,
    /// Divide by 6.
    Div6 = 6,
    /// Divide by 7.
    Div7 = 7,
    /// Divide by 8.
    Div8 = 8,
    /// Divide by 9.
    Div9 = 9,
    /// Divide by 10.
    Div10 = 10,
    /// Divide by 12.
    Div12 = 11,
    /// Divide by 16.
    Div16 = 12,
    /// Divide by 20.
    Div20 = 13,
    /// Divide by 24.
    Div24 = 14,
    /// Divide by 28.
    Div28 = 15,
    /// Divide by 32.
    Div32 = 0,
}
impl HsiDiv {
    /// Numeric division factor (not the register encoding).
    pub const fn divisor(self) -> u32 {
        match self {
            Self::Div1 => 1,
            Self::Div2 => 2,
            Self::Div3 => 3,
            Self::Div4 => 4,
            Self::Div5 => 5,
            Self::Div6 => 6,
            Self::Div7 => 7,
            Self::Div8 => 8,
            Self::Div9 => 9,
            Self::Div10 => 10,
            Self::Div12 => 12,
            Self::Div16 => 16,
            Self::Div20 => 20,
            Self::Div24 => 24,
            Self::Div28 => 28,
            Self::Div32 => 32,
        }
    }
}

const DEFAULT_DIV: HsiDiv = crate::RCC_DEFAULT_HSI_DIV;
const MAX_FLASH_WAIT: u32 = crate::RCC_INITIAL_FLASH_WAIT;

/// HSI configuration. HSI is always enabled in the supported clock tree.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Hsi {
    /// Divider applied to the factory-calibrated 96 MHz HSIOSC clock.
    pub div: HsiDiv,
}
impl Default for Hsi {
    fn default() -> Self {
        Self { div: DEFAULT_DIV }
    }
}

/// Divider from SysClk to HCLK.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(u8)]
pub enum AHBPrescaler {
    /// Divide by 1.
    Div1 = 0,
    /// Divide by 2.
    Div2 = 1,
    /// Divide by 4.
    Div4 = 2,
    /// Divide by 8.
    Div8 = 3,
    /// Divide by 16.
    Div16 = 4,
    /// Divide by 32.
    Div32 = 5,
    /// Divide by 64.
    Div64 = 6,
    /// Divide by 128.
    Div128 = 7,
}
impl AHBPrescaler {
    /// Numeric division factor.
    pub const fn divisor(self) -> u32 {
        1 << self as u8
    }
}

/// Divider from HCLK to PCLK. L012 has one PCLK domain.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(u8)]
pub enum APBPrescaler {
    /// Divide by 1.
    Div1 = 0,
    /// Divide by 2.
    Div2 = 1,
    /// Divide by 4.
    Div4 = 2,
    /// Divide by 8.
    Div8 = 3,
}
impl APBPrescaler {
    /// Numeric division factor.
    pub const fn divisor(self) -> u32 {
        1 << self as u8
    }
}

/// Direct system-clock sources implemented by this backend.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Sysclk {
    /// Factory-calibrated HSI through its configured divider.
    HSI,
    /// Qualified external high-speed oscillator or input.
    HSE,
}

/// HSE electrical mode. The board must reserve the actual oscillator pads.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum HseMode {
    /// Crystal or ceramic resonator on OSC_IN and OSC_OUT.
    Oscillator,
    /// Externally driven digital clock on OSC_IN; OSC_OUT remains available.
    Bypass,
}

pub use pac::sysctrl::vals::HseDrive;

/// Board-qualified external clock, independent of factory HSI accuracy.
///
/// All three frequencies are required. Bounds include oscillator tolerance,
/// temperature, aging, load, short-term cycle variation and external oscillator
/// supply effects throughout
/// `operating_conditions`. The MCU cannot measure or enforce that declaration.
/// Bypass also requires the own datasheet's input voltage, pulse width, edge and
/// duty constraints. In oscillator mode the board must qualify drive/load/startup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Hse {
    /// Nominal source frequency; never an electrical upper-bound substitute.
    pub freq: Hertz,
    /// Guaranteed minimum actual frequency, in whole hertz.
    pub min_freq: Hertz,
    /// Guaranteed maximum actual frequency, in whole hertz.
    pub max_freq: Hertz,
    /// MCU supply and ambient interval over which the source bounds are valid.
    pub operating_conditions: crate::rcc::OperatingConditions,
    pub mode: HseMode,
    /// Crystal drive setting for both pre-start and run phases.
    /// Ignored electrically in bypass mode.
    pub drive: HseDrive,
}
impl Hse {
    fn bounds(self) -> Result<crate::rcc::ClockBounds, Error> {
        if self.drive.to_bits() > HseDrive::Level7.to_bits() {
            return Err(Error::InvalidHseConfiguration);
        }
        let range = match self.mode {
            HseMode::Oscillator => crate::RCC_HSE_CRYSTAL_RANGE_HZ,
            HseMode::Bypass => crate::RCC_HSE_BYPASS_RANGE_HZ,
        };
        let bounds = crate::rcc::ClockBounds::external(
            self.freq,
            self.min_freq,
            self.max_freq,
            self.operating_conditions,
        )
        .ok_or(Error::InvalidHseBounds)?;
        if self.min_freq.0 < range.0 || self.max_freq.0 > range.1 {
            return Err(Error::HseOutsideQualifiedRange);
        }
        let c = self.operating_conditions;
        if c.min_supply_mv < crate::RCC_HSE_SUPPLY_MV.0
            || c.max_supply_mv > crate::RCC_HSE_SUPPLY_MV.1
            || c.min_temperature_c < crate::RCC_HSE_TEMPERATURE_C.0
            || c.max_temperature_c > crate::RCC_HSE_TEMPERATURE_C.1
        {
            return Err(Error::HseConditionsOutsideQualifiedRange);
        }
        Ok(bounds)
    }

    fn detector_count(self) -> Result<u16, Error> {
        let count = crate::RCC_HSE_CCS_NUMERATOR_HZ.div_ceil(u64::from(self.min_freq.0));
        // Check the count, and prove the minimum source can satisfy a complete
        // detector window even at the fastest documented legal unchanged LSI.
        if count == 0
            || count > u64::from(crate::RCC_HSE_CCS_MAXIMUM_COUNT)
            || u64::from(self.min_freq.0) * count
                <= u64::from(crate::RCC_HSE_CCS_CYCLE_COUNT)
                    * u64::from(crate::RCC_HSE_CCS_LSI_MAXIMUM_HZ)
        {
            return Err(Error::InvalidHseDetector);
        }
        Ok(count as u16)
    }
}

/// Clock tree initialization options, using Embassy's RCC configuration naming.
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub struct Config {
    /// Board-declared supply and ambient TA envelope. No measurement is made.
    /// The full qualified range is the default; declare the actual narrower
    /// board range to permit faster operation where the datasheet allows it.
    pub operating_conditions: crate::rcc::OperatingConditions,
    /// HSI oscillator divider.
    pub hsi: Hsi,
    /// Board-qualified HSE; its oscillator pads are reserved even with HSI SYSCLK.
    pub hse: Option<Hse>,
    /// Native board-qualified LSE, independent of system-clock selection.
    /// Requires the downstream RTC/LSE-output, UART3 and whole-GPIOC
    /// functional handover documented by [`crate::init`]. Failure retains
    /// requested pads and may retain the source request.
    #[cfg(rcc_lse)]
    pub lse: Option<super::Lse>,
    /// Requested system source. Factory HSI remains enabled.
    pub sys: Sysclk,
    /// HCLK prescaler.
    pub ahb_pre: AHBPrescaler,
    /// PCLK prescaler.
    pub apb_pre: APBPrescaler,
    /// Maximum register polls for each hardware wait. Must be nonzero.
    ///
    /// This is an iteration budget, not a duration: the entry clock may be
    /// unknown. No timer or Embassy time driver is required during init.
    pub timeout: u32,
}
impl Config {
    /// Default nominal 8 MHz clock tree. Silicon reset is 4 MHz (HSI /24).
    pub const fn new() -> Self {
        Self {
            operating_conditions: crate::rcc::OperatingConditions::new(),
            hsi: Hsi { div: DEFAULT_DIV },
            hse: None,
            #[cfg(rcc_lse)]
            lse: None,
            sys: Sysclk::HSI,
            ahb_pre: AHBPrescaler::Div1,
            apb_pre: APBPrescaler::Div1,
            timeout: 100_000,
        }
    }

    /// Validate the board envelope and calculate clocks without touching hardware.
    ///
    /// Declared VDD/ambient ranges must be inside the selected family's source
    /// qualification. Actual upper HCLK/PCLK must fit the minimum-VDD bus limit.
    /// Nominal Hertz remains rounded down; the bound getters retain exact source
    /// division. A nominal rate equal to a rated ceiling may be rejected because
    /// its qualified positive HSI error crosses that ceiling. No voltage,
    /// temperature or frequency is measured. A stable, legal incoming clock and
    /// appropriate incoming FLASH latency remain initialization preconditions.
    pub fn frequencies(&self) -> Result<Clocks, Error> {
        if self.timeout == 0 {
            return Err(Error::InvalidTimeout);
        }
        let hsi = HSI_FREQ / self.hsi.div.divisor();
        let hse = if let Some(hse) = self.hse {
            let bounds = hse.bounds()?;
            let board = self.operating_conditions;
            let source = hse.operating_conditions;
            if board.min_supply_mv < source.min_supply_mv
                || board.max_supply_mv > source.max_supply_mv
                || board.min_temperature_c < source.min_temperature_c
                || board.max_temperature_c > source.max_temperature_c
            {
                return Err(Error::HseConditionsDoNotCoverBoard);
            }
            if crate::RCC_HSE_PINS.0.is_none()
                || (hse.mode == HseMode::Oscillator && crate::RCC_HSE_PINS.1.is_none())
            {
                return Err(Error::HsePinsUnavailable);
            }
            hse.detector_count()?;
            Some(bounds)
        } else {
            None
        };
        #[cfg(rcc_lse)]
        let lse = self
            .lse
            .map(|c| {
                c.bounds(self.operating_conditions)
                    .map(|bounds| (c, bounds))
            })
            .transpose()?;
        let source = match self.sys {
            Sysclk::HSI => crate::rcc::ClockBounds::hsi(self.hsi.div.divisor()),
            Sysclk::HSE => hse.ok_or(Error::HseNotConfigured)?,
        };
        let clocks = Clocks {
            hsi,
            sys: source.nominal(),
            hclk: source.divided_by(self.ahb_pre.divisor()).nominal(),
            pclk: source
                .divided_by(self.ahb_pre.divisor() * self.apb_pre.divisor())
                .nominal(),
            dividers: [
                self.hsi.div.divisor(),
                self.ahb_pre.divisor(),
                self.ahb_pre.divisor() * self.apb_pre.divisor(),
            ],
            source,
            hse: self.hse.map(|hse| hse.mode),
            #[cfg(rcc_lse)]
            lse,
        };
        crate::rcc::operating::validate(self.operating_conditions, clocks)?;
        // Validate the requested retained HSI as a possible system source too.
        crate::rcc::operating::validate(
            self.operating_conditions,
            Clocks {
                source: crate::rcc::ClockBounds::hsi(self.hsi.div.divisor()),
                ..clocks
            },
        )?;
        // Own L012 hardware forces /24 after CCS, independently of the default /12.
        crate::rcc::operating::validate(
            self.operating_conditions,
            Clocks {
                source: crate::rcc::ClockBounds::hsi(crate::RCC_FIXED_CCS_HSI_DIVISOR),
                ..clocks
            },
        )?;
        Ok(clocks)
    }
}
impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}

/// Nominal frequencies frozen after successful HAL initialization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Clocks {
    /// Divided HSI output, not the undivided HSIOSC frequency.
    pub hsi: Hertz,
    /// System clock mux output.
    pub sys: Hertz,
    /// CPU/AHB clock, including the flash interface.
    pub hclk: Hertz,
    /// APB clock before peripheral-local prescalers and muxes.
    pub pclk: Hertz,
    // Exact cumulative divisors; nominal display rounding must not weaken bounds.
    pub(crate) dividers: [u32; 3],
    pub(crate) source: crate::rcc::ClockBounds,
    hse: Option<HseMode>,
    /// Frozen source declaration, conditional on continuing health.
    #[cfg(rcc_lse)]
    pub lse: Option<(super::Lse, super::ClockBounds)>,
}

/// A clock initialization failure.
///
/// Hardware errors may leave a partially changed clock tree. The initializer
/// never lowers flash latency before the requested clocks have been verified.
/// After a failure, no frequencies are published and peripherals must not be
/// used on the assumption that the requested configuration took effect.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {
    #[cfg(rcc_lse)]
    InvalidLseBounds,
    #[cfg(rcc_lse)]
    LseClockInUse,
    #[cfg(rcc_lse)]
    LsePinConflict,
    #[cfg(rcc_lse)]
    LseGpioGateTimeout,
    /// A configuration gate failed to open and/or return to its entry state.
    /// Both fields may be true; no successful restoration is then promised.
    #[cfg(rcc_lse)]
    LseConfigurationGateTimeout {
        enable_failed: bool,
        restore_failed: bool,
    },
    #[cfg(rcc_lse)]
    LseMonitorNotReady,
    #[cfg(rcc_lse)]
    LseNotReady,
    InvalidHseBounds,
    HseOutsideQualifiedRange,
    HseConditionsOutsideQualifiedRange,
    HseConditionsDoNotCoverBoard,
    HseNotConfigured,
    HsePinsUnavailable,
    InvalidHseDetector,
    InvalidHseConfiguration,
    HseClockInUse,
    HsePinConfigurationTimeout,
    /// Enabled inherited HSE needs an explicit, exactly matching declaration.
    InheritedHseEnabled,
    HseTimeout,
    ExternalClockFault,
    InvalidEntryClock,
    RetainedClockInspectionTimeout,
    RetainedRtcConfiguration,
    AdcClockInUse,
    LvdClockInUse,
    ComparatorClockInUse,
    OpaClockInUse,
    DacClockInUse,
    /// The selected timer cannot divide this exact nominal PCLK to 1 MHz.
    #[cfg(feature = "_time-driver")]
    UnsupportedTimeDriverClock,
    /// The reserved timer clock/reset did not acknowledge initialization.
    #[cfg(feature = "_time-driver")]
    TimeDriverClockFailure,
    /// Declared minimum VDD exceeds the declared maximum.
    InvalidSupplyRange,
    /// Declared minimum ambient TA exceeds the declared maximum.
    InvalidTemperatureRange,
    /// Declared supply interval extends outside the factory-HSI qualification.
    SupplyOutsideQualifiedRange,
    /// Declared ambient interval extends outside the factory-HSI qualification.
    TemperatureOutsideQualifiedRange,
    /// Worst-case actual HCLK exceeds the declared-voltage bus ceiling.
    HclkTooHigh,
    /// Worst-case actual PCLK exceeds the declared-voltage bus ceiling.
    PclkTooHigh,
    /// A polling budget of zero is invalid.
    InvalidTimeout,
    /// The clock tree has already been frozen by a successful initialization.
    AlreadyInitialized,
    /// The incoming system clock selector is reserved.
    InvalidClockSource,
    /// Factory HSI calibration storage reads as erased (0xffff).
    InvalidCalibration,
    /// An HSIOSC-selected RTC requires the current enabled, stable factory trim.
    /// No oscillator is changed on this error; the RTC bus gate is restored.
    RtcClockInUse,
    /// The RTC configuration bus gate did not acknowledge enable.
    RtcClockGateTimeout,
    /// The FLASH configuration clock could not be enabled.
    FlashClockTimeout,
    /// FLASH WAIT did not read back as requested.
    FlashLatencyTimeout,
    /// The temporary LSI source did not report stable within the poll budget.
    LsiTimeout,
    /// The system clock did not switch to LSI before stopping HSI.
    TemporaryClockSwitchTimeout,
    /// HSI enable or stable status did not clear before calibration.
    HsiStopTimeout,
    /// HSI did not report stable within the poll budget.
    HsiTimeout,
    /// Temporary LSI/clock-security state did not restore after HSI handover.
    TemporaryClockRestoreTimeout,
    /// The system clock selector did not read back as HSI.
    ClockSwitchTimeout,
    /// The requested HSI configuration or bus dividers did not read back.
    ClockConfigurationTimeout,
}

static CLOCKS: Mutex<Cell<Option<Clocks>>> = Mutex::new(Cell::new(None));

/// Get the frozen clock frequencies, or `None` before a successful HAL init.
pub fn try_clocks() -> Option<Clocks> {
    critical_section::with(|cs| CLOCKS.borrow(cs).get())
}

/// Get the frozen clock frequencies.
///
/// Panics before successful HAL initialization. Direct clock-register changes
/// after init invalidate these frequencies and are the caller's responsibility.
pub fn clocks() -> Clocks {
    try_clocks().expect("embassy-cw32 clocks are not initialized")
}

/// Configure once, before HAL drivers or application interrupts can use clocks.
///
/// Safety: the caller must own the device initialization sequence. No other
/// code, including DMA or NMI handlers, may change or depend on clock registers
/// during this call. Normal interrupts are masked for the complete sequence.
pub(crate) unsafe fn init(config: Config) -> Result<(), Error> {
    critical_section::with(|cs| {
        if CLOCKS.borrow(cs).get().is_some() {
            return Err(Error::AlreadyInitialized);
        }
        let clocks = configure(config, cs)?;
        CLOCKS.borrow(cs).set(Some(clocks));
        Ok(())
    })
}

pub(crate) fn hse_pin_reserved(pin: u8) -> bool {
    let Some(clocks) = try_clocks() else {
        return false;
    };
    clocks.hse.is_some_and(|mode| {
        crate::RCC_HSE_PINS.0 == Some(pin)
            || (mode == HseMode::Oscillator && crate::RCC_HSE_PINS.1 == Some(pin))
    })
}

// Waits remain bounded by the caller's CPU-iteration budget while execution
// continues. A predicate may inspect several related hardware fields.
fn poll(
    mut ready: impl FnMut() -> bool,
    timeout: u32,
    error: Error,
    monitor_hse: bool,
    monitor_lse: bool,
) -> Result<(), Error> {
    for _ in 0..timeout {
        check_external_faults(monitor_hse, monitor_lse)?;
        if ready() {
            return Ok(());
        }
        core::hint::spin_loop();
    }
    Err(error)
}

fn barrier() {
    #[cfg(target_arch = "arm")]
    {
        cortex_m::asm::dsb();
        cortex_m::asm::isb();
    }
}

fn set_flash_latency(
    wait: u32,
    timeout: u32,
    monitor_hse: bool,
    monitor_lse: bool,
) -> Result<(), Error> {
    // L012 has FETCH, CACHE and CACHEINVALID plus RFU. Preserve all of them;
    // change only WAIT through FLASH, never the SYSCTRL brake/debug mirror.
    pac::FLASH.cr2().modify(|w| {
        w.set_key(0x5a5a);
        w.set_wait(wait as u8);
    });
    poll(
        || pac::FLASH.cr2().read().wait() == wait as u8,
        timeout,
        Error::FlashLatencyTimeout,
        monitor_hse,
        monitor_lse,
    )?;
    barrier();
    Ok(())
}

fn configure(config: Config, cs: critical_section::CriticalSection<'_>) -> Result<Clocks, Error> {
    let mut clocks = config.frequencies()?;
    let r = pac::SYSCTRL;
    let old_sources = r.cr1().read();
    // Keep the Stage41 no-write refusal for any undeclared inherited HSE,
    // including HSE retained only for RTC/AWT while SYSCLK is another source.
    if old_sources.hseen() && config.hse.is_none() {
        return Err(Error::InheritedHseEnabled);
    }
    let old_clock = r.cr0().read();
    let old_hsi = r.hsi().read();
    let old_hse = r.hse().read();
    let old_lsi = r.lsi().read();
    let old_lse = r.lse().read();
    let monitor_hse = config.hse.is_some() || old_sources.hseen() || old_sources.hseccs();
    let monitor_lse = old_sources.lseen() || old_sources.lseccs();
    #[cfg(rcc_lse)]
    let monitor_lse = monitor_lse || config.lse.is_some();
    check_external_faults(monitor_hse, monitor_lse)?;
    // Frequency legality is an entry precondition; STABLE only proves startup.
    // Every HSI divider encoding is legal on this own register version.
    let entry_ready = match old_clock.sysclk() {
        ClockSource::Hsi => old_sources.hsien() && old_hsi.stable(),
        ClockSource::Hse => old_sources.hseen() && old_hse.stable(),
        ClockSource::Lsi => old_lsi.stable(),
        ClockSource::Lse => old_sources.lseen() && old_lse.stable(),
        _ => return Err(Error::InvalidClockSource),
    };
    if !entry_ready {
        return Err(Error::InvalidEntryClock);
    }
    if old_sources.hseen() && !old_hse.stable() {
        return Err(Error::InvalidEntryClock);
    }
    if old_sources.hseen()
        && (old_hse.driver().to_bits() > HseDrive::Level7.to_bits()
            || old_hse.pdriver().to_bits() > HseDrive::Level7.to_bits())
    {
        return Err(Error::InvalidHseConfiguration);
    }
    let factory =
        unsafe { core::ptr::read_volatile(crate::RCC_FACTORY_HSI_TRIM_ADDRESS as *const u16) };
    if factory == u16::MAX {
        return Err(Error::InvalidCalibration);
    }
    let mut calibrated = pac::sysctrl::regs::Hsi::default();
    calibrated.set_trim(factory);
    let trim = calibrated.trim();
    let needs_trim = old_hsi.trim() != trim;

    // Native central rejection precedes the first configuration-gate write.
    // This phase inspects only configuration domains, never GPIO/timer work.
    #[cfg(rcc_lse)]
    let lse_admission = config
        .lse
        .map(|c| super::lse::preflight(c, cs))
        .transpose()?;

    // Inspect actual central gates, restore their incoming state, never reset.
    // SOURCE owns RTC/AWT's raw clock even when the calendar START bit is zero.
    let (rtc_source, rtc_first) =
        <crate::peripherals::RTC as crate::rcc::SealedRccPeripheral>::RCC_INFO
            .inspect_for_init(cs, config.timeout, || {
                (
                    pac::RTC.cr1().read().source(),
                    u32::from(pac::RTC.psc().read().psc1()) + 1,
                )
            })
            .map_err(|_| Error::RetainedClockInspectionTimeout)?;
    if rtc_source > 3 {
        return Err(Error::RetainedRtcConfiguration);
    }
    if rtc_source == 3 && (needs_trim || !old_sources.hsien() || !old_hsi.stable()) {
        return Err(Error::RtcClockInUse);
    }
    // Any enabled incoming HSE is readback-only reuse, whether RTC selects it
    // or not. No inferred absence of owners authorizes stopping or retuning it.
    let preserve_hse = old_sources.hseen();
    if rtc_source == 1 && !preserve_hse {
        return Err(Error::HseClockInUse);
    }
    if let Some(hse) = config.hse {
        if preserve_hse {
            if !hse_parameters_match(hse)?
                || !crate::rcc_hse_pins_match(hse.mode == HseMode::Bypass, config.timeout, cs)?
            {
                return Err(Error::HseClockInUse);
            }
        } else if old_hse.stable() {
            return Err(Error::InvalidEntryClock);
        }
    }
    // PSC1 also feeds AWT through RTCCLKD; START=0 never releases the source.
    // Its actual input/divisor must satisfy the own <=1 MHz requirement.
    let rtc_upper = match rtc_source {
        1 => config.hse.ok_or(Error::HseClockInUse)?.max_freq.0,
        3 => crate::rcc::ClockBounds::hsi(1).maximum().0,
        _ => 0, // Unchanged legal LSE/LSI are already below this ceiling.
    };
    if u64::from(rtc_upper) > 1_000_000 * u64::from(rtc_first) {
        return Err(Error::RetainedRtcConfiguration);
    }
    // Both native ADC instances share the ADC gate; BGR/TS state is not reset.
    let adc_enabled = <crate::peripherals::ADC1 as crate::rcc::SealedRccPeripheral>::RCC_INFO
        .inspect_for_init(cs, config.timeout, || {
            let adc1 = pac::ADC1.cr().read().en();
            let adc2 = pac::ADC2.cr().read().en();
            adc1 || adc2
        })
        .map_err(|_| Error::RetainedClockInspectionTimeout)?;
    if adc_enabled {
        return Err(Error::AdcClockInUse);
    }
    // L012 LVD has no RCC configuration gate. Do not borrow another IP's VC gate.
    let lvd0 = pac::LVD.cr0().read();
    let lvd1 = pac::LVD.cr1().read();
    let lvd_filtered = lvd0.en() && lvd1.flttime() != 0;
    if lvd_filtered && lvd0.fltclk() {
        return Err(Error::LvdClockInUse);
    }
    // All four comparators share VC's gate. Every CR2 bit is a blank-trigger
    // enable, and blank duration uses PCLK independently of filter selection.
    let comparators = <crate::peripherals::VC1 as crate::rcc::SealedRccPeripheral>::RCC_INFO
        .inspect_for_init(cs, config.timeout, || {
            [pac::VC1, pac::VC2, pac::VC3, pac::VC4]
                .map(|vc| (vc.cr0().read(), vc.cr1().read(), vc.cr2().read()))
        })
        .map_err(|_| Error::RetainedClockInspectionTimeout)?;
    let mut vc_lsi = false;
    for (control, filter, blank) in comparators {
        if control.en() {
            if blank.0 != 0
                || (filter.flttime() != 0 && filter.fltclk() == pac::vc::vals::FilterClock::Pclk)
            {
                return Err(Error::ComparatorClockInUse);
            }
            vc_lsi |= filter.flttime() != 0;
        }
    }
    // OPA static amplification is preserved; calibration uses PCLK even when
    // AZRUN is low, so enabled/armed calibration is independently refused.
    let calibrations = <crate::peripherals::OPA1 as crate::rcc::SealedRccPeripheral>::RCC_INFO
        .inspect_for_init(cs, config.timeout, || {
            [pac::OPA1.cal().read(), pac::OPA2.cal().read()]
        })
        .map_err(|_| Error::RetainedClockInspectionTimeout)?;
    if calibrations
        .iter()
        .any(|cal| cal.calen() || cal.azrun() || cal.start())
    {
        return Err(Error::OpaClockInUse);
    }
    let dac = <crate::peripherals::DAC as crate::rcc::SealedRccPeripheral>::RCC_INFO
        .inspect_for_init(cs, config.timeout, || pac::DAC.cr0().read())
        .map_err(|_| Error::RetainedClockInspectionTimeout)?;
    if (dac.en1() && (dac.ten1() || dac.dmaen1() || dac.wave1() != pac::dac::vals::Wave::Disabled))
        || (dac.en2()
            && (dac.ten2() || dac.dmaen2() || dac.wave2() != pac::dac::vals::Wave::Disabled))
    {
        return Err(Error::DacClockInUse);
    }
    // The own package table establishes no ADC/VC/OPA overlap with PF0/PF1.
    // Digital AF owners, DMA and asynchronous writers must be quiescent on entry.
    let needs_lsi = old_sources.hseccs() || old_sources.lseccs() || lvd_filtered || vc_lsi;
    let ccs_unchanged = |after_lse: bool| {
        #[cfg(rcc_lse)]
        let expected_lse = if after_lse { config.lse } else { None };
        #[cfg(rcc_lse)]
        let (lseen, lseccs) = expected_lse
            .map_or((old_sources.lseen(), old_sources.lseccs()), |c| {
                (true, c.monitored())
            });
        #[cfg(not(rcc_lse))]
        let (lseen, lseccs) = {
            let _ = after_lse;
            (old_sources.lseen(), old_sources.lseccs())
        };
        let v = r.cr1().read();
        v.clkccs() == old_sources.clkccs()
            && v.hseccs() == old_sources.hseccs()
            && v.lseccs() == lseccs
            && v.lselock() == old_sources.lselock()
            && v.lseen() == lseen
    };
    // The source-owned legal incoming range is a precondition, not a TRIM
    // measurement. Preserve larger dividers and prove the transition guard.
    let guard_hclk = old_clock.hclkprs().max(AHBPrescaler::Div8 as u8);
    let guard_pclk = old_clock.pclkprs().max(APBPrescaler::Div8 as u8);
    let bus_max = if config.operating_conditions.min_supply_mv < crate::RCC_LOW_VOLTAGE_THRESHOLD_MV
    {
        crate::RCC_LOW_VOLTAGE_BUS_MAX_HZ
    } else {
        crate::RCC_HIGH_VOLTAGE_BUS_MAX_HZ
    };
    if crate::RCC_HSI_OPERATING_RANGE_HZ
        .1
        .div_ceil(1 << guard_hclk)
        > bus_max
        || crate::RCC_HSI_OPERATING_RANGE_HZ
            .1
            .div_ceil(1 << (guard_hclk + guard_pclk))
            > bus_max
    {
        return Err(Error::InvalidEntryClock);
    }
    <crate::peripherals::FLASH as crate::rcc::SealedRccPeripheral>::RCC_INFO
        .enable_with_cs_readback(
            cs,
            crate::rcc::Readback::Poll {
                attempts: config.timeout,
                spin: true,
            },
        )
        .map_err(|_| Error::FlashClockTimeout)?;
    if u32::from(pac::FLASH.cr2().read().wait()) > MAX_FLASH_WAIT {
        return Err(Error::InvalidEntryClock);
    }
    set_flash_latency(MAX_FLASH_WAIT, config.timeout, monitor_hse, monitor_lse)?;
    // Enable unchanged HSI before selecting it (own RM section4.5.1). Its
    // incoming TRIM must already yield the legal HSIOSC range whenever enabled,
    // including when HSI was initially disabled. This is not measured here.
    r.cr1().modify(|w| {
        w.set_key(0x5a5a);
        w.set_hsien(true);
    });
    poll_clock(
        config.timeout,
        Error::HsiTimeout,
        monitor_hse,
        monitor_lse,
        || r.cr1().read().hsien() && r.hsi().read().stable(),
    )?;
    // All three fields share keyed CR0 (own section4.7.1). Select HSI and
    // install the guarded buses in one write, so an intervening external
    // fallback cannot be overwritten by an inherited external selector.
    // Flash is already WAIT3; /8 covers the source-owned legal HSIOSC range.
    r.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_hclkprs(guard_hclk);
        w.set_pclkprs(guard_pclk);
        w.set_sysclk(ClockSource::Hsi);
    });
    poll_clock(
        config.timeout,
        Error::ClockConfigurationTimeout,
        monitor_hse,
        monitor_lse,
        || {
            let v = r.cr0().read();
            v.sysclk() == ClockSource::Hsi
                && v.hclkprs() == guard_hclk
                && v.pclkprs() == guard_pclk
                && r.cr1().read().hsien()
                && r.hsi().read().stable()
        },
    )?;
    barrier();
    if needs_trim || needs_lsi {
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_lsien(true);
        });
        poll_clock(
            config.timeout,
            Error::LsiTimeout,
            monitor_hse,
            monitor_lse,
            || r.cr1().read().lsien() && r.lsi().read().stable(),
        )?;
    }
    if needs_trim {
        // Automatic switching must not select HSI while stopped for retrim.
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_clkccs(false);
        });
        poll(
            || !r.cr1().read().clkccs(),
            config.timeout,
            Error::ClockConfigurationTimeout,
            monitor_hse,
            monitor_lse,
        )?;
        r.cr0().modify(|w| {
            w.set_key(0x5a5a);
            w.set_sysclk(ClockSource::Lsi);
        });
        poll_clock(
            config.timeout,
            Error::TemporaryClockSwitchTimeout,
            monitor_hse,
            monitor_lse,
            || r.cr0().read().sysclk() == ClockSource::Lsi,
        )?;
        barrier();
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_hsien(false);
        });
        poll_clock(
            config.timeout,
            Error::HsiStopTimeout,
            monitor_hse,
            monitor_lse,
            || !r.cr1().read().hsien() && !r.hsi().read().stable(),
        )?;
        r.hsi().modify(|w| {
            w.set_trim(trim);
            w.set_div(config.hsi.div as u8);
        });
        poll(
            || {
                let v = r.hsi().read();
                v.trim() == trim && v.div() == config.hsi.div as u8
            },
            config.timeout,
            Error::ClockConfigurationTimeout,
            monitor_hse,
            monitor_lse,
        )?;
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_hsien(true);
        });
        poll_clock(
            config.timeout,
            Error::HsiTimeout,
            monitor_hse,
            monitor_lse,
            || r.cr1().read().hsien() && r.hsi().read().stable(),
        )?;
        r.cr0().modify(|w| {
            w.set_key(0x5a5a);
            w.set_sysclk(ClockSource::Hsi);
        });
        poll_clock(
            config.timeout,
            Error::ClockSwitchTimeout,
            monitor_hse,
            monitor_lse,
            || r.cr0().read().sysclk() == ClockSource::Hsi,
        )?;
        barrier();
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_clkccs(old_sources.clkccs());
        });
    } else {
        // Own §4.5.2 allows live divider changes, without changing TRIM.
        r.hsi().modify(|w| w.set_div(config.hsi.div as u8));
    }
    poll_clock(
        config.timeout,
        Error::ClockConfigurationTimeout,
        monitor_hse,
        monitor_lse,
        || {
            let v = r.hsi().read();
            v.trim() == trim
                && v.div() == config.hsi.div as u8
                && v.stable()
                && r.cr1().read().hsien()
                && ccs_unchanged(false)
                && r.cr0().read().sysclk() == ClockSource::Hsi
        },
    )?;
    if let Some(hse) = config.hse.filter(|_| !preserve_hse) {
        // Fresh start only: both stopped indicators must still be clear before
        // any pad/configuration write. Never stop an inherited enabled HSE.
        if r.cr1().read().hseen() || r.hse().read().stable() {
            return Err(Error::HseClockInUse);
        }
        crate::rcc_configure_hse_pins(hse.mode == HseMode::Bypass, config.timeout, cs)?;
        let detector = hse.detector_count()?;
        r.hse().modify(|w| {
            w.set_mode(hse.mode == HseMode::Bypass);
            w.set_driver(hse.drive);
            w.set_pdriver(hse.drive);
            w.set_waitcycle(pac::sysctrl::vals::HseWait::Cycles262144);
            w.set_digflt(false);
            w.set_detcnt(detector);
        });
        poll(
            || hse_parameters_match(hse).unwrap_or(false),
            config.timeout,
            Error::ClockConfigurationTimeout,
            monitor_hse,
            monitor_lse,
        )?;
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_hseen(true);
        });
        poll_clock(
            config.timeout,
            Error::HseTimeout,
            monitor_hse,
            monitor_lse,
            || r.cr1().read().hseen() && r.hse().read().stable(),
        )?;
    }
    // Restore only the software request; automatic LSI consumers stay active.
    // Never require STABLE to clear when LSIEN is cleared.
    r.cr1().modify(|w| {
        w.set_key(0x5a5a);
        w.set_lsien(old_sources.lsien());
    });
    poll(
        || r.cr1().read().lsien() == old_sources.lsien(),
        config.timeout,
        Error::TemporaryClockRestoreTimeout,
        monitor_hse,
        monitor_lse,
    )?;
    #[cfg(rcc_lse)]
    if let (Some(lse), Some(admission)) = (config.lse, lse_admission) {
        let admission = admission.after_owned_flash_wait(MAX_FLASH_WAIT)?;
        super::lse::start(lse, admission, cs)?;
    }
    let sysclk = match config.sys {
        Sysclk::HSI => ClockSource::Hsi,
        Sysclk::HSE => ClockSource::Hse,
    };
    // Install final prescalers while verified HSI is selected. Requested HSI,
    // requested HSE and fixed CCS fallback are all qualified at these divisors.
    // An external fallback therefore cannot race this CR0 RMW into reselecting
    // stale HSE. Flash remains at its maximum documented WAIT throughout.
    if r.cr0().read().sysclk() != ClockSource::Hsi {
        return Err(Error::ClockSwitchTimeout);
    }
    r.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_sysclk(ClockSource::Hsi);
        w.set_hclkprs(config.ahb_pre as u8);
        w.set_pclkprs(config.apb_pre as u8);
    });
    poll_clock(
        config.timeout,
        Error::ClockConfigurationTimeout,
        monitor_hse,
        monitor_lse,
        || {
            let v = r.cr0().read();
            v.sysclk() == ClockSource::Hsi
                && v.hclkprs() == config.ahb_pre as u8
                && v.pclkprs() == config.apb_pre as u8
        },
    )?;
    barrier();
    r.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_sysclk(sysclk);
    });
    poll_clock(
        config.timeout,
        Error::ClockSwitchTimeout,
        monitor_hse,
        monitor_lse,
        || r.cr0().read().sysclk() == sysclk,
    )?;
    barrier();
    let upper_hclk = clocks
        .hclk_bounds()
        .maximum()
        .0
        .max(
            crate::rcc::ClockBounds::hsi(config.hsi.div.divisor())
                .divided_by(config.ahb_pre.divisor())
                .maximum()
                .0,
        )
        .max(
            crate::rcc::ClockBounds::hsi(crate::RCC_FIXED_CCS_HSI_DIVISOR)
                .divided_by(config.ahb_pre.divisor())
                .maximum()
                .0,
        );
    set_flash_latency(
        (upper_hclk - 1) / crate::RCC_FLASH_WAIT_STEP_HZ,
        config.timeout,
        monitor_hse,
        monitor_lse,
    )?;
    check_external_faults(monitor_hse, monitor_lse)?;
    let final_clock = r.cr0().read();
    let final_sources = r.cr1().read();
    let final_hsi = r.hsi().read();
    let final_hse = r.hse().read();
    let final_lsi = r.lsi().read();
    let lse_preserved = r.lse().read().0 == old_lse.0;
    #[cfg(rcc_lse)]
    let lse_preserved = if config.lse.is_some() {
        super::lse::preserved_register(old_lse.0)
    } else {
        lse_preserved
    };
    if final_clock.sysclk() != sysclk
        || !ccs_unchanged(true)
        || final_clock.hclkprs() != config.ahb_pre as u8
        || final_clock.pclkprs() != config.apb_pre as u8
        || !final_sources.hsien()
        || !final_hsi.stable()
        || final_hsi.trim() != trim
        || final_hsi.div() != config.hsi.div as u8
        || final_sources.hseen() != (config.hse.is_some() || old_sources.hseen())
        || final_sources.lsien() != old_sources.lsien()
        || final_lsi.trim() != old_lsi.trim()
        || final_lsi.waitcycle() != old_lsi.waitcycle()
        || ((needs_lsi || old_lsi.stable()) && !final_lsi.stable())
        || !lse_preserved
        || final_hse.hexenpol() != old_hse.hexenpol()
    {
        return Err(Error::ClockConfigurationTimeout);
    }
    if let Some(hse) = config.hse {
        if !final_hse.stable()
            || !hse_parameters_match(hse)?
            || !crate::rcc_hse_pins_match(hse.mode == HseMode::Bypass, config.timeout, cs)?
        {
            return Err(Error::ClockConfigurationTimeout);
        }
    } else if final_hse.0 != old_hse.0 {
        return Err(Error::ClockConfigurationTimeout);
    }
    // Union inherited and requested pad ownership for the complete boot.
    // Current exact-reuse policy never changes the inherited oscillator mode.
    if old_sources.hseen() {
        if !old_hse.mode() {
            clocks.hse = Some(HseMode::Oscillator);
        } else if clocks.hse.is_none() {
            clocks.hse = Some(HseMode::Bypass);
        }
    }
    #[cfg(rcc_lse)]
    if let Some(lse) = config.lse {
        super::lse::verify(lse, cs)?;
    }
    // Pad inspection may have opened/restored a gate; recheck fault/mux at freeze.
    check_external_faults(monitor_hse, monitor_lse)?;
    if r.cr0().read().sysclk() != sysclk {
        return Err(Error::ClockSwitchTimeout);
    }
    Ok(clocks)
}

fn check_external_faults(hse: bool, lse: bool) -> Result<(), Error> {
    let v = pac::SYSCTRL.isr().read();
    if (hse && (v.hsefail() || v.hsefault())) || (lse && (v.lsefail() || v.lsefault())) {
        Err(Error::ExternalClockFault)
    } else {
        Ok(())
    }
}

fn poll_clock(
    timeout: u32,
    error: Error,
    hse: bool,
    lse: bool,
    mut ready: impl FnMut() -> bool,
) -> Result<(), Error> {
    for _ in 0..timeout {
        check_external_faults(hse, lse)?;
        if ready() {
            return Ok(());
        }
        core::hint::spin_loop();
    }
    Err(error)
}

fn hse_parameters_match(hse: Hse) -> Result<bool, Error> {
    let v = pac::SYSCTRL.hse().read();
    Ok(v.mode() == (hse.mode == HseMode::Bypass)
        && v.driver() == hse.drive
        && v.pdriver() == hse.drive
        && v.waitcycle() == pac::sysctrl::vals::HseWait::Cycles262144
        && !v.digflt()
        && v.detcnt() == hse.detector_count()?)
}
