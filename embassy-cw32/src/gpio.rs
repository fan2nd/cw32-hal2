//! Blocking general-purpose input/output for verified CW32 devices.
//!
//! Direct PAC access follows the selected GPIO IP's official manual and SDK.
//! Configuration read/modify/write
//! sequences are protected by a critical section. Output writes use the dedicated
//! set, reset and toggle registers, so they do not overwrite adjacent pins.
//!
//! GPIO clocks are enabled on first use and left enabled: a port may contain pins
//! owned by several drivers. This module does not change the system clock, disable
//! SWD or disable NRST. On the HSE-capable RCC backend, safe construction
//! rejects pads reserved by an initialized or inherited enabled HSE source.
//! Safe singleton pins exclude
//! debug pads, reset pads requiring remapping, and input-only pads. Asynchronous
//! edge waits are provided by the EXTI driver.

use core::convert::Infallible;

use critical_section::CriticalSection;
use embassy_hal_internal::{Peri, PeripheralType, impl_peripheral};

use crate::pac;

/// Internal weak pull resistor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pull {
    /// Disable both pull resistors.
    None,
    /// Enable the pull-up resistor.
    Up,
    /// Enable the pull-down resistor. On L012 only PF3 supports this setting;
    /// other pins panic. A constructor may already have disconnected its pad.
    #[cfg(not(any(gpio_cw32l010_v1, gpio_cw32l011_v1)))]
    Down,
}

/// GPIO output speed selection.
///
/// On F030/A030/F020, Low/High select the two SPEED register settings. Other
/// GPIO IPs have no software speed setting, so only Default is available. No variant
/// promises a switching frequency; consult the datasheet's electrical limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Speed {
    /// Clear the pin's SPEED bit.
    #[cfg(gpio_v1)]
    Low,
    /// Set the pin's SPEED bit.
    #[cfg(gpio_v1)]
    High,
    /// Fixed hardware drive characteristics; no SPEED register exists.
    #[cfg(not(gpio_v1))]
    Default,
}

/// Digital logic level.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Level {
    /// Logic low.
    Low,
    /// Logic high.
    High,
}

impl From<bool> for Level {
    fn from(value: bool) -> Self {
        if value { Self::High } else { Self::Low }
    }
}

impl From<Level> for bool {
    fn from(value: Level) -> Self {
        value == Level::High
    }
}

/// Number encoding `port * 16 + pin`, where A=0, B=1, C=2 and F=5.
pub type PinNumber = u8;

pub(crate) trait SealedPin {
    fn pin_port(&self) -> PinNumber;
}

/// A GPIO pin owned through an Embassy [`Peri`].
#[allow(private_bounds)]
pub trait Pin: PeripheralType + Into<AnyPin> + SealedPin + Sized + 'static {
    /// Pin number within the port (0 through 15).
    fn pin(&self) -> PinNumber {
        self.pin_port() % 16
    }

    /// Port number: A=0, B=1, C=2 or F=5.
    fn port(&self) -> PinNumber {
        self.pin_port() / 16
    }
}

/// Type-erased GPIO pin.
#[derive(Debug)]
pub struct AnyPin {
    pub(crate) pin_port: PinNumber,
}

impl AnyPin {
    /// Construct a pin token without checking singleton ownership or packaging.
    ///
    /// `pin_port` is `port * 16 + pin`; A=0, B=1, C=2 and F=5. This function
    /// checks only that the pad is supported as a bidirectional GPIO in the selected family.
    /// It does not check that the pad is bonded out on the selected package.
    ///
    /// # Safety
    ///
    /// The caller must own this pad exclusively and verify that it is available
    /// on the actual package/board. Its GPIO clock must not be disabled while a
    /// driver uses it. For debug, reset or oscillator pins, the
    /// caller must first resolve the special-function configuration and accept
    /// the possible loss of debug, reset or oscillator operation. This function
    /// does not change any special-function or system-clock configuration.
    ///
    /// # Panics
    ///
    /// Panics for a pad unsupported by the selected family, including input-only pads.
    pub const unsafe fn steal(pin_port: PinNumber) -> Peri<'static, Self> {
        assert!(
            valid_pin(pin_port),
            "unsupported GPIO pin for selected CW32 family"
        );
        unsafe { Peri::new_unchecked(Self { pin_port }) }
    }

    fn block(&self) -> pac::gpio::Gpio {
        // Like embassy-stm32, the selected family has one GPIO register view.
        // build.rs verifies the used offsets/accesses against every real port
        // block and derives addresses and supported pins from source metadata.
        crate::gpio_block(self.port())
    }
}

