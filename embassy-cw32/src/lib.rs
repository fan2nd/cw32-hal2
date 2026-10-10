#![no_std]
#![allow(unsafe_op_in_unsafe_fn)]
//! Experimental CW32 HAL using Embassy singleton and type-level interrupt APIs.
//!
//! Select exactly one supported chip feature. The coverage manifest distinguishes
//! register descriptions from implemented drivers. Initialization configures the
//! verified selected clock path; asynchronous drivers require typed IRQ bindings.

#[cfg(adc)]
pub mod adc;
#[cfg(aes_cw32l083_v1)]
pub mod aes;
#[cfg(autotrim)]
pub mod autotrim;
#[cfg(awt)]
pub mod awt;
#[cfg(vc)]
pub mod comparator;
#[cfg(cordic_cw32l012_v1)]
pub mod cordic;
#[cfg(crc)]
pub mod crc;
#[cfg(dac_cw32l012_v1)]
pub mod dac;
#[cfg(dma_v1)]
pub mod dma;
#[cfg(eau_cw32l012_v1)]
pub mod eau;
#[cfg(gpio_exti)]
pub mod exti;
#[cfg(flash)]
pub mod flash;
pub mod gpio;
#[cfg(halltim_cw32l012_v1)]
pub mod halltim;
#[cfg(i2c)]
pub mod i2c;
pub mod ir;
#[cfg(lcd)]
pub mod lcd;
#[cfg(lptim)]
pub mod lptim;
pub mod lvd;
mod macros;
#[cfg(opa_cw32l012_v1)]
pub mod opamp;
pub mod rcc;
#[cfg(rtc)]
pub mod rtc;
#[cfg(spi)]
pub mod spi;
pub mod time;
#[cfg(feature = "_time-driver")]
pub mod time_driver;
#[cfg(trng_cw32l083_v1)]
pub mod trng;
#[cfg(aes_cw32l083_v1)]
mod crypto_geometry {
    include!(concat!(env!("OUT_DIR"), "/_crypto.rs"));
}
#[cfg(any(gtim_classic, btim))]
pub mod timer;
#[cfg(uart)]
pub mod usart;
#[cfg(vref)]
pub mod vref;
#[cfg(iwdt)]
pub mod wdg;
/// Raw register access, outside the HAL ownership boundary.
///
/// Like the pinned chiptool PAC, register operations use safe Rust syntax but
/// are technically unsafe hardware integration: they can violate memory safety
/// and HAL ownership. In particular, never change a HAL-owned DMA channel,
/// flags, trigger, shared reset or clock, including after a forgotten/error copy.
/// Use ordinary HAL drivers for safe resource operations. Direct PAC takeover
/// must establish quiescence and preserve all other owners independently.
pub use cw32_metapac as pac;
pub use embassy_hal_internal::{Peri, PeripheralType};
include!(concat!(env!("OUT_DIR"), "/_generated.rs"));

/// HAL initialization options, following Embassy's clock configuration boundary.
#[derive(Clone, Copy, Debug, Default)]
#[non_exhaustive]
pub struct Config {
    /// Verified system and bus clock configuration for the selected backend.
    pub rcc: rcc::Config,
}

