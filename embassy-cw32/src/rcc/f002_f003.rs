//! Qualified HSI and direct digital HEX initialization for CW32F002/F003.
//!
//! Own manuals: F002 CN1.4 / F003 CN2.3 §§4.3–4.7, GPIO and AWT chapters;
//! own datasheet external/electrical tables. See docs/qualified-hex.md.
//! Both PB0 and PB1 are inputs, independently usable by retained AWT. There is
//! no HSE crystal, PLL, programmable startup count or documented clock-loss
//! recovery on these chips. STABLE latches startup and does not detect loss.
//! One-time init requires continuous legal clocks; runtime changes and
//! low-power entry/resume are outside this backend's contract.

use crate::{pac, time::Hertz};
use core::cell::Cell;
use critical_section::Mutex;
use pac::sysctrl::vals::Sysclk as ClockSource;

/// Nominal calibrated internal high-speed oscillator frequency.
pub const HSI_FREQ: Hertz = Hertz(crate::RCC_HSI_FREQUENCY_HZ);

/// Documented divider between HSIOSC and HSI. Values are hardware encodings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(u8)]
pub enum HsiDiv {
    /// Divide by 1 (48 MHz).
    Div1 = 0b0110,
    /// Divide by 2 (24 MHz).
    Div2 = 0b1000,
    /// Divide by 4 (12 MHz).
    Div4 = 0b1001,
    /// Divide by 6 (8 MHz, the reset divider).
    Div6 = 0b0101,
    /// Divide by 8 (6 MHz).
    Div8 = 0b1011,
    /// Divide by 10 (4.8 MHz).
    Div10 = 0b1100,
    /// Divide by 12 (4 MHz).
    Div12 = 0b1101,
    /// Divide by 14 (approximately 3.429 MHz).
    Div14 = 0b1110,
    /// Divide by 16 (3 MHz).
    Div16 = 0b1111,
}
impl HsiDiv {
    /// Numeric division factor (not the register encoding).
    pub const fn divisor(self) -> u32 {
        match self {
            Self::Div1 => 1,
            Self::Div2 => 2,
            Self::Div4 => 4,
            Self::Div6 => 6,
            Self::Div8 => 8,
            Self::Div10 => 10,
            Self::Div12 => 12,
            Self::Div14 => 14,
            Self::Div16 => 16,
        }
    }
}

/// HSI configuration. HSI is always enabled in the supported clock tree.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Hsi {
    /// Divider applied to the factory-calibrated 48 MHz HSIOSC clock.
    pub div: HsiDiv,
}
impl Default for Hsi {
    fn default() -> Self {
        Self {
            div: crate::RCC_DEFAULT_HSI_DIV,
        }
    }
}

/// Divider from SysClk to HCLK.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(u8)]
pub enum AHBPrescaler {
    /// Divide by 1.
    Div1 = 0,
    /// Divide by 2.
    Div2 = 1,
    /// Divide by 4.
    Div4 = 2,
    /// Divide by 8.
    Div8 = 3,
    /// Divide by 16.
    Div16 = 4,
    /// Divide by 32.
    Div32 = 5,
    /// Divide by 64.
    Div64 = 6,
    /// Divide by 128.
    Div128 = 7,
}
impl AHBPrescaler {
    /// Numeric division factor.
    pub const fn divisor(self) -> u32 {
        1 << self as u8
    }
}

/// Divider from HCLK to PCLK. F002/F003 have one PCLK domain.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(u8)]
pub enum APBPrescaler {
    /// Divide by 1.
    Div1 = 0,
    /// Divide by 2.
    Div2 = 1,
    /// Divide by 4.
    Div4 = 2,
    /// Divide by 8.
    Div8 = 3,
}
impl APBPrescaler {
    /// Numeric division factor.
    pub const fn divisor(self) -> u32 {
        1 << self as u8
    }
}

/// Direct system-clock sources supported by F002/F003.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Sysclk {
    /// Factory-calibrated HSI through its divider.
    HSI,
    /// Externally driven digital clock on PB0 or PB1.
    HEX,
}

