//! Native L010 init-only LSE. Own RM Rev1.2 and DS Rev1.3, with source-backed
//! electrical/admission facts in docs/lse-l010-qualification.json.
use super::{ClockBounds, Error, OperatingConditions, SealedRccPeripheral};
use crate::{pac, peripherals, time::Hertz};

pub use pac::sysctrl::vals::{LseDrive, LseWait};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum LseMode {
    Oscillator,
    Bypass,
}

/// The native detector policy; neither mode measures frequency or recovers time.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum LseFaultDetection {
    /// Count startup edges with CCS clear. STABLE may remain set after later
    /// source loss; subsequent RTC readiness checks cannot detect that loss.
    StartupOnly,
    /// Use an already stable, legally operating, unchanged LSI and the existing
    /// fault routes. No LSI calibration or startup is attempted here. LSIEN may
    /// be clear: native CCS requests LSI automatically.
    ///
    /// A real fault can set timer capture/system-brake flags (including SBIF
    /// before BKE), request an enabled IRQ, and asynchronously clear PWM MOE.
    /// Existing AOE/output controls determine later behavior. These effects can
    /// precede an init error and survive it. Protection routes and flags are
    /// preserved; the HAL neither services their handlers nor restores outputs.
    MonitoredExistingRoutes,
}

/// Board-qualified nominal 32768 Hz source on the three exact L010 packages.
/// Bounds must cover every individual cycle throughout the declared supply,
/// ambient, aging and load conditions. Average ppm is insufficient. The board
/// must qualify crystal load/drive/startup or bypass voltage, duty, pulses and
/// edges. No startup maximum or continuing source availability is measured.
/// See [`crate::init`] for the functional RTC-observer and GPIOB handover limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Lse {
    pub min_freq: Hertz,
    pub max_freq: Hertz,
    pub operating_conditions: OperatingConditions,
    pub mode: LseMode,
    /// Native four-bit running drive, selected for the actual board.
    pub drive: LseDrive,
    /// Independent four-bit startup drive; set before enable, never retuned.
    pub startup_drive: LseDrive,
    pub wait: LseWait,
    pub fault_detection: LseFaultDetection,
    /// Nonzero CPU-poll budget, not milliseconds. Failure retains the source
    /// request and permanent pad reservations. Ordinary reset may retain LSE.
    pub poll_budget: u32,
}
impl Lse {
    pub(crate) fn bounds(self, board: OperatingConditions) -> Result<ClockBounds, Error> {
        if self.poll_budget == 0 {
            return Err(Error::InvalidTimeout);
        }
        let c = self.operating_conditions;
        let bounds = ClockBounds::external(
            Hertz(crate::RCC_LSE_NOMINAL_HZ),
            self.min_freq,
            self.max_freq,
            c,
        )
        .ok_or(Error::InvalidLseBounds)?;
        if self.max_freq.0 > crate::RCC_LSE_MAXIMUM_HZ
            || c.min_supply_mv < crate::RCC_LSE_SUPPLY_MV.0
            || c.max_supply_mv > crate::RCC_LSE_SUPPLY_MV.1
            || c.min_temperature_c < crate::RCC_LSE_TEMPERATURE_C.0
            || c.max_temperature_c > crate::RCC_LSE_TEMPERATURE_C.1
            || board.min_supply_mv < c.min_supply_mv
            || board.max_supply_mv > c.max_supply_mv
            || board.min_temperature_c < c.min_temperature_c
            || board.max_temperature_c > c.max_temperature_c
        {
            return Err(Error::InvalidLseBounds);
        }
        // Sufficient worst-period detector proof, with one complete extra LSE
        // edge of phase margin. The 36080-Hz legal inherited-LSI maximum comes
        // from own RM +/-10%, never DS factory +/-3% inferred from STABLE.
        if self.monitored()
            && u64::from(self.min_freq.0) * u64::from(crate::RCC_LSE_DETECTOR_LSI_CYCLES)
                <= (u64::from(crate::RCC_LSE_DETECTOR_LSE_EDGES) + 1)
                    * u64::from(crate::RCC_LSE_MONITORED_LSI_MAXIMUM_HZ)
        {
            return Err(Error::InvalidLseBounds);
        }
        if crate::RCC_LSE_PINS.0.is_none() || (!self.bypass() && crate::RCC_LSE_PINS.1.is_none()) {
            return Err(Error::LsePinConflict);
        }
        Ok(bounds)
    }
    fn bypass(self) -> bool {
        self.mode == LseMode::Bypass
    }
    pub(crate) fn monitored(self) -> bool {
        self.fault_detection == LseFaultDetection::MonitoredExistingRoutes
    }
}

