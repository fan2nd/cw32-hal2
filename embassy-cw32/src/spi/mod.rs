//! Serial Peripheral Interface (SPI), verified CW32 master backends.
//!
//! This driver follows Embassy's ownership, configuration and blocking bus API.
//! The peripheral and all three signal pins are exclusively owned through
//! [`Peri`]. Chip select is managed separately, for example by an
//! `embedded-hal-bus` device adapter and a GPIO output.
//!
//! Register behavior is from CW32x030 User Manual CN V2.5, chapter 19 (also
//! checked against EN V1.0 and the F030 SDK V2.2), and the L031/R031/W031,
//! L052 and L083 manuals/SDKs for the shared `cw32l031_v1` controller. See
//! `docs/l031-shared-serial.md` for the source audit and conservative limits.
//! F002/F003 and L010/L011/L012 have dedicated register/clock policies; see
//! `docs/remaining-spi-classic-i2c.md`. BR=7 is reserved on old IP, /256 on
//! L010/L011, and L012 has a linear even-divider field. WIDTH=bits-1,
//! ICR flags clear on writing zero, and BUSY must clear before returning.
//!
//! Blocking full-duplex master is available on all supported backends.
//! F030/A030/F020/L031/L052/L083/R031 and native L012 SPI1/SPI2/SPI3
//! expose staged byte DMA through `Spi::new_with_dma`. L012 qualification uses
//! its own CN V1.4 chapter 22 and DMA chapter 8; safe staging/admission requires
//! exact C8T6/C8U6 memory metadata. Independently qualified families expose
//! interrupt-driven byte masters through `Spi::new_async`, including
//! F020/L031/L052/R031.
//! The constructor fixes each owner's backend; both use the same async
//! `SpiBus<u8>` methods. A static PIO owner still uses PIO, and separate
//! controllers can use different backends. L012 SPI2/SPI3 require the complete
//! shared SPI23 handler for either async backend. L012 now includes the existing
//! DMA variants of the non-exhaustive configuration and transfer errors.
//! Native DMA SIZE selects SRAM byte width; fixed typed DR addresses retain
//! 32-bit peripheral transactions with a 16-bit payload and eight-bit frames.
//!
//! DMA never accesses caller slices. PIO ISRs never access caller memory or DR.
//! Cancellation leaves the current private DMA chunk or at most one PIO frame
//! to settle: preserve the original CS selection and successfully flush before
//! changing it. Generic device adapters that release CS on cancellation are
//! not covered. Dropping an unfinished PIO owner is destructive teardown, not
//! successful wire completion; unresolved DMA owners retain their resources.
//! See `docs/spi-dma.md` and `docs/spi-async-pio.md` for the distinct lifecycles
//! and hardware-validation limits.
#[cfg(spi_dma)]
use crate::dma;
#[cfg(any(spi_dma, spi_async_pio))]
use crate::{
    interrupt::typelevel::{Binding, Handler, Interrupt},
    mode::Async,
};
use core::marker::PhantomData;
#[cfg(any(spi_dma, spi_async_pio))]
use core::{
    cell::RefCell,
    task::{Context, Poll, Waker},
};

pub use embedded_hal::spi::{MODE_0, MODE_1, MODE_2, MODE_3, Mode, Phase, Polarity};

use crate::gpio::{Flex, Pin, Pull};
use crate::mode::{Blocking, Mode as PeriMode};
use crate::rcc::ClockBounds;
use crate::time::Hertz;
use crate::{Peri, PeripheralType, pac};

#[cfg(any(spi_async_pio, spi_dma))]
mod async_api;
mod blocking;
mod config;
#[cfg(spi_dma)]
mod dma_backend;
#[cfg(any(spi_async_pio, spi_dma))]
mod interrupt;
#[cfg(spi_async_pio)]
mod pio;
mod traits;

pub use config::{BitOrder, Config, ConfigError, Error, SampleDelay};
#[cfg(any(spi_async_pio, spi_dma))]
pub use interrupt::InterruptHandler;
#[cfg(all(spi_cw32l012_v1, any(spi_async_pio, spi_dma)))]
pub use interrupt::SharedInterruptHandler;

use blocking::decode_error;
use config::Prescaler;
#[cfg(spi_dma)]
use dma_backend::{SpiDma, dispatch_dma_interrupt};
#[cfg(spi_async_pio)]
use pio::{PioBuffers, dispatch_pio_interrupt};

/// SPI communication-mode markers.
pub mod mode;
use mode::{CommunicationMode, Master};

