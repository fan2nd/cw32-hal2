//! Direct access to the selected classic GTIM PAC.
//! PRS and DMA differences are qualified by each family's own reference manual.
use super::{CounterRegisters, low_level::Timing};
use crate::pac::{self, gtim::vals};

impl CounterRegisters for pac::gtim::Gtim {
    fn counter_initialize(self, timing: Timing) {
        self.cr0().write(|_| {});
        self.counter_disable_requests();
        // Force every output low before connecting any alternate-function pin.
        self.cmmr().write(|v| {
            v.set_cc1m(vals::CcMode::ForceLow);
            v.set_cc2m(vals::CcMode::ForceLow);
            v.set_cc3m(vals::CcMode::ForceLow);
            v.set_cc4m(vals::CcMode::ForceLow);
        });
        self.etr().write(|_| {});
        self.cr1().write(|_| {});
        self.ccr1().write(|v| v.set_ccr(0));
        self.ccr2().write(|v| v.set_ccr(0));
        self.ccr3().write(|v| v.set_ccr(0));
        self.ccr4().write(|v| v.set_ccr(0));
        self.cnt().write(|v| v.set_cnt(0));
        self.arr().write(|v| v.set_arr(timing.reload));
        #[cfg(any(gtim_cw32l031_v1, gtim_cw32l052_v1))]
        self.psc()
            .write(|v| v.set_psc((timing.prescaler.divisor() - 1) as u16));
        self.counter_stop(timing);
        // R1W0 command: retain reserved reset-one bits 8:7, clear owned events.
        let mut clear = pac::gtim::regs::Icr::write_noop();
        clear.set_ov(false);
        clear.set_ti(false);
        clear.set_ud(false);
        clear.set_cc1(false);
        clear.set_cc2(false);
        clear.set_cc3(false);
        clear.set_cc4(false);
        clear.set_dirchange(false);
        self.icr().write_value(clear);
    }
    fn counter_configure(self, timing: Timing) {
        super::low_level::reconfigure_counter(self, timing);
    }
    fn counter_start(self, _timing: Timing) {
        self.cr0().write(|v| {
            v.set_mode(vals::Mode::Timer);
            #[cfg(any(gtim_v1, gtim_cw32f002_v1))]
            v.set_prs(_timing.prescaler.bits());
            v.set_en(true);
        });
    }
    fn counter_stop(self, _timing: Timing) {
        self.cr0().write(|v| {
            v.set_mode(vals::Mode::Timer);
            #[cfg(any(gtim_v1, gtim_cw32f002_v1))]
            v.set_prs(_timing.prescaler.bits());
            v.set_en(false);
        });
    }
    fn counter_running(self) -> bool {
        self.cr0().read().en()
    }
    fn counter_read(self) -> u16 {
        self.cnt().read().cnt()
    }
    fn counter_write(self, value: u16) {
        self.cnt().write(|v| v.set_cnt(value));
    }
    fn counter_overflow(self) -> bool {
        self.isr().read().ov()
    }
    fn counter_clear_overflow(self) {
        // One selective R1W0 command; never read-modify-clear asynchronous flags.
        let mut clear = pac::gtim::regs::Icr::write_noop();
        clear.set_ov(false);
        self.icr().write_value(clear);
    }
    fn counter_disable_requests(self) {
        self.ier().write(|_| {});
        // F002/F003 have no DMA register at the otherwise shared offset.
        #[cfg(not(gtim_cw32f002_v1))]
        self.dma().write(|_| {});
    }
}