static MONITOR_LSI: critical_section::Mutex<core::cell::Cell<Option<(u16, u8)>>> =
    critical_section::Mutex::new(core::cell::Cell::new(None));

fn lsi_parameters() -> (u16, u8) {
    let lsi = pac::SYSCTRL.lsi().read();
    (lsi.trim(), lsi.waitcycle())
}
fn parameters_match(config: Lse) -> bool {
    let r = pac::SYSCTRL.lse().read();
    r.mode() == config.bypass()
        && r.driver() == config.drive
        && r.pdriver() == config.startup_drive
        && r.waitcycle() == config.wait
}
fn faults() -> bool {
    let flags = pac::SYSCTRL.isr().read();
    flags.lsefail() || flags.lsefault()
}
fn ready(config: Lse, require_frozen: bool) -> bool {
    let cr1 = pac::SYSCTRL.cr1().read();
    if !cr1.lseen()
        || !pac::SYSCTRL.lse().read().stable()
        || faults()
        || !parameters_match(config)
        || cr1.lseccs() != config.monitored()
    {
        return false;
    }
    !config.monitored()
        || (pac::SYSCTRL.lsi().read().stable()
            && critical_section::with(|cs| match MONITOR_LSI.borrow(cs).get() {
                Some(parameters) => lsi_parameters() == parameters,
                None => !require_frozen,
            }))
}
pub(crate) fn healthy(config: Lse) -> bool {
    ready(config, true)
}

/// Captured before the first gate write; all frozen monitor parameters must
/// remain unchanged through the enclosing RCC transition.
#[derive(Clone, Copy)]
pub(crate) struct Admission {
    reused: bool,
    lsi: Option<(u16, u8)>,
    lse: u32,
    routes: u32,
    interrupts: u32,
}

fn central_new(config: Lse) -> Result<(), Error> {
    let r = pac::SYSCTRL;
    let cr1 = r.cr1().read();
    let lse = r.lse().read();
    let flags = r.isr().read();
    let ier = r.ier().read();
    if cr1.lseen()
        || cr1.lselock()
        || lse.pinlock()
        || lse.stable()
        || flags.lserdy()
        || flags.lsestable()
        || faults()
        || ier.lserdy()
        || ier.lsefail()
        || ier.lsefault()
        || r.cr0().read().sysclk() == pac::sysctrl::vals::Sysclk::Lse
        || r.mco().read().source() == crate::RCC_LSE_MCO_SOURCE
    {
        return Err(Error::LseClockInUse);
    }
    if !config.monitored() && cr1.lseccs() {
        return Err(Error::LseClockInUse);
    }
    if config.monitored() && !r.lsi().read().stable() {
        return Err(Error::LseMonitorNotReady);
    }
    // Active-low resets are observed, never asserted or released to inspect.
    if peripherals::GPIOB::RCC_INFO.reset_asserted()
        || peripherals::RTC::RCC_INFO.reset_asserted()
        || peripherals::UART1::RCC_INFO.reset_asserted()
        || peripherals::UART2::RCC_INFO.reset_asserted()
        || peripherals::LPTIM::RCC_INFO.reset_asserted()
    {
        return Err(Error::LseClockInUse);
    }
    Ok(())
}

