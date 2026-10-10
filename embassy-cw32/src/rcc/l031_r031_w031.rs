//! Qualified HSI, direct HSE and exact-package LSE clocks on CW32L031/R031/W031.
//!
//! Own sources: L031 RM CN1.6, R031 RM CN1.3 and W031 RM CN1.4
//! §§4.3–4.7 and 7.4, plus each own datasheet electrical tables.
//! See docs/qualified-l031-hse.md for source pages and board obligations.
//! Existing CCS controls are preserved. A requested new LSE start enables only
//! its detector; this monitored-source admission rule is a software policy.
//! There is no PLL, FLASH prefetch or FLASH cache on this register version.
//! The default remains factory HSI /6. HSIOSC stays alive for independent users.
//! PLL configuration, runtime switching and low-power operation are unsupported.

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
    /// Divide by 1 (48 MHz).
    Div1 = 0b0110,
    /// Divide by 2 (24 MHz).
    Div2 = 0b1000,
    /// Divide by 4 (12 MHz).
    Div4 = 0b1001,
    /// Divide by 6 (8 MHz, the reset divider).
    Div6 = 0b0101,
    /// Divide by 8 (6 MHz).
    Div8 = 0b1011,
    /// Divide by 10 (4.8 MHz).
    Div10 = 0b1100,
    /// Divide by 12 (4 MHz).
    Div12 = 0b1101,
    /// Divide by 14 (approximately 3.429 MHz).
    Div14 = 0b1110,
    /// Divide by 16 (3 MHz).
    Div16 = 0b1111,
}
impl HsiDiv {
    /// Numeric division factor (not the register encoding).
    pub const fn divisor(self) -> u32 {
        match self {
            Self::Div1 => 1,
            Self::Div2 => 2,
            Self::Div4 => 4,
            Self::Div6 => 6,
            Self::Div8 => 8,
            Self::Div10 => 10,
            Self::Div12 => 12,
            Self::Div14 => 14,
            Self::Div16 => 16,
        }
    }
}

