//! Verified classic CW32 12-bit ADC protocol.
//!
//! Supported families: F030/A030/F020, F002/F003, L031/R031/W031, L052/L083.
//! This driver uses Embassy peripheral ownership, sealed channels and borrowed
//! channel erasure. `blocking_read` performs one software-triggered conversion;
//! `blocking_read_sequence` performs one ordered software-triggered scan.
//! Async reads/scans use the dedicated ADC IRQ; DMA and hardware triggers are excluded.
//!
//! Sources: each family's own user manual, datasheet and SDK, pinned in
//! `docs/adc-classic-family-electrical-audit.md` and the qualified analog-route
//! sidecars. The x030/F020 baseline is independently qualified by their own
//! manuals/datasheets. Sampling takes 5/6/8/10 ADCCLK cycles followed by 19
//! successive-comparison cycles. EN starts the analog circuitry; READY must be
//! observed before conversion (approximately 40 us). No documented software ADC
//! calibration command exists. Temperature readings are raw counts; no factory
//! trims or physical-unit conversion are applied. F020 SDK-only 14-bit extension
//! registers are neither read nor written.
//!
//! The reference is VDDA or an internal 1.5/2.5 V source, except F002, which
//! supports VDDA only and has no temperature or bandgap measurement channel.
//! F002 supply/channel names are VDD/VDD3 in its own manual. External reference
//! operation is not exposed because that requires separate pin ownership.
//! Input voltages must satisfy the board's electrical limits and must not exceed
//! the selected reference. L/R/W operating tables also require VDDA=VDD.
//! Shared BGREN, where documented, is retained across configuration, failure
//! and drop, and a preexisting ADC clock gate is never disabled. The peripheral
//! is never reset. Other ADC reference/temperature controls remain exclusive;
//! do not concurrently change them through raw registers.
//! ADC limits and settling waits include the selected RCC source envelope.
//! The board must satisfy that source's declared voltage, temperature and
//! accuracy conditions throughout use. Rate-only envelopes are rejected by
//! checked constructors before ADC/RCC writes; use a cycle-qualified source.
//! R031 logical ADC_IN0..8 use hardware mux4..12; generated pin traits consume
//! separately qualified hardware-mux metadata rather than parsing signal names.

use core::marker::PhantomData;

use crate::interrupt::typelevel::{Binding, Handler, Interrupt};
use crate::mode::{Async, Blocking, Mode};
use crate::rcc::{ClockBounds, Readback, SealedRccPeripheral};
use crate::time::Hertz;
use crate::{Peri, PeripheralType, pac};
use core::future::poll_fn;
use core::sync::atomic::compiler_fence;
use core::sync::atomic::{AtomicBool, Ordering};
use core::task::Poll;
use embassy_sync::waitqueue::AtomicWaker;

/// Maximum number of ordered slots supported by the selected ADC.
/// Four on F002/F003/F020/F030/A030; eight on L031/R031/W031/L052/L083.
pub const MAX_SEQUENCE_LEN: usize = crate::ADC_SEQUENCE_MAX_LEN;

/// Fixed hardware conversion resolution. No unsupported resolutions are exposed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Resolution {
    /// 12 bits, returned right-aligned as 0 through 4095.
    Bits12,
}
impl Resolution {
    /// Number of conversion bits; effective analog accuracy is lower.
    pub const fn bits(self) -> u8 {
        12
    }
    /// Largest right-aligned conversion result.
    pub const fn max_count(self) -> u32 {
        4095
    }
}
/// Largest result at the selected resolution.
pub const fn resolution_to_max_count(resolution: Resolution) -> u32 {
    resolution.max_count()
}

/// Acquisition time, in ADCCLK cycles (CR0.SAM).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(u8)]
pub enum SampleTime {
    /// 5 acquisition cycles plus 19 conversion cycles.
    Cycles5 = 0,
    /// 6 acquisition cycles plus 19 conversion cycles.
    Cycles6 = 1,
    /// 8 acquisition cycles plus 19 conversion cycles.
    Cycles8 = 2,
    /// 10 acquisition cycles plus 19 conversion cycles.
    Cycles10 = 3,
}
impl SampleTime {
    /// Acquisition cycles.
    pub const fn cycles(self) -> u8 {
        crate::ADC_SAMPLE_CYCLES[self as usize] as u8
    }
    /// Total cycles for a single conversion, including acquisition.
    pub const fn conversion_cycles(self) -> u8 {
        self.cycles() + crate::ADC_COMPARISON_CYCLES as u8
    }
}

/// PCLK prescaler, with CW32 CR0.CLK encodings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(u8)]
pub enum Prescaler {
    /// PCLK / 1.
    Div1 = 0,
    /// PCLK / 2.
    Div2 = 1,
    /// PCLK / 4.
    Div4 = 2,
    /// PCLK / 8.
    Div8 = 3,
    /// PCLK / 16.
    Div16 = 4,
    /// PCLK / 32.
    Div32 = 5,
    /// PCLK / 64.
    Div64 = 6,
    /// PCLK / 128.
    Div128 = 7,
}
impl Prescaler {
    /// Numeric division factor.
    pub const fn divisor(self) -> u32 {
        1 << self as u8
    }
}

/// Conversion reference. Internal references require adequate VDDA headroom.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(u8)]
pub enum Reference {
    /// Internal 1.5 V reference; VDDA must be at least 1.8 V.
    #[cfg(not(adc_cw32f002_v1))]
    Internal1V5 = 0,
    /// Internal 2.5 V reference; VDDA must be at least 2.8 V.
    #[cfg(not(adc_cw32f002_v1))]
    Internal2V5 = 1,
    /// Analog supply voltage.
    Vdda = 3,
}

/// ADC configuration, separate from per-read acquisition time.
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub struct Config {
    /// Reference voltage source.
    pub reference: Reference,
    /// Lowest guaranteed board VDDA in millivolts, including supply tolerance.
    ///
    /// Must be 1650..=5500, except R031: 2200..=3600, W031: 1800..=3600.
    /// W031 RF DCDC operation separately requires at least 2000 mV.
    /// Clock limits are selected conservatively from the selected family
    /// manual. This is a board declaration, not a measured voltage. The board
    /// must independently remain below the maximum supply and pin ratings.
    pub vdda_mv: u16,
    /// Requested maximum ADCCLK, not the sample rate. The fastest PCLK divisor
    /// meeting this limit and the voltage/reference/channel limits is selected.
    pub frequency: Hertz,
    /// Enable the input follower for high-impedance external sources. The
    /// follower is always enabled for internal channels and limits throughput
    /// to 200,000 samples/second. Check the source impedance and settling time.
    pub input_buffer: bool,
    /// Maximum status-register polls for each startup/conversion wait. Nonzero.
    /// This is a finite iteration budget, not a time in microseconds.
    pub timeout: u32,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            reference: Reference::Vdda,
            vdda_mv: crate::ADC_SUPPLY_RANGE_MV.0,
            frequency: Hertz(500_000),
            input_buffer: false,
            timeout: 100_000,
        }
    }
}

