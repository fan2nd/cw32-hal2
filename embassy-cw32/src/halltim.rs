//! CW32L012 Hall input capture, polling the latest hardware sample.
//!
//! All three filtered inputs capture on both edges. Any input change stores CNT
//! in WIDTH and clears CNT. There is one WIDTH register and one coalescing CAPF
//! flag: there is no FIFO, edge counter, channel tag or overcapture indication.
//! Polling cannot guarantee every edge, even when `additional_capture_pending`
//! is false. STATE is a live sample, not latched with WIDTH. Observed transitions
//! describe successive software samples only; they cannot establish motor
//! direction or validate a complete six-step Hall sequence.
//!
//! Only the frozen, qualified PCLK clock is used. Tick bounds inherit RCC's
//! supply/temperature/factory-trim contract. No DMA, async/IRQ, PWM output,
//! software-capture command or downstream TRGO routing is exposed. The local
//! TRGO source is overflow (reset selection); this module does not enable a
//! consumer. BTIM3 shares the NVIC vector; this driver never touches NVIC.

use crate::{
    Peri, PeripheralType,
    gpio::{Flex, Pin, Pull},
    pac,
    rcc::{ClockBounds, RccPeripheral},
};
use core::marker::PhantomData;
pub use pac::halltim::vals::Prescaler;

/// The largest documented ARR/CNT/WIDTH value.
pub const MAX_COUNT: u32 = 0x00ff_ffff;

#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub prescaler: Prescaler,
    /// Overflow threshold, 1..=MAX_COUNT. Zero behavior is not exposed.
    pub reload: u32,
    /// Enable the first-stage 5/7 filter.
    pub first_stage_filter: bool,
    /// 0..=32767 counter-clock ticks. Pulses shorter than this are rejected;
    /// pulses at least two ticks longer pass. The gap is unspecified.
    pub filter_length: u16,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            prescaler: Prescaler::Div8,
            reload: MAX_COUNT,
            first_stage_filter: true,
            filter_length: 0,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
pub enum Error {
    InvalidConfig,
    ClockNotInitialized,
    ClockNotEnabled,
    HeldInReset,
    /// No capture was observed within the supplied polling-attempt budget.
    Timeout,
    Stopped,
}

pub(crate) mod sealed {
    use super::*;
    pub trait Instance: RccPeripheral {
        fn regs() -> pac::halltim::Halltim;
    }
    pub trait Channel {}
    pub trait InputPin<T: Instance, C: Channel> {
        fn af(&self) -> u8;
    }
}
#[allow(private_bounds)]
pub trait Instance: PeripheralType + sealed::Instance + 'static {}
#[allow(private_bounds)]
pub trait Channel: sealed::Channel {}
pub enum Ch1 {}
pub enum Ch2 {}
pub enum Ch3 {}
impl sealed::Channel for Ch1 {}
impl Channel for Ch1 {}
impl sealed::Channel for Ch2 {}
impl Channel for Ch2 {}
impl sealed::Channel for Ch3 {}
impl Channel for Ch3 {}
#[allow(private_bounds)]
pub trait InputPin<T: Instance, C: Channel>: Pin + sealed::InputPin<T, C> {}

/// Owned input pad; the constructor disconnects it until HallTim connects AF9.
pub struct HallPin<'d, T: Instance, C: Channel> {
    pin: Flex<'d>,
    af: u8,
    pull: Pull,
    _marker: PhantomData<(T, C)>,
}
impl<'d, T: Instance, C: Channel> HallPin<'d, T, C> {
    pub fn new(pin: Peri<'d, impl InputPin<T, C>>, pull: Pull) -> Self {
        let af = sealed::InputPin::<T, C>::af(&*pin);
        Self {
            pin: Flex::new(pin),
            af,
            pull,
            _marker: PhantomData,
        }
    }
    fn connect(mut self) -> Flex<'d> {
        self.pin.set_as_af(self.af, false, self.pull);
        self.pin
    }
}

