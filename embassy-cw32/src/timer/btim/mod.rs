//! Private BTIM polling engine. Register maps remain actual IP-version PACs.
use super::{CounterRegisters, low_level::Timing};
use crate::pac;

#[cfg(any(btim_cw32l010_v1, btim_cw32l012_v1))]
mod cw32l010_v1;
#[cfg(any(btim_cw32l031_v1, btim_cw32l052_v1))]
mod cw32l031_v1;
#[cfg(any(btim_v1, btim_cw32f002_v1))]
mod v1;

// Every member of a group shares the gate AND reset. A per-instance owner must
// never reset/gate a foreign or independently owned sibling. Keep the group
// clock enabled on drop; initialize only the selected register bank in software.
macro_rules! impl_btim {
    ($name:ident) => {
        impl $crate::timer::sealed::BasicInstance for $crate::peripherals::$name {
            type CounterRegisters = $crate::pac::btim::Btim;
            const COUNTER_REGS: Self::CounterRegisters = $crate::pac::$name;
        }
        impl $crate::timer::BasicInstance for $crate::peripherals::$name {}
    };
}
pub(crate) use impl_btim;

pub(super) trait Registers: Copy {
    const FLAGS: u32;
    fn control(self) -> u32;
    fn write_control(self, value: u32);
    fn stopped_control(timing: Timing) -> u32;
    fn counter(self) -> u16;
    fn write_counter(self, value: u16);
    fn write_reload(self, value: u16);
    fn write_prescaler(self, timing: Timing);
    fn latch_prescaler(self);
    fn disable_requests(self);
    fn clear_inputs(self);
    fn status(self) -> u32;
    fn clear_flags(self, value: u32);
}

pub(super) fn initialize<R: Registers>(regs: R, timing: Timing) {
    regs.write_control(0);
    regs.disable_requests();
    regs.clear_inputs();
    regs.write_counter(0);
    regs.write_reload(timing.reload);
    regs.write_prescaler(timing);
    regs.write_control(R::stopped_control(timing));
    regs.latch_prescaler();
    regs.clear_flags(0); // R1W0. Clear all defined flags, reserved bits stay zero.
}

pub(super) fn reconfigure<R: Registers>(regs: R, timing: Timing) {
    let running = regs.control() & 1 != 0;
    regs.write_control(0);
    regs.write_counter(0); // ARR takes effect immediately; never reduce it below CNT.
    regs.write_reload(timing.reload);
    regs.write_prescaler(timing);
    let control = R::stopped_control(timing);
    regs.write_control(control);
    regs.latch_prescaler(); // Reset divider phase via UG on modern IP; classic IP latches on EN rising.
    regs.clear_flags(R::FLAGS & !1); // Preserve unrelated pending flags, no ISR RMW.
    if running {
        regs.write_control(control | 1);
    }
}

impl CounterRegisters for pac::btim::Btim {
    fn counter_initialize(self, timing: Timing) {
        initialize(self, timing);
    }
    fn counter_configure(self, timing: Timing) {
        reconfigure(self, timing);
    }
    fn counter_start(self, timing: Timing) {
        self.write_control(Self::stopped_control(timing) | 1);
    }
    fn counter_stop(self, timing: Timing) {
        self.write_control(Self::stopped_control(timing));
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
        self.clear_flags(Self::FLAGS & !1);
    }
    fn counter_disable_requests(self) {
        self.disable_requests();
    }
}