/// Configuration, startup or conversion failure.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {
    /// HAL clock initialization has not completed successfully.
    ClockNotInitialized,
    /// A rate-only source envelope does not qualify strict ADC acquisition and
    /// conversion duration bounds. Rejected before acquiring the ADC gate.
    #[cfg(any(rcc_pll, rcc_lsi_sysclk))]
    UnqualifiedCycleTiming,
    /// The inherited ADC reset is asserted; this driver never changes it.
    ResetAsserted,
    /// The ADC clock gate did not acknowledge within the polling budget.
    ClockEnableTimeout,
    /// A sequence must contain one through MAX_SEQUENCE_LEN slots.
    InvalidSequenceLength,
    /// The result slice must contain exactly one element per slot.
    InvalidResultLength,
    /// Classic ADCs have one common acquisition setting for the entire scan.
    InconsistentSampleTime,
    /// Buffered or internal-source multi-slot scans are not source-qualified.
    UnsupportedScanChannel,
    /// Declared VDDA is outside the selected family operating range.
    InvalidSupplyVoltage,
    /// The selected internal reference needs a higher minimum VDDA.
    ReferenceSupplyTooLow,
    /// Requested ADCCLK must be nonzero.
    FrequencyZero,
    /// Even PCLK/128 exceeds a requested or channel-specific clock limit.
    FrequencyTooLow,
    /// Polling budget must be nonzero.
    InvalidTimeout,
    /// ADC analog initialization did not become ready within the budget.
    ReadyTimeout,
    /// A conversion did not complete and stop within the budget.
    ConversionTimeout,
    /// Inherited trigger, watchdog, DMA or accumulation state is unsupported.
    UnsupportedConfiguration,
    /// Required local control readback failed; terminal until chip reset.
    Faulted,
    /// Hardware reported unread-result overwrite; this sample is discarded.
    Overrun,
}
impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::ClockNotInitialized => "ADC clocks are not initialized",
            #[cfg(any(rcc_pll, rcc_lsi_sysclk))]
            Self::UnqualifiedCycleTiming => {
                "ADC cycle timing is not qualified for rate-only clocks"
            }
            Self::ResetAsserted => "ADC reset is asserted",
            Self::ClockEnableTimeout => "ADC clock enable timed out",
            Self::InvalidSequenceLength => "ADC sequence length is outside the hardware range",
            Self::InvalidResultLength => "ADC result length does not match sequence length",
            Self::InconsistentSampleTime => "ADC scan slots require the same sample time",
            Self::UnsupportedScanChannel => {
                "ADC multi-slot scans require unbuffered external channels"
            }
            Self::InvalidSupplyVoltage => "ADC VDDA is outside the selected family operating range",
            Self::ReferenceSupplyTooLow => "ADC internal reference requires a higher VDDA",
            Self::FrequencyZero => "ADC clock limit must be nonzero",
            Self::FrequencyTooLow => "ADC clock limit is below PCLK/128",
            Self::InvalidTimeout => "ADC polling budget must be nonzero",
            Self::ReadyTimeout => "ADC analog startup timed out",
            Self::ConversionTimeout => "ADC conversion timed out",
            Self::UnsupportedConfiguration => "ADC inherited configuration is unsupported",
            Self::Faulted => "ADC cleanup failed; chip reset is required",
            Self::Overrun => "ADC result overrun",
        })
    }
}
impl core::error::Error for Error {}

/// Effective nominal timing for the most recent configuration/conversion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Timing {
    /// Divisor selected for this channel.
    pub prescaler: Prescaler,
    /// PCLK divided by the prescaler, rounded down to whole hertz.
    pub frequency: Hertz,
    /// Source-qualified ADC clock envelope, with exact prescaler arithmetic.
    pub clock_bounds: ClockBounds,
    /// Acquisition cycles, followed by 19 comparison cycles.
    pub sample_time: SampleTime,
}
impl Timing {
    /// Guaranteed minimum acquisition in ns, rounded down. Requires the clock
    /// source's documented operating conditions and temperature range.
    pub fn minimum_acquisition_time_ns(self) -> u64 {
        self.clock_bounds
            .minimum_duration_ns(u32::from(self.sample_time.cycles()))
    }
    /// Worst-case conversion duration in ns, rounded up at the slowest qualified source.
    /// Excludes startup, source settling, trigger latency and software overhead.
    /// The configured timeout remains an iteration count, not this duration.
    pub fn maximum_conversion_time_ns(self) -> u64 {
        self.clock_bounds
            .maximum_duration_ns(u32::from(self.sample_time.conversion_cycles()))
    }
    /// Nominal conversion time in nanoseconds, rounded up. Does not include
    /// startup, software overhead or internal-channel settling waits.
    pub fn conversion_time_ns(self) -> u64 {
        (u64::from(self.sample_time.conversion_cycles()) * 1_000_000_000)
            .div_ceil(u64::from(self.frequency.0))
    }
}

/// Common clock and conversion duration of one completed ordered scan.
/// Bounds exclude startup, settling, software and any between-slot overhead.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SequenceTiming {
    /// One divider shared by all slots.
    pub prescaler: Prescaler,
    /// ADC clock envelope including selected-source tolerance.
    pub clock_bounds: ClockBounds,
    /// Number of results in the supplied order.
    pub length: u8,
    /// Sum of acquisition and successive-comparison cycles for all slots.
    pub conversion_cycles: u32,
}
impl SequenceTiming {
    /// Maximum duration of conversion cycles at the slowest qualified clock.
    pub fn maximum_conversion_time_ns(self) -> u64 {
        self.clock_bounds
            .maximum_duration_ns(self.conversion_cycles)
    }
}

impl Config {
    fn clock_limit(&self) -> Result<u32, Error> {
        if self.timeout == 0 {
            return Err(Error::InvalidTimeout);
        }
        if self.frequency.0 == 0 {
            return Err(Error::FrequencyZero);
        }
        if !(crate::ADC_SUPPLY_RANGE_MV.0..=crate::ADC_SUPPLY_RANGE_MV.1).contains(&self.vdda_mv) {
            return Err(Error::InvalidSupplyVoltage);
        }
        let bands = match self.reference {
            #[cfg(not(adc_cw32f002_v1))]
            Reference::Internal1V5 => crate::ADC_INTERNAL_1V5_BANDS,
            #[cfg(not(adc_cw32f002_v1))]
            Reference::Internal2V5 => crate::ADC_INTERNAL_2V5_BANDS,
            Reference::Vdda => crate::ADC_SUPPLY_BANDS,
        };
        let maximum = bands
            .iter()
            .rev()
            .find(|b| self.vdda_mv >= b.0)
            .ok_or(Error::ReferenceSupplyTooLow)?
            .1;
        Ok(maximum.min(self.frequency.0))
    }

    fn timing(
        &self,
        pclk: ClockBounds,
        channel: u8,
        sample_time: SampleTime,
    ) -> Result<Timing, Error> {
        #[cfg(any(rcc_pll, rcc_lsi_sysclk))]
        if !pclk.has_cycle_timing_bounds() {
            return Err(Error::UnqualifiedCycleTiming);
        }
        let mut maximum = self.clock_limit()?;
        if self.input_buffer || channel >= crate::ADC_FIRST_INTERNAL_CHANNEL {
            maximum = maximum.min(
                u32::from(sample_time.conversion_cycles())
                    * crate::ADC_INPUT_FOLLOWER_MAXIMUM_RATE_HZ,
            );
        }
        // All qualified datasheets require >=5 us temperature acquisition.
        if Some(channel) == crate::ADC_TEMPERATURE_CHANNEL {
            maximum = maximum.min(
                u32::from(sample_time.cycles())
                    * crate::ADC_TEMPERATURE_ACQUISITION_MAXIMUM_RATE_HZ,
            );
        }
        if pclk.nominal().0 == 0 {
            return Err(Error::ClockNotInitialized);
        }
        for prescaler in [
            Prescaler::Div1,
            Prescaler::Div2,
            Prescaler::Div4,
            Prescaler::Div8,
            Prescaler::Div16,
            Prescaler::Div32,
            Prescaler::Div64,
            Prescaler::Div128,
        ] {
            // Compare before division; truncating PCLK/divisor can otherwise
            // accept a clock fractionally above a hardware limit.
            if !pclk
                .divided_by(prescaler.divisor())
                .maximum_exceeds(maximum)
            {
                let clock_bounds = pclk.divided_by(prescaler.divisor());
                let frequency = clock_bounds.nominal();
                if frequency.0 == 0 {
                    return Err(Error::FrequencyTooLow);
                }
                return Ok(Timing {
                    prescaler,
                    frequency,
                    clock_bounds,
                    sample_time,
                });
            }
        }
        Err(Error::FrequencyTooLow)
    }
}

