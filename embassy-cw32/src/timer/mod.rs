//! Owned CW32 polling counters and source-qualified main-output PWM.
//!
//! [`low_level::Timer`] retains the [`crate::Peri`] for a verified basic,
//! general-purpose or advanced timer. BTIM has no PWM capability. Classic ATIM
//! has three external main A outputs (`SimplePwm::new3`); its CH4 is internal.
//! Buffered ATIM exposes only main CH1–4, despite six hardware channel pairs.
//!
//! All kernels use PCLK without an APB multiplier. This bounded API provides
//! internal-clock continuous up-counting and qualified prescaler subsets. ATIM
//! selects requested rate ceilings using RCC's qualified upper clock bound.
//! Shared BTIM reset is never asserted and its clock remains enabled on drop;
//! independently owned GTIM/ATIM gates are disabled only after disconnecting PWM.
//!
//! F030/A030 classic ATIM additionally provides owned complementary PWM, fixed
//! dead time, and global MOE. Buffered ATIM provides complementary PWM and BK1 in
//! `complementary_pwm`, with constructor-only symmetric dead time. External counter,
//! async time driver, one-shot, interrupt and DMA are not provided. Buffered
//! GTIM and buffered ATIM also provide owned polling capture and encoder APIs.
//! No silicon validation has been performed. See `docs/atim-counter-pwm.md`.

use crate::PeripheralType;
#[cfg(any(gtim_classic, gtim_buffered))]
use crate::gpio::Pin;

#[cfg(atim)]
mod advanced;
#[cfg(btim)]
pub(crate) mod btim;
#[cfg(gtim_buffered)]
pub(crate) mod buffered;
#[cfg(gtim_classic)]
mod classic;
#[cfg(gtim_classic)]
mod classic_input;
#[cfg(any(atim_buffered, atim_classic_complementary))]
pub mod complementary_pwm;
#[cfg(any(gtim_classic, gtim_buffered))]
mod input;
#[cfg(any(gtim_classic, gtim_buffered))]
pub mod input_capture;
pub mod low_level;
#[cfg(any(gtim_classic, gtim_buffered))]
pub mod qei;
#[cfg(any(gtim_classic, gtim_buffered))]
pub mod simple_pwm;
#[cfg(trigger_btim1_update)]
pub mod trigger;
#[cfg(btim)]
pub(crate) use btim::impl_btim;

/// Counter operations available on a verified timer, without implying capture,
/// compare, PWM, DMA or interrupt support. The peripheral is retained by `Peri`.
#[allow(private_bounds)]
pub trait BasicInstance: PeripheralType + sealed::BasicInstance + 'static {}

pub(crate) trait CounterRegisters: Copy {
    const MIN_PERIOD: u32 = 1;
    const PRESCALERS: &'static [low_level::Prescaler] = &low_level::Prescaler::ALL;
    const QUALIFIED_FREQUENCY: bool = false;
    fn counter_initialize(self, timing: low_level::Timing);
    fn counter_configure(self, timing: low_level::Timing);
    fn counter_start(self, timing: low_level::Timing);
    fn counter_stop(self, timing: low_level::Timing);
    fn counter_running(self) -> bool;
    fn counter_read(self) -> u16;
    fn counter_write(self, value: u16);
    fn counter_overflow(self) -> bool;
    fn counter_clear_overflow(self);
    fn counter_disable_requests(self);
}

/// One of the four general-purpose timer channels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Channel {
    /// Channel 1.
    Ch1 = 0,
    /// Channel 2.
    Ch2 = 1,
    /// Channel 3.
    Ch3 = 2,
    /// Channel 4.
    Ch4 = 3,
}
#[cfg(any(gtim_classic, gtim_buffered))]
impl Channel {
    pub(crate) const ALL: [Self; 4] = [Self::Ch1, Self::Ch2, Self::Ch3, Self::Ch4];
    pub(crate) const fn index(self) -> usize {
        self as usize
    }
}

pub(crate) mod sealed {
    #[allow(private_bounds)]
    pub trait BasicInstance: crate::rcc::RccPeripheral {
        type CounterRegisters: super::CounterRegisters;
        const COUNTER_REGS: Self::CounterRegisters;
    }
    #[cfg(any(gtim_classic, gtim_buffered))]
    #[allow(private_bounds)]
    pub trait Instance {
        type PwmRegisters: super::simple_pwm::PwmRegisters;
        const REGS: Self::PwmRegisters;
        const NUMBER: u8;
    }
    #[cfg(any(gtim_classic, gtim_buffered))]
    #[allow(private_bounds)]
    pub trait InputInstance {
        const ENCODER_FIXED_RELOAD: Option<u16> = None;
        fn select_external_inputs() {}
        type InputRegisters: super::input::InputRegisters;
        const INPUT_REGS: Self::InputRegisters;
    }
    #[cfg(any(gtim_classic, gtim_buffered))]
    pub trait CapturePin<T: super::InputInstance, C: Channel> {
        fn af(&self) -> u8;
    }
    #[cfg(any(atim_buffered, atim_classic_complementary))]
    pub trait ComplementaryInstance {
        const DEAD_TIME_MAX_TICKS: u16;
    }
    #[cfg(any(atim_buffered, atim_classic_complementary))]
    pub trait TimerComplementaryPin<T: super::ComplementaryInstance, C: Channel> {
        fn af(&self) -> u8;
    }
    #[cfg(atim_buffered)]
    pub trait TimerBreakPin<T: super::ComplementaryInstance> {
        fn af(&self) -> u8;
    }
    pub trait Channel {}
    #[cfg(any(gtim_classic, gtim_buffered))]
    pub trait TimerPin<T: Instance, C: Channel> {
        fn af(&self) -> u8;
    }
}

