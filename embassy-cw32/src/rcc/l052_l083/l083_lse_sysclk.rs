//! Exact-package L083 init-only LSE system target; old auxiliary paths are separate.
//! Own RM CN V2.0 sections4.4.2,4.4.3,4.5,4.7 and DS CN V1.9 tables7-4,7-17/18.
//! The inherited HSI must independently be within its documented safe calibration
//! regime, even when idle at entry. Its rate is not inferred from STABLE or DIV.
//! PLL/MCO/dedicated output consumers must be quiescent: stopping PLL stops its
//! outputs. No source-loss survival, recovery or post-fault timing is promised.
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
        let mut current = pac::SYSCTRL.cr1().read();
        let mut expected = self.sources;
        // Source enables have separate phase checks. Every other control and
        // reserved bit is preserved, except explicitly starting fresh LSE CCS.
        expected.set_hsien(current.hsien());
        expected.set_hseen(current.hseen());
        expected.set_pllen(current.pllen());
        expected.set_lsien(current.lsien());
        expected.set_lseen(current.lseen());
        expected.set_lseccs(self.sources.lseccs() || lse_started);
        expected.set_key(0);
        current.set_key(0);
        current.0 == expected.0
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
    fn classify(self, require_factory: bool, lse_started: bool) -> Result<(), Error> {
        let r = pac::SYSCTRL;
        let control = r.cr1().read();
        let lsi = r.lsi().read();
        let flags = r.isr().read();
        let selected = r.cr0().read().sysclk() == ClockSource::Lsi;
        let matching = lsi.trim() == self.trim;
        let stable = lsi.stable() && flags.lsistable();
        if !self.policy(lse_started)
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
    lsi: pac::sysctrl::regs::Lsi,
    pll: pac::sysctrl::regs::Pll,
    pll_phase: PllPhase,
    factory_monitor: bool,
    monitor_live: bool,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum PllPhase {
    Inherited,
    Stopping,
    Stopped,
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
            lsi: r.lsi().read(),
            pll: r.pll().read(),
            pll_phase: PllPhase::Inherited,
            factory_monitor: false,
            monitor_live: false,
        };
        match entry.clock.sysclk() {
            ClockSource::Hsi
            | ClockSource::Hse
            | ClockSource::Lsi
            | ClockSource::Lse
            | ClockSource::Pll => {}
            _ => return Err(Error::InvalidClockSource),
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
        if !sources.hsien() && entry.hsi.stable() {
            return Err(Error::HsiClockInUse);
        }
        entry.monitor.classify(false, false)?;
        if entry.clock.sysclk() == ClockSource::Pll && !sources.pllen() {
            return Err(Error::PllConfigurationTimeout);
        }
        if sources.pllen() {
            if !entry.pll.stable() {
                return Err(Error::PllTimeout);
            }
            if entry.pll.reserved_debug().to_bits() != crate::RCC_PLL_RESERVED_DEBUG_DEFAULT {
                return Err(Error::PllReservedConfiguration);
            }
            if !(crate::RCC_PLL_MULTIPLIER_RANGE.0..=crate::RCC_PLL_MULTIPLIER_RANGE.1)
                .contains(&entry.pll.mul().to_bits())
            {
                return Err(Error::InvalidPllMultiplier);
            }
        } else if entry.pll.stable() {
            return Err(Error::PllStopTimeout);
        }
        entry.check_pll()?;
        Ok(entry)
    }

    fn check_pll(self) -> Result<(), Error> {
        use pac::sysctrl::vals::PllSource;
        let r = pac::SYSCTRL;
        let pll = r.pll().read();
        // Compare native parameters, including the otherwise unexposed upper
        // reserved region. No PLL parameter or reserved/debug writes occur.
        if pll.source() != self.pll.source()
            || pll.freqin() != self.pll.freqin()
            || pll.mul() != self.pll.mul()
            || pll.freqout() != self.pll.freqout()
            || pll.waitcycle() != self.pll.waitcycle()
            || pll.reserved_debug() != self.pll.reserved_debug()
            || pll.0 >> 20 != self.pll.0 >> 20
        {
            return Err(Error::PllConfigurationTimeout);
        }
        let enabled = r.cr1().read().pllen();
        match self.pll_phase {
            PllPhase::Inherited => {
                if enabled != self.monitor.sources.pllen() || pll.stable() != self.pll.stable() {
                    return Err(Error::PllConfigurationTimeout);
                }
            }
            PllPhase::Stopping => {}
            PllPhase::Stopped => {
                if enabled || pll.stable() {
                    return Err(Error::PllStopTimeout);
                }
            }
        }
        // HSI SOURCE3 consumes post-DIV HSI. Neither trim nor DIV can change
        // until BOTH PLL stop acknowledgments, even after SYSCLK left PLL.
        if self.pll_phase != PllPhase::Stopped && self.monitor.sources.pllen() {
            match self.pll.source() {
                PllSource::Hsi => {
                    if !r.cr1().read().hsien()
                        || !r.hsi().read().stable()
                        || r.hsi().read().0 != self.hsi.0
                    {
                        return Err(Error::PllConfigurationTimeout);
                    }
                }
                PllSource::HseCrystal | PllSource::HseBypass => {
                    if !self.monitor.sources.hseen()
                        || !r.cr1().read().hseen()
                        || !r.hse().read().stable()
                        || r.hse().read().0 != self.hse.0
                        || self.hse.mode() != (self.pll.source() == PllSource::HseBypass)
                    {
                        return Err(Error::PllConfigurationTimeout);
                    }
                }
                _ => return Err(Error::PllConfigurationTimeout),
            }
        }
        Ok(())
    }

    fn before_escape(self, config: Config) -> Result<(), Error> {
        self.check(config, false)?;
        let r = pac::SYSCTRL;
        let mut control = r.cr1().read();
        let mut expected = self.monitor.sources;
        // HSIEN is the only permitted source mutation before the first CR0
        // write. Ignore its startup progress and the write-only key here.
        control.set_hsien(expected.hsien());
        control.set_key(0);
        expected.set_key(0);
        let clock = r.cr0().read();
        let hsi = r.hsi().read();
        if control.0 != expected.0
            || clock.sysclk() != self.clock.sysclk()
            || clock.hclkprs() != self.clock.hclkprs()
            || clock.pclkprs() != self.clock.pclkprs()
            || hsi.trim() != self.hsi.trim()
            || hsi.div() != self.hsi.div()
            || r.hse().read().0 != self.hse.0
            || r.lse().read().0 != self.lse.0
        {
            return Err(Error::ClockConfigurationTimeout);
        }
        Ok(())
    }

    fn verify_bridge(
        self,
        config: Config,
        trim: u16,
        div: u8,
        ahb: u8,
        apb: u8,
    ) -> Result<(), Error> {
        self.check(config, false)?;
        if !protected_clock_matches(ClockSource::Hsi, ahb, apb)
            || !hsi_ready(trim, div)
            || !self.monitor.ready(false)
        {
            return Err(Error::ClockConfigurationTimeout);
        }
        Ok(())
    }

    fn check(self, config: Config, lse_started: bool) -> Result<(), Error> {
        self.check_pll()?;
        self.monitor.classify(self.factory_monitor, lse_started)?;
        let r = pac::SYSCTRL;
        if !self.factory_monitor && r.lsi().read().trim() != self.lsi.trim() {
            return Err(Error::LseClockInUse);
        }
        if self.monitor_live && !self.monitor.ready(lse_started) {
            return Err(Error::LsiTimeout);
        }
        if self.monitor.sources.lseen() {
            if !r.cr1().read().lseen()
                || r.lse().read().0 != self.lse.0
                || !r.lse().read().stable()
                || !r.isr().read().lsestable()
            {
                return Err(Error::LseClockInUse);
            }
        } else if !lse_started && r.cr1().read().lseen() {
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
        if !self.monitor.sources.hseen() && config.hse.is_none() && r.cr1().read().hseen() {
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
                self.check(config, lse_started)?;
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

fn protected_clock_matches(source: ClockSource, ahb: u8, apb: u8) -> bool {
    let clock = pac::SYSCTRL.cr0().read();
    clock.sysclk() == source
        && clock.hclkprs() == ahb
        && clock.pclkprs() == apb
        && pac::SYSCTRL.ahben().read().flash()
        && pac::FLASH.cr2().read().wait() == crate::RCC_INITIAL_FLASH_WAIT as u8
}

pub(super) fn configure(
    config: Config,
    cs: critical_section::CriticalSection<'_>,
) -> Result<Clocks, Error> {
    let mut clocks = config.frequencies()?;
    let (lse, _) = clocks.lse.ok_or(Error::LseNotConfigured)?;
    let r = pac::SYSCTRL;
    let mut entry = Entry::admit(lse.poll_budget, cs)?;
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
    // Classify every inherited detector before any oscillator mutation. A
    // mismatching stopped LSI is intentionally not written until PLL is off.
    entry.before_escape(config)?;
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
    entry.before_escape(config)?;
    let ahb = entry.clock.hclkprs().max(AHBPrescaler::Div4 as u8);
    let apb = entry.clock.pclkprs().max(APBPrescaler::Div8 as u8);

    if !r.cr1().read().hsien() {
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_hsien(true);
        });
    }
    for _ in 0..config.timeout {
        entry.before_escape(config)?;
        if hsi_ready(entry.hsi.trim(), entry.hsi.div()) {
            break;
        }
        core::hint::spin_loop();
    }
    entry.before_escape(config)?;
    if !hsi_ready(entry.hsi.trim(), entry.hsi.div()) {
        return Err(Error::HsiTimeout);
    }
    if !r.ahben().read().flash()
        || pac::FLASH.cr2().read().wait() != crate::RCC_INITIAL_FLASH_WAIT as u8
    {
        return Err(Error::FlashLatencyTimeout);
    }
    // First CR0 write for EVERY entry source. Explicit HSI avoids replaying
    // stale HSE/LSE after CCS fallback. WAIT2 and independently legal incoming
    // HSI make either internal field-update order safe; guards never relax.
    r.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_sysclk(ClockSource::Hsi);
        w.set_hclkprs(ahb);
        w.set_pclkprs(apb);
    });
    entry.wait(
        config,
        false,
        config.timeout,
        Error::ClockSwitchTimeout,
        || {
            protected_clock_matches(ClockSource::Hsi, ahb, apb)
                && hsi_ready(entry.hsi.trim(), entry.hsi.div())
        },
    )?;
    barrier();

    // Keep the original post-DIV HSI or HSE reference unchanged through BOTH
    // PLL stop acknowledgments. Preserved parameters do not preserve outputs.
    if entry.monitor.sources.pllen() {
        entry.check(config, false)?;
        entry.pll_phase = PllPhase::Stopping;
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_pllen(false);
        });
        entry.wait(config, false, config.timeout, Error::PllStopTimeout, || {
            !r.cr1().read().pllen()
                && !r.pll().read().stable()
                && protected_clock_matches(ClockSource::Hsi, ahb, apb)
                && hsi_ready(entry.hsi.trim(), entry.hsi.div())
        })?;
    }
    entry.pll_phase = PllPhase::Stopped;
    entry.check(config, false)?;
    if !protected_clock_matches(ClockSource::Hsi, ahb, apb)
        || !hsi_ready(entry.hsi.trim(), entry.hsi.div())
    {
        return Err(Error::ClockConfigurationTimeout);
    }
    // Preserve the generic helper and its two complete stopped/consumer/
    // stopped passes. It deliberately rejects selected PLL, now safely left.
    prepare_lse_monitor(lse.poll_budget, cs)?;
    entry.factory_monitor = true;
    entry.check(config, false)?;
    if !protected_clock_matches(ClockSource::Hsi, ahb, apb)
        || !hsi_ready(entry.hsi.trim(), entry.hsi.div())
    {
        return Err(Error::ClockConfigurationTimeout);
    }
    if !r.cr1().read().lsien() {
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_lsien(true);
        });
    }
    for attempt in 0..lse.poll_budget {
        entry.check(config, false)?;
        entry.monitor.classify(true, false)?;
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
    entry.monitor_live = true;
    entry.verify_bridge(config, entry.hsi.trim(), entry.hsi.div(), ahb, apb)?;

    if needs_trim {
        // Reinspect raw-HSI owners before the private bridge; inspection can
        // advance hardware, so recheck the entire protected HSI use edge.
        admit_owners(config, entry, true, cs)?;
        entry.verify_bridge(config, entry.hsi.trim(), entry.hsi.div(), ahb, apb)?;
        r.cr0().modify(|w| {
            w.set_key(0x5a5a);
            w.set_sysclk(ClockSource::Lsi);
        });
        entry.wait(
            config,
            false,
            config.timeout,
            Error::TemporaryClockSwitchTimeout,
            || protected_clock_matches(ClockSource::Lsi, ahb, apb) && entry.monitor.ready(false),
        )?;
        barrier();
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_hsien(false);
        });
        entry.wait(config, false, config.timeout, Error::HsiStopTimeout, || {
            !r.cr1().read().hsien()
                && !r.hsi().read().stable()
                && r.hsi().read().trim() == entry.hsi.trim()
                && r.hsi().read().div() == entry.hsi.div()
                && protected_clock_matches(ClockSource::Lsi, ahb, apb)
                && entry.monitor.ready(false)
        })?;
        // L083 HSI has NO WAIT field. Only TRIM changes while stopped; DIV
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
                    && protected_clock_matches(ClockSource::Lsi, ahb, apb)
                    && entry.monitor.ready(false)
            },
        )?;
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_hsien(true);
        });
        entry.wait(config, false, config.timeout, Error::HsiTimeout, || {
            hsi_ready(trim, entry.hsi.div())
                && protected_clock_matches(ClockSource::Lsi, ahb, apb)
                && entry.monitor.ready(false)
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
                protected_clock_matches(ClockSource::Hsi, ahb, apb)
                    && hsi_ready(trim, entry.hsi.div())
                    && entry.monitor.ready(false)
            },
        )?;
        barrier();
    }
    entry.verify_bridge(config, trim, entry.hsi.div(), ahb, apb)?;
    // Own section4.5.2 permits a live DIV-only update with TRIM unchanged.
    r.hsi().modify(|w| w.set_div(config.hsi.div as u8));
    entry.wait(
        config,
        false,
        config.timeout,
        Error::ClockConfigurationTimeout,
        || {
            hsi_ready(trim, config.hsi.div as u8)
                && protected_clock_matches(ClockSource::Hsi, ahb, apb)
                && entry.monitor.ready(false)
        },
    )?;

    if let Some(hse) = config.hse.filter(|_| !entry.monitor.sources.hseen()) {
        admit_owners(config, entry, needs_trim, cs)?;
        entry.verify_bridge(config, trim, config.hsi.div as u8, ahb, apb)?;
        if r.cr1().read().hseen() || r.hse().read().0 != entry.hse.0 || r.hse().read().stable() {
            return Err(Error::HseClockInUse);
        }
        crate::rcc_configure_hse_pins(hse.mode == HseMode::Bypass, config.timeout, cs)?;
        entry.verify_bridge(config, trim, config.hsi.div as u8, ahb, apb)?;
        if r.cr1().read().hseen() || r.hse().read().0 != entry.hse.0 || !entry.monitor.ready(false)
        {
            return Err(Error::HseClockInUse);
        }
        let detector = hse.detector_count()?;
        r.hse().modify(|w| {
            w.set_mode(hse.mode == HseMode::Bypass);
            w.set_driver(hse.drive);
            w.set_freqrange(hse.range());
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
                    && protected_clock_matches(ClockSource::Hsi, ahb, apb)
                    && hsi_ready(trim, config.hsi.div as u8)
                    && entry.monitor.ready(false)
            },
        )?;
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_hseen(true);
        });
        entry.wait(config, false, config.timeout, Error::HseTimeout, || {
            r.cr1().read().hseen()
                && r.hse().read().stable()
                && protected_clock_matches(ClockSource::Hsi, ahb, apb)
                && hsi_ready(trim, config.hsi.div as u8)
                && entry.monitor.ready(false)
        })?;
    }
    entry.hse_ready(config)?;
    entry.hse_pads(config, cs)?;
    entry.hse_ready(config)?;
    entry.verify_bridge(config, trim, config.hsi.div as u8, ahb, apb)?;

    if !reused {
        entry.cold_lse(config)?;
        if lse::preflight(lse, cs)? {
            return Err(Error::LseClockInUse);
        }
        entry.cold_lse(config)?;
    }
    entry.verify_bridge(config, trim, config.hsi.div as u8, ahb, apb)?;
    entry.hse_ready(config)?;
    lse::freeze_sysclk_monitor(cs)?;
    entry.verify_bridge(config, trim, config.hsi.div as u8, ahb, apb)?;
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
        entry.verify_bridge(config, trim, config.hsi.div as u8, ahb, apb)?;
        entry.hse_ready(config)?;
        r.lse().modify(|w| {
            w.set_mode(bypass);
            w.set_driver(lse.drive);
            w.set_amp(lse.amplitude);
            w.set_waitcycle(lse.wait);
        });
        let parameters = r.lse().read();
        if parameters.mode() != bypass
            || parameters.driver() != lse.drive
            || parameters.amp() != lse.amplitude
            || parameters.waitcycle() != lse.wait
            || parameters.stable()
            || r.cr1().read().lseen()
            || !entry.monitor.ready(false)
        {
            return Err(Error::LseNotReady);
        }
        entry.verify_bridge(config, trim, config.hsi.div as u8, ahb, apb)?;
        entry.hse_ready(config)?;
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_lseccs(true);
            w.set_lseen(true);
        });
    }
    entry.wait(config, true, lse.poll_budget, Error::LseNotReady, || {
        lse::healthy(lse)
            && r.isr().read().lsestable()
            && entry.monitor.ready(true)
            && protected_clock_matches(ClockSource::Hsi, ahb, apb)
            && hsi_ready(trim, config.hsi.div as u8)
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
    // Conservative fallback covers raw factory HSI (48.96 MHz) without any
    // HSI/AHB/APB divider credit. WAIT2 stays installed; no latency or CR0
    // mutation follows the final complete use-edge check.
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
