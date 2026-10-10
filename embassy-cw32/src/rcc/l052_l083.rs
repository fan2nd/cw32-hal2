//! CW32L052/CW32L083 qualified HSI/direct HSE and L083 HSI/HSE-fed PLL clock trees.
//! HSE, retained AUTOTRIM/RTC/LVD ownership and pad qualification:
//! docs/qualified-l052-hse.md and docs/qualified-l083-hse.md; own family sources.
//!
//! Sources: L052 UM CN V1.5 and L083 UM CN V2.0 §§4.3.4, 4.5, 4.7, 7.4,
//! 7.9.2; each official SDK's RCC_HSI_Enable and FLASH_SetLatency.
//! Both use factory-calibrated 48 MHz HSIOSC at 0x00100a00 and the same nine
//! documented divider encodings. L052 has no PLL. L083 admits the separately
//! qualified HSI/HSE-fed PLL; see docs/l083-hsi-pll.md. An inherited PLL is
//! stopped only after safely leaving it, before changing its reference or fields.
//! A changed trim requires temporary LSI, stopping HSI, then calibration and
//! restart. Divider-only live changes are explicitly permitted by §4.5.2.
//! Unrequested HSE/LSE, LSI parameters/previous enable, clock security, debug/reset, flash
//! reserved bits and unrelated gates are preserved. No CACHE/FETCH bits exist.
//! Every hardware wait is bounded. A failed transition can leave temporary
//! LSI enabled/selected or PLL disabled; reset before retrying. Call only before
//! peripheral/DMA/application interrupt code can depend on these clocks.
//! Polling requires CPU progress; HSE-fed PLL reference loss has no qualified
//! fallback, continuous lock indication or post-loss rate guarantee.
//! MCO and dedicated PLL_OUT consumers must also be quiescent: PLL stopping and
//! reconfiguration do not preserve clocks already exported to other hardware.
//! The incoming clock state must already satisfy the board's voltage limits.
//! Both datasheets require HCLK/PCLK <=24 MHz below 1.8 V (minimum 1.65 V);
//! above 24 MHz the board must supply at least 1.8 V. Init does not measure VDD.
//! Conservative temporary bus dividers prevent an intermediate overspeed.
//! Exact qualified L052/L083 factory-LSI SYSCLK uses an init-only transition: LSIEN
//! remains set, live LSI is never retuned, and failed partial changes have no
//! rollback or safe-retry guarantee. Reset, and power reset for POR-retained
//! LSE controls when required, before retrying. GPIOA/B/C/D/F (and L083 GPIOE) inspection
//! temporarily resumes whole-bank input sampling, filters, interrupts and
//! events even on a later refusal. The caller hands over those functional
//! effects and external pin feedback before initialization. Restoring a gate
//! does not undo hardware activity. No timer, LCD pump, watchdog, IRQ or RTC
//! command is used to manufacture admission.

use crate::{pac, time::Hertz};
use core::cell::Cell;
use critical_section::Mutex;

#[cfg(all(rcc_lse, rcc_cw32l083_v1))]
mod l083_lse_sysclk;
#[cfg(all(rcc_lse, rcc_cw32l052_v1))]
mod lse_sysclk;
#[cfg(rcc_hse)]
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

/// Divider from HCLK to PCLK. L052/L083 have one PCLK domain.
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

/// L083 source-qualified PLL multipliers, using the literal hardware field.
#[cfg(rcc_pll)]
pub use pac::sysctrl::vals::PllMul;

/// Qualified PLL reference. The divider belongs to `Config.hsi`, not the PLL.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[cfg(rcc_pll)]
pub enum PllSource {
    /// Factory-trimmed HSI after its configured divider.
    HSI,
    /// Undivided HSE from `Config.hse`, in oscillator or bypass mode. The
    /// existing HSE board and operating-condition contract and all PLL
    /// input/output limits apply. Oscillator mode follows the vendor-documented
    /// crystal-to-PLL path; admission does not independently certify internal
    /// reference duty. Bypass requires the specified OSC_IN waveform, including
    /// 40–60% duty. PLL bounds describe rates only; individual-cycle timing and
    /// recovery after reference loss are not guaranteed.
    HSE,
}

/// One-time L083 PLL configuration. It must be selected as `Sysclk::PLL`.
///
/// Both actual input endpoints must fit one documented input bin, and both
/// actual multiplied output endpoints must fit one output bin and the separate
/// electrical limits. This conservative policy admits HSI /6 x2,x4,x5,x7 and
/// /10 x3,x4,x6,x7,x8,x9,x11,x12 at the full factory-HSI qualification.
/// HSE uses the complete declared source bounds through the same checks.
/// Settings outside these bounds and bins are not qualified by this API.
/// The PLL supplies qualified clock-rate bounds, not absolute period/jitter bounds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[cfg(rcc_pll)]
pub struct Pll {
    pub src: PllSource,
    pub mul: PllMul,
}
#[cfg(rcc_pll)]
struct PllParameters {
    bounds: crate::rcc::ClockBounds,
    source: pac::sysctrl::vals::PllSource,
    input_range: pac::sysctrl::vals::PllInputRange,
    output_range: pac::sysctrl::vals::PllOutputRange,
}
#[cfg(rcc_pll)]
impl Pll {
    fn parameters(self, config: &Config) -> Result<PllParameters, Error> {
        let c = config.operating_conditions;
        if c.min_supply_mv < crate::RCC_PLL_SUPPLY_MV.0
            || c.max_supply_mv > crate::RCC_PLL_SUPPLY_MV.1
            || c.min_temperature_c < crate::RCC_PLL_TEMPERATURE_C.0
            || c.max_temperature_c > crate::RCC_PLL_TEMPERATURE_C.1
        {
            return Err(Error::PllConditionsOutsideQualifiedRange);
        }
        let multiplier = self.mul.to_bits();
        if !(crate::RCC_PLL_MULTIPLIER_RANGE.0..=crate::RCC_PLL_MULTIPLIER_RANGE.1)
            .contains(&multiplier)
        {
            return Err(Error::InvalidPllMultiplier);
        }
        let (input, source) = match self.src {
            PllSource::HSI => (
                crate::rcc::ClockBounds::hsi(config.hsi.div.divisor()),
                pac::sysctrl::vals::PllSource::Hsi,
            ),
            PllSource::HSE => {
                let hse = config.hse.ok_or(Error::HseNotConfigured)?;
                let bounds = hse.bounds()?;
                if !crate::RCC_PLL_HSE_SUPPORTED {
                    return Err(Error::PllInputOutsideQualifiedRange);
                }
                let source = match hse.mode {
                    HseMode::Oscillator => pac::sysctrl::vals::PllSource::HseCrystal,
                    HseMode::Bypass => pac::sysctrl::vals::PllSource::HseBypass,
                };
                (bounds, source)
            }
        };
        if input.minimum_below(crate::RCC_PLL_INPUT_RANGE_HZ.0)
            || input.maximum_exceeds(crate::RCC_PLL_INPUT_RANGE_HZ.1)
        {
            return Err(Error::PllInputOutsideQualifiedRange);
        }
        let bounds = input
            .multiplied_by(u32::from(multiplier))
            .ok_or(Error::PllArithmeticOverflow)?;
        if bounds.minimum_below(crate::RCC_PLL_OUTPUT_RANGE_HZ.0)
            || bounds.maximum_exceeds(crate::RCC_PLL_OUTPUT_RANGE_HZ.1)
        {
            return Err(Error::PllOutputOutsideQualifiedRange);
        }
        let input_index = crate::RCC_PLL_INPUT_BINS_HZ
            .iter()
            .position(|(lo, hi)| !input.minimum_below(*lo) && !input.maximum_exceeds(*hi))
            .ok_or(Error::PllInputCrossesBin)?;
        let output_index = crate::RCC_PLL_OUTPUT_BINS_HZ
            .iter()
            .position(|(lo, hi)| !bounds.minimum_below(*lo) && !bounds.maximum_exceeds(*hi))
            .ok_or(Error::PllOutputCrossesBin)?;
        Ok(PllParameters {
            bounds,
            source,
            input_range: pac::sysctrl::vals::PllInputRange::from_bits(input_index as u8),
            output_range: pac::sysctrl::vals::PllOutputRange::from_bits(output_index as u8),
        })
    }
}

/// System-clock sources implemented by this backend.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[cfg(rcc_hse)]
pub enum Sysclk {
    /// Factory-calibrated HSI through its configured divider.
    HSI,
    /// Qualified external high-speed oscillator or input.
    HSE,
    /// Init-only factory-calibrated LSI. Its bounds describe rates only.
    /// Exact qualified L052 and L083 parts: 1.65..5.5 V, VDDA = VDD, -40..85 C.
    /// Factory nominal 32800 Hz, rate envelope 31816..33784 Hz. No startup
    /// duration, per-cycle timing, continuing operation or recovery is promised.
    /// Admission temporarily resumes whole GPIOA/B/C/D/F banks, including
    /// unrelated pins and their armed filter/interrupt/event paths. Hand over
    /// these functional effects and external pin feedback before initialization.
    /// All qualified L052/L083 RTC LSI aliases are rate-only under every SYSCLK.
    /// L083 additionally inspects GPIOE, stops any admitted inherited PLL, and
    /// retains Flash WAIT2. External declarations require VDD >= 1.8 V.
    #[cfg(rcc_lsi_sysclk)]
    LSI,
    /// Init-only board-qualified LSE, with retained factory LSI monitoring.
    /// On the five qualified L083 packages, conservative fallback admission
    /// requires VDD >= 1.8 V and retains Flash WAIT2 regardless of dividers.
    #[cfg(rcc_lse)]
    LSE,
    /// L083 HSI/HSE-fed PLL, with independent electrical and analog-bin checks.
    #[cfg(rcc_pll)]
    PLL,
}

/// HSE electrical mode. The board must reserve the actual oscillator pads.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[cfg(rcc_hse)]
pub enum HseMode {
    /// Crystal or ceramic resonator on OSC_IN and OSC_OUT.
    Oscillator,
    /// Externally driven digital clock on OSC_IN; OSC_OUT remains available.
    Bypass,
}