/// A sealed, verified PWM-capable timer peripheral owned using [`crate::Peri`].
#[allow(private_bounds)]
#[cfg(any(gtim_classic, gtim_buffered))]
pub trait Instance: BasicInstance + sealed::Instance + 'static {}
/// A timer qualified for direct-pin input capture and quadrature decoding.
#[cfg(any(gtim_classic, gtim_buffered))]
#[allow(private_bounds)]
pub trait InputInstance: GeneralInstance4Channel + sealed::InputInstance {}

/// A buffered ATIM qualified for owned complementary PWM and break handling.
#[cfg(atim_buffered)]
#[allow(private_bounds)]
pub trait ComplementaryInstance: GeneralInstance4Channel + sealed::ComplementaryInstance {}
/// A classic ATIM qualified for owned three-pair PWM and global output enable.
#[cfg(atim_classic_complementary)]
#[allow(private_bounds)]
pub trait ComplementaryInstance: AdvancedInstance3Channel + sealed::ComplementaryInstance {}
/// A package-qualified complementary PWM pin (classic CHxB or buffered CHxN).
#[cfg(any(atim_buffered, atim_classic_complementary))]
#[allow(private_bounds)]
pub trait TimerComplementaryPin<T: ComplementaryInstance, C: TimerChannel>:
    Pin + sealed::TimerComplementaryPin<T, C>
{
}
/// A package-qualified external BK1 input pin.
#[cfg(atim_buffered)]
#[allow(private_bounds)]
pub trait TimerBreakPin<T: ComplementaryInstance>: Pin + sealed::TimerBreakPin<T> {}

/// A timer instance with four exposed main compare channels.
#[cfg(any(gtim_classic, gtim_buffered))]
pub trait GeneralInstance4Channel: Instance {}
/// A classic ATIM with three external main-output channels; CH4 is internal only.
#[cfg(atim_classic)]
pub trait AdvancedInstance3Channel: Instance {}
/// A sealed type-level timer channel.
#[allow(private_bounds)]
pub trait TimerChannel: sealed::Channel + 'static {}
macro_rules! channels {
    ($($name:ident),*) => {$(
        #[doc = concat!("Type-level ", stringify!($name), " marker.")]
        pub enum $name {}
        impl sealed::Channel for $name {}
        impl TimerChannel for $name {}
    )*};
}
channels!(Ch1, Ch2, Ch3, Ch4);

/// A package-verified alternate-function route for one timer/channel pair.
#[allow(private_bounds)]
#[cfg(any(gtim_classic, gtim_buffered))]
pub trait TimerPin<T: Instance, C: TimerChannel>: Pin + sealed::TimerPin<T, C> {}

#[cfg(any(gtim_classic, gtim_buffered))]
// Reserving the sole GTIM leaves no public instance/pin macro invocation.
#[cfg_attr(feature = "_time-driver", allow(unused_macros))]
macro_rules! impl_gtim {
    ($name:ident, $number:literal) => {
        impl $crate::timer::sealed::BasicInstance for $crate::peripherals::$name {
            type CounterRegisters = $crate::pac::gtim::Gtim;
            const COUNTER_REGS: Self::CounterRegisters = $crate::pac::$name;
        }
        impl $crate::timer::BasicInstance for $crate::peripherals::$name {}
        impl $crate::timer::sealed::Instance for $crate::peripherals::$name {
            type PwmRegisters = $crate::pac::gtim::Gtim;
            const REGS: $crate::pac::gtim::Gtim = $crate::pac::$name;
            const NUMBER: u8 = $number;
        }
        impl $crate::timer::Instance for $crate::peripherals::$name {}
        impl $crate::timer::GeneralInstance4Channel for $crate::peripherals::$name {}
        #[cfg(gtim_buffered)]
        impl $crate::timer::sealed::InputInstance for $crate::peripherals::$name {
            type InputRegisters = $crate::pac::gtim::Gtim;
            const INPUT_REGS: Self::InputRegisters = $crate::pac::$name;
        }
        #[cfg(gtim_buffered)]
        impl $crate::timer::InputInstance for $crate::peripherals::$name {}
    };
}
#[cfg(any(gtim_classic, gtim_buffered))]
#[cfg_attr(feature = "_time-driver", allow(unused_imports))]
pub(crate) use impl_gtim;
#[cfg(any(gtim_classic, gtim_buffered))]
// Reserving the sole GTIM leaves no public instance/pin macro invocation.
#[cfg_attr(feature = "_time-driver", allow(unused_macros))]
macro_rules! impl_timer_pin {
    ($instance:ident, $channel:ident, $pin:ident, $af:literal) => {
        impl
            $crate::timer::sealed::TimerPin<$crate::peripherals::$instance, $crate::timer::$channel>
            for $crate::peripherals::$pin
        {
            fn af(&self) -> u8 {
                $af
            }
        }
        impl $crate::timer::TimerPin<$crate::peripherals::$instance, $crate::timer::$channel>
            for $crate::peripherals::$pin
        {
        }
    };
}
#[cfg(any(gtim_classic, gtim_buffered))]
#[cfg_attr(feature = "_time-driver", allow(unused_imports))]
pub(crate) use impl_timer_pin;

