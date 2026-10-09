//! UART with Embassy-style owned pins, typed interrupts and split halves.
//!
//! Both blocking and interrupt-driven async transfers are implemented. Async
//! transfers normally use one interrupt/wakeup per byte. On F030/A030/L083,
//! `UartTx::new_with_dma`, `UartRx::new_with_dma` and `Uart::new_with_dma`
//! own static staging for ordinary borrowed writes and reads. RX DMA receives
//! finite explicitly armed chunks; reception/copy/rearm gaps can lose frames.
//! Without DMA there is no software RX buffer: service each byte promptly. L010/L011/L012
//! expose noise and overrun errors; other supported UARTs have no overrun flag
//! and can silently overwrite an unread frame. No throughput guarantee follows.
//! Hardware RTS/CTS is available through owned, package-checked flow-control pins.
//! Continuous/ring RX DMA, idle-line receive, synchronous mode, LSI/deep-sleep UART,
//! and custom nine-bit data are not exposed.
//!
//! Register semantics follow each family's official manual and SDK. See
//! `docs/remaining-uart-implementation.md`, `docs/f020-serial-compatibility.md`
//! and `docs/l031-r031-w031-uart.md`. ICR is R1W0 with version-specific reset
//! and clearable bits. Flush always waits for TXBUSY (buffer plus shifter) to
//! become zero, using TC only to wake. Optional LIN, automatic baud detection,
//! RS485 direction, loopback and UART timers remain disabled.
//!
//! L083 vectors are shared in pairs. Async construction requires the matching
//! `SharedInterruptHandler<interrupt::typelevel::UART1_UART4>` (or 2/5, 3/6).
//! It guards each instance before MMIO. Constructing or dropping either owner
//! never disables or unpends its partner's NVIC line; the vector remains enabled
//! after first use. A HAL-torn-down instance has masked IER and an inactive guard.
//! An unowned bootloader/external partner is skipped without MMIO, so its owner
//! must quiesce or service that source before the HAL enables the shared vector.
//! This driver never resets or clears another owner's UART to establish that.

use core::cell::Cell;
use core::future::Future;
use core::marker::PhantomData;
use core::pin::Pin as FuturePin;
use core::task::{Context, Poll};

use embassy_sync::waitqueue::AtomicWaker;

use crate::gpio::{AnyPin, Flex, Pin, Pull};
use crate::interrupt::typelevel::{Binding, Handler, Interrupt};
use crate::mode::{Async, Blocking, Mode};
use crate::{Peri, PeripheralType, pac};

use pac::uart::{regs, vals};

/// Eight data bits are supported with no, even, or odd parity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Parity {
    /// Eight data bits without a parity bit.
    ParityNone,
    /// Eight data bits followed by even parity.
    ParityEven,
    /// Eight data bits followed by odd parity.
    ParityOdd,
}

/// Stop-bit length supported by the hardware.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum StopBits {
    /// One stop bit.
    STOP1,
    /// One and a half stop bits.
    STOP1P5,
    /// Two stop bits.
    STOP2,
}

/// Receiver oversampling. Lower factors permit higher baud rates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Oversampling {
    /// Sixteen samples, with a four-bit fractional divider.
    Oversampling16,
    /// Eight samples, with an integer divider.
    Oversampling8,
    /// Four samples, with an integer divider.
    Oversampling4,
}

/// UART configuration. The clock source is the PCLK frozen by HAL initialization.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct Config {
    /// Requested baud rate in bits per second.
    pub baudrate: u32,
    /// Parity bit configuration.
    pub parity: Parity,
    /// Stop-bit length.
    pub stop_bits: StopBits,
    /// Sampling mode.
    pub oversampling: Oversampling,
    /// Maximum divider-rounding error in parts per million, at most 1,000,000.
    /// This excludes the physical oscillator's frequency tolerance.
    pub max_error_ppm: u32,
    /// RX weak pull resistor. The manual recommends pull-up for idle-high UART.
    pub rx_pull: Pull,
    /// CTS weak pull resistor. CTS is active low; pull-up pauses an undriven peer.
    /// Used only by constructors that take a CTS pin.
    pub cts_pull: Pull,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            baudrate: 115_200,
            parity: Parity::ParityNone,
            stop_bits: StopBits::STOP1,
            oversampling: Oversampling::Oversampling16,
            max_error_ppm: 20_000,
            rx_pull: Pull::Up,
            cts_pull: Pull::None,
        }
    }
}

/// Configuration was rejected before the UART or pins were changed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ConfigError {
    /// Successful HAL clock initialization is required first.
    ClockNotConfigured,
    /// A zero baud rate is invalid.
    BaudrateZero,
    /// The selected sampling mode requires a divider below its minimum.
    BaudrateTooHigh,
    /// The baud-rate divider exceeds its sixteen-bit integer part.
    BaudrateTooLow,
    /// Divider rounding exceeds the configured tolerance.
    BaudrateError,
    /// `max_error_ppm` must not exceed 1,000,000.
    InvalidTolerance,
    /// The selected CTS pin does not support the requested pull resistor.
    UnsupportedCtsPull,
    /// DMA staging must be nonempty, bounded and wholly inside qualified SRAM.
    #[cfg(uart_dma)]
    DmaBuffer(crate::dma::ConfigError),
    /// Normal HAL startup did not admit this one-time safe DMA channel.
    #[cfg(uart_dma)]
    DmaChannel(crate::dma::CopyChannelError),
    /// Pull-down is unavailable on every reviewed L012 UART RX route.
    #[cfg(cw32l012)]
    UnsupportedRxPull,
}

/// UART receive errors and safe DMA lease errors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {
    /// Received parity did not match.
    Parity,
    /// Invalid stop bit / frame structure.
    Framing,
    /// DMA failed or its completion could not be verified; resources are retained.
    #[cfg(uart_dma)]
    Dma(crate::dma::Error),
    /// A previous staged chunk is running. An inherent async operation can settle it.
    #[cfg(uart_dma)]
    DmaBusy,
    /// A previous error permanently quarantined this direction's DMA resources.
    #[cfg(uart_dma)]
    DmaQuarantined,
    /// Noise was detected. L010/L012 do not report it at fourfold oversampling.
    #[cfg(any(uart_cw32l010_v1, uart_cw32l012_v1))]
    Noise,
    /// An unread frame was overwritten by a newer frame; receive data was lost.
    #[cfg(any(uart_cw32l010_v1, uart_cw32l012_v1))]
    Overrun,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::Parity => "UART parity error",
            Self::Framing => "UART frame error",
            #[cfg(uart_dma)]
            Self::Dma(_) => "UART DMA transfer failed",
            #[cfg(uart_dma)]
            Self::DmaBusy => "UART DMA chunk is still running",
            #[cfg(uart_dma)]
            Self::DmaQuarantined => "UART DMA resources are quarantined",

            #[cfg(any(uart_cw32l010_v1, uart_cw32l012_v1))]
            Self::Noise => "UART noise error",
            #[cfg(any(uart_cw32l010_v1, uart_cw32l012_v1))]
            Self::Overrun => "UART receive overrun",
        })
    }
}
impl core::error::Error for Error {}
impl core::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::ClockNotConfigured => "HAL clocks are not initialized",
            Self::BaudrateZero => "UART baud rate must be nonzero",
            Self::BaudrateTooHigh => "UART baud rate exceeds the sampling limit",
            Self::BaudrateTooLow => "UART baud rate requires an excessive divider",
            Self::BaudrateError => "UART baud-rate rounding exceeds the configured tolerance",
            Self::InvalidTolerance => "UART baud-rate tolerance exceeds 100 percent",
            Self::UnsupportedCtsPull => "UART CTS pin does not support the requested pull",
            #[cfg(uart_dma)]
            Self::DmaBuffer(_) => "UART DMA staging buffer is invalid",
            #[cfg(uart_dma)]
            Self::DmaChannel(_) => "UART DMA channel admission failed",
            #[cfg(cw32l012)]
            Self::UnsupportedRxPull => "UART RX pull-down is unavailable on this device",
        })
    }
}
impl core::error::Error for ConfigError {}

impl embedded_io::Error for Error {
    fn kind(&self) -> embedded_io::ErrorKind {
        match self {
            #[cfg(uart_dma)]
            Self::DmaBusy | Self::DmaQuarantined | Self::Dma(_) => embedded_io::ErrorKind::Other,
            _ => embedded_io::ErrorKind::InvalidData,
        }
    }
}

pub(crate) mod sealed {
    use super::*;
    pub(crate) trait Instance: crate::rcc::RccPeripheral {
        fn info() -> Info;
        fn state() -> &'static State;
    }
    pub(crate) trait Pin<T, Signal> {
        const AF: u8;
    }
    pub enum Tx {}
    pub enum Rx {}
    pub enum Cts {}
    pub enum Rts {}
}

/// A supported UART peripheral singleton.
#[allow(private_bounds)]
pub trait Instance: PeripheralType + sealed::Instance + 'static {
    /// Corresponding type-level interrupt.
    type Interrupt: Interrupt;
    /// Required handler. Shared vectors use a group handler that dispatches
    /// every UART on that vector, including an independently owned partner.
    type InterruptHandler: Handler<Self::Interrupt>;
}

/// A pin verified to carry this UART's TXD signal.
#[allow(private_bounds)]
pub trait TxPin<T: Instance>: Pin + sealed::Pin<T, sealed::Tx> {}
/// A pin verified to carry this UART's RXD signal.
#[allow(private_bounds)]
pub trait RxPin<T: Instance>: Pin + sealed::Pin<T, sealed::Rx> {}
/// A pin verified to carry this UART's active-low CTS input.
#[allow(private_bounds)]
pub trait CtsPin<T: Instance>: Pin + sealed::Pin<T, sealed::Cts> {}
/// A pin verified to carry this UART's active-low RTS output.
#[allow(private_bounds)]
pub trait RtsPin<T: Instance>: Pin + sealed::Pin<T, sealed::Rts> {}