/// Configuration-only gates, in RTC/UART1/UART2/LPTIM order. Restores gates on
/// rejection; failed restoration is an error. DATE/PSC/status are never written.
fn configuration_consumers(
    config: Lse,
    cs: critical_section::CriticalSection<'_>,
) -> Result<bool, Error> {
    let inspection_error = |error| {
        use super::peripheral::ClockInspectionError;
        match error {
            ClockInspectionError::EnableFailed { restore_failed } => {
                Error::LseConfigurationGateTimeout {
                    enable_failed: true,
                    restore_failed,
                }
            }
            ClockInspectionError::RestoreFailed => Error::LseConfigurationGateTimeout {
                enable_failed: false,
                restore_failed: true,
            },
        }
    };
    let rtc_lse = peripherals::RTC::RCC_INFO
        .inspect_for_init(cs, config.poll_budget, || {
            let cr1 = pac::RTC.cr1().read();
            if cr1.source() > 3 {
                return Err(Error::LseClockInUse);
            }
            if cr1.source() != crate::RCC_LSE_RTC_SOURCE {
                return Ok(false);
            }
            let mut cr0 = pac::RTC.cr0().read();
            cr0.set_h24(false);
            if cr0.0 != 0
                || cr1.access()
                || cr1.wait()
                || pac::RTC.cr2().read().0 != 0
                || pac::RTC.compcfr1().read().0 != 0
                || pac::RTC.ier().read().0 != 0
                || pac::RTC.isr().read().0 != 0
            {
                return Err(Error::LseClockInUse);
            }
            // This quiet image does NOT define RTC1HZ=0 or prove no RTC_OUT observer.
            Ok(true)
        })
        .map_err(inspection_error)??;
    for (info, uart) in [
        (peripherals::UART1::RCC_INFO, pac::UART1),
        (peripherals::UART2::RCC_INFO, pac::UART2),
    ] {
        if info
            .inspect_for_init(cs, config.poll_budget, || {
                u8::from(uart.cr1().read().source()) == crate::RCC_LSE_UART_SOURCE
            })
            .map_err(inspection_error)?
        {
            return Err(Error::LseClockInUse);
        }
    }
    let conflict = peripherals::LPTIM::RCC_INFO
        .inspect_for_init(cs, config.poll_budget, || {
            let cr = pac::LPTIM.cr().read();
            let cfgr = pac::LPTIM.cfgr().read();
            cr.en()
                && (cfgr.iclksrc() == pac::lptim::vals::Source::Lse
                    || (rtc_lse && u8::from(cfgr.trigen()) != 0 && matches!(cfgr.trigsel(), 1..=5)))
        })
        .map_err(inspection_error)?;
    if conflict {
        return Err(Error::LseClockInUse);
    }
    Ok(rtc_lse)
}

pub(crate) fn preflight(
    config: Lse,
    cs: critical_section::CriticalSection<'_>,
) -> Result<Admission, Error> {
    let r = pac::SYSCTRL;
    let reused = r.cr1().read().lseen();
    if reused {
        if !ready(config, false) {
            return Err(Error::LseNotReady);
        }
        if peripherals::GPIOB::RCC_INFO.reset_asserted() {
            return Err(Error::LsePinConflict);
        }
    } else {
        central_new(config)?;
    }
    let admission = Admission {
        reused,
        lsi: config.monitored().then(lsi_parameters),
        lse: r.lse().read().0,
        routes: r.cr2().read().0,
        interrupts: r.ier().read().0,
    };
    if !reused {
        configuration_consumers(config, cs)?;
    }
    Ok(admission)
}

fn poll(mut ready: impl FnMut() -> bool, attempts: u32, error: Error) -> Result<(), Error> {
    for _ in 0..attempts {
        if ready() {
            return Ok(());
        }
        core::hint::spin_loop();
    }
    Err(error)
}