/// Exclusive ADC conversion owner. Pins are borrowed for a conversion and
/// remain analog afterwards. Shared BGREN and an inherited clock gate remain
/// enabled on drop; other owned analog controls are shut down.
///
/// Validation leaves ADC state and output buffers unchanged. Timeouts and
/// overruns stop and disable conversion; subsequent reads retry initialization.
pub struct Adc<'d, T: Instance, M: Mode = Blocking> {
    _peripheral: Peri<'d, T>,
    config: Config,
    pclk: ClockBounds,
    hclk: ClockBounds,
    timing: Timing,
    active: bool,
    async_lifecycle: bool,
    clock_was_enabled: bool,
    _mode: PhantomData<M>,
}
impl<'d, T: Instance> Adc<'d, T, Blocking> {
    /// Construct a blocking ADC, panicking on configuration/startup failure.
    pub fn new(peripheral: Peri<'d, T>, config: Config) -> Self {
        Self::try_new(peripheral, config)
            .unwrap_or_else(|error| panic!("ADC initialization failed: {}", error))
    }
    /// Explicit blocking constructor.
    pub fn new_blocking(peripheral: Peri<'d, T>, config: Config) -> Self {
        Self::new(peripheral, config)
    }
    /// Validate configuration, acquire the gate without reset, then await READY.
    /// Tokens are consumed on failure; reborrow them when reuse is required.
    pub fn try_new(peripheral: Peri<'d, T>, config: Config) -> Result<Self, Error> {
        check_fault::<T>()?;
        let clocks = crate::rcc::try_clocks().ok_or(Error::ClockNotInitialized)?;
        let timing = config.timing(clocks.pclk_bounds(), 0, SampleTime::Cycles10)?;
        let clock_was_enabled = critical_section::with(|cs| {
            let rcc = crate::peripherals::ADC::RCC_INFO;
            if rcc.reset_asserted() {
                return Err(Error::ResetAsserted);
            }
            let was_enabled = rcc.is_enabled();
            if rcc
                .enable_with_cs_readback(
                    cs,
                    Readback::Poll {
                        attempts: config.timeout,
                        spin: false,
                    },
                )
                .is_err()
            {
                if !was_enabled {
                    let _ = rcc.disable_with_cs(cs);
                }
                return Err(Error::ClockEnableTimeout);
            }
            Ok(was_enabled)
        })?;
        let mut adc = Self {
            _peripheral: peripheral,
            config,
            pclk: clocks.pclk_bounds(),
            hclk: clocks.hclk_bounds(),
            timing,
            active: false,
            async_lifecycle: false,
            clock_was_enabled,
            _mode: PhantomData,
        };
        adc.initialize()?;
        Ok(adc)
    }
    /// Fallible explicit blocking constructor.
    pub fn try_new_blocking(peripheral: Peri<'d, T>, config: Config) -> Result<Self, Error> {
        Self::try_new(peripheral, config)
    }
    /// Read one verified external pin or internal source with finite polling.
    /// GPIO preparation precedes timing validation. Internal channels use the
    /// follower and their source-qualified startup/acquisition constraints.
    pub fn blocking_read<'a>(
        &mut self,
        channel: impl BorrowedChannel<'a, T>,
        sample_time: SampleTime,
    ) -> Result<u16, Error> {
        let channel = channel.reborrow_adc();
        let mut result = [0];
        self.blocking_read_sequence(&[(&channel, sample_time)], &mut result)?;
        Ok(result[0])
    }
    /// Perform one scan in the supplied slot order, including repeated muxes.
    ///
    /// Every slot must request the same sample time: classic SAM, CLK, BUF and
    /// REF apply to the whole scan. Multi-slot operation is qualified only for
    /// external inputs with `Config::input_buffer == false`; internal inputs
    /// and buffered external inputs remain available as single reads.
    ///
    /// Obtain sealed handles with `degrade_adc` or `reborrow_adc`. A handle may
    /// be referenced by several slots without duplicating pin ownership. The
    /// result slice must match the slot count exactly. Samples are sequential.
    /// All validation precedes ADC writes. Results are copied only after EOS
    /// (EOC for one slot), no overrun and hardware-cleared START. A failure
    /// leaves the entire caller buffer unchanged. Startup/conversion failures
    /// stop and power down owned analog controls, preserving shared BGREN and
    /// an inherited gate. Validation errors leave the ADC state unchanged.
    /// The timeout budget covers the whole scan, not each individual slot.
    pub fn blocking_read_sequence(
        &mut self,
        sequence: &[(&BorrowedAdcChannel<'_, T>, SampleTime)],
        results: &mut [u16],
    ) -> Result<SequenceTiming, Error> {
        check_fault::<T>()?;
        if sequence.is_empty() || sequence.len() > MAX_SEQUENCE_LEN {
            return Err(Error::InvalidSequenceLength);
        }
        if results.len() != sequence.len() {
            return Err(Error::InvalidResultLength);
        }
        let sample_time = sequence[0].1;
        let mut prescaler = Prescaler::Div1;
        let mut temperature = false;
        let mut bandgap = false;
        let mut internal = false;
        for &(channel, sample) in sequence {
            if sample != sample_time {
                return Err(Error::InconsistentSampleTime);
            }
            internal |= channel.channel >= crate::ADC_FIRST_INTERNAL_CHANNEL;
            temperature |= Some(channel.channel) == crate::ADC_TEMPERATURE_CHANNEL;
            bandgap |= Some(channel.channel) == crate::ADC_BANDGAP_CHANNEL;
            let timing = self
                .config
                .timing(self.pclk, channel.channel, sample_time)?;
            if timing.prescaler as u8 > prescaler as u8 {
                prescaler = timing.prescaler;
            }
        }
        let scan = sequence.len() > 1;
        if scan
            && ((internal && crate::ADC_INTERNAL_REQUIRES_SINGLE_CHANNEL)
                || (self.config.input_buffer && crate::ADC_BUFFERED_REQUIRES_SINGLE_CHANNEL))
        {
            return Err(Error::UnsupportedScanChannel);
        }
        let clock_bounds = self.pclk.divided_by(prescaler.divisor());
        let timing = SequenceTiming {
            prescaler,
            clock_bounds,
            length: sequence.len() as u8,
            conversion_cycles: u32::from(sample_time.conversion_cycles()) * sequence.len() as u32,
        };
        if !self.active {
            self.initialize()?;
        }
        let regs = T::regs();
        regs.start().write(|v| v.set_start(false));
        self.drain_results();
        self.clear_all_flags();
        regs.cr1().write(|v| v.set_chmux(sequence[0].0.channel));
        // These are different authored register versions, not a layout cast.
        #[cfg(any(adc_v1, adc_cw32f020_v1, adc_cw32f002_v1, adc_cw32f003_v1))]
        regs.sqr().write(|v| {
            v.set_ens(sequence.len() as u8 - 1);
            for (slot, &(channel, _)) in sequence
                .iter()
                .take(crate::ADC_SEQUENCE_SLOTS_PER_REGISTER)
                .enumerate()
            {
                v.set_sqr(slot, channel.channel);
            }
        });
        #[cfg(any(adc_cw32l031_v1, adc_cw32l052_v1, adc_cw32l083_v1))]
        {
            regs.sqr0().write(|v| {
                v.set_ens(sequence.len() as u8 - 1);
                for (slot, &(channel, _)) in sequence
                    .iter()
                    .take(crate::ADC_SEQUENCE_SLOTS_PER_REGISTER)
                    .enumerate()
                {
                    v.set_sqr(slot, channel.channel);
                }
            });
            regs.sqr1().write(|v| {
                for (slot, &(channel, _)) in sequence
                    .iter()
                    .skip(crate::ADC_SEQUENCE_SLOTS_PER_REGISTER)
                    .enumerate()
                {
                    v.set_sqr(slot, channel.channel);
                }
            });
        }
        let temperature_needs_settling = critical_section::with(|_| {
            #[cfg(not(adc_cw32f002_v1))]
            let before = regs.cr0().read();
            regs.cr0().modify(|v| {
                v.set_en(true);
                v.set_mode(if scan {
                    pac::adc::vals::Mode::Scan
                } else {
                    pac::adc::vals::Mode::Single
                });
                v.set_clk(prescaler as u8);
                v.set_sam(sample_time as u8);
                v.set_buf(self.config.input_buffer || internal);
                #[cfg(not(adc_cw32f002_v1))]
                v.set_tsen(before.tsen() || temperature);
                #[cfg(any(adc_v1, adc_cw32f020_v1, adc_cw32f003_v1))]
                v.set_bgren(
                    before.bgren()
                        || temperature
                        || bandgap
                        || self.config.reference != Reference::Vdda,
                );
            });
            temperature && {
                #[cfg(not(adc_cw32f002_v1))]
                {
                    !before.tsen() || !before.buf()
                }
                #[cfg(adc_cw32f002_v1)]
                {
                    false
                }
            }
        });
        self.timing = Timing {
            prescaler,
            frequency: clock_bounds.nominal(),
            clock_bounds,
            sample_time,
        };
        if temperature_needs_settling {
            self.delay_us(crate::ADC_TEMPERATURE_STARTUP_US);
        } else if bandgap {
            // An inherited BGR may have been enabled just before this call.
            self.delay_us(crate::ADC_BANDGAP_STARTUP_US);
        }
        self.wait_ready()?;
        regs.start().write(|v| v.set_start(true));
        for _ in 0..self.config.timeout {
            let status = regs.isr().read();
            if status.ovw() {
                self.shutdown();
                return Err(Error::Overrun);
            }
            let complete = if scan { status.eos() } else { status.eoc() };
            if complete && !regs.start().read().start() {
                for (slot, result) in results.iter_mut().enumerate() {
                    *result =
                        regs.result(slot).read().result() & Resolution::Bits12.max_count() as u16;
                }
                self.clear_all_flags();
                return Ok(timing);
            }
        }
        self.shutdown();
        Err(Error::ConversionTimeout)
    }
    /// Obtain a raw internal temperature channel (not degrees Celsius).
    #[cfg(not(adc_cw32f002_v1))]
    pub fn enable_temperature(&mut self) -> Temperature<T> {
        Temperature(PhantomData)
    }
    /// Obtain the nominal 1.2 V internal reference measurement channel.
    #[cfg(not(adc_cw32f002_v1))]
    pub fn enable_vrefint(&mut self) -> VrefInt<T> {
        VrefInt(PhantomData)
    }
    /// Obtain the internal analog supply divided by three channel.
    pub fn enable_vdda(&mut self) -> Vdda<T> {
        Vdda(PhantomData)
    }
    /// Validate and apply configuration with fresh bounded analog startup.
    /// Validation errors preserve the current ADC state.
    pub fn set_config(&mut self, config: &Config) -> Result<(), Error> {
        check_fault::<T>()?;
        let timing = config.timing(self.pclk, 0, SampleTime::Cycles10)?;
        self.shutdown();
        self.config = *config;
        self.timing = timing;
        self.initialize()
    }
    /// Effective clock and common acquisition timing of the last conversion.
    pub fn timing(&self) -> Timing {
        self.timing
    }
    /// Fixed hardware resolution.
    pub fn resolution(&self) -> Resolution {
        Resolution::Bits12
    }
}
impl<'d, T: Instance> Adc<'d, T, Async> {
    /// Construct an interrupt-driven software ADC; panic on setup failure.
    /// The IRQ must be bound to `InterruptHandler<T>` with `bind_interrupts!`.
    pub fn new_async(
        peripheral: Peri<'d, T>,
        irqs: impl Binding<T::Interrupt, InterruptHandler<T>> + 'd,
        config: Config,
    ) -> Self {
        Self::try_new_async(peripheral, irqs, config)
            .unwrap_or_else(|error| panic!("ADC initialization failed: {}", error))
    }

    /// Validate before acquiring the gate, preserving shared BGR and its clock.
    /// Inherited hardware triggers or watchdog enables are rejected without
    /// changing their configuration or flags. Raw PAC code must not concurrently
    /// reconfigure this exclusively owned ADC. A prior cleanup fault is terminal
    /// across both blocking and asynchronous replacement owners until chip reset.
    pub fn try_new_async(
        peripheral: Peri<'d, T>,
        _irqs: impl Binding<T::Interrupt, InterruptHandler<T>> + 'd,
        config: Config,
    ) -> Result<Self, Error> {
        check_fault::<T>()?;
        let clocks = crate::rcc::try_clocks().ok_or(Error::ClockNotInitialized)?;
        let timing = config.timing(clocks.pclk_bounds(), 0, SampleTime::Cycles10)?;
        // Keep gate acquisition, inherited-source rejection and initial masking
        // atomic with respect to a previously enabled/pending ADC vector.
        let mut adc = critical_section::with(|cs| {
            let clock_was_enabled = clock_acquire(config.timeout, cs)?;
            if inherited_async_configuration::<T>() {
                clock_release(clock_was_enabled, cs);
                return Err(Error::UnsupportedConfiguration);
            }
            let mut adc = Self {
                _peripheral: peripheral,
                config,
                pclk: clocks.pclk_bounds(),
                hclk: clocks.hclk_bounds(),
                timing,
                active: false,
                clock_was_enabled,
                async_lifecycle: true,
                _mode: PhantomData,
            };
            adc.async_cleanup()?;
            Ok(adc)
        })?;
        adc.initialize_async()?;
        T::state().irq_active.store(true, Ordering::Release);
        // Dedicated vector proven by the metadata generator. No pending clear
        // is needed: the handler ignores masked sources and preserves sticky flags.
        unsafe { T::Interrupt::enable() };
        Ok(adc)
    }

    /// Read one borrowed channel using EOC and OVW interrupts.
    /// `Config::timeout` does not impose a deadline while waiting for completion.
    /// Wrap this future in `with_timeout` or `select` for caller cancellation.
    pub async fn read<'a>(
        &mut self,
        channel: impl BorrowedChannel<'a, T>,
        sample_time: SampleTime,
    ) -> Result<u16, Error> {
        let channel = channel.reborrow_adc();
        let mut result = [0];
        self.read_sequence(&[(&channel, sample_time)], &mut result)
            .await?;
        Ok(result[0])
    }

    /// Read one finite software sequence with EOC (single) or EOS (scan) wake.
    /// All slots share SAM/CLK/BUF/REF and retain the blocking electrical limits.
    /// OVW takes precedence over completion. The ISR never holds a caller pointer.
    /// Output changes only after every result and final cleanup check succeed.
    ///
    /// Dropping stops and disables the local converter. Reuse reinitializes EN
    /// and programs the complete sequence; classic START=0 does not document a
    /// cursor reset or an independent drain acknowledgment. Forgetting skips
    /// cleanup, so every later operation establishes fresh state explicitly.
    /// Failed control readback is terminal until chip reset. Config::timeout
    /// bounds synchronous checks only; compose with with_timeout/select for an
    /// elapsed-time deadline and a functioning caller time/wake source.
    pub async fn read_sequence(
        &mut self,
        sequence: &[(&BorrowedAdcChannel<'_, T>, SampleTime)],
        results: &mut [u16],
    ) -> Result<SequenceTiming, Error> {
        check_fault::<T>()?;
        if sequence.is_empty() || sequence.len() > MAX_SEQUENCE_LEN {
            return Err(Error::InvalidSequenceLength);
        }
        if results.len() != sequence.len() {
            return Err(Error::InvalidResultLength);
        }
        let sample_time = sequence[0].1;
        let mut prescaler = Prescaler::Div1;
        let mut temperature = false;
        let mut bandgap = false;
        let mut internal = false;
        for &(channel, sample) in sequence {
            if sample != sample_time {
                return Err(Error::InconsistentSampleTime);
            }
            internal |= channel.channel >= crate::ADC_FIRST_INTERNAL_CHANNEL;
            temperature |= Some(channel.channel) == crate::ADC_TEMPERATURE_CHANNEL;
            bandgap |= Some(channel.channel) == crate::ADC_BANDGAP_CHANNEL;
            let timing = self
                .config
                .timing(self.pclk, channel.channel, sample_time)?;
            if timing.prescaler as u8 > prescaler as u8 {
                prescaler = timing.prescaler;
            }
        }
        let scan = sequence.len() > 1;
        if scan
            && ((internal && crate::ADC_INTERNAL_REQUIRES_SINGLE_CHANNEL)
                || (self.config.input_buffer && crate::ADC_BUFFERED_REQUIRES_SINGLE_CHANNEL))
        {
            return Err(Error::UnsupportedScanChannel);
        }
        let clock_bounds = self.pclk.divided_by(prescaler.divisor());
        let timing = SequenceTiming {
            prescaler,
            clock_bounds,
            length: sequence.len() as u8,
            conversion_cycles: u32::from(sample_time.conversion_cycles()) * sequence.len() as u32,
        };
        // This also repairs a forgotten future; never rely on its destructor.
        self.async_cleanup()?;
        self.timing = Timing {
            prescaler,
            frequency: clock_bounds.nominal(),
            clock_bounds,
            sample_time,
        };
        self.initialize_async()?;
        let regs = T::regs();
        // Preserve inactive watchdog channel selection and thresholds.
        regs.cr1().modify(|v| {
            v.set_chmux(sequence[0].0.channel);
            v.set_align(false);
            v.set_discard(false);
        });
        // These are different authored register versions, not a layout cast.
        #[cfg(any(adc_v1, adc_cw32f020_v1, adc_cw32f002_v1, adc_cw32f003_v1))]
        regs.sqr().write(|v| {
            v.set_ens(sequence.len() as u8 - 1);
            for (slot, &(channel, _)) in sequence
                .iter()
                .take(crate::ADC_SEQUENCE_SLOTS_PER_REGISTER)
                .enumerate()
            {
                v.set_sqr(slot, channel.channel);
            }
        });
        #[cfg(any(adc_cw32l031_v1, adc_cw32l052_v1, adc_cw32l083_v1))]
        {
            regs.sqr0().write(|v| {
                v.set_ens(sequence.len() as u8 - 1);
                for (slot, &(channel, _)) in sequence
                    .iter()
                    .take(crate::ADC_SEQUENCE_SLOTS_PER_REGISTER)
                    .enumerate()
                {
                    v.set_sqr(slot, channel.channel);
                }
            });
            regs.sqr1().write(|v| {
                for (slot, &(channel, _)) in sequence
                    .iter()
                    .skip(crate::ADC_SEQUENCE_SLOTS_PER_REGISTER)
                    .enumerate()
                {
                    v.set_sqr(slot, channel.channel);
                }
            });
        }
        critical_section::with(|_| {
            regs.cr0().modify(|v| {
                v.set_mode(if scan {
                    pac::adc::vals::Mode::Scan
                } else {
                    pac::adc::vals::Mode::Single
                });
                v.set_buf(self.config.input_buffer || internal);
                #[cfg(not(adc_cw32f002_v1))]
                v.set_tsen(temperature);
                #[cfg(any(adc_v1, adc_cw32f020_v1, adc_cw32f003_v1))]
                v.set_bgren(
                    v.bgren() || temperature || bandgap || self.config.reference != Reference::Vdda,
                );
            });
        });
        if !self.async_poll_control(|| {
            let cr = regs.cr0().read();
            let cr1 = regs.cr1().read();
            let controls = cr.en()
                && cr.mode()
                    == if scan {
                        pac::adc::vals::Mode::Scan
                    } else {
                        pac::adc::vals::Mode::Single
                    }
                && cr.clk() == prescaler as u8
                && cr.sam() == sample_time as u8
                && cr.ref_() == self.config.reference as u8
                && cr.buf() == (self.config.input_buffer || internal)
                && cr1.chmux() == sequence[0].0.channel
                && !cr1.align()
                && !cr1.discard();
            #[cfg(not(adc_cw32f002_v1))]
            let controls = controls && cr.tsen() == temperature;
            #[cfg(any(adc_v1, adc_cw32f020_v1, adc_cw32f003_v1))]
            let controls = controls
                && (!(temperature || bandgap || self.config.reference != Reference::Vdda)
                    || cr.bgren());
            #[cfg(any(adc_v1, adc_cw32f020_v1, adc_cw32f002_v1, adc_cw32f003_v1))]
            let slots = {
                let sqr = regs.sqr().read();
                sqr.ens() == sequence.len() as u8 - 1
                    && sequence
                        .iter()
                        .enumerate()
                        .all(|(slot, &(channel, _))| sqr.sqr(slot) == channel.channel)
            };
            #[cfg(any(adc_cw32l031_v1, adc_cw32l052_v1, adc_cw32l083_v1))]
            let slots = {
                let sqr0 = regs.sqr0().read();
                let sqr1 = regs.sqr1().read();
                sqr0.ens() == sequence.len() as u8 - 1
                    && sequence.iter().enumerate().all(|(slot, &(channel, _))| {
                        if slot < crate::ADC_SEQUENCE_SLOTS_PER_REGISTER {
                            sqr0.sqr(slot) == channel.channel
                        } else {
                            sqr1.sqr(slot - crate::ADC_SEQUENCE_SLOTS_PER_REGISTER)
                                == channel.channel
                        }
                    })
            };
            controls && slots
        }) {
            let _ = self.async_cleanup();
            return Err(self.async_fault());
        }
        if temperature {
            self.delay_us(crate::ADC_TEMPERATURE_STARTUP_US);
        } else if bandgap {
            self.delay_us(crate::ADC_BANDGAP_STARTUP_US);
        }
        // Recheck READY after common source and buffer programming.
        if !self.async_poll_control(|| regs.isr().read().ready()) {
            self.async_cleanup()?;
            return Err(Error::ReadyTimeout);
        }
        if let Err(error) = self.async_clear_completion() {
            let _ = self.async_cleanup();
            return Err(error);
        }
        let mut guard = AsyncGuard {
            adc: self,
            armed: true,
        };
        let mut started = false;
        poll_fn(|cx| {
            T::state().waker.register(cx.waker());
            compiler_fence(Ordering::SeqCst);
            if let Err(error) = check_fault::<T>() {
                return Poll::Ready(Err(error));
            }
            if !started {
                critical_section::with(|_| {
                    regs.ier().modify(|v| {
                        v.set_eoc(!scan);
                        v.set_eos(scan);
                        v.set_ovw(true);
                    })
                });
                if !guard.adc.async_poll_control(|| {
                    let ier = regs.ier().read();
                    ier.eos() == scan && ier.eoc() != scan && ier.ovw()
                }) {
                    return Poll::Ready(Err(guard.adc.async_fault()));
                }
                compiler_fence(Ordering::SeqCst);
                regs.start().write(|v| v.set_start(true));
                started = true;
            }
            let status = regs.isr().read();
            if status.ovw() {
                Poll::Ready(Err(Error::Overrun))
            } else if if scan { status.eos() } else { status.eoc() } {
                Poll::Ready(Ok(()))
            } else {
                Poll::Pending
            }
        })
        .await?;
        // Completion is sticky and its interrupt is now masked. Never await a second
        // wake if START lags completion: finalize with this finite synchronous poll.
        let mut complete = false;
        for _ in 0..guard.adc.config.timeout {
            let status = regs.isr().read();
            if status.ovw() {
                return Err(Error::Overrun);
            }
            let terminal = if scan { status.eos() } else { status.eoc() };
            if terminal && !regs.start().read().start() {
                complete = true;
                break;
            }
        }
        if !complete {
            let cleanup = guard.adc.async_cleanup();
            guard.armed = false;
            cleanup?;
            return Err(Error::ConversionTimeout);
        }
        compiler_fence(Ordering::SeqCst);
        // Retained results cannot be overwritten by a following sequence:
        // MODE is finite and all hardware triggers are off, and START is clear.
        let mut samples = [0; MAX_SEQUENCE_LEN];
        for (slot, result) in samples[..sequence.len()].iter_mut().enumerate() {
            *result = regs.result(slot).read().result() & Resolution::Bits12.max_count() as u16;
        }
        guard.adc.async_mask_completion()?;
        if regs.isr().read().ovw() {
            return Err(Error::Overrun);
        }
        guard.adc.async_clear_completion()?;
        check_fault::<T>()?;
        results.copy_from_slice(&samples[..sequence.len()]);
        guard.armed = false;
        Ok(timing)
    }

    /// Internal raw temperature channel, with the normal startup/acquisition bounds.
    #[cfg(not(adc_cw32f002_v1))]
    pub fn enable_temperature(&mut self) -> Temperature<T> {
        Temperature(PhantomData)
    }
    /// Internal raw bandgap channel; not a selectable conversion reference.
    #[cfg(not(adc_cw32f002_v1))]
    pub fn enable_vrefint(&mut self) -> VrefInt<T> {
        VrefInt(PhantomData)
    }
    /// Internal analog supply divided by three channel.
    pub fn enable_vdda(&mut self) -> Vdda<T> {
        Vdda(PhantomData)
    }
    /// Validate configuration first, then stop and reinitialize. A terminal fault
    /// cannot be cleared by configuration changes or replacement owners.
    pub fn set_config(&mut self, config: &Config) -> Result<(), Error> {
        check_fault::<T>()?;
        let timing = config.timing(self.pclk, 0, SampleTime::Cycles10)?;
        self.async_cleanup()?;
        self.config = *config;
        self.timing = timing;
        self.initialize_async()
    }
    /// Timing of the last slot at the sequence clock, or initial acquisition.
    pub fn timing(&self) -> Timing {
        self.timing
    }
    /// Fixed 12-bit resolution.
    pub fn resolution(&self) -> Resolution {
        Resolution::Bits12
    }
}

// A scoped owner borrow, never an ISR-visible pointer. Exists before completion is
// armed and before START; every error/drop path after arming requests cleanup.
struct AsyncGuard<'a, 'd, T: Instance> {
    adc: &'a mut Adc<'d, T, Async>,
    armed: bool,
}
impl<T: Instance> Drop for AsyncGuard<'_, '_, T> {
    fn drop(&mut self) {
        if self.armed {
            let _ = self.adc.async_cleanup();
        }
    }
}

impl<T: Instance, M: Mode> Adc<'_, T, M> {
    fn async_poll_control(&self, mut ready: impl FnMut() -> bool) -> bool {
        for _ in 0..self.config.timeout {
            if ready() {
                return true;
            }
        }
        false
    }

    fn async_fault(&mut self) -> Error {
        T::state().faulted.store(true, Ordering::Release);
        T::state().irq_active.store(false, Ordering::Release);
        // Only a failed control readback disables the dedicated ADC vector.
        // Its exclusivity is asserted from the generated GLOBAL links.
        T::Interrupt::disable();
        self.active = false;
        Error::Faulted
    }

    fn async_mask_completion(&mut self) -> Result<(), Error> {
        let regs = T::regs();
        critical_section::with(|_| {
            regs.ier().modify(|v| {
                v.set_eoc(false);
                v.set_eos(false);
                v.set_ovw(false);
            })
        });
        if self.async_poll_control(|| {
            let ier = regs.ier().read();
            !ier.eoc() && !ier.eos() && !ier.ovw()
        }) {
            check_fault::<T>()
        } else {
            Err(self.async_fault())
        }
    }

    fn async_clear_completion(&mut self) -> Result<(), Error> {
        // R1W0: clear EOC/EOS/OVW; retain EOA and every watchdog event.
        T::regs().icr().write(|v| {
            v.set_eoc(false);
            v.set_eos(false);
            v.set_eoa(true);
            v.set_wdtl(true);
            v.set_wdth(true);
            v.set_wdtr(true);
            v.set_ovw(false);
        });
        if self.async_poll_control(|| {
            let status = T::regs().isr().read();
            !status.eoc() && !status.eos() && !status.ovw()
        }) {
            check_fault::<T>()
        } else {
            Err(self.async_fault())
        }
    }

    fn async_cleanup(&mut self) -> Result<(), Error> {
        let regs = T::regs();
        let masked = self.async_mask_completion().is_ok();
        regs.trigger().write(|_| {});
        regs.start().write(|v| {
            v.set_start(false);
            v.set_autostop(false);
        });
        let stopped = self.async_poll_control(|| {
            let start = regs.start().read();
            !start.start() && !start.autostop() && !hardware_trigger_enabled::<T>()
        });
        critical_section::with(|_| {
            regs.cr0().modify(|v| {
                v.set_en(false);
                v.set_mode(pac::adc::vals::Mode::Single);
                v.set_buf(false);
                v.set_ref_(Reference::Vdda as u8);
                #[cfg(not(adc_cw32f002_v1))]
                v.set_tsen(false);
            });
        });
        let disabled = self.async_poll_control(|| {
            let cr = regs.cr0().read();
            let ready = !cr.en()
                && !cr.buf()
                && cr.mode() == pac::adc::vals::Mode::Single
                && cr.ref_() == Reference::Vdda as u8;
            #[cfg(not(adc_cw32f002_v1))]
            let ready = ready && !cr.tsen();
            ready
        });
        self.drain_results(); // Housekeeping only; not proof of analog drain.
        let cleared = self.async_clear_completion().is_ok();
        self.active = false;
        if masked && stopped && disabled && cleared {
            check_fault::<T>()
        } else {
            Err(self.async_fault())
        }
    }

    fn initialize_async(&mut self) -> Result<(), Error> {
        check_fault::<T>()?;
        let regs = T::regs();
        regs.cr1().modify(|v| {
            v.set_chmux(0);
            v.set_align(false);
            v.set_discard(false);
        });
        critical_section::with(|_| {
            regs.cr0().modify(|v| {
                v.set_mode(pac::adc::vals::Mode::Single);
                v.set_ref_(self.config.reference as u8);
                v.set_clk(self.timing.prescaler as u8);
                v.set_sam(self.timing.sample_time as u8);
                v.set_buf(false);
                v.set_bias(0);
                #[cfg(not(adc_cw32f002_v1))]
                v.set_tsen(false);
                #[cfg(any(adc_v1, adc_cw32f020_v1, adc_cw32f003_v1))]
                v.set_bgren(v.bgren() || self.config.reference != Reference::Vdda);
                v.set_en(true);
            });
        });
        if !self.async_poll_control(|| {
            let cr = regs.cr0().read();
            let ready = cr.en()
                && cr.mode() == pac::adc::vals::Mode::Single
                && cr.clk() == self.timing.prescaler as u8
                && cr.sam() == self.timing.sample_time as u8
                && cr.ref_() == self.config.reference as u8
                && !cr.buf()
                && cr.bias() == 0;
            #[cfg(not(adc_cw32f002_v1))]
            let ready = ready && !cr.tsen();
            #[cfg(any(adc_v1, adc_cw32f020_v1, adc_cw32f003_v1))]
            let ready = ready && (self.config.reference == Reference::Vdda || cr.bgren());
            ready
        }) {
            let _ = self.async_cleanup();
            return Err(self.async_fault());
        }
        if !self.async_poll_control(|| regs.isr().read().ready()) {
            self.async_cleanup()?;
            return Err(Error::ReadyTimeout);
        }
        self.active = true;
        Ok(())
    }
}

impl<T: Instance, M: Mode> Drop for Adc<'_, T, M> {
    fn drop(&mut self) {
        if self.async_lifecycle {
            let _ = self.async_cleanup();
            T::state().irq_active.store(false, Ordering::Release);
        } else {
            self.shutdown();
        }
        critical_section::with(|cs| {
            if check_fault::<T>().is_ok() && !self.clock_was_enabled && !self.bandgap_enabled() {
                let _ = crate::peripherals::ADC::RCC_INFO.disable_with_cs(cs);
            }
        });
    }
}

