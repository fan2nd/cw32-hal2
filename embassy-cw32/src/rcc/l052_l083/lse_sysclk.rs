//! Exact-package L052 init-only LSE system target. The old auxiliary path is
//! deliberately separate. All source loss checks require continuing CPU progress.
use super::super::{lse, Lse, LseMode};
use super::*;

#[derive(Clone, Copy)]
struct Monitor {
    trim: u16,
    wait: u8,
    sources: pac::sysctrl::regs::Cr1,
}

impl Monitor {
    fn policy(self, lse_started: bool) -> bool {
        let current = pac::SYSCTRL.cr1().read();
        current.clkccs() == self.sources.clkccs()
            && current.hseccs() == self.sources.hseccs()
            && current.lseccs() == (self.sources.lseccs() || lse_started)
            && current.lselock() == self.sources.lselock()
    }

    fn ready(self, lse_started: bool) -> bool {
        let r = pac::SYSCTRL;
        let lsi = r.lsi().read();
        self.policy(lse_started)
            && r.cr1().read().lsien()
            && lsi.stable()
            && r.isr().read().lsistable()
            && lsi.trim() == self.trim
            && lsi.waitcycle() == self.wait
    }

    // Matching stopped LSI is intentionally a no-parameter-write branch, not
    // a claim that parked direct consumers are absent. Mismatching stopped LSI
    // still goes through the old helper's two complete consumer passes.
    fn classify(self, require_factory: bool) -> Result<(), Error> {
        let r = pac::SYSCTRL;
        let control = r.cr1().read();
        let lsi = r.lsi().read();
        let flags = r.isr().read();
        let selected = r.cr0().read().sysclk() == ClockSource::Lsi;
        let matching = lsi.trim() == self.trim;
        let stable = lsi.stable() && flags.lsistable();
        if !self.policy(false)
            || lsi.waitcycle() != self.wait
            || (require_factory && !matching)
            // Sequential reads may straddle startup; reject conservatively.
            || lsi.stable() != flags.lsistable()
            || (!control.lsien() && (stable || selected))
            || (selected && !stable)
            || ((control.hseccs() || control.lseccs())
                && !(control.lsien() && stable && matching))
            || (!matching
                && (control.lsien() || stable || selected || control.hseccs() || control.lseccs()))
            || (!control.lsien() && (r.ier().read().lsirdy() || flags.lsirdy()))
        {
            return Err(Error::LseClockInUse);
        }
        Ok(())
    }
}

// Only successful system-target initialization publishes this record. An
// absent record contributes true, leaving old auxiliary LSE semantics intact.
static SUCCESS: Mutex<Cell<Option<Monitor>>> = Mutex::new(Cell::new(None));

pub(super) fn monitor_ready() -> bool {
    critical_section::with(|cs| SUCCESS.borrow(cs).get().is_none_or(|m| m.ready(true)))
}

#[derive(Clone, Copy)]
struct Entry {
    clock: pac::sysctrl::regs::Cr0,
    monitor: Monitor,
    hsi: pac::sysctrl::regs::Hsi,
    hse: pac::sysctrl::regs::Hse,
    lse: pac::sysctrl::regs::Lse,
}

impl Entry {
    fn admit(timeout: u32, cs: critical_section::CriticalSection<'_>) -> Result<Self, Error> {
        use crate::rtc::sealed::Instance;
        // Required unconditionally, BEFORE reading factory LSI or taking a
        // trim-match shortcut. This restores AUTOTRIM's configuration gate.
        if !crate::rcc_lse_monitor_can_freeze(timeout, cs)? {
            return Err(Error::LseClockInUse);
        }
        let r = pac::SYSCTRL;
        let factory = unsafe {
            core::ptr::read_volatile(crate::peripherals::RTC::FACTORY_TRIM_ADDRESS as *const u16)
        };
        if factory == u16::MAX {
            return Err(Error::LseNotReady);
        }
        let mut calibrated = pac::sysctrl::regs::Lsi::default();
        calibrated.set_trim(factory);
        let entry = Self {
            clock: r.cr0().read(),
            monitor: Monitor {
                trim: calibrated.trim(),
                wait: r.lsi().read().waitcycle(),
                sources: r.cr1().read(),
            },
            hsi: r.hsi().read(),
            hse: r.hse().read(),
            lse: r.lse().read(),
        };
        match entry.clock.sysclk() {
            ClockSource::Hsi | ClockSource::Hse | ClockSource::Lsi | ClockSource::Lse => {}
            _ => return Err(Error::InvalidClockSource), // L052 has no PLL.
        }
        if !matches!(entry.hsi.div(), 5 | 6 | 8 | 9 | 11..=15) {
            return Err(Error::InvalidHsiDivider);
        }
        let sources = entry.monitor.sources;
        if entry.clock.sysclk() == ClockSource::Hsi && (!sources.hsien() || !entry.hsi.stable()) {
            return Err(Error::HsiTimeout);
        }
        if entry.clock.sysclk() == ClockSource::Hse && (!sources.hseen() || !entry.hse.stable()) {
            return Err(Error::HseTimeout);
        }
        if entry.clock.sysclk() == ClockSource::Lse
            && (!sources.lseen() || !entry.lse.stable() || !r.isr().read().lsestable())
        {
            return Err(Error::LseClockInUse);
        }
        if !sources.hseen() && entry.hse.stable() {
            return Err(Error::HseClockInUse);
        }
        entry.monitor.classify(false)?;
        Ok(entry)
    }