#[cfg(rcc_hse)]
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
#[cfg(rcc_hse)]
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
    /// Crystal drive setting; L052 uses it for both pre-start and run phases.
    /// Ignored electrically in bypass mode.
    pub drive: HseDrive,
}
#[cfg(rcc_hse)]
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
    /// HSI oscillator divider.
    pub hsi: Hsi,
    /// Optional board-qualified source; its pads are reserved with any SYSCLK.
    #[cfg(rcc_hse)]
    pub hse: Option<Hse>,
    /// Init-only nominal 32768 Hz LSE; None preserves inherited state.
    #[cfg(rcc_lse)]
    pub lse: Option<super::Lse>,
    /// Optional L083 PLL, admitted only when selected as the system source.
    #[cfg(rcc_pll)]
    pub pll: Option<Pll>,
    /// Requested system-clock source. HSI remains enabled for a legal escape.
    #[cfg(rcc_hse)]
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
            #[cfg(rcc_hse)]
            hse: None,
            #[cfg(rcc_lse)]
            lse: None,
            #[cfg(rcc_hse)]
            sys: Sysclk::HSI,
            #[cfg(rcc_pll)]
            pll: None,
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
        #[cfg(not(rcc_hse))]
        let clocks = {
            let hsi = HSI_FREQ / self.hsi.div.divisor();
            let hclk = hsi / self.ahb_pre.divisor();
            let pclk = hclk / self.apb_pre.divisor();
            let clocks = Clocks {
                hsi,
                sys: hsi,
                hclk,
                pclk,
                dividers: [
                    self.hsi.div.divisor(),
                    self.hsi.div.divisor() * self.ahb_pre.divisor(),
                    self.hsi.div.divisor() * self.ahb_pre.divisor() * self.apb_pre.divisor(),
                ],
            };
            crate::rcc::operating::validate(self.operating_conditions, clocks)?;
            clocks
        };
        #[cfg(rcc_hse)]
        let clocks = {
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
            #[cfg(rcc_pll)]
            let pll = match (self.pll, self.sys) {
                (Some(pll), Sysclk::PLL) => Some(pll.parameters(self)?.bounds),
                (None, Sysclk::PLL) => return Err(Error::PllNotConfigured),
                (Some(_), _) => return Err(Error::PllNotSelected),
                (None, _) => None,
            };
            let source = match self.sys {
                Sysclk::HSI => crate::rcc::ClockBounds::hsi(self.hsi.div.divisor()),
                Sysclk::HSE => hse.ok_or(Error::HseNotConfigured)?,
                #[cfg(rcc_lsi_sysclk)]
                Sysclk::LSI => crate::rcc::ClockBounds::lsi(),
                #[cfg(rcc_lse)]
                Sysclk::LSE => {
                    use crate::rtc::sealed::Instance;
                    let (config, bounds) = lse.ok_or(Error::LseNotConfigured)?;
                    // Hardware counts and a distinct software phase margin;
                    // factory LSI rate facts are not per-cycle jitter bounds.
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
                #[cfg(rcc_pll)]
                Sysclk::PLL => pll.ok_or(Error::PllNotConfigured)?,
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
                #[cfg(rcc_pll)]
                pll,
            };
            crate::rcc::operating::validate(self.operating_conditions, clocks)?;
            #[cfg(rcc_lsi_sysclk)]
            if self.sys == Sysclk::LSI {
                let board = self.operating_conditions;
                if board.min_supply_mv < crate::RCC_LSI_SUPPLY_MV.0
                    || board.max_supply_mv > crate::RCC_LSI_SUPPLY_MV.1
                {
                    return Err(Error::SupplyOutsideQualifiedRange);
                }
                if board.min_temperature_c < crate::RCC_LSI_TEMPERATURE_C.0
                    || board.max_temperature_c > crate::RCC_LSI_TEMPERATURE_C.1
                {
                    return Err(Error::TemperatureOutsideQualifiedRange);
                }
                // P8 executes configured factory HSI at the final divisors.
                crate::rcc::operating::validate(
                    board,
                    Clocks {
                        source: crate::rcc::ClockBounds::hsi(self.hsi.div.divisor()),
                        ..clocks
                    },
                )?;
                // Own L052 CCS escape is HSI /6. No post-fault divider credit.
                #[cfg(rcc_cw32l052_v1)]
                crate::rcc::operating::validate(
                    board,
                    Clocks {
                        source: crate::rcc::ClockBounds::hsi(crate::RCC_FIXED_CCS_HSI_DIVISOR),
                        dividers: [self.hsi.div.divisor(), 1, 1],
                        ..clocks
                    },
                )?;
            }
            #[cfg(all(rcc_lsi_sysclk, rcc_cw32l083_v1))]
            if self.sys == Sysclk::LSI && (self.hse.is_some() || self.lse.is_some()) {
                // New LSI-target admission only: own L083 fallback has no
                // divider contract, so bound raw factory HSI with no credit.
                crate::rcc::operating::validate(
                    self.operating_conditions,
                    Clocks {
                        source: crate::rcc::ClockBounds::hsi(1),
                        dividers: [self.hsi.div.divisor(), 1, 1],
                        ..clocks
                    },
                )?;
            }
            #[cfg(all(rcc_lse, rcc_cw32l052_v1))]
            if self.sys == Sysclk::LSE {
                // Final dividers are installed while executing configured HSI.
                // This use edge is independent of an HSE declaration or CLKCCS.
                crate::rcc::operating::validate(
                    self.operating_conditions,
                    Clocks {
                        source: crate::rcc::ClockBounds::hsi(self.hsi.div.divisor()),
                        ..clocks
                    },
                )?;
                // Own fallback output is fixed HSI /6. Post-fault bus-divider
                // retention is unspecified, so cover its undivided upper rate.
                crate::rcc::operating::validate(
                    self.operating_conditions,
                    Clocks {
                        source: crate::rcc::ClockBounds::hsi(crate::RCC_FIXED_CCS_HSI_DIVISOR),
                        dividers: [self.hsi.div.divisor(), 1, 1],
                        ..clocks
                    },
                )?;
            }
            #[cfg(all(rcc_lse, rcc_cw32l083_v1))]
            if self.sys == Sysclk::LSE {
                // Requested final divisors are installed while still on HSI.
                crate::rcc::operating::validate(
                    self.operating_conditions,
                    Clocks {
                        source: crate::rcc::ClockBounds::hsi(self.hsi.div.divisor()),
                        ..clocks
                    },
                )?;
                // Own L083 manual specifies CCS selecting HSI, without an
                // explicit post-fault divider contract. This target gives no
                // HSI/AHB/APB divisor credit: 48.96 MHz requires VDD >= 1.8 V.
                // It is a conservative bound, not a hardware divider rewrite.
                crate::rcc::operating::validate(
                    self.operating_conditions,
                    Clocks {
                        source: crate::rcc::ClockBounds::hsi(1),
                        dividers: [self.hsi.div.divisor(), 1, 1],
                        ..clocks
                    },
                )?;
            }
            // Qualify requested HSI and the own-family CCS fallback for every
            // HSE declaration, even when the incoming switching policy is off.
            // L052 hardware forces HSI /6; L083 retains the configured divider.
            // This admission restriction does not require enabling CCS.
            // It establishes an electrically legal HSI escape, not guaranteed
            // fallback or continued CPU progress after HSE-fed PLL reference loss.
            if self.hse.is_some() {
                crate::rcc::operating::validate(
                    self.operating_conditions,
                    Clocks {
                        source: crate::rcc::ClockBounds::hsi(self.hsi.div.divisor()),
                        ..clocks
                    },
                )?;
                #[cfg(rcc_cw32l052_v1)]
                crate::rcc::operating::validate(
                    self.operating_conditions,
                    Clocks {
                        source: crate::rcc::ClockBounds::hsi(crate::RCC_FIXED_CCS_HSI_DIVISOR),
                        ..clocks
                    },
                )?;
            }
            clocks
        };
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
    #[cfg(rcc_hse)]
    pub(crate) source: crate::rcc::ClockBounds,
    #[cfg(rcc_hse)]
    hse: Option<HseMode>,
    #[cfg(rcc_lse)]
    pub(crate) lse: Option<(super::Lse, crate::rcc::ClockBounds)>,
    #[cfg(rcc_pll)]
    pll: Option<crate::rcc::ClockBounds>,
}
#[cfg(rcc_pll)]
impl Clocks {
    /// Raw PLL clock-rate bounds after successful initialization, before AHB/APB.
    /// A `Config::frequencies()` result is a prospective calculation only.
    pub fn pll_bounds(self) -> Option<crate::rcc::ClockBounds> {
        self.pll
    }
    /// Nominal raw PLL rate, rounded down. No independent PLL output is enabled.
    pub fn pll_frequency(self) -> Option<Hertz> {
        self.pll.map(|b| b.nominal())
    }
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
    /// The incoming selected source has inconsistent request/readiness.
    #[cfg(rcc_lsi_sysclk)]
    InvalidEntryClock,
    /// The raw factory LSI calibration halfword is erased.
    #[cfg(rcc_lsi_sysclk)]
    InvalidLsiCalibration,
    /// A live LSI does not carry the single-read factory calibration.
    #[cfg(rcc_lsi_sysclk)]
    LsiCalibrationInUse,
    /// Retained consumers, observers or immutable configuration changed.
    #[cfg(rcc_lsi_sysclk)]
    LsiClockInUse,
    /// An owned LSI request or native parameter write did not acknowledge.
    #[cfg(rcc_lsi_sysclk)]
    LsiConfigurationTimeout,
    /// A temporary inspection gate could not be enabled; restoration succeeded.
    #[cfg(rcc_lsi_sysclk)]
    LsiGateEnableTimeout,
    /// An inspection gate did not return to its captured state.
    #[cfg(rcc_lsi_sysclk)]
    LsiGateRestoreTimeout,
    #[cfg(rcc_lse)]
    LseNotConfigured,
    #[cfg(rcc_lse)]
    InvalidLseDetector,
    #[cfg(rcc_lse)]
    LseMonitorConditionsOutsideQualifiedRange,
    #[cfg(rcc_lse)]
    InvalidLseBounds,
    #[cfg(rcc_lse)]
    LseNotReady,
    #[cfg(rcc_lse)]
    LsePinConflict,
    #[cfg(rcc_lse)]
    LseClockInUse,
    #[cfg(rcc_pll)]
    PllNotConfigured,
    #[cfg(rcc_pll)]
    PllNotSelected,
    #[cfg(rcc_pll)]
    InvalidPllMultiplier,
    #[cfg(rcc_pll)]
    PllArithmeticOverflow,
    #[cfg(rcc_pll)]
    PllConditionsOutsideQualifiedRange,
    #[cfg(rcc_pll)]
    PllInputOutsideQualifiedRange,
    #[cfg(rcc_pll)]
    PllOutputOutsideQualifiedRange,
    #[cfg(rcc_pll)]
    PllInputCrossesBin,
    #[cfg(rcc_pll)]
    PllOutputCrossesBin,
    /// A nondefault inherited reserved/debug value is not qualified for PLL use.
    #[cfg(rcc_pll)]
    PllReservedConfiguration,
    #[cfg(rcc_pll)]
    PllConfigurationTimeout,
    #[cfg(rcc_pll)]
    PllTimeout,
    #[cfg(rcc_hse)]
    InvalidHseBounds,
    #[cfg(rcc_hse)]
    HseOutsideQualifiedRange,
    #[cfg(rcc_hse)]
    HseConditionsOutsideQualifiedRange,
    #[cfg(rcc_hse)]
    HseConditionsDoNotCoverBoard,
    #[cfg(rcc_hse)]
    HsePinsUnavailable,
    #[cfg(rcc_hse)]
    HseNotConfigured,
    #[cfg(rcc_hse)]
    InvalidHseDetector,
    #[cfg(rcc_hse)]
    HseTimeout,
    #[cfg(rcc_hse)]
    HseStopTimeout,
    #[cfg(rcc_hse)]
    HsePinConfigurationTimeout,
    /// Retained RTC/AUTOTRIM ownership conflicts with the requested HSE setup.
    #[cfg(rcc_hse)]
    HseClockInUse,
    #[cfg(rcc_hse)]
    RetainedClockInspectionTimeout,
    /// Recalibrating HSI would interrupt a retained AUTOTRIM or LVD filter.
    #[cfg(rcc_hse)]
    HsiClockInUse,
    /// Active calibration, automatic retrim or a reserved AUTOTRIM source.
    #[cfg(rcc_hse)]
    RetainedAutotrimConfiguration,
    /// The retained RTC source selector is reserved.
    #[cfg(rcc_hse)]
    RetainedRtcConfiguration,
    /// Active AUTOTRIM ETR may occupy the HSE input pad.
    #[cfg(rcc_hse)]
    HsePinInUse,
    /// A monitored external source has a sticky startup or clock-loss fault.
    #[cfg(rcc_hse)]
    ExternalClockFault,
    /// Temporary LSI selection or restoration did not acknowledge.
    #[cfg(rcc_hse)]
    TemporaryClockSwitchTimeout,
    #[cfg(rcc_hse)]
    LsiRestoreTimeout,

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
    /// Temporary LSI did not report stable within the poll budget.
    LsiTimeout,
    /// L083 PLL did not stop before an HSI configuration change.
    #[cfg(rcc_cw32l083_v1)]
    PllStopTimeout,
    /// HSI did not stop before a required trim change.
    HsiStopTimeout,
    /// HSI did not report stable within the poll budget.
    HsiTimeout,
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
        #[cfg(not(rcc_hse))]
        let clocks = configure(config)?;
        #[cfg(rcc_hse)]
        let clocks = configure_hse(config, cs)?;
        CLOCKS.borrow(cs).set(Some(clocks));
        Ok(())
    })
}

#[cfg(rcc_hse)]
pub(crate) fn hse_pin_reserved(pin: u8) -> bool {
    let Some(clocks) = try_clocks() else {
        return false;
    };
    clocks.hse.is_some_and(|mode| {
        crate::RCC_HSE_PINS.0 == Some(pin)
            || (mode == HseMode::Oscillator && crate::RCC_HSE_PINS.1 == Some(pin))
    })
}

// Waits remain bounded by the caller's poll budget; each predicate performs
// one read of the selected PAC register.
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
    // Both FLASH variants have WAIT only; preserve reserved bits 15:3.
    // SYSCTRL_CR2[6:4] is a mirror; using FLASH avoids writing SWD/brake fields.
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

#[cfg(not(rcc_hse))]
fn switch(source: u8, timeout: u32) -> Result<(), Error> {
    pac::SYSCTRL.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_sysclk(source);
    });
    poll(
        || pac::SYSCTRL.cr0().read().sysclk() == source,
        timeout,
        Error::ClockSwitchTimeout,
    )?;
    barrier();
    Ok(())
}

#[cfg(not(rcc_hse))]
fn configure(config: Config) -> Result<Clocks, Error> {
    let clocks = config.frequencies()?;
    let source = pac::SYSCTRL.cr0().read().sysclk();
    let valid_source = matches!(source, 0 | 1 | 3 | 4);
    if !valid_source {
        return Err(Error::InvalidClockSource);
    }
    if !matches!(pac::SYSCTRL.hsi().read().div(), 5 | 6 | 8 | 9 | 11..=15) {
        return Err(Error::InvalidHsiDivider);
    }
    // Factory calibration is a halfword at the source-qualified family address.
    let trim =
        unsafe { core::ptr::read_volatile(crate::RCC_FACTORY_HSI_TRIM_ADDRESS as *const u16) };
    if trim == u16::MAX {
        return Err(Error::InvalidCalibration);
    }
    // Decode the calibration field with the PAC's documented TRIM width.
    let trim = pac::sysctrl::regs::Hsi(u32::from(trim)).trim();
    let hsi = pac::SYSCTRL.hsi().read();
    let needs_trim = hsi.trim() != trim;
    let old_cr1 = pac::SYSCTRL.cr1().read();
    let old_lsi_enable = old_cr1.lsien();
    let needs_bridge = needs_trim;

    // AHBEN is unkeyed on both families. Never inject a key into reserved bits.
    pac::SYSCTRL.ahben().modify(|w| {
        w.set_flash(true);
    });
    poll(
        || pac::SYSCTRL.ahben().read().flash(),
        config.timeout,
        Error::FlashClockTimeout,
    )?;
    // WAIT=2 is documented through 72 MHz, covering legal bootloader clocks.
    // Never decrease latency before the final HCLK has been verified.
    set_flash_latency(crate::RCC_INITIAL_FLASH_WAIT, config.timeout)?;
    // A minimum /4 HCLK guard bounds every legal L052 source below 24 MHz;
    // retain a slower entry divider rather than speeding it up.
    // Avoid /128 unless already selected: LSI /128 makes initialization slow.
    // APB /8 can only slow the bus. No source is changed at this step.
    let guarded_ahb = pac::SYSCTRL
        .cr0()
        .read()
        .hclkprs()
        .max(AHBPrescaler::Div4 as u8);
    pac::SYSCTRL.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_hclkprs(guarded_ahb);
        w.set_pclkprs(APBPrescaler::Div8 as u8);
    });
    poll(
        || {
            let cr0 = pac::SYSCTRL.cr0().read();
            cr0.hclkprs() == guarded_ahb && cr0.pclkprs() == APBPrescaler::Div8 as u8
        },
        config.timeout,
        Error::ClockConfigurationTimeout,
    )?;
    barrier();

    if needs_bridge {
        // UM §4.3.4 forbids live oscillator parameter changes. Use LSI as a
        // temporary bridge even when entered from an external clock: this
        // also prevents its clock-security fallback from restarting HSI.
        // LSI TRIM and WAITCYCLE are left untouched, including if a watchdog
        // or another peripheral had already enabled LSI automatically.
        pac::SYSCTRL.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_lsien(true);
        });
        poll(
            || pac::SYSCTRL.cr1().read().lsien(),
            config.timeout,
            Error::ClockConfigurationTimeout,
        )?;
        poll(
            || pac::SYSCTRL.lsi().read().stable(),
            config.timeout,
            Error::LsiTimeout,
        )?;
        switch(3, config.timeout)?;
    }
    if needs_trim {
        pac::SYSCTRL.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_hsien(false);
        });
        poll(
            || !pac::SYSCTRL.cr1().read().hsien(),
            config.timeout,
            Error::HsiStopTimeout,
        )?;
        poll(
            || !pac::SYSCTRL.hsi().read().stable(),
            config.timeout,
            Error::HsiStopTimeout,
        )?;
        // HSI is stopped and the CPU runs from verified LSI. Preserve all
        // reserved bits; do not invent the nonexistent HSI.WAITCYCLE field
        // mentioned by the manual's generic §4.5.6 prose.
        pac::SYSCTRL.hsi().modify(|w| {
            w.set_trim(trim);
        });
        poll(
            || pac::SYSCTRL.hsi().read().trim() == trim,
            config.timeout,
            Error::ClockConfigurationTimeout,
        )?;
    }

    // Divider-only changes while HSI runs are explicitly allowed by §4.5.2.
    let divider = config.hsi.div as u8;
    pac::SYSCTRL.hsi().modify(|w| {
        w.set_div(divider);
    });
    poll(
        || {
            let hsi = pac::SYSCTRL.hsi().read();
            hsi.div() == divider && hsi.trim() == trim
        },
        config.timeout,
        Error::ClockConfigurationTimeout,
    )?;
    pac::SYSCTRL.cr1().modify(|w| {
        w.set_key(0x5a5a);
        w.set_hsien(true);
    });
    poll(
        || pac::SYSCTRL.cr1().read().hsien(),
        config.timeout,
        Error::ClockConfigurationTimeout,
    )?;
    // STABLE is the current oscillator state. HSIRDY is a clearable edge flag,
    // not a readiness requirement, and may already have been acknowledged.
    poll(
        || pac::SYSCTRL.hsi().read().stable(),
        config.timeout,
        Error::HsiTimeout,
    )?;
    switch(0, config.timeout)?;
    // L052 CLKCCS can force the 8 MHz divider if an external source faults
    // between the pre-switch HSI readback and this switch. Recheck after HSI
    // is selected, when §4.4.3.3 fallback no longer applies to the active source.
    poll(
        || {
            let hsi = pac::SYSCTRL.hsi().read();
            hsi.div() == divider && hsi.trim() == trim
        },
        config.timeout,
        Error::ClockConfigurationTimeout,
    )?;
    poll(
        || pac::SYSCTRL.hsi().read().stable(),
        config.timeout,
        Error::HsiTimeout,
    )?;

    pac::SYSCTRL.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_hclkprs(config.ahb_pre as u8);
        w.set_pclkprs(config.apb_pre as u8);
        w.set_sysclk(0);
    });
    poll(
        || {
            let cr0 = pac::SYSCTRL.cr0().read();
            cr0.hclkprs() == config.ahb_pre as u8
                && cr0.pclkprs() == config.apb_pre as u8
                && cr0.sysclk() == 0
        },
        config.timeout,
        Error::ClockConfigurationTimeout,
    )?;
    barrier();
    // Restore only the software LSI enable bit; hardware-dependent LSI users
    // remain supported and LSI oscillator parameters were never changed.
    if needs_bridge {
        pac::SYSCTRL.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_lsien(old_lsi_enable);
        });
        poll(
            || pac::SYSCTRL.cr1().read().lsien() == old_lsi_enable,
            config.timeout,
            Error::ClockConfigurationTimeout,
        )?;
    }
    let wait = (clocks.hclk_bounds().maximum().0 - 1) / crate::RCC_FLASH_WAIT_STEP_HZ;
    set_flash_latency(wait, config.timeout)?;
    Ok(clocks)
}

#[cfg(rcc_hse)]
fn wait_until(timeout: u32, error: Error, ready: impl FnMut() -> bool) -> Result<(), Error> {
    poll(ready, timeout, error)
}

