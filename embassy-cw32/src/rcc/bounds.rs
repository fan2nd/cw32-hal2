//! Source-qualified frequency envelopes. See docs/adc-hsi-clock-bounds.md.
//!
//! Factory-HSI bounds require the trim loaded by RCC, unchanged clock
//! registers, the device's specified supply range, and an ambient temperature
//! inside HSI_BOUND_TEMPERATURE_C. They are not measured clocks. In particular,
//! extended 85..105 C operation on some L families has no qualified HSI bound.
//! HSE envelopes instead use the explicitly declared board-qualified source
//! endpoints and conditions. Neither source accuracy is inferred from the other.
//! F020/F030/A030, CW32F002F3P7/F3U7, CW32F003F4P7/F4U7/E4P7 and exactly
//! CW32L031C8T6/C8U6/F8U6, CW32R031C8U6, CW32W031R8U6 and
//! CW32L052C8T6/R8S6/R8T6 have factory-LSI rate envelopes under their own source
//! conditions, without strict cycle-duration bounds. Their RTC LSI aliases,
//! where present, remain rate-only under every SYSCLK. Generic
//! F002/F003/L031/R031/W031/L052 aliases, all L083 and other unlisted packages
//! gain no factory-LSI SYSCLK qualification.
use crate::time::Hertz;

/// Qualified ambient-temperature interval for the factory HSI error bound.
pub const HSI_BOUND_TEMPERATURE_C: (i16, i16) = crate::RCC_HSI_TEMPERATURE_RANGE_C;
/// Qualified supply interval for the factory HSI error bound, in millivolts.
pub const HSI_BOUND_SUPPLY_MV: (u16, u16) = crate::RCC_HSI_SUPPLY_RANGE_MV;
const HSI_ERROR_PERCENT: u32 = crate::RCC_HSI_ERROR_PERCENT;

// The RTC and SYSCLK capabilities describe the same factory-qualified LSI.
// Keep their independently generated source facts identical on these families.
#[cfg(all(rtc, rcc_lsi_sysclk))]
const _: () = {
    use crate::{peripherals::RTC, rtc::sealed::Instance};
    assert!(RTC::SOURCE_NOMINAL_HZ == crate::RCC_LSI_NOMINAL_HZ);
    assert!(RTC::SOURCE_MINIMUM_HZ == crate::RCC_LSI_MINIMUM_HZ);
    assert!(RTC::SOURCE_MAXIMUM_HZ == crate::RCC_LSI_MAXIMUM_HZ);
    assert!(RTC::TEMPERATURE_C.0 == crate::RCC_LSI_TEMPERATURE_C.0);
    assert!(RTC::TEMPERATURE_C.1 == crate::RCC_LSI_TEMPERATURE_C.1);
    assert!(RTC::SUPPLY_MV.0 == crate::RCC_LSI_SUPPLY_MV.0);
    assert!(RTC::SUPPLY_MV.1 == crate::RCC_LSI_SUPPLY_MV.1);
};