pub use pac::sysctrl::vals::HexInput;

/// Board-qualified digital input, independent of factory HSI accuracy.
///
/// The complete actual envelope must remain within 4–32 MHz. Include source
/// tolerance, temperature, aging, loading, supply and short-term cycle variation
/// throughout `operating_conditions`. Both the 40–60% duty requirement and the
/// minimum 15 ns high/low times apply; rise/fall times must be at most 20 ns.
/// Meet the own datasheet's voltage thresholds and I/O ratings as well.
/// These declarations are not measurements. The source must remain continuous
/// throughout initialization and use: these chips have no documented automatic
/// loss-clock fallback, and STABLE is a startup latch rather than a loss monitor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Hex {
    /// Nominal frequency; never a substitute for the actual upper bound.
    pub freq: Hertz,
    /// Guaranteed minimum actual frequency, in whole hertz.
    pub min_freq: Hertz,
    /// Guaranteed maximum actual frequency, in whole hertz.
    pub max_freq: Hertz,
    /// MCU supply and ambient interval qualifying these source bounds.
    pub operating_conditions: crate::rcc::OperatingConditions,
    /// One independently available digital input; neither is a crystal output.
    pub input: HexInput,
}
impl Hex {
    fn bounds(self) -> Result<crate::rcc::ClockBounds, Error> {
        let bounds = crate::rcc::ClockBounds::external(
            self.freq,
            self.min_freq,
            self.max_freq,
            self.operating_conditions,
        )
        .ok_or(Error::InvalidHexBounds)?;
        if self.min_freq.0 < crate::RCC_HEX_RANGE_HZ.0
            || self.max_freq.0 > crate::RCC_HEX_RANGE_HZ.1
        {
            return Err(Error::HexOutsideQualifiedRange);
        }
        let c = self.operating_conditions;
        if c.min_supply_mv < crate::RCC_HEX_SUPPLY_MV.0
            || c.max_supply_mv > crate::RCC_HEX_SUPPLY_MV.1
            || c.min_temperature_c < crate::RCC_HEX_TEMPERATURE_C.0
            || c.max_temperature_c > crate::RCC_HEX_TEMPERATURE_C.1
        {
            return Err(Error::HexConditionsOutsideQualifiedRange);
        }
        Ok(bounds)
    }
}

/// Clock tree initialization options, using Embassy's RCC configuration naming.
///
/// One-time initialization requires an electrically legal, stable inherited
/// HSI/HEX/LSI source, a documented HSI divider and correct incoming Flash wait.
/// Every source used during the transition must remain continuous and within
/// its own electrical limits. A trim change uses unchanged LSI, which must be
/// in its documented 32.8 kHz ±10% range. A ready flag cannot qualify arbitrary
/// inherited trim. Clock-dependent peripherals, DMA and interrupts must be
/// quiescent, and no concurrent code (including NMI) may change the clock tree.
///
/// These conditions apply when passed to `embassy_cw32::try_init` or `init`.
/// This is not a runtime clock-switch or loss-clock recovery interface. See
/// `docs/qualified-hex.md` for the complete source and retained-owner contract.
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub struct Config {
    /// Board-declared supply and ambient TA envelope. No measurement is made.
    /// The full qualified range is the default; declare the actual narrower
    /// board range to permit faster operation where the datasheet allows it.
    pub operating_conditions: crate::rcc::OperatingConditions,
    /// HSI oscillator divider.
    pub hsi: Hsi,
    /// Optional external input, configured and reserved even when SYSCLK is HSI.
    pub hex: Option<Hex>,
    /// System-clock source. HEX requires `hex: Some(...)`.
    pub sys: Sysclk,
    /// HCLK prescaler.
    pub ahb_pre: AHBPrescaler,
    /// PCLK prescaler.
    pub apb_pre: APBPrescaler,
    /// Maximum register polls for each hardware wait. Must be nonzero.
    ///
    /// This is an iteration budget, not a duration: the entry clock may be
    /// unknown. No timer or Embassy time driver is required during init.
    pub timeout: u32,
}
impl Config {
    /// Reset-equivalent nominal 8 MHz clock tree.
    pub const fn new() -> Self {
        Self {
            operating_conditions: crate::rcc::OperatingConditions::new(),
            hsi: Hsi {
                div: crate::RCC_DEFAULT_HSI_DIV,
            },
            hex: None,
            sys: Sysclk::HSI,
            ahb_pre: AHBPrescaler::Div1,
            apb_pre: APBPrescaler::Div1,
            timeout: 100_000,
        }
    }