#[cfg(rcc_hse)]
fn configure_hse(
    config: Config,
    cs: critical_section::CriticalSection<'_>,
) -> Result<Clocks, Error> {
    #[cfg(rcc_lsi_sysclk)]
    if config.sys == Sysclk::LSI {
        return configure_lsi(config, cs);
    }
    #[cfg(all(rcc_lse, rcc_cw32l052_v1))]
    if config.sys == Sysclk::LSE {
        return lse_sysclk::configure(config, cs);
    }
    #[cfg(all(rcc_lse, rcc_cw32l083_v1))]
    if config.sys == Sysclk::LSE {
        return l083_lse_sysclk::configure(config, cs);
    }
    let mut clocks = config.frequencies()?;
    let r = pac::SYSCTRL;
    #[cfg(rcc_lse)]
    let reuse_lse = config
        .lse
        .map(|c| super::lse::preflight(c, cs))
        .transpose()?;
    #[cfg(rcc_lse)]
    if let Some(lse) = config.lse {
        prepare_lse_monitor(lse.poll_budget, cs)?;
    }
    let old_clock = r.cr0().read();
    let old_sources = r.cr1().read();
    let old_hse = r.hse().read();
    #[cfg(rcc_pll)]
    if config.pll.is_some()
        && r.pll().read().reserved_debug().to_bits() != crate::RCC_PLL_RESERVED_DEBUG_DEFAULT
    {
        return Err(Error::PllReservedConfiguration);
    }
    let monitor_hse = config.hse.is_some() || old_sources.hseen();
    let monitor_lse = old_sources.lseen();
    #[cfg(rcc_lse)]
    let monitor_lse = monitor_lse || config.lse.is_some();
    #[cfg(rcc_lse)]
    let old_lsi = config.lse.map(|_| r.lsi().read());
    let needs_lsi = (monitor_hse && old_sources.hseccs()) || (monitor_lse && old_sources.lseccs());
    #[cfg(rcc_lse)]
    let needs_lsi = needs_lsi || config.lse.is_some();
    // HSE/LSE CCS controls are configurable here, not reserved mandatory ones.
    let ccs_unchanged = || {
        let v = r.cr1().read();
        v.clkccs() == old_sources.clkccs()
            && v.hseccs() == old_sources.hseccs()
            && v.lseccs() == old_sources.lseccs()
            && v.lselock() == old_sources.lselock()
            && v.lseen() == old_sources.lseen()
    };
    match old_clock.sysclk() {
        ClockSource::Hsi | ClockSource::Hse | ClockSource::Lsi | ClockSource::Lse => {}
        #[cfg(rcc_cw32l083_v1)]
        ClockSource::Pll => {}
        _ => return Err(Error::InvalidClockSource),
    }
    check_external_faults(monitor_hse, monitor_lse)?;
    let old_hsi = r.hsi().read();
    if !matches!(old_hsi.div(), 5 | 6 | 8 | 9 | 11..=15) {
        return Err(Error::InvalidHsiDivider);
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
    // Inspect AUTOTRIM through its real APBEN2 gate, without reset or writes.
    // Even an unchanged requested trim cannot admit active calibration: it may
    // asynchronously change HSI/LSI underneath initialization and frozen bounds.
    let autotrim = <crate::peripherals::AUTOTRIM as crate::rcc::SealedRccPeripheral>::RCC_INFO
        .inspect_for_init(cs, config.timeout, || pac::AUTOTRIM.cr().read())
        .map_err(|_| Error::RetainedClockInspectionTimeout)?;
    if autotrim.en()
        && (autotrim.md() != pac::autotrim::vals::Mode::Timer
            || autotrim.auto()
            || !matches!(
                autotrim.src(),
                pac::autotrim::vals::Source::HsiOsc
                    | pac::autotrim::vals::Source::Lsi
                    | pac::autotrim::vals::Source::Hse
                    | pac::autotrim::vals::Source::Lse
                    | pac::autotrim::vals::Source::Etr
            ))
    {
        return Err(Error::RetainedAutotrimConfiguration);
    }
    if needs_trim && autotrim.en() && autotrim.src() == pac::autotrim::vals::Source::HsiOsc {
        return Err(Error::HsiClockInUse);
    }
    if needs_trim
        && pac::LVD.cr0().read().en()
        && pac::LVD.cr1().read().flten()
        && pac::LVD.cr1().read().fltclk()
    {
        return Err(Error::HsiClockInUse);
    }
    let rtc_source = <crate::peripherals::RTC as crate::rcc::SealedRccPeripheral>::RCC_INFO
        .inspect_for_init(cs, config.timeout, || pac::RTC.cr1().read().source())
        .map_err(|_| Error::RetainedClockInspectionTimeout)?;
    if !matches!(
        rtc_source,
        pac::rtc::vals::Source::Lse
            | pac::rtc::vals::Source::Lsi
            | pac::rtc::vals::Source::HseDiv128
            | pac::rtc::vals::Source::HseDiv256
            | pac::rtc::vals::Source::HseDiv512
            | pac::rtc::vals::Source::HseDiv1024
    ) {
        return Err(Error::RetainedRtcConfiguration);
    }
    let mut preserve_hse = false;
    if let Some(hse) = config.hse {
        // AUTOTRIM ETR can route to PF0. Without a proven nonconflicting route,
        // reject the request before changing any oscillator or pad.
        if autotrim.en() && autotrim.src() == pac::autotrim::vals::Source::Etr {
            return Err(Error::HsePinInUse);
        }
        let rtc_hse = matches!(
            rtc_source,
            pac::rtc::vals::Source::HseDiv128
                | pac::rtc::vals::Source::HseDiv256
                | pac::rtc::vals::Source::HseDiv512
                | pac::rtc::vals::Source::HseDiv1024
        );
        preserve_hse =
            rtc_hse || (autotrim.en() && autotrim.src() == pac::autotrim::vals::Source::Hse);
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
        w.set_hclkprs(guard_hclk);
        w.set_pclkprs(guard_pclk);
    });
    wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
        let v = r.cr0().read();
        v.hclkprs() == guard_hclk && v.pclkprs() == guard_pclk
    })?;
    barrier();

    r.cr1().modify(|w| {
        w.set_key(0x5a5a);
        w.set_hsien(true);
    });
    wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
        let v = r.cr1().read();
        v.hsien() && ccs_unchanged()
    })?;
    wait_until(config.timeout, Error::HsiTimeout, || {
        r.hsi().read().stable()
    })?;
    // Leave the legal incoming source via unchanged HSI before any retuning.
    r.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_sysclk(ClockSource::Hsi);
    });
    wait_until(config.timeout, Error::ClockSwitchTimeout, || {
        r.cr0().read().sysclk() == ClockSource::Hsi
    })?;
    barrier();
    // Own RM section 4.5 permits PLL transitions only with HSI/HSE. The
    // unchanged-HSI escape above is therefore required before the LSI bridge.
    #[cfg(rcc_cw32l083_v1)]
    let needs_bridge = needs_trim || old_sources.pllen() || old_clock.sysclk() == ClockSource::Pll;
    #[cfg(rcc_cw32l052_v1)]
    let needs_bridge = needs_trim;
    let lsi_was_enabled = old_sources.lsien();
    // A retained enabled external detector requires unchanged legal LSI,
    // even without a trim bridge. CLKCCS alone does not enable a detector.
    if needs_bridge || needs_lsi {
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
    }
    if needs_bridge {
        r.cr0().modify(|w| {
            w.set_key(0x5a5a);
            w.set_sysclk(ClockSource::Lsi);
        });
        wait_until(config.timeout, Error::TemporaryClockSwitchTimeout, || {
            r.cr0().read().sysclk() == ClockSource::Lsi
        })?;
        barrier();
    }
    // Stop and observe PLL before changing its HSI/HSE source or HSI divider;
    // configuration fields are not written until both stop acknowledgments hold.
    #[cfg(rcc_cw32l083_v1)]
    {
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_pllen(false);
        });
        wait_until(config.timeout, Error::PllStopTimeout, || {
            !r.cr1().read().pllen()
        })?;
        wait_until(config.timeout, Error::PllStopTimeout, || {
            !r.pll().read().stable()
        })?;
    }
    if needs_trim {
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_hsien(false);
        });
        wait_until(config.timeout, Error::HsiStopTimeout, || {
            !r.cr1().read().hsien()
        })?;
        wait_until(config.timeout, Error::HsiStopTimeout, || {
            !r.hsi().read().stable()
        })?;
        r.hsi().modify(|w| w.set_trim(trim));
        wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
            r.hsi().read().trim() == trim
        })?;
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_hsien(true);
        });
        wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
            let v = r.cr1().read();
            v.hsien() && ccs_unchanged()
        })?;
        wait_until(config.timeout, Error::HsiTimeout, || {
            r.hsi().read().stable()
        })?;
    }
    if needs_bridge {
        r.cr0().modify(|w| {
            w.set_key(0x5a5a);
            w.set_sysclk(ClockSource::Hsi);
        });
        wait_until(config.timeout, Error::ClockSwitchTimeout, || {
            r.cr0().read().sysclk() == ClockSource::Hsi
        })?;
        barrier();
        if !lsi_was_enabled && !needs_lsi {
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
        let hsi = r.hsi().read();
        hsi.div() == config.hsi.div as u8
            && hsi.trim() == trim
            && r.cr0().read().sysclk() == ClockSource::Hsi
    })?;
    wait_until(config.timeout, Error::HsiTimeout, || {
        r.hsi().read().stable()
    })?;

    if let Some(hse) = config.hse.filter(|_| !preserve_hse) {
        // Even an already-ready source is stopped before pin or parameter
        // changes: STABLE alone is not continuous oscillator validity.
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_hseen(false);
        });
        wait_until(config.timeout, Error::HseStopTimeout, || {
            !r.cr1().read().hseen()
        })?;
        wait_until(config.timeout, Error::HseStopTimeout, || {
            !r.hse().read().stable()
        })?;
        crate::rcc_configure_hse_pins(hse.mode == HseMode::Bypass, config.timeout, cs)?;
        let detector = hse.detector_count()?;
        r.hse().modify(|w| {
            w.set_mode(hse.mode == HseMode::Bypass);
            w.set_driver(hse.drive);
            w.set_freqrange(hse.range());
            // L052 has distinct pre-start fields. All parameters must be set
            // before enabling; reserved bits31:24 are preserved by modify.
            #[cfg(rcc_cw32l052_v1)]
            {
                w.set_pdriver(hse.drive);
                w.set_pfreqrange(hse.range());
            }
            w.set_waitcycle(pac::sysctrl::vals::HseWait::Cycles262144);
            w.set_flt(false);
            w.set_detcnt(detector);
        });
        wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
            hse_parameters_match(hse).unwrap_or(false)
        })?;
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_hseen(true);
        });
        wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
            r.cr1().read().hseen()
        })?;
        wait_until(config.timeout, Error::HseTimeout, || {
            r.hse().read().stable()
        })?;
    }
    #[cfg(rcc_pll)]
    if let Some(pll) = config.pll {
        let parameters = pll.parameters(&config)?;
        // Both PLLEN and the real STABLE latch were observed clear before any
        // reference changes above. Modify preserves reserved/debug defaults.
        r.pll().modify(|w| {
            w.set_source(parameters.source);
            w.set_freqin(parameters.input_range);
            w.set_mul(pll.mul);
            w.set_freqout(parameters.output_range);
            w.set_waitcycle(pac::sysctrl::vals::PllWait::from_bits(
                crate::RCC_PLL_STARTUP_ENCODING,
            ));
        });
        wait_until(config.timeout, Error::PllConfigurationTimeout, || {
            pll_parameters_match(pll, &parameters)
                && !r.cr1().read().pllen()
                && !r.pll().read().stable()
        })?;
        // Recheck the complete selected dependency immediately before enabling
        // PLL. STABLE alone cannot establish source mode, pads or fault state.
        verify_pll_reference(pll, &config, trim, cs)?;
        check_external_faults(monitor_hse, monitor_lse)?;
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_pllen(true);
        });
        wait_until(config.timeout, Error::PllConfigurationTimeout, || {
            r.cr1().read().pllen() && ccs_unchanged() && pll_parameters_match(pll, &parameters)
        })?;
        // PLLRDY is a clearable event; only PLL.STABLE is the startup handshake.
        wait_until(config.timeout, Error::PllTimeout, || {
            r.pll().read().stable()
        })?;
    }
    check_external_faults(monitor_hse, monitor_lse)?;
    let sysclk = match config.sys {
        Sysclk::HSI => ClockSource::Hsi,
        Sysclk::HSE => ClockSource::Hse,
        #[cfg(rcc_lsi_sysclk)]
        Sysclk::LSI => return Err(Error::InvalidClockSource),
        // The explicit target branch above bypasses this old auxiliary path.
        #[cfg(rcc_lse)]
        Sysclk::LSE => return Err(Error::InvalidClockSource),
        #[cfg(rcc_pll)]
        Sysclk::PLL => ClockSource::Pll,
    };
    #[cfg(rcc_pll)]
    if let Some(pll) = config.pll {
        verify_pll_reference(pll, &config, trim, cs)?;
        if !r.cr1().read().pllen() || !pll_parameters_match(pll, &pll.parameters(&config)?) {
            return Err(Error::PllConfigurationTimeout);
        }
        if !r.pll().read().stable() {
            return Err(Error::PllTimeout);
        }
        check_external_faults(monitor_hse, monitor_lse)?;
    }
    // Keep restrictive buses while selecting the source. Install final dividers
    // only after mux acknowledgment, so a slower target never exposes old HSI
    // through newly weakened final buses.
    if config.sys != Sysclk::HSI {
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
    // The retained HSI escape can be used at the final divisors too.
    if config.sys != Sysclk::HSI || config.hse.is_some() {
        upper_hclk = upper_hclk.max(
            crate::rcc::ClockBounds::hsi(config.hsi.div.divisor())
                .divided_by(config.ahb_pre.divisor())
                .maximum()
                .0,
        );
        #[cfg(rcc_cw32l052_v1)]
        {
            upper_hclk = upper_hclk.max(
                crate::rcc::ClockBounds::hsi(crate::RCC_FIXED_CCS_HSI_DIVISOR)
                    .divided_by(config.ahb_pre.divisor())
                    .maximum()
                    .0,
            );
        }
    }
    // Verify the complete final source tree before reducing Flash latency and
    // again after the Flash handshake and the last requested LSE operation.
    // A requested LSE may change only its own enable/detector policy here.
    let verify_final_tree = |lse_started: bool| -> Result<(), Error> {
        if r.cr0().read().sysclk() != sysclk {
            return Err(Error::ClockSwitchTimeout);
        }
        check_external_faults(monitor_hse, monitor_lse)?;
        if !r.hsi().read().stable() {
            return Err(Error::HsiTimeout);
        }
        let final_sources = r.cr1().read();
        let expected_lsi = lsi_was_enabled || needs_lsi;
        if final_sources.clkccs() != old_sources.clkccs()
            || final_sources.hseccs() != old_sources.hseccs()
            || final_sources.lseccs() != (old_sources.lseccs() || lse_started)
            || final_sources.lselock() != old_sources.lselock()
            || final_sources.lseen() != (old_sources.lseen() || lse_started)
            || !final_sources.hsien()
            || final_sources.lsien() != expected_lsi
            || final_sources.hseen() != monitor_hse
            || r.hsi().read().div() != config.hsi.div as u8
            || r.hsi().read().trim() != trim
            || r.cr0().read().hclkprs() != config.ahb_pre as u8
            || r.cr0().read().pclkprs() != config.apb_pre as u8
        {
            return Err(Error::ClockConfigurationTimeout);
        }
        #[cfg(rcc_pll)]
        match config.pll {
            Some(pll) => {
                if !final_sources.pllen() || !pll_parameters_match(pll, &pll.parameters(&config)?) {
                    return Err(Error::PllConfigurationTimeout);
                }
                if !r.pll().read().stable() {
                    return Err(Error::PllTimeout);
                }
            }
            None => {
                if final_sources.pllen() || r.pll().read().stable() {
                    return Err(Error::PllStopTimeout);
                }
            }
        }
        if let Some(hse) = config.hse {
            verify_hse_source(hse, config.timeout, cs)?;
        }
        #[cfg(rcc_lse)]
        if let Some(old_lsi) = old_lsi {
            let current = r.lsi().read();
            if !r.cr1().read().lsien()
                || !current.stable()
                || current.trim() != old_lsi.trim()
                || current.waitcycle() != old_lsi.waitcycle()
            {
                return Err(Error::LsiTimeout);
            }
        }
        Ok(())
    };
    verify_final_tree(false)?;
    let final_flash_wait = (upper_hclk - 1) / crate::RCC_FLASH_WAIT_STEP_HZ;
    set_flash_latency(final_flash_wait, config.timeout)?;
    verify_final_tree(false)?;
    #[cfg(rcc_lse)]
    if let Some(lse) = config.lse {
        super::lse::start(lse, reuse_lse.unwrap(), cs)?;
        let after = r.cr1().read();
        if after.clkccs() != old_sources.clkccs()
            || after.hseccs() != old_sources.hseccs()
            || after.lselock() != old_sources.lselock()
            || !after.lseccs()
        {
            return Err(Error::ClockConfigurationTimeout);
        }
        super::lse::verify(lse, cs)?;
    }
    #[cfg(rcc_lse)]
    let lse_started = config.lse.is_some();
    #[cfg(not(rcc_lse))]
    let lse_started = false;
    verify_final_tree(lse_started)?;
    if !r.ahben().read().flash() {
        return Err(Error::FlashClockTimeout);
    }
    if pac::FLASH.cr2().read().wait() != final_flash_wait as u8 {
        return Err(Error::FlashLatencyTimeout);
    }
    // Freeze inherited ownership too, so safe GPIO remains blocked for the
    // entire boot even if later unsupported raw PAC writes disable HSE.
    // An inherited crystal keeps both pads reserved for the boot even if an
    // unowned source was successfully stopped and reconfigured as bypass.
    // This is conservative ownership policy, not a claim PF1 remains in use.
    if old_sources.hseen() {
        if !old_hse.mode() {
            clocks.hse = Some(HseMode::Oscillator);
        } else if clocks.hse.is_none() {
            clocks.hse = Some(HseMode::Bypass);
        }
    }
    Ok(clocks)
}