    fn check(self, config: Config, lse_started: bool) -> Result<(), Error> {
        if !self.monitor.policy(lse_started) {
            return Err(Error::ClockConfigurationTimeout);
        }
        if pac::SYSCTRL.lsi().read().stable() != pac::SYSCTRL.isr().read().lsistable() {
            return Err(Error::LseClockInUse);
        }
        // Every transition poll preserves an inherited live HSE, including
        // with HSECCS disabled; STABLE loss need not produce a detector flag.
        if self.monitor.sources.hseen()
            && (!pac::SYSCTRL.cr1().read().hseen()
                || !pac::SYSCTRL.hse().read().stable()
                || pac::SYSCTRL.hse().read().0 != self.hse.0)
        {
            return Err(Error::HseClockInUse);
        }
        check_external_faults(
            self.monitor.sources.hseen() || self.monitor.sources.hseccs() || config.hse.is_some(),
            true,
        )
    }

    fn wait(
        self,
        config: Config,
        lse_started: bool,
        timeout: u32,
        error: Error,
        mut ready: impl FnMut() -> bool,
    ) -> Result<(), Error> {
        for _ in 0..timeout {
            self.check(config, lse_started)?;
            if ready() {
                return Ok(());
            }
            core::hint::spin_loop();
        }
        Err(error)
    }

    fn hse_ready(self, config: Config) -> Result<(), Error> {
        let r = pac::SYSCTRL;
        if let Some(hse) = config.hse {
            if !r.cr1().read().hseen() || !r.hse().read().stable() || !hse_parameters_match(hse)? {
                return Err(Error::HseClockInUse);
            }
        } else if r.cr1().read().hseen() != self.monitor.sources.hseen()
            || r.hse().read().0 != self.hse.0
            || (self.monitor.sources.hseen() && !r.hse().read().stable())
        {
            return Err(Error::HseClockInUse);
        }
        Ok(())
    }

    fn hse_pads(
        self,
        config: Config,
        cs: critical_section::CriticalSection<'_>,
    ) -> Result<(), Error> {
        let bypass = config
            .hse
            .map(|hse| hse.mode == HseMode::Bypass)
            .or_else(|| self.monitor.sources.hseen().then_some(self.hse.mode()));
        if let Some(bypass) = bypass {
            if !crate::rcc_hse_pins_match(bypass, config.timeout, cs)? {
                return Err(Error::HseClockInUse);
            }
        }
        Ok(())
    }

    fn cold_lse(self, config: Config) -> Result<(), Error> {
        let r = pac::SYSCTRL;
        let control = r.cr1().read();
        let flags = r.isr().read();
        let ier = r.ier().read();
        self.check(config, false)?;
        if control.lseen()
            || control.lselock()
            || r.lse().read().0 != self.lse.0
            || r.lse().read().stable()
            || flags.lsestable()
            || flags.lserdy()
            || ier.lserdy()
            || ier.lsefail()
            || ier.lsefault()
            || r.cr0().read().sysclk() == ClockSource::Lse
        {
            return Err(Error::LseClockInUse);
        }
        Ok(())
    }

