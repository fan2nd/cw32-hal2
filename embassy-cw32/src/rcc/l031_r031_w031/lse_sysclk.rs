use super::*;

// Only a requested LSE source needs this monitored-source policy. Default None
// retains the original RCC path. Factory-valid live LSI is never retuned.
#[cfg(rcc_lse)]
pub(super) fn prepare_lse_monitor(
    timeout: u32,
    cs: critical_section::CriticalSection<'_>,
) -> Result<(), Error> {
    use crate::rtc::sealed::Instance;
    let r = pac::SYSCTRL;
    let factory = unsafe {
        core::ptr::read_volatile(crate::peripherals::RTC::FACTORY_TRIM_ADDRESS as *const u16)
    };
    if factory == u16::MAX {
        return Err(Error::LseNotReady);
    }
    let mut calibration = pac::sysctrl::regs::Lsi::default();
    calibration.set_trim(factory);
    let before = r.lsi().read();
    // An enabled or pending ready interrupt is a retained observer when this
    // request would start LSI. Never clear it or mask the peripheral's owner.
    if !r.cr1().read().lsien() && (r.ier().read().lsirdy() || r.isr().read().lsirdy()) {
        return Err(Error::LseClockInUse);
    }
    if before.trim() == calibration.trim() {
        return Ok(());
    }
    // LSIEN alone is not used as ownership proof. The generated gate-preserving
    // checks cover every reviewed direct selector and detector, twice, with
    // unchanged native trim/wait and both hardware stable indications low.
    let stopped = || {
        let control = r.cr1().read();
        let lsi = r.lsi().read();
        !control.lsien()
            && !lsi.stable()
            && !r.isr().read().lsistable()
            && !control.hseccs()
            && !control.lseccs()
            && !r.ier().read().lsirdy()
            && !r.isr().read().lsirdy()
            && lsi.trim() == before.trim()
            && lsi.waitcycle() == before.waitcycle()
    };
    for _ in 0..2 {
        if !stopped() || !crate::rcc_lsi_consumers_idle(timeout, cs)? || !stopped() {
            return Err(Error::LseClockInUse);
        }
    }
    // Own manuals require parameters before enable. LSI has no write key.
    // Keep WAIT and every unrelated/reserved field; never stop a live source.
    r.lsi().modify(|w| w.set_trim(calibration.trim()));
    wait_until(timeout, Error::LseNotReady, || {
        let lsi = r.lsi().read();
        lsi.trim() == calibration.trim() && lsi.waitcycle() == before.waitcycle()
    })
}

// The stronger monitor contract belongs only to a successful LSE system target.
// The common auxiliary leaf keeps its historical TRIM/WAIT/direct-STABLE checks.
#[cfg(rcc_lse)]
pub(super) static LSE_SYSCLK_MONITOR: Mutex<Cell<Option<LseSysclkState>>> = Mutex::new(Cell::new(None));

#[cfg(rcc_lse)]
#[derive(Clone, Copy)]
pub(super) struct LseSysclkState {
    trim: u16,
    wait: u8,
    pub(super) sources: pac::sysctrl::regs::Cr1,
    pub(super) hse: pac::sysctrl::regs::Hse,
    lse: pac::sysctrl::regs::Lse,
}

#[cfg(rcc_lse)]
impl LseSysclkState {
    pub(super) fn admit() -> Result<Self, Error> {
        use crate::rtc::sealed::Instance;
        let r = pac::SYSCTRL;
        let factory = unsafe {
            core::ptr::read_volatile(crate::peripherals::RTC::FACTORY_TRIM_ADDRESS as *const u16)
        };
        // Reject the erased raw word before native masking; zero is legal.
        if factory == u16::MAX {
            return Err(Error::LseNotReady);
        }
        let mut calibration = pac::sysctrl::regs::Lsi::default();
        calibration.set_trim(factory);
        let lsi = r.lsi().read();
        let state = Self {
            trim: calibration.trim(),
            wait: lsi.waitcycle(),
            sources: r.cr1().read(),
            hse: r.hse().read(),
            lse: r.lse().read(),
        };
        // Selected-source stop inhibition means EN=0 cannot establish a fresh
        // source. Never repair an unenabled/unready selected LSE as cold entry.
        if r.cr0().read().sysclk() == ClockSource::Lse
            && (!state.sources.lseen() || !state.lse.stable())
        {
            return Err(Error::LseClockInUse);
        }
        state.classify_monitor(false)?;
        Ok(state)
    }