/// Three input levels in channel order. No phase order or valid motor pattern is assumed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Levels {
    pub ch1: bool,
    pub ch2: bool,
    pub ch3: bool,
}
impl Levels {
    /// CH1 in bit0, CH2 in bit1, CH3 in bit2.
    pub const fn bits(self) -> u8 {
        self.ch1 as u8 | (self.ch2 as u8) << 1 | (self.ch3 as u8) << 2
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum InputChannel {
    Ch1,
    Ch2,
    Ch3,
}
/// Difference between successive software-observed filtered states. A single
/// changed bit does not prove that only one physical edge occurred.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ObservedTransition {
    FirstSample,
    Unchanged,
    OneChanged { channel: InputChannel, high: bool },
    MultipleChanged,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Capture {
    /// Latest WIDTH read, in counter-clock ticks. Earlier captures may be lost.
    /// After overflow this is only a fragment of the elapsed interval.
    pub width_ticks: u32,
    /// Live filtered inputs sampled after WIDTH; not captured atomically with it.
    pub levels: Levels,
    pub transition: ObservedTransition,
    /// Sticky if any overflow was observed since `restart`, conservatively
    /// invalidating full elapsed-time interpretation until the next restart.
    /// It does not identify which captured interval overflowed.
    pub overflow_since_restart: bool,
    /// CAPF was set again after clearing it. Another latest sample is pending;
    /// false never guarantees that no earlier captures were overwritten.
    pub additional_capture_pending: bool,
}

/// Exclusive owner of HALLTIM and one legal pin for each of its three inputs.
pub struct HallTim<'d, T: Instance> {
    _peri: Peri<'d, T>,
    pins: [Flex<'d>; 3],
    tick: ClockBounds,
    previous: Option<Levels>,
    overflow: bool,
}
impl<'d, T: Instance> HallTim<'d, T> {
    /// Configure a stopped timer. All three inputs are required because the
    /// hardware has no per-channel capture disable. No reset line is pulsed.
    pub fn new(
        peri: Peri<'d, T>,
        ch1: HallPin<'d, T, Ch1>,
        ch2: HallPin<'d, T, Ch2>,
        ch3: HallPin<'d, T, Ch3>,
        config: Config,
    ) -> Result<Self, Error> {
        if config.reload == 0 || config.reload > MAX_COUNT || config.filter_length > 0x7fff {
            return Err(Error::InvalidConfig);
        }
        let source = T::kernel_clock_bounds().ok_or(Error::ClockNotInitialized)?;
        let divisor = match config.prescaler {
            Prescaler::Div1 => 1,
            Prescaler::Div2 => 2,
            Prescaler::Div4 => 4,
            Prescaler::Div8 => 8,
        };
        critical_section::with(|cs| {
            if T::RCC_INFO.reset_asserted() {
                return Err(Error::HeldInReset);
            }
            T::RCC_INFO
                .enable_with_cs(cs)
                .map_err(|_| Error::ClockNotEnabled)?;
            if !T::RCC_INFO.is_enabled() {
                return Err(Error::ClockNotEnabled);
            }
            let r = T::regs();
            r.cr().modify(|w| w.set_en(false));
            // Explicitly disable inherited interrupt/DMA requests, including MATCHIE.
            r.dier().write_value(Default::default());
            r.cr().write(|w| {
                w.set_div(config.prescaler);
                w.set_flt1en(config.first_stage_filter);
                w.set_flt2len(config.filter_length);
                w.set_mms(pac::halltim::vals::MasterMode::Overflow);
            });
            r.arr().write(|w| w.set_data(config.reload));
            r.ccr().write(|w| w.set_data(0));
            r.cnt().write(|w| w.set_data(0));
            clear(true, true, true, r);
            Ok(())
        })?;
        Ok(Self {
            _peri: peri,
            pins: [ch1.connect(), ch2.connect(), ch3.connect()],
            tick: source.divided_by(divisor),
            previous: None,
            overflow: false,
        })
    }
    /// Counter-clock envelope, with the same operating limits as RCC.
    pub fn tick_clock(&self) -> ClockBounds {
        self.tick
    }
    /// Resume counting. Pending captures and the current counter are retained.
    pub fn start(&mut self) {
        T::regs().cr().modify(|w| w.set_en(true));
    }
    /// Stop counting/capture; retain pending flags and the latest sample.
    pub fn stop(&mut self) {
        T::regs().cr().modify(|w| w.set_en(false));
    }
    pub fn is_running(&self) -> bool {
        T::regs().cr().read().en()
    }
    /// Discard pending observations, clear CNT, and start a new observation run.
    /// Restarts the counter and software observation state; no software capture is issued.
    /// Cycling EN does not establish that hardware filter history is reset.
    pub fn restart(&mut self) {
        self.stop();
        T::regs().cnt().write(|w| w.set_data(0));
        clear(true, true, true, T::regs());
        self.previous = None;
        self.overflow = false;
        self.start();
    }
    /// Live counter; wraps at ARR and resets on every filtered input transition.
    pub fn counter(&self) -> u32 {
        T::regs().cnt().read().data()
    }
    /// Live raw and filtered channel states from one STATE read.
    pub fn levels(&self) -> (Levels, Levels) {
        let s = T::regs().state().read();
        (
            Levels {
                ch1: s.ch1s(),
                ch2: s.ch2s(),
                ch3: s.ch3s(),
            },
            filtered(s),
        )
    }
    /// Poll once. Overflow-only polls are retained in the next capture result.
    /// Clear CAPF before reading WIDTH so a later edge leaves a pending event.
    /// Hardware may update WIDTH/STATE at any point; this is deliberately a
    /// latest-observation API, not an atomic or lossless capture queue.
    pub fn try_capture(&mut self) -> Option<Capture> {
        let r = T::regs();
        let before = r.isr().read();
        self.overflow |= before.ovf();
        if before.ovf() {
            clear(false, true, false, r);
        }
        if !before.capf() {
            return None;
        }
        clear(true, false, false, r);
        let width_ticks = r.width().read().data();
        let levels = filtered(r.state().read());
        let after = r.isr().read();
        self.overflow |= after.ovf();
        let transition = match self.previous {
            None => ObservedTransition::FirstSample,
            Some(p) => match p.bits() ^ levels.bits() {
                0 => ObservedTransition::Unchanged,
                1 => ObservedTransition::OneChanged {
                    channel: InputChannel::Ch1,
                    high: levels.ch1,
                },
                2 => ObservedTransition::OneChanged {
                    channel: InputChannel::Ch2,
                    high: levels.ch2,
                },
                4 => ObservedTransition::OneChanged {
                    channel: InputChannel::Ch3,
                    high: levels.ch3,
                },
                _ => ObservedTransition::MultipleChanged,
            },
        };
        self.previous = Some(levels);
        Some(Capture {
            width_ticks,
            levels,
            transition,
            overflow_since_restart: self.overflow,
            additional_capture_pending: after.capf(),
        })
    }
    /// Bounded polling. The budget counts attempts, not time; zero never polls.
    /// Timeout preserves counting and any pending event. Does not enter sleep.
    pub fn wait_capture(&mut self, poll_budget: u32) -> Result<Capture, Error> {
        if !self.is_running() {
            return Err(Error::Stopped);
        }
        for _ in 0..poll_budget {
            if let Some(capture) = self.try_capture() {
                return Ok(capture);
            }
            core::hint::spin_loop();
        }
        Err(Error::Timeout)
    }
}
fn filtered(s: pac::halltim::regs::State) -> Levels {
    Levels {
        ch1: s.ch1f(),
        ch2: s.ch2f(),
        ch3: s.ch3f(),
    }
}
fn clear(capture: bool, overflow: bool, matched: bool, r: pac::halltim::Halltim) {
    let mut command = pac::halltim::regs::Icr::write_noop();
    command.set_capf(!capture);
    command.set_ovf(!overflow);
    command.set_matchf(!matched);
    r.icr().write_value(command);
}
impl<T: Instance> Drop for HallTim<'_, T> {
    fn drop(&mut self) {
        self.stop();
        T::regs().dier().write_value(Default::default());
        for pin in &mut self.pins {
            pin.set_as_disconnected();
        }
        // Central RCC_INFO preserves every neighboring gate/reset and any shared group.
        critical_section::with(|cs| {
            let _ = T::RCC_INFO.disable_with_cs(cs);
        });
    }
}
macro_rules! impl_instance {
    ($instance:ident) => {
        impl $crate::halltim::sealed::Instance for $crate::peripherals::$instance {
            fn regs() -> $crate::pac::halltim::Halltim {
                $crate::pac::$instance
            }
        }
        impl $crate::halltim::Instance for $crate::peripherals::$instance {}
    };
}
pub(crate) use impl_instance;
macro_rules! impl_pin {
    ($instance:ident, $channel:ident, $pin:ident, $af:expr) => {
        impl
            $crate::halltim::sealed::InputPin<
                $crate::peripherals::$instance,
                $crate::halltim::$channel,
            > for $crate::peripherals::$pin
        {
            fn af(&self) -> u8 {
                $af
            }
        }
        impl $crate::halltim::InputPin<$crate::peripherals::$instance, $crate::halltim::$channel>
            for $crate::peripherals::$pin
        {
        }
    };
}
pub(crate) use impl_pin;
