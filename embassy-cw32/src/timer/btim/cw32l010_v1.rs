//! L010/L011 and L012 linear-PSC BTIM adapters, with explicit UG divider-phase reset.
use super::{Registers, Timing, pac};
impl Registers for pac::btim::Btim {
    const FLAGS: u32 = 0x41;
    fn control(self) -> u32 {
        self.cr1().read().0
    }
    fn write_control(self, value: u32) {
        self.cr1().write(|v| v.0 = value);
    }
    fn stopped_control(_timing: Timing) -> u32 {
        1 << 2
    } // URS: overflow-only UIF.
    fn write_prescaler(self, timing: Timing) {
        self.psc()
            .write(|v| v.set_psc((timing.prescaler.divisor() - 1) as u16));
    }
    fn latch_prescaler(self) {
        self.egr().write(|v| v.set_ug(true)); // WO; never read or modify EGR.
    }
    fn disable_requests(self) {
        #[cfg(btim_cw32l010_v1)]
        self.ier().write(|v| v.0 = 0);
        #[cfg(btim_cw32l012_v1)]
        self.dier().write(|v| v.0 = 0); // IRQ and DMA.
    }
    fn clear_inputs(self) {
        self.smcr().write(|v| v.0 = 0); // Internal PCLK, no slave or reset input.
        self.cr2().write(|v| v.0 = 0);
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
    fn status(self) -> u32 {
        self.isr().read().0
    }
    fn clear_flags(self, value: u32) {
        self.icr().write(|v| v.0 = value);
    }
}
