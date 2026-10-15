use super::inspection::lse_target_inspect;
use super::io::{barrier, check_external_faults, hse_parameters_match};
use super::{
    pac, AHBPrescaler, APBPrescaler, ClockSource, Clocks, Config, Error, HseMode, L012HseTarget,
    MAX_FLASH_WAIT,
};

/// Original target identity, never recaptured after a source or WAIT change.
/// The native opaque Admission retains its separate classification/ownership.
#[cfg(rcc_lse)]
#[derive(Clone, Copy)]
pub(super) struct LseSysclkSnapshot {
    pub(super) clock: pac::sysctrl::regs::Cr0,
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
    fn monitors_hse(self, config: Config) -> bool {
        config.hse.is_some() || self.sources.hseen() || self.sources.hseccs()
    }

    pub(super) fn faults(self, config: Config) -> Result<(), Error> {
        check_external_faults(self.monitors_hse(config), true)
    }

    pub(super) fn policy(self, config: Config) -> Result<(), Error> {
        self.faults(config)?;
        let r = pac::SYSCTRL;
        let mut expected = self.routes;
        let mut current = r.cr2().read();
        expected.set_key(0);
        current.set_key(0);
        expected.set_flashwait(current.flashwait());
        // The WAIT pair is checked separately against exact owned phase values.
        // No other original policy bit is replaceable by a live observation.
        if current.0 != expected.0
            || r.ier().read().0 != self.interrupts.0
            || r.mco().read().0 != self.mco.0
        {
            return Err(Error::LseClockInUse);
        }
        let current = r.lsi().read();
        if current.trim() != self.lsi.trim() || current.waitcycle() != self.lsi.waitcycle() {
            return Err(if config.lse.ok_or(Error::LseNotConfigured)?.monitored() {
                Error::LseMonitorNotReady
            } else {
                Error::LseClockInUse
            });
        }
        if config.lse.ok_or(Error::LseNotConfigured)?.monitored() {
            // Native L012 qualification is the own nine-bit factory tuple.
            // Later readiness cannot manufacture entry monitor admission.
            let factory = unsafe {
                core::ptr::read_volatile(crate::RCC_LSE_LSI_FACTORY_TRIM_ADDRESS as *const u16)
            };
            let mut calibrated = pac::sysctrl::regs::Lsi::default();
            calibrated.set_trim(factory);
            if !self.lsi.stable()
                || !current.stable()
                || factory == u16::MAX
                || self.lsi.trim() != calibrated.trim()
            {
                return Err(Error::LseMonitorNotReady);
            }
        }
        Ok(())
    }

    pub(super) fn clock_matches(self, source: ClockSource, ahb: u8, apb: u8) -> Result<(), Error> {
        let mut expected = self.clock;
        expected.set_key(0);
        expected.set_sysclk(source);
        expected.set_hclkprs(ahb);
        expected.set_pclkprs(apb);
        let mut current = pac::SYSCTRL.cr0().read();
        current.set_key(0);
        if current.0 != expected.0 {
            return Err(Error::ClockConfigurationTimeout);
        }
        Ok(())
    }

    // Before the trim bridge, only the owned unchanged-HSI request may differ
    // in CR1; every other bit, including CLKCCS and software LSIEN, is original.
    pub(super) fn unchanged_sources(
        self,
        config: Config,
        hsi_requested: bool,
        hsi_ready: bool,
    ) -> Result<(), Error> {
        self.policy(config)?;
        let r = pac::SYSCTRL;
        let mut expected = self.sources;
        expected.set_key(0);
        if hsi_requested {
            expected.set_hsien(true);
        }
        let mut current = r.cr1().read();
        current.set_key(0);
        if current.0 != expected.0 || r.lse().read().0 != self.lse.0 {
            return Err(Error::LseClockInUse);
        }
        let hsi = r.hsi().read();
        if (hsi.0 ^ self.hsi.0) & !crate::RCC_HSI_STABLE_MASK != 0
            || ((hsi_ready || self.hsi.stable()) && !hsi.stable())
            || r.hse().read().0 != self.hse.0
            || (self.lsi.stable() && !r.lsi().read().stable())
        {
            return Err(Error::ClockConfigurationTimeout);
        }
        Ok(())
    }