impl_peripheral!(AnyPin);
impl Pin for AnyPin {}
impl SealedPin for AnyPin {
    fn pin_port(&self) -> PinNumber {
        self.pin_port
    }
}

impl core::fmt::Display for AnyPin {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "P{}{}", char::from(b'A' + self.port()), self.pin())
    }
}

// Called only by build.rs-generated implementations for verified package pins.
macro_rules! impl_pin {
    ($name:ident, $port:tt, $pin:expr) => {
        impl $crate::gpio::Pin for $crate::peripherals::$name {}
        impl $crate::gpio::SealedPin for $crate::peripherals::$name {
            fn pin_port(&self) -> $crate::gpio::PinNumber {
                $port * 16 + $pin
            }
        }
        impl From<$crate::peripherals::$name> for $crate::gpio::AnyPin {
            fn from(_: $crate::peripherals::$name) -> Self {
                Self {
                    pin_port: $port * 16 + $pin,
                }
            }
        }
    };
}
pub(crate) use impl_pin;

/// Flexible GPIO pin supporting input, push-pull and open-drain output modes.
///
/// The output latch is retained across mode changes. Dropping the driver leaves
/// the pin in analog mode with its output driver and weak pulls disabled.
#[derive(Debug)]
pub struct Flex<'d> {
    pin: Peri<'d, AnyPin>,
}

impl<'d> Flex<'d> {
    /// Enable the GPIO port clock and disconnect the pin.
    ///
    /// The output latch is not changed. Call [`Self::set_level`] before changing
    /// to output mode when a specific initial level is required.
    /// Panics before any GPIO gate, unlock or pad write if an external clock,
    /// inherited LSE or retained AWT reserves this pin for the current boot.
    pub fn new(pin: Peri<'d, impl Pin>) -> Self {
        let pin = pin.into();
        assert!(
            !crate::rcc::lse_pin_reserved(pin.pin_port()),
            "pin is reserved by inherited LSE ownership"
        );
        #[cfg(rcc_hse)]
        assert!(
            !crate::rcc::hse_pin_reserved(pin.pin_port()),
            "pin is reserved by the initialized HSE source"
        );
        #[cfg(any(rcc_cw32f002_v1, rcc_cw32f003_v1))]
        assert!(
            !crate::rcc::hex_pin_reserved(pin.pin_port()),
            "pin is reserved by HEX or retained AWT"
        );
        critical_section::with(|cs| {
            crate::gpio_rcc(pin.port())
                .enable_with_cs(cs)
                .expect("GPIO clock gate did not enable");
            let r = pin.block();
            #[cfg(any(gpio_v1, gpio_cw32l052_v1))]
            r.lock()
                .modify(|v| v.0 = 0x5a5a_0000 | (v.0 & 0xffff & !(1 << pin.pin())));
            #[cfg(gpio_cw32l083_v1)]
            r.lckr()
                .modify(|v| v.0 = 0x5a5a_0000 | (v.0 & 0xffff & !(1 << pin.pin())));
            // Other IPs reserve +0x3c; never send the unlock key there.
            let _ = r;
            pin.disconnect(cs);
        });
        Self { pin }
    }