/// HSI configuration. HSI is always enabled in the supported clock tree.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Hsi {
    /// Divider applied to the factory-calibrated 48 MHz HSIOSC clock.
    pub div: HsiDiv,
}
impl Default for Hsi {
    fn default() -> Self {
        Self {
            div: crate::RCC_DEFAULT_HSI_DIV,
        }
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

/// Divider from HCLK to PCLK. Supported backends have one PCLK domain.
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
    /// Factory-trimmed 32,800 Hz LSI on the exact qualified L031 packages.
    /// Rate bounds do not certify individual-cycle or jitter timing. Cold entry
    /// excludes every reviewed direct client and ready observer, even when the
    /// existing trim matches; live factory-matching clients remain untouched.
    #[cfg(rcc_lsi_sysclk)]
    LSI,
    /// The single board-qualified nominal 32768 Hz `Config.lse` source.
    /// Init-only, with a permanently retained factory-LSI detector reference.
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
    /// Crystal drive setting. Ignored electrically in bypass mode.
    pub drive: HseDrive,
}
impl Hse {
    fn bounds(self) -> Result<crate::rcc::ClockBounds, Error> {
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

    fn range(self) -> pac::sysctrl::vals::HseRange {
        // The manual bins are explicitly nominal. At shared endpoints select
        // the upper bin; actual electrical endpoints are checked independently.
        let index = crate::RCC_HSE_FREQUENCY_RANGES_HZ
            .iter()
            .position(|(_, max)| self.freq.0 < *max)
            .unwrap_or(3);
        pac::sysctrl::vals::HseRange::from_bits(index as u8)
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
    /// HSI oscillator divider. HSI remains enabled for every system source.
    pub hsi: Hsi,
    /// Optional external source, configured and reserved even when SYSCLK is HSI.
    pub hse: Option<Hse>,
    /// Init-only board-qualified LSE. None preserves its inherited setup.
    #[cfg(rcc_lse)]
    pub lse: Option<super::Lse>,
    /// System clock source. HSE requires `hse`; LSE requires `lse`.
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
    /// Reset-equivalent nominal 8 MHz clock tree.
    pub const fn new() -> Self {
        Self {
            operating_conditions: crate::rcc::OperatingConditions::new(),
            hsi: Hsi {
                div: crate::RCC_DEFAULT_HSI_DIV,
            },
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
        #[cfg(rcc_lse)]
        let lse = self
            .lse
            .map(|c| c.bounds(self.operating_conditions).map(|b| (c, b)))
            .transpose()?;
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
            #[cfg(rcc_lsi_sysclk)]
            Sysclk::LSI => {
                let c = self.operating_conditions;
                if c.min_supply_mv < crate::RCC_LSI_SUPPLY_MV.0
                    || c.max_supply_mv > crate::RCC_LSI_SUPPLY_MV.1
                {
                    return Err(Error::SupplyOutsideQualifiedRange);
                }
                if c.min_temperature_c < crate::RCC_LSI_TEMPERATURE_C.0
                    || c.max_temperature_c > crate::RCC_LSI_TEMPERATURE_C.1
                {
                    return Err(Error::TemperatureOutsideQualifiedRange);
                }
                crate::rcc::ClockBounds::lsi()
            }
            #[cfg(rcc_lse)]
            Sysclk::LSE => {
                use crate::rtc::sealed::Instance;
                let (config, bounds) = lse.ok_or(Error::LseNotConfigured)?;
                // Own hardware counts and a separate software margin. Factory
                // LSI rate facts do not promise every-cycle/window timing.
                if u64::from(config.min_freq.0) * u64::from(crate::RCC_LSE_SYSCLK_LSI_CYCLES)
                    <= (u64::from(crate::RCC_LSE_SYSCLK_LSE_EDGES)
                        + u64::from(crate::RCC_LSE_SYSCLK_MARGIN_LSE_EDGES))
                        * u64::from(crate::peripherals::RTC::SOURCE_MAXIMUM_HZ)
                {
                    return Err(Error::InvalidLseDetector);
                }
                let c = self.operating_conditions;
                if c.min_supply_mv < crate::peripherals::RTC::SUPPLY_MV.0
                    || c.max_supply_mv > crate::peripherals::RTC::SUPPLY_MV.1
                    || c.min_temperature_c < crate::peripherals::RTC::TEMPERATURE_C.0
                    || c.max_temperature_c > crate::peripherals::RTC::TEMPERATURE_C.1
                {
                    return Err(Error::LseMonitorConditionsOutsideQualifiedRange);
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
        // A retained CCS policy may select HSI. Conservatively qualify HSI for
        // every declared HSE, even if incoming fault switching is disabled.
        // This is an admission restriction, not a must-enable CCS requirement.
        #[cfg(rcc_lse)]
        let retain_hsi = self.hse.is_some() || self.sys == Sysclk::LSE;
        #[cfg(not(rcc_lse))]
        let retain_hsi = self.hse.is_some();
        #[cfg(rcc_lsi_sysclk)]
        let retain_hsi = retain_hsi || self.sys == Sysclk::LSI;
        // LSE also executes configured HSI under the final buses before the
        // final mux write, regardless of the inherited system CCS policy.
        if retain_hsi {
            crate::rcc::operating::validate(
                self.operating_conditions,
                Clocks {
                    source: crate::rcc::ClockBounds::hsi(self.hsi.div.divisor()),
                    ..clocks
                },
            )?;
        }
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
    /// APB clock; also the UART kernel clock when UART SOURCE is PCLK.
    pub pclk: Hertz,
    // Exact cumulative divisors; nominal display rounding must not weaken bounds.
    pub(crate) dividers: [u32; 3],
    pub(crate) source: crate::rcc::ClockBounds,
    hse: Option<HseMode>,
    #[cfg(rcc_lse)]
    pub(crate) lse: Option<(super::Lse, crate::rcc::ClockBounds)>,
}

/// A clock initialization failure.
///
/// Hardware errors may leave a partially changed clock tree. The initializer
/// never lowers flash latency before the requested clocks have been verified.
/// Calibration failures may leave the CPU on temporary LSI. Reset before retrying.
/// After a failure, no frequencies are published and peripherals must not be
/// used on the assumption that the requested configuration took effect.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {
    /// Factory LSI calibration storage reads as erased before native masking.
    #[cfg(rcc_lsi_sysclk)]
    InvalidLsiCalibration,
    /// A requested or selected LSI uses a different trim and cannot be retuned.
    #[cfg(rcc_lsi_sysclk)]
    LsiCalibrationInUse,
    /// LSI ownership, observers or retained configuration are ambiguous.
    #[cfg(rcc_lsi_sysclk)]
    LsiClockInUse,
    #[cfg(rcc_lsi_sysclk)]
    LsiGateEnableTimeout,
    #[cfg(rcc_lsi_sysclk)]
    LsiGateRestoreTimeout,
    #[cfg(rcc_lsi_sysclk)]
    LsiConfigurationTimeout,
    /// The incoming selected source lacks both startup-ready observations.
    #[cfg(rcc_lsi_sysclk)]
    InvalidEntryClock,
    #[cfg(rcc_lse)]
    InvalidLseBounds,
    /// LSE SYSCLK requires the existing board source declaration.
    #[cfg(rcc_lse)]
    LseNotConfigured,
    /// The new system target does not satisfy its software detector margin.
    #[cfg(rcc_lse)]
    InvalidLseDetector,
    /// Board conditions do not qualify the own factory-LSI detector reference.
    #[cfg(rcc_lse)]
    LseMonitorConditionsOutsideQualifiedRange,
    #[cfg(rcc_lse)]
    LseNotReady,
    #[cfg(rcc_lse)]
    LsePinConflict,
    #[cfg(rcc_lse)]
    LseClockInUse,
    InvalidHseBounds,
    HseOutsideQualifiedRange,
    HseConditionsOutsideQualifiedRange,
    HseConditionsDoNotCoverBoard,
    HsePinsUnavailable,
    HseNotConfigured,
    InvalidHseDetector,
    HseTimeout,
    HseStopTimeout,
    HsePinConfigurationTimeout,
    /// Retained RTC/AWT ownership conflicts with the requested HSE setup.
    HseClockInUse,
    RetainedClockInspectionTimeout,
    /// Recalibrating HSI would interrupt a retained AWT or LVD filter.
    HsiClockInUse,
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
    /// The incoming HSI divider is not documented by the supported manuals.
    InvalidHsiDivider,
    /// Factory HSI calibration storage reads as erased (0xffff).
    InvalidCalibration,
    /// The FLASH configuration clock could not be enabled.
    FlashClockTimeout,
    /// FLASH WAIT did not read back as requested.
    FlashLatencyTimeout,
    /// HSI did not report stable within the poll budget.
    HsiTimeout,
    /// Target or temporary LSI did not report stable within the polling budget.
    LsiTimeout,
    /// HSI did not disable and lose its STABLE indication before calibration.
    HsiStopTimeout,
    /// The system clock selector did not read back as the requested source.
    ClockSwitchTimeout,
    /// The temporary LSI software-enable state could not be restored.
    LsiRestoreTimeout,
    /// SYSCLK did not switch to LSI before stopping HSI.
    TemporaryClockSwitchTimeout,
    /// A relevant external source has a sticky startup-failure or loss flag.
    ExternalClockFault,
    /// Active AWT ETR routing may conflict with the HSE input pad.
    HsePinInUse,
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
// The caller must enter from a clock/voltage operating point legal for the
// selected chip, and ensure the final HCLK/PCLK obey the board's VDD limits.
// At least /4 AHB and /8 APB keep the legal incoming sources and qualified
// HSI envelope below the low-voltage bus ceiling throughout the transition.
// Conservative initial Flash WAIT is retained until final readbacks succeed.
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

fn barrier() {
    #[cfg(target_arch = "arm")]
    {
        cortex_m::asm::dsb();
        cortex_m::asm::isb();
    }
}

fn wait_until(timeout: u32, error: Error, mut ready: impl FnMut() -> bool) -> Result<(), Error> {
    for _ in 0..timeout {
        if ready() {
            return Ok(());
        }
        core::hint::spin_loop();
    }
    Err(error)
}

fn set_flash_latency(wait: u32, timeout: u32) -> Result<(), Error> {
    pac::FLASH.cr2().modify(|w| {
        w.set_key(0x5a5a);
        w.set_wait(wait as u8);
    });
    wait_until(timeout, Error::FlashLatencyTimeout, || {
        let r = pac::FLASH.cr2().read();
        u32::from(r.wait()) == wait
    })?;
    barrier();
    Ok(())
}

// Original classification plus an intended-change ledger. This owns no MMIO
// abstraction: the existing native sequence below still performs every source
// transition. Readable identity excludes only WO keys and actual RO STABLE.
#[cfg(rcc_lsi_sysclk)]
struct LsiSysclkState {
    entry_clock: pac::sysctrl::regs::Cr0,
    entry_sources: pac::sysctrl::regs::Cr1,
    entry_hsi: pac::sysctrl::regs::Hsi,
    clock: pac::sysctrl::regs::Cr0,
    sources: pac::sysctrl::regs::Cr1,
    hsi: pac::sysctrl::regs::Hsi,
    hse: pac::sysctrl::regs::Hse,
    lse: pac::sysctrl::regs::Lse,
    lsi: pac::sysctrl::regs::Lsi,
    ier: pac::sysctrl::regs::Ier,
    factory_trim: u16,
    hsi_trim: u16,
    cold: bool,
    requested: bool,
    ready: Cell<bool>,
    monitor_hse: bool,
    monitor_lse: bool,
    consumers: Option<crate::RccFactoryLsiConsumers>,
    awt: Option<pac::awt::regs::Cr>,
}

#[cfg(rcc_lsi_sysclk)]
pub(crate) fn lsi_inspection_error(error: super::peripheral::ClockInspectionError) -> Error {
    match error {
        super::peripheral::ClockInspectionError::EnableFailed {
            restore_failed: false,
        } => Error::LsiGateEnableTimeout,
        super::peripheral::ClockInspectionError::EnableFailed {
            restore_failed: true,
        }
        | super::peripheral::ClockInspectionError::RestoreFailed => Error::LsiGateRestoreTimeout,
    }
}

#[cfg(rcc_lsi_sysclk)]
impl LsiSysclkState {
    fn capture(config: Config) -> Result<Self, Error> {
        let r = pac::SYSCTRL;
        let mut clock = r.cr0().read();
        let mut sources = r.cr1().read();
        let hsi = r.hsi().read();
        let hse = r.hse().read();
        let lse = r.lse().read();
        let lsi = r.lsi().read();
        let mut ier = r.ier().read();
        let flags = r.isr().read();
        let pending = cortex_m::peripheral::NVIC::is_pending(pac::Interrupt::SYSCTRL);
        let entry_ready = match clock.sysclk() {
            ClockSource::Hsi => hsi.stable() && flags.hsistable(),
            ClockSource::Hse => sources.hseen() && hse.stable() && flags.hsestable(),
            ClockSource::Lsi => lsi.stable() && flags.lsistable(),
            ClockSource::Lse => sources.lseen() && lse.stable() && flags.lsestable(),
            _ => return Err(Error::InvalidClockSource),
        };
        if !matches!(hsi.div(), 5 | 6 | 8 | 9 | 11..=15) {
            return Err(Error::InvalidHsiDivider);
        }
        if !entry_ready {
            return Err(Error::InvalidEntryClock);
        }
        // One aligned factory read per source. Erased raw words are rejected
        // before native masking; zero is a permitted LSI calibration value.
        let raw =
            unsafe { core::ptr::read_volatile(crate::RCC_LSI_FACTORY_TRIM_ADDRESS as *const u16) };
        if raw == u16::MAX {
            return Err(Error::InvalidLsiCalibration);
        }
        let mut calibrated = lsi;
        calibrated.set_trim(raw);
        let selected = clock.sysclk() == ClockSource::Lsi;
        let live = sources.lsien() || selected;
        if live && lsi.trim() != calibrated.trim() {
            return Err(Error::LsiCalibrationInUse);
        }
        if lsi.stable() != flags.lsistable() || (!live && lsi.stable()) {
            return Err(Error::LsiClockInUse);
        }
        if (sources.hseccs() || sources.lseccs())
            && !(sources.lsien() && lsi.stable() && flags.lsistable())
        {
            return Err(Error::LsiClockInUse);
        }
        if !live
            && (sources.hseccs() || sources.lseccs() || ier.lsirdy() || flags.lsirdy() || pending)
        {
            return Err(Error::LsiClockInUse);
        }
        let raw_hsi =
            unsafe { core::ptr::read_volatile(crate::RCC_FACTORY_HSI_TRIM_ADDRESS as *const u16) };
        if raw_hsi == u16::MAX {
            return Err(Error::InvalidCalibration);
        }
        let mut calibrated_hsi = hsi;
        calibrated_hsi.set_trim(raw_hsi);
        let monitor_hse =
            config.hse.is_some() || sources.hseen() || clock.sysclk() == ClockSource::Hse;
        let monitor_lse =
            config.lse.is_some() || sources.lseen() || clock.sysclk() == ClockSource::Lse;
        check_external_faults(monitor_hse, monitor_lse)?;
        clock.set_key(0);
        sources.set_key(0);
        ier.set_key(0);
        Ok(Self {
            entry_clock: clock,
            entry_sources: sources,
            entry_hsi: hsi,
            clock,
            sources,
            hsi,
            hse,
            lse,
            lsi,
            ier,
            factory_trim: calibrated.trim(),
            hsi_trim: calibrated_hsi.trim(),
            cold: !live,
            requested: false,
            ready: Cell::new(lsi.stable()),
            monitor_hse,
            monitor_lse,
            consumers: None,
            awt: None,
        })
    }

    fn check_identity(&self) -> Result<(), Error> {
        let r = pac::SYSCTRL;
        let mut clock = r.cr0().read();
        let mut sources = r.cr1().read();
        let mut ier = r.ier().read();
        let hsi = r.hsi().read();
        let hse = r.hse().read();
        let lse = r.lse().read();
        let lsi = r.lsi().read();
        let flags = r.isr().read();
        clock.set_key(0);
        sources.set_key(0);
        ier.set_key(0);
        check_external_faults(self.monitor_hse, self.monitor_lse)?;
        if clock.0 != self.clock.0
            || sources.0 != self.sources.0
            || ier.0 != self.ier.0
            || hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK
                != self.hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK
            || hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK
                != self.hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK
            || lse.0 & crate::RCC_LSI_LSE_PARAMETERS_MASK
                != self.lse.0 & crate::RCC_LSI_LSE_PARAMETERS_MASK
            || lsi.0 & crate::RCC_LSI_PARAMETERS_MASK != self.lsi.0 & crate::RCC_LSI_PARAMETERS_MASK
            || lsi.stable() != flags.lsistable()
        {
            return Err(Error::LsiClockInUse);
        }
        if self.ready.get() && !lsi.stable() {
            return Err(Error::LsiTimeout);
        }
        if lsi.stable() {
            // Any coherent observation is sticky, including before prepare's
            // polling loop. A later low status never earns a new startup wait.
            self.ready.set(true);
        }
        let selected_ready = match clock.sysclk() {
            ClockSource::Hsi => hsi.stable() && flags.hsistable(),
            ClockSource::Hse => sources.hseen() && hse.stable() && flags.hsestable(),
            ClockSource::Lsi => lsi.stable() && flags.lsistable(),
            ClockSource::Lse => sources.lseen() && lse.stable() && flags.lsestable(),
            _ => false,
        };
        if !selected_ready {
            return Err(Error::InvalidEntryClock);
        }
        if self.cold
            && !self.requested
            && (sources.lsien()
                || clock.sysclk() == ClockSource::Lsi
                || lsi.stable()
                || flags.lsistable()
                || sources.hseccs()
                || sources.lseccs()
                || ier.lsirdy()
                || flags.lsirdy()
                || cortex_m::peripheral::NVIC::is_pending(pac::Interrupt::SYSCTRL))
        {
            return Err(Error::LsiClockInUse);
        }
        if self.requested && !sources.lsien() {
            return Err(Error::LsiConfigurationTimeout);
        }
        Ok(())
    }

    fn check_consumers(
        &self,
        timeout: u32,
        cs: critical_section::CriticalSection<'_>,
    ) -> Result<(), Error> {
        if let Some(expected) = self.consumers {
            let actual =
                crate::rcc_factory_lsi_consumers(timeout, cs, self.monitor_hse, self.monitor_lse)?;
            if actual != expected {
                return Err(Error::LsiClockInUse);
            }
        }
        Ok(())
    }

    fn check_target(
        &self,
        timeout: u32,
        cs: critical_section::CriticalSection<'_>,
    ) -> Result<(), Error> {
        self.check_identity()?;
        self.check_consumers(timeout, cs)?;
        self.check_identity()
    }

    fn prepare(
        &mut self,
        timeout: u32,
        cs: critical_section::CriticalSection<'_>,
    ) -> Result<(), Error> {
        let r = pac::SYSCTRL;
        if !self.cold {
            return self.wait_ready(timeout);
        }
        for _ in 0..2 {
            self.check_identity()?;
            let consumers =
                crate::rcc_factory_lsi_consumers(timeout, cs, self.monitor_hse, self.monitor_lse)?;
            self.check_identity()?;
            if self.consumers.is_some_and(|previous| previous != consumers) {
                return Err(Error::LsiClockInUse);
            }
            self.consumers = Some(consumers);
        }
        self.check_identity()?;
        let mut expected = self.lsi;
        expected.set_trim(self.factory_trim);
        if self.lsi.trim() != self.factory_trim {
            r.lsi().modify(|w| w.set_trim(self.factory_trim));
        }
        wait_until(timeout, Error::LsiConfigurationTimeout, || {
            r.lsi().read().0 == expected.0 && !r.isr().read().lsistable()
        })?;
        self.lsi = expected;
        // Matching TRIM skips only its write. Both complete passes and this
        // third request-use-edge comparison remain mandatory for cold entry.
        self.check_target(timeout, cs)
    }

    fn wait_ready(&mut self, timeout: u32) -> Result<(), Error> {
        for _ in 0..timeout {
            self.check_identity()?;
            let r = pac::SYSCTRL;
            let lsi = r.lsi().read();
            let mirror = r.isr().read().lsistable();
            if lsi.stable() != mirror {
                return Err(Error::LsiClockInUse);
            }
            if lsi.stable() {
                self.ready.set(true);
                return self.check_identity();
            }
            core::hint::spin_loop();
        }
        Err(Error::LsiTimeout)
    }

    fn inspect_awt(
        &self,
        timeout: u32,
        cs: critical_section::CriticalSection<'_>,
    ) -> Result<pac::awt::regs::Cr, Error> {
        let info = <crate::peripherals::AWT as crate::rcc::SealedRccPeripheral>::RCC_INFO;
        let gate = info.is_enabled();
        if info.reset_asserted() {
            return Err(Error::LsiClockInUse);
        }
        let buffered = info
            .inspect_for_init(cs, timeout, || {
                if !info.is_enabled() || info.reset_asserted() {
                    return Err(Error::LsiClockInUse);
                }
                let value = pac::AWT.cr().read();
                if !info.is_enabled() || info.reset_asserted() {
                    return Err(Error::LsiClockInUse);
                }
                Ok(value)
            })
            .map_err(lsi_inspection_error)?;
        if info.is_enabled() != gate {
            return Err(Error::LsiGateRestoreTimeout);
        }
        check_external_faults(self.monitor_hse, self.monitor_lse)?;
        if info.reset_asserted() {
            return Err(Error::LsiClockInUse);
        }
        buffered
    }

    fn check_hsi_owners(
        &self,
        timeout: u32,
        cs: critical_section::CriticalSection<'_>,
    ) -> Result<(), Error> {
        let awt = self.inspect_awt(timeout, cs)?;
        if self.awt.is_none_or(|original| original.0 != awt.0) {
            return Err(Error::HsiClockInUse);
        }
        if (awt.en() && awt.src() == pac::awt::vals::Source::Hsiosc)
            || (pac::LVD.cr0().read().en()
                && pac::LVD.cr1().read().flten()
                && pac::LVD.cr1().read().fltclk())
        {
            return Err(Error::HsiClockInUse);
        }
        self.check_identity()
    }

    fn verify_tree(
        &self,
        config: Config,
        cs: critical_section::CriticalSection<'_>,
    ) -> Result<(), Error> {
        self.check_target(config.timeout, cs)?;
        let awt = self.inspect_awt(config.timeout, cs)?;
        if self.awt.is_none_or(|original| original.0 != awt.0) {
            return Err(Error::LsiClockInUse);
        }
        if self.sources.hseen() && !crate::rcc_hse_pins_match(self.hse.mode(), config.timeout, cs)?
        {
            return Err(Error::HseClockInUse);
        }
        if let Some(lse) = config.lse {
            super::lse::verify(lse, cs)?;
        } else if self.sources.lseen()
            && !crate::rcc_lse_pins_match(self.lse.mode(), false, config.timeout, cs)?
        {
            return Err(Error::LseClockInUse);
        }
        self.check_target(config.timeout, cs)?;
        let r = pac::SYSCTRL;
        let flags = r.isr().read();
        if !self.ready.get()
            || !self.sources.lsien()
            || !r.lsi().read().stable()
            || !flags.lsistable()
        {
            return Err(Error::LsiTimeout);
        }
        if !self.sources.hsien() || !r.hsi().read().stable() || !flags.hsistable() {
            return Err(Error::HsiTimeout);
        }
        if self.sources.hseen() && (!r.hse().read().stable() || !flags.hsestable()) {
            return Err(Error::HseTimeout);
        }
        if self.sources.lseen() && (!r.lse().read().stable() || !flags.lsestable()) {
            return Err(Error::LseNotReady);
        }
        check_external_faults(self.monitor_hse, self.monitor_lse)
    }
}

fn configure(config: Config, cs: critical_section::CriticalSection<'_>) -> Result<Clocks, Error> {
    let mut clocks = config.frequencies()?;
    let r = pac::SYSCTRL;
    #[cfg(rcc_lsi_sysclk)]
    let permanent_lsi = config.sys == Sysclk::LSI;
    #[cfg(not(rcc_lsi_sysclk))]
    let permanent_lsi = false;
    // Immutable target capture precedes every old LSE helper and source write.
    #[cfg(rcc_lsi_sysclk)]
    let mut lsi_sysclk = if permanent_lsi {
        Some(LsiSysclkState::capture(config)?)
    } else {
        None
    };
    // Only the new target classifies the monitor before any parameter/request
    // change. Old auxiliary LSE retains its original matching-trim shortcut.
    #[cfg(rcc_lse)]
    let lse_sysclk = if config.sys == Sysclk::LSE {
        Some(LseSysclkState::admit()?)
    } else {
        None
    };
    #[cfg(rcc_lse)]
    let reuse_lse = config
        .lse
        .map(|c| super::lse::preflight(c, cs))
        .transpose()?;
    #[cfg(rcc_lse)]
    if let Some(lse) = config.lse.filter(|_| !permanent_lsi) {
        prepare_lse_monitor(lse.poll_budget, cs)?;
    }
    #[cfg(rcc_lsi_sysclk)]
    let old_clock = lsi_sysclk
        .as_ref()
        .map(|state| state.entry_clock)
        .unwrap_or_else(|| r.cr0().read());
    #[cfg(not(rcc_lsi_sysclk))]
    let old_clock = r.cr0().read();
    #[cfg(rcc_lsi_sysclk)]
    let old_sources = lsi_sysclk
        .as_ref()
        .map(|state| state.entry_sources)
        .unwrap_or_else(|| r.cr1().read());
    #[cfg(not(rcc_lsi_sysclk))]
    let old_sources = r.cr1().read();
    let monitor_hse = config.hse.is_some() || old_sources.hseen();
    let monitor_lse = old_sources.lseen();
    #[cfg(rcc_lse)]
    let monitor_lse = monitor_lse || config.lse.is_some();
    #[cfg(rcc_lse)]
    let old_lsi = config.lse.map(|_| r.lsi().read());
    let needs_lsi = (monitor_hse && old_sources.hseccs()) || (monitor_lse && old_sources.lseccs());
    #[cfg(rcc_lse)]
    let needs_lsi = needs_lsi || config.lse.is_some();
    // Before LSE startup every inherited CCS bit must remain identical.
    // New-start policy changes only LSECCS at the dedicated LSE start step.
    let ccs_unchanged = || {
        let v = r.cr1().read();
        v.clkccs() == old_sources.clkccs()
            && v.hseccs() == old_sources.hseccs()
            && v.lseccs() == old_sources.lseccs()
    };
    if !matches!(
        old_clock.sysclk(),
        ClockSource::Hsi | ClockSource::Hse | ClockSource::Lsi | ClockSource::Lse
    ) {
        return Err(Error::InvalidClockSource);
    }
    check_external_faults(monitor_hse, monitor_lse)?;
    #[cfg(rcc_lsi_sysclk)]
    let old_hsi = lsi_sysclk
        .as_ref()
        .map(|state| state.entry_hsi)
        .unwrap_or_else(|| r.hsi().read());
    #[cfg(not(rcc_lsi_sysclk))]
    let old_hsi = r.hsi().read();
    if !matches!(old_hsi.div(), 5 | 6 | 8 | 9 | 11..=15) {
        return Err(Error::InvalidHsiDivider);
    }
    let read_hsi_trim = || {
        let factory =
            unsafe { core::ptr::read_volatile(crate::RCC_FACTORY_HSI_TRIM_ADDRESS as *const u16) };
        if factory == u16::MAX {
            return Err(Error::InvalidCalibration);
        }
        let mut calibrated = pac::sysctrl::regs::Hsi::default();
        calibrated.set_trim(factory);
        Ok(calibrated.trim())
    };
    #[cfg(rcc_lsi_sysclk)]
    let trim = match lsi_sysclk.as_ref() {
        Some(state) => state.hsi_trim,
        None => read_hsi_trim()?,
    };
    #[cfg(not(rcc_lsi_sysclk))]
    let trim = read_hsi_trim()?;
    let needs_trim = old_hsi.trim() != trim;
    // APB gates control register access, not the independent AWT/RTC source.
    // Inspect with their original gates restored; never reset retained owners.
    let awt = if needs_trim || config.hse.is_some() || permanent_lsi {
        #[cfg(rcc_lsi_sysclk)]
        let target_awt = lsi_sysclk
            .as_ref()
            .map(|state| state.inspect_awt(config.timeout, cs))
            .transpose()?;
        #[cfg(not(rcc_lsi_sysclk))]
        let target_awt: Option<pac::awt::regs::Cr> = None;
        Some(match target_awt {
            Some(value) => value,
            None => <crate::peripherals::AWT as crate::rcc::SealedRccPeripheral>::RCC_INFO
                .inspect_for_init(cs, config.timeout, || pac::AWT.cr().read())
                .map_err(|_| Error::RetainedClockInspectionTimeout)?,
        })
    } else {
        None
    };
    #[cfg(rcc_lsi_sysclk)]
    if let Some(state) = lsi_sysclk.as_mut() {
        state.awt = awt;
        state.check_identity()?;
    }
    if needs_trim && awt.is_some_and(|v| v.en() && v.src() == pac::awt::vals::Source::Hsiosc) {
        return Err(Error::HsiClockInUse);
    }
    if needs_trim
        && pac::LVD.cr0().read().en()
        && pac::LVD.cr1().read().flten()
        && pac::LVD.cr1().read().fltclk()
    {
        return Err(Error::HsiClockInUse);
    }
    let mut preserve_hse = false;
    if let Some(hse) = config.hse {
        // AWT ETR can route to PF0. Without a proven nonconflicting route,
        // reject the request before changing any oscillator or pad.
        if awt.is_some_and(|v| v.en() && v.src() == pac::awt::vals::Source::Etr) {
            return Err(Error::HsePinInUse);
        }
        let rtc_source = <crate::peripherals::RTC as crate::rcc::SealedRccPeripheral>::RCC_INFO
            .inspect_for_init(cs, config.timeout, || pac::RTC.cr1().read().source())
            .map_err(|_| Error::RetainedClockInspectionTimeout)?;
        let rtc_hse = matches!(
            rtc_source,
            pac::rtc::vals::Source::HseDiv128
                | pac::rtc::vals::Source::HseDiv256
                | pac::rtc::vals::Source::HseDiv512
                | pac::rtc::vals::Source::HseDiv1024
        );
        preserve_hse =
            rtc_hse || awt.is_some_and(|v| v.en() && v.src() == pac::awt::vals::Source::Hse);
        if preserve_hse
            && (!r.cr1().read().hseen()
                || !r.hse().read().stable()
                || !hse_parameters_match(hse)?
                || !crate::rcc_hse_pins_match(hse.mode == HseMode::Bypass, config.timeout, cs)?)
        {
            return Err(Error::HseClockInUse);
        }
    }

    // Establish conservative Flash and monotonic bus guards before increasing
    // any source. Preserve an inherited divider already stronger than /4,/8.
    <crate::peripherals::FLASH as crate::rcc::SealedRccPeripheral>::RCC_INFO
        .enable_with_cs_readback(
            cs,
            crate::rcc::Readback::Poll {
                attempts: config.timeout,
                spin: true,
            },
        )
        .map_err(|_| Error::FlashClockTimeout)?;
    set_flash_latency(crate::RCC_INITIAL_FLASH_WAIT, config.timeout)?;
    let guard_hclk = old_clock.hclkprs().max(AHBPrescaler::Div4 as u8);
    let guard_pclk = old_clock.pclkprs().max(APBPrescaler::Div8 as u8);
    r.cr0().modify(|w| {
        w.set_key(0x5a5a);
        if permanent_lsi {
            w.set_sysclk(old_clock.sysclk());
        }
        w.set_hclkprs(guard_hclk);
        w.set_pclkprs(guard_pclk);
    });
    wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
        let v = r.cr0().read();
        v.hclkprs() == guard_hclk && v.pclkprs() == guard_pclk
    })?;
    barrier();
    #[cfg(rcc_lsi_sysclk)]
    if let Some(state) = lsi_sysclk.as_mut() {
        let mut expected = state.clock;
        expected.set_hclkprs(guard_hclk);
        expected.set_pclkprs(guard_pclk);
        wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
            let mut actual = r.cr0().read();
            actual.set_key(0);
            actual.0 == expected.0
        })?;
        state.clock = expected;
        state.prepare(config.timeout, cs)?;
    }

    // The first source write for an LSI target permanently requests LSI and HSI
    // together, before escaping even an incoming selected LSI with LSIEN=0.
    r.cr1().modify(|w| {
        w.set_key(0x5a5a);
        w.set_hsien(true);
        if permanent_lsi {
            w.set_lsien(true);
        }
    });
    wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
        let v = r.cr1().read();
        v.hsien() && ccs_unchanged()
    })?;
    #[cfg(rcc_lsi_sysclk)]
    if let Some(state) = lsi_sysclk.as_mut() {
        let mut expected = state.sources;
        expected.set_hsien(true);
        expected.set_lsien(true);
        wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
            let mut actual = r.cr1().read();
            actual.set_key(0);
            actual.0 == expected.0
        })?;
        state.sources = expected;
        state.requested = true;
        state.wait_ready(config.timeout)?;
        state.check_target(config.timeout, cs)?;
    }
    wait_until(config.timeout, Error::HsiTimeout, || {
        r.hsi().read().stable()
    })?;
    #[cfg(rcc_lsi_sysclk)]
    if let Some(state) = lsi_sysclk.as_ref() {
        state.check_identity()?;
        if !r.isr().read().hsistable() {
            return Err(Error::HsiTimeout);
        }
    }
    // Leave the legal incoming source via unchanged HSI before any retuning.
    r.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_sysclk(ClockSource::Hsi);
    });
    wait_until(config.timeout, Error::ClockSwitchTimeout, || {
        r.cr0().read().sysclk() == ClockSource::Hsi
    })?;
    barrier();
    #[cfg(rcc_lsi_sysclk)]
    if let Some(state) = lsi_sysclk.as_mut() {
        let mut expected = state.clock;
        expected.set_sysclk(ClockSource::Hsi);
        wait_until(config.timeout, Error::ClockSwitchTimeout, || {
            let mut actual = r.cr0().read();
            actual.set_key(0);
            actual.0 == expected.0
        })?;
        state.clock = expected;
        state.check_identity()?;
    }
    let lsi_was_enabled = r.cr1().read().lsien();
    // A retained enabled external detector requires unchanged legal LSI,
    // even without a trim bridge. CLKCCS alone does not enable a detector.
    if (needs_trim || needs_lsi) && !permanent_lsi {
        #[cfg(rcc_lse)]
        if let Some(state) = lse_sysclk {
            // Gate inspection and the HSI escape may have waited. Reclassify
            // the current request/readiness before enabling this reference.
            state.admit_request()?;
        }
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_lsien(true);
        });
        wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
            let v = r.cr1().read();
            v.lsien() && ccs_unchanged()
        })?;
        wait_until(config.timeout, Error::LsiTimeout, || {
            r.lsi().read().stable()
        })?;
        #[cfg(rcc_lse)]
        if let Some(state) = lse_sysclk {
            // A mixed transitional poll is not readiness. No HSI stop/trim or
            // LSE detector start is admitted until the complete predicate holds.
            wait_until(config.timeout, Error::LsiTimeout, || {
                state.monitor_ready(false)
            })?;
        }
    }
    if needs_trim {
        #[cfg(rcc_lsi_sysclk)]
        if let Some(state) = lsi_sysclk.as_ref() {
            state.check_target(config.timeout, cs)?;
            state.check_hsi_owners(config.timeout, cs)?;
        }
        r.cr0().modify(|w| {
            w.set_key(0x5a5a);
            w.set_sysclk(ClockSource::Lsi);
        });
        wait_until(config.timeout, Error::TemporaryClockSwitchTimeout, || {
            r.cr0().read().sysclk() == ClockSource::Lsi
        })?;
        barrier();
        #[cfg(rcc_lsi_sysclk)]
        if let Some(state) = lsi_sysclk.as_mut() {
            let mut expected = state.clock;
            expected.set_sysclk(ClockSource::Lsi);
            wait_until(config.timeout, Error::TemporaryClockSwitchTimeout, || {
                let mut actual = r.cr0().read();
                actual.set_key(0);
                actual.0 == expected.0
            })?;
            state.clock = expected;
            state.check_identity()?;
        }
        #[cfg(rcc_lsi_sysclk)]
        if let Some(state) = lsi_sysclk.as_ref() {
            state.check_target(config.timeout, cs)?;
            state.check_hsi_owners(config.timeout, cs)?;
        }
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_hsien(false);
            if permanent_lsi {
                w.set_lsien(true);
            }
        });
        wait_until(config.timeout, Error::HsiStopTimeout, || {
            !r.cr1().read().hsien()
        })?;
        wait_until(config.timeout, Error::HsiStopTimeout, || {
            !r.hsi().read().stable()
        })?;
        #[cfg(rcc_lsi_sysclk)]
        if let Some(state) = lsi_sysclk.as_mut() {
            let mut expected = state.sources;
            expected.set_hsien(false);
            wait_until(config.timeout, Error::HsiStopTimeout, || {
                let mut actual = r.cr1().read();
                actual.set_key(0);
                actual.0 == expected.0
            })?;
            state.sources = expected;
            state.check_identity()?;
        }
        r.hsi().modify(|w| w.set_trim(trim));
        wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
            r.hsi().read().trim() == trim
        })?;
        #[cfg(rcc_lsi_sysclk)]
        if let Some(state) = lsi_sysclk.as_mut() {
            let mut expected = state.hsi;
            expected.set_trim(trim);
            wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
                r.hsi().read().0 & crate::RCC_LSI_HSI_PARAMETERS_MASK
                    == expected.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK
            })?;
            state.hsi = expected;
            state.check_identity()?;
        }
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_hsien(true);
            if permanent_lsi {
                w.set_lsien(true);
            }
        });
        wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
            let v = r.cr1().read();
            v.hsien() && ccs_unchanged()
        })?;
        wait_until(config.timeout, Error::HsiTimeout, || {
            r.hsi().read().stable()
        })?;
        #[cfg(rcc_lsi_sysclk)]
        if let Some(state) = lsi_sysclk.as_mut() {
            let mut expected = state.sources;
            expected.set_hsien(true);
            wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
                let mut actual = r.cr1().read();
                actual.set_key(0);
                actual.0 == expected.0
            })?;
            state.sources = expected;
            state.check_identity()?;
        }
        #[cfg(rcc_lsi_sysclk)]
        if let Some(state) = lsi_sysclk.as_ref() {
            state.check_target(config.timeout, cs)?;
            if !r.isr().read().hsistable() {
                return Err(Error::HsiTimeout);
            }
        }
        r.cr0().modify(|w| {
            w.set_key(0x5a5a);
            w.set_sysclk(ClockSource::Hsi);
        });
        wait_until(config.timeout, Error::ClockSwitchTimeout, || {
            r.cr0().read().sysclk() == ClockSource::Hsi
        })?;
        barrier();
        #[cfg(rcc_lsi_sysclk)]
        if let Some(state) = lsi_sysclk.as_mut() {
            let mut expected = state.clock;
            expected.set_sysclk(ClockSource::Hsi);
            wait_until(config.timeout, Error::ClockSwitchTimeout, || {
                let mut actual = r.cr0().read();
                actual.set_key(0);
                actual.0 == expected.0
            })?;
            state.clock = expected;
            state.check_identity()?;
        }
        if !lsi_was_enabled && !needs_lsi && !permanent_lsi {
            r.cr1().modify(|w| {
                w.set_key(0x5a5a);
                w.set_lsien(false);
            });
            wait_until(config.timeout, Error::LsiRestoreTimeout, || {
                !r.cr1().read().lsien()
            })?;
        }
    }
    r.hsi().modify(|w| w.set_div(config.hsi.div as u8));
    wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
        r.hsi().read().div() == config.hsi.div as u8
    })?;
    wait_until(config.timeout, Error::HsiTimeout, || {
        r.hsi().read().stable()
    })?;

    #[cfg(rcc_lsi_sysclk)]
    if let Some(state) = lsi_sysclk.as_mut() {
        let mut expected = state.hsi;
        expected.set_div(config.hsi.div as u8);
        wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
            r.hsi().read().0 & crate::RCC_LSI_HSI_PARAMETERS_MASK
                == expected.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK
        })?;
        state.hsi = expected;
        state.check_target(config.timeout, cs)?;
        if !r.isr().read().hsistable() {
            return Err(Error::HsiTimeout);
        }
    }
    if let Some(hse) = config.hse.filter(|_| !preserve_hse) {
        #[cfg(rcc_lsi_sysclk)]
        if let Some(state) = lsi_sysclk.as_ref() {
            state.check_target(config.timeout, cs)?;
        }
        // Even an already-ready source is stopped before pin or parameter
        // changes: STABLE alone is not continuous oscillator validity.
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_hseen(false);
            if permanent_lsi {
                w.set_lsien(true);
            }
        });
        wait_until(config.timeout, Error::HseStopTimeout, || {
            !r.cr1().read().hseen()
        })?;
        wait_until(config.timeout, Error::HseStopTimeout, || {
            !r.hse().read().stable()
        })?;
        #[cfg(rcc_lsi_sysclk)]
        if let Some(state) = lsi_sysclk.as_mut() {
            let mut expected = state.sources;
            expected.set_hseen(false);
            wait_until(config.timeout, Error::HseStopTimeout, || {
                let mut actual = r.cr1().read();
                actual.set_key(0);
                actual.0 == expected.0
            })?;
            state.sources = expected;
            state.check_identity()?;
        }
        crate::rcc_configure_hse_pins(hse.mode == HseMode::Bypass, config.timeout, cs)?;
        #[cfg(rcc_lsi_sysclk)]
        if let Some(state) = lsi_sysclk.as_mut() {
            // This existing helper leaves only its actual projected pad-bank
            // gates enabled. Bypass owns input only; unavailable pads own none.
            if let Some(consumers) = state.consumers.as_mut() {
                let (input, output) = crate::RCC_LSI_HSE_GPIO_GATE_INDICES;
                if let Some(index) = input {
                    consumers.1[index] = true;
                }
                if hse.mode == HseMode::Oscillator {
                    if let Some(index) = output {
                        consumers.1[index] = true;
                    }
                }
            }
            state.check_target(config.timeout, cs)?;
        }
        let detector = hse.detector_count()?;
        r.hse().modify(|w| {
            w.set_mode(hse.mode == HseMode::Bypass);
            w.set_driver(hse.drive);
            w.set_freqrange(hse.range());
            w.set_waitcycle(pac::sysctrl::vals::HseWait::Cycles262144);
            w.set_flt(false);
            w.set_detcnt(detector);
        });
        wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
            let v = r.hse().read();
            let range = v.freqrange();
            v.mode() == (hse.mode == HseMode::Bypass)
                && v.driver() == hse.drive
                && range == hse.range()
                && v.waitcycle() == pac::sysctrl::vals::HseWait::Cycles262144
                && !v.flt()
                && v.detcnt() == detector
        })?;
        #[cfg(rcc_lsi_sysclk)]
        if let Some(state) = lsi_sysclk.as_mut() {
            let mut expected = state.hse;
            expected.set_mode(hse.mode == HseMode::Bypass);
            expected.set_driver(hse.drive);
            expected.set_freqrange(hse.range());
            expected.set_waitcycle(pac::sysctrl::vals::HseWait::Cycles262144);
            expected.set_flt(false);
            expected.set_detcnt(detector);
            wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
                r.hse().read().0 & crate::RCC_LSI_HSE_PARAMETERS_MASK
                    == expected.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK
            })?;
            state.hse = expected;
            state.check_target(config.timeout, cs)?;
        }
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_hseen(true);
            if permanent_lsi {
                w.set_lsien(true);
            }
        });
        wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
            r.cr1().read().hseen()
        })?;
        wait_until(config.timeout, Error::HseTimeout, || {
            r.hse().read().stable()
        })?;
        #[cfg(rcc_lsi_sysclk)]
        if let Some(state) = lsi_sysclk.as_mut() {
            let mut expected = state.sources;
            expected.set_hseen(true);
            wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
                let mut actual = r.cr1().read();
                actual.set_key(0);
                actual.0 == expected.0
            })?;
            state.sources = expected;
            state.check_identity()?;
        }
        #[cfg(rcc_lsi_sysclk)]
        if let Some(state) = lsi_sysclk.as_ref() {
            state.check_target(config.timeout, cs)?;
            if !r.isr().read().hsestable() {
                return Err(Error::HseTimeout);
            }
        }
    }
    check_external_faults(monitor_hse, monitor_lse)?;
    #[cfg(rcc_lsi_sysclk)]
    if let Some(mut state) = lsi_sysclk {
        // P7: the existing auxiliary helper runs on guarded HSI, with a
        // permanently requested factory-LSI monitor. Its bounded before/after
        // proof does not change the helper's pad-window or monitor contract.
        state.check_target(config.timeout, cs)?;
        if let Some(lse) = config.lse {
            let reused = reuse_lse.unwrap();
            super::lse::start(lse, reused, cs)?;
            if !reused {
                // Native L031's established helper writes a default register,
                // not a modify. Model exactly that deliberate legacy behavior.
                let mut expected_lse = pac::sysctrl::regs::Lse::default();
                expected_lse.set_mode(lse.mode == super::LseMode::Bypass);
                expected_lse.set_driver(lse.drive);
                expected_lse.set_amp(lse.amplitude);
                expected_lse.set_waitcycle(lse.wait);
                let mut expected_sources = state.sources;
                expected_sources.set_lseen(true);
                expected_sources.set_lseccs(true);
                wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
                    let mut sources = r.cr1().read();
                    sources.set_key(0);
                    sources.0 == expected_sources.0
                        && r.lse().read().0 & crate::RCC_LSI_LSE_PARAMETERS_MASK
                            == expected_lse.0 & crate::RCC_LSI_LSE_PARAMETERS_MASK
                })?;
                state.sources = expected_sources;
                state.lse = expected_lse;
                // LSE pad inspection restores GPIOC; it advances no gate.
            }
            state.check_target(config.timeout, cs)?;
        }
        // P8: qualify actual owners and pads, then execute the independently
        // validated configured HSI under final buses before selecting LSI.
        state.verify_tree(config, cs)?;
        let mut final_hsi = state.clock;
        final_hsi.set_sysclk(ClockSource::Hsi);
        final_hsi.set_hclkprs(config.ahb_pre as u8);
        final_hsi.set_pclkprs(config.apb_pre as u8);
        r.cr0().modify(|w| {
            w.set_key(0x5a5a);
            w.set_sysclk(ClockSource::Hsi);
            w.set_hclkprs(config.ahb_pre as u8);
            w.set_pclkprs(config.apb_pre as u8);
        });
        wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
            let mut actual = r.cr0().read();
            actual.set_key(0);
            actual.0 == final_hsi.0
        })?;
        barrier();
        state.clock = final_hsi;
        state.verify_tree(config, cs)?;
        // P9: latency includes retained HSI even though the final source is
        // slower. No oscillator/source/divider write follows the LSI selection.
        let upper_hclk = clocks.hclk_bounds().maximum().0.max(
            crate::rcc::ClockBounds::hsi(config.hsi.div.divisor())
                .divided_by(config.ahb_pre.divisor())
                .maximum()
                .0,
        );
        let flash_wait = (upper_hclk - 1) / crate::RCC_FLASH_WAIT_STEP_HZ;
        set_flash_latency(flash_wait, config.timeout)?;
        state.verify_tree(config, cs)?;
        if u32::from(pac::FLASH.cr2().read().wait()) != flash_wait {
            return Err(Error::FlashLatencyTimeout);
        }
        let mut final_lsi = state.clock;
        final_lsi.set_sysclk(ClockSource::Lsi);
        r.cr0().modify(|w| {
            w.set_key(0x5a5a);
            w.set_sysclk(ClockSource::Lsi);
        });
        wait_until(config.timeout, Error::ClockSwitchTimeout, || {
            let mut actual = r.cr0().read();
            actual.set_key(0);
            actual.0 == final_lsi.0
        })?;
        barrier();
        state.clock = final_lsi;
        state.verify_tree(config, cs)?;
        if u32::from(pac::FLASH.cr2().read().wait()) != flash_wait {
            return Err(Error::FlashLatencyTimeout);
        }
        if clocks.hse.is_none() && state.entry_sources.hseen() {
            clocks.hse = Some(if state.hse.mode() {
                HseMode::Bypass
            } else {
                HseMode::Oscillator
            });
        }
        // Last hardware observation is the final selector and both dividers.
        let mut final_clock = r.cr0().read();
        final_clock.set_key(0);
        if final_clock.0 != final_lsi.0 {
            return Err(Error::ClockSwitchTimeout);
        }
        return Ok(clocks);
    }
    #[cfg(rcc_lse)]
    if let Some(state) = lse_sysclk {
        let lse = config.lse.ok_or(Error::LseNotConfigured)?;
        if !state.monitor_ready(false) || r.cr0().read().sysclk() != ClockSource::Hsi {
            return Err(Error::LsiTimeout);
        }
        // Fresh/exact LSE is established while HSI still executes. Only a fresh
        // start may add LSECCS; CLKCCS/HSECCS and LSELOCK remain inherited.
        super::lse::start(lse, reuse_lse.unwrap(), cs)?;
        let verify_tree = |source, final_buses| {
            state.verify_tree(config, lse, trim, reuse_lse.unwrap(), cs)?;
            let clock = r.cr0().read();
            if clock.sysclk() != source
                || (final_buses
                    && (clock.hclkprs() != config.ahb_pre as u8
                        || clock.pclkprs() != config.apb_pre as u8))
            {
                return Err(Error::ClockConfigurationTimeout);
            }
            Ok(())
        };
        verify_tree(ClockSource::Hsi, false)?;
        // Pure validation proved this final HSI tree legal. Explicit HSI also
        // avoids reasserting a sampled external selector during divider setup.
        r.cr0().modify(|w| {
            w.set_key(0x5a5a);
            w.set_sysclk(ClockSource::Hsi);
            w.set_hclkprs(config.ahb_pre as u8);
            w.set_pclkprs(config.apb_pre as u8);
        });
        wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
            let clock = r.cr0().read();
            clock.sysclk() == ClockSource::Hsi
                && clock.hclkprs() == config.ahb_pre as u8
                && clock.pclkprs() == config.apb_pre as u8
        })?;
        barrier();
        let upper_hclk = clocks.hclk_bounds().maximum().0.max(
            crate::rcc::ClockBounds::hsi(config.hsi.div.divisor())
                .divided_by(config.ahb_pre.divisor())
                .maximum()
                .0,
        );
        let flash_wait = (upper_hclk - 1) / crate::RCC_FLASH_WAIT_STEP_HZ;
        set_flash_latency(flash_wait, config.timeout)?;
        verify_tree(ClockSource::Hsi, true)?;
        if u32::from(pac::FLASH.cr2().read().wait()) != flash_wait {
            return Err(Error::FlashLatencyTimeout);
        }
        r.cr0().modify(|w| {
            w.set_key(0x5a5a);
            w.set_sysclk(ClockSource::Lse);
        });
        wait_until(config.timeout, Error::ClockSwitchTimeout, || {
            let clock = r.cr0().read();
            clock.sysclk() == ClockSource::Lse
                && clock.hclkprs() == config.ahb_pre as u8
                && clock.pclkprs() == config.apb_pre as u8
        })?;
        barrier();
        // No CR0/source/divider write follows this final selector write.
        verify_tree(ClockSource::Lse, true)?;
        if u32::from(pac::FLASH.cr2().read().wait()) != flash_wait {
            return Err(Error::FlashLatencyTimeout);
        }
        // Preserve the old common tail's inherited HSE pad ownership before
        // returning through the dedicated target sequence.
        if clocks.hse.is_none() && state.sources.hseen() {
            clocks.hse = Some(if state.hse.mode() {
                HseMode::Bypass
            } else {
                HseMode::Oscillator
            });
        }
        let final_clock = r.cr0().read();
        if final_clock.sysclk() != ClockSource::Lse
            || final_clock.hclkprs() != config.ahb_pre as u8
            || final_clock.pclkprs() != config.apb_pre as u8
        {
            return Err(Error::ClockSwitchTimeout);
        }
        // No fallible work remains. Failed RCC init never installs this marker;
        // the shared monitor snapshot alone does not strengthen old leaf paths.
        LSE_SYSCLK_MONITOR.borrow(cs).set(Some(state));
        return Ok(clocks);
    }
    let sysclk = match config.sys {
        Sysclk::HSI => ClockSource::Hsi,
        Sysclk::HSE => ClockSource::Hse,
        #[cfg(rcc_lsi_sysclk)]
        Sysclk::LSI => unreachable!("LSI returns through its dedicated sequence"),
        #[cfg(rcc_lse)]
        Sysclk::LSE => unreachable!("LSE returns through its dedicated sequence"),
    };
    // Keep restrictive buses while selecting the source. Install final dividers
    // only after mux acknowledgment, so a slower target never exposes old HSI
    // through newly weakened final buses.
    if config.sys == Sysclk::HSE {
        r.cr0().modify(|w| {
            w.set_key(0x5a5a);
            w.set_sysclk(sysclk);
        });
        wait_until(config.timeout, Error::ClockSwitchTimeout, || {
            r.cr0().read().sysclk() == sysclk
        })?;
        barrier();
    }
    r.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_sysclk(sysclk);
        w.set_hclkprs(config.ahb_pre as u8);
        w.set_pclkprs(config.apb_pre as u8);
    });
    wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
        let v = r.cr0().read();
        v.sysclk() == sysclk
            && v.hclkprs() == config.ahb_pre as u8
            && v.pclkprs() == config.apb_pre as u8
    })?;
    barrier();
    wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
        let v = r.hsi().read();
        v.div() == config.hsi.div as u8 && v.trim() == trim
    })?;
    wait_until(config.timeout, Error::HsiTimeout, || {
        r.hsi().read().stable()
    })?;
    if config.hse.is_some() {
        wait_until(config.timeout, Error::HseTimeout, || {
            r.hse().read().stable()
        })?;
    }
    let mut upper_hclk = clocks.hclk_bounds().maximum().0;
    if config.hse.is_some() {
        upper_hclk = upper_hclk.max(
            crate::rcc::ClockBounds::hsi(config.hsi.div.divisor())
                .divided_by(config.ahb_pre.divisor())
                .maximum()
                .0,
        );
    }
    set_flash_latency(
        (upper_hclk - 1) / crate::RCC_FLASH_WAIT_STEP_HZ,
        config.timeout,
    )?;
    // Detect mux changes during the final Flash handshake before publishing.
    if r.cr0().read().sysclk() != sysclk {
        return Err(Error::ClockSwitchTimeout);
    }
    check_external_faults(monitor_hse, monitor_lse)?;
    if !ccs_unchanged() {
        return Err(Error::ClockConfigurationTimeout);
    }
    #[cfg(rcc_lse)]
    if let Some(old_lsi) = old_lsi {
        let lsi = r.lsi().read();
        if !r.cr1().read().lsien()
            || !lsi.stable()
            || lsi.trim() != old_lsi.trim()
            || lsi.waitcycle() != old_lsi.waitcycle()
        {
            return Err(Error::LsiTimeout);
        }
    }
    #[cfg(rcc_lse)]
    if let Some(lse) = config.lse {
        super::lse::start(lse, reuse_lse.unwrap(), cs)?;
        let final_sources = r.cr1().read();
        if final_sources.clkccs() != old_sources.clkccs()
            || final_sources.hseccs() != old_sources.hseccs()
            || !final_sources.lseccs()
        {
            return Err(Error::ClockConfigurationTimeout);
        }
        super::lse::verify(lse, cs)?;
    }
    // Freeze inherited ownership too, so safe GPIO remains blocked for the
    // entire boot even if later unsupported raw PAC writes disable HSE.
    if clocks.hse.is_none() && old_sources.hseen() {
        clocks.hse = Some(if r.hse().read().mode() {
            HseMode::Bypass
        } else {
            HseMode::Oscillator
        });
    }
    Ok(clocks)
}