// Exact factory-LSI targets share this bounded state machine. Each family's
// generated collectors retain its own native controls and ownership graph.
#[cfg(all(rcc_lsi_sysclk, rcc_cw32l052_v1))]
use crate::{
    rcc_l052_lsi_configure_hse_pins as rcc_lsi_configure_hse_pins,
    rcc_l052_lsi_configure_lse_pins as rcc_lsi_configure_lse_pins,
    rcc_l052_lsi_consumers as rcc_lsi_consumers,
    rcc_l052_lsi_enable_hse_pins as rcc_lsi_enable_hse_pins,
    rcc_l052_lsi_lse_preflight as rcc_lsi_lse_preflight, rcc_l052_lsi_pads as rcc_lsi_pads,
    RccL052LsiConsumers as RccLsiConsumers, RccL052LsiLseSnapshot as RccLsiLseSnapshot,
    RccL052LsiPads as RccLsiPads, RCC_L052_LSI_CR0_KEY_MASK as LSI_CR0_KEY_MASK,
    RCC_L052_LSI_CR1_KEY_MASK as LSI_CR1_KEY_MASK, RCC_L052_LSI_IER_KEY_MASK as LSI_IER_KEY_MASK,
};
#[cfg(all(rcc_lsi_sysclk, rcc_cw32l083_v1))]
use crate::{
    rcc_l083_lsi_configure_lse_pins as rcc_lsi_configure_lse_pins,
    rcc_l083_lsi_consumers as rcc_lsi_consumers,
    rcc_l083_lsi_enable_hse_pins as rcc_lsi_enable_hse_pins,
    rcc_l083_lsi_lse_preflight as rcc_lsi_lse_preflight, rcc_l083_lsi_pads as rcc_lsi_pads,
    RccL083LsiConsumers as RccLsiConsumers, RccL083LsiLseSnapshot as RccLsiLseSnapshot,
    RccL083LsiPads as RccLsiPads, RCC_L083_LSI_CR0_KEY_MASK as LSI_CR0_KEY_MASK,
    RCC_L083_LSI_CR1_KEY_MASK as LSI_CR1_KEY_MASK, RCC_L083_LSI_IER_KEY_MASK as LSI_IER_KEY_MASK,
};

// Exact L052/L083 factory-LSI target. Keep this path separate from the established
// HSI/HSE and auxiliary/system-LSE paths: admission and owned edges differ.
#[cfg(rcc_lsi_sysclk)]
#[derive(Clone, Copy, Eq, PartialEq)]
enum LsiEntryClass {
    Cold,
    LiveReady,
    LiveStarting,
}

#[cfg(rcc_lsi_sysclk)]
#[derive(Clone, Copy)]
struct LsiIdentity {
    clock: pac::sysctrl::regs::Cr0,
    sources: pac::sysctrl::regs::Cr1,
    ier: pac::sysctrl::regs::Ier,
    hsi: pac::sysctrl::regs::Hsi,
    hse: pac::sysctrl::regs::Hse,
    lse: pac::sysctrl::regs::Lse,
    lsi: pac::sysctrl::regs::Lsi,
    flags: pac::sysctrl::regs::Isr,
    pending: bool,
    #[cfg(rcc_cw32l083_v1)]
    cr2: pac::sysctrl::regs::Cr2,
    #[cfg(rcc_cw32l083_v1)]
    pll: pac::sysctrl::regs::Pll,
    #[cfg(rcc_cw32l083_v1)]
    irq_enabled: bool,
    #[cfg(rcc_cw32l083_v1)]
    fault_irq_enabled: bool,
    #[cfg(rcc_cw32l083_v1)]
    fault_pending: bool,
    #[cfg(rcc_cw32l083_v1)]
    flash_gate: bool,
    #[cfg(rcc_cw32l083_v1)]
    flash_reset: bool,
}

#[cfg(rcc_lsi_sysclk)]
impl LsiIdentity {
    fn read() -> Self {
        let r = pac::SYSCTRL;
        let mut clock = r.cr0().read();
        let mut sources = r.cr1().read();
        let mut ier = r.ier().read();
        clock.0 &= !LSI_CR0_KEY_MASK;
        sources.0 &= !LSI_CR1_KEY_MASK;
        ier.0 &= !LSI_IER_KEY_MASK;
        #[cfg(rcc_cw32l083_v1)]
        let mut cr2 = r.cr2().read();
        #[cfg(rcc_cw32l083_v1)]
        {
            cr2.0 &= !crate::RCC_L083_LSI_CR2_KEY_MASK;
        }
        #[cfg(rcc_cw32l083_v1)]
        let flash = <crate::peripherals::FLASH as crate::rcc::SealedRccPeripheral>::RCC_INFO;
        Self {
            clock,
            sources,
            ier,
            hsi: r.hsi().read(),
            hse: r.hse().read(),
            lse: r.lse().read(),
            lsi: r.lsi().read(),
            flags: r.isr().read(),
            pending: cortex_m::peripheral::NVIC::is_pending(pac::Interrupt::SYSCTRL),
            #[cfg(rcc_cw32l083_v1)]
            cr2,
            #[cfg(rcc_cw32l083_v1)]
            pll: r.pll().read(),
            #[cfg(rcc_cw32l083_v1)]
            irq_enabled: cortex_m::peripheral::NVIC::is_enabled(pac::Interrupt::SYSCTRL),
            #[cfg(rcc_cw32l083_v1)]
            fault_irq_enabled: cortex_m::peripheral::NVIC::is_enabled(pac::Interrupt::CLKFAULT),
            #[cfg(rcc_cw32l083_v1)]
            fault_pending: cortex_m::peripheral::NVIC::is_pending(pac::Interrupt::CLKFAULT),
            #[cfg(rcc_cw32l083_v1)]
            flash_gate: flash.is_enabled(),
            #[cfg(rcc_cw32l083_v1)]
            flash_reset: flash.reset_asserted(),
        }
    }

    fn external_faults(self, hse: bool, lse: bool) -> Result<(), Error> {
        #[cfg(rcc_cw32l083_v1)]
        if (hse || lse) && self.fault_pending {
            return Err(Error::ExternalClockFault);
        }
        if (hse && (self.flags.hsefail() || self.flags.hsefault()))
            || (lse && (self.flags.lsefail() || self.flags.lsefault()))
        {
            Err(Error::ExternalClockFault)
        } else {
            Ok(())
        }
    }

    fn selected_ready(self) -> bool {
        match self.clock.sysclk() {
            ClockSource::Hsi => self.sources.hsien() && self.hsi.stable() && self.flags.hsistable(),
            ClockSource::Hse => self.sources.hseen() && self.hse.stable() && self.flags.hsestable(),
            ClockSource::Lsi => self.sources.lsien() && self.lsi.stable() && self.flags.lsistable(),
            ClockSource::Lse => self.sources.lseen() && self.lse.stable() && self.flags.lsestable(),
            #[cfg(rcc_cw32l083_v1)]
            ClockSource::Pll => self.sources.pllen() && self.pll.stable() && self.flags.pllstable(),
            _ => false,
        }
    }
}

// Each pending value is one explicit owned write. Equality permits its exact
// old or target word until acknowledgment, never an arbitrary field encoding.
// Readiness permissions are phase-local; none relax the permanent LSI latch.
#[cfg(rcc_lsi_sysclk)]
#[derive(Clone, Copy)]
enum LsiTransition {
    Steady,
    #[cfg(rcc_cw32l083_v1)]
    PllStopping(pac::sysctrl::regs::Cr1),
    #[cfg(rcc_cw32l083_v1)]
    FlashWait(pac::sysctrl::regs::Cr2, pac::flash::regs::Cr2),
    #[cfg(rcc_cw32l083_v1)]
    HsePad(bool, u8),
    Clock(pac::sysctrl::regs::Cr0),
    LsiTrim(pac::sysctrl::regs::Lsi),
    LsiRequest(pac::sysctrl::regs::Cr1),
    LsiReady,
    HsiStopRequest(pac::sysctrl::regs::Cr1),
    HsiStopped,
    HsiTrim(pac::sysctrl::regs::Hsi),
    HsiRequest(pac::sysctrl::regs::Cr1),
    HsiReady,
    HsiDivider(pac::sysctrl::regs::Hsi),
    HseParameters(pac::sysctrl::regs::Hse),
    HseRequest(pac::sysctrl::regs::Cr1),
    HseReady,
    LseParameters(pac::sysctrl::regs::Lse),
    LseRequest(pac::sysctrl::regs::Cr1),
    LseReady,
}

