use super::*;

/// Target-only identity from before the first configuration-gate write. Normal
/// native Admission remains responsible for oscillator/pad/monitor ownership.
#[cfg(rcc_lse)]
#[derive(Clone, Copy)]
pub(super) struct LseSysclkSnapshot {
    pub(super) sources: pac::sysctrl::regs::Cr1,
    pub(super) hsi: pac::sysctrl::regs::Hsi,
    pub(super) hse: pac::sysctrl::regs::Hse,
    pub(super) lsi: pac::sysctrl::regs::Lsi,
    pub(super) lse: pac::sysctrl::regs::Lse,
    pub(super) routes: pac::sysctrl::regs::Cr2,
    pub(super) interrupts: pac::sysctrl::regs::Ier,
    pub(super) mco: pac::sysctrl::regs::Mco,
}

#[cfg(rcc_lse)]
impl LseSysclkSnapshot {
    fn policy_unchanged(self) -> Result<(), Error> {
        let r = pac::SYSCTRL;
        // FLASHWAIT[6:4] shares the FLASH.WAIT function. Only our authoritative
        // FLASH write/readback licenses this typed field difference; never
        // recapture brake, RTC low-power, wake, debug or interrupt policy.
        let mut original = self.routes;
        let mut current = r.cr2().read();
        original.set_flashwait(0);
        current.set_flashwait(0);
        if current.0 != original.0
            || r.ier().read().0 != self.interrupts.0
            || r.mco().read().0 != self.mco.0
        {
            return Err(Error::LseClockInUse);
        }
        let lsi = r.lsi().read();
        // STABLE can make expected startup progress; TRIM and WAIT cannot.
        if lsi.trim() != self.lsi.trim() || lsi.waitcycle() != self.lsi.waitcycle() {
            return Err(Error::ClockConfigurationTimeout);
        }
        Ok(())
    }

    pub(super) fn before_lse_start(self, config: Config) -> Result<(), Error> {
        self.policy_unchanged()?;
        if pac::SYSCTRL.lse().read().0 != self.lse.0 {
            return Err(Error::LseClockInUse);
        }
        if config.lse.ok_or(Error::LseNotConfigured)?.monitored()
            && (!self.lsi.stable() || !pac::SYSCTRL.lsi().read().stable())
        {
            // Later enabling or readiness cannot manufacture inherited monitor
            // admission. The first native preflight established this early fact.
            return Err(Error::LseMonitorNotReady);
        }
        Ok(())
    }

    pub(super) fn hsi_ownership(self, trim: u16) -> Result<(), Error> {
        let changing = self.hsi.trim() != trim || !self.sources.hsien() || !self.hsi.stable();
        if changing
            && (pac::SYSCTRL.mco().read().source() == 3 || pac::SYSCTRL.ier().read().hsirdy())
        {
            // Own MCO SOURCE3 is raw HSIOSC. A start/restart can also latch
            // HSIRDY through an already enabled request even with IER untouched.
            return Err(Error::HsiClockInUse);
        }
        Ok(())
    }

    fn first_lsi_central(self, config: Config) -> Result<(), Error> {
        self.before_lse_start(config)?;
        let r = pac::SYSCTRL;
        let mut original = self.sources;
        let mut current = r.cr1().read();
        // Only the enclosing unchanged-HSI enable can precede the second pass.
        original.set_hsien(false);
        current.set_hsien(false);
        original.set_key(0);
        current.set_key(0);
        if current.0 != original.0 {
            return Err(Error::ClockConfigurationTimeout);
        }
        if r.mco().read().source() == 4 || r.ier().read().lsirdy() {
            return Err(Error::LsiClockInUse);
        }
        Ok(())
    }

