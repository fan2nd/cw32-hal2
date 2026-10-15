//! UART with Embassy-style owned pins, typed interrupts and split halves.
//!
//! Both blocking and interrupt-driven async transfers are implemented. Async
//! transfers normally use one interrupt/wakeup per byte. On F030/A030/L083
//! and F020/L031/L052/R031, `UartTx::new_with_dma`,
//! `UartRx::new_with_dma` and `Uart::new_with_dma` own static staging for
//! ordinary borrowed writes and reads. RX DMA receives finite explicitly armed
//! chunks; reception/copy/rearm gaps can lose frames. L012 supports only staged
//! `UartTx::new_with_dma`; native RX/combined DMA remains excluded.
//! Ordinary non-DMA `Uart`/`UartRx` have no software RX buffer: service each
//! byte promptly. `BufferedUart`/`BufferedUartRx` continuously collect through
//! interrupts into consumed static storage, including between read calls.
//! A full software ring preserves its prefix and reports `BufferOverflow`;
//! later observations are discarded until the pending error is read.
//! L010/L011/L012 support finite async `read_until_idle` into a caller buffer
//! and expose noise and overrun errors. Idle boundaries are best-effort under
//! service delay; ordinary idle reads do not collect between calls. Other
//! supported UARTs have no overrun flag and can silently overwrite an unread
//! frame. No throughput guarantee follows.
//! Hardware RTS/CTS is available through owned, package-checked flow-control pins.
//! Buffered collection depends on interrupt progress; hardware RTS does not
//! reflect software occupancy. Buffered idle markers, DMA rings, RX DMA on
//! L010/L011/L012, synchronous mode and LSI/deep-sleep UART are not exposed.
//! L012 ordinary PIO supports nine data bits without parity through the u16
//! APIs. Buffered/DMA owners remain byte-only; other families reject nine bits.
//!
//! Register semantics follow each family's official manual and SDK. See
//! `docs/remaining-uart-implementation.md`, `docs/f020-serial-compatibility.md`
//! and `docs/l031-r031-w031-uart.md`. ICR is R1W0 with version-specific reset
//! and clearable bits. Flush always waits for TXBUSY (buffer plus shifter) to
//! become zero, using TC only to wake. Optional LIN, automatic baud detection,
//! RS485 direction, loopback and UART timers remain disabled.
//! Four-channel staged TX follows each own byte SRAM-to-TDR example: L031
//! CN V1.6 p344, L052 CN V1.5 p381, R031 CN V1.3 p347 and W031 CN V1.4 p347;
//! see `docs/four-channel-dma-owned.md` for their own UART1/2/3 qualification.
//! Finite staged RX additionally follows each own byte RDR-to-SRAM example:
//! F020 CN V1.4 p288, L031 CN V1.6 p345, L052 CN V1.5 p382, R031 CN V1.3
//! p348 and W031 CN V1.4 p348; see `docs/classic-uart-dma-rx.md`.
//! L012 uses its own CN V1.4 p474 byte SRAM-to-TDR example and native paired
//! DMACH12/DMACH34 vectors; see `docs/l012-dma-owned.md`.
//!
//! L083 vectors are shared in pairs. Async construction requires the matching
//! `SharedInterruptHandler<interrupt::typelevel::UART1_UART4>` (or 2/5, 3/6).
//! It guards each instance before MMIO. Constructing or dropping either owner
//! never disables or unpends its partner's NVIC line; the vector remains enabled
//! after first use. A HAL-torn-down instance has masked IER and an inactive guard.
//! An unowned bootloader/external partner is skipped without MMIO, so its owner
//! must quiesce or service that source before the HAL enables the shared vector.
//! This driver never resets or clears another owner's UART to establish that.

use core::cell::{Cell, RefCell};
use core::future::{Future, poll_fn};
use core::marker::PhantomData;
use core::pin::Pin as FuturePin;
use core::task::{Context, Poll};

use embassy_sync::waitqueue::AtomicWaker;

use crate::gpio::{AnyPin, Flex, Pin, Pull};
use crate::interrupt::typelevel::{Binding, Handler, Interrupt};
use crate::mode::{Async, Blocking, Mode};
use crate::{Peri, PeripheralType, pac};

use pac::uart::{regs, vals};

mod buffered;
mod config;
#[cfg(uart_dma_rx)]
mod dma_rx;
#[cfg(uart_dma_tx)]
mod dma_tx;
#[cfg(uart_cw32l012_v1)]
mod half_duplex;
mod interrupt;
#[cfg(uart_cw32l012_v1)]
mod nine_bit;
mod rx;
mod traits;
mod tx;
mod uart;

pub use config::{Config, ConfigError, DataBits, Error, Oversampling, Parity, StopBits};
#[cfg(uart_cw32l012_v1)]
pub use half_duplex::HalfDuplexUart;
pub use interrupt::InterruptHandler;
#[cfg(uart_cw32l083_v1)]
pub use interrupt::SharedInterruptHandler;