    fn verify_tree(
        self,
        config: Config,
        lse: Lse,
        trim: u16,
        source: ClockSource,
        ahb: u8,
        apb: u8,
        flash: u32,
        cs: critical_section::CriticalSection<'_>,
    ) -> Result<(), Error> {
        let r = pac::SYSCTRL;
        let check = || {
            self.check(config, true)?;
            let clock = r.cr0().read();
            let hsi = r.hsi().read();
            if clock.sysclk() != source
                || clock.hclkprs() != ahb
                || clock.pclkprs() != apb
                || !r.cr1().read().hsien()
                || !hsi.stable()
                || hsi.trim() != trim
                || hsi.div() != config.hsi.div as u8
            {
                return Err(Error::ClockConfigurationTimeout);
            }
            if !self.monitor.ready(true) {
                return Err(Error::LsiTimeout);
            }
            if !r.ahben().read().flash() {
                return Err(Error::FlashClockTimeout);
            }
            if pac::FLASH.cr2().read().wait() != flash as u8 {
                return Err(Error::FlashLatencyTimeout);
            }
            self.hse_ready(config)?;
            if !lse::healthy(lse)
                || !r.isr().read().lsestable()
                || (self.monitor.sources.lseen() && r.lse().read().0 != self.lse.0)
            {
                return Err(Error::LseNotReady);
            }
            Ok(())
        };
        check()?;
        // Bank inspection can advance sampling and events. Check the whole
        // tree again after all pad work, before returning an accepted use edge.
        lse::verify(lse, cs)?;
        self.hse_pads(config, cs)?;
        check()
    }
}

fn admit_owners(
    config: Config,
    entry: Entry,
    needs_trim: bool,
    cs: critical_section::CriticalSection<'_>,
) -> Result<(), Error> {
    let autotrim = <crate::peripherals::AUTOTRIM as crate::rcc::SealedRccPeripheral>::RCC_INFO
        .inspect_for_init(cs, config.timeout, || pac::AUTOTRIM.cr().read())
        .map_err(|_| Error::RetainedClockInspectionTimeout)?;
    // The unconditional native guard in Entry::admit already rejects AUTO,
    // active calibration and all reserved modes/sources, even when disabled.
    if needs_trim && autotrim.en() && autotrim.src() == pac::autotrim::vals::Source::HsiOsc {
        return Err(Error::HsiClockInUse);
    }
    if needs_trim
        && pac::LVD.cr0().read().en()
        && pac::LVD.cr1().read().flten()
        && pac::LVD.cr1().read().fltclk()
    {
        return Err(Error::HsiClockInUse);
    }
    let rtc = <crate::peripherals::RTC as crate::rcc::SealedRccPeripheral>::RCC_INFO
        .inspect_for_init(cs, config.timeout, || pac::RTC.cr1().read().source())
        .map_err(|_| Error::RetainedClockInspectionTimeout)?;
    if !matches!(
        rtc,
        pac::rtc::vals::Source::Lse
            | pac::rtc::vals::Source::Lsi
            | pac::rtc::vals::Source::HseDiv128
            | pac::rtc::vals::Source::HseDiv256
            | pac::rtc::vals::Source::HseDiv512
            | pac::rtc::vals::Source::HseDiv1024
    ) {
        return Err(Error::RetainedRtcConfiguration);
    }
    if let Some(hse) = config.hse {
        if autotrim.en() && autotrim.src() == pac::autotrim::vals::Source::Etr {
            return Err(Error::HsePinInUse);
        }
        let owned = matches!(
            rtc,
            pac::rtc::vals::Source::HseDiv128
                | pac::rtc::vals::Source::HseDiv256
                | pac::rtc::vals::Source::HseDiv512
                | pac::rtc::vals::Source::HseDiv1024
        ) || (autotrim.en() && autotrim.src() == pac::autotrim::vals::Source::Hse);
        // Every enabled requested HSE is strict reuse, regardless of whether
        // a retained RTC/AUTOTRIM owner was found. A live mismatch is rejected.
        if entry.monitor.sources.hseen() || owned {
            verify_hse_source(hse, config.timeout, cs)?;
        }
    }
    if entry.monitor.sources.hseen() {
        if !pac::SYSCTRL.cr1().read().hseen()
            || !pac::SYSCTRL.hse().read().stable()
            || pac::SYSCTRL.hse().read().0 != entry.hse.0
        {
            return Err(Error::HseClockInUse);
        }
        entry.hse_pads(config, cs)?;
    }
    entry.check(config, false)
}

fn hsi_ready(trim: u16, div: u8) -> bool {
    let hsi = pac::SYSCTRL.hsi().read();
    pac::SYSCTRL.cr1().read().hsien() && hsi.stable() && hsi.trim() == trim && hsi.div() == div
}