pub(crate) trait SealedInstance {
    fn regs() -> pac::adc::Adc;
    fn state() -> &'static State;
}
/// A verified ADC instance, implemented only by generated chip metadata.
#[allow(private_bounds)]
pub trait Instance: PeripheralType + SealedInstance + 'static {
    /// Dedicated ADC GLOBAL interrupt obtained from chip metadata.
    type Interrupt: Interrupt;
}

/// Per-instance interrupt state. Never contains a caller buffer or channel pointer.
pub(crate) struct State {
    waker: AtomicWaker,
    irq_active: AtomicBool,
    faulted: AtomicBool,
}
impl State {
    pub(crate) const fn new() -> Self {
        Self {
            waker: AtomicWaker::new(),
            irq_active: AtomicBool::new(false),
            faulted: AtomicBool::new(false),
        }
    }
}

/// Completion/OVW software conversion handler, bound using `bind_interrupts!`.
/// The sticky completion is retained for the future; no sample is copied here.
pub struct InterruptHandler<T: Instance>(PhantomData<T>);
impl<T: Instance> Handler<T::Interrupt> for InterruptHandler<T> {
    unsafe fn on_interrupt() {
        let state = T::state();
        let wake = critical_section::with(|_| {
            if !state.irq_active.load(Ordering::Acquire) {
                return false;
            }
            let regs = T::regs();
            let ier = regs.ier().read();
            let status = regs.isr().read();
            if (ier.eoc() && status.eoc())
                || (ier.eos() && status.eos())
                || (ier.ovw() && status.ovw())
            {
                regs.ier().modify(|v| {
                    v.set_eoc(false);
                    v.set_eos(false);
                    v.set_ovw(false);
                });
                // A failed masking readback must not trap the executor in an
                // IRQ storm. One read is a finite, conservative ISR check.
                let masked = regs.ier().read();
                if masked.eoc() || masked.eos() || masked.ovw() {
                    state.faulted.store(true, Ordering::Release);
                    state.irq_active.store(false, Ordering::Release);
                    T::Interrupt::disable();
                }
                true
            } else {
                false
            }
        });
        if wake {
            state.waker.wake();
        }
    }
}