/// Initialize clocks and acquire all peripheral singletons exactly once.
///
/// Supported firmware starts after hardware reset, or a low-level runtime/boot
/// handover that has already left all bus masters quiescent before Rust starts
/// using application memory: no outstanding accesses, armed/gated transfers or
/// pending requests that can resume when clocks are enabled. Initialization is
/// not an active-DMA bootloader
/// recovery routine: inherited accesses could already corrupt memory before it
/// runs. Runtime integration and direct PAC access must preserve HAL ownership.
/// This is the platform entry model, not a per-driver unsafe caller obligation.
///
/// On F020/F030/A030, selecting factory LSI SYSCLK (or LSE SYSCLK on the
/// three qualified packages) may briefly open each
/// entire GPIOA/B/C/F bank to inspect retained source selectors. Sampling,
/// filters and armed events can advance, including before a failure. GPIO
/// configuration and flags are preserved; gate restoration cannot undo events.
/// This is an explicit functional handover under the entry model above.
///
/// On CW32L010, starting a previously disabled LSE while RTC selects LSE
/// additionally requires a handover with no dependent RTC_OUT or RTC_1Hz
/// observer. Disconnect or leave inactive PB04/PB06 RTC digital output pads
/// and their external users, BTIM RTC trigger/reset paths, and GTIM/ATIM RTC
/// input capture or trigger paths. LPTIM and directly visible RTC output
/// conflicts are also checked. Dormant timer clocks are not enabled to prove
/// this condition. Ordinary reset entry with no intervening setup satisfies
/// the reset-route condition; register similarity is not proof of reset and
/// a firmware jump must establish it independently. Root disconnection also
/// excludes downstream timer/ADC-trigger and GPIO-filter cascades; interrupt
/// masking alone does not disconnect these observers.
///
/// L010 LSE pad admission may temporarily run the whole GPIOB bank. Input
/// sampling, filtering and armed edge capture may advance, including before
/// a later initialization error. The handover must not depend on the bank
/// remaining paused or on absence of those effects, including retained
/// LSI/LPTIM-filtered or asynchronous alternate-function participants.
/// Unrelated controls and flags are preserved; reopening the gate is not a
/// side-effect-free read. No particular unrelated output-level change is
/// implied by gate opening.
///
/// CW32L011/L012 have the same functional requirement when a new LSE feeds
/// RTC SOURCE0, and again when an RTC owner later activates or changes the
/// calendar source: no independent RTC_OUT/RTC_1Hz recipient may depend on
/// the transition unless its effects are within that RTC owner's scope. This
/// includes all output pads/external wiring, BTIM trigger/reset, GTIM/ATIM
/// inputs and their cascades, and LPTIM events. L011 routes include PA01/PA03,
/// BTIM1..3 code6, GTIM/ATIM TI code8 and LPTIM triggers1..4. L012 includes
/// PA01/PA03/PB14/PB15/PC13 and LPTIM triggers1..5. Its BTIM/ATIM mapping
/// conflicts remain unresolved; all documented alternatives must be inactive
/// or disconnected. Reserved RTC1HZ0 and a reset-looking selector are not
/// proof that a root is constant or disconnected.
///
/// On L012, newly starting LSE also requires no independent direct LSE_OUT
/// recipients on PB12/PF01/PF03, and no inaccessible inherited UART3 LSE
/// owner. The two current manuals disagree about UART3's gate operation.
/// Only already-open output banks and UART3 are inspected; closed gates do
/// not prove absence of users. These conditions are not fully checked by the
/// HAL. Dormant output/timer banks are never opened merely to inspect them.
///
/// L011/L012 pad admission temporarily runs the whole GPIOC bank, including
/// during exact source reuse. Sampling, filters and armed edge capture can
/// advance before an error. The handover must permit that progress; restoring
/// the incoming gate cannot undo it. PC13, unrelated controls, shared FLTCLK
/// and flags are preserved. No particular unrelated output glitch is implied.
/// MonitoredExistingRoutes intentionally permits existing asynchronous fault
/// capture, enabled IRQ and brake/PWM effects before an error and afterward;
/// preserving routes does not freeze their observers during a real fault.
///
/// These are functional limits on supported hardware handovers. They do not
/// replace the existing pre-Rust bus-master/memory-ownership boundary or give
/// safe Rust callers a hidden memory-safety obligation.
///
/// On x030/L083, safe static-copy admission is recorded once after clock init.
/// An already enabled/reset-held or non-default DMA controller is rejected for
/// safe copies without resetting it, clearing flags or treating EN=0 as a drain
/// acknowledgment. Rejection leaves other peripherals usable; acquiring a
/// `dma::CopyChannel` reports the reason. A non-default-register rejection is
/// detected after clock enable and leaves that gate enabled; it is diagnostics
/// within the supported entry model, not arbitrary-handover sanitization.
/// Generic profiles without qualified SRAM cannot acquire this capability.
/// The shared DMA gate remains enabled
/// once admitted; no safe DMA operation resets or disables it.
///
/// Panics on a clock initialization error or a repeated call. Uses bounded
/// hardware waits. The default is nominal 8 MHz HSI/HCLK/PCLK except L010/L011,
/// which use their documented nominal 4 MHz reset clock. See the family RCC config.
pub fn init(config: Config) -> Peripherals {
    try_init(config).unwrap_or_else(|error| panic!("CW32 clock initialization failed: {:?}", error))
}

/// Fallible initialization with the same one-time ownership boundary and public
/// native RTC/output-observer and whole-bank functional handover requirements as [`init`].
///
/// Invalid configuration is rejected before taking ownership. A hardware timeout
/// can leave clocks partially changed and consumes the singleton set; reset the
/// device before retrying. RCC errors publish no new clocks. A later time-driver
/// initialization error may occur after RCC has published verified clocks; it
/// still returns no peripheral tokens.
/// With a time-driver feature, exact nominal 1 MHz divisibility is also checked
/// before taking ownership; the whole selected timer/IRQ is then initialized
/// before returning the remaining peripherals. See the time-driver module for the
/// continuous-clock and strict IRQ blackout contract.
pub fn try_init(config: Config) -> Result<Peripherals, rcc::Error> {
    let clocks = config.rcc.frequencies()?;
    #[cfg(feature = "_time-driver")]
    let timer_divisor = time_driver::validate_clock(clocks.pclk_bounds())?;
    #[cfg(not(feature = "_time-driver"))]
    let _ = clocks;
    let peripherals = Peripherals::take();
    unsafe {
        rcc::init(config.rcc)?;
    }
    #[cfg(feature = "_time-driver")]
    time_driver::init(timer_divisor)?;
    #[cfg(dma_v1)]
    dma::init(config.rcc.timeout);
    Ok(peripherals)
}

/// Driver operating modes, matching Embassy naming.
pub mod mode;

/// RAM parity diagnostics with exclusive controller ownership.
pub mod ram;