    pub(super) fn hsi_ownership(
        self,
        config: Config,
        trim: u16,
        cs: critical_section::CriticalSection<'_>,
    ) -> Result<(), Error> {
        self.policy(config)?;
        if self.hsi.trim() != trim || !self.sources.hsien() || !self.hsi.stable() {
            if pac::SYSCTRL.mco().read().source() == 3 || pac::SYSCTRL.ier().read().hsirdy() {
                return Err(Error::HsiClockInUse);
            }
            self.i2c_internal_ownership(config, cs, Error::HsiClockInUse)?;
        }
        Ok(())
    }

    fn i2c_internal_ownership(
        self,
        config: Config,
        cs: critical_section::CriticalSection<'_>,
        error: Error,
    ) -> Result<(), Error> {
        use crate::rcc::SealedRccPeripheral;
        for (info, i2c) in [
            (crate::peripherals::I2C1::RCC_INFO, pac::I2C1),
            (crate::peripherals::I2C2::RCC_INFO, pac::I2C2),
        ] {
            let conflict = lse_target_inspect(config, cs, info, self.monitors_hse(config), || {
                // Raw1 is reserved; raw3 conflicts between own master prose
                // and SDK/slave sources. Neither can prove raw-HSI/LSI absence.
                matches!(u8::from(i2c.mcr0().read().clksrc()), 1 | 3)
                    || matches!(u8::from(i2c.scr0().read().clksrc()), 1 | 3)
            })?;
            if conflict {
                return Err(error);
            }
        }
        Ok(())
    }

    pub(super) fn admit_first_lsi_request(
        self,
        config: Config,
        cs: critical_section::CriticalSection<'_>,
    ) -> Result<(), Error> {
        use crate::rcc::SealedRccPeripheral;
        self.policy(config)?;
        let r = pac::SYSCTRL;
        if r.mco().read().source() == 4 || r.ier().read().lsirdy() {
            return Err(Error::LsiClockInUse);
        }
        let conflict = lse_target_inspect(
            config,
            cs,
            crate::peripherals::RTC::RCC_INFO,
            self.monitors_hse(config),
            || matches!(pac::RTC.cr1().read().source(), 2 | 4..=u8::MAX),
        )?;
        if conflict {
            return Err(Error::LsiClockInUse);
        }
        for (info, uart) in [
            (crate::peripherals::UART1::RCC_INFO, pac::UART1),
            (crate::peripherals::UART2::RCC_INFO, pac::UART2),
        ] {
            if lse_target_inspect(config, cs, info, self.monitors_hse(config), || {
                uart.cr1().read().source() == pac::uart::vals::Source::Lsi
            })? {
                return Err(Error::LsiClockInUse);
            }
        }
        // UART3's work-gate semantics disagree between own sources. Read only
        // an operational instance and never open its closed gate for admission.
        let uart3 = crate::peripherals::UART3::RCC_INFO;
        self.faults(config)?;
        if uart3.reset_asserted() {
            return Err(Error::LseClockInUse);
        }
        if uart3.is_enabled() {
            if uart3.reset_asserted() || !uart3.is_enabled() {
                return Err(Error::LseClockInUse);
            }
            let conflict = pac::UART3.cr1().read().source() == pac::uart::vals::Source::Lsi;
            self.faults(config)?;
            if uart3.reset_asserted() || !uart3.is_enabled() {
                return Err(Error::LseClockInUse);
            }
            if conflict {
                return Err(Error::LsiClockInUse);
            }
        }
        if uart3.reset_asserted() {
            return Err(Error::LseClockInUse);
        }
        self.i2c_internal_ownership(config, cs, Error::LsiClockInUse)?;
        if lse_target_inspect(
            config,
            cs,
            crate::peripherals::LPTIM::RCC_INFO,
            self.monitors_hse(config),
            || {
                pac::LPTIM.cr0().read().en()
                    && pac::LPTIM.cfgr().read().iclksrc() == pac::lptim::vals::Source::Lsi
            },
        )? {
            return Err(Error::LsiClockInUse);
        }
        // Ungated LVD is independent of the VC configuration domain. Native
        // auto-request depends on EN/source, including FLTTIME=0.
        let lvd = pac::LVD.cr0().read();
        self.faults(config)?;
        if lvd.en() && !lvd.fltclk() {
            return Err(Error::LsiClockInUse);
        }
        if lse_target_inspect(
            config,
            cs,
            crate::peripherals::VC1::RCC_INFO,
            self.monitors_hse(config),
            || {
                [pac::VC1, pac::VC2, pac::VC3, pac::VC4].iter().any(|vc| {
                    vc.cr0().read().en()
                        && vc.cr1().read().fltclk() == pac::vc::vals::FilterClock::InternalRc
                })
            },
        )? {
            return Err(Error::LsiClockInUse);
        }
        // No timer/GPIO/output work gate is opened. The documented functional
        // handover covers inaccessible UART3, direct/cascade observers and IWDT.
        self.policy(config)
    }