use config::{Baud, configure, configured_baud, require_byte_config, validate_cts_pull};
#[cfg(uart_dma_rx)]
use dma_rx::{RxDma, RxDmaPhase};
#[cfg(uart_dma_tx)]
use dma_tx::TxDma;
#[cfg(all(uart_dma_tx, uart_dma_rx))]
use dma_tx::TxDmaPhase;
#[cfg(not(uart_cw32l083_v1))]
pub(crate) use interrupt::disable_interrupt;
use interrupt::{Wait, WaitEvent, receive, transmit};
#[cfg(any(uart_cw32l010_v1, uart_cw32l012_v1))]
use interrupt::{receive_observed, retire_receive_wait};
#[cfg(uart_cw32l012_v1)]
use interrupt::{receive_observed_word, transmit_word};

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
            // Safe token reborrows can outlive a forgotten buffered owner.
            // Retire its ring storage before resetting any hardware state.
            *state.rx_ring.borrow(cs).borrow_mut() = None;
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
            if let Direction::Rx = direction {
                // RX sources are masked and nonretained RX/RTS disabled before
                // detaching storage. Ordinary and DMA receivers have no ring.
                *state.rx_ring.borrow(cs).borrow_mut() = None;
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
    rx_ring: critical_section::Mutex<RefCell<Option<RxRing>>>,
}
impl State {
    pub(crate) const fn new() -> Self {
        Self {
            tx_waker: AtomicWaker::new(),
            rx_waker: AtomicWaker::new(),
            lifecycle: critical_section::Mutex::new(Cell::new(Lifecycle::new())),
            rx_ring: critical_section::Mutex::new(RefCell::new(None)),
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

// Describes storage owned or borrowed by the RX owner, without retaining an
// allocation-wide reference. ISR writes must not reborrow a full mutable
// slice while fill_buf's shared prefix is live. Never expose this descriptor.
struct RxStorage {
    ptr: *mut u8,
    capacity: usize,
}
struct RxRing {
    storage: RxStorage,
    read: usize,
    write: usize,
    len: usize,
    exposed: usize,
    error: Option<Error>,
    waiting: bool,
}

/// Why an IRQ receive ring could not be replaced. The supplied buffer is returned.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum BufferReplaceError {
    /// The new ring must have nonzero capacity.
    Empty,
    /// Read the queued bytes and pending error before retrying.
    PendingReceive,
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
    #[cfg(uart_dma_tx)]
    dma: Option<TxDma>,
    actual_baudrate: u32,
    data_bits: DataBits,
    _mode: PhantomData<M>,
}
/// Owned UART receiver, optionally using private finite-chunk DMA staging.
pub struct UartRx<'d, M: Mode> {
    info: Info,
    state: &'static State,
    _pin: Option<Flex<'d>>,
    _rts: Option<Flex<'d>>,
    #[cfg(uart_dma_rx)]
    dma: Option<RxDma>,
    actual_baudrate: u32,
    data_bits: DataBits,
    _mode: PhantomData<M>,
}

/// Owned UART receiver with continuous interrupt-driven software buffering.
///
/// Safe constructors consume a nonempty static mutable slice, including on
/// configuration error. Unsafe `new_borrowed*` variants accept an ordinary
/// mutable borrow tied to this owner and release it on normal Drop.
/// The HAL does not initialize its bytes. [`Self::into_buffer`] consumes RX and
/// returns the current storage with this owner's lifetime after shutdown.
/// [`Self::try_replace_buffer`] can swap an empty ring for new static storage.
/// Normal Drop also releases borrowed storage without an explicit return.
/// Combined UART halves share a lifetime, so a still-used TX
/// half may conservatively keep that borrow alive even after RX Drop.
/// Any nonzero capacity is accepted, including one byte. Peripheral and pin
/// borrows need not be static. The ISR retains a raw ring pointer; borrowed
/// constructors require an additional lifetime/no-leak safety obligation.
///
/// Collection starts during construction and continues between reads while
/// interrupts can run. Software overflow and line errors preserve the queued
/// clean prefix, then report the first pending error. Later ISR observations
/// are discarded until that error is consumed. This is an ordering of software
/// observations: a frame already in RDR can be collected after error recovery.
/// Hardware RTS reflects RDR occupancy, not the software ring's free capacity.
///
/// Inherent and async trait reads return the available clean prefix, waiting
/// for at least one byte for nonempty slices. Use `read_exact` to fill a slice.
/// Empty reads consume neither queued bytes nor errors.
///
/// Buffering is not lossless: ten families have no hardware overrun flag, and
/// even native L010/L011/L012 flags can coalesce. There is no packet boundary,
/// idle method, timeout, DMA ring, loss count or throughput guarantee. Forgetting
/// a safely constructed owner can leave collection running forever with valid
/// static storage; borrowed constructors have a stricter safety contract.
pub struct BufferedUartRx<'d> {
    info: Info,
    state: &'static State,
    _pin: Flex<'d>,
    _rts: Option<Flex<'d>>,
    actual_baudrate: u32,
    _buffer: PhantomData<&'d mut [u8]>,
}

/// Owned UART with ordinary async TX and continuous IRQ-buffered RX.
///
/// The safe static and unsafe borrowed receive-storage contracts, and the
/// error/recovery limits, are those of
/// [`BufferedUartRx`]. Split the real TX and RX owners to use them concurrently.
pub struct BufferedUart<'d> {
    tx: UartTx<'d, Async>,
    rx: BufferedUartRx<'d>,
}

#[derive(Clone, Copy)]
enum Direction {
    Tx,
    Rx,
}