    /// Validate the board envelope and calculate clocks without touching hardware.
    ///
    /// Declared VDD/ambient ranges must be inside the selected family's source
    /// qualification. Actual upper HCLK/PCLK must fit the minimum-VDD bus limit.
    /// Nominal Hertz remains rounded down; the bound getters retain exact source
    /// division. A nominal rate equal to a rated ceiling may be rejected because
    /// its qualified positive HSI error crosses that ceiling. No voltage,
    /// temperature or frequency is measured. A stable, legal incoming clock and
    /// appropriate incoming FLASH latency remain initialization preconditions.
    pub fn frequencies(&self) -> Result<Clocks, Error> {
        if self.timeout == 0 {
            return Err(Error::InvalidTimeout);
        }
        let hsi = HSI_FREQ / self.hsi.div.divisor();
        let hex = if let Some(hex) = self.hex {
            let bounds = hex.bounds()?;
            let board = self.operating_conditions;
            let source = hex.operating_conditions;
            if board.min_supply_mv < source.min_supply_mv
                || board.max_supply_mv > source.max_supply_mv
                || board.min_temperature_c < source.min_temperature_c
                || board.max_temperature_c > source.max_temperature_c
            {
                return Err(Error::HexConditionsDoNotCoverBoard);
            }
            if crate::RCC_HEX_PINS[hex.input.to_bits() as usize].is_none() {
                return Err(Error::HexPinUnavailable);
            }
            Some(bounds)
        } else {
            None
        };
        let source = match self.sys {
            Sysclk::HSI => crate::rcc::ClockBounds::hsi(self.hsi.div.divisor()),
            Sysclk::HEX => hex.ok_or(Error::HexNotConfigured)?,
        };
        let clocks = Clocks {
            hsi,
            sys: source.nominal(),
            hclk: source.divided_by(self.ahb_pre.divisor()).nominal(),
            pclk: source
                .divided_by(self.ahb_pre.divisor() * self.apb_pre.divisor())
                .nominal(),
            dividers: [
                self.hsi.div.divisor(),
                self.ahb_pre.divisor(),
                self.ahb_pre.divisor() * self.apb_pre.divisor(),
            ],
            source,
            reserved_hex_inputs: self.hex.map_or(0, |hex| 1 << hex.input.to_bits()),
        };
        crate::rcc::operating::validate(self.operating_conditions, clocks)?;
        Ok(clocks)
    }
}
impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}

/// Nominal frequencies frozen after successful HAL initialization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Clocks {
    /// Divided HSI output, not the undivided HSIOSC frequency.
    pub hsi: Hertz,
    /// System clock mux output.
    pub sys: Hertz,
    /// CPU/AHB clock, including the flash interface.
    pub hclk: Hertz,
    /// APB clock before peripheral-local prescalers and muxes.
    pub pclk: Hertz,
    // Exact cumulative divisors; nominal display rounding must not weaken bounds.
    pub(crate) dividers: [u32; 3],
    pub(crate) source: crate::rcc::ClockBounds,
    // Frozen whole-boot ownership: system HEX and independent retained AWT pads.
    reserved_hex_inputs: u8,
}