    pub(super) fn prepared_sources(
        self,
        config: Config,
        trim: u16,
        needs_lsi: bool,
        after_lse: bool,
    ) -> Result<(), Error> {
        self.policy(config)?;
        let r = pac::SYSCTRL;
        let requested_lse = config.lse.ok_or(Error::LseNotConfigured)?;
        let mut expected = self.sources;
        expected.set_key(0);
        expected.set_hsien(true);
        expected.set_hseen(config.hse.is_some() || self.sources.hseen());
        if after_lse {
            expected.set_lseen(true);
            expected.set_lseccs(requested_lse.monitored());
        }
        let mut current = r.cr1().read();
        current.set_key(0);
        if current.0 != expected.0 {
            return Err(Error::LseClockInUse);
        }
        let mut expected_hsi = self.hsi;
        expected_hsi.set_trim(trim);
        expected_hsi.set_div(config.hsi.div as u8);
        let hsi = r.hsi().read();
        if (hsi.0 ^ expected_hsi.0) & !crate::RCC_HSI_STABLE_MASK != 0
            || !hsi.stable()
            || ((needs_lsi || self.lsi.stable()) && !r.lsi().read().stable())
        {
            return Err(Error::ClockConfigurationTimeout);
        }
        let mut expected_hse = self.hse;
        if let Some(hse) = config.hse {
            if !self.sources.hseen() {
                expected_hse.set_mode(hse.mode == HseMode::Bypass);
                expected_hse.set_driver(hse.drive);
                expected_hse.set_pdriver(hse.drive);
                expected_hse.set_waitcycle(pac::sysctrl::vals::HseWait::Cycles262144);
                expected_hse.set_digflt(false);
                expected_hse.set_detcnt(hse.detector_count()?);
            }
            if !hse_parameters_match(hse)? || !r.hse().read().stable() {
                return Err(Error::ClockConfigurationTimeout);
            }
        }
        let hse_difference = r.hse().read().0 ^ expected_hse.0;
        if (config.hse.is_some() && hse_difference & !crate::RCC_HSE_STABLE_MASK != 0)
            || (config.hse.is_none() && hse_difference != 0)
        {
            return Err(Error::ClockConfigurationTimeout);
        }
        if after_lse {
            let lse = r.lse().read();
            // The generated native change mask includes read-only STABLE.
            // Combine preserved original bits with every requested field and
            // explicit ready/frozen-monitor checks; the mask alone is not proof.
            if !crate::rcc::lse::preserved_register(self.lse.0)
                || lse.mode() != (requested_lse.mode == crate::rcc::LseMode::Bypass)
                || lse.driver() != requested_lse.drive
                || lse.pdriver() != requested_lse.startup_drive
                || lse.waitcycle() != requested_lse.wait
                || !lse.stable()
                || !crate::rcc::lse::healthy(requested_lse)
            {
                return Err(Error::LseNotReady);
            }
        } else if r.lse().read().0 != self.lse.0 {
            return Err(Error::LseClockInUse);
        }
        Ok(())
    }

