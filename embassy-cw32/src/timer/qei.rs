//! Owned quadrature decoding for qualified GTIM and buffered ATIM inputs.
//!
//! The `Qei`, `Config`, `QeiMode`, `Direction`, `count`, `reset` and
//! `read_direction` names follow the pinned Embassy STM32 API. CW32 counters
//! are uniformly 16-bit, so Config needs no timer type parameter. CH1 and CH2
//! are separately checked by type; the same channel cannot be supplied twice.
use super::{
    CapturePin, Channel, InputInstance,
    input::InputRegisters,
    input_capture::{CaptureInput, ChannelConfig},
    low_level::{ConfigError, FilterValue, Prescaler, Timer},
};
pub use super::{Ch1, Ch2};
use crate::{
    Peri,
    gpio::{Flex, Pull},
};

/// Quadrature decoder configuration. Input signals must meet the own-device
/// datasheet timing and voltage limits; filtering cannot correct illegal phase
/// transitions or recover skipped edges. There is no switch debouncing guarantee.
#[derive(Clone, Copy)]
pub struct Config {
    /// CH1's weak input pull. L012 pull-down restrictions are checked by GPIO.
    pub ch1_pull: Pull,
    /// CH2's weak input pull.
    pub ch2_pull: Pull,
    /// x2 on CH1, x2 on CH2, or x4 on both channels.
    pub mode: QeiMode,
    /// Maximum count, inclusive. Buffered IP accepts 1…65535; classic GTIM
    /// requires 65535, checked from each instance's qualified metadata.
    /// The counter wraps modulo auto_reload + 1.
    pub auto_reload: u16,
    /// Filter applied equally to the two encoder signals. Sample clocks derive
    /// from PCLK; buffered IP uses CKD=0. No QEI event prescaling is exposed.
    pub filter: FilterValue,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            ch1_pull: Pull::None,
            ch2_pull: Pull::None,
            mode: QeiMode::Mode3,
            auto_reload: u16::MAX,
            filter: FilterValue::NoFilter,
        }
    }
}

/// Source-qualified quadrature modes. Both signals determine the direction in
/// every mode. These names follow Embassy; the CW32 mappings are documented here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum QeiMode {
    /// Count both CH1 edges, direction from CH2 (x2; mode=1).
    Mode1,
    /// Count both CH2 edges, direction from CH1 (x2; mode=2).
    Mode2,
    /// Count both edges on both inputs (x4; mode=3).
    Mode3,
}
/// Hardware's most recently observed quadrature direction, not velocity or an
/// atomic companion to count(). It can change independently between reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Direction {
    /// Incrementing count.
    Upcounting,
    /// Decrementing count.
    Downcounting,
}