pub(super) fn configure(
    config: Config,
    cs: critical_section::CriticalSection<'_>,
) -> Result<Clocks, Error> {
    let mut clocks = config.frequencies()?;
    let (lse, _) = clocks.lse.ok_or(Error::LseNotConfigured)?;
    let r = pac::SYSCTRL;
    let entry = Entry::admit(lse.poll_budget, cs)?;
    entry.check(config, false)?;
    let reused = lse::preflight(lse, cs)?;
    if reused != entry.monitor.sources.lseen() || r.lse().read().0 != entry.lse.0 {
        return Err(Error::LseClockInUse);
    }
    if reused && (!entry.monitor.ready(false) || !r.isr().read().lsestable()) {
        return Err(Error::LseNotReady);
    }
    if !reused {
        entry.cold_lse(config)?;
    }
    let raw =
        unsafe { core::ptr::read_volatile(crate::RCC_FACTORY_HSI_TRIM_ADDRESS as *const u16) };
    if raw == u16::MAX {
        return Err(Error::InvalidCalibration);
    }
    let mut calibrated = pac::sysctrl::regs::Hsi::default();
    calibrated.set_trim(raw);
    let trim = calibrated.trim();
    let needs_trim = entry.hsi.trim() != trim;
    admit_owners(config, entry, needs_trim, cs)?;
    // Preserve the existing helper and its unconditional AUTOTRIM check plus
    // two stopped/consumer/stopped passes exactly. Reclassify after any write.
    prepare_lse_monitor(lse.poll_budget, cs)?;
    entry.monitor.classify(true)?;

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
    let ahb = entry.clock.hclkprs().max(AHBPrescaler::Div4 as u8);
    let apb = entry.clock.pclkprs().max(APBPrescaler::Div8 as u8);
    entry.check(config, false)?;
    r.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_hclkprs(ahb);
        w.set_pclkprs(apb);
    });
    entry.wait(
        config,
        false,
        config.timeout,
        Error::ClockConfigurationTimeout,
        || {
            let v = r.cr0().read();
            v.sysclk() == entry.clock.sysclk() && v.hclkprs() == ahb && v.pclkprs() == apb
        },
    )?;
    barrier();

    r.cr1().modify(|w| {
        w.set_key(0x5a5a);
        w.set_hsien(true);
    });
    entry.wait(config, false, config.timeout, Error::HsiTimeout, || {
        hsi_ready(entry.hsi.trim(), entry.hsi.div())
    })?;
    r.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_sysclk(ClockSource::Hsi);
    });
    entry.wait(
        config,
        false,
        config.timeout,
        Error::ClockSwitchTimeout,
        || r.cr0().read().sysclk() == ClockSource::Hsi,
    )?;
    barrier();

    entry.monitor.classify(true)?;
    if !r.cr1().read().lsien() {
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_lsien(true);
        });
    }
    for attempt in 0..lse.poll_budget {
        entry.check(config, false)?;
        entry.monitor.classify(true)?;
        if entry.monitor.ready(false) {
            break;
        }
        if attempt + 1 == lse.poll_budget {
            return Err(Error::LsiTimeout);
        }
        core::hint::spin_loop();
    }
    if !entry.monitor.ready(false) {
        return Err(Error::LsiTimeout);
    }

    if needs_trim {
        // The factory monitor is ready immediately before this private bridge.
        entry.check(config, false)?;
        if !entry.monitor.ready(false) {
            return Err(Error::LsiTimeout);
        }
        r.cr0().modify(|w| {
            w.set_key(0x5a5a);
            w.set_sysclk(ClockSource::Lsi);
        });
        entry.wait(
            config,
            false,
            config.timeout,
            Error::TemporaryClockSwitchTimeout,
            || r.cr0().read().sysclk() == ClockSource::Lsi && entry.monitor.ready(false),
        )?;
        barrier();
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_hsien(false);
        });
        entry.wait(config, false, config.timeout, Error::HsiStopTimeout, || {
            !r.cr1().read().hsien() && !r.hsi().read().stable() && entry.monitor.ready(false)
        })?;
        // L052 HSI has NO WAIT field. Only TRIM changes while stopped; DIV
        // and reserved bits are retained by this typed modification.
        r.hsi().modify(|w| w.set_trim(trim));
        entry.wait(
            config,
            false,
            config.timeout,
            Error::ClockConfigurationTimeout,
            || {
                let hsi = r.hsi().read();
                hsi.trim() == trim
                    && hsi.div() == entry.hsi.div()
                    && !r.cr1().read().hsien()
                    && !hsi.stable()
                    && entry.monitor.ready(false)
            },
        )?;
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_hsien(true);
        });
        entry.wait(config, false, config.timeout, Error::HsiTimeout, || {
            hsi_ready(trim, entry.hsi.div()) && entry.monitor.ready(false)
        })?;
        r.cr0().modify(|w| {
            w.set_key(0x5a5a);
            w.set_sysclk(ClockSource::Hsi);
        });
        entry.wait(
            config,
            false,
            config.timeout,
            Error::ClockSwitchTimeout,
            || {
                r.cr0().read().sysclk() == ClockSource::Hsi
                    && hsi_ready(trim, entry.hsi.div())
                    && entry.monitor.ready(false)
            },
        )?;
        barrier();
    }
    if r.cr0().read().sysclk() != ClockSource::Hsi
        || !hsi_ready(trim, entry.hsi.div())
        || !entry.monitor.ready(false)
    {
        return Err(Error::ClockConfigurationTimeout);
    }
    // Own section4.5.2 permits a live DIV-only update with TRIM unchanged.
    r.hsi().modify(|w| w.set_div(config.hsi.div as u8));
    entry.wait(
        config,
        false,
        config.timeout,
        Error::ClockConfigurationTimeout,
        || {
            hsi_ready(trim, config.hsi.div as u8)
                && r.cr0().read().sysclk() == ClockSource::Hsi
                && entry.monitor.ready(false)
        },
    )?;

    if let Some(hse) = config.hse.filter(|_| !entry.monitor.sources.hseen()) {
        admit_owners(config, entry, needs_trim, cs)?;
        if r.cr1().read().hseen() || r.hse().read().0 != entry.hse.0 || r.hse().read().stable() {
            return Err(Error::HseClockInUse);
        }
        crate::rcc_configure_hse_pins(hse.mode == HseMode::Bypass, config.timeout, cs)?;
        if r.cr1().read().hseen() || r.hse().read().0 != entry.hse.0 || !entry.monitor.ready(false)
        {
            return Err(Error::HseClockInUse);
        }
        let detector = hse.detector_count()?;
        r.hse().modify(|w| {
            w.set_mode(hse.mode == HseMode::Bypass);
            w.set_driver(hse.drive);
            w.set_pdriver(hse.drive);
            w.set_freqrange(hse.range());
            w.set_pfreqrange(hse.range());
            w.set_waitcycle(pac::sysctrl::vals::HseWait::Cycles262144);
            w.set_flt(false);
            w.set_detcnt(detector);
        });
        entry.wait(
            config,
            false,
            config.timeout,
            Error::ClockConfigurationTimeout,
            || {
                hse_parameters_match(hse).unwrap_or(false)
                    && !r.cr1().read().hseen()
                    && !r.hse().read().stable()
                    && entry.monitor.ready(false)
            },
        )?;
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_hseen(true);
        });
        entry.wait(config, false, config.timeout, Error::HseTimeout, || {
            r.cr1().read().hseen() && r.hse().read().stable() && entry.monitor.ready(false)
        })?;
    }
    entry.hse_ready(config)?;
    entry.hse_pads(config, cs)?;
    entry.check(config, false)?;
    if !hsi_ready(trim, config.hsi.div as u8) || !entry.monitor.ready(false) {
        return Err(Error::ClockConfigurationTimeout);
    }

    if !reused {
        entry.cold_lse(config)?;
        if lse::preflight(lse, cs)? {
            return Err(Error::LseClockInUse);
        }
        entry.cold_lse(config)?;
    }
    if !entry.monitor.ready(false) {
        return Err(Error::LsiTimeout);
    }
    lse::freeze_sysclk_monitor(cs)?;
    if !reused {
        let bypass = lse.mode == LseMode::Bypass;
        crate::rcc_configure_lse_pins(bypass, lse.poll_budget, cs)?;
        // Pad inspection runs a whole bank. Repeat source/consumer admission
        // afterward; use configured-pad mode rather than claiming unused pins.
        entry.cold_lse(config)?;
        if !crate::rcc_lse_consumers_idle(lse.poll_budget, cs)?
            || !crate::rcc_lse_pins_match(bypass, false, lse.poll_budget, cs)?
        {
            return Err(Error::LseClockInUse);
        }
        entry.cold_lse(config)?;
        if !entry.monitor.ready(false) {
            return Err(Error::LsiTimeout);
        }
        r.lse().modify(|w| {
            w.set_mode(bypass);
            w.set_driver(lse.drive);
            w.set_amp(lse.amplitude);
            w.set_pdriver(lse.startup_drive);
            w.set_pamp(lse.startup_amplitude);
            w.set_waitcycle(lse.wait);
        });
        let parameters = r.lse().read();
        if parameters.mode() != bypass
            || parameters.driver() != lse.drive
            || parameters.amp() != lse.amplitude
            || parameters.pdriver() != lse.startup_drive
            || parameters.pamp() != lse.startup_amplitude
            || parameters.waitcycle() != lse.wait
            || parameters.stable()
            || r.cr1().read().lseen()
            || !entry.monitor.ready(false)
        {
            return Err(Error::LseNotReady);
        }
        entry.check(config, false)?;
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_lseccs(true);
            w.set_lseen(true);
        });
    }
    entry.wait(config, true, lse.poll_budget, Error::LseNotReady, || {
        lse::healthy(lse) && r.isr().read().lsestable() && entry.monitor.ready(true)
    })?;

    // Final dividers are relaxed while still running configured HSI and WAIT2.
    // Pure validation proved this use edge independently of the slow target.
    entry.verify_tree(
        config,
        lse,
        trim,
        ClockSource::Hsi,
        ahb,
        apb,
        crate::RCC_INITIAL_FLASH_WAIT,
        cs,
    )?;
    r.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_sysclk(ClockSource::Hsi);
        w.set_hclkprs(config.ahb_pre as u8);
        w.set_pclkprs(config.apb_pre as u8);
    });
    entry.wait(
        config,
        true,
        config.timeout,
        Error::ClockConfigurationTimeout,
        || {
            let v = r.cr0().read();
            v.sysclk() == ClockSource::Hsi
                && v.hclkprs() == config.ahb_pre as u8
                && v.pclkprs() == config.apb_pre as u8
        },
    )?;
    barrier();
    entry.verify_tree(
        config,
        lse,
        trim,
        ClockSource::Hsi,
        config.ahb_pre as u8,
        config.apb_pre as u8,
        crate::RCC_INITIAL_FLASH_WAIT,
        cs,
    )?;

    // The single intentional final LSE mux write. NO CR0 mutation follows,
    // even on failure: an RMW could replay LSE after asynchronous fallback.
    r.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_sysclk(ClockSource::Lse);
    });
    entry.wait(
        config,
        true,
        config.timeout,
        Error::ClockSwitchTimeout,
        || {
            let v = r.cr0().read();
            v.sysclk() == ClockSource::Lse
                && v.hclkprs() == config.ahb_pre as u8
                && v.pclkprs() == config.apb_pre as u8
        },
    )?;
    barrier();
    entry.verify_tree(
        config,
        lse,
        trim,
        ClockSource::Lse,
        config.ahb_pre as u8,
        config.apb_pre as u8,
        crate::RCC_INITIAL_FLASH_WAIT,
        cs,
    )?;
    let upper = clocks
        .hclk_bounds()
        .maximum()
        .0
        .max(
            crate::rcc::ClockBounds::hsi(config.hsi.div.divisor())
                .divided_by(config.ahb_pre.divisor())
                .maximum()
                .0,
        )
        .max(
            crate::rcc::ClockBounds::hsi(crate::RCC_FIXED_CCS_HSI_DIVISOR)
                .maximum()
                .0,
        );
    let wait = (upper - 1) / crate::RCC_FLASH_WAIT_STEP_HZ;
    set_flash_latency(wait, config.timeout)?;
    entry.verify_tree(
        config,
        lse,
        trim,
        ClockSource::Lse,
        config.ahb_pre as u8,
        config.apb_pre as u8,
        wait,
        cs,
    )?;
    if entry.monitor.sources.hseen() {
        if !entry.hse.mode() {
            clocks.hse = Some(HseMode::Oscillator);
        } else if clocks.hse.is_none() {
            clocks.hse = Some(HseMode::Bypass);
        }
    }
    // No fallible work follows. init publishes CLOCKS in this same critical
    // section immediately after returning. Failures publish neither record.
    SUCCESS.borrow(cs).set(Some(entry.monitor));
    Ok(clocks)
}