    /// Configure a digital input and its weak pull resistor.
    ///
    /// On L012, Pull::Down panics for every pin except PF3 before mode changes.
    pub fn set_as_input(&mut self, pull: Pull) {
        critical_section::with(|cs| {
            self.pin.check_pull(pull);
            self.pin.disconnect(cs);
            self.pin.set_af_number(0, cs);
            self.pin.set_pull(pull, cs);
            self.pin
                .block()
                .analog()
                .modify(|v| v.0 &= !(1 << self.pin.pin()));
        });
    }

    /// Configure a push-pull output, disabling both weak pulls.
    ///
    /// The current output latch is applied. Set the desired level first.
    pub fn set_as_output(&mut self, speed: Speed) {
        critical_section::with(|cs| {
            self.pin.set_output(speed, false, Pull::None, cs);
        });
    }

    /// Configure an open-drain output with a readable digital input.
    ///
    /// A low latch drives low; a high latch releases the line. Both weak pulls
    /// are disabled, so an external pull-up is normally required.
    pub fn set_as_input_output(&mut self, speed: Speed) {
        self.set_as_input_output_pull(speed, Pull::None);
    }

    /// Configure open-drain input/output and a weak pull resistor.
    ///
    /// On L012, Pull::Down panics for every pin except PF3 before mode changes.
    pub fn set_as_input_output_pull(&mut self, speed: Speed, pull: Pull) {
        critical_section::with(|cs| {
            self.pin.set_output(speed, true, pull, cs);
        });
    }

    /// Configure a verified peripheral alternate function. Only typed peripheral
    /// drivers call this: arbitrary AF selection is not part of the public API.
    #[cfg(gpio_af)]
    pub(crate) fn set_as_af(&mut self, af: u8, output: bool, pull: Pull) {
        critical_section::with(|cs| {
            self.pin.set_alternate(af, output, false, pull, cs);
        });
    }

    /// Select a verified bidirectional open-drain peripheral route.
    #[cfg(gpio_af)]
    pub(crate) fn set_as_af_open_drain(&mut self, af: u8, pull: Pull) {
        critical_section::with(|cs| {
            self.pin.set_alternate(af, true, true, pull, cs);
        });
    }

    /// Disable the digital input/output path and both weak pulls.
    pub fn set_as_analog(&mut self) {
        self.set_as_disconnected();
    }

    /// Disconnect the pin, retaining only its output latch.
    pub fn set_as_disconnected(&mut self) {
        critical_section::with(|cs| self.pin.disconnect(cs));
    }

    /// Whether this is a digital input or an open-drain input/output.
    pub fn is_input(&self) -> bool {
        let r = self.pin.block();
        let mask = 1 << self.pin.pin();
        r.analog().read().0 & mask == 0
            && (r.dir().read().0 & mask != 0 || r.opendrain().read().0 & mask != 0)
    }

    /// Whether the digital output driver is enabled.
    pub fn is_output(&self) -> bool {
        let r = self.pin.block();
        let mask = 1 << self.pin.pin();
        r.analog().read().0 & mask == 0 && r.dir().read().0 & mask == 0
    }

    /// Whether the pin is in analog/disconnected mode.
    pub fn is_disconnected(&self) -> bool {
        self.pin.block().analog().read().0 & (1 << self.pin.pin()) != 0
    }

    /// Read the digital input level.
    pub fn level(&self) -> Level {
        self.is_high().into()
    }

    /// Read whether the digital input is high.
    pub fn is_high(&self) -> bool {
        self.pin.block().idr().read().0 & (1 << self.pin.pin()) != 0
    }

    /// Read whether the digital input is low.
    pub fn is_low(&self) -> bool {
        !self.is_high()
    }

    /// Read the output latch, which can differ from the input level.
    pub fn output_level(&self) -> Level {
        self.is_set_high().into()
    }

    /// Whether the output latch is high.
    pub fn is_set_high(&self) -> bool {
        self.pin.block().odr().read().0 & (1 << self.pin.pin()) != 0
    }

