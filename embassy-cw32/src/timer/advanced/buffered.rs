//! Independently qualified L010/L011/L012 ATIM. No GTIM register aliasing.
use super::{Channel, Prescaler, Registers, Timing, pac};
pub(super) const PRESCALERS: &[Prescaler] = &Prescaler::ALL;
const CONTROL: u32 = (1 << 7) | (1 << 2); // ARR preload, overflow-only requests.
impl Registers for pac::atim::Atim {
    const CHANNELS: usize = 4;
    const FLAGS: u32 = 0x00ff_3fff;
    fn control(self) -> u32 {
        self.cr1().read().0
    }
    fn write_control(self, value: u32) {
        self.cr1().write(|v| v.0 = value);
    }
    fn stopped_control(_timing: Timing) -> u32 {
        CONTROL
    }
    fn initialize(self, timing: Timing) {
        self.write_control(0);
        self.disable_requests();
        self.ccer().write(|v| v.0 = 0); // Main and complementary outputs disabled.
        self.bdtr().write(|v| v.0 = 0); // LOCK=0, MOE/AOE/break/dead-time disabled.
        self.dtr2().write(|v| v.0 = 0);
        self.cr2().write(|v| v.0 = 0); // No commutation preloads, idle inversion or trigger output.
        self.smcr().write(|v| v.0 = 0);
        self.rcr().write(|v| v.0 = 0); // Every overflow updates.
        self.ecr().write(|v| v.0 = 0);
        self.tisel1().write(|v| v.0 = 0);
        self.tisel2().write(|v| v.0 = 0);
        self.af1().write(|v| v.0 = 0);
        self.af2().write(|v| v.0 = 0); // No external/comparator break sources.
        self.ccmr1cmp().write(|v| v.0 = 0x4848);
        self.ccmr2cmp().write(|v| v.0 = 0x4848);
        self.ccmr3cmp().write(|v| v.0 = 0x4040); // Unexposed CH5/6 forced low, disabled.
        self.ccr5().write(|v| v.0 = 0);
        self.ccr6().write(|v| v.0 = 0);
        for channel in Channel::ALL {
            self.write_compare(channel, 0);
        }
        self.write_control(CONTROL);
        self.write_period(timing);
        self.update(CONTROL);
        self.clear_flags(0);
    }
    fn write_period(self, timing: Timing) {
        assert!(timing.period() >= 2); // ARR=0 explicitly stops the counter.
        self.arr().write(|v| v.set_arr(timing.reload));
        self.psc()
            .write(|v| v.set_psc((timing.prescaler.divisor() - 1) as u16));
    }
    fn update(self, _control: u32) {
        self.egr().write(|v| v.set_ug(true));
    }
    fn counter(self) -> u16 {
        self.cnt().read().cnt()
    }
    fn write_counter(self, value: u16) {
        self.cnt().write(|v| v.set_cnt(value));
    }
    fn status(self) -> u32 {
        self.isr().read().0
    }
    fn clear_flags(self, value: u32) {
        self.icr().write(|v| v.0 = value);
    }
    fn disable_requests(self) {
        #[cfg(atim_cw32l010_v1)]
        self.ier().write(|v| v.0 = 0);
        #[cfg(atim_cw32l012_v1)]
        self.dier().write(|v| v.0 = 0);
    }
    fn mode(self, channel: Channel) -> u8 {
        let pair = if channel.index() < 2 {
            self.ccmr1cmp().read().0
        } else {
            self.ccmr2cmp().read().0
        };
        let mode = ((pair >> (4 + (channel.index() % 2) * 8)) & 7) as u8;
        match mode {
            4 => 0,
            5 => 1,
            other => other,
        }
    }
    fn write_mode(self, channel: Channel, mode: u8) {
        assert!(matches!(mode, 0 | 1 | 6 | 7));
        let shift = (channel.index() % 2) * 8;
        let mask = (0xff << shift) | (1 << (16 + shift));
        let native = if mode < 2 { mode + 4 } else { mode };
        // Freeze -> PWM guarantees a fresh comparison in each own ATIM manual.
        // CCPC=0 means mode/enable changes are immediate, not COMG-buffered.
        if channel.index() < 2 {
            self.ccmr1cmp()
                .modify(|v| v.0 = (v.0 & !mask) | (8 << shift));
            self.ccmr1cmp()
                .modify(|v| v.0 = (v.0 & !mask) | ((8 | (u32::from(native) << 4)) << shift));
        } else {
            self.ccmr2cmp()
                .modify(|v| v.0 = (v.0 & !mask) | (8 << shift));
            self.ccmr2cmp()
                .modify(|v| v.0 = (v.0 & !mask) | ((8 | (u32::from(native) << 4)) << shift));
        }
    }
    fn write_compare(self, channel: Channel, value: u16) {
        match channel {
            Channel::Ch1 => self.ccr1().write(|v| v.set_ccr1(value)),
            Channel::Ch2 => self.ccr2().write(|v| v.set_ccr2(value)),
            Channel::Ch3 => self.ccr3().write(|v| v.set_ccr3(value)),
            Channel::Ch4 => self.ccr4().write(|v| v.set_ccr4(value)),
        }
    }
    fn enable_outputs(self) {
        self.ccer().write(|v| v.0 = 0x1111); // Positive main outputs1–4 only; NE/NP and CH5/6 zero.
        self.bdtr().write(|v| v.0 = 1 << 15); // MOE; no automatic output, break or dead time.
    }
    fn disable_outputs(self) {
        self.ccer().write(|v| v.0 = 0);
        self.bdtr().write(|v| v.0 = 0);
    }
}
