//! Typed classic GTIM access. Source distinctions are qualified in build.rs.
use crate::{
    TIME_DRIVER_REGS as R,
    pac::gtim::{regs, vals},
};

pub(super) fn initialize(divisor: u32) {
    R.cr0().write(|w| {
        w.set_en(false);
        w.set_mode(vals::Mode::Timer); // continuous edge-aligned upcount from internal PCLK
        w.set_trs(false);
        #[cfg(any(gtim_v1, gtim_cw32f002_v1))]
        w.set_prs(divisor.trailing_zeros() as u8);
    });
    R.ier().write(|_| {});
    #[cfg(not(gtim_cw32f002_v1))]
    R.dma().write(|_| {});
    R.cr1().write(|_| {});
    R.etr().write(|_| {});
    #[cfg(any(gtim_cw32l031_v1, gtim_cw32l052_v1))]
    R.psc().write(|w| w.set_psc((divisor - 1) as u16));
    R.cmmr().write(|w| {
        // Classic mode zero has no comparison function. Mode A compares and
        // forces the internal output low; no GPIO alternate function is claimed.
        w.set_cc1m(vals::CcMode::CompareLow);
        w.set_cc2m(vals::CcMode::CompareLow);
    });
    R.arr().write(|w| w.set_arr(u16::MAX));
    R.cnt().write(|w| w.set_cnt(0));
    R.ccr1().write(|w| w.set_ccr(0x8000));
    R.ccr2().write(|w| w.set_ccr(0));
    let mut clear = regs::Icr::write_noop();
    clear.set_ov(false);
    clear.set_ti(false);
    clear.set_ud(false);
    clear.set_cc1(false);
    clear.set_cc2(false);
    clear.set_cc3(false);
    clear.set_cc4(false);
    clear.set_dirchange(false);
    R.icr().write_value(clear);
    R.ier().write(|w| {
        w.set_ov(true);
        w.set_cc1(true);
    });
}
pub(super) fn start() {
    R.cr0().modify(|w| w.set_en(true));
}
pub(super) fn alarm_irq(enabled: bool) {
    R.ier().modify(|w| w.set_cc2(enabled));
}
pub(super) fn write_alarm(at: u16) {
    R.ccr2().write(|w| w.set_ccr(at));
}
pub(super) fn clear_alarm() {
    let mut clear = regs::Icr::write_noop();
    clear.set_cc2(false);
    R.icr().write_value(clear);
}
pub(super) fn take_interrupt() -> u8 {
    let status = R.isr().read();
    // One status snapshot and one W0C command. Reserved defaults and events not
    // observed by this snapshot are preserved, including newly arriving flags.
    let mut clear = regs::Icr::write_noop();
    clear.set_ov(!status.ov());
    clear.set_cc1(!status.cc1());
    clear.set_cc2(!status.cc2());
    R.icr().write_value(clear);
    u8::from(status.ov()) + u8::from(status.cc1())
}
