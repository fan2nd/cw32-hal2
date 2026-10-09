//! Main-output PWM with owned, typed alternate-function pins.
//! GTIM and buffered ATIM expose four channels; classic ATIM uses `new3` for
//! its three A outputs. B/complementary output and protection are not exposed.
//!
//! The constructor/channel methods follow Embassy STM32's `SimplePwm` shape.
//! CW32-specific differences are explicit: only edge-aligned up-counting and
//! high-speed push-pull pins are supported; `PwmPin::new` takes only the pin;
//! [`SimplePwm::split`] borrows the driver rather than consuming/leaking it.
//! All channel handles must be released before the driver or frequency can change.
//!
//! Classic GTIM duty/polarity changes take effect immediately. ATIM and buffered GTIM
//! latches ordinary PWM duty changes at the next update. Entering/leaving forced
//! endpoint/disabled modes or changing polarity stops and restarts the entire
//! timer at zero, also committing other channels’ pending duty updates. Frequency
//! changes force every output inactive and restart the period at zero. These
//! operations are not promised to be glitch-free or phase-continuous. They must not drive safety-critical power
//! stages without suitable independent protection and hardware validation.
use core::{convert::Infallible, marker::PhantomData};

#[cfg(gtim_buffered)]
use super::buffered::{apply_channel, reconfigure_pwm, stop_pwm};
#[cfg(gtim_classic)]
use super::low_level::reconfigure_counter;
use super::low_level::{
    ConfigError, CountingMode, OutputPolarity, Timer, Timing, select_timing_for,
};
use super::{
    Ch1, Ch2, Ch3, Ch4, Channel, CounterRegisters, GeneralInstance4Channel, Instance, TimerChannel,
    TimerPin,
};
#[cfg(gtim_classic)]
use crate::pac::gtim::vals::CcMode;
use crate::{
    Peri,
    gpio::{Flex, Pull},
    pac,
    time::Hertz,
};

pub(crate) trait PwmRegisters: CounterRegisters {
    fn pwm_enable(self);
    fn pwm_apply(self, channel: Channel, state: ChannelState, period: u32);
    fn pwm_reconfigure(self, states: &mut [ChannelState; 4], previous: u32, timing: Timing);
    fn pwm_stop(self, states: &[ChannelState; 4], disconnect: impl FnOnce());
}
impl PwmRegisters for pac::gtim::Gtim {
    fn pwm_enable(self) {
        #[cfg(gtim_buffered)]
        super::buffered::enable_pwm_outputs(self);
    }
    fn pwm_apply(self, channel: Channel, state: ChannelState, period: u32) {
        apply_channel(self, channel, state, period);
    }
    fn pwm_reconfigure(self, states: &mut [ChannelState; 4], previous: u32, timing: Timing) {
        reconfigure_pwm(self, states, previous, timing);
    }
    fn pwm_stop(self, states: &[ChannelState; 4], disconnect: impl FnOnce()) {
        stop_pwm(self, states, disconnect);
    }
}

