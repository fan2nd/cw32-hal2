//! Common buffered register layout; request fields retain the selected IP version.
use super::{Channel, Registers, pac};
impl Registers for pac::gtim::Gtim {
    fn control(self) -> u32 {
        self.cr1().read().0
    }
    fn write_control(self, value: u32) {
        self.cr1().write(|v| v.0 = value);
    }
    fn counter(self) -> u16 {
        self.cnt().read().cnt()
    }
    fn write_counter(self, value: u16) {
        self.cnt().write(|v| v.set_cnt(value));
    }
    fn write_reload(self, value: u16) {
        self.arr().write(|v| v.set_arr(value));
    }
    fn write_prescaler(self, value: u16) {
        self.psc().write(|v| v.set_psc(value));
    }
    fn update(self) {
        self.egr().write(|v| v.set_ug(true));
    } // Never RMW self-clearing EGR.
    fn disable_requests(self) {
        // L010/L011 IER disables interrupts; the same L012 SDK/PAC
        // register also contains DMA request enables. Both clear by writing zero.
        self.ier().write(|v| v.0 = 0);
    }
    fn clear_inputs(self) {
        self.smcr().write(|v| v.0 = 0);
        self.cr2().write(|v| v.0 = 0);
        self.ecr().write(|v| v.0 = 0);
        self.tisel().write(|v| v.0 = 0);
        self.af1().write(|v| v.0 = 0);
        self.af2().write(|v| v.0 = 0);
    }
    fn status(self) -> u32 {
        self.isr().read().0
    }
    fn clear_flags(self, value: u32) {
        self.icr().write(|v| v.0 = value);
    }
    fn modes(self, pair: usize) -> u32 {
        match pair {
            0 => self.ccmr1cmp().read().0,
            1 => self.ccmr2cmp().read().0,
            _ => unreachable!(),
        }
    }
    fn write_modes(self, pair: usize, value: u32) {
        match pair {
            0 => self.ccmr1cmp().write(|v| v.0 = value),
            1 => self.ccmr2cmp().write(|v| v.0 = value),
            _ => unreachable!(),
        }
    }
    fn enables(self) -> u32 {
        self.ccer().read().0
    }
    fn write_enables(self, value: u32) {
        self.ccer().write(|v| v.0 = value);
    }
    fn write_compare(self, channel: Channel, value: u16) {
        match channel {
            Channel::Ch1 => self.ccr1().write(|v| v.set_ccr1(value)),
            Channel::Ch2 => self.ccr2().write(|v| v.set_ccr2(value)),
            Channel::Ch3 => self.ccr3().write(|v| v.set_ccr3(value)),
            Channel::Ch4 => self.ccr4().write(|v| v.set_ccr4(value)),
        }
    }
}