// STABLE is a startup latch, not a continuous source-valid indication. Sticky
// faults are observed without writing ICR or disturbing unrelated events.
// Rejecting stale relevant flags is a deliberate first-batch restriction.
fn check_external_faults(hse: bool, lse: bool) -> Result<(), Error> {
    let v = pac::SYSCTRL.isr().read();
    if (hse && (v.hsefail() || v.hsefault())) || (lse && (v.lsefail() || v.lsefault())) {
        Err(Error::ExternalClockFault)
    } else {
        Ok(())
    }
}

fn hse_parameters_match(hse: Hse) -> Result<bool, Error> {
    let v = pac::SYSCTRL.hse().read();
    let range = v.freqrange();
    Ok(v.mode() == (hse.mode == HseMode::Bypass)
        && v.driver() == hse.drive
        && range == hse.range()
        && v.waitcycle() == pac::sysctrl::vals::HseWait::Cycles262144
        && !v.flt()
        && v.detcnt() == hse.detector_count()?)
}

// Only a requested LSE source needs this monitored-source policy. Default None
// retains the original RCC path. Factory-valid live LSI is never retuned.
#[cfg(rcc_lse)]
fn prepare_lse_monitor(
    timeout: u32,
    cs: critical_section::CriticalSection<'_>,
) -> Result<(), Error> {
    use crate::rtc::sealed::Instance;
    let r = pac::SYSCTRL;
    let factory = unsafe {
        core::ptr::read_volatile(crate::peripherals::RTC::FACTORY_TRIM_ADDRESS as *const u16)
    };
    if factory == u16::MAX {
        return Err(Error::LseNotReady);
    }
    let mut calibration = pac::sysctrl::regs::Lsi::default();
    calibration.set_trim(factory);
    let before = r.lsi().read();
    // An enabled or pending ready interrupt is a retained observer when this
    // request would start LSI. Never clear it or mask the peripheral's owner.
    if !r.cr1().read().lsien() && (r.ier().read().lsirdy() || r.isr().read().lsirdy()) {
        return Err(Error::LseClockInUse);
    }
    if before.trim() == calibration.trim() {
        return Ok(());
    }
    // LSIEN alone is not used as ownership proof. The generated gate-preserving
    // checks cover every reviewed direct selector and detector, twice, with
    // unchanged native trim/wait and both hardware stable indications low.
    let stopped = || {
        let control = r.cr1().read();
        let lsi = r.lsi().read();
        !control.lsien()
            && !lsi.stable()
            && !r.isr().read().lsistable()
            && !control.hseccs()
            && !control.lseccs()
            && !r.ier().read().lsirdy()
            && !r.isr().read().lsirdy()
            && lsi.trim() == before.trim()
            && lsi.waitcycle() == before.waitcycle()
    };
    for _ in 0..2 {
        if !stopped() || !crate::rcc_lsi_consumers_idle(timeout, cs)? || !stopped() {
            return Err(Error::LseClockInUse);
        }
    }
    // Own manuals require parameters before enable. LSI has no write key.
    // Keep WAIT and every unrelated/reserved field; never stop a live source.
    r.lsi().modify(|w| w.set_trim(calibration.trim()));
    wait_until(timeout, Error::LseNotReady, || {
        let lsi = r.lsi().read();
        lsi.trim() == calibration.trim() && lsi.waitcycle() == before.waitcycle()
    })
}

