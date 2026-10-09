//! Shared low-speed oscillator enable, preserving trim and other users.
use core::num::NonZeroU32;
/// Enable the existing LSI configuration and wait for its hardware stable flag.
/// Never disable LSI on failure or drop: other peripherals can request it.
pub(crate) fn enable_lsi(polls: NonZeroU32) -> bool {
    critical_section::with(|_| {
        crate::pac::SYSCTRL.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_lsien(true);
        });
    });
    for _ in 0..polls.get() {
        if crate::pac::SYSCTRL.cr1().read().lsien() && crate::pac::SYSCTRL.lsi().read().stable() {
            return true;
        }
        core::hint::spin_loop();
    }
    false
}