/// Exact source envelope propagated through integer hardware divisors.
///
/// Public whole-hertz getters round outward. Some sources qualify rate only;
/// strict cycle-duration helpers require `has_cycle_timing_bounds()`.
/// Electrical comparisons and time
/// bounds retain the exact fraction, including HSI /7, /14 and bus dividers.
/// Valid only under the originating source capability and its declared conditions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ClockBounds {
    nominal_source: u32,
    minimum_source: u32,
    maximum_source: u32,
    divisor: u32,
    #[cfg(any(rcc_pll, rcc_lsi_sysclk))]
    rate_only: bool,
    temperature_c: (i16, i16),
    supply_mv: (u16, u16),
}
impl ClockBounds {
    pub(crate) const fn hsi(divisor: u32) -> Self {
        let nominal = super::HSI_FREQ.0;
        Self {
            nominal_source: nominal,
            minimum_source: nominal / 100 * (100 - HSI_ERROR_PERCENT),
            maximum_source: nominal / 100 * (100 + HSI_ERROR_PERCENT),
            divisor,
            #[cfg(any(rcc_pll, rcc_lsi_sysclk))]
            rate_only: false,
            temperature_c: HSI_BOUND_TEMPERATURE_C,
            supply_mv: HSI_BOUND_SUPPLY_MV,
        }
    }
    /// Factory-LSI rate envelope for the specifically qualified SYSCLK families.
    #[cfg(rcc_lsi_sysclk)]
    pub(crate) const fn lsi() -> Self {
        Self {
            nominal_source: crate::RCC_LSI_NOMINAL_HZ,
            minimum_source: crate::RCC_LSI_MINIMUM_HZ,
            maximum_source: crate::RCC_LSI_MAXIMUM_HZ,
            divisor: 1,
            rate_only: true,
            temperature_c: crate::RCC_LSI_TEMPERATURE_C,
            supply_mv: crate::RCC_LSI_SUPPLY_MV,
        }
    }
    #[cfg(rcc_external_clock)]
    pub(crate) fn external(
        nominal: Hertz,
        minimum: Hertz,
        maximum: Hertz,
        conditions: super::OperatingConditions,
    ) -> Option<Self> {
        if minimum.0 == 0
            || minimum.0 > nominal.0
            || nominal.0 > maximum.0
            || conditions.min_supply_mv > conditions.max_supply_mv
            || conditions.min_temperature_c > conditions.max_temperature_c
        {
            return None;
        }
        Some(Self {
            nominal_source: nominal.0,
            minimum_source: minimum.0,
            maximum_source: maximum.0,
            divisor: 1,
            #[cfg(any(rcc_pll, rcc_lsi_sysclk))]
            rate_only: false,
            temperature_c: (conditions.min_temperature_c, conditions.max_temperature_c),
            supply_mv: (conditions.min_supply_mv, conditions.max_supply_mv),
        })
    }
    /// Multiply the original exact source numerators before any rounding.
    #[cfg(rcc_pll)]
    pub(crate) fn multiplied_by(self, multiplier: u32) -> Option<Self> {
        if multiplier == 0 {
            return None;
        }
        let multiply =
            |value: u32| u32::try_from(u64::from(value).checked_mul(u64::from(multiplier))?).ok();
        Some(Self {
            nominal_source: multiply(self.nominal_source)?,
            minimum_source: multiply(self.minimum_source)?,
            maximum_source: multiply(self.maximum_source)?,
            rate_only: true,
            ..self
        })
    }
    #[cfg(rtc)]
    pub(crate) const fn rtc_source() -> Self {
        #[cfg(rcc_lsi_sysclk)]
        {
            Self::lsi()
        }
        #[cfg(not(rcc_lsi_sysclk))]
        {
            use crate::{peripherals::RTC, rtc::sealed::Instance};
            Self {
                nominal_source: RTC::SOURCE_NOMINAL_HZ,
                minimum_source: RTC::SOURCE_MINIMUM_HZ,
                maximum_source: RTC::SOURCE_MAXIMUM_HZ,
                divisor: 1,
                #[cfg(any(rcc_pll, rcc_lsi_sysclk))]
                rate_only: false,
                temperature_c: RTC::TEMPERATURE_C,
                supply_mv: RTC::SUPPLY_MV,
            }
        }
    }
    /// Exact integer divisor needed to produce a requested nominal clock.
    /// Reject rational rates instead of reconstructing them from rounded Hertz.
    #[cfg(feature = "_time-driver")]
    pub(crate) fn exact_divisor_for(self, target: u32) -> Option<u32> {
        let denominator = u64::from(target).checked_mul(u64::from(self.divisor))?;
        let source = u64::from(self.nominal_source);
        if denominator == 0 || source % denominator != 0 {
            return None;
        }
        u32::try_from(source / denominator).ok().filter(|v| *v != 0)
    }
    /// Nominal frequency, rounded down. This is not a measured rate.
    pub const fn nominal(self) -> Hertz {
        Hertz(self.nominal_source / self.divisor)
    }
    /// Conservative lower frequency, rounded down to whole hertz.
    pub const fn minimum(self) -> Hertz {
        Hertz(self.minimum_source / self.divisor)
    }
    /// Conservative upper frequency, rounded up to whole hertz.
    pub const fn maximum(self) -> Hertz {
        Hertz(self.maximum_source.div_ceil(self.divisor))
    }
    /// Source-qualification temperature interval (ambient temperature, C).
    pub const fn temperature_range_c(self) -> (i16, i16) {
        self.temperature_c
    }
    /// Supply interval qualifying this source envelope, in millivolts.
    pub const fn supply_range_mv(self) -> (u16, u16) {
        self.supply_mv
    }
    pub(crate) const fn divided_by(self, divisor: u32) -> Self {
        Self {
            divisor: self.divisor * divisor,
            ..self
        }
    }
    pub(crate) fn maximum_exceeds(self, limit: u32) -> bool {
        u64::from(self.maximum_source) > u64::from(limit) * u64::from(self.divisor)
    }
    #[cfg(any(rcc_pll, adc_cw32l010_v1, adc_cw32l011_v1, adc_cw32l012_v1))]
    pub(crate) fn minimum_below(self, limit: u32) -> bool {
        u64::from(self.minimum_source) < u64::from(limit) * u64::from(self.divisor)
    }
    #[cfg(any(adc_cw32l010_v1, adc_cw32l011_v1, adc_cw32l012_v1))]
    pub(crate) fn acquisition_too_short(self, cycles: u32, picoseconds: u64) -> bool {
        u128::from(self.maximum_source) * u128::from(picoseconds)
            > u128::from(cycles) * 1_000_000_000_000 * u128::from(self.divisor)
    }
    /// Whether this source has the cycle-duration qualification used by the
    /// strict duration helpers. Rate bounds alone do not establish it.
    /// A rate-only envelope does not assert that cycle durations are unbounded.
    pub const fn has_cycle_timing_bounds(self) -> bool {
        #[cfg(any(rcc_pll, rcc_lsi_sysclk))]
        {
            !self.rate_only
        }
        #[cfg(not(any(rcc_pll, rcc_lsi_sysclk)))]
        {
            true
        }
    }
    /// Lower bound on the duration of a number of clock cycles, in ns.
    ///
    /// # Panics
    /// Panics for a rate-only envelope. Check [`Self::has_cycle_timing_bounds`]
    /// first. Source rate accuracy or a cycle-to-cycle jitter limit alone cannot
    /// be substituted for an absolute period error bound.
    pub fn minimum_duration_ns(self, cycles: u32) -> u64 {
        assert!(
            self.has_cycle_timing_bounds(),
            "source cycle-duration bounds are not qualified"
        );
        ((u128::from(cycles) * 1_000_000_000 * u128::from(self.divisor))
            / u128::from(self.maximum_source)) as u64
    }
    /// Upper bound on the duration of a number of clock cycles, in ns.
    ///
    /// # Panics
    /// Panics for a rate-only envelope. Check
    /// [`Self::has_cycle_timing_bounds`] before requesting strict duration bounds.
    pub fn maximum_duration_ns(self, cycles: u32) -> u64 {
        assert!(
            self.has_cycle_timing_bounds(),
            "source cycle-duration bounds are not qualified"
        );
        (u128::from(cycles) * 1_000_000_000 * u128::from(self.divisor))
            .div_ceil(u128::from(self.minimum_source)) as u64
    }
    /// Rate-derived cycles for an internal software delay. This does not
    /// establish pulse-by-pulse timing qualification. Strict analog consumers
    /// must reject a rate-only source; RTC uses this only between WINDOW polls.
    pub(crate) fn delay_cycles_us(self, microseconds: u32) -> u64 {
        (u64::from(self.maximum_source) * u64::from(microseconds))
            .div_ceil(1_000_000 * u64::from(self.divisor))
    }
    /// CPU cycles covering a peripheral's slowest possible cycle interval.
    /// Treating the endpoints independently is conservative even for a common
    /// oscillator; this also covers bounded drift during startup.
    #[cfg(any(adc_cw32l010_v1, adc_cw32l011_v1, adc_cw32l012_v1))]
    pub(crate) fn cycles_for(self, peripheral: Self, cycles: u32) -> u64 {
        (u64::from(self.maximum_source) * u64::from(peripheral.divisor) * u64::from(cycles))
            .div_ceil(u64::from(self.divisor) * u64::from(peripheral.minimum_source))
    }
}

