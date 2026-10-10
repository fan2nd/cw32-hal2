//! CW32L012 one-time factory-HSI and qualified direct-HSE/LSE clocks.
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
    /// Native board-qualified 32768 Hz source on C8T6/C8U6.
    #[cfg(rcc_lse)]
    LSE,
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
            #[cfg(rcc_lse)]
            Sysclk::LSE => {
                let (config, bounds) = lse.ok_or(Error::LseNotConfigured)?;
                // Consume the independently qualified SYSCLK detector receipt,
                // in addition to native auxiliary source qualification above.
                if config.monitored()
                    && u64::from(config.min_freq.0) * u64::from(crate::RCC_LSE_SYSCLK_LSI_CYCLES)
                        <= (u64::from(crate::RCC_LSE_SYSCLK_LSE_EDGES)
                            + u64::from(crate::RCC_LSE_SYSCLK_MARGIN_LSE_EDGES))
                            * u64::from(crate::RCC_LSE_MONITORED_LSI_MAXIMUM_HZ)
                {
                    return Err(Error::InvalidLseBounds);
                }
                bounds
            }
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
        // LSE target admission takes no bus-divider retention credit for the
        // full effective HSI/24 fallback. Other source contracts stay unchanged.
        let fallback_dividers = clocks.dividers;
        #[cfg(rcc_lse)]
        let fallback_dividers = if self.sys == Sysclk::LSE {
            [self.hsi.div.divisor(), 1, 1]
        } else {
            fallback_dividers
        };
        // Own L012 hardware forces /24 after CCS, independently of the default /12.
        crate::rcc::operating::validate(
            self.operating_conditions,
            Clocks {
                source: crate::rcc::ClockBounds::hsi(crate::RCC_FIXED_CCS_HSI_DIVISOR),
                dividers: fallback_dividers,
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
/// lowers flash latency only after verifying that the active clock, final bus
/// dividers, retained source and qualified fallback are safe at that latency.
/// After a failure, no frequencies are published and peripherals must not be
/// used on the assumption that the requested configuration took effect.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {
    #[cfg(rcc_lse)]
    InvalidLseBounds,
    #[cfg(rcc_lse)]
    LseNotConfigured,
    #[cfg(rcc_lse)]
    LseClockSwitchTimeout,
    #[cfg(rcc_lse)]
    HsiClockInUse,
    #[cfg(rcc_lse)]
    LsiClockInUse,
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
    // Immutable target identity precedes native preflight and every gate write.
    #[cfg(rcc_lse)]
    let lse_target = (config.sys == Sysclk::LSE).then(|| LseSysclkSnapshot {
        clock: old_clock,
        sources: old_sources,
        hsi: old_hsi,
        hse: old_hse,
        lsi: old_lsi,
        lse: old_lse,
        routes: r.cr2().read(),
        interrupts: r.ier().read(),
        mco: r.mco().read(),
    });
    #[cfg(not(rcc_lse))]
    let l012_sysclk = false;
    #[cfg(rcc_lse)]
    let l012_sysclk = lse_target.is_some();
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
    if let Some(entry) = lse_target {
        entry.unchanged_sources(config, false, false)?;
    }
    #[cfg(rcc_lse)]
    let lse_admission = config
        .lse
        .map(|c| super::lse::preflight(c, cs, l012_sysclk))
        .transpose();
    #[cfg(rcc_lse)]
    if let Some(entry) = lse_target {
        if let Err(
            error @ (Error::LseConfigurationGateTimeout { .. } | Error::LseGpioGateTimeout),
        ) = lse_admission
        {
            return Err(error);
        }
        entry.policy(config)?;
    }
    #[cfg(rcc_lse)]
    let lse_admission = lse_admission?;

    // Inspect actual central gates, restore their incoming state, never reset.
    // SOURCE owns RTC/AWT's raw clock even when the calendar START bit is zero.
    let (rtc_source, rtc_first) = inspect_retained(
        config,
        cs,
        monitor_hse,
        <crate::peripherals::RTC as crate::rcc::SealedRccPeripheral>::RCC_INFO,
        || {
            (
                pac::RTC.cr1().read().source(),
                u32::from(pac::RTC.psc().read().psc1()) + 1,
            )
        },
    )?;
    let invalid_rtc_source = rtc_source > 3;
    #[cfg(rcc_lse)]
    let invalid_rtc_source = invalid_rtc_source && lse_target.is_none();
    if invalid_rtc_source {
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
                || !crate::rcc_hse_pins_match(
                    hse.mode == HseMode::Bypass,
                    config.timeout,
                    cs,
                    l012_sysclk,
                )?
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
    // Both ADCs share a configuration-and-work gate. The target's documented
    // functional handover permits conversion/trigger progress during this
    // inspection, including before EN refusal; restoration cannot undo it.
    // The separate platform bus-master/memory-ownership boundary still applies.
    let adc_enabled = inspect_retained(
        config,
        cs,
        monitor_hse,
        <crate::peripherals::ADC1 as crate::rcc::SealedRccPeripheral>::RCC_INFO,
        || {
            let adc1 = pac::ADC1.cr().read().en();
            let adc2 = pac::ADC2.cr().read().en();
            adc1 || adc2
        },
    )?;
    if adc_enabled {
        return Err(Error::AdcClockInUse);
    }
    // L012 LVD has no RCC configuration gate. Do not borrow another IP's VC gate.
    let lvd0 = pac::LVD.cr0().read();
    let lvd1 = pac::LVD.cr1().read();
    #[cfg(rcc_lse)]
    if let Some(entry) = lse_target {
        entry.faults(config)?;
    }
    let lvd_filtered = lvd0.en() && lvd1.flttime() != 0;
    if lvd_filtered && lvd0.fltclk() {
        return Err(Error::LvdClockInUse);
    }
    // All four comparators share VC's gate. Every CR2 bit is a blank-trigger
    // enable, and blank duration uses PCLK independently of filter selection.
    let comparators = inspect_retained(
        config,
        cs,
        monitor_hse,
        <crate::peripherals::VC1 as crate::rcc::SealedRccPeripheral>::RCC_INFO,
        || {
            [pac::VC1, pac::VC2, pac::VC3, pac::VC4]
                .map(|vc| (vc.cr0().read(), vc.cr1().read(), vc.cr2().read()))
        },
    )?;
    let mut vc_lsi = false;
    #[cfg(rcc_lse)]
    let mut target_vc_lsi = false;
    for (control, filter, blank) in comparators {
        if control.en() {
            if blank.0 != 0
                || (filter.flttime() != 0 && filter.fltclk() == pac::vc::vals::FilterClock::Pclk)
            {
                return Err(Error::ComparatorClockInUse);
            }
            vc_lsi |= filter.flttime() != 0;
            #[cfg(rcc_lse)]
            {
                // Native automatic request is independent of filter count.
                target_vc_lsi |= filter.fltclk() == pac::vc::vals::FilterClock::InternalRc;
            }
        }
    }
    // OPA static amplification is preserved; calibration uses PCLK even when
    // AZRUN is low, so enabled/armed calibration is independently refused.
    let calibrations = inspect_retained(
        config,
        cs,
        monitor_hse,
        <crate::peripherals::OPA1 as crate::rcc::SealedRccPeripheral>::RCC_INFO,
        || [pac::OPA1.cal().read(), pac::OPA2.cal().read()],
    )?;
    if calibrations
        .iter()
        .any(|cal| cal.calen() || cal.azrun() || cal.start())
    {
        return Err(Error::OpaClockInUse);
    }
    let dac = inspect_retained(
        config,
        cs,
        monitor_hse,
        <crate::peripherals::DAC as crate::rcc::SealedRccPeripheral>::RCC_INFO,
        || pac::DAC.cr0().read(),
    )?;
    if (dac.en1() && (dac.ten1() || dac.dmaen1() || dac.wave1() != pac::dac::vals::Wave::Disabled))
        || (dac.en2()
            && (dac.ten2() || dac.dmaen2() || dac.wave2() != pac::dac::vals::Wave::Disabled))
    {
        return Err(Error::DacClockInUse);
    }
    // The own package table establishes no ADC/VC/OPA overlap with PF0/PF1.
    // Digital AF owners, DMA and asynchronous writers must be quiescent on entry.
    let needs_lsi = old_sources.hseccs() || old_sources.lseccs() || lvd_filtered || vc_lsi;
    #[cfg(rcc_lse)]
    let needs_lsi = if lse_target.is_some() {
        old_sources.hseccs()
            || old_sources.lseccs()
            || (lvd0.en() && !lvd0.fltclk())
            || target_vc_lsi
    } else {
        needs_lsi
    };
    // Entry readiness, not LSIEN or later STABLE progress, owns this latch.
    #[cfg(rcc_lse)]
    let first_lsi_request = lse_target.is_some() && (needs_trim || needs_lsi) && !old_lsi.stable();
    #[cfg(rcc_lse)]
    if let Some(entry) = lse_target {
        entry.hsi_ownership(config, trim, cs)?;
        if first_lsi_request {
            entry.admit_first_lsi_request(config, cs)?;
        }
        if rtc_source > 3 {
            return Err(Error::RetainedRtcConfiguration);
        }
    }
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
    #[cfg(rcc_lse)]
    if let Some(entry) = lse_target {
        entry.faults(config)?;
        if <crate::peripherals::FLASH as crate::rcc::SealedRccPeripheral>::RCC_INFO.reset_asserted()
        {
            return Err(Error::LseClockInUse);
        }
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
    // Capture authoritative FLASH controls only after its gate acknowledges.
    #[cfg(rcc_lse)]
    let target_flash = lse_target
        .map(|entry| -> Result<_, Error> {
            entry.flash_access(config)?;
            let flash = pac::FLASH.cr2().read();
            entry.flash_access(config)?;
            Ok(flash)
        })
        .transpose()?;
    #[cfg(rcc_lse)]
    if let (Some(entry), Some(flash)) = (lse_target, target_flash) {
        let verify = || {
            entry.unchanged_sources(config, false, false)?;
            entry.clock_matches(old_clock.sysclk(), old_clock.hclkprs(), old_clock.pclkprs())
        };
        verify()?;
        let original_wait = entry.routes.flashwait();
        if u32::from(original_wait) > MAX_FLASH_WAIT {
            return Err(Error::FlashLatencyTimeout);
        }
        entry.require_wait(config, flash, original_wait)?;
        entry.write_wait(config, flash, original_wait, MAX_FLASH_WAIT as u8, verify)?;
    } else {
        if u32::from(pac::FLASH.cr2().read().wait()) > MAX_FLASH_WAIT {
            return Err(Error::InvalidEntryClock);
        }
        set_flash_latency(MAX_FLASH_WAIT, config.timeout, monitor_hse, monitor_lse)?;
    }
    #[cfg(not(rcc_lse))]
    {
        if u32::from(pac::FLASH.cr2().read().wait()) > MAX_FLASH_WAIT {
            return Err(Error::InvalidEntryClock);
        }
        set_flash_latency(MAX_FLASH_WAIT, config.timeout, monitor_hse, monitor_lse)?;
    }
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
        #[cfg(rcc_lse)]
        if let (Some(entry), Some(flash)) = (lse_target, target_flash) {
            if first_lsi_request {
                entry.admit_first_lsi_request(config, cs)?;
            }
            // Mandatory for EVERY needed temporary assertion, even if entry
            // LSI was already stable. Consumer gate restorations finish first;
            // no gate/source write intervenes between this commit check and EN.
            entry.unchanged_sources(config, true, true)?;
            entry.clock_matches(ClockSource::Hsi, guard_hclk, guard_pclk)?;
            entry.require_wait(config, flash, MAX_FLASH_WAIT as u8)?;
        }
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
        crate::rcc_configure_hse_pins(
            hse.mode == HseMode::Bypass,
            config.timeout,
            cs,
            l012_sysclk,
        )?;
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
        if let (Some(entry), Some(flash)) = (lse_target, target_flash) {
            entry.prepared_sources(config, trim, needs_lsi, false)?;
            entry.clock_matches(ClockSource::Hsi, guard_hclk, guard_pclk)?;
            entry.require_wait(config, flash, MAX_FLASH_WAIT as u8)?;
        }
        if let Some(entry) = lse_target {
            entry.flash_access(config)?;
        }
        let admission = admission.after_owned_flash_wait(MAX_FLASH_WAIT);
        if let Some(entry) = lse_target {
            entry.flash_access(config)?;
        }
        let admission = admission?;
        let result = super::lse::start(lse, admission, cs, l012_sysclk);
        if let Some(entry) = lse_target {
            // Gate restoration is resolved first; otherwise both external
            // fault domains win over native semantic/readiness errors.
            if let Err(
                error @ (Error::LseConfigurationGateTimeout { .. } | Error::LseGpioGateTimeout),
            ) = result
            {
                return Err(error);
            }
            entry.policy(config)?;
        }
        result?;
    }
    #[cfg(rcc_lse)]
    if let (Some(entry), Some(flash)) = (lse_target, target_flash) {
        return finish_lse_sysclk(config, clocks, entry, flash, trim, needs_lsi, cs);
    }
    let sysclk = match config.sys {
        Sysclk::HSI => ClockSource::Hsi,
        Sysclk::HSE => ClockSource::Hse,
        #[cfg(rcc_lse)]
        Sysclk::LSE => unreachable!(),
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
            || !crate::rcc_hse_pins_match(
                hse.mode == HseMode::Bypass,
                config.timeout,
                cs,
                l012_sysclk,
            )?
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
        super::lse::verify(lse, cs, false)?;
    }
    // Pad inspection may have opened/restored a gate; recheck fault/mux at freeze.
    check_external_faults(monitor_hse, monitor_lse)?;
    if r.cr0().read().sysclk() != sysclk {
        return Err(Error::ClockSwitchTimeout);
    }
    Ok(clocks)
}

// Retained non-target paths retain the original inspection/error contract.
fn inspect_retained<R>(
    config: Config,
    cs: critical_section::CriticalSection<'_>,
    monitor_hse: bool,
    info: super::peripheral::RccInfo,
    read: impl FnOnce() -> R,
) -> Result<R, Error> {
    #[cfg(rcc_lse)]
    if config.sys == Sysclk::LSE {
        return lse_target_inspect(config, cs, info, monitor_hse, read);
    }
    let _ = monitor_hse;
    info.inspect_for_init(cs, config.timeout, read)
        .map_err(|_| Error::RetainedClockInspectionTimeout)
}

#[cfg(rcc_lse)]
pub(crate) fn lse_target_inspection_error(error: super::peripheral::ClockInspectionError) -> Error {
    match error {
        super::peripheral::ClockInspectionError::EnableFailed { restore_failed } => {
            Error::LseConfigurationGateTimeout {
                enable_failed: true,
                restore_failed,
            }
        }
        super::peripheral::ClockInspectionError::RestoreFailed => {
            Error::LseConfigurationGateTimeout {
                enable_failed: false,
                restore_failed: true,
            }
        }
    }
}

#[cfg(rcc_lse)]
fn lse_target_inspect<R>(
    config: Config,
    cs: critical_section::CriticalSection<'_>,
    info: super::peripheral::RccInfo,
    monitor_hse: bool,
    read: impl FnOnce() -> R,
) -> Result<R, Error> {
    check_external_faults(monitor_hse, true)?;
    if info.reset_asserted() {
        return Err(Error::LseClockInUse);
    }
    // A semantic refusal is held until this gate's independent restoration
    // succeeds. In particular, enable+restore failure preserves both facts.
    let result = info
        .inspect_for_init(cs, config.timeout, || {
            if info.reset_asserted() {
                return None;
            }
            let result = read();
            (!info.reset_asserted()).then_some(result)
        })
        .map_err(lse_target_inspection_error)?;
    check_external_faults(monitor_hse, true)?;
    if info.reset_asserted() {
        return Err(Error::LseClockInUse);
    }
    result.ok_or(Error::LseClockInUse)
}

/// Original target identity, never recaptured after a source or WAIT change.
/// The native opaque Admission retains its separate classification/ownership.
#[cfg(rcc_lse)]
#[derive(Clone, Copy)]
struct LseSysclkSnapshot {
    clock: pac::sysctrl::regs::Cr0,
    sources: pac::sysctrl::regs::Cr1,
    hsi: pac::sysctrl::regs::Hsi,
    hse: pac::sysctrl::regs::Hse,
    lsi: pac::sysctrl::regs::Lsi,
    lse: pac::sysctrl::regs::Lse,
    routes: pac::sysctrl::regs::Cr2,
    interrupts: pac::sysctrl::regs::Ier,
    mco: pac::sysctrl::regs::Mco,
}

#[cfg(rcc_lse)]
impl LseSysclkSnapshot {
    fn monitors_hse(self, config: Config) -> bool {
        config.hse.is_some() || self.sources.hseen() || self.sources.hseccs()
    }

    fn faults(self, config: Config) -> Result<(), Error> {
        check_external_faults(self.monitors_hse(config), true)
    }

    fn policy(self, config: Config) -> Result<(), Error> {
        self.faults(config)?;
        let r = pac::SYSCTRL;
        let mut expected = self.routes;
        let mut current = r.cr2().read();
        expected.set_key(0);
        current.set_key(0);
        expected.set_flashwait(current.flashwait());
        // The WAIT pair is checked separately against exact owned phase values.
        // No other original policy bit is replaceable by a live observation.
        if current.0 != expected.0
            || r.ier().read().0 != self.interrupts.0
            || r.mco().read().0 != self.mco.0
        {
            return Err(Error::LseClockInUse);
        }
        let current = r.lsi().read();
        if current.trim() != self.lsi.trim() || current.waitcycle() != self.lsi.waitcycle() {
            return Err(if config.lse.ok_or(Error::LseNotConfigured)?.monitored() {
                Error::LseMonitorNotReady
            } else {
                Error::LseClockInUse
            });
        }
        if config.lse.ok_or(Error::LseNotConfigured)?.monitored() {
            // Native L012 qualification is the own nine-bit factory tuple.
            // Later readiness cannot manufacture entry monitor admission.
            let factory = unsafe {
                core::ptr::read_volatile(crate::RCC_LSE_LSI_FACTORY_TRIM_ADDRESS as *const u16)
            };
            let mut calibrated = pac::sysctrl::regs::Lsi::default();
            calibrated.set_trim(factory);
            if !self.lsi.stable()
                || !current.stable()
                || factory == u16::MAX
                || self.lsi.trim() != calibrated.trim()
            {
                return Err(Error::LseMonitorNotReady);
            }
        }
        Ok(())
    }

    fn clock_matches(self, source: ClockSource, ahb: u8, apb: u8) -> Result<(), Error> {
        let mut expected = self.clock;
        expected.set_key(0);
        expected.set_sysclk(source);
        expected.set_hclkprs(ahb);
        expected.set_pclkprs(apb);
        let mut current = pac::SYSCTRL.cr0().read();
        current.set_key(0);
        if current.0 != expected.0 {
            return Err(Error::ClockConfigurationTimeout);
        }
        Ok(())
    }

    // Before the trim bridge, only the owned unchanged-HSI request may differ
    // in CR1; every other bit, including CLKCCS and software LSIEN, is original.
    fn unchanged_sources(
        self,
        config: Config,
        hsi_requested: bool,
        hsi_ready: bool,
    ) -> Result<(), Error> {
        self.policy(config)?;
        let r = pac::SYSCTRL;
        let mut expected = self.sources;
        expected.set_key(0);
        if hsi_requested {
            expected.set_hsien(true);
        }
        let mut current = r.cr1().read();
        current.set_key(0);
        if current.0 != expected.0 || r.lse().read().0 != self.lse.0 {
            return Err(Error::LseClockInUse);
        }
        let hsi = r.hsi().read();
        if (hsi.0 ^ self.hsi.0) & !crate::RCC_HSI_STABLE_MASK != 0
            || ((hsi_ready || self.hsi.stable()) && !hsi.stable())
            || r.hse().read().0 != self.hse.0
            || (self.lsi.stable() && !r.lsi().read().stable())
        {
            return Err(Error::ClockConfigurationTimeout);
        }
        Ok(())
    }

    fn hsi_ownership(
        self,
        config: Config,
        trim: u16,
        cs: critical_section::CriticalSection<'_>,
    ) -> Result<(), Error> {
        self.policy(config)?;
        if self.hsi.trim() != trim || !self.sources.hsien() || !self.hsi.stable() {
            if pac::SYSCTRL.mco().read().source() == 3 || pac::SYSCTRL.ier().read().hsirdy() {
                return Err(Error::HsiClockInUse);
            }
            self.i2c_internal_ownership(config, cs, Error::HsiClockInUse)?;
        }
        Ok(())
    }

    fn i2c_internal_ownership(
        self,
        config: Config,
        cs: critical_section::CriticalSection<'_>,
        error: Error,
    ) -> Result<(), Error> {
        use crate::rcc::SealedRccPeripheral;
        for (info, i2c) in [
            (crate::peripherals::I2C1::RCC_INFO, pac::I2C1),
            (crate::peripherals::I2C2::RCC_INFO, pac::I2C2),
        ] {
            let conflict = lse_target_inspect(config, cs, info, self.monitors_hse(config), || {
                // Raw1 is reserved; raw3 conflicts between own master prose
                // and SDK/slave sources. Neither can prove raw-HSI/LSI absence.
                matches!(u8::from(i2c.mcr0().read().clksrc()), 1 | 3)
                    || matches!(u8::from(i2c.scr0().read().clksrc()), 1 | 3)
            })?;
            if conflict {
                return Err(error);
            }
        }
        Ok(())
    }

    fn admit_first_lsi_request(
        self,
        config: Config,
        cs: critical_section::CriticalSection<'_>,
    ) -> Result<(), Error> {
        use crate::rcc::SealedRccPeripheral;
        self.policy(config)?;
        let r = pac::SYSCTRL;
        if r.mco().read().source() == 4 || r.ier().read().lsirdy() {
            return Err(Error::LsiClockInUse);
        }
        let conflict = lse_target_inspect(
            config,
            cs,
            crate::peripherals::RTC::RCC_INFO,
            self.monitors_hse(config),
            || matches!(pac::RTC.cr1().read().source(), 2 | 4..=u8::MAX),
        )?;
        if conflict {
            return Err(Error::LsiClockInUse);
        }
        for (info, uart) in [
            (crate::peripherals::UART1::RCC_INFO, pac::UART1),
            (crate::peripherals::UART2::RCC_INFO, pac::UART2),
        ] {
            if lse_target_inspect(config, cs, info, self.monitors_hse(config), || {
                uart.cr1().read().source() == pac::uart::vals::Source::Lsi
            })? {
                return Err(Error::LsiClockInUse);
            }
        }
        // UART3's work-gate semantics disagree between own sources. Read only
        // an operational instance and never open its closed gate for admission.
        let uart3 = crate::peripherals::UART3::RCC_INFO;
        self.faults(config)?;
        if uart3.reset_asserted() {
            return Err(Error::LseClockInUse);
        }
        if uart3.is_enabled() {
            if uart3.reset_asserted() || !uart3.is_enabled() {
                return Err(Error::LseClockInUse);
            }
            let conflict = pac::UART3.cr1().read().source() == pac::uart::vals::Source::Lsi;
            self.faults(config)?;
            if uart3.reset_asserted() || !uart3.is_enabled() {
                return Err(Error::LseClockInUse);
            }
            if conflict {
                return Err(Error::LsiClockInUse);
            }
        }
        if uart3.reset_asserted() {
            return Err(Error::LseClockInUse);
        }
        self.i2c_internal_ownership(config, cs, Error::LsiClockInUse)?;
        if lse_target_inspect(
            config,
            cs,
            crate::peripherals::LPTIM::RCC_INFO,
            self.monitors_hse(config),
            || {
                pac::LPTIM.cr0().read().en()
                    && pac::LPTIM.cfgr().read().iclksrc() == pac::lptim::vals::Source::Lsi
            },
        )? {
            return Err(Error::LsiClockInUse);
        }
        // Ungated LVD is independent of the VC configuration domain. Native
        // auto-request depends on EN/source, including FLTTIME=0.
        let lvd = pac::LVD.cr0().read();
        self.faults(config)?;
        if lvd.en() && !lvd.fltclk() {
            return Err(Error::LsiClockInUse);
        }
        if lse_target_inspect(
            config,
            cs,
            crate::peripherals::VC1::RCC_INFO,
            self.monitors_hse(config),
            || {
                [pac::VC1, pac::VC2, pac::VC3, pac::VC4].iter().any(|vc| {
                    vc.cr0().read().en()
                        && vc.cr1().read().fltclk() == pac::vc::vals::FilterClock::InternalRc
                })
            },
        )? {
            return Err(Error::LsiClockInUse);
        }
        // No timer/GPIO/output work gate is opened. The documented functional
        // handover covers inaccessible UART3, direct/cascade observers and IWDT.
        self.policy(config)
    }

    fn prepared_sources(
        self,
        config: Config,
        trim: u16,
        needs_lsi: bool,
        after_lse: bool,
    ) -> Result<(), Error> {
        self.policy(config)?;
        let r = pac::SYSCTRL;
        let requested_lse = config.lse.ok_or(Error::LseNotConfigured)?;
        let mut expected = self.sources;
        expected.set_key(0);
        expected.set_hsien(true);
        expected.set_hseen(config.hse.is_some() || self.sources.hseen());
        if after_lse {
            expected.set_lseen(true);
            expected.set_lseccs(requested_lse.monitored());
        }
        let mut current = r.cr1().read();
        current.set_key(0);
        if current.0 != expected.0 {
            return Err(Error::LseClockInUse);
        }
        let mut expected_hsi = self.hsi;
        expected_hsi.set_trim(trim);
        expected_hsi.set_div(config.hsi.div as u8);
        let hsi = r.hsi().read();
        if (hsi.0 ^ expected_hsi.0) & !crate::RCC_HSI_STABLE_MASK != 0
            || !hsi.stable()
            || ((needs_lsi || self.lsi.stable()) && !r.lsi().read().stable())
        {
            return Err(Error::ClockConfigurationTimeout);
        }
        let mut expected_hse = self.hse;
        if let Some(hse) = config.hse {
            if !self.sources.hseen() {
                expected_hse.set_mode(hse.mode == HseMode::Bypass);
                expected_hse.set_driver(hse.drive);
                expected_hse.set_pdriver(hse.drive);
                expected_hse.set_waitcycle(pac::sysctrl::vals::HseWait::Cycles262144);
                expected_hse.set_digflt(false);
                expected_hse.set_detcnt(hse.detector_count()?);
            }
            if !hse_parameters_match(hse)? || !r.hse().read().stable() {
                return Err(Error::ClockConfigurationTimeout);
            }
        }
        let hse_difference = r.hse().read().0 ^ expected_hse.0;
        if (config.hse.is_some() && hse_difference & !crate::RCC_HSE_STABLE_MASK != 0)
            || (config.hse.is_none() && hse_difference != 0)
        {
            return Err(Error::ClockConfigurationTimeout);
        }
        if after_lse {
            let lse = r.lse().read();
            // The generated native change mask includes read-only STABLE.
            // Combine preserved original bits with every requested field and
            // explicit ready/frozen-monitor checks; the mask alone is not proof.
            if !super::lse::preserved_register(self.lse.0)
                || lse.mode() != (requested_lse.mode == super::LseMode::Bypass)
                || lse.driver() != requested_lse.drive
                || lse.pdriver() != requested_lse.startup_drive
                || lse.waitcycle() != requested_lse.wait
                || !lse.stable()
                || !super::lse::healthy(requested_lse)
            {
                return Err(Error::LseNotReady);
            }
        } else if r.lse().read().0 != self.lse.0 {
            return Err(Error::LseClockInUse);
        }
        Ok(())
    }

    fn flash_access(self, config: Config) -> Result<(), Error> {
        self.faults(config)?;
        let info = <crate::peripherals::FLASH as crate::rcc::SealedRccPeripheral>::RCC_INFO;
        if info.reset_asserted() {
            return Err(Error::LseClockInUse);
        }
        if !info.is_enabled() {
            return Err(Error::FlashClockTimeout);
        }
        Ok(())
    }

    fn wait_pair(self, config: Config, flash: pac::flash::regs::Cr2) -> Result<(u8, u8), Error> {
        let mut current_routes = pac::SYSCTRL.cr2().read();
        let mut expected_routes = self.routes;
        expected_routes.set_flashwait(current_routes.flashwait());
        current_routes.set_key(0);
        expected_routes.set_key(0);
        if current_routes.0 != expected_routes.0 {
            return Err(Error::LseClockInUse);
        }
        self.flash_access(config)?;
        let mut current_flash = pac::FLASH.cr2().read();
        self.flash_access(config)?;
        let mut expected_flash = flash;
        expected_flash.set_wait(current_flash.wait());
        current_flash.set_key(0);
        expected_flash.set_key(0);
        if current_flash.0 != expected_flash.0 {
            return Err(Error::FlashLatencyTimeout);
        }
        Ok((current_flash.wait(), current_routes.flashwait()))
    }

    fn require_wait(
        self,
        config: Config,
        flash: pac::flash::regs::Cr2,
        wait: u8,
    ) -> Result<(), Error> {
        if self.wait_pair(config, flash)? != (wait, wait) {
            return Err(Error::FlashLatencyTimeout);
        }
        Ok(())
    }

    fn write_wait(
        self,
        config: Config,
        flash: pac::flash::regs::Cr2,
        old: u8,
        new: u8,
        verify: impl Fn() -> Result<(), Error>,
    ) -> Result<(), Error> {
        verify()?;
        self.require_wait(config, flash, old)?;
        self.flash_access(config)?;
        pac::FLASH.cr2().modify(|w| {
            w.set_key(0x5a5a);
            w.set_wait(new);
        });
        self.flash_access(config)?;
        for _ in 0..config.timeout {
            // Fault/source/policy/divider errors precede either success or the
            // sole permitted wait branch. Mixed alias observations fail closed;
            // no propagation bound or atomic observation is assumed.
            verify()?;
            let pair = self.wait_pair(config, flash)?;
            if pair == (new, new) {
                barrier();
                verify()?;
                return self.require_wait(config, flash, new);
            }
            if old == new || pair != (old, old) {
                return Err(Error::FlashLatencyTimeout);
            }
            core::hint::spin_loop();
        }
        Err(Error::FlashLatencyTimeout)
    }
}

/// L012 target tail: final WAIT is established under verified factory HSI.
/// The final LSE selection is the last CR0 write and the last Flash write is
/// already behind it. All failure and pad/gate cleanup paths preserve that edge.
#[cfg(rcc_lse)]
fn finish_lse_sysclk(
    config: Config,
    mut clocks: Clocks,
    entry: LseSysclkSnapshot,
    flash: pac::flash::regs::Cr2,
    trim: u16,
    needs_lsi: bool,
    cs: critical_section::CriticalSection<'_>,
) -> Result<Clocks, Error> {
    let r = pac::SYSCTRL;
    let lse = config.lse.ok_or(Error::LseNotConfigured)?;
    let sources = || entry.prepared_sources(config, trim, needs_lsi, true);
    let verify_hsi = || {
        sources()?;
        entry.clock_matches(ClockSource::Hsi, config.ahb_pre as u8, config.apb_pre as u8)
    };
    sources()?;
    entry.clock_matches(
        ClockSource::Hsi,
        entry.clock.hclkprs().max(AHBPrescaler::Div8 as u8),
        entry.clock.pclkprs().max(APBPrescaler::Div8 as u8),
    )?;
    entry.require_wait(config, flash, MAX_FLASH_WAIT as u8)?;
    if let Some(hse) = config.hse {
        if !crate::rcc_hse_pins_match(hse.mode == HseMode::Bypass, config.timeout, cs, true)? {
            return Err(Error::ClockConfigurationTimeout);
        }
    }
    // HSE pad verification may open/restore a gate. Recheck before CR0 changes.
    sources()?;
    entry.clock_matches(
        ClockSource::Hsi,
        entry.clock.hclkprs().max(AHBPrescaler::Div8 as u8),
        entry.clock.pclkprs().max(APBPrescaler::Div8 as u8),
    )?;
    entry.require_wait(config, flash, MAX_FLASH_WAIT as u8)?;
    r.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_sysclk(ClockSource::Hsi);
        w.set_hclkprs(config.ahb_pre as u8);
        w.set_pclkprs(config.apb_pre as u8);
    });
    // Configuration identity changes are errors, never source-readiness lag.
    verify_hsi()?;
    entry.require_wait(config, flash, MAX_FLASH_WAIT as u8)?;
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
        // Full effective 4.08 MHz fallback, without bus-divider credit.
        .max(
            crate::rcc::ClockBounds::hsi(crate::RCC_FIXED_CCS_HSI_DIVISOR)
                .maximum()
                .0,
        );
    let final_wait = ((upper_hclk - 1) / crate::RCC_FLASH_WAIT_STEP_HZ) as u8;
    entry.write_wait(config, flash, MAX_FLASH_WAIT as u8, final_wait, verify_hsi)?;
    verify_hsi()?;
    entry.require_wait(config, flash, final_wait)?;
    // LAST CR0 write. No rollback, Flash retry, divider repair or LSE reselect
    // is permitted below, including timeout, fault and pad/gate failure paths.
    r.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_sysclk(ClockSource::Lse);
    });
    for attempt in 0..config.timeout {
        sources()?;
        entry.require_wait(config, flash, final_wait)?;
        let current = r.cr0().read();
        // Only the old HSI selector may await the requested LSE selector.
        // Dividers and all untouched CR0 bits must match on every attempt.
        entry.clock_matches(current.sysclk(), config.ahb_pre as u8, config.apb_pre as u8)?;
        if current.sysclk() == ClockSource::Lse {
            break;
        }
        if current.sysclk() != ClockSource::Hsi || attempt + 1 == config.timeout {
            return Err(Error::LseClockSwitchTimeout);
        }
        core::hint::spin_loop();
    }
    barrier();
    let verify_lse = || {
        sources()?;
        let clock = r.cr0().read();
        if clock.sysclk() != ClockSource::Lse {
            return Err(Error::LseClockSwitchTimeout);
        }
        entry.clock_matches(ClockSource::Lse, config.ahb_pre as u8, config.apb_pre as u8)?;
        entry.require_wait(config, flash, final_wait)
    };
    verify_lse()?;
    if let Some(hse) = config.hse {
        if !crate::rcc_hse_pins_match(hse.mode == HseMode::Bypass, config.timeout, cs, true)? {
            return Err(Error::ClockConfigurationTimeout);
        }
    }
    // Native GPIOC/pad/monitor verification is ownership-aware and never
    // writes CR0 or FLASH. Its independent gate restoration remains required.
    let result = super::lse::verify(lse, cs, true);
    if let Err(error @ (Error::LseConfigurationGateTimeout { .. } | Error::LseGpioGateTimeout)) =
        result
    {
        return Err(error);
    }
    entry.policy(config)?;
    result?;
    verify_lse()?;
    if entry.sources.hseen() {
        if !entry.hse.mode() {
            clocks.hse = Some(HseMode::Oscillator);
        } else if clocks.hse.is_none() {
            clocks.hse = Some(HseMode::Bypass);
        }
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