fn check_fault<T: Instance>() -> Result<(), Error> {
    if T::state().faulted.load(Ordering::Acquire) {
        Err(Error::Faulted)
    } else {
        Ok(())
    }
}

pub(crate) trait SealedAdcChannel<T> {
    fn setup(&mut self) {}
    fn channel(&self) -> u8;
}
/// A channel verified for this ADC. External channels own an Embassy pin token;
/// internal channels can only be obtained from their ADC instance.
#[allow(private_bounds)]
pub trait AdcChannel<'d, T>: SealedAdcChannel<T> + Sized {
    /// Consume and erase a channel's concrete type, retaining its ownership.
    fn degrade_adc(mut self) -> BorrowedAdcChannel<'d, T> {
        self.setup();
        BorrowedAdcChannel {
            channel: self.channel(),
            _marker: PhantomData,
        }
    }
    /// Borrow and erase a channel. Its pin cannot be used until this borrow ends.
    fn reborrow_adc(&mut self) -> BorrowedAdcChannel<'_, T> {
        self.setup();
        BorrowedAdcChannel {
            channel: self.channel(),
            _marker: PhantomData,
        }
    }
}
/// Type-erased owned or borrowed channel. Cannot be constructed with an arbitrary
/// channel number. Its lifetime retains the original channel's exclusive borrow.
pub struct BorrowedAdcChannel<'a, T> {
    channel: u8,
    _marker: PhantomData<&'a mut T>,
}
impl<T: Instance> BorrowedAdcChannel<'_, T> {
    /// Verified ADC mux index.
    pub fn get_hw_channel(&self) -> u8 {
        self.channel
    }
}
impl<T> SealedAdcChannel<T> for BorrowedAdcChannel<'_, T> {
    fn channel(&self) -> u8 {
        self.channel
    }
}
impl<'a, T> AdcChannel<'a, T> for BorrowedAdcChannel<'a, T> {}
trait SealedBorrowedChannel<'a, T> {
    fn reborrow_adc(self) -> BorrowedAdcChannel<'a, T>;
}
/// A mutable channel reference or already type-erased channel.
#[allow(private_bounds)]
pub trait BorrowedChannel<'a, T>: SealedBorrowedChannel<'a, T> {}
impl<'a, T, C: SealedBorrowedChannel<'a, T>> BorrowedChannel<'a, T> for C {}
impl<'a, 'd, T, C: AdcChannel<'d, T>> SealedBorrowedChannel<'a, T> for &'a mut C {
    fn reborrow_adc(self) -> BorrowedAdcChannel<'a, T> {
        AdcChannel::reborrow_adc(self)
    }
}
impl<'a, T> SealedBorrowedChannel<'a, T> for BorrowedAdcChannel<'a, T> {
    fn reborrow_adc(self) -> BorrowedAdcChannel<'a, T> {
        self
    }
}
/// Internal temperature sensor (channel 14), returning raw ADC counts.
#[cfg(not(adc_cw32f002_v1))]
pub struct Temperature<T: Instance>(PhantomData<T>);
/// Internal nominal 1.2 V bandgap source (channel 15).
#[cfg(not(adc_cw32f002_v1))]
pub struct VrefInt<T: Instance>(PhantomData<T>);
/// Internal analog supply divided by three (channel 13).
pub struct Vdda<T: Instance>(PhantomData<T>);
macro_rules! impl_internal {
    ($name:ident, $channel:expr) => {
        impl<T: Instance> SealedAdcChannel<T> for $name<T> {
            fn channel(&self) -> u8 {
                $channel
            }
        }
        impl<'d, T: Instance> AdcChannel<'d, T> for $name<T> {}
    };
}
#[cfg(not(adc_cw32f002_v1))]
impl_internal!(Temperature, crate::ADC_TEMPERATURE_CHANNEL.unwrap());
#[cfg(not(adc_cw32f002_v1))]
impl_internal!(VrefInt, crate::ADC_BANDGAP_CHANNEL.unwrap());
impl_internal!(Vdda, crate::ADC_SUPPLY_CHANNEL);