macro_rules! impl_pin {
    ($pin:ident, $uart:ident, TxPin, $af:expr) => {
        impl $crate::usart::sealed::Pin<$crate::peripherals::$uart, $crate::usart::sealed::Tx>
            for $crate::peripherals::$pin
        {
            const AF: u8 = $af;
        }
        impl $crate::usart::TxPin<$crate::peripherals::$uart> for $crate::peripherals::$pin {}
    };
    ($pin:ident, $uart:ident, RxPin, $af:expr) => {
        impl $crate::usart::sealed::Pin<$crate::peripherals::$uart, $crate::usart::sealed::Rx>
            for $crate::peripherals::$pin
        {
            const AF: u8 = $af;
        }
        impl $crate::usart::RxPin<$crate::peripherals::$uart> for $crate::peripherals::$pin {}
    };
    ($pin:ident, $uart:ident, CtsPin, $af:expr) => {
        impl $crate::usart::sealed::Pin<$crate::peripherals::$uart, $crate::usart::sealed::Cts>
            for $crate::peripherals::$pin
        {
            const AF: u8 = $af;
        }
        impl $crate::usart::CtsPin<$crate::peripherals::$uart> for $crate::peripherals::$pin {}
    };
    ($pin:ident, $uart:ident, RtsPin, $af:expr) => {
        impl $crate::usart::sealed::Pin<$crate::peripherals::$uart, $crate::usart::sealed::Rts>
            for $crate::peripherals::$pin
        {
            const AF: u8 = $af;
        }
        impl $crate::usart::RtsPin<$crate::peripherals::$uart> for $crate::peripherals::$pin {}
    };
}
pub(crate) use impl_pin;

macro_rules! impl_instance {
    ($uart:ident, $irq:ident, $index:expr) => {
        impl $crate::usart::sealed::Instance for $crate::peripherals::$uart {
            fn info() -> $crate::usart::Info {
                $crate::usart::Info {
                    regs: $crate::pac::$uart,
                    rcc: <Self as $crate::rcc::SealedRccPeripheral>::RCC_INFO,
                    #[cfg(not(uart_cw32l083_v1))]
                    disable_irq: $crate::usart::disable_interrupt::<
                        $crate::interrupt::typelevel::$irq,
                    >,
                }
            }
            fn state() -> &'static $crate::usart::State {
                static STATE: $crate::usart::State = $crate::usart::State::new();
                &STATE
            }
        }
        impl $crate::usart::Instance for $crate::peripherals::$uart {
            type Interrupt = $crate::interrupt::typelevel::$irq;
            #[cfg(not(uart_cw32l083_v1))]
            type InterruptHandler = $crate::usart::InterruptHandler<Self>;
            #[cfg(uart_cw32l083_v1)]
            type InterruptHandler = $crate::usart::SharedInterruptHandler<Self::Interrupt>;
        }
    };
}
pub(crate) use impl_instance;

#[derive(Clone, Copy)]
pub(crate) struct Info {
    pub(crate) regs: pac::uart::Uart,
    pub(crate) rcc: crate::rcc::RccInfo,
    #[cfg(not(uart_cw32l083_v1))]
    pub(crate) disable_irq: fn(),
}

impl Info {
    fn prepare<T: Instance>(self, config: Config, baud: Baud) {
        // A shared vector may already serve a live partner, even when this new
        // owner is blocking. Never disable or unpend that partner's vector.
        #[cfg(not(uart_cw32l083_v1))]
        T::Interrupt::disable();
        let state = T::state();
        critical_section::with(|cs| {
            state.lifecycle.borrow(cs).set(Lifecycle::new());
            self.rcc
                .enable_and_reset_with_cs(cs)
                .expect("UART clock gate did not acknowledge");
            configure(self.regs, config, baud);
        });
        #[cfg(not(uart_cw32l083_v1))]
        T::Interrupt::unpend();
    }

    fn shutdown(self, state: &State, direction: Direction, retain: bool) {
        critical_section::with(|cs| {
            self.regs.ier().modify(|w| match direction {
                Direction::Tx => {
                    w.set_txe(false);
                    w.set_tc(false);
                }
                Direction::Rx => WaitEvent::Receive.set_enabled(w, false),
            });
            let mut owners = state.lifecycle.borrow(cs).get();
            match direction {
                Direction::Tx => {
                    owners.tx_live = false;
                    owners.tx_retained = retain;
                }
                Direction::Rx => {
                    owners.rx_live = false;
                    owners.rx_retained = retain;
                }
            }
            if !retain {
                self.regs.cr1().modify(|w| match direction {
                    Direction::Tx => w.set_txen(false),
                    Direction::Rx => w.set_rxen(false),
                });
                self.regs.cr2().modify(|w| match direction {
                    Direction::Tx => w.set_ctsen(false),
                    Direction::Rx => w.set_rtsen(false),
                });
            }
            state.lifecycle.borrow(cs).set(owners);
            // A retained direction owns its clock and configuration forever,
            // even after its live peer drops. It never disables peer dispatch.
            if !owners.live() {
                #[cfg(not(uart_cw32l083_v1))]
                (self.disable_irq)();
                if !owners.tx_retained && !owners.rx_retained {
                    self.rcc
                        .disable_with_cs(cs)
                        .expect("UART clock gate did not acknowledge");
                }
            }
        });
    }
}

#[derive(Clone, Copy)]
struct Lifecycle {
    tx_live: bool,
    rx_live: bool,
    tx_retained: bool,
    rx_retained: bool,
}
impl Lifecycle {
    const fn new() -> Self {
        Self {
            tx_live: false,
            rx_live: false,
            tx_retained: false,
            rx_retained: false,
        }
    }
    fn live(self) -> bool {
        self.tx_live || self.rx_live
    }
}

pub(crate) struct State {
    tx_waker: AtomicWaker,
    rx_waker: AtomicWaker,
    // The same critical section covers ownership, shared register updates,
    // IRQ dispatch and final clock gating, including abandoned DMA halves.
    lifecycle: critical_section::Mutex<Cell<Lifecycle>>,
}
impl State {
    pub(crate) const fn new() -> Self {
        Self {
            tx_waker: AtomicWaker::new(),
            rx_waker: AtomicWaker::new(),
            lifecycle: critical_section::Mutex::new(Cell::new(Lifecycle::new())),
        }
    }
    fn activate(&self, direction: Direction) {
        critical_section::with(|cs| {
            let mut owners = self.lifecycle.borrow(cs).get();
            match direction {
                Direction::Tx => owners.tx_live = true,
                Direction::Rx => owners.rx_live = true,
            }
            self.lifecycle.borrow(cs).set(owners);
        });
    }
}

/// Handler for a UART with its own vector. Bind it with `bind_interrupts!`.
/// L083 constructors instead require `SharedInterruptHandler`, which dispatches
/// both instances on the shared vector. A partial binding cannot construct one.
pub struct InterruptHandler<T: Instance>(PhantomData<T>);
#[cfg(not(uart_cw32l083_v1))]
impl<T: Instance> Handler<T::Interrupt> for InterruptHandler<T> {
    unsafe fn on_interrupt() {
        dispatch_interrupt(T::info().regs, T::state());
    }
}

/// L083 shared-vector handler. The type argument is the type-level vector:
/// `SharedInterruptHandler<interrupt::typelevel::UART1_UART4>` (or
/// `UART2_UART5`, `UART3_UART6`). Bind that handler once; it safely services both
/// UARTs and is accepted by either instance's async constructor.
///
/// An unconstructed or dropped partner is skipped before any clock-gated MMIO.
/// The NVIC line stays enabled after first use, including after the last drop;
/// each HAL-owned inactive UART's local interrupts are masked and its guard is
/// false. A bootloader/external owner must quiesce an unowned partner's source
/// (or provide its own compatible handling) before enabling this vector. This
/// driver never reads, resets or clears an inactive unowned UART to do that.
#[cfg(uart_cw32l083_v1)]
pub struct SharedInterruptHandler<I: Interrupt>(PhantomData<I>);

#[cfg(uart_cw32l083_v1)]
macro_rules! group {
    ($irq:ident, $first:ident, $second:ident) => {
        impl Handler<crate::interrupt::typelevel::$irq>
            for SharedInterruptHandler<crate::interrupt::typelevel::$irq>
        {
            unsafe fn on_interrupt() {
                dispatch_group(
                    <crate::peripherals::$first as sealed::Instance>::info().regs,
                    <crate::peripherals::$first as sealed::Instance>::state(),
                    <crate::peripherals::$second as sealed::Instance>::info().regs,
                    <crate::peripherals::$second as sealed::Instance>::state(),
                );
            }
        }
    };
}
#[cfg(uart_cw32l083_v1)]
group!(UART1_UART4, UART1, UART4);
#[cfg(uart_cw32l083_v1)]
group!(UART2_UART5, UART2, UART5);
#[cfg(uart_cw32l083_v1)]
group!(UART3_UART6, UART3, UART6);

#[cfg(uart_cw32l083_v1)]
fn dispatch_group(a: pac::uart::Uart, a_state: &State, b: pac::uart::Uart, b_state: &State) {
    dispatch_interrupt(a, a_state);
    dispatch_interrupt(b, b_state);
}