#[cfg(rcc_lsi_sysclk)]
struct LsiSysclkState {
    // This record and class are immutable, including through owned pad changes.
    entry: LsiIdentity,
    class: LsiEntryClass,
    factory_lsi: u16,
    factory_hsi: u16,
    clock: pac::sysctrl::regs::Cr0,
    sources: pac::sysctrl::regs::Cr1,
    hsi: pac::sysctrl::regs::Hsi,
    hse: pac::sysctrl::regs::Hse,
    lse: pac::sysctrl::regs::Lse,
    lsi: pac::sysctrl::regs::Lsi,
    // HSI, HSE, LSE, LSI. A true observation is sticky except owned HSI stop.
    ready: [bool; 4],
    requested: bool,
    lsirdy_seen: bool,
    pending_seen: bool,
    cold_lse_requested: bool,
    lse_config: Option<super::Lse>,
    lse_pads_configured: bool,
    lse_requested: bool,
    lserdy_seen: bool,
    monitor_hse: bool,
    monitor_lse: bool,
    timeout: u32,
    consumers: Option<RccLsiConsumers>,
    pads: Option<RccLsiPads>,
    lse_preflight: Option<RccLsiLseSnapshot>,
    #[cfg(rcc_cw32l083_v1)]
    pll_stopped: bool,
    #[cfg(rcc_cw32l083_v1)]
    pll_ready: bool,
    #[cfg(rcc_cw32l083_v1)]
    pll_bridge: Option<ClockSource>,
    #[cfg(rcc_cw32l083_v1)]
    cr2: pac::sysctrl::regs::Cr2,
    #[cfg(rcc_cw32l083_v1)]
    flash_gate: bool,
    #[cfg(rcc_cw32l083_v1)]
    flash_cr2: Option<pac::flash::regs::Cr2>,
    #[cfg(rcc_cw32l083_v1)]
    events: [bool; 4],
    #[cfg(rcc_cw32l083_v1)]
    event_owned: [bool; 4],
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
        // No inspection gate or source/pad/Flash write precedes this record.
        let entry = LsiIdentity::read();
        let raw_hsi =
            unsafe { core::ptr::read_volatile(crate::RCC_FACTORY_HSI_TRIM_ADDRESS as *const u16) };
        let raw_lsi =
            unsafe { core::ptr::read_volatile(crate::RCC_LSI_FACTORY_TRIM_ADDRESS as *const u16) };
        let monitor_hse = config.hse.is_some()
            || entry.sources.hseen()
            || entry.sources.hseccs()
            || entry.clock.sysclk() == ClockSource::Hse;
        #[cfg(rcc_cw32l083_v1)]
        let monitor_hse = monitor_hse
            || (entry.sources.pllen()
                && matches!(
                    entry.pll.source(),
                    pac::sysctrl::vals::PllSource::HseCrystal
                        | pac::sysctrl::vals::PllSource::HseBypass
                ));
        let monitor_lse = config.lse.is_some()
            || entry.sources.lseen()
            || entry.sources.lseccs()
            || entry.clock.sysclk() == ClockSource::Lse;
        entry.external_faults(monitor_hse, monitor_lse)?;
        // Reject erased halfwords before native TRIM masking. Zero is valid.
        if raw_hsi == u16::MAX {
            return Err(Error::InvalidCalibration);
        }
        if raw_lsi == u16::MAX {
            return Err(Error::InvalidLsiCalibration);
        }
        let mut factory_hsi = entry.hsi;
        factory_hsi.set_trim(raw_hsi);
        let mut factory_lsi = entry.lsi;
        factory_lsi.set_trim(raw_lsi);
        if !matches!(
            entry.clock.sysclk(),
            ClockSource::Hsi | ClockSource::Hse | ClockSource::Lsi | ClockSource::Lse
        ) && {
            #[cfg(rcc_cw32l083_v1)]
            {
                entry.clock.sysclk() != ClockSource::Pll
            }
            #[cfg(rcc_cw32l052_v1)]
            {
                true
            }
        } {
            return Err(Error::InvalidClockSource);
        }
        if !matches!(entry.hsi.div(), 5 | 6 | 8 | 9 | 11..=15) {
            return Err(Error::InvalidHsiDivider);
        }
        if entry.hsi.stable() != entry.flags.hsistable()
            || entry.hse.stable() != entry.flags.hsestable()
            || entry.lse.stable() != entry.flags.lsestable()
            || entry.sources.hsien() != entry.hsi.stable()
            || entry.sources.hseen() != entry.hse.stable()
            || entry.sources.lseen() != entry.lse.stable()
            || !entry.selected_ready()
        {
            return Err(Error::InvalidEntryClock);
        }
        #[cfg(rcc_cw32l083_v1)]
        if entry.sources.pllen() != entry.pll.stable()
            || entry.pll.stable() != entry.flags.pllstable()
            || entry.cr2.flashwait() > 2
            || entry.flash_reset
        {
            return Err(Error::InvalidEntryClock);
        }
        let selected_lsi = entry.clock.sysclk() == ClockSource::Lsi;
        if (entry.sources.lsien() || selected_lsi || entry.lsi.stable() || entry.flags.lsistable())
            && entry.lsi.trim() != factory_lsi.trim()
        {
            return Err(Error::LsiCalibrationInUse);
        }
        if entry.lsi.stable() != entry.flags.lsistable() {
            return Err(Error::LsiClockInUse);
        }
        let detector = entry.sources.hseccs() || entry.sources.lseccs();
        if detector && !(entry.sources.lsien() && entry.lsi.stable()) {
            return Err(Error::LsiClockInUse);
        }
        let class = if !entry.sources.lsien() {
            if selected_lsi
                || entry.lsi.stable()
                || detector
                || entry.ier.lsirdy()
                || entry.flags.lsirdy()
                || entry.pending
            {
                return Err(Error::LsiClockInUse);
            }
            LsiEntryClass::Cold
        } else if entry.lsi.stable() {
            LsiEntryClass::LiveReady
        } else {
            if selected_lsi || detector {
                return Err(Error::LsiClockInUse);
            }
            LsiEntryClass::LiveStarting
        };
        Ok(Self {
            entry,
            class,
            factory_lsi: factory_lsi.trim(),
            factory_hsi: factory_hsi.trim(),
            clock: entry.clock,
            sources: entry.sources,
            hsi: entry.hsi,
            hse: entry.hse,
            lse: entry.lse,
            lsi: entry.lsi,
            ready: [
                entry.hsi.stable(),
                entry.hse.stable(),
                entry.lse.stable(),
                entry.lsi.stable(),
            ],
            requested: false,
            lsirdy_seen: entry.flags.lsirdy(),
            pending_seen: entry.pending,
            cold_lse_requested: config.lse.is_some() && !entry.sources.lseen(),
            lse_config: config.lse,
            lse_pads_configured: false,
            lse_requested: false,
            lserdy_seen: entry.flags.lserdy(),
            monitor_hse,
            monitor_lse,
            timeout: config.timeout,
            consumers: None,
            pads: None,
            lse_preflight: None,
            #[cfg(rcc_cw32l083_v1)]
            pll_stopped: !entry.sources.pllen(),
            #[cfg(rcc_cw32l083_v1)]
            pll_ready: entry.pll.stable(),
            #[cfg(rcc_cw32l083_v1)]
            pll_bridge: None,
            #[cfg(rcc_cw32l083_v1)]
            cr2: entry.cr2,
            #[cfg(rcc_cw32l083_v1)]
            flash_gate: entry.flash_gate,
            #[cfg(rcc_cw32l083_v1)]
            flash_cr2: None,
            #[cfg(rcc_cw32l083_v1)]
            events: [
                entry.flags.hsirdy(),
                entry.flags.hserdy(),
                entry.flags.lserdy(),
                entry.flags.lsirdy(),
            ],
            #[cfg(rcc_cw32l083_v1)]
            event_owned: [false; 4],
        })
    }

    fn check_identity(&mut self, phase: LsiTransition) -> Result<LsiIdentity, Error> {
        let actual = LsiIdentity::read();
        actual.external_faults(self.monitor_hse, self.monitor_lse)?;
        let clock_target = match phase {
            LsiTransition::Clock(target) => target,
            _ => self.clock,
        };
        let sources_target = match phase {
            #[cfg(rcc_cw32l083_v1)]
            LsiTransition::PllStopping(target) => target,
            LsiTransition::LsiRequest(target)
            | LsiTransition::HsiStopRequest(target)
            | LsiTransition::HsiRequest(target)
            | LsiTransition::HseRequest(target)
            | LsiTransition::LseRequest(target) => target,
            _ => self.sources,
        };
        let hsi_target = match phase {
            LsiTransition::HsiTrim(target) | LsiTransition::HsiDivider(target) => target,
            _ => self.hsi,
        };
        let hse_target = match phase {
            LsiTransition::HseParameters(target) => target,
            _ => self.hse,
        };
        let lse_target = match phase {
            LsiTransition::LseParameters(target) => target,
            _ => self.lse,
        };
        let lsi_target = match phase {
            LsiTransition::LsiTrim(target) => target,
            _ => self.lsi,
        };
        let hsi_word = actual.hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK;
        let hse_word = actual.hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK;
        let lse_word = actual.lse.0 & crate::RCC_LSI_LSE_PARAMETERS_MASK;
        let lsi_word = actual.lsi.0 & crate::RCC_LSI_PARAMETERS_MASK;
        if (actual.clock.0 != self.clock.0 && actual.clock.0 != clock_target.0)
            || (actual.sources.0 != self.sources.0 && actual.sources.0 != sources_target.0)
            || actual.ier.0 != self.entry.ier.0
            || (hsi_word != self.hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK
                && hsi_word != hsi_target.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
            || (hse_word != self.hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK
                && hse_word != hse_target.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
            || (lse_word != self.lse.0 & crate::RCC_LSI_LSE_PARAMETERS_MASK
                && lse_word != lse_target.0 & crate::RCC_LSI_LSE_PARAMETERS_MASK)
            || (lsi_word != self.lsi.0 & crate::RCC_LSI_PARAMETERS_MASK
                && lsi_word != lsi_target.0 & crate::RCC_LSI_PARAMETERS_MASK)
        {
            return Err(Error::LsiClockInUse);
        }
        #[cfg(rcc_cw32l083_v1)]
        self.check_l083_identity(actual, phase)?;
        let observed = [
            actual.hsi.stable(),
            actual.hse.stable(),
            actual.lse.stable(),
            actual.lsi.stable(),
        ];
        let mirrors = [
            actual.flags.hsistable(),
            actual.flags.hsestable(),
            actual.flags.lsestable(),
            actual.flags.lsistable(),
        ];
        let starts = [
            matches!(
                phase,
                LsiTransition::HsiRequest(_) | LsiTransition::HsiReady
            ),
            matches!(
                phase,
                LsiTransition::HseRequest(_) | LsiTransition::HseReady
            ),
            matches!(
                phase,
                LsiTransition::LseRequest(_) | LsiTransition::LseReady
            ),
            self.requested
                && matches!(
                    phase,
                    LsiTransition::LsiRequest(_) | LsiTransition::LsiReady
                ),
        ];
        let stopping_hsi = matches!(
            phase,
            LsiTransition::HsiStopRequest(_) | LsiTransition::HsiStopped
        );
        let lost = [
            Error::HsiTimeout,
            Error::HseTimeout,
            Error::LseNotReady,
            Error::LsiTimeout,
        ];
        for index in 0..4 {
            if self.ready[index]
                && !(index == 0 && stopping_hsi)
                && (!observed[index] || !mirrors[index])
            {
                return Err(lost[index]);
            }
            if observed[index] != mirrors[index] {
                return Err(Error::LsiClockInUse);
            }
            if index == 0 && stopping_hsi {
                if !self.ready[index] && observed[index] {
                    return Err(Error::LsiClockInUse);
                }
            } else if observed[index] != self.ready[index] && !starts[index] {
                return Err(Error::LsiClockInUse);
            }
        }
        if !actual.selected_ready() {
            return Err(Error::InvalidEntryClock);
        }
        #[cfg(rcc_cw32l052_v1)]
        if !self.requested {
            // Live-starting may not be resampled as live-ready during admission.
            if actual.lsi.stable() != self.entry.lsi.stable()
                || actual.flags.lsirdy() != self.entry.flags.lsirdy()
                || actual.pending != self.entry.pending
            {
                return Err(Error::LsiClockInUse);
            }
        } else if (self.lsirdy_seen && !actual.flags.lsirdy())
            || (self.pending_seen && !actual.pending)
        {
            return Err(Error::LsiClockInUse);
        }
        if self.cold_lse_requested {
            if (!self.lse_requested && actual.flags.lserdy() != self.entry.flags.lserdy())
                || (self.lse_requested && self.lserdy_seen && !actual.flags.lserdy())
            {
                return Err(Error::LseClockInUse);
            }
        }
        self.ready = observed;
        self.lsirdy_seen |= actual.flags.lsirdy();
        self.pending_seen |= actual.pending;
        self.lserdy_seen |= actual.flags.lserdy();
        // Advance only the known owned delta, immediately after its complete
        // normalized acknowledgment. A later old value is then a refusal.
        if actual.clock.0 == clock_target.0 {
            self.clock = clock_target;
        }
        if actual.sources.0 == sources_target.0 {
            self.sources = sources_target;
        }
        if hsi_word == hsi_target.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK {
            self.hsi = hsi_target;
        }
        if hse_word == hse_target.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK {
            self.hse = hse_target;
        }
        if lse_word == lse_target.0 & crate::RCC_LSI_LSE_PARAMETERS_MASK {
            self.lse = lse_target;
        }
        if lsi_word == lsi_target.0 & crate::RCC_LSI_PARAMETERS_MASK {
            self.lsi = lsi_target;
        }
        Ok(actual)
    }

    #[cfg(rcc_cw32l083_v1)]
    fn check_l083_identity(
        &mut self,
        actual: LsiIdentity,
        phase: LsiTransition,
    ) -> Result<(), Error> {
        let (system_target, flash_target) = match phase {
            LsiTransition::FlashWait(system, flash) => (system, Some(flash)),
            _ => (self.cr2, self.flash_cr2),
        };
        if (actual.cr2.0 != self.cr2.0 && actual.cr2.0 != system_target.0)
            || actual.irq_enabled != self.entry.irq_enabled
            || actual.fault_irq_enabled != self.entry.fault_irq_enabled
            || actual.fault_pending != self.entry.fault_pending
            || actual.flash_gate != self.flash_gate
            || actual.flash_reset != self.entry.flash_reset
        {
            return Err(Error::LsiClockInUse);
        }
        if let Some(expected) = self.flash_cr2 {
            // The central gate and reset were checked before this local read.
            let mut flash = pac::FLASH.cr2().read();
            flash.0 &= !crate::RCC_L083_LSI_FLASH_CR2_KEY_MASK;
            let target = flash_target.ok_or(Error::FlashLatencyTimeout)?;
            if flash.0 != expected.0 && flash.0 != target.0 {
                return Err(Error::FlashLatencyTimeout);
            }
            if !matches!(phase, LsiTransition::FlashWait(_, _))
                && flash.wait() != actual.cr2.flashwait()
            {
                return Err(Error::FlashLatencyTimeout);
            }
            if actual.cr2.0 == system_target.0 && flash.0 == target.0 {
                self.cr2 = system_target;
                self.flash_cr2 = Some(target);
            }
        }
        if actual.pll.0 & crate::RCC_LSI_PLL_PARAMETERS_MASK
            != self.entry.pll.0 & crate::RCC_LSI_PLL_PARAMETERS_MASK
            || actual.pll.stable() != actual.flags.pllstable()
        {
            return Err(Error::PllConfigurationTimeout);
        }
        // SYSCLK departure alone never releases the original reference.
        if self.entry.sources.pllen() && !self.pll_stopped {
            if matches!(
                phase,
                LsiTransition::HsiStopRequest(_)
                    | LsiTransition::HsiStopped
                    | LsiTransition::HsiTrim(_)
                    | LsiTransition::HsiDivider(_)
                    | LsiTransition::LsiTrim(_)
                    | LsiTransition::LsiRequest(_)
            ) {
                return Err(Error::PllStopTimeout);
            }
            match self.entry.pll.source() {
                pac::sysctrl::vals::PllSource::Hsi => {
                    if !actual.sources.hsien()
                        || !actual.hsi.stable()
                        || !actual.flags.hsistable()
                        || actual.hsi.0 != self.entry.hsi.0
                    {
                        return Err(Error::PllConfigurationTimeout);
                    }
                }
                pac::sysctrl::vals::PllSource::HseCrystal
                | pac::sysctrl::vals::PllSource::HseBypass => {
                    if !actual.sources.hseen()
                        || !actual.hse.stable()
                        || !actual.flags.hsestable()
                        || actual.hse.0 != self.entry.hse.0
                    {
                        return Err(Error::PllConfigurationTimeout);
                    }
                }
                _ => return Err(Error::PllConfigurationTimeout),
            }
        }
        if self.pll_stopped {
            if actual.sources.pllen() || actual.pll.stable() {
                return Err(Error::PllStopTimeout);
            }
        } else if matches!(phase, LsiTransition::PllStopping(_)) {
            if !self.pll_ready && actual.pll.stable() {
                return Err(Error::PllStopTimeout);
            }
            self.pll_ready = actual.pll.stable();
            if !actual.sources.pllen() && !actual.pll.stable() {
                self.pll_stopped = true;
            }
        } else if !actual.sources.pllen() || !actual.pll.stable() {
            return Err(Error::PllConfigurationTimeout);
        }
        let enabled = [
            actual.sources.hsien(),
            actual.sources.hseen(),
            actual.sources.lseen(),
            actual.sources.lsien(),
        ];
        let ready = [
            actual.hsi.stable(),
            actual.hse.stable(),
            actual.lse.stable(),
            actual.lsi.stable(),
        ];
        let stopping_hsi = matches!(
            phase,
            LsiTransition::HsiStopRequest(_) | LsiTransition::HsiStopped
        );
        for index in 0..4 {
            if ready[index] && !enabled[index] && !(index == 0 && stopping_hsi) {
                return Err(Error::LsiClockInUse);
            }
        }
        if self.requested
            && !actual.sources.lsien()
            && !matches!(phase, LsiTransition::LsiRequest(_))
        {
            return Err(Error::LsiClockInUse);
        }
        let events = [
            actual.flags.hsirdy(),
            actual.flags.hserdy(),
            actual.flags.lserdy(),
            actual.flags.lsirdy(),
        ];
        let entry_events = [
            self.entry.flags.hsirdy(),
            self.entry.flags.hserdy(),
            self.entry.flags.lserdy(),
            self.entry.flags.lsirdy(),
        ];
        let event_enabled = [
            actual.ier.hsirdy(),
            actual.ier.hserdy(),
            actual.ier.lserdy(),
            actual.ier.lsirdy(),
        ];
        for index in 0..4 {
            if (!self.event_owned[index] && events[index] != entry_events[index])
                || (self.events[index] && !events[index])
            {
                return Err(Error::LsiClockInUse);
            }
        }
        if actual.flags.pllrdy() != self.entry.flags.pllrdy()
            || (actual.flags.0
                & !(crate::RCC_L083_LSI_ISR_READY_MASK | crate::RCC_L083_LSI_ISR_EVENT_MASK))
                != (self.entry.flags.0
                    & !(crate::RCC_L083_LSI_ISR_READY_MASK | crate::RCC_L083_LSI_ISR_EVENT_MASK))
        {
            return Err(Error::LsiClockInUse);
        }
        if !self.requested && self.class != LsiEntryClass::LiveReady {
            // A prior owned HSI request cannot erase a cold SYSCTRL observer.
            if actual.pending != self.entry.pending
                || actual.lsi.stable() != self.entry.lsi.stable()
            {
                return Err(Error::LsiClockInUse);
            }
        } else if (self.pending_seen && !actual.pending)
            || (!self.pending_seen
                && actual.pending
                && !(0..4).any(|index| {
                    self.event_owned[index]
                        && events[index]
                        && !entry_events[index]
                        && event_enabled[index]
                }))
        {
            return Err(Error::LsiClockInUse);
        }
        self.events = events;
        Ok(())
    }

    #[cfg(rcc_cw32l083_v1)]
    fn hsi_divisor(&self) -> Result<u32, Error> {
        match self.entry.hsi.div() {
            5 => Ok(6),
            6 => Ok(1),
            8 => Ok(2),
            9 => Ok(4),
            11 => Ok(8),
            12 => Ok(10),
            13 => Ok(12),
            14 => Ok(14),
            15 => Ok(16),
            _ => Err(Error::InvalidHsiDivider),
        }
    }

    #[cfg(rcc_cw32l083_v1)]
    fn validate_edge(
        config: Config,
        clocks: Clocks,
        source: crate::rcc::ClockBounds,
        clock: pac::sysctrl::regs::Cr0,
        wait: u8,
    ) -> Result<(), Error> {
        let ahb = 1u32 << clock.hclkprs();
        let apb = 1u32 << clock.pclkprs();
        crate::rcc::operating::validate(
            config.operating_conditions,
            Clocks {
                source,
                dividers: [config.hsi.div.divisor(), ahb, ahb * apb],
                ..clocks
            },
        )?;
        if wait > 2
            || source
                .divided_by(ahb)
                .maximum_exceeds((u32::from(wait) + 1) * crate::RCC_FLASH_WAIT_STEP_HZ)
        {
            return Err(Error::FlashLatencyTimeout);
        }
        Ok(())
    }

    #[cfg(rcc_cw32l083_v1)]
    fn fallback_bound(config: Config, clocks: Clocks) -> Result<(), Error> {
        crate::rcc::operating::validate(
            config.operating_conditions,
            Clocks {
                source: crate::rcc::ClockBounds::hsi(1),
                dividers: [config.hsi.div.divisor(), 1, 1],
                ..clocks
            },
        )
    }

    #[cfg(rcc_cw32l083_v1)]
    fn admit_l083(&mut self, config: Config, clocks: Clocks) -> Result<(), Error> {
        use pac::sysctrl::vals::PllSource as Reference;
        let entry = self.entry;
        let pll = entry.pll;
        if pll.reserved_debug().to_bits() != crate::RCC_PLL_RESERVED_DEBUG_DEFAULT {
            return Err(Error::PllReservedConfiguration);
        }
        // Reconstruct only documented fields in a local value. All unnamed
        // RFU bits must retain their own documented reset default of zero;
        // no PLL register write occurs, and STABLE is checked separately.
        let mut defined = pac::sysctrl::regs::Pll::default();
        defined.set_source(pll.source());
        defined.set_freqin(pll.freqin());
        defined.set_mul(pll.mul());
        defined.set_freqout(pll.freqout());
        defined.set_waitcycle(pll.waitcycle());
        defined.set_reserved_debug(pll.reserved_debug());
        if pll.0 & crate::RCC_LSI_PLL_PARAMETERS_MASK != defined.0 {
            return Err(Error::PllReservedConfiguration);
        }
        let multiplier = pll.mul().to_bits();
        if !(crate::RCC_PLL_MULTIPLIER_RANGE.0..=crate::RCC_PLL_MULTIPLIER_RANGE.1)
            .contains(&multiplier)
        {
            return Err(Error::InvalidPllMultiplier);
        }
        if !matches!(
            pll.source(),
            Reference::Hsi | Reference::HseCrystal | Reference::HseBypass
        ) {
            return Err(Error::PllConfigurationTimeout);
        }
        if matches!(pll.source(), Reference::HseCrystal | Reference::HseBypass)
            && entry.hse.mode() != (pll.source() == Reference::HseBypass)
        {
            return Err(Error::PllConfigurationTimeout);
        }
        // Every encoded WAIT/input/output bin is documented. Inactive PLL
        // parameters remain defined without claiming they describe a live rate.
        let pll_bounds = if entry.sources.pllen() {
            let c = config.operating_conditions;
            if c.min_supply_mv < crate::RCC_PLL_SUPPLY_MV.0
                || c.max_supply_mv > crate::RCC_PLL_SUPPLY_MV.1
                || c.min_temperature_c < crate::RCC_PLL_TEMPERATURE_C.0
                || c.max_temperature_c > crate::RCC_PLL_TEMPERATURE_C.1
            {
                return Err(Error::PllConditionsOutsideQualifiedRange);
            }
            let input = match pll.source() {
                Reference::Hsi => {
                    if !entry.sources.hsien()
                        || !entry.hsi.stable()
                        || entry.hsi.trim() != self.factory_hsi
                    {
                        return Err(Error::HsiClockInUse);
                    }
                    crate::rcc::ClockBounds::hsi(self.hsi_divisor()?)
                }
                Reference::HseCrystal | Reference::HseBypass => {
                    let hse = config.hse.ok_or(Error::HseNotConfigured)?;
                    if !entry.sources.hseen()
                        || !entry.hse.stable()
                        || entry.hse.mode() != (pll.source() == Reference::HseBypass)
                        || entry.hse.mode() != (hse.mode == HseMode::Bypass)
                    {
                        return Err(Error::PllConfigurationTimeout);
                    }
                    hse.bounds()?
                }
                _ => return Err(Error::PllConfigurationTimeout),
            };
            if input.minimum_below(crate::RCC_PLL_INPUT_RANGE_HZ.0)
                || input.maximum_exceeds(crate::RCC_PLL_INPUT_RANGE_HZ.1)
            {
                return Err(Error::PllInputOutsideQualifiedRange);
            }
            let input_bin = crate::RCC_PLL_INPUT_BINS_HZ[usize::from(pll.freqin().to_bits())];
            if input.minimum_below(input_bin.0) || input.maximum_exceeds(input_bin.1) {
                return Err(Error::PllInputCrossesBin);
            }
            let output = input
                .multiplied_by(u32::from(multiplier))
                .ok_or(Error::PllArithmeticOverflow)?;
            if output.minimum_below(crate::RCC_PLL_OUTPUT_RANGE_HZ.0)
                || output.maximum_exceeds(crate::RCC_PLL_OUTPUT_RANGE_HZ.1)
            {
                return Err(Error::PllOutputOutsideQualifiedRange);
            }
            // Native 1xx encodings all denote the documented 48..72 MHz bin.
            let output_index = match pll.freqout().to_bits() {
                0..=3 => pll.freqout().to_bits(),
                4..=7 => 4,
                _ => return Err(Error::PllOutputCrossesBin),
            };
            let output_bin = crate::RCC_PLL_OUTPUT_BINS_HZ[usize::from(output_index)];
            if output.minimum_below(output_bin.0) || output.maximum_exceeds(output_bin.1) {
                return Err(Error::PllOutputCrossesBin);
            }
            Some(output)
        } else {
            None
        };
        let source = match entry.clock.sysclk() {
            ClockSource::Hsi if entry.hsi.trim() == self.factory_hsi => {
                Some(crate::rcc::ClockBounds::hsi(self.hsi_divisor()?))
            }
            // Preserve the caller's safe, unchanged non-factory HSI interval;
            // it receives no invented absolute factory-rate bound.
            ClockSource::Hsi => None,
            ClockSource::Hse => Some(config.hse.ok_or(Error::HseNotConfigured)?.bounds()?),
            ClockSource::Lse => Some(
                config
                    .lse
                    .ok_or(Error::LseNotConfigured)?
                    .bounds(config.operating_conditions)?,
            ),
            ClockSource::Lsi => Some(crate::rcc::ClockBounds::lsi()),
            ClockSource::Pll => Some(pll_bounds.ok_or(Error::PllConfigurationTimeout)?),
            _ => return Err(Error::InvalidClockSource),
        };
        if let Some(source) = source {
            Self::validate_edge(config, clocks, source, entry.clock, entry.cr2.flashwait())?;
        }
        let fallback = entry.sources.clkccs()
            && matches!(entry.clock.sysclk(), ClockSource::Hse | ClockSource::Lse);
        if fallback {
            Self::fallback_bound(config, clocks)?;
            if !entry.sources.hsien() || !entry.hsi.stable() || entry.hsi.trim() != self.factory_hsi
            {
                return Err(Error::HsiClockInUse);
            }
            if entry.cr2.flashwait() != 2 {
                return Err(Error::FlashLatencyTimeout);
            }
        }
        if entry.clock.sysclk() == ClockSource::Pll {
            let mut guard = entry.clock;
            guard.set_hclkprs(entry.clock.hclkprs().max(AHBPrescaler::Div4 as u8));
            guard.set_pclkprs(entry.clock.pclkprs().max(APBPrescaler::Div8 as u8));
            self.pll_bridge = if entry.hsi.trim() == self.factory_hsi {
                Self::validate_edge(
                    config,
                    clocks,
                    crate::rcc::ClockBounds::hsi(self.hsi_divisor()?),
                    guard,
                    2,
                )?;
                Some(ClockSource::Hsi)
            } else if matches!(pll.source(), Reference::HseCrystal | Reference::HseBypass) {
                let hse = config.hse.ok_or(Error::HseNotConfigured)?;
                Self::validate_edge(config, clocks, hse.bounds()?, guard, 2)?;
                if entry.sources.clkccs() {
                    Self::fallback_bound(config, clocks)?;
                    // An HSE escape cannot bypass unprovable HSI fallback.
                    return Err(Error::HsiClockInUse);
                }
                Some(ClockSource::Hse)
            } else {
                return Err(Error::HsiClockInUse);
            };
        }
        Ok(())
    }

    #[cfg(rcc_cw32l083_v1)]
    fn stop_l083_pll(
        &mut self,
        config: Config,
        cs: critical_section::CriticalSection<'_>,
    ) -> Result<(), Error> {
        if self.pll_stopped {
            return self.check_all(LsiTransition::Steady, cs);
        }
        if let Some(bridge) = self.pll_bridge {
            if bridge == ClockSource::Hsi && !self.sources.hsien() {
                self.check_all(LsiTransition::Steady, cs)?;
                let mut enabled = self.sources;
                enabled.set_hsien(true);
                self.event_owned[0] = true;
                pac::SYSCTRL.cr1().modify(|w| {
                    w.set_key(0x5a5a);
                    w.set_hsien(true);
                });
                self.wait(
                    LsiTransition::HsiRequest(enabled),
                    config.timeout,
                    Error::ClockConfigurationTimeout,
                    cs,
                )?;
                self.wait(
                    LsiTransition::HsiReady,
                    config.timeout,
                    Error::HsiTimeout,
                    cs,
                )?;
            }
            self.check_all(LsiTransition::Steady, cs)?;
            let mut target = self.clock;
            target.set_sysclk(bridge);
            pac::SYSCTRL.cr0().modify(|w| {
                w.set_key(0x5a5a);
                w.set_sysclk(bridge);
            });
            self.wait(
                LsiTransition::Clock(target),
                config.timeout,
                Error::TemporaryClockSwitchTimeout,
                cs,
            )?;
            barrier();
        }
        // Immutable entry PLLEN makes every collection repeat all bonded and
        // unbonded PLL-output owner checks right up to the stop use edge.
        self.check_all(LsiTransition::Steady, cs)?;
        if self.clock.sysclk() == ClockSource::Pll {
            return Err(Error::PllStopTimeout);
        }
        let mut stopped = self.sources;
        stopped.set_pllen(false);
        pac::SYSCTRL.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_pllen(false);
        });
        self.wait(
            LsiTransition::PllStopping(stopped),
            config.timeout,
            Error::PllStopTimeout,
            cs,
        )?;
        self.check_all(LsiTransition::Steady, cs)
    }

    fn collect(
        &mut self,
        phase: LsiTransition,
        cs: critical_section::CriticalSection<'_>,
    ) -> Result<(RccLsiConsumers, RccLsiPads), Error> {
        self.check_identity(phase)?;
        let consumers = rcc_lsi_consumers(
            self.timeout,
            cs,
            self.class == LsiEntryClass::Cold,
            #[cfg(rcc_cw32l083_v1)]
            self.entry.sources.pllen(),
            self.monitor_hse,
            self.monitor_lse,
        )?;
        let pads = rcc_lsi_pads(self.timeout, cs, self.monitor_hse, self.monitor_lse)?;
        let lse_snapshot = if self.lse_preflight.is_some() {
            let config = self.lse_config.ok_or(Error::LseClockInUse)?;
            Some(rcc_lsi_lse_preflight(
                config.mode == super::LseMode::Bypass,
                !self.lse_pads_configured,
                #[cfg(rcc_cw32l083_v1)]
                self.entry.sources.pllen(),
                config.poll_budget,
                cs,
                self.monitor_hse,
                self.monitor_lse,
            )?)
        } else {
            None
        };
        self.check_identity(phase)?;
        // Each collector already resolved restore/enable/readback before faults
        // and its buffered semantics. Compare only after their full restoration.
        #[cfg(rcc_cw32l083_v1)]
        if let LsiTransition::HsePad(bypass, step) = phase {
            let target_consumers = self
                .consumers
                .map(|saved| saved.hse_configured_step(bypass, step));
            let target_pads = self
                .pads
                .map(|saved| saved.hse_configured_step(bypass, step));
            let target_lse = self
                .lse_preflight
                .map(|saved| saved.hse_configured_step(bypass, step));
            if (Some(consumers) != self.consumers && Some(consumers) != target_consumers)
                || (Some(pads) != self.pads && Some(pads) != target_pads)
                || (lse_snapshot != self.lse_preflight && lse_snapshot != target_lse)
            {
                return Err(Error::LsiClockInUse);
            }
            // These are computed one-register deltas, never arbitrary samples.
            if Some(consumers) == target_consumers
                && Some(pads) == target_pads
                && lse_snapshot == target_lse
            {
                self.consumers = target_consumers;
                self.pads = target_pads;
                self.lse_preflight = target_lse;
            }
            return Ok((consumers, pads));
        }
        if self.consumers.is_some_and(|saved| saved != consumers)
            || self.pads.is_some_and(|saved| saved != pads)
            || lse_snapshot != self.lse_preflight
        {
            return Err(Error::LsiClockInUse);
        }
        Ok((consumers, pads))
    }

    fn check_all(
        &mut self,
        phase: LsiTransition,
        cs: critical_section::CriticalSection<'_>,
    ) -> Result<(), Error> {
        self.collect(phase, cs).map(|_| ())
    }

    fn wait(
        &mut self,
        phase: LsiTransition,
        attempts: u32,
        error: Error,
        cs: critical_section::CriticalSection<'_>,
    ) -> Result<(), Error> {
        for _ in 0..attempts {
            self.check_all(phase, cs)?;
            let acknowledged = match phase {
                LsiTransition::Steady => true,
                #[cfg(rcc_cw32l083_v1)]
                LsiTransition::PllStopping(target) => {
                    self.sources.0 == target.0 && self.pll_stopped
                }
                #[cfg(rcc_cw32l083_v1)]
                LsiTransition::FlashWait(system, flash) => {
                    self.cr2.0 == system.0
                        && self.flash_cr2.is_some_and(|actual| actual.0 == flash.0)
                }
                #[cfg(rcc_cw32l083_v1)]
                LsiTransition::HsePad(bypass, step) => {
                    self.consumers
                        == self
                            .consumers
                            .map(|saved| saved.hse_configured_step(bypass, step))
                        && self.pads
                            == self
                                .pads
                                .map(|saved| saved.hse_configured_step(bypass, step))
                        && self.lse_preflight
                            == self
                                .lse_preflight
                                .map(|saved| saved.hse_configured_step(bypass, step))
                }
                LsiTransition::Clock(target) => self.clock.0 == target.0,
                LsiTransition::LsiTrim(target) => {
                    self.lsi.0 & crate::RCC_LSI_PARAMETERS_MASK
                        == target.0 & crate::RCC_LSI_PARAMETERS_MASK
                }
                LsiTransition::LsiRequest(target)
                | LsiTransition::HsiStopRequest(target)
                | LsiTransition::HsiRequest(target)
                | LsiTransition::HseRequest(target)
                | LsiTransition::LseRequest(target) => self.sources.0 == target.0,
                LsiTransition::LsiReady => self.ready[3],
                LsiTransition::HsiStopped => !self.ready[0],
                LsiTransition::HsiReady => self.ready[0],
                LsiTransition::HseReady => self.ready[1],
                LsiTransition::LseReady => self.ready[2],
                LsiTransition::HsiTrim(target) | LsiTransition::HsiDivider(target) => {
                    self.hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK
                        == target.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK
                }
                LsiTransition::HseParameters(target) => {
                    self.hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK
                        == target.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK
                }
                LsiTransition::LseParameters(target) => {
                    self.lse.0 & crate::RCC_LSI_LSE_PARAMETERS_MASK
                        == target.0 & crate::RCC_LSI_LSE_PARAMETERS_MASK
                }
            };
            if acknowledged {
                return Ok(());
            }
            core::hint::spin_loop();
        }
        Err(error)
    }

    fn hsi_owners(&mut self, cs: critical_section::CriticalSection<'_>) -> Result<(), Error> {
        self.check_all(LsiTransition::Steady, cs)?;
        let consumers = self.consumers.ok_or(Error::LsiClockInUse)?;
        let autotrim = pac::autotrim::regs::Cr(consumers.autotrim_cr);
        let lvd0 = pac::lvd::regs::Cr0(consumers.lvd_cr0);
        let lvd1 = pac::lvd::regs::Cr1(consumers.lvd_cr1);
        if (autotrim.en() && autotrim.src() == pac::autotrim::vals::Source::HsiOsc)
            || (lvd0.en() && lvd1.flten() && lvd1.fltclk())
        {
            return Err(Error::HsiClockInUse);
        }
        Ok(())
    }

    fn external_owners(&self, config: Config) -> Result<(), Error> {
        let consumers = self.consumers.ok_or(Error::LsiClockInUse)?;
        let pads = self.pads.ok_or(Error::LsiClockInUse)?;
        let autotrim = pac::autotrim::regs::Cr(consumers.autotrim_cr);
        let rtc_source = pac::rtc::regs::Cr1(consumers.rtc_cr1).source();
        let rtc_hse = matches!(
            rtc_source,
            pac::rtc::vals::Source::HseDiv128
                | pac::rtc::vals::Source::HseDiv256
                | pac::rtc::vals::Source::HseDiv512
                | pac::rtc::vals::Source::HseDiv1024
        );
        let autotrim_hse = autotrim.en() && autotrim.src() == pac::autotrim::vals::Source::Hse;
        if (rtc_hse || autotrim_hse) && !self.entry.sources.hseen() {
            return Err(Error::HseClockInUse);
        }
        if self.entry.sources.hseen() && !pads.hse_matches(self.entry.hse.mode()) {
            return Err(Error::HseClockInUse);
        }
        if self.entry.sources.lseen() && !pads.lse_matches(self.entry.lse.mode(), false) {
            return Err(Error::LseClockInUse);
        }
        if let Some(hse) = config.hse {
            if self.entry.sources.hseen() {
                let requested = self.hse_parameters(hse)?;
                if requested.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK
                    != self.hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK
                    || !pads.hse_matches(hse.mode == HseMode::Bypass)
                {
                    return Err(Error::HseClockInUse);
                }
            } else {
                if autotrim.en() && autotrim.src() == pac::autotrim::vals::Source::Etr {
                    return Err(Error::HsePinInUse);
                }
                #[cfg(rcc_cw32l083_v1)]
                if !pads.hse_startup_allowed(hse.mode == HseMode::Bypass) {
                    return Err(Error::HsePinInUse);
                }
            }
        }
        Ok(())
    }

    fn hse_parameters(&self, config: Hse) -> Result<pac::sysctrl::regs::Hse, Error> {
        let mut target = self.hse;
        target.set_mode(config.mode == HseMode::Bypass);
        target.set_driver(config.drive);
        #[cfg(rcc_cw32l052_v1)]
        target.set_pdriver(config.drive);
        target.set_freqrange(config.range());
        #[cfg(rcc_cw32l052_v1)]
        target.set_pfreqrange(config.range());
        target.set_waitcycle(pac::sysctrl::vals::HseWait::Cycles262144);
        target.set_flt(false);
        target.set_detcnt(config.detector_count()?);
        Ok(target)
    }

    fn lse_parameters(&self, config: super::Lse) -> pac::sysctrl::regs::Lse {
        let mut target = self.lse;
        target.set_mode(config.mode == super::LseMode::Bypass);
        target.set_driver(config.drive);
        target.set_amp(config.amplitude);
        #[cfg(rcc_cw32l052_v1)]
        {
            target.set_pdriver(config.startup_drive);
            target.set_pamp(config.startup_amplitude);
        }
        target.set_waitcycle(config.wait);
        target
    }

    fn cold_lse_preflight(
        &mut self,
        config: super::Lse,
        unused: bool,
        cs: critical_section::CriticalSection<'_>,
    ) -> Result<(), Error> {
        self.check_all(LsiTransition::Steady, cs)?;
        if self.entry.sources.lseen()
            || self.sources.lseen()
            || self.entry.sources.lselock()
            || self.ready[2]
            || self.entry.flags.lserdy()
            || self.entry.ier.lserdy()
            || self.entry.ier.lsefail()
            || self.entry.ier.lsefault()
            || self.clock.sysclk() == ClockSource::Lse
        {
            return Err(Error::LseClockInUse);
        }
        #[cfg(rcc_cw32l083_v1)]
        {
            let observed = self.check_identity(LsiTransition::Steady)?;
            if observed.pending || observed.fault_pending {
                return Err(Error::LseClockInUse);
            }
        }
        let snapshot = rcc_lsi_lse_preflight(
            config.mode == super::LseMode::Bypass,
            unused,
            #[cfg(rcc_cw32l083_v1)]
            self.entry.sources.pllen(),
            config.poll_budget,
            cs,
            self.monitor_hse,
            self.monitor_lse,
        )?;
        self.check_all(LsiTransition::Steady, cs)?;
        if self
            .lse_preflight
            .is_some_and(|expected| expected != snapshot)
        {
            return Err(Error::LseClockInUse);
        }
        if self.lse_preflight.is_none() {
            self.lse_preflight = Some(snapshot);
        }
        Ok(())
    }

    fn verify_sources(
        &mut self,
        config: Config,
        cs: critical_section::CriticalSection<'_>,
    ) -> Result<(), Error> {
        self.check_all(LsiTransition::Steady, cs)?;
        let pads = self.pads.ok_or(Error::LsiClockInUse)?;
        #[cfg(rcc_cw32l083_v1)]
        if !self.pll_stopped || self.sources.pllen() || self.pll_ready {
            return Err(Error::PllStopTimeout);
        }
        if !self.sources.lsien() || !self.ready[3] || self.lsi.trim() != self.factory_lsi {
            return Err(Error::LsiTimeout);
        }
        if !self.sources.hsien()
            || !self.ready[0]
            || self.hsi.trim() != self.factory_hsi
            || self.hsi.div() != config.hsi.div as u8
        {
            return Err(Error::ClockConfigurationTimeout);
        }
        if let Some(hse) = config.hse {
            if !self.sources.hseen()
                || !self.ready[1]
                || self.hse_parameters(hse)?.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK
                    != self.hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK
                || !pads.hse_matches(hse.mode == HseMode::Bypass)
            {
                return Err(Error::HseClockInUse);
            }
        } else if self.entry.sources.hseen() && !pads.hse_matches(self.entry.hse.mode()) {
            return Err(Error::HseClockInUse);
        }
        if let Some(lse) = config.lse {
            if !self.sources.lseen()
                || !self.sources.lseccs()
                || !self.ready[2]
                || self.lse_parameters(lse).0 & crate::RCC_LSI_LSE_PARAMETERS_MASK
                    != self.lse.0 & crate::RCC_LSI_LSE_PARAMETERS_MASK
                || !pads.lse_matches(lse.mode == super::LseMode::Bypass, false)
                || !super::lse::healthy(lse)
            {
                check_external_faults(self.monitor_hse, self.monitor_lse)?;
                return Err(Error::LseNotReady);
            }
        } else if self.entry.sources.lseen() && !pads.lse_matches(self.entry.lse.mode(), false) {
            return Err(Error::LseClockInUse);
        }
        self.check_all(LsiTransition::Steady, cs)
    }

    #[cfg(rcc_cw32l052_v1)]
    fn flash_latency(
        &mut self,
        wait: u32,
        cs: critical_section::CriticalSection<'_>,
    ) -> Result<(), Error> {
        self.check_all(LsiTransition::Steady, cs)?;
        pac::FLASH.cr2().modify(|w| {
            w.set_key(0x5a5a);
            w.set_wait(wait as u8);
        });
        for _ in 0..self.timeout {
            self.check_all(LsiTransition::Steady, cs)?;
            if pac::FLASH.cr2().read().wait() == wait as u8 {
                barrier();
                return self.check_all(LsiTransition::Steady, cs);
            }
            core::hint::spin_loop();
        }
        Err(Error::FlashLatencyTimeout)
    }
    #[cfg(rcc_cw32l083_v1)]
    fn flash_latency(
        &mut self,
        wait: u32,
        cs: critical_section::CriticalSection<'_>,
    ) -> Result<(), Error> {
        self.check_all(LsiTransition::Steady, cs)?;
        let mut system = self.cr2;
        system.set_flashwait(wait as u8);
        let mut target = self.flash_cr2.ok_or(Error::FlashLatencyTimeout)?;
        target.set_wait(wait as u8);
        pac::FLASH.cr2().modify(|w| {
            w.set_key(0x5a5a);
            w.set_wait(wait as u8);
        });
        self.wait(
            LsiTransition::FlashWait(system, target),
            self.timeout,
            Error::FlashLatencyTimeout,
            cs,
        )?;
        barrier();
        self.check_all(LsiTransition::Steady, cs)
    }
}