// Emitted only for the selected device and package's verified analog routes.
macro_rules! impl_instance {
    ($name:ident, $irq:ident) => {
        impl $crate::adc::SealedInstance for $crate::peripherals::$name {
            fn regs() -> $crate::pac::adc::Adc {
                $crate::pac::$name
            }
            fn state() -> &'static $crate::adc::State {
                static STATE: $crate::adc::State = $crate::adc::State::new();
                &STATE
            }
        }
        impl $crate::adc::Instance for $crate::peripherals::$name {
            type Interrupt = $crate::interrupt::typelevel::$irq;
        }
    };
}
pub(crate) use impl_instance;
macro_rules! impl_pin {
    ($instance:ident, $pin:ident, $channel:literal) => {
        impl $crate::adc::SealedAdcChannel<$crate::peripherals::$instance>
            for $crate::Peri<'_, $crate::peripherals::$pin>
        {
            fn setup(&mut self) {
                // Flex enables/unlocks the port and disconnects the digital
                // path. Its drop also leaves analog mode and weak pulls off.
                let mut pin = $crate::gpio::Flex::new(self.reborrow());
                pin.set_as_analog();
            }
            fn channel(&self) -> u8 {
                $channel
            }
        }
        impl<'d> $crate::adc::AdcChannel<'d, $crate::peripherals::$instance>
            for $crate::Peri<'d, $crate::peripherals::$pin>
        {
        }
    };
}
pub(crate) use impl_pin;

