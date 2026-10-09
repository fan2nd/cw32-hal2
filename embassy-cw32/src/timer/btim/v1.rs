//! x030/F020 v1 and F002/F003 canonical BTIM adapters (F002 has no DMA).
use super::{Registers, Timing, pac};
impl Registers for pac::btim::Btim {
    const FLAGS: u32 = 0x7;
    fn control(self) -> u32 {
        self.bcr().read().0
    }
    fn write_control(self, value: u32) {
        self.bcr().write(|v| v.0 = value);
    }
    fn stopped_control(timing: Timing) -> u32 {
        u32::from(timing.prescaler.bits()) << 7
    }
    fn write_prescaler(self, _timing: Timing) {} // PRS is in BCR.
    fn latch_prescaler(self) {} // New PRS latches on EN rising, not a software UG.
    fn disable_requests(self) {
        self.ier().write(|v| v.0 = 0);
        #[cfg(btim_v1)]
        self.dma().write(|v| v.0 = 0);
    }
    fn clear_inputs(self) {
        self.acr().write(|v| v.0 = 0);
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
