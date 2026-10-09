//! CW32L010/CW32L011 one-time factory-HSI and qualified direct-HSE clocks.
//!
//! Own source qualification, electrical limits and retained-owner rules:
//! docs/qualified-l010-l011-hse.md. The incoming source, buses, Flash latency,
//! HSI trim and unchanged LSI must already be legal for the declared board.
//! The initializer cannot repair an illegally clocked bootloader retroactively.
//! Other bus-dependent peripherals, DMA and application interrupt handlers must
//! be quiescent. ADC and SYSCLK/PCLK-filtered analog users are explicitly checked.
//! No oscillator frequency or board condition is measured. External loss can
//! halt execution when fallback is disabled; a fallback invalidates frozen
//! external rates. A failed init publishes no clocks and requires reset before
//! retry. PLL, runtime switching, sleep/resume and recovery are outside this API.

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
    /// Divide by 11 (L010 only).
    #[cfg(cw32l010)]
    Div11 = 11,
    /// Divide by 12.
    #[cfg(cw32l010)]
    Div12 = 12,
    /// Divide by 12.
    #[cfg(cw32l011)]
    Div12 = 11,
    /// Divide by 13 (L010 only).
    #[cfg(cw32l010)]
    Div13 = 13,
    /// Divide by 14 (L010 only).
    #[cfg(cw32l010)]
    Div14 = 14,
    /// Divide by 15 (L010 only).
    #[cfg(cw32l010)]
    Div15 = 15,
    /// Divide by 16.
    #[cfg(cw32l010)]
    Div16 = 0,
    /// Divide by 16.
    #[cfg(cw32l011)]
    Div16 = 12,
    /// Divide by 20 (L011 only).
    #[cfg(cw32l011)]
    Div20 = 13,
    /// Divide by 24 (L011 only).
    #[cfg(cw32l011)]
    Div24 = 14,
    /// Divide by 28 (L011 only).
    #[cfg(cw32l011)]
    Div28 = 15,
    /// Divide by 32 (L011 only).
    #[cfg(cw32l011)]
    Div32 = 0,
}
impl HsiDiv {
    /// Numeric division factor (not the register encoding).
    pub const fn divisor(self) -> u32 {
        #[cfg(cw32l010)]
        {
            if self as u8 == 0 { 16 } else { self as u32 }
        }
        #[cfg(cw32l011)]
        {
            [32, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 12, 16, 20, 24, 28][self as usize]
        }
    }
}

const DEFAULT_DIV: HsiDiv = crate::RCC_DEFAULT_HSI_DIV;
const MAX_FLASH_WAIT: u32 = crate::RCC_INITIAL_FLASH_WAIT;

/// HSI configuration. HSI is always enabled in the supported clock tree.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Hsi {
    /// Divider applied to the factory-calibrated family HSIOSC clock (48/96 MHz).
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