/// A clock initialization failure.
///
/// Hardware errors may leave a partially changed clock tree. The initializer
/// never lowers flash latency before the requested clocks have been verified.
/// Calibration failures may leave the CPU on temporary LSI. Reset before retrying.
/// After a failure, no frequencies are published and peripherals must not be
/// used on the assumption that the requested configuration took effect.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {
    /// HEX system clock selected without a declaration.
    HexNotConfigured,
    /// External bounds are zero, inverted or exclude the nominal frequency.
    InvalidHexBounds,
    /// Actual HEX bounds extend beyond the own-source intersection 4–32 MHz.
    HexOutsideQualifiedRange,
    /// External qualification extends outside the own-device electrical range.
    HexConditionsOutsideQualifiedRange,
    /// External source qualification does not cover the declared board envelope.
    HexConditionsDoNotCoverBoard,
    /// The selected HEX pad is absent from the selected package projection.
    HexPinUnavailable,
    /// The selected input pad or its configuration gate did not acknowledge.
    HexPinConfigurationTimeout,
    /// HEX failed to stop and clear its startup-stable indication.
    HexStopTimeout,
    /// HEX did not report startup stability within the poll budget.
    HexTimeout,
    /// Active AWT prevents HEX reconfiguration or has an unproved ETR route.
    HexClockInUse,
    /// Active AWT or LVD filtering depends on HSIOSC that needs retuning.
    HsiClockInUse,
    /// Retained AWT's configuration gate could not be inspected/restored.
    RetainedClockInspectionTimeout,
    /// Active AWT uses a reserved source selector.
    InvalidRetainedClockSource,
    /// The selected inherited source does not report ready.
    InvalidEntryClock,

    /// The selected timer cannot divide this exact nominal PCLK to 1 MHz.
    #[cfg(feature = "_time-driver")]
    UnsupportedTimeDriverClock,
    /// The reserved timer clock/reset did not acknowledge initialization.
    #[cfg(feature = "_time-driver")]
    TimeDriverClockFailure,
    /// Declared minimum VDD exceeds the declared maximum.
    InvalidSupplyRange,
    /// Declared minimum ambient TA exceeds the declared maximum.
    InvalidTemperatureRange,
    /// Declared supply interval extends outside the factory-HSI qualification.
    SupplyOutsideQualifiedRange,
    /// Declared ambient interval extends outside the factory-HSI qualification.
    TemperatureOutsideQualifiedRange,
    /// Worst-case actual HCLK exceeds the declared-voltage bus ceiling.
    HclkTooHigh,
    /// Worst-case actual PCLK exceeds the declared-voltage bus ceiling.
    PclkTooHigh,
    /// A polling budget of zero is invalid.
    InvalidTimeout,
    /// The clock tree has already been frozen by a successful initialization.
    AlreadyInitialized,
    /// The incoming system clock selector is reserved.
    InvalidClockSource,
    /// The incoming HSI divider is not documented by the supported manuals.
    InvalidHsiDivider,
    /// Factory HSI calibration storage reads as erased (0xffff).
    InvalidCalibration,
    /// The FLASH configuration clock could not be enabled.
    FlashClockTimeout,
    /// FLASH WAIT did not read back as requested.
    FlashLatencyTimeout,
    /// HSI did not report stable within the poll budget.
    HsiTimeout,
    /// Temporary LSI did not stabilize within the poll budget.
    LsiTimeout,
    /// HSI did not stop before factory calibration could be installed.
    HsiStopTimeout,
    /// The temporary LSI software-enable request could not be restored.
    LsiRestoreTimeout,
    /// The system clock selector did not read back as the requested source.
    ClockSwitchTimeout,
    /// The requested HSI configuration or bus dividers did not read back.
    ClockConfigurationTimeout,
}

static CLOCKS: Mutex<Cell<Option<Clocks>>> = Mutex::new(Cell::new(None));

/// Get the frozen clock frequencies, or `None` before a successful HAL init.
pub fn try_clocks() -> Option<Clocks> {
    critical_section::with(|cs| CLOCKS.borrow(cs).get())
}

/// Get the frozen clock frequencies.
///
/// Panics before successful HAL initialization. Direct clock-register changes
/// after init invalidate these frequencies and are the caller's responsibility.
pub fn clocks() -> Clocks {
    try_clocks().expect("embassy-cw32 clocks are not initialized")
}