/// Owned bidirectional UART. Split it to transmit and receive concurrently.
///
/// Like Embassy STM32, `new_blocking` needs no interrupt binding; `new` creates
/// an async driver and requires the type-correct binding. This no-DMA async
/// constructor intentionally takes no DMA channels.
pub struct Uart<'d, M: Mode> {
    tx: UartTx<'d, M>,
    rx: UartRx<'d, M>,
}

/// Owned UART transmitter. Dropping it can truncate queued data: flush first.
pub struct UartTx<'d, M: Mode> {
    info: Info,
    state: &'static State,
    _pin: Option<Flex<'d>>,
    _cts: Option<Flex<'d>>,
    #[cfg(uart_dma)]
    dma: Option<TxDma>,
    actual_baudrate: u32,
    _mode: PhantomData<M>,
}
/// Owned UART receiver, optionally using private finite-chunk DMA staging.
pub struct UartRx<'d, M: Mode> {
    info: Info,
    state: &'static State,
    _pin: Option<Flex<'d>>,
    _rts: Option<Flex<'d>>,
    #[cfg(uart_dma)]
    dma: Option<RxDma>,
    actual_baudrate: u32,
    _mode: PhantomData<M>,
}

impl<'d> Uart<'d, Blocking> {
    /// Create a blocking UART with active-low hardware RTS and CTS.
    /// RTS pauses the peer while an unread frame occupies the receive register.
    /// CTS can stall writes and flush indefinitely while the peer is not ready.
    pub fn new_blocking_with_rtscts<T: Instance, RTS: RtsPin<T>, CTS: CtsPin<T>>(
        peri: Peri<'d, T>,
        tx: Peri<'d, impl TxPin<T>>,
        rx: Peri<'d, impl RxPin<T>>,
        rts: Peri<'d, RTS>,
        cts: Peri<'d, CTS>,
        config: Config,
    ) -> Result<Self, ConfigError> {
        let uart = Self::new_inner(
            peri,
            tx,
            rx,
            Some((rts.into(), <RTS as sealed::Pin<T, sealed::Rts>>::AF)),
            Some((cts.into(), <CTS as sealed::Pin<T, sealed::Cts>>::AF)),
            config,
        )?;
        Ok(uart)
    }

    /// Create a blocking UART with owned, instance-checked TX and RX pins.
    pub fn new_blocking<T: Instance>(
        peri: Peri<'d, T>,
        tx: Peri<'d, impl TxPin<T>>,
        rx: Peri<'d, impl RxPin<T>>,
        config: Config,
    ) -> Result<Self, ConfigError> {
        Self::new_inner(peri, tx, rx, None, None, config)
    }
}
impl<'d> Uart<'d, Async> {
    /// Create an async UART with active-low hardware RTS and CTS.
    /// RTS pauses the peer while an unread frame occupies the receive register.
    /// CTS can stall writes and flush indefinitely while the peer is not ready.
    pub fn new_with_rtscts<T: Instance, RTS: RtsPin<T>, CTS: CtsPin<T>>(
        peri: Peri<'d, T>,
        tx: Peri<'d, impl TxPin<T>>,
        rx: Peri<'d, impl RxPin<T>>,
        rts: Peri<'d, RTS>,
        cts: Peri<'d, CTS>,
        _irq: impl Binding<T::Interrupt, T::InterruptHandler>,
        config: Config,
    ) -> Result<Self, ConfigError> {
        let uart = Self::new_inner(
            peri,
            tx,
            rx,
            Some((rts.into(), <RTS as sealed::Pin<T, sealed::Rts>>::AF)),
            Some((cts.into(), <CTS as sealed::Pin<T, sealed::Cts>>::AF)),
            config,
        )?;
        unsafe { T::Interrupt::enable() };
        Ok(uart)
    }

    /// Create an interrupt-driven UART without DMA or a software RX buffer.
    pub fn new<T: Instance>(
        peri: Peri<'d, T>,
        tx: Peri<'d, impl TxPin<T>>,
        rx: Peri<'d, impl RxPin<T>>,
        _irq: impl Binding<T::Interrupt, T::InterruptHandler>,
        config: Config,
    ) -> Result<Self, ConfigError> {
        let uart = Self::new_inner(peri, tx, rx, None, None, config)?;
        unsafe { T::Interrupt::enable() };
        Ok(uart)
    }

    /// Queue all bytes. Cancellation can leave an already-transmitted prefix.
    /// Use `flush` to wait for the final stop bit to leave the wire.
    pub async fn write(&mut self, buffer: &[u8]) -> Result<(), Error> {
        self.tx.write(buffer).await
    }
    /// Receive exactly `buffer.len()` bytes.
    /// Cancellation may leave a filled prefix. Without DMA, unread hardware
    /// data stays in place; with DMA, the active private chunk is retained for
    /// a later read. Receive/rearm/copy gaps may still lose frames. See
    /// [`UartRx::read`] for the receive and cancellation contract.
    pub async fn read(&mut self, buffer: &mut [u8]) -> Result<(), Error> {
        self.rx.read(buffer).await
    }
    /// Wait until both the transmit buffer and shift register are empty.
    pub async fn flush(&mut self) -> Result<(), Error> {
        self.tx.flush().await
    }
}