    /// Whether the output latch is low.
    pub fn is_set_low(&self) -> bool {
        !self.is_set_high()
    }

    /// Atomically set this pin's output latch high.
    pub fn set_high(&mut self) {
        self.pin.block().bsrr().write(|v| v.0 = 1 << self.pin.pin());
    }

    /// Atomically set this pin's output latch low.
    pub fn set_low(&mut self) {
        self.pin.block().brr().write(|v| v.0 = 1 << self.pin.pin());
    }

    /// Atomically set this pin's output latch.
    pub fn set_level(&mut self, level: Level) {
        match level {
            Level::Low => self.set_low(),
            Level::High => self.set_high(),
        }
    }

    /// Atomically toggle this pin's output latch using the TOG register.
    pub fn toggle(&mut self) {
        self.pin.block().tog().write(|v| v.0 = 1 << self.pin.pin());
    }
}

impl Drop for Flex<'_> {
    fn drop(&mut self) {
        self.set_as_disconnected();
    }
}

/// GPIO input. Dropping it disconnects the pin.
#[derive(Debug)]
pub struct Input<'d> {
    pin: Flex<'d>,
}

impl<'d> Input<'d> {
    /// Create an input with the requested pull configuration.
    ///
    /// On L012, Pull::Down is supported only on PF3; other pins panic.
    pub fn new(pin: Peri<'d, impl Pin>, pull: Pull) -> Self {
        let mut pin = Flex::new(pin);
        pin.set_as_input(pull);
        Self { pin }
    }

    /// Read the pin's input level.
    pub fn level(&self) -> Level {
        self.pin.level()
    }
    /// Whether the input is high.
    pub fn is_high(&self) -> bool {
        self.pin.is_high()
    }
    /// Whether the input is low.
    pub fn is_low(&self) -> bool {
        self.pin.is_low()
    }
}

/// Push-pull GPIO output. Dropping it disconnects the pin.
#[derive(Debug)]
pub struct Output<'d> {
    pin: Flex<'d>,
}

impl<'d> Output<'d> {
    /// Set the output latch before enabling the output driver.
    pub fn new(pin: Peri<'d, impl Pin>, initial_output: Level, speed: Speed) -> Self {
        let mut pin = Flex::new(pin);
        pin.set_level(initial_output);
        pin.set_as_output(speed);
        Self { pin }
    }
}

/// Open-drain GPIO output with input readback.
///
/// A high output releases the line. Dropping the driver disconnects the pin.
#[derive(Debug)]
pub struct OutputOpenDrain<'d> {
    pin: Flex<'d>,
}

impl<'d> OutputOpenDrain<'d> {
    /// Set the output latch before enabling open-drain output; disable weak pulls.
    pub fn new(pin: Peri<'d, impl Pin>, initial_output: Level, speed: Speed) -> Self {
        Self::new_pull(pin, initial_output, speed, Pull::None)
    }

    /// Set the output latch before enabling open-drain output and a weak pull.
    ///
    /// On L012, Pull::Down is supported only on PF3; other pins panic.
    pub fn new_pull(
        pin: Peri<'d, impl Pin>,
        initial_output: Level,
        speed: Speed,
        pull: Pull,
    ) -> Self {
        let mut pin = Flex::new(pin);
        pin.set_level(initial_output);
        pin.set_as_input_output_pull(speed, pull);
        Self { pin }
    }

    /// Read the actual line level, independent of the output latch.
    pub fn level(&self) -> Level {
        self.pin.level()
    }
    /// Whether the line is high.
    pub fn is_high(&self) -> bool {
        self.pin.is_high()
    }
    /// Whether the line is low.
    pub fn is_low(&self) -> bool {
        self.pin.is_low()
    }
}