/// GPIOB's gate controls the WHOLE BANK's operation, not just configuration.
/// The public functional handover allows sampling/filter/edge progress even on
/// failure. Preserve unrelated pin controls/flags; never open a timer gate.
fn pads(config: Lse, unused: bool, rtc_lse: bool, configure: bool) -> Result<(), Error> {
    if peripherals::GPIOB::RCC_INFO.reset_asserted() {
        return Err(Error::LsePinConflict);
    }
    let was_enabled = pac::SYSCTRL.ahben().read().gpiob();
    let set_gate = |enabled| {
        pac::SYSCTRL.ahben().modify(|w| {
            w.set_key(0x5a5a);
            w.set_gpiob(enabled);
        });
        poll(
            || pac::SYSCTRL.ahben().read().gpiob() == enabled,
            config.poll_budget,
            Error::LseGpioGateTimeout,
        )
    };
    if !was_enabled {
        if let Err(error) = set_gate(true) {
            // Best-effort bounded restoration; never report it succeeded on a timeout.
            set_gate(false)?;
            return Err(error);
        }
    }
    let result = (|| {
        let r = pac::GPIOB;
        let dir = r.dir().read();
        let analog = r.analog().read();
        let af = r.afrl().read();
        // Only inside this already-permitted operation window, and before any
        // PB01 change. This does not retrospectively prove gate opening inert.
        if rtc_lse
            && ((!analog.pin4() && !dir.pin4() && af.afr4() == 2)
                || (!analog.pin6() && !dir.pin6() && af.afr6() == 2))
        {
            return Err(Error::LseClockInUse);
        }
        let pin_mask = if config.bypass() { 2 } else { 3 };
        if !dir.pin1()
            || analog.pin1() != (unused || !config.bypass())
            || af.afr1() != 0
            || (!config.bypass() && (!dir.pin0() || !analog.pin0() || af.afr0() != 0))
            || (r.opendrain().read().0
                | r.pur().read().0
                | r.riseie().read().0
                | r.fallie().read().0
                | r.filter().read().0)
                & pin_mask
                != 0
        {
            return Err(Error::LsePinConflict);
        }
        if configure && config.bypass() {
            r.analog().modify(|w| w.set_pin1(false));
            if r.analog().read().pin1() {
                return Err(Error::LsePinConflict);
            }
        }
        Ok(())
    })();
    if !was_enabled {
        set_gate(false)?;
    }
    result
}

pub(crate) fn verify(config: Lse, _cs: critical_section::CriticalSection<'_>) -> Result<(), Error> {
    if !healthy(config) {
        return Err(Error::LseNotReady);
    }
    pads(config, false, false, false)?;
    if !healthy(config) {
        return Err(Error::LseNotReady);
    }
    Ok(())
}

pub(crate) fn start(
    config: Lse,
    admission: Admission,
    cs: critical_section::CriticalSection<'_>,
) -> Result<(), Error> {
    let r = pac::SYSCTRL;
    if r.cr2().read().0 != admission.routes
        || r.ier().read().0 != admission.interrupts
        || r.lse().read().0 != admission.lse
    {
        return Err(Error::LseClockInUse);
    }
    if let Some(parameters) = admission.lsi {
        if !r.lsi().read().stable() || lsi_parameters() != parameters {
            return Err(Error::LseMonitorNotReady);
        }
        if MONITOR_LSI
            .borrow(cs)
            .get()
            .is_some_and(|old| old != parameters)
        {
            return Err(Error::LseMonitorNotReady);
        }
        MONITOR_LSI.borrow(cs).set(Some(parameters));
    }
    if admission.reused {
        return verify(config, cs);
    }
    central_new(config)?;
    let rtc_lse = configuration_consumers(config, cs)?;
    central_new(config)?;
    pads(config, true, rtc_lse, true)?;
    central_new(config)?;
    r.lse().modify(|w| {
        w.set_mode(config.bypass());
        w.set_driver(config.drive);
        w.set_pdriver(config.startup_drive);
        w.set_waitcycle(config.wait);
    });
    if !parameters_match(config) {
        return Err(Error::LseNotReady);
    }
    r.cr1().modify(|w| {
        w.set_key(0x5a5a);
        w.set_lseccs(config.monitored());
        w.set_lseen(true);
    });
    for _ in 0..config.poll_budget {
        if faults() {
            return Err(Error::LseNotReady);
        }
        if healthy(config) {
            if r.cr2().read().0 != admission.routes || r.ier().read().0 != admission.interrupts {
                return Err(Error::LseClockInUse);
            }
            return verify(config, cs);
        }
        core::hint::spin_loop();
    }
    Err(Error::LseNotReady)
}

pub(crate) fn preserved_register(old: u32) -> bool {
    // Only requested parameters and read-only initial STABLE may differ.
    (pac::SYSCTRL.lse().read().0 ^ old) & !crate::RCC_LSE_CHANGE_MASK == 0
}