impl<'d, M: Mode> Uart<'d, M> {
    fn new_inner<T: Instance, TX: TxPin<T>, RX: RxPin<T>>(
        _peri: Peri<'d, T>,
        tx: Peri<'d, TX>,
        rx: Peri<'d, RX>,
        rts: Option<(Peri<'d, AnyPin>, u8)>,
        cts: Option<(Peri<'d, AnyPin>, u8)>,
        config: Config,
    ) -> Result<Self, ConfigError> {
        let baud = configured_baud::<T>(config)?;
        validate_cts_pull(cts.as_ref().map(|(pin, _)| &**pin), config.cts_pull)?;
        let info = T::info();
        info.prepare::<T>(config, baud);
        let mut tx = UartTx::from_pin::<T, TX>(tx, info, baud.actual);
        let mut rx = UartRx::from_pin::<T, RX>(rx, info, baud.actual, config.rx_pull);
        tx._cts = cts.map(|(pin, af)| {
            let mut pin = Flex::new(pin);
            pin.set_as_af(af, false, config.cts_pull);
            pin
        });
        rx._rts = rts.map(|(pin, af)| {
            let mut pin = Flex::new(pin);
            pin.set_as_af(af, true, Pull::None);
            pin
        });
        critical_section::with(|_| {
            info.regs.cr2().modify(|w| {
                w.set_ctsen(tx._cts.is_some());
                w.set_rtsen(rx._rts.is_some());
            });
            info.regs.cr1().modify(|w| {
                w.set_txen(true);
                w.set_rxen(true);
            });
        });
        Ok(Self { tx, rx })
    }
    /// Split into independent transmitter and receiver ownership.
    pub fn split(self) -> (UartTx<'d, M>, UartRx<'d, M>) {
        (self.tx, self.rx)
    }
    /// Borrow both halves to use concurrently without consuming the UART.
    pub fn split_ref(&mut self) -> (&mut UartTx<'d, M>, &mut UartRx<'d, M>) {
        (&mut self.tx, &mut self.rx)
    }
    /// Baud rate after divider rounding, excluding oscillator tolerance.
    pub fn actual_baudrate(&self) -> u32 {
        self.tx.actual_baudrate
    }
    /// Queue bytes using blocking status checks. Call `blocking_flush` afterward
    /// if completion on the wire is required.
    pub fn blocking_write(&mut self, buffer: &[u8]) -> Result<(), Error> {
        self.tx.blocking_write(buffer)
    }
    /// Read exactly the requested number of bytes using blocking status checks.
    pub fn blocking_read(&mut self, buffer: &mut [u8]) -> Result<(), Error> {
        self.rx.blocking_read(buffer)
    }
    fn check_read(&mut self) -> Result<(), Error> {
        self.rx.check_read()
    }
    /// Wait for all queued data to leave the wire.
    pub fn blocking_flush(&mut self) -> Result<(), Error> {
        self.tx.blocking_flush()
    }
}

// The lease belongs to the driver, never to an operation future. Only Idle
// permits staging access. A forgotten waiter leaves InFlight intact.
#[cfg(uart_dma)]
#[derive(Clone, Copy, Eq, PartialEq)]
enum TxDmaPhase {
    Idle,
    InFlight,
    Quarantined,
}

#[cfg(uart_dma)]
struct TxDma {
    channel: crate::dma::Channel<'static>,
    staging: &'static mut [u8],
    request: crate::dma::Request,
    phase: TxDmaPhase,
}

#[cfg(uart_dma)]
impl TxDma {
    fn validate_staging(staging: &[u8]) -> Result<(), crate::dma::ConfigError> {
        use crate::dma::ConfigError;
        if staging.is_empty() || staging.len() > u16::MAX as usize {
            return Err(ConfigError::InvalidLength);
        }
        let (base, size) = crate::DMA_COPY_SRAM.ok_or(ConfigError::MissingMemoryMetadata)?;
        let start = staging.as_ptr() as usize;
        let end = start
            .checked_add(staging.len())
            .ok_or(ConfigError::AddressOutOfRange)?;
        let limit = (base as usize)
            .checked_add(size as usize)
            .ok_or(ConfigError::AddressOutOfRange)?;
        if start < base as usize || end > limit {
            return Err(ConfigError::NotInSram);
        }
        Ok(())
    }

    fn poll_complete(
        &mut self,
        regs: pac::uart::Uart,
        cx: Option<&mut Context<'_>>,
    ) -> Poll<Result<(), Error>> {
        match self.phase {
            TxDmaPhase::Idle => return Poll::Ready(Ok(())),
            TxDmaPhase::Quarantined => return Poll::Ready(Err(Error::DmaQuarantined)),
            TxDmaPhase::InFlight => {}
        }
        match self.channel.poll_hardware_complete(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(error)) => {
                self.phase = TxDmaPhase::Quarantined;
                Poll::Ready(Err(Error::Dma(error)))
            }
            Poll::Ready(Ok(())) => {
                // Own manual §18.7.1.5: close the request after DMA TC. The
                // channel helper also required Complete STATUS and no TE.
                critical_section::with(|_| regs.cr2().modify(|v| v.set_dmatx(false)));
                // SAFETY: this same lease retained all resources continuously;
                // clean completion and request closure precede the fence and
                // retirement. No error path can reach Idle.
                unsafe { self.channel.retire_hardware() };
                self.phase = TxDmaPhase::Idle;
                Poll::Ready(Ok(()))
            }
        }
    }

    async fn complete(&mut self, regs: pac::uart::Uart) -> Result<(), Error> {
        core::future::poll_fn(|cx| self.poll_complete(regs, Some(cx))).await
    }

    fn check_complete(&mut self, regs: pac::uart::Uart) -> Result<(), Error> {
        match self.poll_complete(regs, None) {
            Poll::Ready(result) => result,
            Poll::Pending => Err(Error::DmaBusy),
        }
    }

    async fn write(&mut self, regs: pac::uart::Uart, buffer: &[u8]) -> Result<(), Error> {
        // Reap a cancelled/forgotten write's current chunk before touching SRAM,
        // including on an empty write. Its unscheduled remainder is discarded.
        self.complete(regs).await?;
        for chunk in buffer.chunks(self.staging.len()) {
            self.staging[..chunk.len()].copy_from_slice(chunk);
            // Install the permanent lease state before EN or any possible panic
            // after it. DMA never receives the caller's pointer.
            self.phase = TxDmaPhase::InFlight;
            // SAFETY: the typed constructor fixed the generated request and
            // consumed static UART/channel/pin/staging owners. This driver's
            // Drop retains them on any unconfirmed completion, including panic.
            let result = unsafe {
                self.channel.start_write(
                    self.request,
                    &self.staging[..chunk.len()],
                    regs.tdr().as_ptr().cast::<u8>(),
                )
            };
            if result.is_err() {
                // Validation was completed before admission. An unexpected
                // reservation/configuration failure cannot restore the lease.
                self.phase = TxDmaPhase::Quarantined;
                return Err(Error::DmaQuarantined);
            }
            // The channel is enabled before the UART request (§18.7.1.5).
            critical_section::with(|_| regs.cr2().modify(|v| v.set_dmatx(true)));
            self.complete(regs).await?;
        }
        Ok(())
    }
}

#[cfg(uart_dma)]
impl UartTx<'static, Async> {
    /// Create a safe staged DMA transmitter on F030/A030/L083 without CTS.
    ///
    /// Consumes static UART, TX pin, compatible DMA channel and SRAM staging.
    /// Short-lived peripheral reborrows are deliberately excluded. Like other
    /// UART constructors, rejected inputs are consumed; no transfer is started.
    /// The staging slice must contain 1..=65535 bytes in qualified SRAM. Each
    /// write CPU-copies bounded chunks into it; chunk boundaries may have gaps.
    ///
    /// Admission comes only from normal HAL initialization of a reset/clean
    /// runtime, as for [`crate::dma::CopyChannel`]. No raw channel can upgrade.
    /// Cancelling/forgetting a write leaves its current chunk running. A later
    /// write or flush awaits that same clean completion before reuse. Errors
    /// permanently quarantine resources; dropping an unfinished driver retains
    /// its clocks, UART configuration, pin, channel and staging. No finite
    /// completion-time or hardware-throughput guarantee is made.
    pub fn new_with_dma<T, TX, D>(
        _peri: Peri<'static, T>,
        tx: Peri<'static, TX>,
        channel: Peri<'static, D>,
        irq: impl Binding<T::Interrupt, T::InterruptHandler>
        + Binding<D::Interrupt, crate::dma::InterruptHandler<D>>
        + 'static,
        staging: &'static mut [u8],
        config: Config,
    ) -> Result<Self, ConfigError>
    where
        T: Instance,
        TX: TxPin<T>,
        D: crate::dma::RequestRoute<T, crate::dma::signal::TX>,
    {
        let baud = configured_baud::<T>(config)?;
        TxDma::validate_staging(staging).map_err(ConfigError::DmaBuffer)?;
        let channel = crate::dma::Channel::new_admitted(channel, irq)
            .map_err(|error| ConfigError::DmaChannel(error.error))?;
        let info = T::info();
        info.prepare::<T>(config, baud);
        let mut tx = Self::from_pin::<T, TX>(tx, info, baud.actual);
        tx.dma = Some(TxDma {
            channel,
            staging,
            request: D::REQUEST,
            phase: TxDmaPhase::Idle,
        });
        critical_section::with(|_| info.regs.cr1().modify(|v| v.set_txen(true)));
        unsafe { T::Interrupt::enable() };
        Ok(tx)
    }
}

impl<'d> UartTx<'d, Blocking> {
    /// Create a blocking transmit-only UART with active-low hardware CTS.
    pub fn new_blocking_with_cts<T: Instance, TX: TxPin<T>, CTS: CtsPin<T>>(
        _peri: Peri<'d, T>,
        tx: Peri<'d, TX>,
        cts: Peri<'d, CTS>,
        config: Config,
    ) -> Result<Self, ConfigError> {
        let tx = Self::new_inner::<T, TX>(
            tx,
            Some((cts.into(), <CTS as sealed::Pin<T, sealed::Cts>>::AF)),
            config,
        )?;
        Ok(tx)
    }

    /// Create a blocking transmit-only UART.
    pub fn new_blocking<T: Instance, TX: TxPin<T>>(
        _peri: Peri<'d, T>,
        tx: Peri<'d, TX>,
        config: Config,
    ) -> Result<Self, ConfigError> {
        Self::new_inner::<T, TX>(tx, None, config)
    }
}
impl<'d> UartTx<'d, Async> {
    /// Create an async transmit-only UART with active-low hardware CTS.
    pub fn new_with_cts<T: Instance, TX: TxPin<T>, CTS: CtsPin<T>>(
        _peri: Peri<'d, T>,
        tx: Peri<'d, TX>,
        cts: Peri<'d, CTS>,
        _irq: impl Binding<T::Interrupt, T::InterruptHandler>,
        config: Config,
    ) -> Result<Self, ConfigError> {
        let tx = Self::new_inner::<T, TX>(
            tx,
            Some((cts.into(), <CTS as sealed::Pin<T, sealed::Cts>>::AF)),
            config,
        )?;
        unsafe { T::Interrupt::enable() };
        Ok(tx)
    }

    /// Create an interrupt-driven transmit-only UART.
    pub fn new<T: Instance, TX: TxPin<T>>(
        _peri: Peri<'d, T>,
        tx: Peri<'d, TX>,
        _irq: impl Binding<T::Interrupt, T::InterruptHandler>,
        config: Config,
    ) -> Result<Self, ConfigError> {
        let tx = Self::new_inner::<T, TX>(tx, None, config)?;
        unsafe { T::Interrupt::enable() };
        Ok(tx)
    }
    /// Queue bytes, using private staging if constructed with DMA.
    /// Cancellation can leave a transmitted prefix and a running staged chunk.
    /// A later write/flush first waits for that chunk's clean completion.
    pub async fn write(&mut self, buffer: &[u8]) -> Result<(), Error> {
        #[cfg(uart_dma)]
        if let Some(dma) = self.dma.as_mut() {
            return dma.write(self.info.regs, buffer).await;
        }
        for &byte in buffer {
            Wait::new(self.info.regs, &self.state.tx_waker, WaitEvent::TxReady).await;
            transmit(self.info.regs, byte);
        }
        Ok(())
    }
    /// Wait until the last queued byte has finished transmission.
    pub async fn flush(&mut self) -> Result<(), Error> {
        #[cfg(uart_dma)]
        if let Some(dma) = self.dma.as_mut() {
            dma.complete(self.info.regs).await?;
        }
        Wait::new(self.info.regs, &self.state.tx_waker, WaitEvent::TxComplete).await;
        Ok(())
    }
}
impl<'d, M: Mode> UartTx<'d, M> {
    fn from_pin<T: Instance, TX: TxPin<T>>(pin: Peri<'d, TX>, info: Info, actual: u32) -> Self {
        let mut pin = Flex::new(pin);
        pin.set_as_af(<TX as sealed::Pin<T, sealed::Tx>>::AF, true, Pull::None);
        T::state().activate(Direction::Tx);
        Self {
            info,
            state: T::state(),
            _pin: Some(pin),
            _cts: None,
            #[cfg(uart_dma)]
            dma: None,
            actual_baudrate: actual,
            _mode: PhantomData,
        }
    }