/// Configure once, before HAL drivers or application interrupts can use clocks.
///
/// Safety: the caller must own the device initialization sequence. No other
/// code, including DMA or NMI handlers, may change or depend on clock registers
/// during this call. Normal interrupts are masked for the complete sequence.
// Entry must be an electrically legal, stable HSI/HEX/LSI with documented HSI
// divider and appropriate Flash latency. The entry and any unchanged bridge
// sources must stay available. LSI used as a trim bridge must retain its own
// documented 32.8 kHz ±10% qualification. Unknown trim cannot be qualified by
// STABLE. This is not recovery from a failed source or runtime clock switching.
// Monotonic AHB >=/4 and APB /8 guards protect every legal incoming source and
// the retained HSI envelope before source changes; WAIT2 protects the bridge.
pub(crate) unsafe fn init(config: Config) -> Result<(), Error> {
    critical_section::with(|cs| {
        if CLOCKS.borrow(cs).get().is_some() {
            return Err(Error::AlreadyInitialized);
        }
        let clocks = configure(config, cs)?;
        CLOCKS.borrow(cs).set(Some(clocks));
        Ok(())
    })
}

// Waits remain bounded by the caller's poll budget; each predicate performs
// one read of the selected PAC register.
fn poll(mut ready: impl FnMut() -> bool, timeout: u32, error: Error) -> Result<(), Error> {
    for _ in 0..timeout {
        if ready() {
            return Ok(());
        }
        core::hint::spin_loop();
    }
    Err(error)
}

fn barrier() {
    #[cfg(target_arch = "arm")]
    {
        cortex_m::asm::dsb();
        cortex_m::asm::isb();
    }
}

fn set_flash_latency(wait: u32, timeout: u32) -> Result<(), Error> {
    // Bits 15:3 are reserved on this register version; preserve them, never
    // apply the F030 FETCH/CACHE bits. Only WAIT is programmable here.
    pac::FLASH.cr2().modify(|w| {
        w.set_key(0x5a5a);
        w.set_wait(wait as u8);
    });
    poll(
        || pac::FLASH.cr2().read().wait() == wait as u8,
        timeout,
        Error::FlashLatencyTimeout,
    )?;
    barrier();
    Ok(())
}

pub(crate) fn hex_pin_reserved(pin: u8) -> bool {
    try_clocks().is_some_and(|clocks| {
        crate::RCC_HEX_PINS
            .iter()
            .enumerate()
            .any(|(input, pad)| clocks.reserved_hex_inputs & (1 << input) != 0 && *pad == Some(pin))
    })
}

fn select_source(source: ClockSource, timeout: u32) -> Result<(), Error> {
    pac::SYSCTRL.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_sysclk(source);
    });
    poll(
        || pac::SYSCTRL.cr0().read().sysclk() == source,
        timeout,
        Error::ClockSwitchTimeout,
    )?;
    barrier();
    Ok(())
}