    pub(super) fn flash_access(self, config: Config) -> Result<(), Error> {
        self.faults(config)?;
        let info = <crate::peripherals::FLASH as crate::rcc::SealedRccPeripheral>::RCC_INFO;
        if info.reset_asserted() {
            return Err(Error::LseClockInUse);
        }
        if !info.is_enabled() {
            return Err(Error::FlashClockTimeout);
        }
        Ok(())
    }

    fn wait_pair(self, config: Config, flash: pac::flash::regs::Cr2) -> Result<(u8, u8), Error> {
        let mut current_routes = pac::SYSCTRL.cr2().read();
        let mut expected_routes = self.routes;
        expected_routes.set_flashwait(current_routes.flashwait());
        current_routes.set_key(0);
        expected_routes.set_key(0);
        if current_routes.0 != expected_routes.0 {
            return Err(Error::LseClockInUse);
        }
        self.flash_access(config)?;
        let mut current_flash = pac::FLASH.cr2().read();
        self.flash_access(config)?;
        let mut expected_flash = flash;
        expected_flash.set_wait(current_flash.wait());
        current_flash.set_key(0);
        expected_flash.set_key(0);
        if current_flash.0 != expected_flash.0 {
            return Err(Error::FlashLatencyTimeout);
        }
        Ok((current_flash.wait(), current_routes.flashwait()))
    }

    pub(super) fn require_wait(
        self,
        config: Config,
        flash: pac::flash::regs::Cr2,
        wait: u8,
    ) -> Result<(), Error> {
        if self.wait_pair(config, flash)? != (wait, wait) {
            return Err(Error::FlashLatencyTimeout);
        }
        Ok(())
    }

    pub(super) fn write_wait(
        self,
        config: Config,
        flash: pac::flash::regs::Cr2,
        old: u8,
        new: u8,
        verify: impl Fn() -> Result<(), Error>,
    ) -> Result<(), Error> {
        verify()?;
        self.require_wait(config, flash, old)?;
        self.flash_access(config)?;
        pac::FLASH.cr2().modify(|w| {
            w.set_key(0x5a5a);
            w.set_wait(new);
        });
        self.flash_access(config)?;
        for _ in 0..config.timeout {
            // Fault/source/policy/divider errors precede either success or the
            // sole permitted wait branch. Mixed alias observations fail closed;
            // no propagation bound or atomic observation is assumed.
            verify()?;
            let pair = self.wait_pair(config, flash)?;
            if pair == (new, new) {
                barrier();
                verify()?;
                return self.require_wait(config, flash, new);
            }
            if old == new || pair != (old, old) {
                return Err(Error::FlashLatencyTimeout);
            }
            core::hint::spin_loop();
        }
        Err(Error::FlashLatencyTimeout)
    }
}