    fn new_inner<T: Instance, TX: TxPin<T>>(
        tx: Peri<'d, TX>,
        cts: Option<(Peri<'d, AnyPin>, u8)>,
        config: Config,
    ) -> Result<Self, ConfigError> {
        let baud = configured_baud::<T>(config)?;
        validate_cts_pull(cts.as_ref().map(|(pin, _)| &**pin), config.cts_pull)?;
        let info = T::info();
        info.prepare::<T>(config, baud);
        let mut tx = Self::from_pin::<T, TX>(tx, info, baud.actual);
        tx._cts = cts.map(|(pin, af)| {
            let mut pin = Flex::new(pin);
            pin.set_as_af(af, false, config.cts_pull);
            pin
        });
        critical_section::with(|_| info.regs.cr2().modify(|w| w.set_ctsen(tx._cts.is_some())));
        critical_section::with(|_| info.regs.cr1().modify(|v| v.set_txen(true)));
        Ok(tx)
    }
    /// Baud rate after divider rounding.
    pub fn actual_baudrate(&self) -> u32 {
        self.actual_baudrate
    }
    /// Queue bytes with CPU writes and blocking status checks.
    /// A DMA owner first reaps clean completion, or returns DmaBusy/quarantine.
    pub fn blocking_write(&mut self, buffer: &[u8]) -> Result<(), Error> {
        #[cfg(uart_dma)]
        if let Some(dma) = self.dma.as_mut() {
            dma.check_complete(self.info.regs)?;
        }
        for &byte in buffer {
            while !self.info.regs.isr().read().txe() {
                core::hint::spin_loop();
            }
            transmit(self.info.regs, byte);
        }
        Ok(())
    }
    /// Wait for the transmit buffer and shift register to empty.
    /// A DMA owner first reaps clean completion, or returns DmaBusy/quarantine.
    pub fn blocking_flush(&mut self) -> Result<(), Error> {
        #[cfg(uart_dma)]
        if let Some(dma) = self.dma.as_mut() {
            dma.check_complete(self.info.regs)?;
        }
        while self.info.regs.isr().read().txbusy() {
            core::hint::spin_loop();
        }
        Ok(())
    }
}

// RX staging is never reborrowed while DMA may write it. The exclusive static
// reference is consumed once; only a positively retired Ready chunk is copied.
#[cfg(uart_dma)]
#[derive(Clone, Copy)]
enum RxDmaPhase {
    Idle,
    InFlight {
        len: usize,
        line_error: Option<Error>,
        error_reported: bool,
    },
    Ready {
        len: usize,
        offset: usize,
    },
    Quarantined,
}

#[cfg(uart_dma)]
struct RxDma {
    channel: crate::dma::Channel<'static>,
    staging: *mut u8,
    capacity: usize,
    _staging: PhantomData<&'static mut [u8]>,
    request: crate::dma::Request,
    phase: RxDmaPhase,
}

#[cfg(uart_dma)]
impl RxDma {
    fn new(
        channel: crate::dma::Channel<'static>,
        staging: &'static mut [u8],
        request: crate::dma::Request,
    ) -> Self {
        let capacity = staging.len();
        let staging = staging.as_mut_ptr();
        Self {
            channel,
            staging,
            capacity,
            _staging: PhantomData,
            request,
            phase: RxDmaPhase::Idle,
        }
    }

    fn line_error(status: regs::Isr) -> Option<Error> {
        if status.pe() {
            Some(Error::Parity)
        } else if status.fe() {
            Some(Error::Framing)
        } else {
            None
        }
    }

    fn merge_error(previous: Option<Error>, observed: Option<Error>) -> Option<Error> {
        match (previous, observed) {
            (Some(Error::Parity), _) | (_, Some(Error::Parity)) => Some(Error::Parity),
            (Some(error), _) | (_, Some(error)) => Some(error),
            _ => None,
        }
    }

    // RC is software-cleared, even when DMA has already read RDR. Never treat
    // stale RC as an extra byte or read RDR on a DMA completion/error path.
    fn acknowledge(regs: pac::uart::Uart, status: regs::Isr) {
        let mut command = regs::Icr::write_noop();
        command.set_rc(!status.rc());
        command.set_pe(!status.pe());
        command.set_fe(!status.fe());
        regs.icr().write_value(command);
    }

    fn poll_complete(
        &mut self,
        regs: pac::uart::Uart,
        state: &State,
        mut cx: Option<&mut Context<'_>>,
    ) -> Poll<Result<(), Error>> {
        let (len, mut line_error, mut error_reported) = match self.phase {
            RxDmaPhase::Idle | RxDmaPhase::Ready { .. } => return Poll::Ready(Ok(())),
            RxDmaPhase::Quarantined => return Poll::Ready(Err(Error::DmaQuarantined)),
            RxDmaPhase::InFlight {
                len,
                line_error,
                error_reported,
            } => (len, line_error, error_reported),
        };
        if let Some(cx) = cx.as_mut() {
            state.rx_waker.register(cx.waker());
        }
        critical_section::with(|_| {
            // Register both wakers before final observations. The nonblocking
            // second observation services this channel's flags even if the
            // caller has masked CPU interrupts, and makes observed TE win.
            if let Some(cx) = cx.as_deref_mut() {
                let _ = self.channel.poll_hardware_complete(Some(cx));
            }
            if cx.is_some() && !error_reported {
                regs.ier()
                    .modify(|w| WaitEvent::ReceiveErrors.set_enabled(w, true));
            }
            line_error = Self::merge_error(line_error, Self::line_error(regs.isr().read()));
            match self.channel.poll_hardware_complete(None) {
                Poll::Ready(Err(error)) => {
                    regs.ier()
                        .modify(|w| WaitEvent::Receive.set_enabled(w, false));
                    self.phase = RxDmaPhase::Quarantined;
                    Poll::Ready(Err(Error::Dma(error)))
                }
                Poll::Ready(Ok(())) => {
                    // Own manual p350: EN before DMARX, then close DMARX only
                    // after clean DMA TC. A UART error alone is never a drain.
                    regs.cr2().modify(|w| w.set_dmarx(false));
                    let status = regs.isr().read();
                    line_error = Self::merge_error(line_error, Self::line_error(status));
                    regs.ier()
                        .modify(|w| WaitEvent::Receive.set_enabled(w, false));
                    Self::acknowledge(regs, status);
                    // SAFETY: this persistent lease retained the destination,
                    // endpoint and clocks; clean completion and request closure
                    // now precede the DMA fence and retirement.
                    unsafe { self.channel.retire_hardware() };
                    self.phase = if line_error.is_some() {
                        RxDmaPhase::Idle
                    } else {
                        RxDmaPhase::Ready { len, offset: 0 }
                    };
                    if !error_reported {
                        if let Some(error) = line_error {
                            return Poll::Ready(Err(error));
                        }
                    }
                    Poll::Ready(Ok(()))
                }
                Poll::Pending => {
                    let report = if !error_reported { line_error } else { None };
                    if report.is_some() {
                        error_reported = true;
                        regs.ier()
                            .modify(|w| WaitEvent::Receive.set_enabled(w, false));
                    }
                    self.phase = RxDmaPhase::InFlight {
                        len,
                        line_error,
                        error_reported,
                    };
                    match report {
                        Some(error) => Poll::Ready(Err(error)),
                        None => Poll::Pending,
                    }
                }
            }
        })
    }

    fn check_complete(&mut self, regs: pac::uart::Uart, state: &State) -> Result<(), Error> {
        match self.poll_complete(regs, state, None) {
            Poll::Ready(result) => result,
            Poll::Pending => Err(Error::DmaBusy),
        }
    }

    async fn complete(&mut self, regs: pac::uart::Uart, state: &State) -> Result<(), Error> {
        // Only waiter masking uses Drop. The DMA lease and all error state
        // remain valid even if this guard and the whole future are forgotten.
        let _waiter = Wait::new(regs, &state.rx_waker, WaitEvent::ReceiveErrors);
        core::future::poll_fn(|cx| self.poll_complete(regs, state, Some(cx))).await
    }

    fn copy_ready(&mut self, buffer: &mut [u8]) -> usize {
        let RxDmaPhase::Ready { len, offset } = self.phase else {
            return 0;
        };
        let count = buffer.len().min(len - offset);
        if count != 0 {
            // SAFETY: Ready requires clean retirement. The static destination
            // is exclusively owned and disjoint from this new caller borrow.
            unsafe {
                core::ptr::copy_nonoverlapping(self.staging.add(offset), buffer.as_mut_ptr(), count)
            };
        }
        self.phase = if offset + count == len {
            RxDmaPhase::Idle
        } else {
            RxDmaPhase::Ready {
                len,
                offset: offset + count,
            }
        };
        count
    }

    fn start(&mut self, regs: pac::uart::Uart, len: usize) -> Result<(), Error> {
        // Called only from Idle. Initialize only RX flags. Boundary frames may
        // be lost: RC neither counts bytes nor proves an unread DMA-era frame.
        let error = critical_section::with(|_| {
            regs.cr2().modify(|w| w.set_dmarx(false));
            regs.ier()
                .modify(|w| WaitEvent::Receive.set_enabled(w, false));
            let status = regs.isr().read();
            Self::acknowledge(regs, status);
            Self::line_error(status)
        });
        if let Some(error) = error {
            return Err(error);
        }
        // Publish before EN, including any panic/configuration failure after it.
        self.phase = RxDmaPhase::InFlight {
            len,
            line_error: None,
            error_reported: false,
        };
        // SAFETY: static private staging, typed RDR/request and this owner's
        // channel, pins and clocks are retained through every unconfirmed path.
        let result = unsafe {
            self.channel.start_read(
                self.request,
                regs.rdr().as_ptr().cast::<u8>(),
                self.staging,
                len,
            )
        };
        if result.is_err() {
            self.phase = RxDmaPhase::Quarantined;
            return Err(Error::DmaQuarantined);
        }
        critical_section::with(|_| regs.cr2().modify(|w| w.set_dmarx(true)));
        Ok(())
    }