    // This classification owns no parameters and does no consumer inspection.
    // The existing own-family prepare_lse_monitor still supplies both complete
    // stopped/consumer/stopped passes for every mismatching trim. A matching
    // stopped source keeps its no-write/no-parameter-owner-proof path, although
    // requesting it may resume parked direct consumers (public handover rule).
    fn classify_monitor(self, require_factory: bool) -> Result<(), Error> {
        let r = pac::SYSCTRL;
        let control = r.cr1().read();
        let lsi = r.lsi().read();
        let flags = r.isr().read();
        let selected = r.cr0().read().sysclk() == ClockSource::Lsi;
        let matching = lsi.trim() == self.trim;
        let stable = lsi.stable() && flags.lsistable();
        if lsi.waitcycle() != self.wait
            || (require_factory && !matching)
            || control.clkccs() != self.sources.clkccs()
            || control.hseccs() != self.sources.hseccs()
            || control.lseccs() != self.sources.lseccs()
            || control.lselock() != self.sources.lselock()
            // Separate reads can straddle real startup. Rejection is a
            // conservative handover policy, not evidence of defective silicon.
            || lsi.stable() != flags.lsistable()
            || (!control.lsien() && (stable || selected))
            || (!stable && selected)
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

    pub(super) fn admit_request(self) -> Result<(), Error> {
        // prepare_lse_monitor has now either preserved matching TRIM or applied
        // an admitted trim-only update. WAIT and CCS policy must still match.
        self.classify_monitor(true)
    }

    pub(super) fn monitor_ready(self, lse_started: bool) -> bool {
        let r = pac::SYSCTRL;
        let control = r.cr1().read();
        let lsi = r.lsi().read();
        control.lsien()
            && lsi.stable()
            && r.isr().read().lsistable()
            && lsi.trim() == self.trim
            && lsi.waitcycle() == self.wait
            && control.clkccs() == self.sources.clkccs()
            && control.hseccs() == self.sources.hseccs()
            && control.lseccs() == (lse_started || self.sources.lseccs())
            && control.lselock() == self.sources.lselock()
    }

    pub(super) fn verify_tree(
        self,
        config: Config,
        lse: crate::rcc::Lse,
        hsi_trim: u16,
        reused_lse: bool,
        cs: critical_section::CriticalSection<'_>,
    ) -> Result<(), Error> {
        let r = pac::SYSCTRL;
        let verify_sources = || {
            let control = r.cr1().read();
            let hsi = r.hsi().read();
            if !self.monitor_ready(true) {
                return Err(Error::LsiTimeout);
            }
            if !control.hsien()
                || !hsi.stable()
                || hsi.trim() != hsi_trim
                || hsi.div() != config.hsi.div as u8
                || !control.lseen()
                || control.hseen() != (self.sources.hseen() || config.hse.is_some())
            {
                return Err(Error::ClockConfigurationTimeout);
            }
            if let Some(hse) = config.hse {
                if !r.hse().read().stable() || !hse_parameters_match(hse)? {
                    return Err(Error::HseTimeout);
                }
            } else if self.sources.hseen()
                && (!r.hse().read().stable() || r.hse().read().0 != self.hse.0)
            {
                return Err(Error::HseClockInUse);
            }
            if reused_lse && r.lse().read().0 != self.lse.0 {
                return Err(Error::LseNotReady);
            }
            check_external_faults(control.hseen(), true)?;
            if !crate::rcc::lse::healthy(lse) {
                return Err(Error::LseNotReady);
            }
            Ok(())
        };
        verify_sources()?;
        // Each gate-preserving pad inspection can advance bank sampling and
        // events. Check the source tree again after all of that work.
        crate::rcc::lse::verify(lse, cs)?;
        let hse_bypass = config
            .hse
            .map(|hse| hse.mode == HseMode::Bypass)
            .or_else(|| self.sources.hseen().then_some(self.hse.mode()));
        if let Some(bypass) = hse_bypass {
            if !crate::rcc_hse_pins_match(bypass, config.timeout, cs)? {
                return Err(Error::HseClockInUse);
            }
        }
        verify_sources()
    }
}