impl<T: Instance, M: Mode> Adc<'_, T, M> {
    fn initialize(&mut self) -> Result<(), Error> {
        // Reset-free acquisition may inherit enabled triggering/conversion.
        // Mask trigger sources before stopping, then power down owned analog
        // controls before draining results or applying a fresh configuration.
        self.shutdown();
        let regs = T::regs();
        regs.cr1().write(|_| {}); // Right aligned; DMA and watchdog disabled.
        regs.cr2().write(|_| {}); // No accumulation.
        critical_section::with(|_| {
            #[cfg(any(adc_v1, adc_cw32f020_v1, adc_cw32f003_v1))]
            let inherited_bandgap = regs.cr0().read().bgren();
            regs.cr0().write(|v| {
                v.set_en(true);
                v.set_mode(pac::adc::vals::Mode::Single);
                v.set_ref_(self.config.reference as u8);
                v.set_clk(self.timing.prescaler as u8);
                v.set_sam(self.timing.sample_time as u8);
                #[cfg(any(adc_v1, adc_cw32f020_v1, adc_cw32f003_v1))]
                v.set_bgren(inherited_bandgap || self.config.reference != Reference::Vdda);
            });
        });
        self.wait_ready()?;
        self.active = true;
        Ok(())
    }
    fn wait_ready(&mut self) -> Result<(), Error> {
        for _ in 0..self.config.timeout {
            if T::regs().isr().read().ready() {
                return Ok(());
            }
        }
        self.shutdown();
        Err(Error::ReadyTimeout)
    }
    fn drain_results(&self) {
        for slot in 0..MAX_SEQUENCE_LEN {
            let _ = T::regs().result(slot).read().result();
        }
    }
    fn clear_all_flags(&self) {
        // ICR is R1W0. READY is read-only and has no ICR field.
        T::regs().icr().write(|v| {
            v.set_eoc(false);
            v.set_eos(false);
            v.set_eoa(false);
            v.set_wdtl(false);
            v.set_wdth(false);
            v.set_wdtr(false);
            v.set_ovw(false);
        });
    }
    fn delay_us(&self, microseconds: u32) {
        cortex_m::asm::delay(self.hclk.delay_cycles_us(microseconds) as u32);
    }
    fn bandgap_enabled(&self) -> bool {
        #[cfg(any(adc_v1, adc_cw32f020_v1, adc_cw32f003_v1))]
        {
            T::regs().cr0().read().bgren()
        }
        #[cfg(not(any(adc_v1, adc_cw32f020_v1, adc_cw32f003_v1)))]
        {
            false
        }
    }
    fn shutdown(&mut self) {
        let regs = T::regs();
        regs.trigger().write(|_| {});
        regs.ier().write(|_| {});
        regs.start().write(|v| v.set_start(false));
        critical_section::with(|_| {
            #[cfg(any(adc_v1, adc_cw32f020_v1, adc_cw32f003_v1))]
            let keep_bandgap = regs.cr0().read().bgren();
            regs.cr0().write(|v| {
                v.set_en(false);
                #[cfg(any(adc_v1, adc_cw32f020_v1, adc_cw32f003_v1))]
                v.set_bgren(keep_bandgap);
            });
        });
        self.drain_results();
        self.clear_all_flags();
        self.active = false;
    }
}