    async fn read(
        &mut self,
        regs: pac::uart::Uart,
        state: &State,
        mut buffer: &mut [u8],
    ) -> Result<(), Error> {
        // Empty inherent reads can settle an old lease but never consume Ready.
        self.complete(regs, state).await?;
        while !buffer.is_empty() {
            let copied = self.copy_ready(buffer);
            buffer = &mut buffer[copied..];
            if buffer.is_empty() {
                break;
            }
            self.start(regs, self.capacity.min(buffer.len()))?;
            self.complete(regs, state).await?;
        }
        Ok(())
    }
}

#[cfg(uart_dma)]
impl UartRx<'static, Async> {
    /// Create a safe finite-chunk DMA receiver on F030/A030/L083 without RTS.
    ///
    /// Consumes static UART, RX pin, an admitted compatible channel and private
    /// SRAM staging of 1..=65535 bytes. Rejected inputs are consumed. Ordinary
    /// read buffers are CPU-copy destinations and are never passed to DMA.
    /// Cancelling/forgetting a read preserves its current chunk and clean tails.
    /// FE/PE returns promptly, but recovery must await clean DMA completion;
    /// errors or ambiguous DMA completion permanently quarantine resources.
    /// Dropping an unresolved owner retains its pins, channel, staging and clocks.
    /// This is not continuous reception: gaps can lose frames, with no overrun
    /// indication, lossless guarantee, early abort or bounded completion time.
    pub fn new_with_dma<T, RX, D>(
        _peri: Peri<'static, T>,
        rx: Peri<'static, RX>,
        channel: Peri<'static, D>,
        irq: impl Binding<T::Interrupt, T::InterruptHandler>
        + Binding<D::Interrupt, crate::dma::InterruptHandler<D>>
        + 'static,
        staging: &'static mut [u8],
        config: Config,
    ) -> Result<Self, ConfigError>
    where
        T: Instance,
        RX: RxPin<T>,
        D: crate::dma::RequestRoute<T, crate::dma::signal::RX>,
    {
        let baud = configured_baud::<T>(config)?;
        TxDma::validate_staging(staging).map_err(ConfigError::DmaBuffer)?;
        let channel = crate::dma::Channel::new_admitted(channel, irq)
            .map_err(|error| ConfigError::DmaChannel(error.error))?;
        let info = T::info();
        info.prepare::<T>(config, baud);
        let mut rx = Self::from_pin::<T, RX>(rx, info, baud.actual, config.rx_pull);
        rx.dma = Some(RxDma::new(channel, staging, D::REQUEST));
        critical_section::with(|_| info.regs.cr1().modify(|w| w.set_rxen(true)));
        unsafe { T::Interrupt::enable() };
        Ok(rx)
    }
}

#[cfg(uart_dma)]
impl Uart<'static, Async> {
    /// Create both DMA directions with one UART configuration, then use split
    /// or split_ref for concurrent writes and finite-chunk reads.
    ///
    /// Both channel capabilities and private static staging allocations are
    /// consumed. Shared DMA vectors must bind both channel handlers. The UART
    /// is prepared once after all validation and atomic pair admission. Each
    /// half retains only its own unresolved lease on Drop, preserving its peer.
    /// RX has the loss/cancellation limits of [`UartRx::new_with_dma`]; TX has
    /// the retained-resource limits of [`UartTx::new_with_dma`]. No RTS/CTS.
    pub fn new_with_dma<T, TX, RX, DT, DR>(
        _peri: Peri<'static, T>,
        tx: Peri<'static, TX>,
        rx: Peri<'static, RX>,
        tx_channel: Peri<'static, DT>,
        rx_channel: Peri<'static, DR>,
        irq: impl Binding<T::Interrupt, T::InterruptHandler>
        + Binding<DT::Interrupt, crate::dma::InterruptHandler<DT>>
        + Binding<DR::Interrupt, crate::dma::InterruptHandler<DR>>
        + 'static,
        tx_staging: &'static mut [u8],
        rx_staging: &'static mut [u8],
        config: Config,
    ) -> Result<Self, ConfigError>
    where
        T: Instance,
        TX: TxPin<T>,
        RX: RxPin<T>,
        DT: crate::dma::RequestRoute<T, crate::dma::signal::TX>,
        DR: crate::dma::RequestRoute<T, crate::dma::signal::RX>,
    {
        let baud = configured_baud::<T>(config)?;
        TxDma::validate_staging(tx_staging).map_err(ConfigError::DmaBuffer)?;
        TxDma::validate_staging(rx_staging).map_err(ConfigError::DmaBuffer)?;
        let (tx_channel, rx_channel) =
            crate::dma::Channel::new_admitted_pair(tx_channel, rx_channel, irq)
                .map_err(ConfigError::DmaChannel)?;
        let info = T::info();
        info.prepare::<T>(config, baud);
        let mut tx = UartTx::from_pin::<T, TX>(tx, info, baud.actual);
        let mut rx = UartRx::from_pin::<T, RX>(rx, info, baud.actual, config.rx_pull);
        tx.dma = Some(TxDma {
            channel: tx_channel,
            staging: tx_staging,
            request: DT::REQUEST,
            phase: TxDmaPhase::Idle,
        });
        rx.dma = Some(RxDma::new(rx_channel, rx_staging, DR::REQUEST));
        critical_section::with(|_| {
            info.regs.cr1().modify(|w| {
                w.set_txen(true);
                w.set_rxen(true);
            })
        });
        unsafe { T::Interrupt::enable() };
        Ok(Self { tx, rx })
    }
}

impl<'d> UartRx<'d, Blocking> {
    /// Create a blocking receive-only UART with active-low hardware RTS.
    pub fn new_blocking_with_rts<T: Instance, RX: RxPin<T>, RTS: RtsPin<T>>(
        _peri: Peri<'d, T>,
        rx: Peri<'d, RX>,
        rts: Peri<'d, RTS>,
        config: Config,
    ) -> Result<Self, ConfigError> {
        let rx = Self::new_inner::<T, RX>(
            rx,
            Some((rts.into(), <RTS as sealed::Pin<T, sealed::Rts>>::AF)),
            config,
        )?;
        Ok(rx)
    }

    /// Create a blocking receive-only UART.
    pub fn new_blocking<T: Instance, RX: RxPin<T>>(
        _peri: Peri<'d, T>,
        rx: Peri<'d, RX>,
        config: Config,
    ) -> Result<Self, ConfigError> {
        Self::new_inner::<T, RX>(rx, None, config)
    }
}
impl<'d> UartRx<'d, Async> {
    /// Create an async receive-only UART with active-low hardware RTS.
    pub fn new_with_rts<T: Instance, RX: RxPin<T>, RTS: RtsPin<T>>(
        _peri: Peri<'d, T>,
        rx: Peri<'d, RX>,
        rts: Peri<'d, RTS>,
        _irq: impl Binding<T::Interrupt, T::InterruptHandler>,
        config: Config,
    ) -> Result<Self, ConfigError> {
        let rx = Self::new_inner::<T, RX>(
            rx,
            Some((rts.into(), <RTS as sealed::Pin<T, sealed::Rts>>::AF)),
            config,
        )?;
        unsafe { T::Interrupt::enable() };
        Ok(rx)
    }

    /// Create an interrupt-driven receive-only UART.
    pub fn new<T: Instance, RX: RxPin<T>>(
        _peri: Peri<'d, T>,
        rx: Peri<'d, RX>,
        _irq: impl Binding<T::Interrupt, T::InterruptHandler>,
        config: Config,
    ) -> Result<Self, ConfigError> {
        let rx = Self::new_inner::<T, RX>(rx, None, config)?;
        unsafe { T::Interrupt::enable() };
        Ok(rx)
    }
    /// Receive exactly the requested bytes, using private staging with DMA.
    /// Cancellation keeps completed caller bytes and the current DMA chunk;
    /// a later read reaps it and preserves any clean surplus for another read.
    /// An empty inherent read may await an old chunk without consuming it.
    /// FE/PE returns promptly but recovery may wait forever for that chunk.
    /// Finite chunks and rearm/copy gaps do not provide lossless reception.
    pub async fn read(&mut self, buffer: &mut [u8]) -> Result<(), Error> {
        #[cfg(uart_dma)]
        if let Some(dma) = self.dma.as_mut() {
            return dma.read(self.info.regs, self.state, buffer).await;
        }
        for byte in buffer {
            loop {
                Wait::new(self.info.regs, &self.state.rx_waker, WaitEvent::Receive).await;
                if let Some(received) = receive(self.info.regs)? {
                    *byte = received;
                    break;
                }
            }
        }
        Ok(())
    }
}
impl<'d, M: Mode> UartRx<'d, M> {
    fn from_pin<T: Instance, RX: RxPin<T>>(
        pin: Peri<'d, RX>,
        info: Info,
        actual: u32,
        pull: Pull,
    ) -> Self {
        let mut pin = Flex::new(pin);
        pin.set_as_af(<RX as sealed::Pin<T, sealed::Rx>>::AF, false, pull);
        T::state().activate(Direction::Rx);
        Self {
            info,
            state: T::state(),
            _pin: Some(pin),
            _rts: None,
            #[cfg(uart_dma)]
            dma: None,
            actual_baudrate: actual,
            _mode: PhantomData,
        }
    }