macro_rules! output_methods {
    ($driver:ident) => {
        impl $driver<'_> {
            /// Atomically set the output latch high.
            pub fn set_high(&mut self) {
                self.pin.set_high();
            }
            /// Atomically set the output latch low.
            pub fn set_low(&mut self) {
                self.pin.set_low();
            }
            /// Atomically set the output latch to the requested level.
            pub fn set_level(&mut self, level: Level) {
                self.pin.set_level(level);
            }
            /// Read the output latch.
            pub fn output_level(&self) -> Level {
                self.pin.output_level()
            }
            /// Whether the output latch is high.
            pub fn is_set_high(&self) -> bool {
                self.pin.is_set_high()
            }
            /// Whether the output latch is low.
            pub fn is_set_low(&self) -> bool {
                self.pin.is_set_low()
            }
            /// Atomically toggle this output latch.
            pub fn toggle(&mut self) {
                self.pin.toggle();
            }
        }
    };
}
output_methods!(Output);
output_methods!(OutputOpenDrain);

macro_rules! impl_error_type {
    ($($driver:ident),+ $(,)?) => { $(
        impl embedded_hal::digital::ErrorType for $driver<'_> {
            type Error = Infallible;
        }
    )+ };
}
impl_error_type!(Input, Output, OutputOpenDrain, Flex);

macro_rules! impl_input {
    ($($driver:ident),+ $(,)?) => { $(
        impl embedded_hal::digital::InputPin for $driver<'_> {
            fn is_high(&mut self) -> Result<bool, Self::Error> { Ok($driver::is_high(self)) }
            fn is_low(&mut self) -> Result<bool, Self::Error> { Ok($driver::is_low(self)) }
        }
    )+ };
}
impl_input!(Input, OutputOpenDrain, Flex);

macro_rules! impl_output {
    ($($driver:ident),+ $(,)?) => { $(
        impl embedded_hal::digital::OutputPin for $driver<'_> {
            fn set_high(&mut self) -> Result<(), Self::Error> {
                $driver::set_high(self);
                Ok(())
            }
            fn set_low(&mut self) -> Result<(), Self::Error> {
                $driver::set_low(self);
                Ok(())
            }
        }
        impl embedded_hal::digital::StatefulOutputPin for $driver<'_> {
            fn is_set_high(&mut self) -> Result<bool, Self::Error> { Ok($driver::is_set_high(self)) }
            fn is_set_low(&mut self) -> Result<bool, Self::Error> { Ok($driver::is_set_low(self)) }
            fn toggle(&mut self) -> Result<(), Self::Error> {
                $driver::toggle(self);
                Ok(())
            }
        }
    )+ };
}
impl_output!(Output, OutputOpenDrain, Flex);

// Shared GPIO mode sequences. All configuration RMW operations require the
// caller's critical-section token; latch commands above use atomic registers.
impl AnyPin {
    fn check_pull(&self, _pull: Pull) {
        #[cfg(gpio_cw32l012_v1)]
        assert!(
            _pull != Pull::Down || self.has_pull_down(),
            "CW32L012 has an internal pull-down only on PF3"
        );
    }

    #[cfg(not(any(gpio_cw32l010_v1, gpio_cw32l011_v1)))]
    fn has_pull_down(&self) -> bool {
        crate::GPIO_PULL_DOWN_MASKS[self.port() as usize] & (1 << self.pin()) != 0
    }

    fn disconnect(&self, _cs: CriticalSection<'_>) {
        let r = self.block();
        let mask = 1 << self.pin();
        // Disable the output driver before changing the pad's other controls.
        r.dir().modify(|v| v.0 |= mask);
        r.analog().modify(|v| v.0 |= mask);
        r.pur().modify(|v| v.0 &= !mask);
        #[cfg(not(any(gpio_cw32l010_v1, gpio_cw32l011_v1)))]
        if self.has_pull_down() {
            r.pdr().modify(|v| v.0 &= !mask);
        }
        r.riseie().modify(|v| v.0 &= !mask);
        r.fallie().modify(|v| v.0 &= !mask);
        #[cfg(gpio_irq_level)]
        {
            r.highie().modify(|v| v.0 &= !mask);
            r.lowie().modify(|v| v.0 &= !mask);
        }
        r.opendrain().modify(|v| v.0 &= !mask);
    }

