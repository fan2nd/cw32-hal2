//! Interrupt-driven GPIO input waits with exclusive per-pin ownership.
//!
//! Bind the pin's GPIO interrupt group to [`InterruptHandler`] using
//! [`crate::bind_interrupts!`]. Banks C/D share a vector on L052/L083, and
//! banks E/F share a vector on L083. Waits on every pin and bank are independent.
//! The handler checks software active state before accessing each bank's MMIO.
//!
//! Level waits use native high/low enables when available; edge-only hardware
//! arms the matching edge and checks the input after arming. A captured pulse
//! can finish a level wait even if the level changes before the task runs.
//! Edge waits never complete solely because of the current input level. These
//! are one-shot waits, not edge counters: multiple events can coalesce.
//!
//! GPIO configuration and working clocks must remain enabled while a driver
//! exists. Inherited per-pin filtering and the shared filter clock are retained;
//! the board must keep that source running or explicitly configure/disable the
//! pin's filter. No deep-sleep clock management is provided.
//!
//! The installed vector must service every enabled source, including sources
//! left by a bootloader or owned by RF/raw PAC code. Such owners must use
//! disjoint pins and preserve HAL clocks, enables, flags and shared filter
//! settings. Unowned pending bits are preserved and can cause an interrupt
//! storm if their owner does not service them. Typed bindings do not establish
//! hardware ownership against a bootloader, debugger or unsafe PAC access.
//! Constructors and drop never unpend/disable a shared NVIC line, change its
//! priority, reset a bank or remap SWD/reset/oscillator pins.
//!
//! W0C ICR commands use documented no-op field masks, not reset values, package
//! or safe-pin masks, so input-only BOOT/reset and RF-owned flags remain untouched. See
//! `docs/gpio-async-expansion.md` for the source policy and validation boundary.

use core::convert::Infallible;
use core::marker::PhantomData;

use crate::Peri;
use crate::gpio::{Input, Level, Pin, Pull};
use crate::interrupt::typelevel::{Binding, Handler, Interrupt};

mod engine;
mod irq_groups;

use engine::{WaitFuture, cancel, state};
pub use irq_groups::{GpioInterrupt, PortInterrupt};

/// A typed GPIO pin whose interrupt vector is known at compile time.
///
/// Type erasure to [`crate::gpio::AnyPin`] intentionally loses this trait.
pub trait ExtiPin: Pin {
    /// This pin's GPIO interrupt group, obtained from its reviewed GLOBAL link.
    type Interrupt: GpioInterrupt;
}

/// Handler for a GPIO interrupt group, possibly containing multiple banks.
///
/// For example, bind `GPIOA` to `InterruptHandler<interrupt::typelevel::GPIOA>`.
/// On L052/L083 bind C and D pins to `InterruptHandler<GPIOC_GPIOD>`.
/// Every enabled foreign source requires its own handler on the same vector;
/// `bind_interrupts!` accepts multiple handlers for this purpose.
pub struct InterruptHandler<I: GpioInterrupt>(PhantomData<I>);

impl<I: GpioInterrupt> Handler<I> for InterruptHandler<I> {
    unsafe fn on_interrupt() {
        engine::dispatch(I::BANKS, |bank| {
            engine::on_interrupt(bank, state(bank));
        });
    }
}

/// GPIO input with interrupt-driven asynchronous waits.
///
/// Binding the wrong GPIO interrupt group is rejected at compile time. The port
/// IRQ remains enabled when this driver is dropped because other pins may share
/// it. The pin itself is disconnected by the underlying GPIO input's destructor.
#[derive(Debug)]
pub struct ExtiInput<'d> {
    pin: Input<'d>,
    pin_port: u8,
}

impl<'d> ExtiInput<'d> {
    /// Configure an input and enable its correctly-bound GPIO-port interrupt.
    pub fn new<T: ExtiPin>(
        pin: Peri<'d, T>,
        pull: Pull,
        _irq: impl Binding<T::Interrupt, InterruptHandler<T::Interrupt>>,
    ) -> Self {
        let pin_port = pin.port() * 16 + pin.pin();
        let pin = Input::new(pin, pull);
        cancel(pin_port / 16, state(pin_port / 16), pin_port % 16);
        // Do not unpend or disable a shared port vector: another pin may be
        // waiting already. The binding proves our handler is installed.
        unsafe { T::Interrupt::enable() };
        Self { pin, pin_port }
    }

    /// Read the input level.
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

    /// Wait until the input is high, returning immediately when already high.
    pub async fn wait_for_high(&mut self) {
        self.wait(Trigger::High).await;
    }

    /// Wait until the input is low, returning immediately when already low.
    pub async fn wait_for_low(&mut self) {
        self.wait(Trigger::Low).await;
    }

    /// Wait for the next low-to-high transition after the future is first polled.
    pub async fn wait_for_rising_edge(&mut self) {
        self.wait(Trigger::Rising).await;
    }

    /// Wait for the next high-to-low transition after the future is first polled.
    pub async fn wait_for_falling_edge(&mut self) {
        self.wait(Trigger::Falling).await;
    }

    /// Wait for either edge after the future is first polled.
    pub async fn wait_for_any_edge(&mut self) {
        self.wait(Trigger::AnyEdge).await;
    }

    fn wait(&mut self, trigger: Trigger) -> WaitFuture<'_> {
        let port = self.pin_port / 16;
        WaitFuture::new(port, state(port), self.pin_port % 16, trigger)
    }
}

impl embedded_hal::digital::ErrorType for ExtiInput<'_> {
    type Error = Infallible;
}

impl Drop for ExtiInput<'_> {
    fn drop(&mut self) {
        let port = self.pin_port / 16;
        cancel(port, state(port), self.pin_port % 16);
    }
}

impl embedded_hal::digital::InputPin for ExtiInput<'_> {
    fn is_high(&mut self) -> Result<bool, Self::Error> {
        Ok(ExtiInput::is_high(self))
    }
    fn is_low(&mut self) -> Result<bool, Self::Error> {
        Ok(ExtiInput::is_low(self))
    }
}

impl embedded_hal_async::digital::Wait for ExtiInput<'_> {
    async fn wait_for_high(&mut self) -> Result<(), Self::Error> {
        ExtiInput::wait_for_high(self).await;
        Ok(())
    }
    async fn wait_for_low(&mut self) -> Result<(), Self::Error> {
        ExtiInput::wait_for_low(self).await;
        Ok(())
    }
    async fn wait_for_rising_edge(&mut self) -> Result<(), Self::Error> {
        ExtiInput::wait_for_rising_edge(self).await;
        Ok(())
    }
    async fn wait_for_falling_edge(&mut self) -> Result<(), Self::Error> {
        ExtiInput::wait_for_falling_edge(self).await;
        Ok(())
    }
    async fn wait_for_any_edge(&mut self) -> Result<(), Self::Error> {
        ExtiInput::wait_for_any_edge(self).await;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Trigger {
    Rising,
    Falling,
    AnyEdge,
    High,
    Low,
}