fn configure(config: Config, cs: critical_section::CriticalSection<'_>) -> Result<Clocks, Error> {
    let mut clocks = config.frequencies()?;
    let r = pac::SYSCTRL;
    let old_clock = r.cr0().read();
    let old_sources = r.cr1().read();
    let old_hex = r.hex().read();
    match old_clock.sysclk() {
        ClockSource::Hsi if r.hsi().read().stable() => {}
        ClockSource::Hex if old_sources.hexen() && old_hex.pinen() && old_hex.stable() => {}
        ClockSource::Lsi if r.lsi().read().stable() => {}
        ClockSource::Hsi | ClockSource::Hex | ClockSource::Lsi => {
            return Err(Error::InvalidEntryClock);
        }
        _ => return Err(Error::InvalidClockSource),
    }
    let old_hsi = r.hsi().read();
    if !matches!(old_hsi.div(), 5 | 6 | 8 | 9 | 11..=15) {
        return Err(Error::InvalidHsiDivider);
    }
    let factory =
        unsafe { core::ptr::read_volatile(crate::RCC_FACTORY_HSI_TRIM_ADDRESS as *const u16) };
    if factory == u16::MAX {
        return Err(Error::InvalidCalibration);
    }
    let mut calibrated = pac::sysctrl::regs::Hsi::default();
    calibrated.set_trim(factory);
    let trim = calibrated.trim();
    let needs_trim = old_hsi.trim() != trim;

    // AWT can use either pad independently of PINMUX and HEXEN, including in
    // DeepSleep. Always inspect it, even for HSI-only init with no HEX request.
    // Its APB gate controls configuration access, not whether its source is used.
    let awt = <crate::peripherals::AWT as crate::rcc::SealedRccPeripheral>::RCC_INFO
        .inspect_for_init(cs, config.timeout, || pac::AWT.cr().read())
        .map_err(|_| Error::RetainedClockInspectionTimeout)?;
    let mut awt_hex = false;
    if awt.en() {
        use pac::awt::vals::Source;
        match awt.src() {
            Source::Hsiosc if needs_trim => return Err(Error::HsiClockInUse),
            Source::Hsiosc | Source::Lsi => {}
            Source::HexPb0 => {
                awt_hex = true;
                clocks.reserved_hex_inputs |= 1;
            }
            Source::HexPb1 => {
                awt_hex = true;
                clocks.reserved_hex_inputs |= 2;
            }
            Source::Etr if config.hex.is_some() => return Err(Error::HexClockInUse),
            Source::Etr => {}
            _ => return Err(Error::InvalidRetainedClockSource),
        }
    }
    if needs_trim
        && pac::LVD.cr0().read().en()
        && pac::LVD.cr1().read().flten()
        && pac::LVD.cr1().read().fltclk()
    {
        return Err(Error::HsiClockInUse);
    }
    let preserve_hex = config.hex.is_some() && awt_hex;
    if let Some(hex) = config.hex.filter(|_| preserve_hex) {
        // Global HEX enable/input-gate writes are not proved harmless to AWT.
        // Only exact inherited enabled/ready/configured pad reuse is admitted.
        if !old_sources.hexen()
            || !old_hex.stable()
            || !old_hex.pinen()
            || old_hex.pinmux() != hex.input
            || !crate::rcc_hex_pin_matches(hex.input, config.timeout, cs)?
        {
            return Err(Error::HexClockInUse);
        }
    }
    if config.hex.is_none() && old_sources.hexen() {
        clocks.reserved_hex_inputs |= 1 << old_hex.pinmux().to_bits();
    }

    <crate::peripherals::FLASH as crate::rcc::SealedRccPeripheral>::RCC_INFO
        .enable_with_cs_readback(
            cs,
            crate::rcc::Readback::Poll {
                attempts: config.timeout,
                spin: true,
            },
        )
        .map_err(|_| Error::FlashClockTimeout)?;
    set_flash_latency(crate::RCC_INITIAL_FLASH_WAIT, config.timeout)?;
    let guard_hclk = old_clock.hclkprs().max(AHBPrescaler::Div4 as u8);
    let guard_pclk = old_clock.pclkprs().max(APBPrescaler::Div8 as u8);
    r.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_hclkprs(guard_hclk);
        w.set_pclkprs(guard_pclk);
    });
    poll(
        || {
            let v = r.cr0().read();
            v.hclkprs() == guard_hclk && v.pclkprs() == guard_pclk
        },
        config.timeout,
        Error::ClockConfigurationTimeout,
    )?;
    barrier();

    // Start the unchanged HSI before leaving the inherited source. CR1 bits
    // 15:4 and bit 2 are reserved here: no copied CCS, LSE or PLL writes.
    r.cr1().modify(|w| {
        w.set_key(0x5a5a);
        w.set_hsien(true);
    });
    poll(
        || r.cr1().read().hsien(),
        config.timeout,
        Error::ClockConfigurationTimeout,
    )?;
    poll(
        || r.hsi().read().stable(),
        config.timeout,
        Error::HsiTimeout,
    )?;
    select_source(ClockSource::Hsi, config.timeout)?;
    if needs_trim {
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_lsien(true);
        });
        poll(
            || r.cr1().read().lsien(),
            config.timeout,
            Error::ClockConfigurationTimeout,
        )?;
        poll(
            || r.lsi().read().stable(),
            config.timeout,
            Error::LsiTimeout,
        )?;
        select_source(ClockSource::Lsi, config.timeout)?;
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_hsien(false);
        });
        poll(
            || !r.cr1().read().hsien(),
            config.timeout,
            Error::HsiStopTimeout,
        )?;
        poll(
            || !r.hsi().read().stable(),
            config.timeout,
            Error::HsiStopTimeout,
        )?;
        r.hsi().modify(|w| w.set_trim(trim));
        poll(
            || r.hsi().read().trim() == trim,
            config.timeout,
            Error::ClockConfigurationTimeout,
        )?;
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_hsien(true);
        });
        poll(
            || r.cr1().read().hsien(),
            config.timeout,
            Error::ClockConfigurationTimeout,
        )?;
        poll(
            || r.hsi().read().stable(),
            config.timeout,
            Error::HsiTimeout,
        )?;
        select_source(ClockSource::Hsi, config.timeout)?;
        if !old_sources.lsien() {
            r.cr1().modify(|w| {
                w.set_key(0x5a5a);
                w.set_lsien(false);
            });
            poll(
                || !r.cr1().read().lsien(),
                config.timeout,
                Error::LsiRestoreTimeout,
            )?;
        }
    }
    r.hsi().modify(|w| w.set_div(config.hsi.div as u8));
    poll(
        || r.hsi().read().div() == config.hsi.div as u8,
        config.timeout,
        Error::ClockConfigurationTimeout,
    )?;
    poll(
        || r.hsi().read().stable(),
        config.timeout,
        Error::HsiTimeout,
    )?;

    if let Some(hex) = config.hex.filter(|_| !preserve_hex) {
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_hexen(false);
        });
        poll(
            || !r.cr1().read().hexen(),
            config.timeout,
            Error::HexStopTimeout,
        )?;
        poll(
            || !r.hex().read().stable(),
            config.timeout,
            Error::HexStopTimeout,
        )?;
        crate::rcc_configure_hex_pin(hex.input, config.timeout, cs)?;
        r.hex().modify(|w| {
            w.set_pinmux(hex.input);
            w.set_pinen(true);
        });
        poll(
            || {
                let v = r.hex().read();
                v.pinmux() == hex.input && v.pinen()
            },
            config.timeout,
            Error::ClockConfigurationTimeout,
        )?;
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_hexen(true);
        });
        poll(
            || r.cr1().read().hexen(),
            config.timeout,
            Error::ClockConfigurationTimeout,
        )?;
        poll(
            || r.hex().read().stable(),
            config.timeout,
            Error::HexTimeout,
        )?;
    }
    let source = match config.sys {
        Sysclk::HSI => ClockSource::Hsi,
        Sysclk::HEX => ClockSource::Hex,
    };
    select_source(source, config.timeout)?;
    // Weaken final buses only after the requested source is selected/read back.
    r.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_hclkprs(config.ahb_pre as u8);
        w.set_pclkprs(config.apb_pre as u8);
    });
    poll(
        || {
            let v = r.cr0().read();
            v.sysclk() == source
                && v.hclkprs() == config.ahb_pre as u8
                && v.pclkprs() == config.apb_pre as u8
        },
        config.timeout,
        Error::ClockConfigurationTimeout,
    )?;
    barrier();
    poll(
        || {
            let v = r.hsi().read();
            v.div() == config.hsi.div as u8 && v.trim() == trim
        },
        config.timeout,
        Error::ClockConfigurationTimeout,
    )?;
    poll(
        || r.hsi().read().stable(),
        config.timeout,
        Error::HsiTimeout,
    )?;
    if config.hex.is_some() {
        poll(
            || r.hex().read().stable(),
            config.timeout,
            Error::HexTimeout,
        )?;
    }
    set_flash_latency(
        (clocks.hclk_bounds().maximum().0 - 1) / crate::RCC_FLASH_WAIT_STEP_HZ,
        config.timeout,
    )?;
    if r.cr0().read().sysclk() != source {
        return Err(Error::ClockSwitchTimeout);
    }
    Ok(clocks)
}
