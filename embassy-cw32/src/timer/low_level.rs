//! Owned, polling basic/general-purpose/advanced timer counter. This is not an Embassy global time driver.
use super::{BasicInstance, CounterRegisters};
use crate::{Peri, time::Hertz};

/// Counter/PWM configuration error. Invalid frequency requests leave registers unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
pub enum ConfigError {
    /// Call [`crate::init`] before constructing a timer.
    ClockNotInitialized,
    /// Requested frequency is zero.
    FrequencyZero,
    /// Requested frequency exceeds PCLK.
    FrequencyTooHigh,
    /// Even the maximum prescaler and period exceed the requested frequency.
    FrequencyTooLow,
    /// The requested period is outside this timer’s supported range (ATIM/buffered GTIM: 2…65536; classic GTIM/BTIM: 1…65536).
    InvalidPeriod,
    /// This hardware does not implement the requested prescaler divisor.
    UnsupportedPrescaler,
}
impl core::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::ClockNotInitialized => "timer peripheral clock is not initialized",
            Self::FrequencyZero => "timer frequency must be nonzero",
            Self::FrequencyTooHigh => "timer frequency exceeds PCLK",
            Self::FrequencyTooLow => "timer frequency is below the supported range",
            Self::UnsupportedPrescaler => "unsupported timer prescaler",
            Self::InvalidPeriod => "timer period is outside the supported range",
        })
    }
}
impl core::error::Error for ConfigError {}

/// Input capture edge selection for source-qualified timer inputs.
#[cfg(any(gtim_classic, gtim_buffered))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum InputCaptureMode {
    /// Rising edges.
    Rising,
    /// Falling edges.
    Falling,
    /// Rising and falling edges (capture only, not encoder polarity).
    BothEdges,
}

/// Buffered-timer digital input filter. This driver fixes CKD=0, so fDTS=PCLK.
/// N consecutive equal samples qualify an input transition. Filtering delays
/// edges and rejects short pulses; it does not queue captures or debounce every
/// mechanical switch. Sampling uses PCLK, independently of the CNT prescaler.
#[cfg(gtim_buffered)]
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum FilterValue {
    /// No filter; sample at PCLK.
    NoFilter = 0,
    /// PCLK, 2 samples.
    FckIntN2 = 1,
    /// PCLK, 4 samples.
    FckIntN4 = 2,
    /// PCLK, 8 samples.
    FckIntN8 = 3,
    /// PCLK / 2, 6 samples.
    FdtsDiv2N6 = 4,
    /// PCLK / 2, 8 samples.
    FdtsDiv2N8 = 5,
    /// PCLK / 4, 6 samples.
    FdtsDiv4N6 = 6,
    /// PCLK / 4, 8 samples.
    FdtsDiv4N8 = 7,
    /// PCLK / 8, 6 samples.
    FdtsDiv8N6 = 8,
    /// PCLK / 8, 8 samples.
    FdtsDiv8N8 = 9,
    /// PCLK / 16, 5 samples.
    FdtsDiv16N5 = 10,
    /// PCLK / 16, 6 samples.
    FdtsDiv16N6 = 11,
    /// PCLK / 16, 8 samples.
    FdtsDiv16N8 = 12,
    /// PCLK / 32, 5 samples.
    FdtsDiv32N5 = 13,
    /// PCLK / 32, 6 samples.
    FdtsDiv32N6 = 14,
    /// PCLK / 32, 8 samples.
    FdtsDiv32N8 = 15,
}

/// Classic GTIM digital input filter, sampled from PCLK independently of CNT.
/// N consecutive equal samples qualify a transition. This is not an event queue.
#[cfg(gtim_classic)]
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum FilterValue {
    /// No digital filtering.
    NoFilter = 0,
    /// PCLK, 2 samples.
    FckIntN2 = 1,
    /// PCLK, 4 samples.
    FckIntN4 = 2,
    /// PCLK, 6 samples.
    FckIntN6 = 3,
    /// PCLK / 4, 4 samples.
    FckIntDiv4N4 = 4,
    /// PCLK / 4, 6 samples.
    FckIntDiv4N6 = 5,
    /// PCLK / 8, 4 samples.
    FckIntDiv8N4 = 6,
    /// PCLK / 8, 6 samples.
    FckIntDiv8N6 = 7,
}