/// Owned SPI bus. SCK/MOSI are push-pull outputs using the family GPIO drive
/// setting (high speed on x030/F020; fixed hardware drive on the low-power families).
///
/// Dropping the bus disables SPI and its peripheral clock, then disconnects its
/// pins. Every successful blocking operation has already waited for bus idle.
/// A staged DMA owner instead retains all resources if its pair cannot be
/// nonblockingly confirmed complete. Cancellation may leave clocks active; keep
/// the original CS selection until a successful async flush.
pub struct Spi<'d, M: PeriMode, CM: CommunicationMode = Master> {
    peripheral: Peri<'d, AnySpi>,
    sck: Option<Flex<'d>>,
    _mosi: Option<Flex<'d>>,
    miso: Option<Flex<'d>>,
    #[cfg(spi_dma)]
    dma: Option<SpiDma>,
    #[cfg(spi_async_pio)]
    pio: Option<PioPhase>,
    sck_af: u8,
    miso_af: u8,
    word_bits: u8,
    kernel_clock: ClockBounds,
    config: Config,
    frequency_bounds: ClockBounds,
    _mode: PhantomData<(M, CM)>,
}

// IRQ PIO is positively qualified independently of the existing DMA path.
// Only the task accesses DR or caller memory. An abandoned future leaves this
// owner-local phase, never an address into that future, for the next entry.
#[cfg(spi_async_pio)]
#[derive(Clone, Copy, Eq, PartialEq)]
enum PioPhase {
    Idle,
    AwaitRx,
    Finishing,
}

#[cfg(spi_async_pio)]
struct PioInterruptState {
    active: bool,
    error: Option<Error>,
    waker: Option<Waker>,
}

#[cfg(spi_async_pio)]
pub(crate) struct PioState {
    inner: critical_section::Mutex<RefCell<PioInterruptState>>,
}

#[cfg(spi_async_pio)]
#[derive(Clone, Copy)]
enum PioWait {
    Tx,
    Rx,
    Idle,
}

#[cfg(spi_dma)]
struct InterruptState {
    active: bool,
    error: Option<Error>,
    waker: Option<Waker>,
}

// No pointer into an owner or operation future is retained by an interrupt.
#[cfg(spi_dma)]
pub(crate) struct State {
    inner: critical_section::Mutex<RefCell<InterruptState>>,
}

/// Supported SPI frame types: `u8`, `u16`, and [`word`] wrappers for 4–15 bits.
///
/// The trait is sealed, so an unsupported frame width cannot reach hardware.
///
#[allow(private_bounds)]
pub trait Word: sealed::Word + Copy + Default + 'static {}

/// Non-native word widths. Only the low declared number of bits is transferred.
/// These tuple wrappers follow Embassy's `dma::word::U4` etc. convention.
pub mod word;

#[derive(Clone, Copy)]
pub(crate) struct AnySpi {
    pub(crate) info: &'static Info,
}
impl PeripheralType for AnySpi {}
pub(crate) struct Info {
    pub(crate) regs: pac::spi::Spi,
    pub(crate) maximum_frequency: u32,
    pub(crate) minimum_divisor: u16,
    pub(crate) rcc: crate::rcc::RccInfo,
    #[cfg(spi_async_pio)]
    pub(crate) pio_state: fn() -> &'static PioState,
}

#[cfg(spi_async_pio)]
impl Info {
    fn pio_state(&self) -> &'static PioState {
        (self.pio_state)()
    }
}

pub(crate) mod sealed {
    use super::*;
    pub(crate) trait Instance:
        crate::rcc::RccPeripheral + PeripheralType + Into<AnySpi>
    {
        const MAX_FREQUENCY: u32;
        const MIN_DIVISOR: u16;
        #[cfg(any(spi_dma, spi_async_pio))]
        fn regs() -> pac::spi::Spi;
        #[cfg(spi_dma)]
        fn state() -> &'static State;
        #[cfg(spi_async_pio)]
        fn pio_state() -> &'static PioState;
    }
    pub trait SckPin<T: super::Instance> {
        fn af(&self) -> u8;
    }
    pub trait MosiPin<T: super::Instance> {
        fn af(&self) -> u8;
    }
    pub trait MisoPin<T: super::Instance> {
        fn af(&self) -> u8;
    }
    pub trait Word {
        const BITS: u8;
        fn into_u16(self) -> u16;
        fn from_u16(value: u16) -> Self;
        // Only u8 opts in. No casting of arbitrary word slices or new DMA widths.
        #[cfg(spi_dma)]
        fn as_bytes(_words: &[Self]) -> Option<&[u8]>
        where
            Self: Sized,
        {
            None
        }
        #[cfg(spi_dma)]
        fn as_bytes_mut(_words: &mut [Self]) -> Option<&mut [u8]>
        where
            Self: Sized,
        {
            None
        }
    }
}