// The stronger monitor contract belongs only to a successful LSE system target.
// The common auxiliary leaf keeps its historical TRIM/WAIT/direct-STABLE checks.
#[cfg(rcc_lse)]
static LSE_SYSCLK_MONITOR: Mutex<Cell<Option<LseSysclkState>>> = Mutex::new(Cell::new(None));

#[cfg(rcc_lse)]
#[derive(Clone, Copy)]
struct LseSysclkState {
    trim: u16,
    wait: u8,
    sources: pac::sysctrl::regs::Cr1,
    hse: pac::sysctrl::regs::Hse,
    lse: pac::sysctrl::regs::Lse,
}

#[cfg(rcc_lse)]
impl LseSysclkState {
    fn admit() -> Result<Self, Error> {
        use crate::rtc::sealed::Instance;
        let r = pac::SYSCTRL;
        let factory = unsafe {
            core::ptr::read_volatile(crate::peripherals::RTC::FACTORY_TRIM_ADDRESS as *const u16)
        };
        // Reject the erased raw word before native masking; zero is legal.
        if factory == u16::MAX {
            return Err(Error::LseNotReady);
        }
        let mut calibration = pac::sysctrl::regs::Lsi::default();
        calibration.set_trim(factory);
        let lsi = r.lsi().read();
        let state = Self {
            trim: calibration.trim(),
            wait: lsi.waitcycle(),
            sources: r.cr1().read(),
            hse: r.hse().read(),
            lse: r.lse().read(),
        };
        // Selected-source stop inhibition means EN=0 cannot establish a fresh
        // source. Never repair an unenabled/unready selected LSE as cold entry.
        if r.cr0().read().sysclk() == ClockSource::Lse
            && (!state.sources.lseen() || !state.lse.stable())
        {
            return Err(Error::LseClockInUse);
        }
        state.classify_monitor(false)?;
        Ok(state)
    }

