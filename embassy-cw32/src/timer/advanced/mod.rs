//! Source-qualified ATIM up-counter and main-output PWM. Buffered complementary
//! PWM and BK1 are provided by the sibling complementary_pwm owner. Buffered input support is
//! provided by the sibling input_capture and qei modules.
use super::{
    Channel, CounterRegisters,
    low_level::{OutputPolarity, Prescaler, Timing},
    simple_pwm::{ChannelState, PwmRegisters, fraction},
};
use crate::pac;
#[cfg(atim_buffered)]
mod buffered;
#[cfg(atim_classic)]
mod classic;
#[cfg(atim_buffered)]
use buffered as adapter;
#[cfg(atim_classic)]
use classic as adapter;

pub(super) trait Registers: Copy {
    const CHANNELS: usize;
    const FLAGS: u32;
    fn control(self) -> u32;
    fn write_control(self, value: u32);
    fn stopped_control(timing: Timing) -> u32;
    fn initialize(self, timing: Timing);
    fn write_period(self, timing: Timing);
    fn update(self, control: u32);
    fn counter(self) -> u16;
    fn write_counter(self, value: u16);
    fn status(self) -> u32;
    fn clear_flags(self, value: u32);
    fn disable_requests(self);
    fn mode(self, channel: Channel) -> u8;
    fn write_mode(self, channel: Channel, mode: u8);
    fn write_compare(self, channel: Channel, value: u16);
    fn enable_outputs(self);
    fn disable_outputs(self);
}
fn desired_mode(state: ChannelState, period: u32) -> u8 {
    let inactive = u8::from(state.polarity == OutputPolarity::ActiveLow);
    if !state.enabled || state.duty == 0 {
        inactive
    } else if state.duty == period {
        inactive ^ 1
    } else {
        6 | inactive
    }
}
fn clear_overflow<R: Registers>(regs: R) {
    // Preserve all non-UIF events, including break flags. Never read ICR/ISR to clear.
    regs.clear_flags(R::FLAGS & !1);
}
fn reconfigure_counter<R: Registers>(regs: R, timing: Timing) {
    let running = regs.control() & 1;
    let control = R::stopped_control(timing);
    regs.write_control(control);
    regs.write_period(timing);
    regs.update(control);
    clear_overflow(regs);
    regs.write_control(control | running);
}
fn apply_channel<R: Registers>(regs: R, channel: Channel, state: ChannelState, period: u32) {
    assert!(channel.index() < R::CHANNELS && state.duty <= period);
    critical_section::with(|_| {
        let desired = desired_mode(state, period);
        if regs.mode(channel) == desired {
            regs.write_compare(channel, state.duty.min(65535) as u16);
            return;
        }
        let control = regs.control();
        regs.write_mode(
            channel,
            u8::from(state.polarity == OutputPolarity::ActiveLow),
        );
        regs.write_control(control & !1);
        regs.write_compare(channel, state.duty.min(65535) as u16);
        regs.update(control & !1);
        // Whole-timer phase restart also commits peers' pending compare values.
        regs.write_mode(channel, desired);
        regs.write_control(control);
    });
}
fn inactive<R: Registers>(regs: R, states: &[ChannelState; 4]) {
    for &channel in &Channel::ALL[..R::CHANNELS] {
        regs.write_mode(
            channel,
            u8::from(states[channel.index()].polarity == OutputPolarity::ActiveLow),
        );
    }
}
impl CounterRegisters for pac::atim::Atim {
    const MIN_PERIOD: u32 = 2;
    const PRESCALERS: &'static [Prescaler] = adapter::PRESCALERS;
    const QUALIFIED_FREQUENCY: bool = true;
    fn counter_initialize(self, timing: Timing) {
        self.initialize(timing);
    }
    fn counter_configure(self, timing: Timing) {
        reconfigure_counter(self, timing);
    }
    fn counter_start(self, timing: Timing) {
        self.write_control(Self::stopped_control(timing) | 1);
    }
    fn counter_stop(self, timing: Timing) {
        self.write_control(Self::stopped_control(timing));
    }
    fn counter_running(self) -> bool {
        self.control() & 1 != 0
    }
    fn counter_read(self) -> u16 {
        self.counter()
    }
    fn counter_write(self, value: u16) {
        self.write_counter(value);
    }
    fn counter_overflow(self) -> bool {
        self.status() & 1 != 0
    }
    fn counter_clear_overflow(self) {
        clear_overflow(self);
    }
    fn counter_disable_requests(self) {
        self.disable_requests();
        self.disable_outputs();
    }
}
impl PwmRegisters for pac::atim::Atim {
    fn pwm_enable(self) {
        self.enable_outputs();
    }
    fn pwm_apply(self, channel: Channel, state: ChannelState, period: u32) {
        apply_channel(self, channel, state, period);
    }
    fn pwm_reconfigure(self, states: &mut [ChannelState; 4], previous: u32, timing: Timing) {
        critical_section::with(|_| {
            inactive(self, states);
            let control = Self::stopped_control(timing);
            self.write_control(control);
            self.write_period(timing);
            for &channel in &Channel::ALL[..Self::CHANNELS] {
                let state = &mut states[channel.index()];
                state.duty = fraction(timing.period(), state.duty, previous);
                self.write_compare(channel, state.duty.min(65535) as u16);
            }
            self.update(control);
            clear_overflow(self);
            for &channel in &Channel::ALL[..Self::CHANNELS] {
                self.write_mode(
                    channel,
                    desired_mode(states[channel.index()], timing.period()),
                );
            }
            self.write_control(control | 1);
        });
    }
    fn pwm_stop(self, states: &[ChannelState; 4], disconnect: impl FnOnce()) {
        critical_section::with(|_| {
            inactive(self, states);
            self.write_control(self.control() & !1);
            disconnect(); // Driven inactive before disconnect; no MOE=0 level claim.
            self.disable_outputs();
        });
    }
}
impl super::sealed::BasicInstance for crate::peripherals::ATIM {
    type CounterRegisters = pac::atim::Atim;
    const COUNTER_REGS: Self::CounterRegisters = pac::ATIM;
}
impl super::BasicInstance for crate::peripherals::ATIM {}
impl super::sealed::Instance for crate::peripherals::ATIM {
    type PwmRegisters = pac::atim::Atim;
    const REGS: Self::PwmRegisters = pac::ATIM;
    const NUMBER: u8 = 1;
}
impl super::Instance for crate::peripherals::ATIM {}
#[cfg(atim_classic)]
impl super::AdvancedInstance3Channel for crate::peripherals::ATIM {}
#[cfg(atim_buffered)]
impl super::GeneralInstance4Channel for crate::peripherals::ATIM {}

#[cfg(atim_buffered)]
impl super::sealed::InputInstance for crate::peripherals::ATIM {
    type InputRegisters = pac::atim::Atim;
    const INPUT_REGS: Self::InputRegisters = pac::ATIM;
}
#[cfg(atim_buffered)]
impl super::InputInstance for crate::peripherals::ATIM {}