/// L012 target tail: final WAIT is established under verified factory HSI.
/// The final LSE selection is the last CR0 write and the last Flash write is
/// already behind it. All failure and pad/gate cleanup paths preserve that edge.
#[cfg(rcc_lse)]
pub(super) fn finish_lse_sysclk(
    config: Config,
    mut clocks: Clocks,
    entry: LseSysclkSnapshot,
    flash: pac::flash::regs::Cr2,
    trim: u16,
    needs_lsi: bool,
    cs: critical_section::CriticalSection<'_>,
) -> Result<Clocks, Error> {
    let r = pac::SYSCTRL;
    let lse = config.lse.ok_or(Error::LseNotConfigured)?;
    let sources = || entry.prepared_sources(config, trim, needs_lsi, true);
    let verify_hsi = || {
        sources()?;
        entry.clock_matches(ClockSource::Hsi, config.ahb_pre as u8, config.apb_pre as u8)
    };
    sources()?;
    entry.clock_matches(
        ClockSource::Hsi,
        entry.clock.hclkprs().max(AHBPrescaler::Div8 as u8),
        entry.clock.pclkprs().max(APBPrescaler::Div8 as u8),
    )?;
    entry.require_wait(config, flash, MAX_FLASH_WAIT as u8)?;
    if let Some(hse) = config.hse {
        if !crate::rcc_hse_pins_match(hse.mode == HseMode::Bypass, config.timeout, cs, L012HseTarget::LseSysclk)? {
            return Err(Error::ClockConfigurationTimeout);
        }
    }
    // HSE pad verification may open/restore a gate. Recheck before CR0 changes.
    sources()?;
    entry.clock_matches(
        ClockSource::Hsi,
        entry.clock.hclkprs().max(AHBPrescaler::Div8 as u8),
        entry.clock.pclkprs().max(APBPrescaler::Div8 as u8),
    )?;
    entry.require_wait(config, flash, MAX_FLASH_WAIT as u8)?;
    r.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_sysclk(ClockSource::Hsi);
        w.set_hclkprs(config.ahb_pre as u8);
        w.set_pclkprs(config.apb_pre as u8);
    });
    // Configuration identity changes are errors, never source-readiness lag.
    verify_hsi()?;
    entry.require_wait(config, flash, MAX_FLASH_WAIT as u8)?;
    barrier();
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
        // Full effective 4.08 MHz fallback, without bus-divider credit.
        .max(
            crate::rcc::ClockBounds::hsi(crate::RCC_FIXED_CCS_HSI_DIVISOR)
                .maximum()
                .0,
        );
    let final_wait = ((upper_hclk - 1) / crate::RCC_FLASH_WAIT_STEP_HZ) as u8;
    entry.write_wait(config, flash, MAX_FLASH_WAIT as u8, final_wait, verify_hsi)?;
    verify_hsi()?;
    entry.require_wait(config, flash, final_wait)?;
    // LAST CR0 write. No rollback, Flash retry, divider repair or LSE reselect
    // is permitted below, including timeout, fault and pad/gate failure paths.
    r.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_sysclk(ClockSource::Lse);
    });
    for attempt in 0..config.timeout {
        sources()?;
        entry.require_wait(config, flash, final_wait)?;
        let current = r.cr0().read();
        // Only the old HSI selector may await the requested LSE selector.
        // Dividers and all untouched CR0 bits must match on every attempt.
        entry.clock_matches(current.sysclk(), config.ahb_pre as u8, config.apb_pre as u8)?;
        if current.sysclk() == ClockSource::Lse {
            break;
        }
        if current.sysclk() != ClockSource::Hsi || attempt + 1 == config.timeout {
            return Err(Error::LseClockSwitchTimeout);
        }
        core::hint::spin_loop();
    }
    barrier();
    let verify_lse = || {
        sources()?;
        let clock = r.cr0().read();
        if clock.sysclk() != ClockSource::Lse {
            return Err(Error::LseClockSwitchTimeout);
        }
        entry.clock_matches(ClockSource::Lse, config.ahb_pre as u8, config.apb_pre as u8)?;
        entry.require_wait(config, flash, final_wait)
    };
    verify_lse()?;
    if let Some(hse) = config.hse {
        if !crate::rcc_hse_pins_match(hse.mode == HseMode::Bypass, config.timeout, cs, L012HseTarget::LseSysclk)? {
            return Err(Error::ClockConfigurationTimeout);
        }
    }
    // Native GPIOC/pad/monitor verification is ownership-aware and never
    // writes CR0 or FLASH. Its independent gate restoration remains required.
    let result = crate::rcc::lse::verify(lse, cs, true);
    if let Err(error @ (Error::LseConfigurationGateTimeout { .. } | Error::LseGpioGateTimeout)) =
        result
    {
        return Err(error);
    }
    entry.policy(config)?;
    result?;
    verify_lse()?;
    if entry.sources.hseen() {
        if !entry.hse.mode() {
            clocks.hse = Some(HseMode::Oscillator);
        } else if clocks.hse.is_none() {
            clocks.hse = Some(HseMode::Bypass);
        }
    }
    Ok(clocks)
}