/// A PWM output pin whose timer, channel, AF and package route are checked by type.
///
/// Creating this wrapper disconnects the pad. The PWM constructor selects its AF
/// only after programming the timer's inactive output level. Dropping an unused
/// wrapper leaves the pad disconnected. Pins are high-speed push-pull, no pulls.
pub struct PwmPin<'d, T: Instance, C: TimerChannel> {
    pin: Flex<'d>,
    af: u8,
    _phantom: PhantomData<(T, C)>,
}
impl<'d, T: Instance, C: TimerChannel> PwmPin<'d, T, C> {
    /// Own a verified pin, keeping its alternate function disconnected for now.
    pub fn new(pin: Peri<'d, impl TimerPin<T, C>>) -> Self {
        let af = super::sealed::TimerPin::<T, C>::af(&*pin);
        Self {
            pin: Flex::new(pin),
            af,
            _phantom: PhantomData,
        }
    }
    pub(super) fn connect(mut self) -> Flex<'d> {
        self.pin.set_as_af(self.af, true, Pull::None);
        self.pin
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ChannelState {
    pub enabled: bool,
    pub duty: u32,
    pub polarity: OutputPolarity,
}
impl Default for ChannelState {
    fn default() -> Self {
        Self {
            enabled: false,
            duty: 0,
            polarity: OutputPolarity::ActiveHigh,
        }
    }
}

#[cfg(gtim_classic)]
fn inactive_mode(polarity: OutputPolarity) -> CcMode {
    match polarity {
        OutputPolarity::ActiveHigh => CcMode::ForceLow,
        OutputPolarity::ActiveLow => CcMode::ForceHigh,
    }
}
#[cfg(gtim_classic)]
fn output_mode(state: ChannelState, period: u32) -> CcMode {
    if !state.enabled || state.duty == 0 {
        return inactive_mode(state.polarity);
    }
    if state.duty == period {
        return match state.polarity {
            OutputPolarity::ActiveHigh => CcMode::ForceHigh,
            OutputPolarity::ActiveLow => CcMode::ForceLow,
        };
    }
    match state.polarity {
        OutputPolarity::ActiveHigh => CcMode::PwmHighBelow,
        OutputPolarity::ActiveLow => CcMode::PwmHighAtOrAbove,
    }
}
#[cfg(gtim_classic)]
pub(crate) fn apply_channel(
    regs: pac::gtim::Gtim,
    channel: Channel,
    state: ChannelState,
    period: u32,
) {
    assert!(state.duty <= period);
    // ARR=65535 permits a 65536-tick period, but CCR cannot hold 65536.
    // Endpoints therefore use forced output, never a truncating CCR cast.
    let compare = state.duty.min(65535) as u16;
    match channel {
        Channel::Ch1 => regs.ccr1().write(|v| v.set_ccr(compare)),
        Channel::Ch2 => regs.ccr2().write(|v| v.set_ccr(compare)),
        Channel::Ch3 => regs.ccr3().write(|v| v.set_ccr(compare)),
        Channel::Ch4 => regs.ccr4().write(|v| v.set_ccr(compare)),
    }
    let mode = output_mode(state, period);
    critical_section::with(|_| {
        regs.cmmr().modify(|v| match channel {
            Channel::Ch1 => v.set_cc1m(mode),
            Channel::Ch2 => v.set_cc2m(mode),
            Channel::Ch3 => v.set_cc3m(mode),
            Channel::Ch4 => v.set_cc4m(mode),
        });
    });
}
#[cfg(gtim_classic)]
fn inactive_channels(regs: pac::gtim::Gtim, states: &[ChannelState; 4]) {
    let mut modes = pac::gtim::regs::Cmmr::default();
    modes.set_cc1m(inactive_mode(states[0].polarity));
    modes.set_cc2m(inactive_mode(states[1].polarity));
    modes.set_cc3m(inactive_mode(states[2].polarity));
    modes.set_cc4m(inactive_mode(states[3].polarity));
    critical_section::with(|_| regs.cmmr().write_value(modes));
}
pub(crate) fn fraction(period: u32, numerator: u32, denominator: u32) -> u32 {
    assert!(denominator != 0, "duty denominator must be nonzero");
    assert!(numerator <= denominator, "duty fraction exceeds one");
    (u64::from(numerator) * u64::from(period) / u64::from(denominator)) as u32
}

/// An owned main-output PWM controller with source-qualified channel capabilities.
///
/// Construction starts the counter with all channels disabled at zero duty.
/// Disabled outputs actively drive their inactive level, including after changing
/// polarity. Dropping the driver forces outputs inactive, stops the counter,
/// disconnects every owned pin, then gates the timer clock. GPIO clocks stay on.
pub struct SimplePwm<'d, T: Instance> {
    inner: Timer<'d, T>,
    pins: [Option<Flex<'d>>; 4],
    states: [ChannelState; 4],
}
impl<'d, T: Instance> SimplePwm<'d, T> {
    /// Construct PWM. Panics if clocks are unavailable or frequency is unsupported.
    pub fn new(
        tim: Peri<'d, T>,
        ch1: Option<PwmPin<'d, T, Ch1>>,
        ch2: Option<PwmPin<'d, T, Ch2>>,
        ch3: Option<PwmPin<'d, T, Ch3>>,
        ch4: Option<PwmPin<'d, T, Ch4>>,
        frequency: Hertz,
        counting_mode: CountingMode,
    ) -> Self
    where
        T: GeneralInstance4Channel,
    {
        Self::try_new(tim, ch1, ch2, ch3, ch4, frequency, counting_mode)
            .unwrap_or_else(|e| panic!("invalid PWM configuration: {}", e))
    }
    /// Checked construction. Invalid requests do not touch the timer; the supplied
    /// pin wrappers have already disconnected their pads and are dropped on error.
    /// Tokens are consumed even on error; use `reborrow()` to keep ownership.
    fn try_new_inner(
        tim: Peri<'d, T>,
        ch1: Option<PwmPin<'d, T, Ch1>>,
        ch2: Option<PwmPin<'d, T, Ch2>>,
        ch3: Option<PwmPin<'d, T, Ch3>>,
        ch4: Option<PwmPin<'d, T, Ch4>>,
        frequency: Hertz,
        counting_mode: CountingMode,
    ) -> Result<Self, ConfigError> {
        let CountingMode::EdgeAlignedUp = counting_mode;
        let clock = crate::rcc::bus_frequency::<T>().ok_or(ConfigError::ClockNotInitialized)?;
        let bounds = crate::rcc::bus_clock_bounds::<T>().ok_or(ConfigError::ClockNotInitialized)?;
        let selection_clock = if T::CounterRegisters::QUALIFIED_FREQUENCY {
            bounds.maximum()
        } else {
            clock
        };
        let timing = select_timing_for::<T::CounterRegisters>(selection_clock, frequency)?;
        // Timer configuration forces all outputs low before connecting AFs.
        let mut inner = Timer::new_configured(tim, clock, bounds, timing);
        T::REGS.pwm_enable();
        let pins = [
            ch1.map(PwmPin::connect),
            ch2.map(PwmPin::connect),
            ch3.map(PwmPin::connect),
            ch4.map(PwmPin::connect),
        ];
        inner.start();
        Ok(Self {
            inner,
            pins,
            states: [ChannelState::default(); 4],
        })
    }
    /// Checked four-channel construction. Main outputs only for buffered ATIM.
    pub fn try_new(
        tim: Peri<'d, T>,
        ch1: Option<PwmPin<'d, T, Ch1>>,
        ch2: Option<PwmPin<'d, T, Ch2>>,
        ch3: Option<PwmPin<'d, T, Ch3>>,
        ch4: Option<PwmPin<'d, T, Ch4>>,
        frequency: Hertz,
        counting_mode: CountingMode,
    ) -> Result<Self, ConfigError>
    where
        T: GeneralInstance4Channel,
    {
        Self::try_new_inner(tim, ch1, ch2, ch3, ch4, frequency, counting_mode)
    }
    /// Construct classic ATIM's three main A outputs. B outputs are withheld.
    #[cfg(atim_classic)]
    pub fn new3(
        tim: Peri<'d, T>,
        ch1: Option<PwmPin<'d, T, Ch1>>,
        ch2: Option<PwmPin<'d, T, Ch2>>,
        ch3: Option<PwmPin<'d, T, Ch3>>,
        frequency: Hertz,
        counting_mode: CountingMode,
    ) -> Self
    where
        T: super::AdvancedInstance3Channel,
    {
        Self::try_new3(tim, ch1, ch2, ch3, frequency, counting_mode)
            .unwrap_or_else(|e| panic!("invalid PWM configuration: {}", e))
    }
    /// Checked three-main-output classic ATIM construction.
    #[cfg(atim_classic)]
    pub fn try_new3(
        tim: Peri<'d, T>,
        ch1: Option<PwmPin<'d, T, Ch1>>,
        ch2: Option<PwmPin<'d, T, Ch2>>,
        ch3: Option<PwmPin<'d, T, Ch3>>,
        frequency: Hertz,
        counting_mode: CountingMode,
    ) -> Result<Self, ConfigError>
    where
        T: super::AdvancedInstance3Channel,
    {
        Self::try_new_inner(tim, ch1, ch2, ch3, None, frequency, counting_mode)
    }
    /// A borrowed channel. Dropping this handle leaves the channel's state intact.
    /// Dropping the owning PWM controller disables and disconnects it.
    pub fn channel(&mut self, channel: Channel) -> SimplePwmChannel<'_, T>
    where
        T: GeneralInstance4Channel,
    {
        self.channel_inner(channel)
    }
    fn channel_inner(&mut self, channel: Channel) -> SimplePwmChannel<'_, T> {
        let period = self.inner.period_ticks();
        let frequency = self.get_frequency();
        SimplePwmChannel::new(
            channel,
            &mut self.states[channel.index()],
            period,
            frequency,
        )
    }
    /// Borrow channel 1.
    pub fn ch1(&mut self) -> SimplePwmChannel<'_, T> {
        self.channel_inner(Channel::Ch1)
    }
    /// Borrow channel 2.
    pub fn ch2(&mut self) -> SimplePwmChannel<'_, T> {
        self.channel_inner(Channel::Ch2)
    }
    /// Borrow channel 3.
    pub fn ch3(&mut self) -> SimplePwmChannel<'_, T> {
        self.channel_inner(Channel::Ch3)
    }
    /// Borrow channel 4.
    pub fn ch4(&mut self) -> SimplePwmChannel<'_, T>
    where
        T: GeneralInstance4Channel,
    {
        self.channel_inner(Channel::Ch4)
    }
    /// Borrow four independent channels simultaneously. The controller and its
    /// frequency remain exclusively borrowed until all handles are released.
    pub fn split(&mut self) -> SimplePwmChannels<'_, T>
    where
        T: GeneralInstance4Channel,
    {
        let period = self.inner.period_ticks();
        let frequency = self.get_frequency();
        let [s1, s2, s3, s4] = &mut self.states;
        SimplePwmChannels {
            ch1: SimplePwmChannel::new(Channel::Ch1, s1, period, frequency),
            ch2: SimplePwmChannel::new(Channel::Ch2, s2, period, frequency),
            ch3: SimplePwmChannel::new(Channel::Ch3, s3, period, frequency),
            ch4: SimplePwmChannel::new(Channel::Ch4, s4, period, frequency),
        }
    }
    /// Borrow classic ATIM's three external main outputs independently.
    #[cfg(atim_classic)]
    pub fn split3(
        &mut self,
    ) -> (
        SimplePwmChannel<'_, T>,
        SimplePwmChannel<'_, T>,
        SimplePwmChannel<'_, T>,
    )
    where
        T: super::AdvancedInstance3Channel,
    {
        let period = self.inner.period_ticks();
        let frequency = self.get_frequency();
        let [s1, s2, s3, _] = &mut self.states;
        (
            SimplePwmChannel::new(Channel::Ch1, s1, period, frequency),
            SimplePwmChannel::new(Channel::Ch2, s2, period, frequency),
            SimplePwmChannel::new(Channel::Ch3, s3, period, frequency),
        )
    }
    /// Qualified timer input-clock envelope under the declared RCC conditions.
    pub fn kernel_clock_bounds(&self) -> crate::rcc::ClockBounds {
        self.inner.kernel_clock_bounds()
    }
    /// Outward-rounded PWM frequency envelope, in whole hertz.
    pub fn frequency_bounds(&self) -> (Hertz, Hertz) {
        self.inner.frequency_bounds()
    }
    /// Current PWM frequency rounded down to whole hertz.
    pub fn get_frequency(&self) -> Hertz {
        self.inner.get_frequency()
    }
    /// Nominal timer source frequency; see `kernel_clock_bounds` for its envelope.
    pub fn kernel_clock(&self) -> Hertz {
        self.inner.kernel_clock()
    }
    /// Nominal PWM frequency is `kernel_clock / clock_divisor`.
    pub fn clock_divisor(&self) -> u32 {
        self.inner.clock_divisor()
    }
    /// Counter ticks per period and the exact 100% duty value (classic GTIM: 1…65536;
    /// ATIM/buffered GTIM: 2…65536).
    pub fn max_duty_cycle(&self) -> u32 {
        self.inner.period_ticks()
    }
    /// Change frequency or panic if unsupported. Existing duty fractions are
    /// preserved, rounded down; channels keep their enabled state and polarity.
    /// Outputs are held inactive during reconfiguration, then start a new period.
    pub fn set_frequency(&mut self, frequency: Hertz) {
        self.try_set_frequency(frequency)
            .unwrap_or_else(|e| panic!("invalid PWM frequency: {}", e));
    }
    /// Checked frequency change. An invalid request leaves the waveform unchanged.
    pub fn try_set_frequency(&mut self, frequency: Hertz) -> Result<(), ConfigError> {
        let timing =
            select_timing_for::<T::CounterRegisters>(self.inner.frequency_clock(), frequency)?;
        T::REGS.pwm_reconfigure(&mut self.states, self.inner.period_ticks(), timing);
        self.inner.timing = timing;
        Ok(())
    }
}
impl<T: Instance> Drop for SimplePwm<'_, T> {
    fn drop(&mut self) {
        T::REGS.pwm_stop(&self.states, || {
            for pin in self.pins.iter_mut().flatten() {
                pin.set_as_disconnected();
            }
        });
        // Timer::drop gates the clock after this body; Flex::drop remains harmless.
    }
}