    pub(super) fn admit_first_lsi_request(
        self,
        config: Config,
        cs: critical_section::CriticalSection<'_>,
    ) -> Result<(), Error> {
        use crate::rcc::SealedRccPeripheral;
        self.first_lsi_central(config)?;
        // Each native configuration gate is inspected/restored separately. A
        // restoration failure wins over a semantic conflict from its read.
        let rtc = crate::peripherals::RTC::RCC_INFO;
        if rtc.reset_asserted() {
            return Err(Error::LsiClockInUse);
        }
        let rtc_conflict = rtc
            .inspect_for_init(cs, config.timeout, || {
                let source = pac::RTC.cr1().read().source();
                source == 2 || source > 3 || rtc.reset_asserted()
            })
            .map_err(lse_target_inspection_error)?;
        if rtc_conflict || rtc.reset_asserted() {
            return Err(Error::LsiClockInUse);
        }
        for (info, uart) in [
            (crate::peripherals::UART1::RCC_INFO, pac::UART1),
            (crate::peripherals::UART2::RCC_INFO, pac::UART2),
            #[cfg(cw32l011)]
            (crate::peripherals::UART3::RCC_INFO, pac::UART3),
        ] {
            if info.reset_asserted() {
                return Err(Error::LsiClockInUse);
            }
            let conflict = info
                .inspect_for_init(cs, config.timeout, || {
                    uart.cr1().read().source() == pac::uart::vals::Source::Lsi
                        || info.reset_asserted()
                })
                .map_err(lse_target_inspection_error)?;
            if conflict || info.reset_asserted() {
                return Err(Error::LsiClockInUse);
            }
        }
        let lptim = crate::peripherals::LPTIM::RCC_INFO;
        if lptim.reset_asserted() {
            return Err(Error::LsiClockInUse);
        }
        let lptim_conflict = lptim
            .inspect_for_init(cs, config.timeout, || {
                (pac::LPTIM.cr().read().en()
                    && pac::LPTIM.cfgr().read().iclksrc() == pac::lptim::vals::Source::Lsi)
                    || lptim.reset_asserted()
            })
            .map_err(lse_target_inspection_error)?;
        if lptim_conflict || lptim.reset_asserted() {
            return Err(Error::LsiClockInUse);
        }
        let analog = crate::peripherals::VC1::RCC_INFO;
        if analog.reset_asserted() {
            return Err(Error::LsiClockInUse);
        }
        let analog_conflict = analog
            .inspect_for_init(cs, config.timeout, || {
                let lvd = pac::LVD.cr0().read();
                // Hardware auto-request is EN plus the LSI filter source,
                // including zero filter count; do not use needs_lsi here.
                (lvd.en() && !lvd.fltclk())
                    || (pac::VC1.cr0().read().en()
                        && pac::VC1.cr1().read().fltclk() == pac::vc::vals::FilterClock::InternalRc)
                    || (pac::VC2.cr0().read().en()
                        && pac::VC2.cr1().read().fltclk() == pac::vc::vals::FilterClock::InternalRc)
                    || analog.reset_asserted()
            })
            .map_err(lse_target_inspection_error)?;
        if analog_conflict || analog.reset_asserted() {
            return Err(Error::LsiClockInUse);
        }
        // No GTIM/ATIM/GPIO work gate is opened. Their direct LSI roots, IWDT
        // and downstream/external participants require the public handover.
        self.first_lsi_central(config)
    }

    fn final_sources(self, config: Config, trim: u16, needs_lsi: bool) -> Result<(), Error> {
        self.policy_unchanged()?;
        let r = pac::SYSCTRL;
        let mut expected = self.sources;
        expected.set_key(0);
        expected.set_hsien(true);
        expected.set_hseen(config.hse.is_some() || self.sources.hseen());
        expected.set_lseen(true);
        expected.set_lseccs(config.lse.ok_or(Error::LseNotConfigured)?.monitored());
        let mut sources = r.cr1().read();
        sources.set_key(0);
        let hsi = r.hsi().read();
        let hse = r.hse().read();
        if sources.0 != expected.0
            || !hsi.stable()
            || hsi.trim() != trim
            || hsi.div() != config.hsi.div as u8
            || ((needs_lsi || self.lsi.stable()) && !r.lsi().read().stable())
            || !crate::rcc::lse::preserved_register(self.lse.0)
            || hse.hexenpol() != self.hse.hexenpol()
        {
            return Err(Error::ClockConfigurationTimeout);
        }
        if let Some(requested) = config.hse {
            if !hse.stable() || !hse_parameters_match(requested)? {
                return Err(Error::ClockConfigurationTimeout);
            }
        } else if hse.0 != self.hse.0 {
            return Err(Error::ClockConfigurationTimeout);
        }
        if !crate::rcc::lse::healthy(config.lse.ok_or(Error::LseNotConfigured)?) {
            return Err(Error::LseNotReady);
        }
        Ok(())
    }
}