    // This classification owns no parameters and does no consumer inspection.
    // The existing own-family prepare_lse_monitor still supplies both complete
    // stopped/consumer/stopped passes for every mismatching trim. A matching
    // stopped source keeps its no-write/no-parameter-owner-proof path, although
    // requesting it may resume parked direct consumers (public handover rule).
    fn classify_monitor(self, require_factory: bool) -> Result<(), Error> {
        let r = pac::SYSCTRL;
        let control = r.cr1().read();
        let lsi = r.lsi().read();
        let flags = r.isr().read();
        let selected = r.cr0().read().sysclk() == ClockSource::Lsi;
        let matching = lsi.trim() == self.trim;
        let stable = lsi.stable() && flags.lsistable();
        if lsi.waitcycle() != self.wait
            || (require_factory && !matching)
            || control.clkccs() != self.sources.clkccs()
            || control.hseccs() != self.sources.hseccs()
            || control.lseccs() != self.sources.lseccs()
            || control.lselock() != self.sources.lselock()
            // Separate reads can straddle real startup. Rejection is a
            // conservative handover policy, not evidence of defective silicon.
            || lsi.stable() != flags.lsistable()
            || (!control.lsien() && (stable || selected))
            || (!stable && selected)
            || ((control.hseccs() || control.lseccs())
                && !(control.lsien() && stable && matching))
            || (!matching
                && (control.lsien() || stable || selected || control.hseccs() || control.lseccs()))
            || (!control.lsien() && (r.ier().read().lsirdy() || flags.lsirdy()))
        {
            return Err(Error::LseClockInUse);
        }
        Ok(())
    }