// Classic GTIM has no shadow registers, so quiesce all outputs before any
// period/compare reconfiguration.
#[cfg(gtim_classic)]
pub(crate) fn reconfigure_pwm(
    regs: pac::gtim::Gtim,
    states: &mut [ChannelState; 4],
    previous: u32,
    timing: Timing,
) {
    inactive_channels(regs, states);
    regs.cr0().write(|_| {});
    reconfigure_counter(regs, timing);
    for channel in Channel::ALL {
        let state = &mut states[channel.index()];
        state.duty = fraction(timing.period(), state.duty, previous);
        apply_channel(regs, channel, *state, timing.period());
    }
    regs.counter_start(timing);
}
#[cfg(gtim_classic)]
pub(crate) fn stop_pwm(
    regs: pac::gtim::Gtim,
    states: &[ChannelState; 4],
    disconnect: impl FnOnce(),
) {
    inactive_channels(regs, states);
    regs.cr0().write(|_| {});
    disconnect();
}

/// Four independently borrowable channel handles. Dropping them retains output
/// state; their owning [`SimplePwm`] continues to own the timer and all pins.
pub struct SimplePwmChannels<'a, T: Instance> {
    /// Channel 1.
    pub ch1: SimplePwmChannel<'a, T>,
    /// Channel 2.
    pub ch2: SimplePwmChannel<'a, T>,
    /// Channel 3.
    pub ch3: SimplePwmChannel<'a, T>,
    /// Channel 4.
    pub ch4: SimplePwmChannel<'a, T>,
}