#[cfg(rcc_lsi_sysclk)]
fn configure_lsi(
    config: Config,
    cs: critical_section::CriticalSection<'_>,
) -> Result<Clocks, Error> {
    // P0/P1: pure admission, immutable capture, then two complete equal passes.
    let mut clocks = config.frequencies()?;
    let mut state = LsiSysclkState::capture(config)?;
    let r = pac::SYSCTRL;
    let (consumers, pads) = state.collect(LsiTransition::Steady, cs)?;
    state.consumers = Some(consumers);
    state.pads = Some(pads);
    state.check_all(LsiTransition::Steady, cs)?;
    state.external_owners(config)?;
    #[cfg(rcc_cw32l083_v1)]
    state.admit_l083(config, clocks)?;
    let needs_hsi_trim = state.entry.hsi.trim() != state.factory_hsi;
    if needs_hsi_trim {
        state.hsi_owners(cs)?;
    }
    if let Some(lse) = config.lse {
        if state.entry.sources.lseen() {
            let target = state.lse_parameters(lse);
            if !state.entry.sources.lseccs()
                || target.0 & crate::RCC_LSI_LSE_PARAMETERS_MASK
                    != state.lse.0 & crate::RCC_LSI_LSE_PARAMETERS_MASK
                || !pads.lse_matches(lse.mode == super::LseMode::Bypass, false)
            {
                return Err(Error::LseClockInUse);
            }
            state.check_all(LsiTransition::Steady, cs)?;
        } else {
            state.cold_lse_preflight(lse, true, cs)?;
        }
    }

    // P2: central Flash gate, WAIT2 and monotonic divider-only guard. FLASH is
    // not in the twelve saved consumer-gate slots; GPIO ownership is unchanged.
    state.check_all(LsiTransition::Steady, cs)?;
    let flash = <crate::peripherals::FLASH as crate::rcc::SealedRccPeripheral>::RCC_INFO;
    flash
        .enable_with_cs_readback(
            cs,
            crate::rcc::Readback::Poll {
                attempts: config.timeout,
                spin: true,
            },
        )
        .map_err(|_| Error::FlashClockTimeout)?;
    #[cfg(rcc_cw32l083_v1)]
    {
        state.flash_gate = true;
    }
    state.check_all(LsiTransition::Steady, cs)?;
    if !flash.is_enabled() || flash.reset_asserted() {
        return Err(Error::FlashClockTimeout);
    }
    #[cfg(rcc_cw32l083_v1)]
    {
        let mut local = pac::FLASH.cr2().read();
        local.0 &= !crate::RCC_L083_LSI_FLASH_CR2_KEY_MASK;
        if local.wait() != state.entry.cr2.flashwait() {
            return Err(Error::FlashLatencyTimeout);
        }
        state.flash_cr2 = Some(local);
        state.check_all(LsiTransition::Steady, cs)?;
    }
    state.flash_latency(crate::RCC_INITIAL_FLASH_WAIT, cs)?;
    let mut guarded = state.clock;
    guarded.set_hclkprs(state.entry.clock.hclkprs().max(AHBPrescaler::Div4 as u8));
    guarded.set_pclkprs(state.entry.clock.pclkprs().max(APBPrescaler::Div8 as u8));
    #[cfg(rcc_cw32l083_v1)]
    if state.entry.sources.clkccs()
        && matches!(
            state.entry.clock.sysclk(),
            ClockSource::Hse | ClockSource::Lse
        )
    {
        // Admission proved unchanged factory HSI at the entry divisors and
        // raw fallback envelope. Never replay the external selector.
        guarded.set_sysclk(ClockSource::Hsi);
    }
    state.check_all(LsiTransition::Steady, cs)?;
    r.cr0().modify(|w| {
        w.set_key(0x5a5a);
        #[cfg(rcc_cw32l083_v1)]
        w.set_sysclk(guarded.sysclk());
        w.set_hclkprs(guarded.hclkprs());
        w.set_pclkprs(guarded.pclkprs());
    });
    state.wait(
        LsiTransition::Clock(guarded),
        config.timeout,
        Error::ClockConfigurationTimeout,
        cs,
    )?;
    barrier();

    #[cfg(rcc_cw32l083_v1)]
    state.stop_l083_pll(config, cs)?;

    // P3: even matching cold TRIM takes the final stopped use-edge proof.
    if state.class == LsiEntryClass::Cold {
        state.check_all(LsiTransition::Steady, cs)?;
        let mut target = state.lsi;
        target.set_trim(state.factory_lsi);
        if state.lsi.trim() != state.factory_lsi {
            r.lsi().modify(|w| w.set_trim(state.factory_lsi));
            state.wait(
                LsiTransition::LsiTrim(target),
                config.timeout,
                Error::LsiConfigurationTimeout,
                cs,
            )?;
        }
    }
    // Distinct request edge after optional TRIM acknowledgment. No observer,
    // consumer or parameter is reclassified from a later hardware sample.
    state.check_all(LsiTransition::Steady, cs)?;

    // P4: the first owned source request changes LSIEN alone and is permanent.
    let mut requested = state.sources;
    requested.set_lsien(true);
    state.requested = true;
    #[cfg(rcc_cw32l083_v1)]
    {
        state.event_owned[3] = true;
    }
    r.cr1().modify(|w| {
        w.set_key(0x5a5a);
        w.set_lsien(true);
    });
    state.wait(
        LsiTransition::LsiRequest(requested),
        config.timeout,
        Error::LsiConfigurationTimeout,
        cs,
    )?;
    state.wait(
        LsiTransition::LsiReady,
        config.timeout,
        Error::LsiTimeout,
        cs,
    )?;
    state.check_all(LsiTransition::Steady, cs)?;

    // P5: directly select established factory LSI before stopping a mismatching
    // HSI. No inherited, unqualified HSI trim is enabled or selected as a bridge.
    if needs_hsi_trim || cfg!(rcc_cw32l083_v1) {
        if needs_hsi_trim {
            state.hsi_owners(cs)?;
        }
        let mut target_clock = state.clock;
        target_clock.set_sysclk(ClockSource::Lsi);
        r.cr0().modify(|w| {
            w.set_key(0x5a5a);
            w.set_sysclk(ClockSource::Lsi);
        });
        state.wait(
            LsiTransition::Clock(target_clock),
            config.timeout,
            Error::TemporaryClockSwitchTimeout,
            cs,
        )?;
        barrier();
    }
    if needs_hsi_trim {
        // Repeat frozen HSI-owner equality at the actual stop use edge.
        state.hsi_owners(cs)?;
        if state.sources.hsien() || cfg!(rcc_cw32l052_v1) {
            let mut stopped = state.sources;
            stopped.set_hsien(false);
            r.cr1().modify(|w| {
                w.set_key(0x5a5a);
                w.set_hsien(false);
            });
            state.wait(
                LsiTransition::HsiStopRequest(stopped),
                config.timeout,
                Error::HsiStopTimeout,
                cs,
            )?;
            // HSIEN can acknowledge before either STABLE latch falls. This is its
            // own bounded stop phase, with LSI and every non-owned bit still fixed.
            state.wait(
                LsiTransition::HsiStopped,
                config.timeout,
                Error::HsiStopTimeout,
                cs,
            )?;
        }
        state.check_all(LsiTransition::Steady, cs)?;
        let mut calibrated = state.hsi;
        calibrated.set_trim(state.factory_hsi);
        r.hsi().modify(|w| w.set_trim(state.factory_hsi));
        state.wait(
            LsiTransition::HsiTrim(calibrated),
            config.timeout,
            Error::ClockConfigurationTimeout,
            cs,
        )?;
    } else {
        // Matching trim skips only stop/trim/restart; owner configuration must
        // still equal the immutable complete snapshot.
        state.check_all(LsiTransition::Steady, cs)?;
    }
    if !state.sources.hsien() {
        let mut enabled = state.sources;
        enabled.set_hsien(true);
        state.check_all(LsiTransition::Steady, cs)?;
        #[cfg(rcc_cw32l083_v1)]
        {
            state.event_owned[0] = true;
        }
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_hsien(true);
        });
        state.wait(
            LsiTransition::HsiRequest(enabled),
            config.timeout,
            Error::ClockConfigurationTimeout,
            cs,
        )?;
        state.wait(
            LsiTransition::HsiReady,
            config.timeout,
            Error::HsiTimeout,
            cs,
        )?;
    }
    state.check_all(LsiTransition::Steady, cs)?;
    #[cfg(rcc_cw32l052_v1)]
    {
        let mut hsi_clock = state.clock;
        hsi_clock.set_sysclk(ClockSource::Hsi);
        r.cr0().modify(|w| {
            w.set_key(0x5a5a);
            w.set_sysclk(ClockSource::Hsi);
        });
        state.wait(
            LsiTransition::Clock(hsi_clock),
            config.timeout,
            Error::ClockSwitchTimeout,
            cs,
        )?;
        barrier();
    }
    let mut divided = state.hsi;
    divided.set_div(config.hsi.div as u8);
    state.check_all(LsiTransition::Steady, cs)?;
    r.hsi().modify(|w| w.set_div(config.hsi.div as u8));
    state.wait(
        LsiTransition::HsiDivider(divided),
        config.timeout,
        Error::ClockConfigurationTimeout,
        cs,
    )?;

    #[cfg(rcc_cw32l083_v1)]
    {
        // Native L083 changes DIV while still on qualified LSI, with PLL
        // permanently stopped, then proves the actual factory-HSI use edge.
        state.check_all(LsiTransition::Steady, cs)?;
        LsiSysclkState::validate_edge(
            config,
            clocks,
            crate::rcc::ClockBounds::hsi(config.hsi.div.divisor()),
            state.clock,
            2,
        )?;
        let mut hsi_clock = state.clock;
        hsi_clock.set_sysclk(ClockSource::Hsi);
        r.cr0().modify(|w| {
            w.set_key(0x5a5a);
            w.set_sysclk(ClockSource::Hsi);
        });
        state.wait(
            LsiTransition::Clock(hsi_clock),
            config.timeout,
            Error::ClockSwitchTimeout,
            cs,
        )?;
        barrier();
    }

    // P6: an enabled inherited HSE is preserved, never stopped for reuse.
    if let Some(hse) = config.hse {
        state.check_all(LsiTransition::Steady, cs)?;
        state.external_owners(config)?;
        if !state.entry.sources.hseen() {
            let bypass = hse.mode == HseMode::Bypass;
            let old_pads = state.pads.ok_or(Error::LsiClockInUse)?;
            let old_consumers = state.consumers.ok_or(Error::LsiClockInUse)?;
            // The exact projected GPIOF gate delta is verified before any pad
            // programming. No fresh peripheral sample supplies expectations.
            rcc_lsi_enable_hse_pins(config.timeout, cs, state.monitor_hse, state.monitor_lse)?;
            state.pads = Some(old_pads.hse_gate_enabled());
            state.consumers = Some(old_consumers.hse_gate_enabled());
            state.lse_preflight = state
                .lse_preflight
                .map(|snapshot| snapshot.hse_gate_enabled());
            state.check_all(LsiTransition::Steady, cs)?;
            #[cfg(rcc_cw32l052_v1)]
            {
                rcc_lsi_configure_hse_pins(
                    bypass,
                    config.timeout,
                    cs,
                    state.monitor_hse,
                    state.monitor_lse,
                )?;
                state.pads = state.pads.map(|saved| saved.hse_configured(bypass));
                state.consumers = state.consumers.map(|saved| saved.hse_configured(bypass));
                state.lse_preflight = state
                    .lse_preflight
                    .map(|saved| saved.hse_configured(bypass));
                state.check_all(LsiTransition::Steady, cs)?;
            }
            #[cfg(rcc_cw32l083_v1)]
            for step in 0..crate::RCC_L083_LSI_HSE_PIN_STEPS * if bypass { 1 } else { 2 } {
                state.check_all(LsiTransition::Steady, cs)?;
                crate::rcc_l083_lsi_configure_hse_pin_step(
                    bypass,
                    step,
                    config.timeout,
                    cs,
                    state.monitor_hse,
                    state.monitor_lse,
                )?;
                state.wait(
                    LsiTransition::HsePad(bypass, step),
                    config.timeout,
                    Error::HsePinConfigurationTimeout,
                    cs,
                )?;
            }
            let target = state.hse_parameters(hse)?;
            r.hse().modify(|w| {
                w.set_mode(target.mode());
                w.set_driver(target.driver());
                #[cfg(rcc_cw32l052_v1)]
                w.set_pdriver(target.pdriver());
                w.set_freqrange(target.freqrange());
                #[cfg(rcc_cw32l052_v1)]
                w.set_pfreqrange(target.pfreqrange());
                w.set_waitcycle(target.waitcycle());
                w.set_flt(target.flt());
                w.set_detcnt(target.detcnt());
            });
            state.wait(
                LsiTransition::HseParameters(target),
                config.timeout,
                Error::ClockConfigurationTimeout,
                cs,
            )?;
            state.check_all(LsiTransition::Steady, cs)?;
            let mut enabled = state.sources;
            enabled.set_hseen(true);
            #[cfg(rcc_cw32l083_v1)]
            {
                enabled.set_hseccs(true);
                state.event_owned[1] = true;
            }
            r.cr1().modify(|w| {
                w.set_key(0x5a5a);
                w.set_hseen(true);
                #[cfg(rcc_cw32l083_v1)]
                w.set_hseccs(true);
            });
            state.wait(
                LsiTransition::HseRequest(enabled),
                config.timeout,
                Error::ClockConfigurationTimeout,
                cs,
            )?;
            state.wait(
                LsiTransition::HseReady,
                config.timeout,
                Error::HseTimeout,
                cs,
            )?;
        }
    }

    // P7: optional LSE completes while guarded factory HSI executes. Its strict
    // generated collector retains the old reset-like admission without the old
    // Boolean helpers' loss of restoration/fault error priority.
    if let Some(lse) = config.lse {
        if state.entry.sources.lseen() {
            state.check_all(LsiTransition::Steady, cs)?;
            super::lse::freeze_sysclk_monitor(cs)?;
            state.check_all(LsiTransition::Steady, cs)?;
        } else {
            state.cold_lse_preflight(lse, true, cs)?;
            let bypass = lse.mode == super::LseMode::Bypass;
            rcc_lsi_configure_lse_pins(
                bypass,
                lse.poll_budget,
                cs,
                state.monitor_hse,
                state.monitor_lse,
            )?;
            state.pads = state.pads.map(|saved| saved.lse_configured(bypass));
            #[cfg(rcc_cw32l083_v1)]
            {
                state.consumers = state.consumers.map(|saved| saved.lse_configured(bypass));
            }
            state.lse_preflight = state
                .lse_preflight
                .map(|saved| saved.lse_configured(bypass));
            state.lse_pads_configured = true;
            state.check_all(LsiTransition::Steady, cs)?;
            let target = state.lse_parameters(lse);
            r.lse().modify(|w| {
                w.set_mode(target.mode());
                w.set_driver(target.driver());
                w.set_amp(target.amp());
                #[cfg(rcc_cw32l052_v1)]
                w.set_pdriver(target.pdriver());
                #[cfg(rcc_cw32l052_v1)]
                w.set_pamp(target.pamp());
                w.set_waitcycle(target.waitcycle());
            });
            state.wait(
                LsiTransition::LseParameters(target),
                lse.poll_budget,
                Error::LseNotReady,
                cs,
            )?;
            // A distinct pre-enable proof includes the known bypass pad delta
            // and all original consumer/work-gate/reset/observer identities.
            state.cold_lse_preflight(lse, false, cs)?;
            super::lse::freeze_sysclk_monitor(cs)?;
            state.check_all(LsiTransition::Steady, cs)?;
            let mut enabled = state.sources;
            enabled.set_lseen(true);
            enabled.set_lseccs(true);
            state.lse_requested = true;
            #[cfg(rcc_cw32l083_v1)]
            {
                state.event_owned[2] = true;
            }
            r.cr1().modify(|w| {
                w.set_key(0x5a5a);
                w.set_lseen(true);
                w.set_lseccs(true);
            });
            state.wait(
                LsiTransition::LseRequest(enabled),
                lse.poll_budget,
                Error::LseNotReady,
                cs,
            )?;
            state.wait(
                LsiTransition::LseReady,
                lse.poll_budget,
                Error::LseNotReady,
                cs,
            )?;
        }
    }

    // P8: install final buses while executing the independently qualified HSI
    // tree. All requested source, detector and pad operations are now complete.
    state.verify_sources(config, cs)?;
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
    state.wait(
        LsiTransition::Clock(final_hsi),
        config.timeout,
        Error::ClockConfigurationTimeout,
        cs,
    )?;
    barrier();
    state.verify_sources(config, cs)?;

    // P9: latency is finalized before the final source-only CR0 commit. Fixed
    // HSI /6 escape gets no post-fault AHB divisor credit.
    #[cfg(rcc_cw32l052_v1)]
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
                .maximum()
                .0,
        );
    #[cfg(rcc_cw32l052_v1)]
    let final_wait = (upper_hclk - 1) / crate::RCC_FLASH_WAIT_STEP_HZ;
    #[cfg(rcc_cw32l052_v1)]
    state.flash_latency(final_wait, cs)?;
    #[cfg(rcc_cw32l083_v1)]
    let final_wait = crate::RCC_INITIAL_FLASH_WAIT;
    state.verify_sources(config, cs)?;
    let mut final_lsi = state.clock;
    final_lsi.set_sysclk(ClockSource::Lsi);
    r.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_sysclk(ClockSource::Lsi);
    });
    state.wait(
        LsiTransition::Clock(final_lsi),
        config.timeout,
        Error::ClockSwitchTimeout,
        cs,
    )?;
    barrier();
    // No subsequent source/parameter/divider/Flash/policy write is permitted.
    // Temporary inspection gates retain their precise restoration contract.
    state.verify_sources(config, cs)?;
    if !flash.is_enabled() || flash.reset_asserted() {
        return Err(Error::FlashClockTimeout);
    }
    if pac::FLASH.cr2().read().wait() != final_wait as u8 {
        return Err(Error::FlashLatencyTimeout);
    }
    state.verify_sources(config, cs)?;
    // Preserve inherited HSE pad ownership from the immutable entry mode.
    if state.entry.sources.hseen() {
        if !state.entry.hse.mode() {
            clocks.hse = Some(HseMode::Oscillator);
        } else if clocks.hse.is_none() {
            clocks.hse = Some(HseMode::Bypass);
        }
    }
    // This normalized CR0 read is deliberately the very last hardware
    // observation. Publication happens only in the existing successful caller.
    let last_clock = r.cr0().read().0 & !LSI_CR0_KEY_MASK;
    if last_clock != final_lsi.0 {
        return Err(Error::ClockSwitchTimeout);
    }
    Ok(clocks)
}