impl super::Clocks {
    /// Undivided factory HSIOSC; never reconstruct from rounded bus rates.
    #[cfg(autotrim)]
    pub(crate) fn hsi_osc_bounds(self) -> ClockBounds {
        ClockBounds::hsi(1)
    }

    /// Factory-HSI envelope after the HSI divider.
    pub fn hsi_bounds(self) -> ClockBounds {
        ClockBounds::hsi(self.dividers[0])
    }
    /// Qualified envelope of the selected system-clock source.
    pub fn sys_bounds(self) -> ClockBounds {
        #[cfg(rcc_external_clock)]
        {
            self.source
        }
        #[cfg(not(rcc_external_clock))]
        {
            self.hsi_bounds()
        }
    }
    /// CPU/AHB envelope, including all programmed hardware dividers.
    pub fn hclk_bounds(self) -> ClockBounds {
        #[cfg(rcc_external_clock)]
        {
            self.sys_bounds().divided_by(self.dividers[1])
        }
        #[cfg(not(rcc_external_clock))]
        {
            ClockBounds::hsi(self.dividers[1])
        }
    }
    /// APB envelope, including all programmed hardware dividers.
    pub fn pclk_bounds(self) -> ClockBounds {
        #[cfg(rcc_external_clock)]
        {
            self.sys_bounds().divided_by(self.dividers[2])
        }
        #[cfg(not(rcc_external_clock))]
        {
            ClockBounds::hsi(self.dividers[2])
        }
    }
}