/// A borrowed PWM channel, obtained from [`SimplePwm::channel`] or `split`.
/// Frequency cannot change while a channel is borrowed. Methods accept exact
/// counter ticks using u32, including 65536 for a full 16-bit timer period.
pub struct SimplePwmChannel<'a, T: Instance> {
    regs: T::PwmRegisters,
    channel: Channel,
    state: &'a mut ChannelState,
    period: u32,
    frequency: Hertz,
    _phantom: PhantomData<T>,
}
impl<'a, T: Instance> SimplePwmChannel<'a, T> {
    fn new(channel: Channel, state: &'a mut ChannelState, period: u32, frequency: Hertz) -> Self {
        Self {
            regs: T::REGS,
            channel,
            state,
            period,
            frequency,
            _phantom: PhantomData,
        }
    }
    fn apply(&self) {
        self.regs.pwm_apply(self.channel, *self.state, self.period);
    }
    /// Enable the channel with its retained duty and polarity.
    pub fn enable(&mut self) {
        self.state.enabled = true;
        self.apply();
    }
    /// Actively drive the inactive level, retaining duty for the next enable.
    pub fn disable(&mut self) {
        self.state.enabled = false;
        self.apply();
    }
    /// Whether the channel is enabled, independently of its duty value.
    pub fn is_enabled(&self) -> bool {
        self.state.enabled
    }
    /// PWM frequency rounded down to whole hertz.
    pub fn get_frequency(&self) -> Hertz {
        self.frequency
    }
    /// The full period in timer ticks; this is the 100% duty value.
    pub fn max_duty_cycle(&self) -> u32 {
        self.period
    }
    /// Set 0…max ticks active per period. Panics if out of range. This does not
    /// automatically enable a disabled output. Classic updates are immediate; buffered
    /// GTIM intermediate duty updates latch at overflow. Mode transitions restart
    /// the whole timer and commit every pending compare value.
    pub fn set_duty_cycle(&mut self, duty: u32) {
        assert!(duty <= self.period, "duty exceeds period");
        self.state.duty = duty;
        self.apply();
    }
    /// Current requested active ticks, including while disabled.
    pub fn current_duty_cycle(&self) -> u32 {
        self.state.duty
    }
    /// Set zero duty without changing the enable state.
    pub fn set_duty_cycle_fully_off(&mut self) {
        self.set_duty_cycle(0);
    }
    /// Set full duty without changing the enable state. Handles ARR=65535 exactly.
    pub fn set_duty_cycle_fully_on(&mut self) {
        self.set_duty_cycle(self.period);
    }
    /// Set a fraction rounded down to timer ticks. Panics for denominator zero
    /// or numerator above denominator. Multiplication is widened to avoid overflow.
    pub fn set_duty_cycle_fraction(&mut self, numerator: u32, denominator: u32) {
        self.set_duty_cycle(fraction(self.period, numerator, denominator));
    }
    /// Set 0…100 percent, rounded down to timer ticks. Panics above 100.
    pub fn set_duty_cycle_percent(&mut self, percent: u8) {
        self.set_duty_cycle_fraction(u32::from(percent), 100);
    }
    /// Select active-high/low, including its corresponding disabled level.
    pub fn set_polarity(&mut self, polarity: OutputPolarity) {
        self.state.polarity = polarity;
        self.apply();
    }
}

impl<T: Instance> embedded_hal::pwm::ErrorType for SimplePwmChannel<'_, T> {
    type Error = Infallible;
}
/// The embedded-hal u16 interface uses a normalized 0…65535 scale rather than
/// the inherent methods' exact timer-tick scale. This preserves both endpoints
/// even for a 65536-tick period. Intermediate values round down to timer ticks.
impl<T: Instance> embedded_hal::pwm::SetDutyCycle for SimplePwmChannel<'_, T> {
    fn max_duty_cycle(&self) -> u16 {
        u16::MAX
    }
    fn set_duty_cycle(&mut self, duty: u16) -> Result<(), Infallible> {
        self.set_duty_cycle_fraction(u32::from(duty), u32::from(u16::MAX));
        Ok(())
    }
}