// STABLE is a startup latch, not a continuous source-valid indication. Sticky
// faults are observed without writing ICR or disturbing unrelated events.
// Rejecting stale relevant flags is a deliberate first-batch restriction.
#[cfg(rcc_hse)]
fn check_external_faults(hse: bool, lse: bool) -> Result<(), Error> {
    let v = pac::SYSCTRL.isr().read();
    if (hse && (v.hsefail() || v.hsefault())) || (lse && (v.lsefail() || v.lsefault())) {
        Err(Error::ExternalClockFault)
    } else {
        Ok(())
    }
}

#[cfg(rcc_hse)]
fn hse_parameters_match(hse: Hse) -> Result<bool, Error> {
    let v = pac::SYSCTRL.hse().read();
    let range = v.freqrange();
    #[cfg(rcc_cw32l052_v1)]
    if v.pdriver() != hse.drive || v.pfreqrange() != hse.range() {
        return Ok(false);
    }
    Ok(v.mode() == (hse.mode == HseMode::Bypass)
        && v.driver() == hse.drive
        && range == hse.range()
        && v.waitcycle() == pac::sysctrl::vals::HseWait::Cycles262144
        && !v.flt()
        && v.detcnt() == hse.detector_count()?)
}

#[cfg(rcc_hse)]
fn verify_hse_source(
    hse: Hse,
    timeout: u32,
    cs: critical_section::CriticalSection<'_>,
) -> Result<(), Error> {
    if !pac::SYSCTRL.cr1().read().hseen() || !hse_parameters_match(hse)? {
        return Err(Error::ClockConfigurationTimeout);
    }
    if !pac::SYSCTRL.hse().read().stable() {
        return Err(Error::HseTimeout);
    }
    if !crate::rcc_hse_pins_match(hse.mode == HseMode::Bypass, timeout, cs)? {
        return Err(Error::HsePinConfigurationTimeout);
    }
    check_external_faults(true, false)
}

