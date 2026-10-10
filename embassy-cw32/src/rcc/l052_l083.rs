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

use crate::{pac, time::Hertz};
use core::cell::Cell;
use critical_section::Mutex;

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
    /// Init-only board-qualified LSE, with retained factory LSI monitoring.
    #[cfg(all(rcc_lse, rcc_cw32l052_v1))]
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
                #[cfg(all(rcc_lse, rcc_cw32l052_v1))]
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
    #[cfg(all(rcc_lse, rcc_cw32l052_v1))]
    LseNotConfigured,
    #[cfg(all(rcc_lse, rcc_cw32l052_v1))]
    InvalidLseDetector,
    #[cfg(all(rcc_lse, rcc_cw32l052_v1))]
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
    #[cfg(all(rcc_lse, rcc_cw32l052_v1))]
    if config.sys == Sysclk::LSE {
        return lse_sysclk::configure(config, cs);
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
        // The explicit target branch above bypasses this old auxiliary path.
        #[cfg(all(rcc_lse, rcc_cw32l052_v1))]
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