/// Own L010/L011 target tail: final divisors under calibrated HSI, then the last CR0
/// write selects LSE. All fault, Flash and pad windows after it are read-only
/// with respect to CR0, so fallback cannot be accidentally switched back.
#[cfg(rcc_lse)]
pub(super) fn finish_lse_sysclk(
    config: Config,
    mut clocks: Clocks,
    entry: LseSysclkSnapshot,
    trim: u16,
    needs_lsi: bool,
    cs: critical_section::CriticalSection<'_>,
) -> Result<Clocks, Error> {
    let r = pac::SYSCTRL;
    let lse = config.lse.ok_or(Error::LseNotConfigured)?;
    let monitor_hse = config.hse.is_some() || entry.sources.hseen() || entry.sources.hseccs();
    let verify = |source| {
        // Faults win over an apparently ready source or a changed fallback mux.
        check_external_faults(monitor_hse, true)?;
        entry.final_sources(config, trim, needs_lsi)?;
        let clock = r.cr0().read();
        if clock.sysclk() != source {
            return Err(if source == ClockSource::Lse {
                Error::LseClockSwitchTimeout
            } else {
                Error::ClockSwitchTimeout
            });
        }
        if clock.hclkprs() != config.ahb_pre as u8 || clock.pclkprs() != config.apb_pre as u8 {
            return Err(Error::ClockConfigurationTimeout);
        }
        Ok(())
    };
    check_external_faults(monitor_hse, true)?;
    entry.final_sources(config, trim, needs_lsi)?;
    if r.cr0().read().sysclk() != ClockSource::Hsi {
        return Err(Error::ClockSwitchTimeout);
    }
    r.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_hclkprs(config.ahb_pre as u8);
        w.set_pclkprs(config.apb_pre as u8);
    });
    // Dividers are a retained-configuration identity, never a source-ready
    // condition. Reject a mismatch instead of laundering it through a poll.
    verify(ClockSource::Hsi)?;
    barrier();
    r.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_sysclk(ClockSource::Lse);
    });
    for attempt in 0..config.timeout {
        check_external_faults(monitor_hse, true)?;
        entry.final_sources(config, trim, needs_lsi)?;
        let clock = r.cr0().read();
        if clock.hclkprs() != config.ahb_pre as u8 || clock.pclkprs() != config.apb_pre as u8 {
            return Err(Error::ClockConfigurationTimeout);
        }
        if clock.sysclk() == ClockSource::Lse {
            break;
        }
        if attempt + 1 == config.timeout {
            return Err(Error::LseClockSwitchTimeout);
        }
        core::hint::spin_loop();
    }
    barrier();
    verify(ClockSource::Lse)?;
    let upper_hclk = clocks
        .hclk_bounds()
        .maximum()
        .0
        .max(
            crate::rcc::ClockBounds::hsi(config.hsi.div.divisor())
                .divided_by(config.ahb_pre.divisor())
                .maximum()
                .0,
        )
        // Fixed effective HSI4MHz fallback: no bus-divider retention credit.
        .max(
            crate::rcc::ClockBounds::hsi(crate::RCC_FIXED_CCS_HSI_DIVISOR)
                .maximum()
                .0,
        );
    let final_wait = (upper_hclk - 1) / crate::RCC_FLASH_WAIT_STEP_HZ;
    pac::FLASH.cr2().modify(|w| {
        w.set_key(0x5a5a);
        w.set_wait(final_wait as u8);
    });
    for attempt in 0..config.timeout {
        // Continue checking fault/source/mux/policy first throughout lowering
        // WAIT. A route or monitor mismatch is an error, never readiness lag.
        verify(ClockSource::Lse)?;
        if u32::from(pac::FLASH.cr2().read().wait()) == final_wait {
            break;
        }
        if attempt + 1 == config.timeout {
            return Err(Error::FlashLatencyTimeout);
        }
        core::hint::spin_loop();
    }
    barrier();
    verify(ClockSource::Lse)?;
    if let Some(hse) = config.hse {
        if !crate::rcc_hse_pins_match(
            hse.mode == HseMode::Bypass,
            config.timeout,
            cs,
            #[cfg(all(rcc_cw32l010_v1, rcc_lsi_sysclk))]
            false,
        )? {
            return Err(Error::ClockConfigurationTimeout);
        }
    }
    // GPIOB (L010) or GPIOC (L011) is a working gate, also during exact reuse.
    // Native verify checks pads and the monitor marker; no clock is published.
    crate::rcc::lse::verify(lse, cs, false)?;
    verify(ClockSource::Lse)?;
    if u32::from(pac::FLASH.cr2().read().wait()) != final_wait {
        return Err(Error::FlashLatencyTimeout);
    }
    if entry.sources.hseen() {
        if !entry.hse.mode() {
            clocks.hse = Some(HseMode::Oscillator);
        } else if clocks.hse.is_none() {
            clocks.hse = Some(HseMode::Bypass);
        }
    }
    Ok(clocks)
}