    fn new_inner<T: Instance, RX: RxPin<T>>(
        rx: Peri<'d, RX>,
        rts: Option<(Peri<'d, AnyPin>, u8)>,
        config: Config,
    ) -> Result<Self, ConfigError> {
        let baud = configured_baud::<T>(config)?;
        let info = T::info();
        info.prepare::<T>(config, baud);
        let mut rx = Self::from_pin::<T, RX>(rx, info, baud.actual, config.rx_pull);
        rx._rts = rts.map(|(pin, af)| {
            let mut pin = Flex::new(pin);
            pin.set_as_af(af, true, Pull::None);
            pin
        });
        critical_section::with(|_| info.regs.cr2().modify(|w| w.set_rtsen(rx._rts.is_some())));
        critical_section::with(|_| info.regs.cr1().modify(|v| v.set_rxen(true)));
        Ok(rx)
    }
    /// Baud rate after divider rounding.
    pub fn actual_baudrate(&self) -> u32 {
        self.actual_baudrate
    }
    // Trait reads must not await an abandoned multi-byte DMA chunk just to
    // return their first byte. Empty trait reads use this same observation.
    fn check_read(&mut self) -> Result<(), Error> {
        #[cfg(uart_dma)]
        if let Some(dma) = self.dma.as_mut() {
            dma.check_complete(self.info.regs, self.state)?;
        }
        Ok(())
    }
    /// Read exactly the requested bytes. First observe any DMA chunk once:
    /// pending returns DmaBusy, clean staged bytes are copied before CPU reads.
    pub fn blocking_read(&mut self, buffer: &mut [u8]) -> Result<(), Error> {
        self.check_read()?;
        #[cfg(uart_dma)]
        let buffer = if let Some(dma) = self.dma.as_mut() {
            let copied = dma.copy_ready(buffer);
            &mut buffer[copied..]
        } else {
            buffer
        };
        for byte in buffer {
            loop {
                if let Some(received) = receive(self.info.regs)? {
                    *byte = received;
                    break;
                }
                core::hint::spin_loop();
            }
        }
        Ok(())
    }
}

impl<M: Mode> Drop for UartTx<'_, M> {
    fn drop(&mut self) {
        #[cfg(uart_dma)]
        if let Some(dma) = self.dma.as_mut() {
            if dma.check_complete(self.info.regs).is_err() {
                // EN-clear/TE is not a memory reclamation proof. Retain the
                // static channel/staging/pins and leave UART/DMA clocks and
                // request configuration intact. The DMA ISR remains able to
                // record and mask its own terminal source independently.
                self.info.shutdown(self.state, Direction::Tx, true);
                core::mem::forget(self.dma.take());
                core::mem::forget(self._pin.take());
                core::mem::forget(self._cts.take());
                return;
            }
        }
        self.info.shutdown(self.state, Direction::Tx, false);
    }
}
impl<M: Mode> Drop for UartRx<'_, M> {
    fn drop(&mut self) {
        #[cfg(uart_dma)]
        if let Some(dma) = self.dma.as_mut() {
            // An error return alone does not decide lifetime: a line-error
            // chunk may already have retired cleanly, whereas an unresolved
            // line error retains exactly the same DMA memory lease.
            let _ = dma.check_complete(self.info.regs, self.state);
            if matches!(
                dma.phase,
                RxDmaPhase::InFlight { .. } | RxDmaPhase::Quarantined
            ) {
                self.info.shutdown(self.state, Direction::Rx, true);
                core::mem::forget(self.dma.take());
                core::mem::forget(self._pin.take());
                core::mem::forget(self._rts.take());
                return;
            }
        }
        self.info.shutdown(self.state, Direction::Rx, false);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Baud {
    integer: u16,
    fraction: u8,
    over: vals::Over,
    actual: u32,
}
fn configured_baud<T: Instance>(config: Config) -> Result<Baud, ConfigError> {
    validate_config(config)?;
    // Configuration selects SOURCE=0 (PCLK) after preflight. Do not read a
    // peripheral-local mux before its clock has been enabled.
    let clock = crate::rcc::bus_frequency::<T>().ok_or(ConfigError::ClockNotConfigured)?;
    calculate_baud(clock.0, config)
}
fn validate_config(config: Config) -> Result<(), ConfigError> {
    #[cfg(cw32l012)]
    if config.rx_pull == Pull::Down {
        return Err(ConfigError::UnsupportedRxPull);
    }
    let _ = config;
    Ok(())
}
fn validate_cts_pull(pin: Option<&AnyPin>, pull: Pull) -> Result<(), ConfigError> {
    #[cfg(not(any(gpio_cw32l010_v1, gpio_cw32l011_v1)))]
    if let Some(pin) = pin {
        if pull == Pull::Down
            && crate::GPIO_PULL_DOWN_MASKS[pin.port() as usize] & (1 << pin.pin()) == 0
        {
            return Err(ConfigError::UnsupportedCtsPull);
        }
    }
    let _ = (pin, pull);
    Ok(())
}

fn calculate_baud(clock: u32, config: Config) -> Result<Baud, ConfigError> {
    if config.baudrate == 0 {
        return Err(ConfigError::BaudrateZero);
    }
    if config.max_error_ppm > 1_000_000 {
        return Err(ConfigError::InvalidTolerance);
    }
    let clock = u64::from(clock);
    let baud = u64::from(config.baudrate);
    let (factor, over) = match config.oversampling {
        Oversampling::Oversampling16 => (16, vals::Over::Over16),
        Oversampling::Oversampling8 => (8, vals::Over::Over8),
        Oversampling::Oversampling4 => (4, vals::Over::Over4),
    };
    // Check the requested frequency itself, before rounding into a legal divider.
    if baud * factor > clock {
        return Err(ConfigError::BaudrateTooHigh);
    }
    let (integer, fraction, divisor) = if factor == 16 {
        let divisor = (clock + baud / 2) / baud;
        (divisor / 16, divisor % 16, divisor)
    } else {
        let integer = (clock + baud * factor / 2) / (baud * factor);
        (integer, 0, integer * factor)
    };
    if integer > 65_535 {
        return Err(ConfigError::BaudrateTooLow);
    }
    if integer == 0 {
        return Err(ConfigError::BaudrateTooHigh);
    }
    // Once the rounded divider is in range, baud * divisor is near `clock`,
    // so these u64 products are bounded even for a u32 clock input.
    let requested = baud * divisor;
    if clock.abs_diff(requested) * 1_000_000 > requested * u64::from(config.max_error_ppm) {
        return Err(ConfigError::BaudrateError);
    }
    Ok(Baud {
        integer: integer as u16,
        fraction: fraction as u8,
        over,
        actual: ((clock + divisor / 2) / divisor) as u32,
    })
}

#[cfg(not(uart_cw32l083_v1))]
pub(crate) fn disable_interrupt<I: Interrupt>() {
    I::disable();
}

fn dispatch_interrupt(regs: pac::uart::Uart, state: &State) {
    critical_section::with(|cs| {
        if state.lifecycle.borrow(cs).get().live() {
            on_interrupt(regs, state);
        }
    });
}

#[derive(Clone, Copy)]
enum Direction {
    Tx,
    Rx,
}

// These are software wait conditions, not hardware bit positions. Status,
// enables and clear commands use their distinct generated PAC fieldsets.
#[derive(Clone, Copy)]
enum WaitEvent {
    TxReady,
    TxComplete,
    Receive,
    #[cfg(uart_dma)]
    ReceiveErrors,
}

impl WaitEvent {
    fn set_enabled(self, ier: &mut regs::Ier, enabled: bool) {
        match self {
            Self::TxReady => ier.set_txe(enabled),
            Self::TxComplete => ier.set_tc(enabled),
            #[cfg(uart_dma)]
            Self::ReceiveErrors => {
                ier.set_rc(false);
                ier.set_fe(enabled);
                ier.set_pe(enabled);
            }
            Self::Receive => {
                ier.set_rc(enabled);
                ier.set_fe(enabled);
                ier.set_pe(enabled);
                #[cfg(any(uart_cw32l010_v1, uart_cw32l012_v1))]
                {
                    ier.set_ne(enabled);
                    ier.set_ore(enabled);
                }
            }
        }
    }

    fn ready(self, status: regs::Isr) -> bool {
        match self {
            Self::TxReady => status.txe(),
            Self::TxComplete => !status.txbusy(),
            Self::Receive => receive_event(status),
            #[cfg(uart_dma)]
            Self::ReceiveErrors => status.pe() || status.fe(),
        }
    }
}

fn receive_event(status: regs::Isr) -> bool {
    let pending = status.rc() || status.fe() || status.pe();
    #[cfg(any(uart_cw32l010_v1, uart_cw32l012_v1))]
    let pending = pending || status.ne() || status.ore();
    pending
}

fn transmit(regs: pac::uart::Uart, byte: u8) {
    let mut command = regs::Icr::write_noop();
    command.set_tc(false);
    regs.icr().write_value(command);
    regs.tdr().write(|w| w.set_tdr(u16::from(byte)));
}

fn receive(regs: pac::uart::Uart) -> Result<Option<u8>, Error> {
    let status = regs.isr().read();
    if !receive_event(status) {
        return Ok(None);
    }
    let data = if status.rc() {
        Some(regs.rdr().read().rdr() as u8)
    } else {
        None
    };
    // R1W0: start from the generated no-op command, then clear only observed
    // receive events. Unrelated and later flags, including reserved reset-one
    // bits, remain untouched. Never read-modify-write ICR.
    let mut command = regs::Icr::write_noop();
    command.set_rc(!status.rc());
    command.set_fe(!status.fe());
    command.set_pe(!status.pe());
    #[cfg(any(uart_cw32l010_v1, uart_cw32l012_v1))]
    {
        command.set_ne(!status.ne());
        command.set_ore(!status.ore());
    }
    regs.icr().write_value(command);
    // Data-loss errors have priority, then parity, framing and noise. Clear
    // every observed receive event even when reporting only the highest one.
    #[cfg(any(uart_cw32l010_v1, uart_cw32l012_v1))]
    if status.ore() {
        return Err(Error::Overrun);
    }
    if status.pe() {
        return Err(Error::Parity);
    }
    if status.fe() {
        return Err(Error::Framing);
    }
    #[cfg(any(uart_cw32l010_v1, uart_cw32l012_v1))]
    if status.ne() {
        return Err(Error::Noise);
    }
    Ok(data)
}

fn on_interrupt(regs: pac::uart::Uart, state: &State) {
    let (rx_pending, tx_pending) = critical_section::with(|_| {
        let enabled = regs.ier().read();
        let status = regs.isr().read();
        let rx_pending = (status.rc() && enabled.rc())
            || (status.fe() && enabled.fe())
            || (status.pe() && enabled.pe());
        #[cfg(any(uart_cw32l010_v1, uart_cw32l012_v1))]
        let rx_pending =
            rx_pending || (status.ne() && enabled.ne()) || (status.ore() && enabled.ore());
        let tx_pending = (status.txe() && enabled.txe()) || (status.tc() && enabled.tc());
        // Reuse the sampled IER value. Only this driver's pending direction is
        // masked; all auxiliary and foreign interrupt enables are preserved.
        let mut next = enabled;
        if rx_pending {
            WaitEvent::Receive.set_enabled(&mut next, false);
        }
        if tx_pending {
            next.set_txe(false);
            next.set_tc(false);
        }
        regs.ier().write_value(next);
        (rx_pending, tx_pending)
    });
    // Leave status and RX data intact for the waiting task. One-shot enables
    // avoid an IRQ storm while the executor has not had a chance to run yet.
    if rx_pending {
        state.rx_waker.wake();
    }
    if tx_pending {
        state.tx_waker.wake();
    }
}

struct Wait<'a> {
    regs: pac::uart::Uart,
    waker: &'a AtomicWaker,
    event: WaitEvent,
}
impl<'a> Wait<'a> {
    fn new(regs: pac::uart::Uart, waker: &'a AtomicWaker, event: WaitEvent) -> Self {
        Self { regs, waker, event }
    }
}
impl Future for Wait<'_> {
    type Output = ();
    fn poll(self: FuturePin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        // Register before arming, then recheck after arming. Both an interrupt
        // before first poll and a hardware event during arming are retained.
        self.waker.register(cx.waker());
        critical_section::with(|_| {
            if let WaitEvent::TxComplete = self.event {
                if !self.regs.isr().read().txbusy() {
                    return Poll::Ready(());
                }
                // TC may describe an earlier frame while a later one is queued.
                let mut command = regs::Icr::write_noop();
                command.set_tc(false);
                self.regs.icr().write_value(command);
            }
            self.regs.ier().modify(|w| self.event.set_enabled(w, true));
            if self.event.ready(self.regs.isr().read()) {
                self.regs.ier().modify(|w| self.event.set_enabled(w, false));
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        })
    }
}
impl Drop for Wait<'_> {
    fn drop(&mut self) {
        // No pointer into the caller's buffer is installed in the ISR. Cancelling
        // disables only this half's interrupts and leaves unread RX flags intact.
        critical_section::with(|_| {
            self.regs.ier().modify(|w| self.event.set_enabled(w, false));
        });
    }
}

