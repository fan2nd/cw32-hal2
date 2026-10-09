//! L010/L011/L012 buffered GTIM engine, using each selected PAC register version.
//! Linear PSC/ARR/CCR preloads commit through UG; no classic register aliases.
use super::{
    Channel, CounterRegisters,
    low_level::{OutputPolarity, Timing},
    simple_pwm::{ChannelState, fraction},
};
use crate::pac;
mod registers;

const CONTROL: u32 = (1 << 7) | (1 << 2); // ARPE, URS; up, continuous, PCLK.
const FLAGS: u32 = 0x00f0_1e5f;

pub(crate) trait Registers: Copy {
    fn control(self) -> u32;
    fn write_control(self, value: u32);
    fn counter(self) -> u16;
    fn write_counter(self, value: u16);
    fn write_reload(self, value: u16);
    fn write_prescaler(self, value: u16);
    fn update(self);
    fn disable_requests(self);
    fn clear_inputs(self);
    fn status(self) -> u32;
    fn clear_flags(self, value: u32);
    fn modes(self, pair: usize) -> u32;
    fn write_modes(self, pair: usize, value: u32);
    fn enables(self) -> u32;
    fn write_enables(self, value: u32);
    fn write_compare(self, channel: Channel, value: u16);
}

fn write_mode<R: Registers>(regs: R, channel: Channel, mode: u32) {
    let index = channel.index();
    let shift = (index % 2) * 8;
    // Clear CCyS, FE, CE and the high OCyM bit. Always keep CCR preload enabled.
    let mask = (0xff << shift) | (1 << (16 + shift));
    let value = (regs.modes(index / 2) & !mask) | ((8 | (mode << 4)) << shift);
    regs.write_modes(index / 2, value);
}
fn mode<R: Registers>(regs: R, channel: Channel) -> u32 {
    let index = channel.index();
    let shift = (index % 2) * 8;
    let value = regs.modes(index / 2);
    ((value >> (4 + shift)) & 7) | (((value >> (16 + shift)) & 1) << 3)
}
pub(super) fn output_mode(state: ChannelState, period: u32) -> u32 {
    if !state.enabled || state.duty == 0 {
        4
    } else if state.duty == period {
        5
    } else {
        6
    }
}
fn write_polarity<R: Registers>(regs: R, channel: Channel, polarity: OutputPolarity) {
    let shift = channel.index() * 4;
    let value = 1 | if polarity == OutputPolarity::ActiveLow {
        2
    } else {
        0
    };
    // E stays enabled for forced-inactive drive. NP and reserved bit stay zero.
    regs.write_enables((regs.enables() & !(15 << shift)) | (value << shift));
}
fn write_period<R: Registers>(regs: R, timing: Timing) {
    assert!(timing.period() >= 2); // ARR=0 stops these counters.
    regs.write_reload(timing.reload);
    regs.write_prescaler((timing.prescaler.divisor() - 1) as u16);
}
pub(super) fn initialize<R: Registers>(regs: R, timing: Timing) {
    regs.write_control(0);
    regs.disable_requests();
    regs.write_enables(0); // Disconnect timer outputs until all modes are safe.
    regs.clear_inputs();
    regs.write_modes(0, 0x4848);
    regs.write_modes(1, 0x4848);
    regs.write_control(CONTROL);
    for channel in Channel::ALL {
        regs.write_compare(channel, 0);
    }
    write_period(regs, timing);
    regs.update(); // UG loads PSC/CCR and clears CNT/divider; stopped ARR is already live.
    regs.clear_flags(0); // R1W0, every reserved bit is zero on this IP.
}
pub(super) fn reconfigure_counter<R: Registers>(regs: R, timing: Timing) {
    let running = regs.control() & 1;
    regs.write_control(CONTROL);
    write_period(regs, timing);
    regs.update();
    regs.clear_flags(FLAGS & !1);
    regs.write_control(CONTROL | running);
}
pub(crate) fn enable_pwm_outputs<R: Registers>(regs: R) {
    regs.write_enables(0x1111); // All outputs forced inactive before AF connection.
}

/// Interior PWM-to-PWM updates only write the preloaded CCR. Mode or polarity
/// transitions commit synchronously, preventing an old shadow duty appearing
/// when an endpoint is released. UG restarts all channels and commits every CCR.
pub(crate) fn apply_channel<R: Registers>(
    regs: R,
    channel: Channel,
    state: ChannelState,
    period: u32,
) {
    assert!(state.duty <= period);
    critical_section::with(|_| {
        let desired = output_mode(state, period);
        let active_low = (regs.enables() >> (channel.index() * 4 + 1)) & 1 != 0;
        let running = regs.control() & 1;
        if desired == mode(regs, channel)
            && active_low == (state.polarity == OutputPolarity::ActiveLow)
        {
            regs.write_compare(channel, state.duty.min(65535) as u16);
            return;
        }
        write_mode(regs, channel, 4); // Inactive under the old polarity.
        regs.write_control(CONTROL);
        write_polarity(regs, channel, state.polarity);
        regs.write_compare(channel, state.duty.min(65535) as u16);
        regs.update();
        // The manual explicitly guarantees a fresh comparison on Freeze -> PWM.
        write_mode(regs, channel, 0);
        write_mode(regs, channel, desired);
        regs.write_control(CONTROL | running);
    });
}
fn inactive_channels<R: Registers>(regs: R, states: &[ChannelState; 4]) {
    for channel in Channel::ALL {
        write_mode(regs, channel, 4);
        write_polarity(regs, channel, states[channel.index()].polarity);
    }
}
pub(crate) fn reconfigure_pwm<R: Registers>(
    regs: R,
    states: &mut [ChannelState; 4],
    previous: u32,
    timing: Timing,
) {
    critical_section::with(|_| {
        inactive_channels(regs, states);
        regs.write_control(CONTROL);
        write_period(regs, timing);
        for channel in Channel::ALL {
            let state = &mut states[channel.index()];
            state.duty = fraction(timing.period(), state.duty, previous);
            regs.write_compare(channel, state.duty.min(65535) as u16);
        }
        regs.update(); // Exactly one commit after all shadow values are prepared.
        regs.clear_flags(FLAGS & !1);
        for channel in Channel::ALL {
            write_mode(regs, channel, 0);
            write_mode(
                regs,
                channel,
                output_mode(states[channel.index()], timing.period()),
            );
        }
        regs.write_control(CONTROL | 1);
    });
}
pub(crate) fn stop_pwm<R: Registers>(
    regs: R,
    states: &[ChannelState; 4],
    disconnect: impl FnOnce(),
) {
    critical_section::with(|_| {
        inactive_channels(regs, states);
        regs.write_control(CONTROL);
        disconnect();
        regs.write_enables(0);
    });
}

impl CounterRegisters for pac::gtim::Gtim {
    const MIN_PERIOD: u32 = 2;
    fn counter_initialize(self, timing: Timing) {
        initialize(self, timing);
    }
    fn counter_configure(self, timing: Timing) {
        reconfigure_counter(self, timing);
    }
    fn counter_start(self, _timing: Timing) {
        self.write_control(CONTROL | 1);
    }
    fn counter_stop(self, _timing: Timing) {
        self.write_control(CONTROL);
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
        self.clear_flags(FLAGS & !1);
    }
    fn counter_disable_requests(self) {
        self.disable_requests();
    }
}