/// A package-verified external capture/encoder input, independent of PWM qualification.
#[allow(private_bounds)]
#[cfg(any(gtim_classic, gtim_buffered))]
pub trait CapturePin<T: InputInstance, C: TimerChannel>: Pin + sealed::CapturePin<T, C> {}

#[cfg(any(gtim_classic, gtim_buffered))]
#[cfg_attr(feature = "_time-driver", allow(unused_macros))]
macro_rules! impl_capture_pin {
    ($instance:ident, $channel:ident, $pin:ident, $af:literal) => {
        impl
            $crate::timer::sealed::CapturePin<
                $crate::peripherals::$instance,
                $crate::timer::$channel,
            > for $crate::peripherals::$pin
        {
            fn af(&self) -> u8 {
                $af
            }
        }
        impl $crate::timer::CapturePin<$crate::peripherals::$instance, $crate::timer::$channel>
            for $crate::peripherals::$pin
        {
        }
    };
}
#[cfg(any(gtim_classic, gtim_buffered))]
#[cfg_attr(feature = "_time-driver", allow(unused_imports))]
pub(crate) use impl_capture_pin;

#[cfg(gtim_classic)]
#[cfg_attr(feature = "_time-driver", allow(unused_macros))]
macro_rules! impl_classic_input {
    ($instance:ident, $mux:ident, $reload:expr) => {
        impl $crate::timer::sealed::InputInstance for $crate::peripherals::$instance {
            type InputRegisters = $crate::pac::gtim::Gtim;
            const INPUT_REGS: Self::InputRegisters = $crate::pac::$instance;
            const ENCODER_FIXED_RELOAD: Option<u16> = $reload;
            fn select_external_inputs() {
                // This dedicated mux belongs to the exclusively owned GTIM.
                // Reserved fields retain their documented zero reset value.
                $crate::pac::SYSCTRL.$mux().write(|v| {
                    v.set_ch1(0);
                    v.set_ch2(0);
                    v.set_ch3(0);
                    v.set_ch4(0);
                });
            }
        }
        impl $crate::timer::InputInstance for $crate::peripherals::$instance {}
    };
}
#[cfg(gtim_classic)]
#[cfg_attr(feature = "_time-driver", allow(unused_imports))]
pub(crate) use impl_classic_input;

#[cfg(any(atim_buffered, atim_classic_complementary))]
macro_rules! impl_complementary_pin {
    ($channel:ident, $pin:ident, $af:literal) => {
        impl
            $crate::timer::sealed::TimerComplementaryPin<
                $crate::peripherals::ATIM,
                $crate::timer::$channel,
            > for $crate::peripherals::$pin
        {
            fn af(&self) -> u8 {
                $af
            }
        }
        impl
            $crate::timer::TimerComplementaryPin<$crate::peripherals::ATIM, $crate::timer::$channel>
            for $crate::peripherals::$pin
        {
        }
    };
}
#[cfg(any(atim_buffered, atim_classic_complementary))]
pub(crate) use impl_complementary_pin;
#[cfg(atim_buffered)]
#[allow(unused_macros)] // L010's BK pads require ownership that this API withholds.
macro_rules! impl_timer_break_pin {
    ($pin:ident, $af:literal) => {
        impl $crate::timer::sealed::TimerBreakPin<$crate::peripherals::ATIM> for $crate::peripherals::$pin {
            fn af(&self) -> u8 { $af }
        }
        impl $crate::timer::TimerBreakPin<$crate::peripherals::ATIM> for $crate::peripherals::$pin {}
    };
}
#[cfg(atim_buffered)]
#[allow(unused_imports)]
pub(crate) use impl_timer_break_pin;