/// Supported timer counting direction/alignment.
///
/// This driver exposes up-counting only. Other counting modes are not
/// silently emulated; complementary/protection/encoder modes are not exposed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
pub enum CountingMode {
    /// Count from zero through ARR, then wrap to zero.
    EdgeAlignedUp,
}

/// PWM active output level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum OutputPolarity {
    /// The active part of the duty cycle is high; disabled is low.
    ActiveHigh,
    /// The active part of the duty cycle is low; disabled is high.
    ActiveLow,
}

/// The 16 supported power-of-two PCLK prescalers.
///
/// Classic ATIM accepts only /1, /2, /4, /8, /16, /32, /64 and /256;
/// requesting another divisor returns `UnsupportedPrescaler`.
/// Linear-PSC hardware also supports other divisors; this bounded API
/// deliberately exposes the same 1…32768 power-of-two subset on linear-PSC timers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(u8)]
pub enum Prescaler {
    /// PCLK / 1.
    Div1 = 0,
    /// PCLK / 2.
    Div2,
    /// PCLK / 4.
    Div4,
    /// PCLK / 8.
    Div8,
    /// PCLK / 16.
    Div16,
    /// PCLK / 32.
    Div32,
    /// PCLK / 64.
    Div64,
    /// PCLK / 128.
    Div128,
    /// PCLK / 256.
    Div256,
    /// PCLK / 512.
    Div512,
    /// PCLK / 1024.
    Div1024,
    /// PCLK / 2048.
    Div2048,
    /// PCLK / 4096.
    Div4096,
    /// PCLK / 8192.
    Div8192,
    /// PCLK / 16384.
    Div16384,
    /// PCLK / 32768.
    Div32768,
}
impl Prescaler {
    pub(crate) const ALL: [Self; 16] = [
        Self::Div1,
        Self::Div2,
        Self::Div4,
        Self::Div8,
        Self::Div16,
        Self::Div32,
        Self::Div64,
        Self::Div128,
        Self::Div256,
        Self::Div512,
        Self::Div1024,
        Self::Div2048,
        Self::Div4096,
        Self::Div8192,
        Self::Div16384,
        Self::Div32768,
    ];
    pub(crate) const fn bits(self) -> u8 {
        self as u8
    }
    /// Integer divisor of the PCLK kernel clock.
    pub const fn divisor(self) -> u32 {
        1 << self.bits()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Timing {
    pub prescaler: Prescaler,
    pub reload: u16,
}
impl Timing {
    pub(crate) fn new(prescaler: Prescaler, period: u32) -> Result<Self, ConfigError> {
        if !(1..=65536).contains(&period) {
            return Err(ConfigError::InvalidPeriod);
        }
        Ok(Self {
            prescaler,
            reload: (period - 1) as u16,
        })
    }
    pub(crate) const fn period(self) -> u32 {
        self.reload as u32 + 1
    }
    pub(crate) const fn clock_divisor(self) -> u32 {
        self.prescaler.divisor() * self.period()
    }
    pub(crate) const fn frequency(self, clock: Hertz) -> Hertz {
        Hertz(clock.0 / self.clock_divisor())
    }
}

pub(crate) fn select_timing(clock: Hertz, frequency: Hertz) -> Result<Timing, ConfigError> {
    if clock.0 == 0 {
        return Err(ConfigError::ClockNotInitialized);
    }
    if frequency.0 == 0 {
        return Err(ConfigError::FrequencyZero);
    }
    if frequency.0 > clock.0 {
        return Err(ConfigError::FrequencyTooHigh);
    }
    for prescaler in Prescaler::ALL {
        let denominator = u64::from(frequency.0) * u64::from(prescaler.divisor());
        let period = u64::from(clock.0).div_ceil(denominator);
        if period <= 65536 {
            return Timing::new(prescaler, period as u32);
        }
    }
    Err(ConfigError::FrequencyTooLow)
}

pub(crate) fn explicit_timing_for<R: CounterRegisters>(
    prescaler: Prescaler,
    ticks: u32,
) -> Result<Timing, ConfigError> {
    if !R::PRESCALERS.contains(&prescaler) {
        return Err(ConfigError::UnsupportedPrescaler);
    }
    let timing = Timing::new(prescaler, ticks)?;
    if ticks < R::MIN_PERIOD {
        return Err(ConfigError::InvalidPeriod);
    }
    Ok(timing)
}

pub(crate) fn select_timing_for<R: CounterRegisters>(
    clock: Hertz,
    frequency: Hertz,
) -> Result<Timing, ConfigError> {
    // Validate without IO, then consider only source-qualified hardware divisors.
    select_timing(clock, frequency)?;
    for &prescaler in R::PRESCALERS {
        let denominator = u64::from(frequency.0) * u64::from(prescaler.divisor());
        let period = u64::from(clock.0)
            .div_ceil(denominator)
            .max(u64::from(R::MIN_PERIOD));
        if period <= 65536 {
            return Timing::new(prescaler, period as u32);
        }
    }
    Err(ConfigError::FrequencyTooLow)
}

/// An exclusively owned PCLK-driven basic, general-purpose or advanced counter.
///
/// Construction configures the timer stopped, with 65536 ticks and
/// no prescaling, interrupts or DMA. Start it explicitly. Dropping it stops the
/// counter and disables requests. Independent GTIM/ATIM clocks are gated on drop; the
/// shared BTIM clock stays enabled to protect unowned or independently used siblings.
pub struct Timer<'d, T: BasicInstance> {
    _peripheral: Peri<'d, T>,
    pub(crate) clock: Hertz,
    pub(crate) timing: Timing,
    pub(crate) bounds: crate::rcc::ClockBounds,
}
impl<'d, T: BasicInstance> Timer<'d, T> {
    /// Construct a stopped timer, panicking if RCC has not been initialized.
    pub fn new(peripheral: Peri<'d, T>) -> Self {
        Self::try_new(peripheral).unwrap_or_else(|e| panic!("invalid timer configuration: {}", e))
    }
    /// Construct a stopped timer. On failure the token is consumed; use
    /// `reborrow()` if it should remain available after the attempt.
    pub fn try_new(peripheral: Peri<'d, T>) -> Result<Self, ConfigError> {
        let clock = crate::rcc::bus_frequency::<T>().ok_or(ConfigError::ClockNotInitialized)?;
        let bounds = crate::rcc::bus_clock_bounds::<T>().ok_or(ConfigError::ClockNotInitialized)?;
        if clock.0 == 0 {
            return Err(ConfigError::ClockNotInitialized);
        }
        let timing = Timing::new(Prescaler::Div1, 65536).unwrap();
        Ok(Self::new_configured(peripheral, clock, bounds, timing))
    }
    pub(crate) fn new_configured(
        peripheral: Peri<'d, T>,
        clock: Hertz,
        bounds: crate::rcc::ClockBounds,
        timing: Timing,
    ) -> Self {
        critical_section::with(crate::rcc::enable_and_reset_with_cs::<T>)
            .expect("timer clock gate did not enable");
        T::COUNTER_REGS.counter_initialize(timing);
        Self {
            _peripheral: peripheral,
            clock,
            timing,
            bounds,
        }
    }
    /// Resume counting from the current CNT value. Prescalers have already been
    /// latched by a software update or latch on the EN rising edge, by IP type.
    pub fn start(&mut self) {
        T::COUNTER_REGS.counter_start(self.timing);
    }
    /// Stop counting without changing CNT.
    pub fn stop(&mut self) {
        T::COUNTER_REGS.counter_stop(self.timing);
    }
    /// Whether the counter is running.
    pub fn is_running(&self) -> bool {
        T::COUNTER_REGS.counter_running()
    }
    /// Read the current counter value.
    pub fn get_counter(&self) -> u16 {
        T::COUNTER_REGS.counter_read()
    }
    /// Set CNT. Panics if it exceeds ARR. An active counter continues counting.
    pub fn set_counter(&mut self, value: u16) {
        assert!(value <= self.timing.reload, "counter exceeds reload value");
        T::COUNTER_REGS.counter_write(value);
    }
    /// Nominal PCLK, not a measured rate; there is no APB timer multiplier.
    pub fn kernel_clock(&self) -> Hertz {
        self.clock
    }
    /// Qualified PCLK envelope; valid under RCC's declared operating conditions.
    pub fn kernel_clock_bounds(&self) -> crate::rcc::ClockBounds {
        self.bounds
    }
    /// Outward-rounded frequency envelope in whole hertz, not a measured rate.
    pub fn frequency_bounds(&self) -> (Hertz, Hertz) {
        let bounds = self.kernel_clock_bounds();
        let divisor = self.clock_divisor();
        (
            Hertz(bounds.minimum().0 / divisor),
            Hertz(bounds.maximum().0.div_ceil(divisor)),
        )
    }
    pub(crate) fn frequency_clock(&self) -> Hertz {
        if T::CounterRegisters::QUALIFIED_FREQUENCY {
            self.kernel_clock_bounds().maximum()
        } else {
            self.clock
        }
    }
    /// Current hardware prescaler.
    pub fn prescaler(&self) -> Prescaler {
        self.timing.prescaler
    }
    /// Counter ticks per period, including the ARR value.
    pub fn period_ticks(&self) -> u32 {
        self.timing.period()
    }
    /// Exact frequency denominator: frequency = `kernel_clock / clock_divisor`.
    pub fn clock_divisor(&self) -> u32 {
        self.timing.clock_divisor()
    }
    /// Overflow frequency rounded down to whole hertz. It may be zero for an
    /// explicitly configured sub-hertz period; use [`Self::clock_divisor`] then.
    pub fn get_frequency(&self) -> Hertz {
        self.timing.frequency(self.clock)
    }
    /// Select the highest-resolution period in the supported prescaler subset whose exact frequency does not
    /// exceed `frequency`. ATIM uses the qualified PCLK upper endpoint; existing
    /// GTIM/BTIM selection retains its nominal-clock policy. Panics for an unrepresentable request.
    pub fn set_frequency(&mut self, frequency: Hertz) {
        self.try_set_frequency(frequency)
            .unwrap_or_else(|e| panic!("invalid timer frequency: {}", e));
    }
    /// Checked frequency selection. Stops/resets/restarts CNT and latches the
    /// prescaler immediately if running; invalid requests make no changes.
    pub fn try_set_frequency(&mut self, frequency: Hertz) -> Result<(), ConfigError> {
        let timing = select_timing_for::<T::CounterRegisters>(self.frequency_clock(), frequency)?;
        self.apply_timing(timing);
        Ok(())
    }
    /// Explicitly select a prescaler and period (ATIM/buffered GTIM: 2…65536 ticks;
    /// classic GTIM/BTIM: 1…65536). Restarts CNT at zero,
    /// preserves stopped/running state and clears the previous overflow flag.
    pub fn set_period(&mut self, prescaler: Prescaler, ticks: u32) -> Result<(), ConfigError> {
        let timing = explicit_timing_for::<T::CounterRegisters>(prescaler, ticks)?;
        self.apply_timing(timing);
        Ok(())
    }
    pub(crate) fn apply_timing(&mut self, timing: Timing) {
        T::COUNTER_REGS.counter_configure(timing);
        self.timing = timing;
    }
    /// Whether an overflow occurred since the last clear. Multiple overflows
    /// coalesce into one bit; this must not be used as a lossless async timebase.
    pub fn is_overflow_pending(&self) -> bool {
        T::COUNTER_REGS.counter_overflow()
    }
    /// Clear OV with an R1W0 write, preserving every other pending event.
    pub fn clear_overflow(&mut self) {
        T::COUNTER_REGS.counter_clear_overflow();
    }
}
impl<T: BasicInstance> Drop for Timer<'_, T> {
    fn drop(&mut self) {
        self.stop();
        T::COUNTER_REGS.counter_disable_requests();
        critical_section::with(crate::rcc::disable_with_cs::<T>)
            .expect("timer clock gate did not disable");
    }
}

// Stop and reset CNT before reducing ARR: ARR takes effect immediately. PRS
// latches on the following EN rising edge, not on an invented STM32 update event.
#[cfg(gtim_classic)]
pub(crate) fn reconfigure_counter(regs: crate::pac::gtim::Gtim, timing: Timing) {
    let running = regs.cr0().read().en();
    regs.cr0().write(|_| {});
    regs.cnt().write(|v| v.set_cnt(0));
    regs.arr().write(|v| v.set_arr(timing.reload));
    #[cfg(any(gtim_cw32l031_v1, gtim_cw32l052_v1))]
    regs.psc()
        .write(|v| v.set_psc((timing.prescaler.divisor() - 1) as u16));
    regs.counter_stop(timing);
    regs.counter_clear_overflow();
    if running {
        regs.counter_start(timing);
    }
}