    fn admit_request(self) -> Result<(), Error> {
        // prepare_lse_monitor has now either preserved matching TRIM or applied
        // an admitted trim-only update. WAIT and CCS policy must still match.
        self.classify_monitor(true)
    }

    fn monitor_ready(self, lse_started: bool) -> bool {
        let r = pac::SYSCTRL;
        let control = r.cr1().read();
        let lsi = r.lsi().read();
        control.lsien()
            && lsi.stable()
            && r.isr().read().lsistable()
            && lsi.trim() == self.trim
            && lsi.waitcycle() == self.wait
            && control.clkccs() == self.sources.clkccs()
            && control.hseccs() == self.sources.hseccs()
            && control.lseccs() == (lse_started || self.sources.lseccs())
            && control.lselock() == self.sources.lselock()
    }

    fn verify_tree(
        self,
        config: Config,
        lse: super::Lse,
        hsi_trim: u16,
        reused_lse: bool,
        cs: critical_section::CriticalSection<'_>,
    ) -> Result<(), Error> {
        let r = pac::SYSCTRL;
        let verify_sources = || {
            let control = r.cr1().read();
            let hsi = r.hsi().read();
            if !self.monitor_ready(true) {
                return Err(Error::LsiTimeout);
            }
            if !control.hsien()
                || !hsi.stable()
                || hsi.trim() != hsi_trim
                || hsi.div() != config.hsi.div as u8
                || !control.lseen()
                || control.hseen() != (self.sources.hseen() || config.hse.is_some())
            {
                return Err(Error::ClockConfigurationTimeout);
            }
            if let Some(hse) = config.hse {
                if !r.hse().read().stable() || !hse_parameters_match(hse)? {
                    return Err(Error::HseTimeout);
                }
            } else if self.sources.hseen()
                && (!r.hse().read().stable() || r.hse().read().0 != self.hse.0)
            {
                return Err(Error::HseClockInUse);
            }
            if reused_lse && r.lse().read().0 != self.lse.0 {
                return Err(Error::LseNotReady);
            }
            check_external_faults(control.hseen(), true)?;
            if !super::lse::healthy(lse) {
                return Err(Error::LseNotReady);
            }
            Ok(())
        };
        verify_sources()?;
        // Each gate-preserving pad inspection can advance bank sampling and
        // events. Check the source tree again after all of that work.
        super::lse::verify(lse, cs)?;
        let hse_bypass = config
            .hse
            .map(|hse| hse.mode == HseMode::Bypass)
            .or_else(|| self.sources.hseen().then_some(self.hse.mode()));
        if let Some(bypass) = hse_bypass {
            if !crate::rcc_hse_pins_match(bypass, config.timeout, cs)? {
                return Err(Error::HseClockInUse);
            }
        }
        verify_sources()
    }
}

#[cfg(rcc_lse)]
pub(super) fn lse_sysclk_monitor_ready() -> bool {
    critical_section::with(|cs| {
        LSE_SYSCLK_MONITOR
            .borrow(cs)
            .get()
            .is_none_or(|state| state.monitor_ready(true))
    })
}