/// A timer and exactly one owned input for each encoder phase.
///
/// Starts at zero. Count is a hardware position modulo auto_reload + 1, not a
/// signed or unbounded software position. No overflow accumulation, index,
/// direction-change interrupt, DMA, or async wait is provided. Observing the
/// coalescing flags cannot reconstruct the count or ordering of missed wraps.
/// Dropping stops decoding, disconnects both inputs, then gates the timer clock.
pub struct Qei<'d, T: InputInstance> {
    inner: Timer<'d, T>,
    ch1: Flex<'d>,
    ch2: Flex<'d>,
}
impl<'d, T: InputInstance> Qei<'d, T> {
    /// Construct and start the decoder. Invalid configuration panics.
    pub fn new(
        tim: Peri<'d, T>,
        ch1: Peri<'d, impl CapturePin<T, Ch1>>,
        ch2: Peri<'d, impl CapturePin<T, Ch2>>,
        config: Config,
    ) -> Self {
        Self::try_new(tim, ch1, ch2, config)
            .unwrap_or_else(|e| panic!("invalid encoder configuration: {}", e))
    }
    /// Checked period/clock configuration. Tokens are consumed on error; use
    /// reborrow() to retain them. Unsupported GPIO pulls still panic before AF
    /// connection, with the same limits as gpio::Flex::set_as_input.
    pub fn try_new(
        tim: Peri<'d, T>,
        ch1: Peri<'d, impl CapturePin<T, Ch1>>,
        ch2: Peri<'d, impl CapturePin<T, Ch2>>,
        config: Config,
    ) -> Result<Self, ConfigError> {
        if T::ENCODER_FIXED_RELOAD.is_some_and(|reload| reload != config.auto_reload) {
            return Err(ConfigError::InvalidPeriod);
        }
        let timing = super::low_level::explicit_timing_for::<T::CounterRegisters>(
            Prescaler::Div1,
            u32::from(config.auto_reload) + 1,
        )?;
        let clock = crate::rcc::bus_frequency::<T>().ok_or(ConfigError::ClockNotInitialized)?;
        let bounds = crate::rcc::bus_clock_bounds::<T>().ok_or(ConfigError::ClockNotInitialized)?;
        let ch1 = CaptureInput::<T, Ch1>::from_pin(ch1, config.ch1_pull);
        let ch2 = CaptureInput::<T, Ch2>::from_pin(ch2, config.ch2_pull);
        let inner = Timer::new_configured(tim, clock, bounds, timing);
        T::select_external_inputs();
        T::INPUT_REGS.input_initialize();
        let input = ChannelConfig {
            filter: config.filter,
            ..ChannelConfig::default()
        };
        // Rising polarity here means non-inverted encoder input. Hardware
        // encoder mode uses both edges; NP must remain zero, not BothEdges.
        T::INPUT_REGS.input_configure(Channel::Ch1, input);
        T::INPUT_REGS.input_configure(Channel::Ch2, input);
        T::INPUT_REGS.encoder_mode(config.mode);
        let ch1 = ch1.connect();
        let ch2 = ch2.connect();
        #[cfg(gtim_buffered)]
        {
            T::INPUT_REGS.input_enable(Channel::Ch1, true, input.mode);
            T::INPUT_REGS.input_enable(Channel::Ch2, true, input.mode);
        }
        T::INPUT_REGS.input_start();
        Ok(Self { inner, ch1, ch2 })
    }
    /// Read the hardware count, in 0…auto_reload. No wrap extension is implied.
    pub fn count(&self) -> u32 {
        u32::from(self.inner.get_counter())
    }
    /// Read the most recently determined direction. This is independent of a
    /// separately read count; it is not necessarily the direction of a wrap.
    pub fn read_direction(&self) -> Direction {
        T::INPUT_REGS.encoder_direction()
    }
    /// Set position to zero while decoding continues. Edges concurrent with the
    /// write can occur before or after it; this is not a synchronized index reset.
    pub fn reset(&mut self) {
        self.inner.set_counter(0);
    }
    /// Number of distinct counter positions, including zero.
    pub fn period_ticks(&self) -> u32 {
        self.inner.period_ticks()
    }
    /// Qualified timer input-sampling clock envelope, before the digital filter.
    pub fn kernel_clock_bounds(&self) -> crate::rcc::ClockBounds {
        self.inner.kernel_clock_bounds()
    }
    /// Whether overflow is pending. Buffered IP also includes underflow in this
    /// same flag; classic GTIM has a separate `is_underflow_pending` flag.
    /// A coalescing flag is never a signed wrap count.
    pub fn is_overflow_pending(&self) -> bool {
        self.inner.is_overflow_pending()
    }
    /// Whether classic GTIM wrapped from zero down to ARR. Multiple events coalesce.
    #[cfg(gtim_classic)]
    pub fn is_underflow_pending(&self) -> bool {
        T::INPUT_REGS.encoder_underflow()
    }
    /// Clear only classic GTIM's underflow flag, preserving overflow/capture flags.
    #[cfg(gtim_classic)]
    pub fn clear_underflow(&mut self) {
        T::INPUT_REGS.encoder_clear_underflow();
    }
    /// Clear only the overflow flag (also underflow on buffered IP).
    pub fn clear_overflow(&mut self) {
        self.inner.clear_overflow();
    }
}
impl<T: InputInstance> Drop for Qei<'_, T> {
    fn drop(&mut self) {
        self.inner.stop();
        T::INPUT_REGS.input_enable(
            Channel::Ch1,
            false,
            super::low_level::InputCaptureMode::Rising,
        );
        T::INPUT_REGS.input_enable(
            Channel::Ch2,
            false,
            super::low_level::InputCaptureMode::Rising,
        );
        self.ch1.set_as_disconnected();
        self.ch2.set_as_disconnected();
    }
}