macro_rules! impl_error {
    ($($name:ident),+ $(,)?) => { $(
        impl<M: Mode> embedded_io::ErrorType for $name<'_, M> { type Error = Error; }
    )+ };
}
impl_error!(Uart, UartTx, UartRx);
macro_rules! impl_write {
    ($($name:ident),+ $(,)?) => { $(
        impl<M: Mode> embedded_io::Write for $name<'_, M> {
            fn write(&mut self, buf: &[u8]) -> Result<usize, Error> { self.blocking_write(buf)?; Ok(buf.len()) }
            fn flush(&mut self) -> Result<(), Error> { self.blocking_flush() }
        }
        impl embedded_io_async::Write for $name<'_, Async> {
            async fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
                // The trait requires an empty write to return without waiting.
                // This performs one DMA lease observation, never an await.
                if buf.is_empty() { self.blocking_write(buf)?; return Ok(0); }
                self.write(buf).await?;
                Ok(buf.len())
            }
            async fn flush(&mut self) -> Result<(), Error> { self.flush().await }
        }
    )+ };
}
impl_write!(Uart, UartTx);
// embedded-io Read must return after at least one byte, unlike the inherent
// Embassy-style exact-length read. Receive one byte rather than waiting to fill.
macro_rules! impl_read {
    ($($name:ident),+ $(,)?) => { $(
        impl<M: Mode> embedded_io::Read for $name<'_, M> {
            fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
                if buf.is_empty() { self.check_read()?; return Ok(0); }
                self.blocking_read(&mut buf[..1])?;
                Ok(1)
            }
        }
        impl embedded_io_async::Read for $name<'_, Async> {
            async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
                self.check_read()?;
                if buf.is_empty() { return Ok(0); }
                self.read(&mut buf[..1]).await?;
                Ok(1)
            }
        }
    )+ };
}
impl_read!(Uart, UartRx);

fn configure(regs: pac::uart::Uart, config: Config, baud: Baud) {
    regs.ier().write(|w| {
        w.set_txe(false);
        w.set_tc(false);
        w.set_rc(false);
        w.set_fe(false);
        w.set_pe(false);
        w.set_cts(false);
        #[cfg(any(uart_cw32l010_v1, uart_cw32l012_v1))]
        {
            w.set_rxidle(false);
            w.set_rxbrk(false);
            w.set_baud(false);
            w.set_timov(false);
            w.set_ne(false);
            w.set_ore(false);
            w.set_rxmatch(false);
        }
        #[cfg(any(uart_cw32l031_v1, uart_cw32l052_v1))]
        {
            w.set_timov(false);
            w.set_baud(false);
            w.set_rxbrk(false);
        }
    });
    regs.cr2().write(|w| {
        w.set_addren(false);
        w.set_ctsen(false);
        w.set_rtsen(false);
        w.set_rxinv(false);
        w.set_txinv(false);
        #[cfg(not(any(uart_cw32l010_v1, uart_cw32f002_v1)))]
        {
            w.set_dmarx(false);
            w.set_dmatx(false);
        }
        #[cfg(any(uart_cw32l010_v1, uart_cw32l012_v1))]
        {
            w.set_rxmatchen(false);
            w.set_swap(false);
            w.set_adcrx(false);
            w.set_adctx(false);
            w.set_loop_(false);
            w.set_rxsrc(0);
        }
        #[cfg(not(any(uart_cw32l010_v1, uart_cw32l012_v1)))]
        {
            w.set_signal(false);
            #[cfg(not(uart_cw32l052_v1))]
            w.set_source(vals::Source::Pclk);
            #[cfg(uart_cw32l052_v1)]
            w.set_sorce(vals::Source::Pclk);
        }
        #[cfg(any(uart_cw32l010_v1, uart_cw32l012_v1, uart_cw32l031_v1, uart_cw32l052_v1))]
        w.set_timcr(0);
    });
    #[cfg(any(uart_cw32l010_v1, uart_cw32l012_v1))]
    regs.cr3().write(|w| {
        w.set_dem(false);
        w.set_dep(false);
        w.set_detime(0);
        w.set_lin(false);
        w.set_brkl(false);
    });
    regs.brri().write(|w| w.set_brri(baud.integer));
    regs.brrf().write(|w| w.set_brrf(baud.fraction));
    regs.cr1().write(|w| {
        w.set_txen(false);
        w.set_rxen(false);
        w.set_sync(false);
        w.set_start(false);
        w.set_over(baud.over);
        #[cfg(any(uart_cw32l010_v1, uart_cw32l012_v1))]
        {
            // CHLEN counts the parity bit. Eight data bits with parity need
            // nine total bits; CHLEN=0 automatically clears PARITYEN.
            w.set_chlen(config.parity != Parity::ParityNone);
            w.set_parityen(config.parity != Parity::ParityNone);
            w.set_parity(if config.parity == Parity::ParityOdd {
                vals::Parity::Odd
            } else {
                vals::Parity::Even
            });
            w.set_msbf(false);
            w.set_signal(false);
            w.set_source(vals::Source::Pclk);
        }
        #[cfg(not(any(uart_cw32l010_v1, uart_cw32l012_v1)))]
        w.set_parity(match config.parity {
            Parity::ParityNone => vals::Parity::None,
            Parity::ParityEven => vals::Parity::Even,
            Parity::ParityOdd => vals::Parity::Odd,
        });
        #[cfg(uart_cw32l031_v1)]
        {
            w.set_lin(false);
            w.set_txbrk(false);
            w.set_brkl(false);
        }
        #[cfg(uart_cw32l052_v1)]
        {
            w.set_linen(false);
            w.set_txbrk(false);
            w.set_brklen(false);
        }
        w.set_stop(match config.stop_bits {
            StopBits::STOP1 => vals::Stop::Stop1,
            StopBits::STOP1P5 => vals::Stop::Stop1p5,
            StopBits::STOP2 => vals::Stop::Stop2,
        });
    });
    // Initialization clears all implemented event commands. The generated
    // no-op preserves reserved bits at their own-family documented values.
    let mut command = regs::Icr::write_noop();
    command.set_tc(false);
    command.set_rc(false);
    command.set_fe(false);
    command.set_pe(false);
    command.set_cts(false);
    #[cfg(any(uart_cw32l010_v1, uart_cw32l012_v1))]
    {
        command.set_rxidle(false);
        command.set_rxbrk(false);
        command.set_baud(false);
        command.set_timov(false);
        command.set_ne(false);
        command.set_ore(false);
        command.set_rxmatch(false);
    }
    #[cfg(any(uart_cw32l031_v1, uart_cw32l052_v1))]
    {
        command.set_timov(false);
        command.set_baud(false);
        command.set_rxbrk(false);
    }
    regs.icr().write_value(command);
}
