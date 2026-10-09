//! Owned polling input capture for qualified GTIM and buffered ATIM inputs.
//!
//! The pin wrappers and channel methods follow the pinned Embassy STM32 shape.
//! This CW32 subset accepts direct external inputs only and has no IRQ binding,
//! async waits or DMA. Each CCR is one hardware latch, not an event queue.
use super::{
    CapturePin, Channel, InputInstance, TimerChannel,
    input::InputRegisters,
    low_level::{
        ConfigError, CountingMode, FilterValue, InputCaptureMode, Prescaler, Timer, Timing,
    },
};
pub use super::{Ch1, Ch2, Ch3, Ch4};
use crate::{
    Peri,
    gpio::{Flex, Pull},
    time::Hertz,
};
use core::marker::PhantomData;

/// A package-verified input pin owned for a particular timer and channel.
/// Creating this wrapper selects a digital input, with the requested pull.
/// Dropping it disconnects the pad. L012 pull-down limits are checked by GPIO.
pub struct CaptureInput<'d, T: InputInstance, C: TimerChannel> {
    pin: Flex<'d>,
    af: u8,
    pull: Pull,
    _marker: PhantomData<(T, C)>,
}
impl<'d, T: InputInstance, C: TimerChannel> CaptureInput<'d, T, C> {
    /// Own an external capture input. Internal trigger inputs are not exposed.
    /// Unlike STM32's fallible trigger wrapper, a statically verified pin route
    /// needs no `Option` return. Unsupported GPIO pulls panic before connection.
    pub fn from_pin(pin: Peri<'d, impl CapturePin<T, C>>, pull: Pull) -> Self {
        let af = super::sealed::CapturePin::<T, C>::af(&*pin);
        let mut pin = Flex::new(pin);
        pin.set_as_input(pull);
        Self {
            pin,
            af,
            pull,
            _marker: PhantomData,
        }
    }
    pub(crate) fn connect(mut self) -> Flex<'d> {
        self.pin.set_as_af(self.af, false, self.pull);
        self.pin
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ChannelConfig {
    pub mode: InputCaptureMode,
    pub filter: FilterValue,
    #[cfg(gtim_buffered)]
    pub prescaler: u8,
}
impl Default for ChannelConfig {
    fn default() -> Self {
        Self {
            mode: InputCaptureMode::Rising,
            filter: FilterValue::NoFilter,
            #[cfg(gtim_buffered)]
            prescaler: 0,
        }
    }
}

/// A free-running 16-bit counter with up to four independently enabled captures.
///
/// Construction starts CNT at zero, with ARR=65535 and all capture channels
/// disabled. Enable an owned channel explicitly. Timestamps wrap at 65536 ticks;
/// `later.wrapping_sub(earlier)` is elapsed ticks only if fewer than 65536 ticks
/// elapsed. The overflow bit coalesces and cannot disambiguate multiple wraps.
/// Drop disables captures, stops the counter, disconnects pins, then gates RCC.
pub struct InputCapture<'d, T: InputInstance> {
    inner: Timer<'d, T>,
    pins: [Option<Flex<'d>>; 4],
    config: [ChannelConfig; 4],
}
impl<'d, T: InputInstance> InputCapture<'d, T> {
    /// Construct polling capture. `freq` is a maximum counter tick rate, not
    /// the overflow rate. The qualified upper PCLK bound selects a power-of-two
    /// prescaler (1…32768). Panics for an unrepresentable request.
    pub fn new(
        tim: Peri<'d, T>,
        ch1: Option<CaptureInput<'d, T, Ch1>>,
        ch2: Option<CaptureInput<'d, T, Ch2>>,
        ch3: Option<CaptureInput<'d, T, Ch3>>,
        ch4: Option<CaptureInput<'d, T, Ch4>>,
        freq: Hertz,
        counting_mode: CountingMode,
    ) -> Self {
        Self::try_new(tim, ch1, ch2, ch3, ch4, freq, counting_mode)
            .unwrap_or_else(|e| panic!("invalid capture configuration: {}", e))
    }
    /// Checked construction. Invalid rates do not configure the timer. Pin
    /// wrappers have already selected digital inputs and disconnect on error.
    pub fn try_new(
        tim: Peri<'d, T>,
        ch1: Option<CaptureInput<'d, T, Ch1>>,
        ch2: Option<CaptureInput<'d, T, Ch2>>,
        ch3: Option<CaptureInput<'d, T, Ch3>>,
        ch4: Option<CaptureInput<'d, T, Ch4>>,
        freq: Hertz,
        counting_mode: CountingMode,
    ) -> Result<Self, ConfigError> {
        let CountingMode::EdgeAlignedUp = counting_mode;
        let clock = crate::rcc::bus_frequency::<T>().ok_or(ConfigError::ClockNotInitialized)?;
        let bounds = crate::rcc::bus_clock_bounds::<T>().ok_or(ConfigError::ClockNotInitialized)?;
        if freq.0 == 0 {
            return Err(ConfigError::FrequencyZero);
        }
        if freq.0 > bounds.maximum().0 {
            return Err(ConfigError::FrequencyTooHigh);
        }
        let prescaler = Prescaler::ALL
            .into_iter()
            .find(|p| u64::from(bounds.maximum().0) <= u64::from(freq.0) * u64::from(p.divisor()))
            .ok_or(ConfigError::FrequencyTooLow)?;
        let mut inner = Timer::new_configured(tim, clock, bounds, Timing::new(prescaler, 65536)?);
        T::select_external_inputs();
        T::INPUT_REGS.input_initialize();
        let pins = [
            ch1.map(CaptureInput::connect),
            ch2.map(CaptureInput::connect),
            ch3.map(CaptureInput::connect),
            ch4.map(CaptureInput::connect),
        ];
        inner.start();
        Ok(Self {
            inner,
            pins,
            config: [ChannelConfig::default(); 4],
        })
    }
    fn assert_owned(&self, channel: Channel) {
        assert!(
            self.pins[channel.index()].is_some(),
            "capture channel has no owned input"
        );
    }
    fn apply(&self, channel: Channel) {
        T::INPUT_REGS.input_configure(channel, self.config[channel.index()]);
    }
    /// Enable an owned channel. Panics if its input was omitted at construction.
    pub fn enable(&mut self, channel: Channel) {
        self.assert_owned(channel);
        T::INPUT_REGS.input_enable(channel, true, self.config[channel.index()].mode);
    }
    /// Disable an owned channel. Buffered IP also resets its capture prescaler.
    /// The last timestamp and pending flags remain available.
    pub fn disable(&mut self, channel: Channel) {
        self.assert_owned(channel);
        T::INPUT_REGS.input_enable(channel, false, self.config[channel.index()].mode);
    }
    /// Whether capture is enabled. Omitted channels always return false.
    pub fn is_enabled(&self, channel: Channel) -> bool {
        T::INPUT_REGS.input_enabled(channel)
    }
    /// Set captured edges. Briefly disables the channel (and resets the buffered
    /// IP capture prescaler); edges during this gap can be lost. CNT is unchanged.
    pub fn set_input_capture_mode(&mut self, channel: Channel, mode: InputCaptureMode) {
        self.assert_owned(channel);
        self.config[channel.index()].mode = mode;
        self.apply(channel);
    }
    /// Set the input filter. Has the same reconfiguration gap as changing mode.
    pub fn set_input_capture_filter(&mut self, channel: Channel, filter: FilterValue) {
        self.assert_owned(channel);
        self.config[channel.index()].filter = filter;
        self.apply(channel);
    }
    /// Capture each 2^factor-th selected edge (factor 0…3). Panics above 3.
    /// Reconfiguration resets the edge divider and may lose intervening edges.
    #[cfg(gtim_buffered)]
    pub fn set_input_capture_prescaler(&mut self, channel: Channel, factor: u8) {
        self.assert_owned(channel);
        assert!(factor <= 3, "capture prescaler factor must be 0..=3");
        self.config[channel.index()].prescaler = factor;
        self.apply(channel);
    }
    /// Read the latest latched timestamp. A concurrent edge can replace it.
    /// Buffered IP clears CCyIF on this read and retains CCyOF.
    /// Classic GTIM retains its pending flag; acknowledge it separately using
    /// `clear_input_interrupt`. Classic IP has no overcapture detector.
    /// No atomic relationship with a separate status read is promised.
    pub fn get_capture_value(&self, channel: Channel) -> u16 {
        self.assert_owned(channel);
        T::INPUT_REGS.capture_value(channel)
    }
    /// Whether the hardware capture flag is pending. Interrupts remain
    /// disabled: this is the hardware status flag, following Embassy's name.
    pub fn get_input_interrupt(&self, channel: Channel) -> bool {
        self.assert_owned(channel);
        T::INPUT_REGS.capture_pending(channel)
    }
    /// Whether a capture overwrote an unread capture. Multiple overruns coalesce;
    /// false is not a lossless-delivery guarantee during concurrent reads.
    #[cfg(gtim_buffered)]
    pub fn is_overcapture_pending(&self, channel: Channel) -> bool {
        self.assert_owned(channel);
        T::INPUT_REGS.overcapture_pending(channel)
    }
    /// Acknowledge only this channel's overcapture flag with an R1W0 write.
    /// A concurrent overcapture can coalesce with this acknowledgement. Disable
    /// the channel first when a stable diagnostic snapshot is required.
    #[cfg(gtim_buffered)]
    pub fn clear_overcapture(&mut self, channel: Channel) {
        self.assert_owned(channel);
        T::INPUT_REGS.clear_overcapture(channel);
    }
    /// Acknowledge this classic GTIM capture flag with an R1W0 command.
    /// There is no overcapture flag or event queue. An edge between a CCR read
    /// and this acknowledgement can be lost without detection. Disable capture
    /// before reading and acknowledging when a stable latest-value snapshot is
    /// required; edges during that disabled interval are intentionally lost.
    #[cfg(gtim_classic)]
    pub fn clear_input_interrupt(&mut self, channel: Channel) {
        self.assert_owned(channel);
        T::INPUT_REGS.clear_capture(channel);
    }
    /// Current free-running counter, modulo 65536.
    pub fn get_counter(&self) -> u16 {
        self.inner.get_counter()
    }
    /// Nominal tick rate rounded down, not a measured frequency.
    pub fn tick_frequency(&self) -> Hertz {
        Hertz(self.inner.kernel_clock().0 / self.inner.prescaler().divisor())
    }
    /// Outward-rounded qualified tick-rate bounds in hertz.
    pub fn tick_frequency_bounds(&self) -> (Hertz, Hertz) {
        let b = self.inner.kernel_clock_bounds();
        let d = self.inner.prescaler().divisor();
        (Hertz(b.minimum().0 / d), Hertz(b.maximum().0.div_ceil(d)))
    }
    /// Coalescing counter-overflow status; not a wrap count.
    pub fn is_overflow_pending(&self) -> bool {
        self.inner.is_overflow_pending()
    }
    /// Clear only the counter-overflow status.
    pub fn clear_overflow(&mut self) {
        self.inner.clear_overflow();
    }
}
impl<T: InputInstance> Drop for InputCapture<'_, T> {
    fn drop(&mut self) {
        for channel in Channel::ALL {
            T::INPUT_REGS.input_enable(channel, false, self.config[channel.index()].mode);
        }
        self.inner.stop();
        for pin in self.pins.iter_mut().flatten() {
            pin.set_as_disconnected();
        }
    }
}
