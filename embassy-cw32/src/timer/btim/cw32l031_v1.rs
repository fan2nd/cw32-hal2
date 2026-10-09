//! L031/R031/W031/L083 linear-PSC adapter and L052's CR/ETR naming variant.
use super::{Registers, Timing, pac};
impl Registers for pac::btim::Btim {
    const FLAGS: u32 = 0x7;
    fn control(self) -> u32 {
        #[cfg(btim_cw32l031_v1)]
        {
            self.bcr().read().0
        }
        #[cfg(btim_cw32l052_v1)]
        {
            self.cr().read().0
        }
    }
    fn write_control(self, value: u32) {
        #[cfg(btim_cw32l031_v1)]
        self.bcr().write(|v| v.0 = value);
        #[cfg(btim_cw32l052_v1)]
        self.cr().write(|v| v.0 = value);
    }
    fn stopped_control(_timing: Timing) -> u32 {
        0
    }
    fn write_prescaler(self, timing: Timing) {
        self.psc()
            .write(|v| v.set_psc((timing.prescaler.divisor() - 1) as u16));
    }
    fn latch_prescaler(self) {} // New PSC latches on EN rising or overflow.
    fn disable_requests(self) {
        self.ier().write(|v| v.0 = 0);
        self.dma().write(|v| v.0 = 0);
    }
    fn clear_inputs(self) {
        #[cfg(btim_cw32l031_v1)]
        self.acr().write(|v| v.0 = 0);
        #[cfg(btim_cw32l052_v1)]
        self.etr().write(|v| v.0 = 0);
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
