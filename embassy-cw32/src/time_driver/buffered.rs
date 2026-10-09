//! Typed L010/L011/L012 GTIM access; each selected canonical PAC is used directly.
use crate::{TIME_DRIVER_REGS as R, pac::gtim::regs};

pub(super) fn initialize(divisor: u32) {
    R.cr1().write(|w| {
        w.set_cen(false);
        w.set_urs(true);
    });
    R.ier().write(|_| {});
    R.cr2().write(|_| {});
    R.smcr().write(|_| {}); // PCLK, no slave/reset/encoder input
    R.ccer().write(|_| {}); // no external outputs
    R.ccmr1cmp().write(|w| {
        w.set_cc1s(0);
        w.set_cc2s(0);
        w.set_oc1m(0); // frozen mode still produces compare flags on this IP
        w.set_oc2m(0);
        w.set_oc1pe(false);
        w.set_oc2pe(false);
    });
    R.ccmr2cmp().write(|_| {});
    R.psc().write(|w| w.set_psc((divisor - 1) as u16));
    R.arr().write(|w| w.set_arr(u16::MAX));
    R.cnt().write(|w| w.set_cnt(0));
    R.ccr1().write(|w| w.set_ccr1(0x8000));
    R.ccr2().write(|w| w.set_ccr2(0));
    // GTIM EGR is RW in these manuals. Its self-clearing command must be a
    // direct write, never a read-modify-write. UG latches PSC and resets CNT.
    R.egr().write(|w| w.set_ug(true));
    let mut clear = regs::Icr::write_noop();
    clear.set_uif(false);
    clear.set_cc1if(false);
    clear.set_cc2if(false);
    clear.set_cc3if(false);
    clear.set_cc4if(false);
    clear.set_tif(false);
    clear.set_cc1of(false);
    clear.set_cc2of(false);
    clear.set_cc3of(false);
    clear.set_cc4of(false);
    clear.set_idxf(false);
    clear.set_dirf(false);
    clear.set_ierrf(false);
    clear.set_terrf(false);
    R.icr().write_value(clear);
    R.ier().write(|w| {
        w.set_uie(true);
        w.set_cc1ie(true);
    });
}
pub(super) fn start() {
    R.cr1().modify(|w| w.set_cen(true));
}
pub(super) fn alarm_irq(enabled: bool) {
    R.ier().modify(|w| w.set_cc2ie(enabled));
}
pub(super) fn write_alarm(at: u16) {
    R.ccr2().write(|w| w.set_ccr2(at));
}
pub(super) fn clear_alarm() {
    let mut clear = regs::Icr::write_noop();
    clear.set_cc2if(false);
    R.icr().write_value(clear);
}
pub(super) fn take_interrupt() -> u8 {
    let status = R.isr().read();
    let mut clear = regs::Icr::write_noop();
    clear.set_uif(!status.uif());
    clear.set_cc1if(!status.cc1if());
    clear.set_cc2if(!status.cc2if());
    R.icr().write_value(clear);
    u8::from(status.uif()) + u8::from(status.cc1if())
}