#[cfg(rcc_pll)]
fn verify_pll_reference(
    pll: Pll,
    config: &Config,
    trim: u16,
    cs: critical_section::CriticalSection<'_>,
) -> Result<(), Error> {
    match pll.src {
        PllSource::HSI => {
            let hsi = pac::SYSCTRL.hsi().read();
            if !pac::SYSCTRL.cr1().read().hsien()
                || hsi.div() != config.hsi.div as u8
                || hsi.trim() != trim
            {
                return Err(Error::ClockConfigurationTimeout);
            }
            if !hsi.stable() {
                return Err(Error::HsiTimeout);
            }
            Ok(())
        }
        PllSource::HSE => verify_hse_source(
            config.hse.ok_or(Error::HseNotConfigured)?,
            config.timeout,
            cs,
        ),
    }
}

#[cfg(rcc_pll)]
fn pll_parameters_match(pll: Pll, parameters: &PllParameters) -> bool {
    let v = pac::SYSCTRL.pll().read();
    v.source() == parameters.source
        && v.freqin() == parameters.input_range
        && v.mul() == pll.mul
        && v.freqout() == parameters.output_range
        && v.waitcycle().to_bits() == crate::RCC_PLL_STARTUP_ENCODING
        && v.reserved_debug().to_bits() == crate::RCC_PLL_RESERVED_DEBUG_DEFAULT
}

// Only a requested LSE source needs this monitored-source policy. Default None
// retains the original RCC path. Factory-valid live LSI is never retuned.
#[cfg(rcc_lse)]
fn prepare_lse_monitor(
    timeout: u32,
    cs: critical_section::CriticalSection<'_>,
) -> Result<(), Error> {
    use crate::rtc::sealed::Instance;
    // AUTOTRIM calibration can consume/mutate LSI independently of SRC. Its
    // configuration-only gate may be inspected without resuming gated work.
    if !crate::rcc_lse_monitor_can_freeze(timeout, cs)? {
        return Err(Error::LseClockInUse);
    }
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

#[cfg(all(rcc_lse, rcc_cw32l052_v1))]
pub(super) fn lse_sysclk_monitor_ready() -> bool {
    lse_sysclk::monitor_ready()
}

#[cfg(all(rcc_lse, rcc_cw32l083_v1))]
pub(super) fn lse_sysclk_monitor_ready() -> bool {
    l083_lse_sysclk::monitor_ready()
}
