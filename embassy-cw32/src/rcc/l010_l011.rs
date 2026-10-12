//! CW32L010/CW32L011 one-time factory-HSI and qualified direct-HSE clocks.
//! Exact L010 and L011 packages additionally permit initialization-only reuse of
//! already software-enabled, stable, factory-matching LSI; see
//! docs/l010-live-factory-lsi-sysclk.md and docs/l011-live-factory-lsi-sysclk.md.
//! The live target never starts or programs LSI.
//! Exact qualified L010/L011 packages also support native init-only LSE SYSCLK;
//! see docs/l010-lse-sysclk.md, docs/l011-lse-sysclk.md and [`crate::init`].
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
    /// Reuse live factory-matching LSI on CW32L010F8P6/F8U6/Y8M6
    /// and CW32L011K8T6/K8U6 only.
    /// Requires LSIEN and STABLE at original entry and preserves TRIM/WAIT.
    /// Ordinary POR and `Config::lse = Some(_)` are unsupported. Bounds are
    /// rate-only; IWDT may continue counting and reset during initialization.
    /// No clock-continuity, watchdog deadline or per-cycle guarantee is made.
    /// L011 also requires no configured/enabled HSE and no HSE/LSE detector;
    /// entry SYSCLK must be HSI or LSI. Retained raw-HSI RTC needs PSC1 >= 97.
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    LSI,
    /// Native LSE on CW32L010F8P6/F8U6/Y8M6 and CW32L011K8T6/K8U6.
    /// Requires [`Config::lse`]; generic-family and L012 targets are excluded.
    ///
    /// HSI remains factory calibrated and enabled. StartupOnly can stop the CPU
    /// on later source loss without a fault or error return. MonitoredExistingRoutes
    /// requires an inherited stable, legal unchanged LSI; L011 additionally
    /// requires its own factory-matching TRIM at entry. Existing IRQ/brake routes
    /// are preserved and may affect observers before an init error. Inherited
    /// CLKCCS is preserved; documented HSI4MHz fallback invalidates frozen timings
    /// and does not guarantee progress or register/divider preservation.
    ///
    /// The existing HSI calibration path may temporarily request unchanged legal
    /// LSI; StartupOnly does not require factory LSI. The handover must permit
    /// residual timer/GPIO/IWDT/external progress and L011 PB0 AF3 raw-HSI output
    /// interruptions; concrete vetoes do not establish universal idleness. See
    /// [`crate::init`], [`crate::try_init`], docs/l010-lse-sysclk.md and
    /// docs/l011-lse-sysclk.md. All waits
    /// count CPU iterations while execution continues. Errors publish no clocks,
    /// retain source/pad ownership and require reset before retry; ordinary reset
    /// may retain LSE. The fixed 1 MHz time driver cannot use this system tree.
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
    /// Native board-qualified L010/L011 LSE. The exact qualified three L010 and
    /// two L011 packages may select it as SYSCLK.
    /// See [`crate::init`] and [`crate::try_init`] for the required RTC_OUT/
    /// RTC_1Hz observer disconnection and whole-GPIOB (L010) or GPIOC (L011) handover.
    /// Genuine reset entry with untouched selectors meets those functional
    /// conditions; a register image does not prove reset. Failure permanently
    /// reserves requested pads and may leave the oscillator/gates enabled.
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
    /// Reset-equivalent nominal 4 MHz clock tree.
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
        // New-target exclusions precede even requested external-source bounds.
        #[cfg(all(rcc_cw32l011_v1, rcc_lsi_sysclk))]
        if self.sys == Sysclk::LSI {
            if self.hse.is_some() {
                return Err(Error::UnsupportedLsiHseConfiguration);
            }
            if self.lse.is_some() {
                return Err(Error::UnsupportedLsiLseConfiguration);
            }
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
        #[cfg(all(rcc_cw32l010_v1, rcc_lsi_sysclk))]
        if self.sys == Sysclk::LSI && self.lse.is_some() {
            return Err(Error::UnsupportedLsiLseConfiguration);
        }
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
            #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
            Sysclk::LSI => crate::rcc::ClockBounds::lsi(),
            #[cfg(rcc_lse)]
            Sysclk::LSE => lse.ok_or(Error::LseNotConfigured)?.1,
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
        // Both own families document effective HSI4MHz fallback, without guaranteeing
        // retained bus-divider fields. Give neither HCLK nor PCLK divisor credit.
        #[cfg(rcc_lse)]
        if self.sys == Sysclk::LSE {
            crate::rcc::operating::validate(
                self.operating_conditions,
                Clocks {
                    source: crate::rcc::ClockBounds::hsi(crate::RCC_FIXED_CCS_HSI_DIVISOR),
                    dividers: [self.hsi.div.divisor(), 1, 1],
                    ..clocks
                },
            )?;
        }
        #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
        if self.sys == Sysclk::LSI {
            crate::rcc::operating::validate(
                self.operating_conditions,
                Clocks {
                    source: crate::rcc::ClockBounds::hsi(crate::RCC_FIXED_CCS_HSI_DIVISOR),
                    dividers: [self.hsi.div.divisor(), 1, 1],
                    ..clocks
                },
            )?;
        }
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
    /// Frozen source declaration and policy, conditional on continuing health.
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
    /// This L011 live-LSI slice cannot configure an auxiliary HSE.
    #[cfg(all(rcc_cw32l011_v1, rcc_lsi_sysclk))]
    UnsupportedLsiHseConfiguration,
    /// A live-LSI initialization cannot newly configure an auxiliary LSE.
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    UnsupportedLsiLseConfiguration,
    /// Own factory halfword is erased or the entry LSI trim does not match it.
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    InvalidLsiCalibration,
    /// Entry software request/readiness is absent, or live readiness was lost.
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    LsiNotReady,
    /// Borrowed configuration-gate enable/restore failed, with both diagnostics.
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    LsiConfigurationGateTimeout { enable_failed: bool, restore_failed: bool },
    /// Final LSI selector did not acknowledge within the iteration budget.
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    LsiClockSwitchTimeout,
    #[cfg(rcc_lse)]
    InvalidLseBounds,
    #[cfg(rcc_lse)]
    LseClockInUse,
    #[cfg(rcc_lse)]
    LsePinConflict,
    #[cfg(rcc_lse)]
    LseGpioGateTimeout,
    /// An LSE or target monitor/bridge inspection configuration gate failed to
    /// open and/or return to its entry state. Both fields may be true; no
    /// successful restoration is then promised.
    #[cfg(rcc_lse)]
    LseConfigurationGateTimeout {
        enable_failed: bool,
        restore_failed: bool,
    },
    #[cfg(rcc_lse)]
    LseMonitorNotReady,
    #[cfg(rcc_lse)]
    LseNotReady,
    /// The LSE system target requires a board-qualified LSE declaration.
    #[cfg(rcc_lse)]
    LseNotConfigured,
    /// A first request for entry-nonstable LSI has an inherited consumer/IRQ owner.
    /// L011 live-LSI also rejects either inherited external-clock detector.
    #[cfg(rcc_lse)]
    LsiClockInUse,
    /// Starting/restarting or awaiting HSI conflicts with raw-HSI MCO or HSIRDY.
    #[cfg(rcc_lse)]
    HsiClockInUse,
    /// LSE selection did not acknowledge within the CPU-iteration budget.
    #[cfg(rcc_lse)]
    LseClockSwitchTimeout,
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
    /// L011 live-LSI also rejects RTC reset/lost access, HSE selection, and
    /// raw-HSI PSC1 divisors below 98, even when calendar/AWT is idle.
    RtcClockInUse,
    /// The RTC configuration bus gate did not acknowledge enable.
    RtcClockGateTimeout,
    /// The FLASH configuration clock could not be enabled.
    /// L011 live-LSI also uses this for asserted reset or lost Flash access.
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
    /// Requested source, retained configuration or bus dividers did not read back.
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
    #[cfg(rcc_lse)]
    let lse_target = (config.sys == Sysclk::LSE).then(|| LseSysclkSnapshot {
        sources: old_sources,
        hsi: old_hsi,
        hse: old_hse,
        lsi: old_lsi,
        lse: old_lse,
        routes: r.cr2().read(),
        interrupts: r.ier().read(),
        mco: r.mco().read(),
    });
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    let lsi_target = (config.sys == Sysclk::LSI).then(|| LsiSysclkSnapshot {
        clock: old_clock,
        sources: old_sources,
        hsi: old_hsi,
        hse: old_hse,
        lsi: old_lsi,
        lse: old_lse,
        routes: r.cr2().read(),
        interrupts: r.ier().read(),
        mco: r.mco().read(),
        factory: unsafe { core::ptr::read_volatile(crate::RCC_LSI_FACTORY_TRIM_ADDRESS as *const u16) },
        #[cfg(rcc_cw32l011_v1)]
        flash: None,
    });
    #[cfg(all(rcc_cw32l011_v1, rcc_lsi_sysclk))]
    let mut lsi_target = lsi_target;
    #[cfg(all(rcc_cw32l011_v1, rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        // Classify the immutable original entry before generic readiness errors.
        // Only inherited LSE is relevant to the new HSE-off slice's fault check.
        entry.post_observation()?;
        entry.admit()?;
    }
    let monitor_hse = config.hse.is_some() || old_sources.hseen() || old_sources.hseccs();
    let monitor_lse = old_sources.lseen() || old_sources.lseccs();
    #[cfg(rcc_lse)]
    let monitor_lse = monitor_lse || config.lse.is_some();
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

    // Phase expectations are separate from the immutable original observation.
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    let (mut expected_clock, mut expected_sources, mut expected_hsi, mut expected_hsi_stable,
         mut expected_wait) = (old_clock, old_sources, old_hsi, old_hsi.stable(), None);
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    let (expected_hse, expected_hse_stable) = (old_hse, old_hse.stable());
    #[cfg(all(rcc_cw32l010_v1, rcc_lsi_sysclk))]
    let (mut expected_hse, mut expected_hse_stable) = (expected_hse, expected_hse_stable);
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        entry.admit()?;
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
        let changing_hsi = needs_trim || !old_sources.hsien() || !old_hsi.stable();
        if changing_hsi && (entry.mco.source() == 3 || entry.interrupts.hsirdy()) {
            return Err(Error::HsiClockInUse);
        }
    }

    let live_lsi = false;
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    let live_lsi = live_lsi || lsi_target.is_some();
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    {
        expected_clock.set_key(0);
        expected_sources.set_key(0);
    }

    // Native central rejection precedes the first configuration-gate write.
    // This phase inspects only configuration domains, never GPIO/timer work.
    #[cfg(rcc_lse)]
    let lse_admission = config
        .lse
        .map(|c| super::lse::preflight(c, cs, false))
        .transpose()?;

    // Inspect actual central gates, restore their incoming state, never reset.
    // SOURCE owns RTC/AWT's raw clock even when the calendar START bit is zero.
    #[cfg(rcc_lse)]
    if lse_target.is_some()
        && <crate::peripherals::RTC as crate::rcc::SealedRccPeripheral>::RCC_INFO.reset_asserted()
    {
        return Err(Error::RtcClockInUse);
    }
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        if <crate::peripherals::RTC as crate::rcc::SealedRccPeripheral>::RCC_INFO.reset_asserted() {
            entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
            return Err(Error::RtcClockInUse);
        }
    }
    let ((rtc_source, rtc_psc1), operational) = <crate::peripherals::RTC as crate::rcc::SealedRccPeripheral>::RCC_INFO
        .inspect_for_init(cs, config.timeout, || {
            let operational = true;
            #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
            let operational = operational && (lsi_target.is_none() || {
                let info = <crate::peripherals::RTC as crate::rcc::SealedRccPeripheral>::RCC_INFO;
                info.is_enabled() && !info.reset_asserted()
            });
            #[cfg(all(rcc_cw32l011_v1, rcc_lsi_sysclk))]
            if !operational {
                return ((0, 0), false);
            }
            let source = pac::RTC.cr1().read().source();
            let psc1 = 0;
            #[cfg(all(rcc_cw32l011_v1, rcc_lsi_sysclk))]
            let psc1 = if lsi_target.is_some() { pac::RTC.psc().read().psc1() } else { psc1 };
            let value = (source, psc1);
            #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
            let operational = operational && (lsi_target.is_none() || {
                let info = <crate::peripherals::RTC as crate::rcc::SealedRccPeripheral>::RCC_INFO;
                info.is_enabled() && !info.reset_asserted()
            });
            (value, operational)
        })
        .map_err(|error| retained_inspection_error(config, error))?;
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    let operational = operational && (lsi_target.is_none() || !<crate::peripherals::RTC as crate::rcc::SealedRccPeripheral>::RCC_INFO.reset_asserted());
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if lsi_target.is_some() && !operational {
        return Err(Error::RtcClockInUse);
    }
    let _ = operational;
    if rtc_source > 3 {
        return Err(Error::RetainedRtcConfiguration);
    }
    if rtc_source == 3 && (needs_trim || !old_sources.hsien() || !old_hsi.stable()) {
        return Err(Error::RtcClockInUse);
    }
    #[cfg(all(rcc_cw32l011_v1, rcc_lsi_sysclk))]
    if lsi_target.is_some() {
        // PSC1 is the retained raw eight-bit value; widen before adding one.
        // The raw HSI ceiling is independent of HSI DIV and public RTC defaults.
        let divisor = u32::from(rtc_psc1) + 1;
        if rtc_source == 1
            || (rtc_source == 3
                && crate::rcc::ClockBounds::hsi(1).maximum().0 > 1_000_000 * divisor)
        {
            return Err(Error::RtcClockInUse);
        }
    }
    let _ = rtc_psc1;
    let preserve_hse = rtc_source == 1;
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
    let retained_hse = if !preserve_hse {
        Ok(true)
    } else if !old_sources.hseen() || !old_hse.stable() {
        Ok(false)
    } else {
        crate::rcc_hse_pins_match(old_hse.mode(), config.timeout, cs,
            #[cfg(all(rcc_cw32l010_v1, rcc_lsi_sysclk))] lsi_target.is_some(),
        )
    };
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        if let Err(error @ Error::LsiConfigurationGateTimeout { .. }) = retained_hse {
            return Err(error);
        }
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
    if !retained_hse? {
        return Err(Error::HseClockInUse);
    }
    if let Some(hse) = config.hse {
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
        let declared_hse = if !preserve_hse {
            Ok(true)
        } else {
            match hse_parameters_match(hse) {
                Ok(true) => crate::rcc_hse_pins_match(hse.mode == HseMode::Bypass, config.timeout, cs,
                    #[cfg(all(rcc_cw32l010_v1, rcc_lsi_sysclk))] lsi_target.is_some(),
                ),
                result => result,
            }
        };
        #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
        if let Some(entry) = lsi_target {
            if let Err(error @ Error::LsiConfigurationGateTimeout { .. }) = declared_hse {
                return Err(error);
            }
            entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
        }
        if !declared_hse? {
            return Err(Error::HseClockInUse);
        }
    }
    #[cfg(rcc_lse)]
    if lse_target.is_some()
        && <crate::peripherals::ADC as crate::rcc::SealedRccPeripheral>::RCC_INFO.reset_asserted()
    {
        return Err(Error::AdcClockInUse);
    }
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        if <crate::peripherals::ADC as crate::rcc::SealedRccPeripheral>::RCC_INFO.reset_asserted() {
            entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
            return Err(Error::AdcClockInUse);
        }
    }
    let (adc_enabled, operational) = <crate::peripherals::ADC as crate::rcc::SealedRccPeripheral>::RCC_INFO
        .inspect_for_init(cs, config.timeout, || {
            let operational = true;
            #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
            let operational = operational && (lsi_target.is_none() || {
                let info = <crate::peripherals::ADC as crate::rcc::SealedRccPeripheral>::RCC_INFO;
                info.is_enabled() && !info.reset_asserted()
            });
            #[cfg(all(rcc_cw32l011_v1, rcc_lsi_sysclk))]
            if !operational {
                return (false, false);
            }
            let value = pac::ADC.cr().read().en();
            #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
            let operational = operational && (lsi_target.is_none() || {
                let info = <crate::peripherals::ADC as crate::rcc::SealedRccPeripheral>::RCC_INFO;
                info.is_enabled() && !info.reset_asserted()
            });
            (value, operational)
        })
        .map_err(|error| retained_inspection_error(config, error))?;
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    let operational = operational && (lsi_target.is_none() || !<crate::peripherals::ADC as crate::rcc::SealedRccPeripheral>::RCC_INFO.reset_asserted());
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if lsi_target.is_some() && !operational {
        return Err(Error::AdcClockInUse);
    }
    let _ = operational;
    if adc_enabled {
        return Err(Error::AdcClockInUse);
    }
    // LVD and both comparators share the actual VC configuration gate. BGR
    // lives in ADC; none of these reads changes BGR, TSEN or reference selection.
    #[cfg(rcc_lse)]
    if lse_target.is_some()
        && <crate::peripherals::VC1 as crate::rcc::SealedRccPeripheral>::RCC_INFO.reset_asserted()
    {
        return Err(Error::ComparatorClockInUse);
    }
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        if <crate::peripherals::VC1 as crate::rcc::SealedRccPeripheral>::RCC_INFO.reset_asserted() {
            entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
            return Err(Error::ComparatorClockInUse);
        }
    }
    let ((lvd0, lvd1, vc10, vc11, vc20, vc21), operational) =
        <crate::peripherals::VC1 as crate::rcc::SealedRccPeripheral>::RCC_INFO
            .inspect_for_init(cs, config.timeout, || {
                let operational = true;
                #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
                let operational = operational && (lsi_target.is_none() || {
                    let info = <crate::peripherals::VC1 as crate::rcc::SealedRccPeripheral>::RCC_INFO;
                    info.is_enabled() && !info.reset_asserted()
                });
                #[cfg(all(rcc_cw32l011_v1, rcc_lsi_sysclk))]
                if !operational {
                    return ((Default::default(), Default::default(), Default::default(),
                             Default::default(), Default::default(), Default::default()), false);
                }
                let values = (
                    pac::LVD.cr0().read(),
                    pac::LVD.cr1().read(),
                    pac::VC1.cr0().read(),
                    pac::VC1.cr1().read(),
                    pac::VC2.cr0().read(),
                    pac::VC2.cr1().read(),
                );
                #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
                let operational = operational && (lsi_target.is_none() || {
                    let info = <crate::peripherals::VC1 as crate::rcc::SealedRccPeripheral>::RCC_INFO;
                    info.is_enabled() && !info.reset_asserted()
                });
                (values, operational)
            })
            .map_err(|error| retained_inspection_error(config, error))?;
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    let operational = operational && (lsi_target.is_none() || !<crate::peripherals::VC1 as crate::rcc::SealedRccPeripheral>::RCC_INFO.reset_asserted());
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if lsi_target.is_some() && !operational {
        return Err(Error::ComparatorClockInUse);
    }
    let _ = operational;
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
    // Freeze the FIRST request classification from entry, including automatic
    // clients with LSIEN=0. Later STABLE progress must not erase these checks.
    #[cfg(rcc_lse)]
    let first_lsi_request = lse_target.is_some() && (needs_trim || needs_lsi) && !old_lsi.stable();
    #[cfg(rcc_lse)]
    if let Some(entry) = lse_target {
        entry.hsi_ownership(trim)?;
        if first_lsi_request {
            entry.admit_first_lsi_request(config, cs)?;
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
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
    #[cfg(all(rcc_cw32l011_v1, rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        if <crate::peripherals::FLASH as crate::rcc::SealedRccPeripheral>::RCC_INFO.reset_asserted() {
            entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                         expected_hse, expected_hse_stable, expected_wait)?;
            return Err(Error::FlashClockTimeout);
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
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
    #[cfg(all(rcc_cw32l010_v1, rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        let wait = pac::FLASH.cr2().read().wait();
        if u32::from(wait) > MAX_FLASH_WAIT || wait != entry.routes.flashwait() {
            return Err(Error::InvalidEntryClock);
        }
        expected_wait = Some((wait, wait));
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
    #[cfg(all(rcc_cw32l011_v1, rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target.as_mut() {
        let flash = LsiSysclkSnapshot::read_flash();
        // Buffer access failure until the complete source/fault observation.
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
        let flash = flash?;
        let wait = flash.wait();
        if u32::from(wait) > MAX_FLASH_WAIT || wait != entry.routes.flashwait() {
            return Err(Error::InvalidEntryClock);
        }
        // Only this separate Flash authority is attached; no source is recaptured.
        entry.flash = Some(flash);
        expected_wait = Some((wait, wait));
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
    let legacy_flash_check = true;
    #[cfg(all(rcc_cw32l011_v1, rcc_lsi_sysclk))]
    let legacy_flash_check = legacy_flash_check && !live_lsi;
    if legacy_flash_check && u32::from(pac::FLASH.cr2().read().wait()) > MAX_FLASH_WAIT {
        return Err(Error::InvalidEntryClock);
    }
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
    #[cfg(all(rcc_cw32l010_v1, rcc_lsi_sysclk))]
    if live_lsi {
    pac::FLASH.cr2().modify(|w| {
        w.set_key(0x5a5a);
        w.set_wait(MAX_FLASH_WAIT as u8);
    });
    }
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        #[cfg(rcc_cw32l011_v1)]
        let flash_write = LsiSysclkSnapshot::write_flash_wait(MAX_FLASH_WAIT as u8);
        // Both authoritative WAIT and its route mirror acknowledge the same owned write.
        let prior_0 = expected_wait.unwrap().0;
        let target_0 = MAX_FLASH_WAIT as u8;
        let mut acknowledged_0 = prior_0 == target_0;
        let prior_1 = expected_wait.unwrap().1;
        let target_1 = MAX_FLASH_WAIT as u8;
        let mut acknowledged_1 = prior_1 == target_1;
        for attempt in 0..config.timeout {
            let lsi_stable = entry.retained(config)?;
            let mut current_clock = r.cr0().read();
            let mut current_sources = r.cr1().read();
            let current_hsi = r.hsi().read();
            let current_hse = r.hse().read();
            let current_hsi_stable = current_hsi.stable();
            let current_hse_stable = current_hse.stable();
            #[cfg(rcc_cw32l010_v1)]
            let current_wait = expected_wait.map(|_| (pac::FLASH.cr2().read().wait(), r.cr2().read().flashwait()));
            #[cfg(rcc_cw32l011_v1)]
            let current_wait = entry.flash_wait(expected_wait);
            #[cfg(rcc_cw32l011_v1)]
            entry.post_observation()?;
            current_clock.set_key(0);
            current_sources.set_key(0);
            if current_sources.0 != expected_sources.0
                || (current_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                    != (expected_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                || current_hsi_stable != expected_hsi_stable
                || (current_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                    != (expected_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                || current_hse_stable != expected_hse_stable
            {
                return Err(Error::ClockConfigurationTimeout);
            }
            #[cfg(rcc_cw32l010_v1)]
            if !lsi_stable { return Err(Error::LsiNotReady); }
            if current_clock.0 != expected_clock.0 {
                return Err(Error::ClockConfigurationTimeout);
            }
            #[cfg(rcc_cw32l011_v1)]
            if !lsi_stable { return Err(Error::LsiNotReady); }
            #[cfg(rcc_cw32l011_v1)]
            let current_wait = flash_write.and(current_wait)?;
            #[cfg(rcc_cw32l011_v1)]
            if current_wait.unwrap().0 != current_wait.unwrap().1 {
                return Err(Error::FlashLatencyTimeout);
            }
            let observed_0 = current_wait.unwrap().0;
            if observed_0 != target_0 && (observed_0 != prior_0 || acknowledged_0) {
                return Err(Error::FlashLatencyTimeout);
            }
            acknowledged_0 |= observed_0 == target_0;
            let observed_1 = current_wait.unwrap().1;
            if observed_1 != target_1 && (observed_1 != prior_1 || acknowledged_1) {
                return Err(Error::FlashLatencyTimeout);
            }
            acknowledged_1 |= observed_1 == target_1;
            if acknowledged_0 && acknowledged_1 { break; }
            if attempt + 1 == config.timeout { return Err(Error::FlashLatencyTimeout); }
            core::hint::spin_loop();
        }
        expected_wait.as_mut().unwrap().0 = target_0;
        expected_wait.as_mut().unwrap().1 = target_1;
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
    if !live_lsi {
        set_flash_latency(MAX_FLASH_WAIT, config.timeout)?;
    } else {
        barrier();
    }
    // The unchanged legal inherited HSIOSC may not yet have factory trim.
    // /8 covers the own-family legal oscillator range even below 1.8 V.
    let guard_hclk = old_clock.hclkprs().max(AHBPrescaler::Div8 as u8);
    let guard_pclk = old_clock.pclkprs().max(APBPrescaler::Div8 as u8);
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
    r.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_hclkprs(guard_hclk);
        w.set_pclkprs(guard_pclk);
    });
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        // Only the two guarded dividers may advance, independently and without reversal.
        let prior_0 = expected_clock.hclkprs();
        let target_0 = guard_hclk;
        let mut acknowledged_0 = prior_0 == target_0;
        let prior_1 = expected_clock.pclkprs();
        let target_1 = guard_pclk;
        let mut acknowledged_1 = prior_1 == target_1;
        for attempt in 0..config.timeout {
            let lsi_stable = entry.retained(config)?;
            let mut current_clock = r.cr0().read();
            let mut current_sources = r.cr1().read();
            let current_hsi = r.hsi().read();
            let current_hse = r.hse().read();
            let current_hsi_stable = current_hsi.stable();
            let current_hse_stable = current_hse.stable();
            #[cfg(rcc_cw32l010_v1)]
            let current_wait = expected_wait.map(|_| (pac::FLASH.cr2().read().wait(), r.cr2().read().flashwait()));
            #[cfg(rcc_cw32l011_v1)]
            let current_wait = entry.flash_wait(expected_wait);
            #[cfg(rcc_cw32l011_v1)]
            entry.post_observation()?;
            current_clock.set_key(0);
            current_sources.set_key(0);
            if current_sources.0 != expected_sources.0
                || (current_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                    != (expected_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                || current_hsi_stable != expected_hsi_stable
                || (current_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                    != (expected_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                || current_hse_stable != expected_hse_stable
            {
                return Err(Error::ClockConfigurationTimeout);
            }
            #[cfg(rcc_cw32l010_v1)]
            if !lsi_stable { return Err(Error::LsiNotReady); }
            let observed_0 = current_clock.hclkprs();
            if observed_0 != target_0 && (observed_0 != prior_0 || acknowledged_0) {
                return Err(Error::ClockConfigurationTimeout);
            }
            acknowledged_0 |= observed_0 == target_0;
            current_clock.set_hclkprs(prior_0);
            let observed_1 = current_clock.pclkprs();
            if observed_1 != target_1 && (observed_1 != prior_1 || acknowledged_1) {
                return Err(Error::ClockConfigurationTimeout);
            }
            acknowledged_1 |= observed_1 == target_1;
            current_clock.set_pclkprs(prior_1);
            if current_clock.0 != expected_clock.0 {
                return Err(Error::ClockConfigurationTimeout);
            }
            #[cfg(rcc_cw32l011_v1)]
            if !lsi_stable { return Err(Error::LsiNotReady); }
            #[cfg(rcc_cw32l011_v1)]
            let current_wait = current_wait?;
            if current_wait != expected_wait {
                return Err(Error::FlashLatencyTimeout);
            }
            if acknowledged_0 && acknowledged_1 { break; }
            if attempt + 1 == config.timeout { return Err(Error::ClockConfigurationTimeout); }
            core::hint::spin_loop();
        }
        expected_clock.set_hclkprs(target_0);
        expected_clock.set_pclkprs(target_1);
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
    if !live_lsi {
    poll(
        || {
            let v = r.cr0().read();
            v.hclkprs() == guard_hclk && v.pclkprs() == guard_pclk
        },
        config.timeout,
        Error::ClockConfigurationTimeout,
    )?;
    }
    barrier();

    // Native preflight includes all CR2, whose FLASHWAIT is the same-function
    // view of the WAIT field we just changed. Prove all ORIGINAL non-WAIT bits,
    // IER, LSE and LSI parameters before recapturing only this owned difference.
    // A monitored source already passed stable-LSI admission before any gate.
    #[cfg(rcc_lse)]
    let lse_admission = if let Some(entry) = lse_target {
        if u32::from(pac::FLASH.cr2().read().wait()) != MAX_FLASH_WAIT {
            return Err(Error::FlashLatencyTimeout);
        }
        entry.before_lse_start(config)?;
        entry.hsi_ownership(trim)?;
        let refreshed = config
            .lse
            .map(|c| super::lse::preflight(c, cs, false))
            .transpose()?;
        entry.before_lse_start(config)?;
        refreshed
    } else {
        lse_admission
    };

    // First leave the incoming source on unchanged HSI under guarded buses.
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
    r.cr1().modify(|w| {
        w.set_key(0x5a5a);
        w.set_hsien(true);
    });
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        // Unchanged HSI may become requested and ready; every other field retains entry identity.
        let prior_0 = expected_sources.hsien();
        let target_0 = true;
        let mut acknowledged_0 = prior_0 == target_0;
        let prior_1 = expected_hsi_stable;
        let target_1 = true;
        let mut acknowledged_1 = prior_1 == target_1;
        for attempt in 0..config.timeout {
            let lsi_stable = entry.retained(config)?;
            let mut current_clock = r.cr0().read();
            let mut current_sources = r.cr1().read();
            let current_hsi = r.hsi().read();
            let current_hse = r.hse().read();
            let current_hsi_stable = current_hsi.stable();
            let current_hse_stable = current_hse.stable();
            #[cfg(rcc_cw32l010_v1)]
            let current_wait = expected_wait.map(|_| (pac::FLASH.cr2().read().wait(), r.cr2().read().flashwait()));
            #[cfg(rcc_cw32l011_v1)]
            let current_wait = entry.flash_wait(expected_wait);
            #[cfg(rcc_cw32l011_v1)]
            entry.post_observation()?;
            let observed_0 = current_sources.hsien();
            if observed_0 != target_0 && (observed_0 != prior_0 || acknowledged_0) {
                return Err(Error::ClockConfigurationTimeout);
            }
            acknowledged_0 |= observed_0 == target_0;
            current_sources.set_hsien(prior_0);
            let observed_1 = current_hsi_stable;
            if observed_1 != target_1 && (observed_1 != prior_1 || acknowledged_1) {
                return Err(Error::ClockConfigurationTimeout);
            }
            acknowledged_1 |= observed_1 == target_1;
            current_clock.set_key(0);
            current_sources.set_key(0);
            if current_sources.0 != expected_sources.0
                || (current_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                    != (expected_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                || (current_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                    != (expected_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                || current_hse_stable != expected_hse_stable
            {
                return Err(Error::ClockConfigurationTimeout);
            }
            #[cfg(rcc_cw32l010_v1)]
            if !lsi_stable { return Err(Error::LsiNotReady); }
            if current_clock.0 != expected_clock.0 {
                return Err(Error::ClockConfigurationTimeout);
            }
            #[cfg(rcc_cw32l011_v1)]
            if !lsi_stable { return Err(Error::LsiNotReady); }
            #[cfg(rcc_cw32l011_v1)]
            let current_wait = current_wait?;
            if current_wait != expected_wait {
                return Err(Error::FlashLatencyTimeout);
            }
            if acknowledged_0 && acknowledged_1 { break; }
            if attempt + 1 == config.timeout { return Err(Error::HsiTimeout); }
            core::hint::spin_loop();
        }
        expected_sources.set_hsien(target_0);
        expected_hsi_stable = target_1;
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
    if !live_lsi {
    poll_clock(
        config.timeout,
        Error::HsiTimeout,
        monitor_hse,
        monitor_lse,
        || r.cr1().read().hsien() && r.hsi().read().stable(),
    )?;
    }
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
    r.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_sysclk(ClockSource::Hsi);
    });
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        // Only the acknowledged entry selector or requested HSI selector is admissible.
        let prior_0 = expected_clock.sysclk();
        let target_0 = ClockSource::Hsi;
        let mut acknowledged_0 = prior_0 == target_0;
        for attempt in 0..config.timeout {
            let lsi_stable = entry.retained(config)?;
            let mut current_clock = r.cr0().read();
            let mut current_sources = r.cr1().read();
            let current_hsi = r.hsi().read();
            let current_hse = r.hse().read();
            let current_hsi_stable = current_hsi.stable();
            let current_hse_stable = current_hse.stable();
            #[cfg(rcc_cw32l010_v1)]
            let current_wait = expected_wait.map(|_| (pac::FLASH.cr2().read().wait(), r.cr2().read().flashwait()));
            #[cfg(rcc_cw32l011_v1)]
            let current_wait = entry.flash_wait(expected_wait);
            #[cfg(rcc_cw32l011_v1)]
            entry.post_observation()?;
            current_clock.set_key(0);
            current_sources.set_key(0);
            if current_sources.0 != expected_sources.0
                || (current_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                    != (expected_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                || current_hsi_stable != expected_hsi_stable
                || (current_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                    != (expected_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                || current_hse_stable != expected_hse_stable
            {
                return Err(Error::ClockConfigurationTimeout);
            }
            #[cfg(rcc_cw32l010_v1)]
            if !lsi_stable { return Err(Error::LsiNotReady); }
            let observed_0 = current_clock.sysclk();
            if observed_0 != target_0 && (observed_0 != prior_0 || acknowledged_0) {
                return Err(Error::ClockConfigurationTimeout);
            }
            acknowledged_0 |= observed_0 == target_0;
            current_clock.set_sysclk(prior_0);
            if current_clock.0 != expected_clock.0 {
                return Err(Error::ClockConfigurationTimeout);
            }
            #[cfg(rcc_cw32l011_v1)]
            if !lsi_stable { return Err(Error::LsiNotReady); }
            #[cfg(rcc_cw32l011_v1)]
            let current_wait = current_wait?;
            if current_wait != expected_wait {
                return Err(Error::FlashLatencyTimeout);
            }
            if acknowledged_0 { break; }
            if attempt + 1 == config.timeout { return Err(Error::ClockSwitchTimeout); }
            core::hint::spin_loop();
        }
        expected_clock.set_sysclk(target_0);
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
    if !live_lsi {
    poll_clock(
        config.timeout,
        Error::ClockSwitchTimeout,
        monitor_hse,
        monitor_lse,
        || r.cr0().read().sysclk() == ClockSource::Hsi,
    )?;
    }
    barrier();
    if needs_trim || needs_lsi {
        #[cfg(rcc_lse)]
        if let Some(entry) = lse_target {
            check_external_faults(monitor_hse, monitor_lse)?;
            entry.before_lse_start(config)?;
            if entry.lsi.stable() && !r.lsi().read().stable() {
                return Err(Error::LsiTimeout);
            }
        }
        #[cfg(rcc_lse)]
        if first_lsi_request {
            // Repeat selectors/resets/IRQ/analog ownership at the use edge;
            // use the latched entry classification even if LSI is now stable.
            lse_target.unwrap().admit_first_lsi_request(config, cs)?;
        }
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
        if !live_lsi {
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
    }
    if needs_trim {
        // Automatic switching must not select HSI while stopped for retrim.
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_clkccs(false);
        });
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        // Only temporary clock-security disable may acknowledge here.
        let prior_0 = expected_sources.clkccs();
        let target_0 = false;
        let mut acknowledged_0 = prior_0 == target_0;
        for attempt in 0..config.timeout {
            let lsi_stable = entry.retained(config)?;
            let mut current_clock = r.cr0().read();
            let mut current_sources = r.cr1().read();
            let current_hsi = r.hsi().read();
            let current_hse = r.hse().read();
            let current_hsi_stable = current_hsi.stable();
            let current_hse_stable = current_hse.stable();
            #[cfg(rcc_cw32l010_v1)]
            let current_wait = expected_wait.map(|_| (pac::FLASH.cr2().read().wait(), r.cr2().read().flashwait()));
            #[cfg(rcc_cw32l011_v1)]
            let current_wait = entry.flash_wait(expected_wait);
            #[cfg(rcc_cw32l011_v1)]
            entry.post_observation()?;
            let observed_0 = current_sources.clkccs();
            if observed_0 != target_0 && (observed_0 != prior_0 || acknowledged_0) {
                return Err(Error::ClockConfigurationTimeout);
            }
            acknowledged_0 |= observed_0 == target_0;
            current_sources.set_clkccs(prior_0);
            current_clock.set_key(0);
            current_sources.set_key(0);
            if current_sources.0 != expected_sources.0
                || (current_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                    != (expected_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                || current_hsi_stable != expected_hsi_stable
                || (current_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                    != (expected_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                || current_hse_stable != expected_hse_stable
            {
                return Err(Error::ClockConfigurationTimeout);
            }
            #[cfg(rcc_cw32l010_v1)]
            if !lsi_stable { return Err(Error::LsiNotReady); }
            if current_clock.0 != expected_clock.0 {
                return Err(Error::ClockConfigurationTimeout);
            }
            #[cfg(rcc_cw32l011_v1)]
            if !lsi_stable { return Err(Error::LsiNotReady); }
            #[cfg(rcc_cw32l011_v1)]
            let current_wait = current_wait?;
            if current_wait != expected_wait {
                return Err(Error::FlashLatencyTimeout);
            }
            if acknowledged_0 { break; }
            if attempt + 1 == config.timeout { return Err(Error::ClockConfigurationTimeout); }
            core::hint::spin_loop();
        }
        expected_sources.set_clkccs(target_0);
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
    if !live_lsi {
        poll(
            || !r.cr1().read().clkccs(),
            config.timeout,
            Error::ClockConfigurationTimeout,
        )?;
    }
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
        r.cr0().modify(|w| {
            w.set_key(0x5a5a);
            w.set_sysclk(ClockSource::Lsi);
        });
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        // The already-live LSI bridge changes only the system selector.
        let prior_0 = expected_clock.sysclk();
        let target_0 = ClockSource::Lsi;
        let mut acknowledged_0 = prior_0 == target_0;
        for attempt in 0..config.timeout {
            let lsi_stable = entry.retained(config)?;
            let mut current_clock = r.cr0().read();
            let mut current_sources = r.cr1().read();
            let current_hsi = r.hsi().read();
            let current_hse = r.hse().read();
            let current_hsi_stable = current_hsi.stable();
            let current_hse_stable = current_hse.stable();
            #[cfg(rcc_cw32l010_v1)]
            let current_wait = expected_wait.map(|_| (pac::FLASH.cr2().read().wait(), r.cr2().read().flashwait()));
            #[cfg(rcc_cw32l011_v1)]
            let current_wait = entry.flash_wait(expected_wait);
            #[cfg(rcc_cw32l011_v1)]
            entry.post_observation()?;
            current_clock.set_key(0);
            current_sources.set_key(0);
            if current_sources.0 != expected_sources.0
                || (current_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                    != (expected_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                || current_hsi_stable != expected_hsi_stable
                || (current_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                    != (expected_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                || current_hse_stable != expected_hse_stable
            {
                return Err(Error::ClockConfigurationTimeout);
            }
            #[cfg(rcc_cw32l010_v1)]
            if !lsi_stable { return Err(Error::LsiNotReady); }
            let observed_0 = current_clock.sysclk();
            if observed_0 != target_0 && (observed_0 != prior_0 || acknowledged_0) {
                return Err(Error::ClockConfigurationTimeout);
            }
            acknowledged_0 |= observed_0 == target_0;
            current_clock.set_sysclk(prior_0);
            if current_clock.0 != expected_clock.0 {
                return Err(Error::ClockConfigurationTimeout);
            }
            #[cfg(rcc_cw32l011_v1)]
            if !lsi_stable { return Err(Error::LsiNotReady); }
            #[cfg(rcc_cw32l011_v1)]
            let current_wait = current_wait?;
            if current_wait != expected_wait {
                return Err(Error::FlashLatencyTimeout);
            }
            if acknowledged_0 { break; }
            if attempt + 1 == config.timeout { return Err(Error::TemporaryClockSwitchTimeout); }
            core::hint::spin_loop();
        }
        expected_clock.set_sysclk(target_0);
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
    if !live_lsi {
        poll_clock(
            config.timeout,
            Error::TemporaryClockSwitchTimeout,
            monitor_hse,
            monitor_lse,
            || r.cr0().read().sysclk() == ClockSource::Lsi,
        )?;
    }
        barrier();
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_hsien(false);
        });
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        // HSI request and readiness may only fall while selected live LSI stays strict.
        let prior_0 = expected_sources.hsien();
        let target_0 = false;
        let mut acknowledged_0 = prior_0 == target_0;
        let prior_1 = expected_hsi_stable;
        let target_1 = false;
        let mut acknowledged_1 = prior_1 == target_1;
        for attempt in 0..config.timeout {
            let lsi_stable = entry.retained(config)?;
            let mut current_clock = r.cr0().read();
            let mut current_sources = r.cr1().read();
            let current_hsi = r.hsi().read();
            let current_hse = r.hse().read();
            let current_hsi_stable = current_hsi.stable();
            let current_hse_stable = current_hse.stable();
            #[cfg(rcc_cw32l010_v1)]
            let current_wait = expected_wait.map(|_| (pac::FLASH.cr2().read().wait(), r.cr2().read().flashwait()));
            #[cfg(rcc_cw32l011_v1)]
            let current_wait = entry.flash_wait(expected_wait);
            #[cfg(rcc_cw32l011_v1)]
            entry.post_observation()?;
            let observed_0 = current_sources.hsien();
            if observed_0 != target_0 && (observed_0 != prior_0 || acknowledged_0) {
                return Err(Error::ClockConfigurationTimeout);
            }
            acknowledged_0 |= observed_0 == target_0;
            current_sources.set_hsien(prior_0);
            let observed_1 = current_hsi_stable;
            if observed_1 != target_1 && (observed_1 != prior_1 || acknowledged_1) {
                return Err(Error::ClockConfigurationTimeout);
            }
            acknowledged_1 |= observed_1 == target_1;
            current_clock.set_key(0);
            current_sources.set_key(0);
            if current_sources.0 != expected_sources.0
                || (current_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                    != (expected_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                || (current_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                    != (expected_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                || current_hse_stable != expected_hse_stable
            {
                return Err(Error::ClockConfigurationTimeout);
            }
            #[cfg(rcc_cw32l010_v1)]
            if !lsi_stable { return Err(Error::LsiNotReady); }
            if current_clock.0 != expected_clock.0 {
                return Err(Error::ClockConfigurationTimeout);
            }
            #[cfg(rcc_cw32l011_v1)]
            if !lsi_stable { return Err(Error::LsiNotReady); }
            #[cfg(rcc_cw32l011_v1)]
            let current_wait = current_wait?;
            if current_wait != expected_wait {
                return Err(Error::FlashLatencyTimeout);
            }
            if acknowledged_0 && acknowledged_1 { break; }
            if attempt + 1 == config.timeout { return Err(Error::HsiStopTimeout); }
            core::hint::spin_loop();
        }
        expected_sources.set_hsien(target_0);
        expected_hsi_stable = target_1;
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
    if !live_lsi {
        poll_clock(
            config.timeout,
            Error::HsiStopTimeout,
            monitor_hse,
            monitor_lse,
            || !r.cr1().read().hsien() && !r.hsi().read().stable(),
        )?;
    }
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
        r.hsi().modify(|w| {
            w.set_trim(trim);
            w.set_div(config.hsi.div as u8);
        });
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        // Factory TRIM and requested DIV acknowledge independently only while HSI is stopped.
        let prior_0 = expected_hsi.trim();
        let target_0 = trim;
        let mut acknowledged_0 = prior_0 == target_0;
        let prior_1 = expected_hsi.div();
        let target_1 = config.hsi.div as u8;
        let mut acknowledged_1 = prior_1 == target_1;
        for attempt in 0..config.timeout {
            let lsi_stable = entry.retained(config)?;
            let mut current_clock = r.cr0().read();
            let mut current_sources = r.cr1().read();
            let mut current_hsi = r.hsi().read();
            let current_hse = r.hse().read();
            let current_hsi_stable = current_hsi.stable();
            let current_hse_stable = current_hse.stable();
            #[cfg(rcc_cw32l010_v1)]
            let current_wait = expected_wait.map(|_| (pac::FLASH.cr2().read().wait(), r.cr2().read().flashwait()));
            #[cfg(rcc_cw32l011_v1)]
            let current_wait = entry.flash_wait(expected_wait);
            #[cfg(rcc_cw32l011_v1)]
            entry.post_observation()?;
            let observed_0 = current_hsi.trim();
            if observed_0 != target_0 && (observed_0 != prior_0 || acknowledged_0) {
                return Err(Error::ClockConfigurationTimeout);
            }
            acknowledged_0 |= observed_0 == target_0;
            current_hsi.set_trim(prior_0);
            let observed_1 = current_hsi.div();
            if observed_1 != target_1 && (observed_1 != prior_1 || acknowledged_1) {
                return Err(Error::ClockConfigurationTimeout);
            }
            acknowledged_1 |= observed_1 == target_1;
            current_hsi.set_div(prior_1);
            current_clock.set_key(0);
            current_sources.set_key(0);
            if current_sources.0 != expected_sources.0
                || (current_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                    != (expected_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                || current_hsi_stable != expected_hsi_stable
                || (current_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                    != (expected_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                || current_hse_stable != expected_hse_stable
            {
                return Err(Error::ClockConfigurationTimeout);
            }
            #[cfg(rcc_cw32l010_v1)]
            if !lsi_stable { return Err(Error::LsiNotReady); }
            if current_clock.0 != expected_clock.0 {
                return Err(Error::ClockConfigurationTimeout);
            }
            #[cfg(rcc_cw32l011_v1)]
            if !lsi_stable { return Err(Error::LsiNotReady); }
            #[cfg(rcc_cw32l011_v1)]
            let current_wait = current_wait?;
            if current_wait != expected_wait {
                return Err(Error::FlashLatencyTimeout);
            }
            if acknowledged_0 && acknowledged_1 { break; }
            if attempt + 1 == config.timeout { return Err(Error::ClockConfigurationTimeout); }
            core::hint::spin_loop();
        }
        expected_hsi.set_trim(target_0);
        expected_hsi.set_div(target_1);
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
    if !live_lsi {
        poll(
            || {
                let v = r.hsi().read();
                v.trim() == trim && v.div() == config.hsi.div as u8
            },
            config.timeout,
            Error::ClockConfigurationTimeout,
        )?;
    }
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_hsien(true);
        });
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        // Restarted HSI retains the exact acknowledged factory parameters.
        let prior_0 = expected_sources.hsien();
        let target_0 = true;
        let mut acknowledged_0 = prior_0 == target_0;
        let prior_1 = expected_hsi_stable;
        let target_1 = true;
        let mut acknowledged_1 = prior_1 == target_1;
        for attempt in 0..config.timeout {
            let lsi_stable = entry.retained(config)?;
            let mut current_clock = r.cr0().read();
            let mut current_sources = r.cr1().read();
            let current_hsi = r.hsi().read();
            let current_hse = r.hse().read();
            let current_hsi_stable = current_hsi.stable();
            let current_hse_stable = current_hse.stable();
            #[cfg(rcc_cw32l010_v1)]
            let current_wait = expected_wait.map(|_| (pac::FLASH.cr2().read().wait(), r.cr2().read().flashwait()));
            #[cfg(rcc_cw32l011_v1)]
            let current_wait = entry.flash_wait(expected_wait);
            #[cfg(rcc_cw32l011_v1)]
            entry.post_observation()?;
            let observed_0 = current_sources.hsien();
            if observed_0 != target_0 && (observed_0 != prior_0 || acknowledged_0) {
                return Err(Error::ClockConfigurationTimeout);
            }
            acknowledged_0 |= observed_0 == target_0;
            current_sources.set_hsien(prior_0);
            let observed_1 = current_hsi_stable;
            if observed_1 != target_1 && (observed_1 != prior_1 || acknowledged_1) {
                return Err(Error::ClockConfigurationTimeout);
            }
            acknowledged_1 |= observed_1 == target_1;
            current_clock.set_key(0);
            current_sources.set_key(0);
            if current_sources.0 != expected_sources.0
                || (current_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                    != (expected_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                || (current_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                    != (expected_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                || current_hse_stable != expected_hse_stable
            {
                return Err(Error::ClockConfigurationTimeout);
            }
            #[cfg(rcc_cw32l010_v1)]
            if !lsi_stable { return Err(Error::LsiNotReady); }
            if current_clock.0 != expected_clock.0 {
                return Err(Error::ClockConfigurationTimeout);
            }
            #[cfg(rcc_cw32l011_v1)]
            if !lsi_stable { return Err(Error::LsiNotReady); }
            #[cfg(rcc_cw32l011_v1)]
            let current_wait = current_wait?;
            if current_wait != expected_wait {
                return Err(Error::FlashLatencyTimeout);
            }
            if acknowledged_0 && acknowledged_1 { break; }
            if attempt + 1 == config.timeout { return Err(Error::HsiTimeout); }
            core::hint::spin_loop();
        }
        expected_sources.set_hsien(target_0);
        expected_hsi_stable = target_1;
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
    if !live_lsi {
        poll_clock(
            config.timeout,
            Error::HsiTimeout,
            monitor_hse,
            monitor_lse,
            || r.cr1().read().hsien() && r.hsi().read().stable(),
        )?;
    }
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
        r.cr0().modify(|w| {
            w.set_key(0x5a5a);
            w.set_sysclk(ClockSource::Hsi);
        });
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        // Return from live LSI only to the verified calibrated HSI.
        let prior_0 = expected_clock.sysclk();
        let target_0 = ClockSource::Hsi;
        let mut acknowledged_0 = prior_0 == target_0;
        for attempt in 0..config.timeout {
            let lsi_stable = entry.retained(config)?;
            let mut current_clock = r.cr0().read();
            let mut current_sources = r.cr1().read();
            let current_hsi = r.hsi().read();
            let current_hse = r.hse().read();
            let current_hsi_stable = current_hsi.stable();
            let current_hse_stable = current_hse.stable();
            #[cfg(rcc_cw32l010_v1)]
            let current_wait = expected_wait.map(|_| (pac::FLASH.cr2().read().wait(), r.cr2().read().flashwait()));
            #[cfg(rcc_cw32l011_v1)]
            let current_wait = entry.flash_wait(expected_wait);
            #[cfg(rcc_cw32l011_v1)]
            entry.post_observation()?;
            current_clock.set_key(0);
            current_sources.set_key(0);
            if current_sources.0 != expected_sources.0
                || (current_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                    != (expected_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                || current_hsi_stable != expected_hsi_stable
                || (current_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                    != (expected_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                || current_hse_stable != expected_hse_stable
            {
                return Err(Error::ClockConfigurationTimeout);
            }
            #[cfg(rcc_cw32l010_v1)]
            if !lsi_stable { return Err(Error::LsiNotReady); }
            let observed_0 = current_clock.sysclk();
            if observed_0 != target_0 && (observed_0 != prior_0 || acknowledged_0) {
                return Err(Error::ClockConfigurationTimeout);
            }
            acknowledged_0 |= observed_0 == target_0;
            current_clock.set_sysclk(prior_0);
            if current_clock.0 != expected_clock.0 {
                return Err(Error::ClockConfigurationTimeout);
            }
            #[cfg(rcc_cw32l011_v1)]
            if !lsi_stable { return Err(Error::LsiNotReady); }
            #[cfg(rcc_cw32l011_v1)]
            let current_wait = current_wait?;
            if current_wait != expected_wait {
                return Err(Error::FlashLatencyTimeout);
            }
            if acknowledged_0 { break; }
            if attempt + 1 == config.timeout { return Err(Error::ClockSwitchTimeout); }
            core::hint::spin_loop();
        }
        expected_clock.set_sysclk(target_0);
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
    if !live_lsi {
        poll_clock(
            config.timeout,
            Error::ClockSwitchTimeout,
            monitor_hse,
            monitor_lse,
            || r.cr0().read().sysclk() == ClockSource::Hsi,
        )?;
    }
        barrier();
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_clkccs(old_sources.clkccs());
        });
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        // Restore only the original clock-security policy; LSIEN remains asserted.
        let prior_0 = expected_sources.clkccs();
        let target_0 = old_sources.clkccs();
        let mut acknowledged_0 = prior_0 == target_0;
        for attempt in 0..config.timeout {
            let lsi_stable = entry.retained(config)?;
            let mut current_clock = r.cr0().read();
            let mut current_sources = r.cr1().read();
            let current_hsi = r.hsi().read();
            let current_hse = r.hse().read();
            let current_hsi_stable = current_hsi.stable();
            let current_hse_stable = current_hse.stable();
            #[cfg(rcc_cw32l010_v1)]
            let current_wait = expected_wait.map(|_| (pac::FLASH.cr2().read().wait(), r.cr2().read().flashwait()));
            #[cfg(rcc_cw32l011_v1)]
            let current_wait = entry.flash_wait(expected_wait);
            #[cfg(rcc_cw32l011_v1)]
            entry.post_observation()?;
            let observed_0 = current_sources.clkccs();
            if observed_0 != target_0 && (observed_0 != prior_0 || acknowledged_0) {
                return Err(Error::ClockConfigurationTimeout);
            }
            acknowledged_0 |= observed_0 == target_0;
            current_sources.set_clkccs(prior_0);
            current_clock.set_key(0);
            current_sources.set_key(0);
            if current_sources.0 != expected_sources.0
                || (current_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                    != (expected_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                || current_hsi_stable != expected_hsi_stable
                || (current_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                    != (expected_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                || current_hse_stable != expected_hse_stable
            {
                return Err(Error::ClockConfigurationTimeout);
            }
            #[cfg(rcc_cw32l010_v1)]
            if !lsi_stable { return Err(Error::LsiNotReady); }
            if current_clock.0 != expected_clock.0 {
                return Err(Error::ClockConfigurationTimeout);
            }
            #[cfg(rcc_cw32l011_v1)]
            if !lsi_stable { return Err(Error::LsiNotReady); }
            #[cfg(rcc_cw32l011_v1)]
            let current_wait = current_wait?;
            if current_wait != expected_wait {
                return Err(Error::FlashLatencyTimeout);
            }
            if acknowledged_0 { break; }
            if attempt + 1 == config.timeout { return Err(Error::ClockConfigurationTimeout); }
            core::hint::spin_loop();
        }
        expected_sources.set_clkccs(target_0);
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }

    } else {
        // Both own §4.5.2 allow live divider changes, without changing TRIM.
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
        r.hsi().modify(|w| w.set_div(config.hsi.div as u8));
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        // Legal live HSI DIV-only update preserves factory TRIM and asserted readiness.
        let prior_0 = expected_hsi.div();
        let target_0 = config.hsi.div as u8;
        let mut acknowledged_0 = prior_0 == target_0;
        for attempt in 0..config.timeout {
            let lsi_stable = entry.retained(config)?;
            let mut current_clock = r.cr0().read();
            let mut current_sources = r.cr1().read();
            let mut current_hsi = r.hsi().read();
            let current_hse = r.hse().read();
            let current_hsi_stable = current_hsi.stable();
            let current_hse_stable = current_hse.stable();
            #[cfg(rcc_cw32l010_v1)]
            let current_wait = expected_wait.map(|_| (pac::FLASH.cr2().read().wait(), r.cr2().read().flashwait()));
            #[cfg(rcc_cw32l011_v1)]
            let current_wait = entry.flash_wait(expected_wait);
            #[cfg(rcc_cw32l011_v1)]
            entry.post_observation()?;
            let observed_0 = current_hsi.div();
            if observed_0 != target_0 && (observed_0 != prior_0 || acknowledged_0) {
                return Err(Error::ClockConfigurationTimeout);
            }
            acknowledged_0 |= observed_0 == target_0;
            current_hsi.set_div(prior_0);
            current_clock.set_key(0);
            current_sources.set_key(0);
            if current_sources.0 != expected_sources.0
                || (current_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                    != (expected_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                || current_hsi_stable != expected_hsi_stable
                || (current_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                    != (expected_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                || current_hse_stable != expected_hse_stable
            {
                return Err(Error::ClockConfigurationTimeout);
            }
            #[cfg(rcc_cw32l010_v1)]
            if !lsi_stable { return Err(Error::LsiNotReady); }
            if current_clock.0 != expected_clock.0 {
                return Err(Error::ClockConfigurationTimeout);
            }
            #[cfg(rcc_cw32l011_v1)]
            if !lsi_stable { return Err(Error::LsiNotReady); }
            #[cfg(rcc_cw32l011_v1)]
            let current_wait = current_wait?;
            if current_wait != expected_wait {
                return Err(Error::FlashLatencyTimeout);
            }
            if acknowledged_0 { break; }
            if attempt + 1 == config.timeout { return Err(Error::ClockConfigurationTimeout); }
            core::hint::spin_loop();
        }
        expected_hsi.set_div(target_0);
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }

    }
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
    if !live_lsi {
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
    }
    if let Some(hse) = config.hse.filter(|_| !preserve_hse) {
    #[cfg(all(rcc_cw32l010_v1, rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_hseen(false);
        });
    #[cfg(all(rcc_cw32l010_v1, rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        // Optional unowned HSE stops under verified calibrated HSI.
        let prior_0 = expected_sources.hseen();
        let target_0 = false;
        let mut acknowledged_0 = prior_0 == target_0;
        let prior_1 = expected_hse_stable;
        let target_1 = false;
        let mut acknowledged_1 = prior_1 == target_1;
        for attempt in 0..config.timeout {
            let lsi_stable = entry.retained(config)?;
            let mut current_clock = r.cr0().read();
            let mut current_sources = r.cr1().read();
            let current_hsi = r.hsi().read();
            let current_hse = r.hse().read();
            let current_hsi_stable = current_hsi.stable();
            let current_hse_stable = current_hse.stable();
            let current_wait = expected_wait.map(|_| (pac::FLASH.cr2().read().wait(), r.cr2().read().flashwait()));
            let observed_0 = current_sources.hseen();
            if observed_0 != target_0 && (observed_0 != prior_0 || acknowledged_0) {
                return Err(Error::ClockConfigurationTimeout);
            }
            acknowledged_0 |= observed_0 == target_0;
            current_sources.set_hseen(prior_0);
            let observed_1 = current_hse_stable;
            if observed_1 != target_1 && (observed_1 != prior_1 || acknowledged_1) {
                return Err(Error::ClockConfigurationTimeout);
            }
            acknowledged_1 |= observed_1 == target_1;
            current_clock.set_key(0);
            current_sources.set_key(0);
            if current_sources.0 != expected_sources.0
                || (current_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                    != (expected_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                || current_hsi_stable != expected_hsi_stable
                || (current_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                    != (expected_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
            {
                return Err(Error::ClockConfigurationTimeout);
            }
            if !lsi_stable { return Err(Error::LsiNotReady); }
            if current_clock.0 != expected_clock.0 {
                return Err(Error::ClockConfigurationTimeout);
            }
            if current_wait != expected_wait {
                return Err(Error::FlashLatencyTimeout);
            }
            if acknowledged_0 && acknowledged_1 { break; }
            if attempt + 1 == config.timeout { return Err(Error::HseStopTimeout); }
            core::hint::spin_loop();
        }
        expected_sources.set_hseen(target_0);
        expected_hse_stable = target_1;
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
    if !live_lsi {
        poll_clock(
            config.timeout,
            Error::HseStopTimeout,
            monitor_hse,
            monitor_lse,
            || !r.cr1().read().hseen() && !r.hse().read().stable(),
        )?;
    }
    #[cfg(all(rcc_cw32l010_v1, rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
        let configured_pads = crate::rcc_configure_hse_pins(hse.mode == HseMode::Bypass, config.timeout, cs,
            #[cfg(all(rcc_cw32l010_v1, rcc_lsi_sysclk))] lsi_target.is_some(),
        );
        #[cfg(all(rcc_cw32l010_v1, rcc_lsi_sysclk))]
        if let Some(entry) = lsi_target {
            if let Err(error @ (Error::LsiConfigurationGateTimeout { .. } | Error::HsePinConfigurationTimeout)) = configured_pads {
                return Err(error);
            }
            entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
        }
        configured_pads?;
        let detector = hse.detector_count()?;
    #[cfg(all(rcc_cw32l010_v1, rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
        r.hse().modify(|w| {
            w.set_mode(hse.mode == HseMode::Bypass);
            w.set_driver(hse.drive);
            w.set_pdriver(hse.drive);
            w.set_waitcycle(pac::sysctrl::vals::HseWait::Cycles262144);
            w.set_digflt(false);
            w.set_detcnt(detector);
        });
    #[cfg(all(rcc_cw32l010_v1, rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        // Only the six declared HSE fields can progress; HEXENPOL and all other bits stay exact.
        let prior_0 = expected_hse.mode();
        let target_0 = hse.mode == HseMode::Bypass;
        let mut acknowledged_0 = prior_0 == target_0;
        let prior_1 = expected_hse.driver();
        let target_1 = hse.drive;
        let mut acknowledged_1 = prior_1 == target_1;
        let prior_2 = expected_hse.pdriver();
        let target_2 = hse.drive;
        let mut acknowledged_2 = prior_2 == target_2;
        let prior_3 = expected_hse.waitcycle();
        let target_3 = pac::sysctrl::vals::HseWait::Cycles262144;
        let mut acknowledged_3 = prior_3 == target_3;
        let prior_4 = expected_hse.digflt();
        let target_4 = false;
        let mut acknowledged_4 = prior_4 == target_4;
        let prior_5 = expected_hse.detcnt();
        let target_5 = detector;
        let mut acknowledged_5 = prior_5 == target_5;
        for attempt in 0..config.timeout {
            let lsi_stable = entry.retained(config)?;
            let mut current_clock = r.cr0().read();
            let mut current_sources = r.cr1().read();
            let current_hsi = r.hsi().read();
            let mut current_hse = r.hse().read();
            let current_hsi_stable = current_hsi.stable();
            let current_hse_stable = current_hse.stable();
            let current_wait = expected_wait.map(|_| (pac::FLASH.cr2().read().wait(), r.cr2().read().flashwait()));
            let observed_0 = current_hse.mode();
            if observed_0 != target_0 && (observed_0 != prior_0 || acknowledged_0) {
                return Err(Error::ClockConfigurationTimeout);
            }
            acknowledged_0 |= observed_0 == target_0;
            current_hse.set_mode(prior_0);
            let observed_1 = current_hse.driver();
            if observed_1 != target_1 && (observed_1 != prior_1 || acknowledged_1) {
                return Err(Error::ClockConfigurationTimeout);
            }
            acknowledged_1 |= observed_1 == target_1;
            current_hse.set_driver(prior_1);
            let observed_2 = current_hse.pdriver();
            if observed_2 != target_2 && (observed_2 != prior_2 || acknowledged_2) {
                return Err(Error::ClockConfigurationTimeout);
            }
            acknowledged_2 |= observed_2 == target_2;
            current_hse.set_pdriver(prior_2);
            let observed_3 = current_hse.waitcycle();
            if observed_3 != target_3 && (observed_3 != prior_3 || acknowledged_3) {
                return Err(Error::ClockConfigurationTimeout);
            }
            acknowledged_3 |= observed_3 == target_3;
            current_hse.set_waitcycle(prior_3);
            let observed_4 = current_hse.digflt();
            if observed_4 != target_4 && (observed_4 != prior_4 || acknowledged_4) {
                return Err(Error::ClockConfigurationTimeout);
            }
            acknowledged_4 |= observed_4 == target_4;
            current_hse.set_digflt(prior_4);
            let observed_5 = current_hse.detcnt();
            if observed_5 != target_5 && (observed_5 != prior_5 || acknowledged_5) {
                return Err(Error::ClockConfigurationTimeout);
            }
            acknowledged_5 |= observed_5 == target_5;
            current_hse.set_detcnt(prior_5);
            current_clock.set_key(0);
            current_sources.set_key(0);
            if current_sources.0 != expected_sources.0
                || (current_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                    != (expected_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                || current_hsi_stable != expected_hsi_stable
                || (current_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                    != (expected_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                || current_hse_stable != expected_hse_stable
            {
                return Err(Error::ClockConfigurationTimeout);
            }
            if !lsi_stable { return Err(Error::LsiNotReady); }
            if current_clock.0 != expected_clock.0 {
                return Err(Error::ClockConfigurationTimeout);
            }
            if current_wait != expected_wait {
                return Err(Error::FlashLatencyTimeout);
            }
            if acknowledged_0 && acknowledged_1 && acknowledged_2 && acknowledged_3 && acknowledged_4 && acknowledged_5 { break; }
            if attempt + 1 == config.timeout { return Err(Error::ClockConfigurationTimeout); }
            core::hint::spin_loop();
        }
        expected_hse.set_mode(target_0);
        expected_hse.set_driver(target_1);
        expected_hse.set_pdriver(target_2);
        expected_hse.set_waitcycle(target_3);
        expected_hse.set_digflt(target_4);
        expected_hse.set_detcnt(target_5);
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
    if !live_lsi {
        poll(
            || hse_parameters_match(hse).unwrap_or(false),
            config.timeout,
            Error::ClockConfigurationTimeout,
        )?;
    }
    #[cfg(all(rcc_cw32l010_v1, rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_hseen(true);
        });
    #[cfg(all(rcc_cw32l010_v1, rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        // Declared HSE may become ready only with its acknowledged parameter image.
        let prior_0 = expected_sources.hseen();
        let target_0 = true;
        let mut acknowledged_0 = prior_0 == target_0;
        let prior_1 = expected_hse_stable;
        let target_1 = true;
        let mut acknowledged_1 = prior_1 == target_1;
        for attempt in 0..config.timeout {
            let lsi_stable = entry.retained(config)?;
            let mut current_clock = r.cr0().read();
            let mut current_sources = r.cr1().read();
            let current_hsi = r.hsi().read();
            let current_hse = r.hse().read();
            let current_hsi_stable = current_hsi.stable();
            let current_hse_stable = current_hse.stable();
            let current_wait = expected_wait.map(|_| (pac::FLASH.cr2().read().wait(), r.cr2().read().flashwait()));
            let observed_0 = current_sources.hseen();
            if observed_0 != target_0 && (observed_0 != prior_0 || acknowledged_0) {
                return Err(Error::ClockConfigurationTimeout);
            }
            acknowledged_0 |= observed_0 == target_0;
            current_sources.set_hseen(prior_0);
            let observed_1 = current_hse_stable;
            if observed_1 != target_1 && (observed_1 != prior_1 || acknowledged_1) {
                return Err(Error::ClockConfigurationTimeout);
            }
            acknowledged_1 |= observed_1 == target_1;
            current_clock.set_key(0);
            current_sources.set_key(0);
            if current_sources.0 != expected_sources.0
                || (current_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                    != (expected_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                || current_hsi_stable != expected_hsi_stable
                || (current_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                    != (expected_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
            {
                return Err(Error::ClockConfigurationTimeout);
            }
            if !lsi_stable { return Err(Error::LsiNotReady); }
            if current_clock.0 != expected_clock.0 {
                return Err(Error::ClockConfigurationTimeout);
            }
            if current_wait != expected_wait {
                return Err(Error::FlashLatencyTimeout);
            }
            if acknowledged_0 && acknowledged_1 { break; }
            if attempt + 1 == config.timeout { return Err(Error::HseTimeout); }
            core::hint::spin_loop();
        }
        expected_sources.set_hseen(target_0);
        expected_hse_stable = target_1;
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
    if !live_lsi {
        poll_clock(
            config.timeout,
            Error::HseTimeout,
            monitor_hse,
            monitor_lse,
            || r.cr1().read().hseen() && r.hse().read().stable(),
        )?;
    }
    }
    // Restore only the software request; automatic LSI consumers stay active.
    // Never require STABLE to clear when LSIEN is cleared.
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
    if !live_lsi {
    r.cr1().modify(|w| {
        w.set_key(0x5a5a);
        w.set_lsien(old_sources.lsien());
    });
    poll(
        || r.cr1().read().lsien() == old_sources.lsien(),
        config.timeout,
        Error::TemporaryClockRestoreTimeout,
    )?;
    }
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        return finish_lsi_sysclk(config, clocks, entry, trim, cs);
    }
    #[cfg(rcc_lse)]
    if let Some(entry) = lse_target {
        entry.before_lse_start(config)?;
    }
    #[cfg(rcc_lse)]
    if let (Some(lse), Some(admission)) = (config.lse, lse_admission) {
        let admission = if lse_target.is_none() {
            admission.after_owned_flash_wait(MAX_FLASH_WAIT)?
        } else {
            admission
        };
        super::lse::start(lse, admission, cs, false)?;
    }
    #[cfg(rcc_lse)]
    if let Some(entry) = lse_target {
        return finish_lse_sysclk(config, clocks, entry, trim, needs_lsi, cs);
    }
    let sysclk = match config.sys {
        Sysclk::HSI => ClockSource::Hsi,
        Sysclk::HSE => ClockSource::Hse,
        #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
        Sysclk::LSI => unreachable!("live LSI target completed above"),
        #[cfg(rcc_lse)]
        Sysclk::LSE => unreachable!("LSE target completed above"),
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
            || !crate::rcc_hse_pins_match(hse.mode == HseMode::Bypass, config.timeout, cs,
                #[cfg(all(rcc_cw32l010_v1, rcc_lsi_sysclk))] false,
            )?
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

// Existing non-target retained-inspection error classes stay unchanged.
fn retained_inspection_error(
    config: Config,
    error: super::peripheral::ClockInspectionError,
) -> Error {
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if config.sys == Sysclk::LSI {
        return match error {
            super::peripheral::ClockInspectionError::EnableFailed { restore_failed } =>
                Error::LsiConfigurationGateTimeout { enable_failed: true, restore_failed },
            super::peripheral::ClockInspectionError::RestoreFailed =>
                Error::LsiConfigurationGateTimeout { enable_failed: false, restore_failed: true },
        };
    }
    #[cfg(rcc_lse)]
    if config.sys == Sysclk::LSE {
        return lse_target_inspection_error(error);
    }
    let _ = (config, error);
    Error::RetainedClockInspectionTimeout
}

#[cfg(rcc_lse)]
fn lse_target_inspection_error(error: super::peripheral::ClockInspectionError) -> Error {
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

/// Immutable original-entry identity for exact native live factory-LSI reuse.
#[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
#[derive(Clone, Copy)]
struct LsiSysclkSnapshot {
    clock: pac::sysctrl::regs::Cr0,
    sources: pac::sysctrl::regs::Cr1,
    hsi: pac::sysctrl::regs::Hsi,
    hse: pac::sysctrl::regs::Hse,
    lsi: pac::sysctrl::regs::Lsi,
    lse: pac::sysctrl::regs::Lse,
    routes: pac::sysctrl::regs::Cr2,
    interrupts: pac::sysctrl::regs::Ier,
    mco: pac::sysctrl::regs::Mco,
    factory: u16,
    // Attached once after the first legal Flash observation, never recaptured.
    #[cfg(rcc_cw32l011_v1)]
    flash: Option<pac::flash::regs::Cr2>,
}

#[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
impl LsiSysclkSnapshot {
    fn admit(self) -> Result<(), Error> {
        #[cfg(rcc_cw32l011_v1)]
        {
            if self.sources.hseen() { return Err(Error::HseClockInUse); }
            if self.sources.hseccs() || self.sources.lseccs() {
                return Err(Error::LsiClockInUse);
            }
            match self.clock.sysclk() {
                ClockSource::Hsi | ClockSource::Lsi => {}
                ClockSource::Hse | ClockSource::Lse => return Err(Error::InvalidEntryClock),
                _ => return Err(Error::InvalidClockSource),
            }
        }
        let mut factory = pac::sysctrl::regs::Lsi::default();
        factory.set_trim(self.factory);
        if self.factory == u16::MAX || self.lsi.trim() != factory.trim() {
            return Err(Error::InvalidLsiCalibration);
        }
        if !self.sources.lsien() || !self.lsi.stable() {
            return Err(Error::LsiNotReady);
        }
        Ok(())
    }

    #[cfg(rcc_cw32l011_v1)]
    fn post_observation(self) -> Result<(), Error> {
        // HSE is excluded; stale unrelated HSE flags do not veto this slice.
        check_external_faults(false, self.sources.lseen() || self.sources.lseccs())
    }

    #[cfg(rcc_cw32l011_v1)]
    fn flash_access() -> Result<(), Error> {
        let info = <crate::peripherals::FLASH as crate::rcc::SealedRccPeripheral>::RCC_INFO;
        if !info.is_enabled() || info.reset_asserted() {
            return Err(Error::FlashClockTimeout);
        }
        Ok(())
    }

    #[cfg(rcc_cw32l011_v1)]
    fn read_flash() -> Result<pac::flash::regs::Cr2, Error> {
        Self::flash_access()?;
        let flash = pac::FLASH.cr2().read();
        Self::flash_access()?;
        Ok(flash)
    }

    #[cfg(rcc_cw32l011_v1)]
    fn write_flash_wait(wait: u8) -> Result<(), Error> {
        // The caller has just verified the complete settled phase. Check the
        // actual configuration gate/reset again before this typed RMW's read.
        Self::flash_access()?;
        pac::FLASH.cr2().modify(|w| {
            w.set_key(0x5a5a);
            w.set_wait(wait);
        });
        Self::flash_access()
    }

    #[cfg(rcc_cw32l011_v1)]
    fn flash_wait(self, expected: Option<(u8, u8)>) -> Result<Option<(u8, u8)>, Error> {
        if expected.is_none() { return Ok(None); }
        // Return a buffered result: source/policy/readiness errors have priority.
        let flash = Self::read_flash();
        let mirror = pac::SYSCTRL.cr2().read().flashwait();
        let mut flash = flash?;
        let wait = flash.wait();
        let mut original = self.flash.ok_or(Error::FlashLatencyTimeout)?;
        flash.set_key(0);
        original.set_key(0);
        flash.set_wait(0);
        original.set_wait(0);
        if flash.0 != original.0 || u32::from(wait) > MAX_FLASH_WAIT {
            return Err(Error::FlashLatencyTimeout);
        }
        Ok(Some((wait, mirror)))
    }

    #[cfg(rcc_cw32l011_v1)]
    fn retained(self, config: Config) -> Result<bool, Error> {
        let r = pac::SYSCTRL;
        self.post_observation()?;
        let mut original_sources = self.sources;
        let mut sources = r.cr1().read();
        let requested = sources.lsien();
        let excluded = config.hse.is_some() || config.lse.is_some()
            || self.sources.hseen() || self.sources.hseccs() || self.sources.lseccs()
            || sources.hseen() || sources.hseccs() || sources.lseccs();
        for source in [&mut original_sources, &mut sources] {
            source.set_key(0);
            source.set_hsien(false);
            source.set_clkccs(false);
        }
        let mut routes = r.cr2().read();
        let mut original_routes = self.routes;
        for route in [&mut original_routes, &mut routes] {
            route.set_key(0);
            route.set_flashwait(0);
        }
        let lsi = r.lsi().read();
        let lse = r.lse().read();
        let interrupts = r.ier().read();
        let mco = r.mco().read();
        self.post_observation()?;
        if excluded || !requested
            || sources.0 != original_sources.0
            || (lsi.0 & crate::RCC_LSI_PARAMETERS_MASK)
                != (self.lsi.0 & crate::RCC_LSI_PARAMETERS_MASK)
            || lse.0 != self.lse.0
            || routes.0 != original_routes.0
            || interrupts.0 != self.interrupts.0
            || mco.0 != self.mco.0
        {
            return Err(Error::ClockConfigurationTimeout);
        }
        Ok(lsi.stable())
    }

    #[cfg(rcc_cw32l011_v1)]
    #[allow(clippy::too_many_arguments)]
    fn verify(
        self, config: Config,
        mut expected_clock: pac::sysctrl::regs::Cr0,
        mut expected_sources: pac::sysctrl::regs::Cr1,
        expected_hsi: pac::sysctrl::regs::Hsi, expected_hsi_stable: bool,
        expected_hse: pac::sysctrl::regs::Hse, expected_hse_stable: bool,
        expected_wait: Option<(u8, u8)>,
    ) -> Result<(), Error> {
        let lsi_stable = self.retained(config)?;
        let r = pac::SYSCTRL;
        let mut clock = r.cr0().read();
        let mut sources = r.cr1().read();
        let hsi = r.hsi().read();
        let hse = r.hse().read();
        let flash = self.flash_wait(expected_wait);
        let initial_mirror = if expected_wait.is_none() { Some(r.cr2().read().flashwait()) } else { None };
        self.post_observation()?;
        clock.set_key(0);
        expected_clock.set_key(0);
        sources.set_key(0);
        expected_sources.set_key(0);
        if clock.0 != expected_clock.0 || sources.0 != expected_sources.0
            || (hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                != (expected_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
            || hsi.stable() != expected_hsi_stable
            || (hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                != (expected_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
            || hse.stable() != expected_hse_stable
            || initial_mirror.is_some_and(|mirror| mirror != self.routes.flashwait())
        {
            return Err(Error::ClockConfigurationTimeout);
        }
        if !lsi_stable { return Err(Error::LsiNotReady); }
        if flash? != expected_wait { return Err(Error::FlashLatencyTimeout); }
        Ok(())
    }

    // These invariants have no acknowledgment interval. In particular an LSI
    // request/readiness loss is never repaired or polled into eligibility.
    #[cfg(rcc_cw32l010_v1)]
    fn retained(self, config: Config) -> Result<bool, Error> {
        let r = pac::SYSCTRL;
        check_external_faults(
            config.hse.is_some() || self.sources.hseen() || self.sources.hseccs(),
            self.sources.lseen() || self.sources.lseccs(),
        )?;
        let mut original_sources = self.sources;
        let mut sources = r.cr1().read();
        let requested = sources.lsien();
        // Exactly these three controls are owned in the enclosing sequence;
        // their phase-specific full identity is checked at each use site.
        original_sources.set_key(0);
        sources.set_key(0);
        for source in [&mut original_sources, &mut sources] {
            source.set_hsien(false);
            source.set_hseen(false);
            source.set_clkccs(false);
        }
        let mut routes = r.cr2().read();
        let mut original_routes = self.routes;
        routes.set_flashwait(0);
        original_routes.set_flashwait(0);
        let lsi = r.lsi().read();
        if !requested
            || sources.0 != original_sources.0
            || (lsi.0 & crate::RCC_LSI_PARAMETERS_MASK)
                != (self.lsi.0 & crate::RCC_LSI_PARAMETERS_MASK)
            || r.lse().read().0 != self.lse.0
            || routes.0 != original_routes.0
            || r.ier().read().0 != self.interrupts.0
            || r.mco().read().0 != self.mco.0
        {
            return Err(Error::ClockConfigurationTimeout);
        }
        Ok(lsi.stable())
    }

    // Full settled phase identity. Pending fields are handled only by the
    // concrete local prior-to-target loops; no source-wide ignore mask exists.
    #[cfg(rcc_cw32l010_v1)]
    #[allow(clippy::too_many_arguments)]
    fn verify(
        self, config: Config,
        mut expected_clock: pac::sysctrl::regs::Cr0,
        mut expected_sources: pac::sysctrl::regs::Cr1,
        expected_hsi: pac::sysctrl::regs::Hsi, expected_hsi_stable: bool,
        expected_hse: pac::sysctrl::regs::Hse, expected_hse_stable: bool,
        expected_wait: Option<(u8, u8)>,
    ) -> Result<(), Error> {
        let lsi_stable = self.retained(config)?;
        let r = pac::SYSCTRL;
        let mut clock = r.cr0().read();
        let mut sources = r.cr1().read();
        let hsi = r.hsi().read();
        let hse = r.hse().read();
        clock.set_key(0);
        expected_clock.set_key(0);
        sources.set_key(0);
        expected_sources.set_key(0);
        if sources.0 != expected_sources.0
            || (hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                != (expected_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
            || hsi.stable() != expected_hsi_stable
            || (hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                != (expected_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
            || hse.stable() != expected_hse_stable
        {
            return Err(Error::ClockConfigurationTimeout);
        }
        if !lsi_stable { return Err(Error::LsiNotReady); }
        if clock.0 != expected_clock.0 { return Err(Error::ClockConfigurationTimeout); }
        if let Some((wait, mirror)) = expected_wait {
            if pac::FLASH.cr2().read().wait() != wait || r.cr2().read().flashwait() != mirror {
                return Err(Error::FlashLatencyTimeout);
            }
        } else if r.cr2().read().flashwait() != self.routes.flashwait() {
            return Err(Error::ClockConfigurationTimeout);
        }
        Ok(())
    }
}

/// Target-only identity from before the first configuration-gate write. Normal
/// native Admission remains responsible for oscillator/pad/monitor ownership.
#[cfg(rcc_lse)]
#[derive(Clone, Copy)]
struct LseSysclkSnapshot {
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
    fn policy_unchanged(self) -> Result<(), Error> {
        let r = pac::SYSCTRL;
        // FLASHWAIT[6:4] shares the FLASH.WAIT function. Only our authoritative
        // FLASH write/readback licenses this typed field difference; never
        // recapture brake, RTC low-power, wake, debug or interrupt policy.
        let mut original = self.routes;
        let mut current = r.cr2().read();
        original.set_flashwait(0);
        current.set_flashwait(0);
        if current.0 != original.0
            || r.ier().read().0 != self.interrupts.0
            || r.mco().read().0 != self.mco.0
        {
            return Err(Error::LseClockInUse);
        }
        let lsi = r.lsi().read();
        // STABLE can make expected startup progress; TRIM and WAIT cannot.
        if lsi.trim() != self.lsi.trim() || lsi.waitcycle() != self.lsi.waitcycle() {
            return Err(Error::ClockConfigurationTimeout);
        }
        Ok(())
    }

    fn before_lse_start(self, config: Config) -> Result<(), Error> {
        self.policy_unchanged()?;
        if pac::SYSCTRL.lse().read().0 != self.lse.0 {
            return Err(Error::LseClockInUse);
        }
        if config.lse.ok_or(Error::LseNotConfigured)?.monitored()
            && (!self.lsi.stable() || !pac::SYSCTRL.lsi().read().stable())
        {
            // Later enabling or readiness cannot manufacture inherited monitor
            // admission. The first native preflight established this early fact.
            return Err(Error::LseMonitorNotReady);
        }
        Ok(())
    }

    fn hsi_ownership(self, trim: u16) -> Result<(), Error> {
        let changing = self.hsi.trim() != trim || !self.sources.hsien() || !self.hsi.stable();
        if changing
            && (pac::SYSCTRL.mco().read().source() == 3 || pac::SYSCTRL.ier().read().hsirdy())
        {
            // Own MCO SOURCE3 is raw HSIOSC. A start/restart can also latch
            // HSIRDY through an already enabled request even with IER untouched.
            return Err(Error::HsiClockInUse);
        }
        Ok(())
    }

    fn first_lsi_central(self, config: Config) -> Result<(), Error> {
        self.before_lse_start(config)?;
        let r = pac::SYSCTRL;
        let mut original = self.sources;
        let mut current = r.cr1().read();
        // Only the enclosing unchanged-HSI enable can precede the second pass.
        original.set_hsien(false);
        current.set_hsien(false);
        original.set_key(0);
        current.set_key(0);
        if current.0 != original.0 {
            return Err(Error::ClockConfigurationTimeout);
        }
        if r.mco().read().source() == 4 || r.ier().read().lsirdy() {
            return Err(Error::LsiClockInUse);
        }
        Ok(())
    }

    fn admit_first_lsi_request(
        self,
        config: Config,
        cs: critical_section::CriticalSection<'_>,
    ) -> Result<(), Error> {
        use crate::rcc::SealedRccPeripheral;
        self.first_lsi_central(config)?;
        // Each native configuration gate is inspected/restored separately. A
        // restoration failure wins over a semantic conflict from its read.
        let rtc = crate::peripherals::RTC::RCC_INFO;
        if rtc.reset_asserted() {
            return Err(Error::LsiClockInUse);
        }
        let rtc_conflict = rtc
            .inspect_for_init(cs, config.timeout, || {
                let source = pac::RTC.cr1().read().source();
                source == 2 || source > 3 || rtc.reset_asserted()
            })
            .map_err(lse_target_inspection_error)?;
        if rtc_conflict || rtc.reset_asserted() {
            return Err(Error::LsiClockInUse);
        }
        for (info, uart) in [
            (crate::peripherals::UART1::RCC_INFO, pac::UART1),
            (crate::peripherals::UART2::RCC_INFO, pac::UART2),
            #[cfg(cw32l011)]
            (crate::peripherals::UART3::RCC_INFO, pac::UART3),
        ] {
            if info.reset_asserted() {
                return Err(Error::LsiClockInUse);
            }
            let conflict = info
                .inspect_for_init(cs, config.timeout, || {
                    uart.cr1().read().source() == pac::uart::vals::Source::Lsi
                        || info.reset_asserted()
                })
                .map_err(lse_target_inspection_error)?;
            if conflict || info.reset_asserted() {
                return Err(Error::LsiClockInUse);
            }
        }
        let lptim = crate::peripherals::LPTIM::RCC_INFO;
        if lptim.reset_asserted() {
            return Err(Error::LsiClockInUse);
        }
        let lptim_conflict = lptim
            .inspect_for_init(cs, config.timeout, || {
                (pac::LPTIM.cr().read().en()
                    && pac::LPTIM.cfgr().read().iclksrc() == pac::lptim::vals::Source::Lsi)
                    || lptim.reset_asserted()
            })
            .map_err(lse_target_inspection_error)?;
        if lptim_conflict || lptim.reset_asserted() {
            return Err(Error::LsiClockInUse);
        }
        let analog = crate::peripherals::VC1::RCC_INFO;
        if analog.reset_asserted() {
            return Err(Error::LsiClockInUse);
        }
        let analog_conflict = analog
            .inspect_for_init(cs, config.timeout, || {
                let lvd = pac::LVD.cr0().read();
                // Hardware auto-request is EN plus the LSI filter source,
                // including zero filter count; do not use needs_lsi here.
                (lvd.en() && !lvd.fltclk())
                    || (pac::VC1.cr0().read().en()
                        && pac::VC1.cr1().read().fltclk() == pac::vc::vals::FilterClock::InternalRc)
                    || (pac::VC2.cr0().read().en()
                        && pac::VC2.cr1().read().fltclk() == pac::vc::vals::FilterClock::InternalRc)
                    || analog.reset_asserted()
            })
            .map_err(lse_target_inspection_error)?;
        if analog_conflict || analog.reset_asserted() {
            return Err(Error::LsiClockInUse);
        }
        // No GTIM/ATIM/GPIO work gate is opened. Their direct LSI roots, IWDT
        // and downstream/external participants require the public handover.
        self.first_lsi_central(config)
    }

    fn final_sources(self, config: Config, trim: u16, needs_lsi: bool) -> Result<(), Error> {
        self.policy_unchanged()?;
        let r = pac::SYSCTRL;
        let mut expected = self.sources;
        expected.set_key(0);
        expected.set_hsien(true);
        expected.set_hseen(config.hse.is_some() || self.sources.hseen());
        expected.set_lseen(true);
        expected.set_lseccs(config.lse.ok_or(Error::LseNotConfigured)?.monitored());
        let mut sources = r.cr1().read();
        sources.set_key(0);
        let hsi = r.hsi().read();
        let hse = r.hse().read();
        if sources.0 != expected.0
            || !hsi.stable()
            || hsi.trim() != trim
            || hsi.div() != config.hsi.div as u8
            || ((needs_lsi || self.lsi.stable()) && !r.lsi().read().stable())
            || !super::lse::preserved_register(self.lse.0)
            || hse.hexenpol() != self.hse.hexenpol()
        {
            return Err(Error::ClockConfigurationTimeout);
        }
        if let Some(requested) = config.hse {
            if !hse.stable() || !hse_parameters_match(requested)? {
                return Err(Error::ClockConfigurationTimeout);
            }
        } else if hse.0 != self.hse.0 {
            return Err(Error::ClockConfigurationTimeout);
        }
        if !super::lse::healthy(config.lse.ok_or(Error::LseNotConfigured)?) {
            return Err(Error::LseNotReady);
        }
        Ok(())
    }
}

/// Final native live-LSI preparation: factory HSI carries the final divisors
/// and conservative WAIT before the last clock-tree mutation selects LSI.
#[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
fn finish_lsi_sysclk(
    config: Config, clocks: Clocks, entry: LsiSysclkSnapshot, trim: u16,
    cs: critical_section::CriticalSection<'_>,
) -> Result<Clocks, Error> {
    #[cfg(rcc_cw32l010_v1)]
    let mut clocks = clocks;
    let r = pac::SYSCTRL;
    #[cfg(rcc_cw32l011_v1)]
    {
        let _ = cs;
        if config.hse.is_some() || config.lse.is_some() || clocks.hse.is_some()
            || entry.sources.hseen() || entry.sources.hseccs() || entry.sources.lseccs()
        {
            return Err(Error::ClockConfigurationTimeout);
        }
    }
    let mut expected_clock = entry.clock;
    expected_clock.set_key(0);
    expected_clock.set_sysclk(ClockSource::Hsi);
    expected_clock.set_hclkprs(entry.clock.hclkprs().max(AHBPrescaler::Div8 as u8));
    expected_clock.set_pclkprs(entry.clock.pclkprs().max(APBPrescaler::Div8 as u8));
    let mut expected_sources = entry.sources;
    expected_sources.set_key(0);
    expected_sources.set_hsien(true);
    expected_sources.set_hseen(config.hse.is_some() || entry.sources.hseen());
    let mut expected_hsi = entry.hsi;
    expected_hsi.set_trim(trim);
    expected_hsi.set_div(config.hsi.div as u8);
    let expected_hsi_stable = true;
    let expected_hse = entry.hse;
    #[cfg(rcc_cw32l010_v1)]
    let mut expected_hse = expected_hse;
    #[cfg(rcc_cw32l010_v1)]
    if let Some(hse) = config.hse {
        expected_hse.set_mode(hse.mode == HseMode::Bypass);
        expected_hse.set_driver(hse.drive);
        expected_hse.set_pdriver(hse.drive);
        expected_hse.set_waitcycle(pac::sysctrl::vals::HseWait::Cycles262144);
        expected_hse.set_digflt(false);
        expected_hse.set_detcnt(hse.detector_count()?);
    }
    let expected_hse_stable = config.hse.is_some() || entry.hse.stable();
    let mut expected_wait = Some((MAX_FLASH_WAIT as u8, MAX_FLASH_WAIT as u8));
    entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    r.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_hclkprs(config.ahb_pre as u8);
        w.set_pclkprs(config.apb_pre as u8);
    });
    expected_clock.set_hclkprs(config.ahb_pre as u8);
    expected_clock.set_pclkprs(config.apb_pre as u8);
    // Final dividers are settled identity, not readiness to poll back to health.
    entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    barrier();
    let upper_hclk = clocks.hclk_bounds().maximum().0
        .max(crate::rcc::ClockBounds::hsi(config.hsi.div.divisor())
            .divided_by(config.ahb_pre.divisor()).maximum().0)
        .max(crate::rcc::ClockBounds::hsi(crate::RCC_FIXED_CCS_HSI_DIVISOR).maximum().0);
    let final_wait = (upper_hclk - 1) / crate::RCC_FLASH_WAIT_STEP_HZ;
    if final_wait > MAX_FLASH_WAIT {
        return Err(Error::FlashLatencyTimeout);
    }
    entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    #[cfg(rcc_cw32l010_v1)]
    pac::FLASH.cr2().modify(|w| {
        w.set_key(0x5a5a);
        w.set_wait(final_wait as u8);
    });
    {
        #[cfg(rcc_cw32l011_v1)]
        let flash_write = LsiSysclkSnapshot::write_flash_wait(final_wait as u8);
        // Final WAIT and mirror acknowledge under verified final-divided factory HSI.
        let prior_0 = expected_wait.unwrap().0;
        let target_0 = final_wait as u8;
        let mut acknowledged_0 = prior_0 == target_0;
        let prior_1 = expected_wait.unwrap().1;
        let target_1 = final_wait as u8;
        let mut acknowledged_1 = prior_1 == target_1;
        for attempt in 0..config.timeout {
            let lsi_stable = entry.retained(config)?;
            let mut current_clock = r.cr0().read();
            let mut current_sources = r.cr1().read();
            let current_hsi = r.hsi().read();
            let current_hse = r.hse().read();
            let current_hsi_stable = current_hsi.stable();
            let current_hse_stable = current_hse.stable();
            #[cfg(rcc_cw32l010_v1)]
            let current_wait = expected_wait.map(|_| (pac::FLASH.cr2().read().wait(), r.cr2().read().flashwait()));
            #[cfg(rcc_cw32l011_v1)]
            let current_wait = entry.flash_wait(expected_wait);
            #[cfg(rcc_cw32l011_v1)]
            entry.post_observation()?;
            current_clock.set_key(0);
            current_sources.set_key(0);
            if current_sources.0 != expected_sources.0
                || (current_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                    != (expected_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                || current_hsi_stable != expected_hsi_stable
                || (current_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                    != (expected_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                || current_hse_stable != expected_hse_stable
            {
                return Err(Error::ClockConfigurationTimeout);
            }
            #[cfg(rcc_cw32l010_v1)]
            if !lsi_stable { return Err(Error::LsiNotReady); }
            if current_clock.0 != expected_clock.0 {
                return Err(Error::ClockConfigurationTimeout);
            }
            #[cfg(rcc_cw32l011_v1)]
            if !lsi_stable { return Err(Error::LsiNotReady); }
            #[cfg(rcc_cw32l011_v1)]
            let current_wait = flash_write.and(current_wait)?;
            #[cfg(rcc_cw32l011_v1)]
            if current_wait.unwrap().0 != current_wait.unwrap().1 {
                return Err(Error::FlashLatencyTimeout);
            }
            let observed_0 = current_wait.unwrap().0;
            if observed_0 != target_0 && (observed_0 != prior_0 || acknowledged_0) {
                return Err(Error::FlashLatencyTimeout);
            }
            acknowledged_0 |= observed_0 == target_0;
            let observed_1 = current_wait.unwrap().1;
            if observed_1 != target_1 && (observed_1 != prior_1 || acknowledged_1) {
                return Err(Error::FlashLatencyTimeout);
            }
            acknowledged_1 |= observed_1 == target_1;
            if acknowledged_0 && acknowledged_1 { break; }
            if attempt + 1 == config.timeout { return Err(Error::FlashLatencyTimeout); }
            core::hint::spin_loop();
        }
        expected_wait.as_mut().unwrap().0 = target_0;
        expected_wait.as_mut().unwrap().1 = target_1;
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
    barrier();
    entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    // Last CR0/CR1/oscillator/FLASH mutation for this target.
    r.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_sysclk(ClockSource::Lsi);
    });
    {
        // Only HSI or the requested final LSI selector may appear while acknowledgment is pending.
        let prior_0 = expected_clock.sysclk();
        let target_0 = ClockSource::Lsi;
        let mut acknowledged_0 = prior_0 == target_0;
        for attempt in 0..config.timeout {
            let lsi_stable = entry.retained(config)?;
            let mut current_clock = r.cr0().read();
            let mut current_sources = r.cr1().read();
            let current_hsi = r.hsi().read();
            let current_hse = r.hse().read();
            let current_hsi_stable = current_hsi.stable();
            let current_hse_stable = current_hse.stable();
            #[cfg(rcc_cw32l010_v1)]
            let current_wait = expected_wait.map(|_| (pac::FLASH.cr2().read().wait(), r.cr2().read().flashwait()));
            #[cfg(rcc_cw32l011_v1)]
            let current_wait = entry.flash_wait(expected_wait);
            #[cfg(rcc_cw32l011_v1)]
            entry.post_observation()?;
            current_clock.set_key(0);
            current_sources.set_key(0);
            if current_sources.0 != expected_sources.0
                || (current_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                    != (expected_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                || current_hsi_stable != expected_hsi_stable
                || (current_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                    != (expected_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                || current_hse_stable != expected_hse_stable
            {
                return Err(Error::ClockConfigurationTimeout);
            }
            #[cfg(rcc_cw32l010_v1)]
            if !lsi_stable { return Err(Error::LsiNotReady); }
            let observed_0 = current_clock.sysclk();
            if observed_0 != target_0 && (observed_0 != prior_0 || acknowledged_0) {
                return Err(Error::ClockConfigurationTimeout);
            }
            acknowledged_0 |= observed_0 == target_0;
            current_clock.set_sysclk(prior_0);
            if current_clock.0 != expected_clock.0 {
                return Err(Error::ClockConfigurationTimeout);
            }
            #[cfg(rcc_cw32l011_v1)]
            if !lsi_stable { return Err(Error::LsiNotReady); }
            #[cfg(rcc_cw32l011_v1)]
            let current_wait = current_wait?;
            if current_wait != expected_wait {
                return Err(Error::FlashLatencyTimeout);
            }
            if acknowledged_0 { break; }
            if attempt + 1 == config.timeout { return Err(Error::LsiClockSwitchTimeout); }
            core::hint::spin_loop();
        }
        expected_clock.set_sysclk(target_0);
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    }
    barrier();
    entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    #[cfg(rcc_cw32l010_v1)]
    {
    let final_hse_mode = config.hse.map(|hse| hse.mode).or_else(|| {
        entry.sources.hseen().then_some(if entry.hse.mode() { HseMode::Bypass } else { HseMode::Oscillator })
    });
    if let Some(mode) = final_hse_mode {
        let pads = crate::rcc_hse_pins_match(mode == HseMode::Bypass, config.timeout, cs, true);
        if let Err(error @ Error::LsiConfigurationGateTimeout { .. }) = pads {
            return Err(error);
        }
        // Successful restoration precedes faults/identity, then buffered pads.
        entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
        if !pads? { return Err(Error::ClockConfigurationTimeout); }
    }
    if entry.sources.hseen() {
        if !entry.hse.mode() {
            clocks.hse = Some(HseMode::Oscillator);
        } else if clocks.hse.is_none() {
            clocks.hse = Some(HseMode::Bypass);
        }
    }
    }
    #[cfg(rcc_cw32l011_v1)]
    {
        let current_sources = r.cr1().read();
        entry.post_observation()?;
        let final_hse_mode = config.hse.map(|hse| hse.mode).or_else(|| {
            entry.sources.hseen().then_some(if entry.hse.mode() { HseMode::Bypass } else { HseMode::Oscillator })
        });
        if config.hse.is_some() || final_hse_mode.is_some() || clocks.hse.is_some()
            || entry.sources.hseen() || current_sources.hseen()
            || current_sources.hseccs() || current_sources.lseccs()
        {
            return Err(Error::ClockConfigurationTimeout);
        }
    }
    entry.verify(config, expected_clock, expected_sources, expected_hsi, expected_hsi_stable,
                     expected_hse, expected_hse_stable, expected_wait)?;
    Ok(clocks)
}

/// Own L010/L011 target tail: final divisors under calibrated HSI, then the last CR0
/// write selects LSE. All fault, Flash and pad windows after it are read-only
/// with respect to CR0, so fallback cannot be accidentally switched back.
#[cfg(rcc_lse)]
fn finish_lse_sysclk(
    config: Config,
    mut clocks: Clocks,
    entry: LseSysclkSnapshot,
    trim: u16,
    needs_lsi: bool,
    cs: critical_section::CriticalSection<'_>,
) -> Result<Clocks, Error> {
    let r = pac::SYSCTRL;
    let lse = config.lse.ok_or(Error::LseNotConfigured)?;
    let monitor_hse = config.hse.is_some() || entry.sources.hseen() || entry.sources.hseccs();
    let verify = |source| {
        // Faults win over an apparently ready source or a changed fallback mux.
        check_external_faults(monitor_hse, true)?;
        entry.final_sources(config, trim, needs_lsi)?;
        let clock = r.cr0().read();
        if clock.sysclk() != source {
            return Err(if source == ClockSource::Lse {
                Error::LseClockSwitchTimeout
            } else {
                Error::ClockSwitchTimeout
            });
        }
        if clock.hclkprs() != config.ahb_pre as u8 || clock.pclkprs() != config.apb_pre as u8 {
            return Err(Error::ClockConfigurationTimeout);
        }
        Ok(())
    };
    check_external_faults(monitor_hse, true)?;
    entry.final_sources(config, trim, needs_lsi)?;
    if r.cr0().read().sysclk() != ClockSource::Hsi {
        return Err(Error::ClockSwitchTimeout);
    }
    r.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_hclkprs(config.ahb_pre as u8);
        w.set_pclkprs(config.apb_pre as u8);
    });
    // Dividers are a retained-configuration identity, never a source-ready
    // condition. Reject a mismatch instead of laundering it through a poll.
    verify(ClockSource::Hsi)?;
    barrier();
    r.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_sysclk(ClockSource::Lse);
    });
    for attempt in 0..config.timeout {
        check_external_faults(monitor_hse, true)?;
        entry.final_sources(config, trim, needs_lsi)?;
        let clock = r.cr0().read();
        if clock.hclkprs() != config.ahb_pre as u8 || clock.pclkprs() != config.apb_pre as u8 {
            return Err(Error::ClockConfigurationTimeout);
        }
        if clock.sysclk() == ClockSource::Lse {
            break;
        }
        if attempt + 1 == config.timeout {
            return Err(Error::LseClockSwitchTimeout);
        }
        core::hint::spin_loop();
    }
    barrier();
    verify(ClockSource::Lse)?;
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
        // Fixed effective HSI4MHz fallback: no bus-divider retention credit.
        .max(
            crate::rcc::ClockBounds::hsi(crate::RCC_FIXED_CCS_HSI_DIVISOR)
                .maximum()
                .0,
        );
    let final_wait = (upper_hclk - 1) / crate::RCC_FLASH_WAIT_STEP_HZ;
    pac::FLASH.cr2().modify(|w| {
        w.set_key(0x5a5a);
        w.set_wait(final_wait as u8);
    });
    for attempt in 0..config.timeout {
        // Continue checking fault/source/mux/policy first throughout lowering
        // WAIT. A route or monitor mismatch is an error, never readiness lag.
        verify(ClockSource::Lse)?;
        if u32::from(pac::FLASH.cr2().read().wait()) == final_wait {
            break;
        }
        if attempt + 1 == config.timeout {
            return Err(Error::FlashLatencyTimeout);
        }
        core::hint::spin_loop();
    }
    barrier();
    verify(ClockSource::Lse)?;
    if let Some(hse) = config.hse {
        if !crate::rcc_hse_pins_match(hse.mode == HseMode::Bypass, config.timeout, cs,
                #[cfg(all(rcc_cw32l010_v1, rcc_lsi_sysclk))] false,
            )? {
            return Err(Error::ClockConfigurationTimeout);
        }
    }
    // GPIOB (L010) or GPIOC (L011) is a working gate, also during exact reuse.
    // Native verify checks pads and the monitor marker; no clock is published.
    super::lse::verify(lse, cs, false)?;
    verify(ClockSource::Lse)?;
    if u32::from(pac::FLASH.cr2().read().wait()) != final_wait {
        return Err(Error::FlashLatencyTimeout);
    }
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