/// Divider from HCLK to PCLK. L010/L011 have one PCLK domain.
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
    /// Reset-equivalent nominal 4 MHz clock tree.
    pub const fn new() -> Self {
        Self {
            operating_conditions: crate::rcc::OperatingConditions::new(),
            hsi: Hsi { div: DEFAULT_DIV },
            hse: None,
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
        // The own-source fixed CCS divisor is independent of the startup default.
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
    InvalidHseBounds,
    HseOutsideQualifiedRange,
    HseConditionsOutsideQualifiedRange,
    HseConditionsDoNotCoverBoard,
    HseNotConfigured,
    HsePinsUnavailable,
    InvalidHseDetector,
    InvalidHseConfiguration,
    HseClockInUse,
    HsePinInUse,
    HsePinConfigurationTimeout,
    HseStopTimeout,
    HseTimeout,
    ExternalClockFault,
    InvalidEntryClock,
    RetainedClockInspectionTimeout,
    RetainedRtcConfiguration,
    AdcClockInUse,
    LvdClockInUse,
    ComparatorClockInUse,
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
fn poll(mut ready: impl FnMut() -> bool, timeout: u32, error: Error) -> Result<(), Error> {
    for _ in 0..timeout {
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

fn set_flash_latency(wait: u32, timeout: u32) -> Result<(), Error> {
    // Bits 15:3 are reserved on this register version; preserve them, never
    // apply the F030 FETCH/CACHE bits. Only WAIT is programmable here.
    pac::FLASH.cr2().modify(|w| {
        w.set_key(0x5a5a);
        w.set_wait(wait as u8);
    });
    poll(
        || pac::FLASH.cr2().read().wait() == wait as u8,
        timeout,
        Error::FlashLatencyTimeout,
    )?;
    barrier();
    Ok(())
}

fn configure(config: Config, cs: critical_section::CriticalSection<'_>) -> Result<Clocks, Error> {
    let mut clocks = config.frequencies()?;
    let r = pac::SYSCTRL;
    let old_clock = r.cr0().read();
    let old_sources = r.cr1().read();
    let old_hsi = r.hsi().read();
    let old_hse = r.hse().read();
    let old_lsi = r.lsi().read();
    let old_lse = r.lse().read();
    let monitor_hse = config.hse.is_some() || old_sources.hseen() || old_sources.hseccs();
    let monitor_lse = old_sources.lseen() || old_sources.lseccs();
    check_external_faults(monitor_hse, monitor_lse)?;
    // Frequency legality is an entry precondition; STABLE only proves startup.
    // Every HSI divider encoding is legal on these own register versions.
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

    // Inspect actual central gates, restore their incoming state, never reset.
    // SOURCE owns RTC/AWT's raw clock even when the calendar START bit is zero.
    let rtc_source = <crate::peripherals::RTC as crate::rcc::SealedRccPeripheral>::RCC_INFO
        .inspect_for_init(cs, config.timeout, || pac::RTC.cr1().read().source())
        .map_err(|_| Error::RetainedClockInspectionTimeout)?;
    if rtc_source > 3 {
        return Err(Error::RetainedRtcConfiguration);
    }
    if rtc_source == 3 && (needs_trim || !old_sources.hsien() || !old_hsi.stable()) {
        return Err(Error::RtcClockInUse);
    }
    let preserve_hse = rtc_source == 1;
    if preserve_hse
        && (!old_sources.hseen()
            || !old_hse.stable()
            || !crate::rcc_hse_pins_match(old_hse.mode(), config.timeout, cs)?)
    {
        return Err(Error::HseClockInUse);
    }
    if let Some(hse) = config.hse {
        if preserve_hse
            && (!hse_parameters_match(hse)?
                || !crate::rcc_hse_pins_match(hse.mode == HseMode::Bypass, config.timeout, cs)?)
        {
            return Err(Error::HseClockInUse);
        }
    }
    let adc_enabled = <crate::peripherals::ADC as crate::rcc::SealedRccPeripheral>::RCC_INFO
        .inspect_for_init(cs, config.timeout, || pac::ADC.cr().read().en())
        .map_err(|_| Error::RetainedClockInspectionTimeout)?;
    if adc_enabled {
        return Err(Error::AdcClockInUse);
    }
    // LVD and both comparators share the actual VC configuration gate. BGR
    // lives in ADC; none of these reads changes BGR, TSEN or reference selection.
    let (lvd0, lvd1, vc10, vc11, vc20, vc21) =
        <crate::peripherals::VC1 as crate::rcc::SealedRccPeripheral>::RCC_INFO
            .inspect_for_init(cs, config.timeout, || {
                (
                    pac::LVD.cr0().read(),
                    pac::LVD.cr1().read(),
                    pac::VC1.cr0().read(),
                    pac::VC1.cr1().read(),
                    pac::VC2.cr0().read(),
                    pac::VC2.cr1().read(),
                )
            })
            .map_err(|_| Error::RetainedClockInspectionTimeout)?;
    if let Some(hse) = config.hse.filter(|_| !preserve_hse) {
        // Stopping an inherited crystal also disturbs OSC_OUT, even when the
        // requested bypass setup only writes OSC_IN. Readback-only reuse is safe.
        let owns_output =
            hse.mode == HseMode::Oscillator || (old_sources.hseen() && !old_hse.mode());
        let intersects = |masks: [u32; 2], mux: u8| {
            (masks[0] | if owns_output { masks[1] } else { 0 }) & (1 << mux) != 0
        };
        if (vc10.en()
            && (intersects(crate::RCC_HSE_VC1_INP_MASKS, vc10.inp())
                || intersects(crate::RCC_HSE_VC1_INN_MASKS, vc10.inn())))
            || (vc20.en()
                && (intersects(crate::RCC_HSE_VC2_INP_MASKS, vc20.inp())
                    || intersects(crate::RCC_HSE_VC2_INN_MASKS, vc20.inn())))
        {
            return Err(Error::HsePinInUse);
        }
    }
    let lvd_filtered = lvd0.en() && lvd1.flttime() != 0;
    if lvd_filtered && lvd0.fltclk() {
        return Err(Error::LvdClockInUse);
    }
    // Blanking windows also use PCLK independently of filter-clock selection.
    if (vc10.en()
        && (vc11.blankatch1()
            || vc11.blankatch2()
            || vc11.blankatch3()
            || vc11.blankatch4()
            || vc11.blankatch5()
            || vc11.blankatch6()))
        || (vc20.en()
            && (vc21.blankatch1()
                || vc21.blankatch2()
                || vc21.blankatch3()
                || vc21.blankatch4()
                || vc21.blankatch5()
                || vc21.blankatch6()))
    {
        return Err(Error::ComparatorClockInUse);
    }
    let vc1_filtered = vc10.en() && vc11.flttime() != 0;
    let vc2_filtered = vc20.en() && vc21.flttime() != 0;
    if (vc1_filtered && vc11.fltclk() == pac::vc::vals::FilterClock::Pclk)
        || (vc2_filtered && vc21.fltclk() == pac::vc::vals::FilterClock::Pclk)
    {
        return Err(Error::ComparatorClockInUse);
    }
    let needs_lsi = old_sources.hseccs()
        || old_sources.lseccs()
        || lvd_filtered
        || vc1_filtered
        || vc2_filtered;
    let ccs_unchanged = || {
        let v = r.cr1().read();
        v.clkccs() == old_sources.clkccs()
            && v.hseccs() == old_sources.hseccs()
            && v.lseccs() == old_sources.lseccs()
            && v.lselock() == old_sources.lselock()
            && v.lseen() == old_sources.lseen()
    };
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
    set_flash_latency(MAX_FLASH_WAIT, config.timeout)?;
    // The unchanged legal inherited HSIOSC may not yet have factory trim.
    // /8 covers the own-family legal oscillator range even below 1.8 V.
    let guard_hclk = old_clock.hclkprs().max(AHBPrescaler::Div8 as u8);
    let guard_pclk = old_clock.pclkprs().max(APBPrescaler::Div8 as u8);
    r.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_hclkprs(guard_hclk);
        w.set_pclkprs(guard_pclk);
    });
    poll(
        || {
            let v = r.cr0().read();
            v.hclkprs() == guard_hclk && v.pclkprs() == guard_pclk
        },
        config.timeout,
        Error::ClockConfigurationTimeout,
    )?;
    barrier();

    // First leave the incoming source on unchanged HSI under guarded buses.
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
        // Both own §4.5.2 allow live divider changes, without changing TRIM.
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
                && ccs_unchanged()
                && r.cr0().read().sysclk() == ClockSource::Hsi
        },
    )?;
    if let Some(hse) = config.hse.filter(|_| !preserve_hse) {
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_hseen(false);
        });
        poll_clock(
            config.timeout,
            Error::HseStopTimeout,
            monitor_hse,
            monitor_lse,
            || !r.cr1().read().hseen() && !r.hse().read().stable(),
        )?;
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
    )?;
    let sysclk = match config.sys {
        Sysclk::HSI => ClockSource::Hsi,
        Sysclk::HSE => ClockSource::Hse,
    };
    // Keep guarded buses until the target source has acknowledged selection.
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
    r.cr0().modify(|w| {
        w.set_key(0x5a5a);
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
            v.sysclk() == sysclk
                && v.hclkprs() == config.ahb_pre as u8
                && v.pclkprs() == config.apb_pre as u8
        },
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
    )?;
    check_external_faults(monitor_hse, monitor_lse)?;
    let final_clock = r.cr0().read();
    let final_sources = r.cr1().read();
    let final_hsi = r.hsi().read();
    let final_hse = r.hse().read();
    let final_lsi = r.lsi().read();
    if final_clock.sysclk() != sysclk
        || !ccs_unchanged()
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
        || r.lse().read().0 != old_lse.0
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
    // The inherited enabled crystal reserves both pads for the whole boot,
    // including after an unowned oscillator is deliberately changed to bypass.
    if old_sources.hseen() {
        if !old_hse.mode() {
            clocks.hse = Some(HseMode::Oscillator);
        } else if clocks.hse.is_none() {
            clocks.hse = Some(HseMode::Bypass);
        }
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