fn hardware_trigger_enabled<T: Instance>() -> bool {
    let v = T::regs().trigger().read();
    #[cfg(adc_v1)]
    {
        v.atim()
            || v.gtim1()
            || v.gtim2()
            || v.gtim3()
            || v.gtim4()
            || v.btim1()
            || v.btim2()
            || v.btim3()
            || v.uart1()
            || v.uart2()
            || v.uart3()
            || v.spi1()
            || v.spi2()
            || v.i2c1()
            || v.i2c2()
            || v.dma()
    }
    #[cfg(adc_cw32f020_v1)]
    {
        v.gtim1()
            || v.gtim2()
            || v.gtim3()
            || v.gtim4()
            || v.btim1()
            || v.btim2()
            || v.btim3()
            || v.uart1()
            || v.uart2()
            || v.uart3()
            || v.spi1()
            || v.spi2()
            || v.i2c1()
            || v.i2c2()
            || v.dma()
    }
    #[cfg(adc_cw32f002_v1)]
    {
        // Own manual calls bit3 PA02, SDK/PAC PA32. Only reject its enable;
        // no public trigger route or field rename follows from this check.
        v.gtim()
            || v.pa10()
            || v.pa32()
            || v.pa54()
            || v.btim1()
            || v.btim2()
            || v.btim3()
            || v.uart1()
            || v.uart2()
            || v.pa76()
            || v.spi()
            || v.pb10()
            || v.i2c()
            || v.pb32()
            || v.pb64()
    }
    #[cfg(adc_cw32f003_v1)]
    {
        v.atim()
            || v.gtim()
            || v.pa10()
            || v.pa32()
            || v.pa54()
            || v.btim1()
            || v.btim2()
            || v.btim3()
            || v.uart1()
            || v.uart2()
            || v.pa76()
            || v.spi()
            || v.pb10()
            || v.i2c()
            || v.pb32()
            || v.pb74()
    }
    #[cfg(adc_cw32l031_v1)]
    {
        v.atim()
            || v.gtim1()
            || v.gtim2()
            || v.btim1()
            || v.btim2()
            || v.btim3()
            || v.uart1()
            || v.uart2()
            || v.uart3()
            || v.spi1()
            || v.i2c1()
            || v.dma()
            || v.gpioa()
            || v.gpiob()
            || v.gpioc()
            || v.gpiof()
    }
    #[cfg(adc_cw32l052_v1)]
    {
        v.atim()
            || v.gtim1()
            || v.gtim2()
            || v.gtim3()
            || v.btim1()
            || v.btim2()
            || v.btim3()
            || v.uart1()
            || v.uart2()
            || v.uart3()
            || v.spi1()
            || v.spi2()
            || v.i2c1()
            || v.i2c2()
            || v.dma()
            || v.gpioa()
            || v.gpiob()
            || v.gpioc()
            || v.gpiod()
            || v.gpiof()
    }
    #[cfg(adc_cw32l083_v1)]
    {
        v.atim()
            || v.gtim1()
            || v.gtim2()
            || v.gtim3()
            || v.gtim4()
            || v.btim1()
            || v.btim2()
            || v.btim3()
            || v.uart1()
            || v.uart2()
            || v.uart3()
            || v.spi1()
            || v.spi2()
            || v.i2c1()
            || v.i2c2()
            || v.dma()
            || v.uart4()
            || v.uart5()
            || v.uart6()
            || v.gpioa()
            || v.gpiob()
            || v.gpioc()
            || v.gpiod()
            || v.gpioe()
            || v.gpiof()
    }
}
fn inherited_async_configuration<T: Instance>() -> bool {
    let regs = T::regs();
    let ier = regs.ier().read();
    let cr1 = regs.cr1().read();
    let unsupported = hardware_trigger_enabled::<T>()
        || cr1.wdtall()
        || ier.eoa()
        || ier.wdtl()
        || ier.wdth()
        || ier.wdtr()
        || regs.cr2().read().accen();
    #[cfg(any(adc_v1, adc_cw32f020_v1))]
    let unsupported = unsupported || cr1.dmaen();
    #[cfg(any(adc_cw32l031_v1, adc_cw32l052_v1, adc_cw32l083_v1))]
    let unsupported = unsupported || cr1.dmasofen() || cr1.dmasocen();
    unsupported
}

fn clock_acquire(timeout: u32, cs: critical_section::CriticalSection<'_>) -> Result<bool, Error> {
    let rcc = crate::peripherals::ADC::RCC_INFO;
    if rcc.reset_asserted() {
        return Err(Error::ResetAsserted);
    }
    let was_enabled = rcc.is_enabled();
    if rcc
        .enable_with_cs_readback(
            cs,
            Readback::Poll {
                attempts: timeout,
                spin: false,
            },
        )
        .is_err()
    {
        if !was_enabled {
            let _ = rcc.disable_with_cs(cs);
        }
        return Err(Error::ClockEnableTimeout);
    }
    Ok(was_enabled)
}
fn clock_release(was_enabled: bool, cs: critical_section::CriticalSection<'_>) {
    #[cfg(any(adc_v1, adc_cw32f020_v1, adc_cw32f003_v1))]
    let bandgap = pac::ADC.cr0().read().bgren();
    #[cfg(not(any(adc_v1, adc_cw32f020_v1, adc_cw32f003_v1)))]
    let bandgap = false;
    if !was_enabled && !bandgap {
        let _ = crate::peripherals::ADC::RCC_INFO.disable_with_cs(cs);
    }
}