    fn set_pull(&self, pull: Pull, _cs: CriticalSection<'_>) {
        let r = self.block();
        let mask = 1 << self.pin();
        // Disable both first; never enable opposing pulls simultaneously.
        r.pur().modify(|v| v.0 &= !mask);
        #[cfg(not(any(gpio_cw32l010_v1, gpio_cw32l011_v1)))]
        if self.has_pull_down() {
            r.pdr().modify(|v| v.0 &= !mask);
        }
        match pull {
            Pull::None => {}
            Pull::Up => r.pur().modify(|v| v.0 |= mask),
            #[cfg(not(any(gpio_cw32l010_v1, gpio_cw32l011_v1)))]
            Pull::Down => r.pdr().modify(|v| v.0 |= mask),
        }
    }

    fn set_af_number(&self, af: u8, _cs: CriticalSection<'_>) {
        let r = self.block();
        let pin = self.pin();
        let shift = (pin % 8) * 4;
        // Three-bit IPs preserve the reserved high bit of each AF nibble.
        let clear = crate::GPIO_AF_MASK << shift;
        let value = u32::from(af) << shift;
        if pin < 8 {
            r.afrl().modify(|v| v.0 = (v.0 & !clear) | value);
        } else {
            #[cfg(not(gpio_cw32f002_v1))]
            r.afrh().modify(|v| v.0 = (v.0 & !clear) | value);
            #[cfg(gpio_cw32f002_v1)]
            unreachable!("F002/F003 have no high GPIO pins or AFRH register");
        }
    }

    fn set_output(&self, speed: Speed, open_drain: bool, pull: Pull, cs: CriticalSection<'_>) {
        self.check_pull(pull);
        self.disconnect(cs);
        self.set_af_number(0, cs);
        self.set_pull(pull, cs);
        self.set_output_type(speed, true, open_drain, cs);
    }

    #[cfg(gpio_af)]
    fn set_alternate(
        &self,
        af: u8,
        output: bool,
        open_drain: bool,
        pull: Pull,
        cs: CriticalSection<'_>,
    ) {
        assert!(
            u32::from(af) <= crate::GPIO_AF_MAX,
            "undocumented alternate selector"
        );
        self.check_pull(pull);
        self.disconnect(cs);
        self.set_pull(pull, cs);
        self.set_af_number(af, cs);
        #[cfg(gpio_v1)]
        let speed = Speed::High;
        #[cfg(not(gpio_v1))]
        let speed = Speed::Default;
        self.set_output_type(speed, output, open_drain, cs);
    }

    fn set_output_type(
        &self,
        speed: Speed,
        output: bool,
        open_drain: bool,
        _cs: CriticalSection<'_>,
    ) {
        let r = self.block();
        let mask = 1 << self.pin();
        r.opendrain()
            .modify(|v| v.0 = set_bit(v.0, mask, open_drain));
        #[cfg(gpio_v1)]
        r.speed()
            .modify(|v| v.0 = set_bit(v.0, mask, speed == Speed::High));
        #[cfg(not(gpio_v1))]
        let Speed::Default = speed;
        r.analog().modify(|v| v.0 &= !mask);
        // Enable output last, after the caller sets the latch and AF/type.
        r.dir().modify(|v| v.0 = set_bit(v.0, mask, !output));
    }
}

const fn set_bit(value: u32, mask: u32, set: bool) -> u32 {
    if set { value | mask } else { value & !mask }
}

const fn valid_pin(pin_port: PinNumber) -> bool {
    let port = (pin_port / 16) as usize;
    port < crate::GPIO_PIN_MASKS.len() && crate::GPIO_PIN_MASKS[port] & (1 << (pin_port % 16)) != 0
}