/// A verified SPI instance. Implementations are generated only for supported chips.
#[allow(private_bounds)]
pub trait Instance: sealed::Instance {
    /// Metadata-derived SPI GLOBAL vector for the qualified async path.
    #[cfg(any(spi_dma, spi_async_pio))]
    type Interrupt: Interrupt;
    /// Complete required async handler; L012 SPI2/SPI3 use the shared SPI23 group.
    #[cfg(any(spi_async_pio, spi_dma))]
    type InterruptHandler: Handler<Self::Interrupt>;
}
/// Verified SCK alternate-function route for an SPI instance.
#[allow(private_bounds)]
pub trait SckPin<T: Instance>: Pin + sealed::SckPin<T> {}
/// Verified MOSI alternate-function route for an SPI instance.
#[allow(private_bounds)]
pub trait MosiPin<T: Instance>: Pin + sealed::MosiPin<T> {}
/// Verified MISO alternate-function route for an SPI instance.
#[allow(private_bounds)]
pub trait MisoPin<T: Instance>: Pin + sealed::MisoPin<T> {}

// Generated hooks take only SDK/datasheet-verified instance and package routes.
macro_rules! impl_instance {
    ($name:ident, $maximum_frequency:literal, $minimum_divisor:literal, $irq:ident) => {
        impl $crate::spi::sealed::Instance for $crate::peripherals::$name {
            const MAX_FREQUENCY: u32 = $maximum_frequency;
            const MIN_DIVISOR: u16 = $minimum_divisor;
            #[cfg(any(spi_dma, spi_async_pio))]
            fn regs() -> $crate::pac::spi::Spi {
                $crate::pac::$name
            }
            #[cfg(spi_dma)]
            fn state() -> &'static $crate::spi::State {
                static STATE: $crate::spi::State = $crate::spi::State::new();
                &STATE
            }
            #[cfg(spi_async_pio)]
            fn pio_state() -> &'static $crate::spi::PioState {
                static STATE: $crate::spi::PioState = $crate::spi::PioState::new();
                &STATE
            }
        }
        impl $crate::spi::Instance for $crate::peripherals::$name {
            #[cfg(any(spi_dma, spi_async_pio))]
            type Interrupt = $crate::interrupt::typelevel::$irq;
            #[cfg(any(spi_async_pio, spi_dma))]
            $crate::spi::pio_interrupt_handler!($irq);
        }
        impl From<$crate::peripherals::$name> for $crate::spi::AnySpi {
            fn from(_: $crate::peripherals::$name) -> Self {
                use $crate::spi::sealed::Instance;
                static INFO: $crate::spi::Info = $crate::spi::Info {
                    regs: $crate::pac::$name,
                    maximum_frequency: <$crate::peripherals::$name>::MAX_FREQUENCY,
                    minimum_divisor: <$crate::peripherals::$name>::MIN_DIVISOR,
                    rcc: <$crate::peripherals::$name as $crate::rcc::SealedRccPeripheral>::RCC_INFO,
                    #[cfg(spi_async_pio)]
                    pio_state: <$crate::peripherals::$name>::pio_state,
                };
                Self { info: &INFO }
            }
        }
    };
}
pub(crate) use impl_instance;

// Match the actual metadata vector, not a register version or instance alias.
#[cfg(any(spi_async_pio, spi_dma))]
macro_rules! pio_interrupt_handler {
    (SPI23) => {
        type InterruptHandler =
            $crate::spi::SharedInterruptHandler<$crate::interrupt::typelevel::SPI23>;
    };
    ($irq:ident) => {
        type InterruptHandler = $crate::spi::InterruptHandler<Self>;
    };
}
#[cfg(any(spi_async_pio, spi_dma))]
pub(crate) use pio_interrupt_handler;

macro_rules! impl_pin {
    ($signal:ident, $instance:ident, $pin:ident, $af:literal) => {
        impl $crate::spi::sealed::$signal<$crate::peripherals::$instance>
            for $crate::peripherals::$pin
        {
            fn af(&self) -> u8 {
                $af
            }
        }
        impl $crate::spi::$signal<$crate::peripherals::$instance> for $crate::peripherals::$pin {}
    };
}
pub(crate) use impl_pin;
