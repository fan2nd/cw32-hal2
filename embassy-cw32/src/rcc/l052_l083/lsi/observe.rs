use super::*;

#[cfg(rcc_lsi_sysclk)]
impl LsiSysclkState {
    pub(super) fn collect(
        &mut self,
        phase: LsiTransition,
        cs: critical_section::CriticalSection<'_>,
    ) -> Result<(RccLsiConsumers, RccLsiPads), Error> {
        self.check_identity(phase)?;
        let consumers = rcc_lsi_consumers(
            self.timeout,
            cs,
            self.class == LsiEntryClass::Cold,
            #[cfg(rcc_cw32l083_v1)]
            self.entry.sources.pllen(),
            self.monitor_hse,
            self.monitor_lse,
        )?;
        let pads = rcc_lsi_pads(self.timeout, cs, self.monitor_hse, self.monitor_lse)?;
        let lse_snapshot = if self.lse_preflight.is_some() {
            let config = self.lse_config.ok_or(Error::LseClockInUse)?;
            Some(rcc_lsi_lse_preflight(
                config.mode == crate::rcc::LseMode::Bypass,
                !self.lse_pads_configured,
                #[cfg(rcc_cw32l083_v1)]
                self.entry.sources.pllen(),
                config.poll_budget,
                cs,
                self.monitor_hse,
                self.monitor_lse,
            )?)
        } else {
            None
        };
        self.check_identity(phase)?;
        // Each collector already resolved restore/enable/readback before faults
        // and its buffered semantics. Compare only after their full restoration.
        #[cfg(rcc_cw32l083_v1)]
        if let LsiTransition::HsePad(bypass, step) = phase {
            let target_consumers = self
                .consumers
                .map(|saved| saved.hse_configured_step(bypass, step));
            let target_pads = self
                .pads
                .map(|saved| saved.hse_configured_step(bypass, step));
            let target_lse = self
                .lse_preflight
                .map(|saved| saved.hse_configured_step(bypass, step));
            if (Some(consumers) != self.consumers && Some(consumers) != target_consumers)
                || (Some(pads) != self.pads && Some(pads) != target_pads)
                || (lse_snapshot != self.lse_preflight && lse_snapshot != target_lse)
            {
                return Err(Error::LsiClockInUse);
            }
            // These are computed one-register deltas, never arbitrary samples.
            if Some(consumers) == target_consumers
                && Some(pads) == target_pads
                && lse_snapshot == target_lse
            {
                self.consumers = target_consumers;
                self.pads = target_pads;
                self.lse_preflight = target_lse;
            }
            return Ok((consumers, pads));
        }
        if self.consumers.is_some_and(|saved| saved != consumers)
            || self.pads.is_some_and(|saved| saved != pads)
            || lse_snapshot != self.lse_preflight
        {
            return Err(Error::LsiClockInUse);
        }
        Ok((consumers, pads))
    }

    pub(super) fn check_all(
        &mut self,
        phase: LsiTransition,
        cs: critical_section::CriticalSection<'_>,
    ) -> Result<(), Error> {
        self.collect(phase, cs).map(|_| ())
    }

    pub(super) fn wait(
        &mut self,
        phase: LsiTransition,
        attempts: u32,
        error: Error,
        cs: critical_section::CriticalSection<'_>,
    ) -> Result<(), Error> {
        for _ in 0..attempts {
            self.check_all(phase, cs)?;
            let acknowledged = match phase {
                LsiTransition::Steady => true,
                #[cfg(rcc_cw32l083_v1)]
                LsiTransition::PllStopping(target) => {
                    self.sources.0 == target.0 && self.pll_stopped
                }
                #[cfg(rcc_cw32l083_v1)]
                LsiTransition::FlashWait(system, flash) => {
                    self.cr2.0 == system.0
                        && self.flash_cr2.is_some_and(|actual| actual.0 == flash.0)
                }
                #[cfg(rcc_cw32l083_v1)]
                LsiTransition::HsePad(bypass, step) => {
                    self.consumers
                        == self
                            .consumers
                            .map(|saved| saved.hse_configured_step(bypass, step))
                        && self.pads
                            == self
                                .pads
                                .map(|saved| saved.hse_configured_step(bypass, step))
                        && self.lse_preflight
                            == self
                                .lse_preflight
                                .map(|saved| saved.hse_configured_step(bypass, step))
                }
                LsiTransition::Clock(target) => self.clock.0 == target.0,
                LsiTransition::LsiTrim(target) => {
                    self.lsi.0 & crate::RCC_LSI_PARAMETERS_MASK
                        == target.0 & crate::RCC_LSI_PARAMETERS_MASK
                }
                LsiTransition::LsiRequest(target)
                | LsiTransition::HsiStopRequest(target)
                | LsiTransition::HsiRequest(target)
                | LsiTransition::HseRequest(target)
                | LsiTransition::LseRequest(target) => self.sources.0 == target.0,
                LsiTransition::LsiReady => self.ready[3],
                LsiTransition::HsiStopped => !self.ready[0],
                LsiTransition::HsiReady => self.ready[0],
                LsiTransition::HseReady => self.ready[1],
                LsiTransition::LseReady => self.ready[2],
                LsiTransition::HsiTrim(target) | LsiTransition::HsiDivider(target) => {
                    self.hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK
                        == target.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK
                }
                LsiTransition::HseParameters(target) => {
                    self.hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK
                        == target.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK
                }
                LsiTransition::LseParameters(target) => {
                    self.lse.0 & crate::RCC_LSI_LSE_PARAMETERS_MASK
                        == target.0 & crate::RCC_LSI_LSE_PARAMETERS_MASK
                }
            };
            if acknowledged {
                return Ok(());
            }
            core::hint::spin_loop();
        }
        Err(error)
    }

    pub(super) fn hsi_owners(
        &mut self,
        cs: critical_section::CriticalSection<'_>,
    ) -> Result<(), Error> {
        self.check_all(LsiTransition::Steady, cs)?;
        let consumers = self.consumers.ok_or(Error::LsiClockInUse)?;
        let autotrim = pac::autotrim::regs::Cr(consumers.autotrim_cr);
        let lvd0 = pac::lvd::regs::Cr0(consumers.lvd_cr0);
        let lvd1 = pac::lvd::regs::Cr1(consumers.lvd_cr1);
        if (autotrim.en() && autotrim.src() == pac::autotrim::vals::Source::HsiOsc)
            || (lvd0.en() && lvd1.flten() && lvd1.fltclk())
        {
            return Err(Error::HsiClockInUse);
        }
        Ok(())
    }

    pub(super) fn external_owners(&self, config: Config) -> Result<(), Error> {
        let consumers = self.consumers.ok_or(Error::LsiClockInUse)?;
        let pads = self.pads.ok_or(Error::LsiClockInUse)?;
        let autotrim = pac::autotrim::regs::Cr(consumers.autotrim_cr);
        let rtc_source = pac::rtc::regs::Cr1(consumers.rtc_cr1).source();
        let rtc_hse = matches!(
            rtc_source,
            pac::rtc::vals::Source::HseDiv128
                | pac::rtc::vals::Source::HseDiv256
                | pac::rtc::vals::Source::HseDiv512
                | pac::rtc::vals::Source::HseDiv1024
        );
        let autotrim_hse = autotrim.en() && autotrim.src() == pac::autotrim::vals::Source::Hse;
        if (rtc_hse || autotrim_hse) && !self.entry.sources.hseen() {
            return Err(Error::HseClockInUse);
        }
        if self.entry.sources.hseen() && !pads.hse_matches(self.entry.hse.mode()) {
            return Err(Error::HseClockInUse);
        }
        if self.entry.sources.lseen() && !pads.lse_matches(self.entry.lse.mode(), false) {
            return Err(Error::LseClockInUse);
        }
        if let Some(hse) = config.hse {
            if self.entry.sources.hseen() {
                let requested = self.hse_parameters(hse)?;
                if requested.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK
                    != self.hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK
                    || !pads.hse_matches(hse.mode == HseMode::Bypass)
                {
                    return Err(Error::HseClockInUse);
                }
            } else {
                if autotrim.en() && autotrim.src() == pac::autotrim::vals::Source::Etr {
                    return Err(Error::HsePinInUse);
                }
                #[cfg(rcc_cw32l083_v1)]
                if !pads.hse_startup_allowed(hse.mode == HseMode::Bypass) {
                    return Err(Error::HsePinInUse);
                }
            }
        }
        Ok(())
    }

    pub(super) fn hse_parameters(&self, config: Hse) -> Result<pac::sysctrl::regs::Hse, Error> {
        let mut target = self.hse;
        target.set_mode(config.mode == HseMode::Bypass);
        target.set_driver(config.drive);
        #[cfg(rcc_cw32l052_v1)]
        target.set_pdriver(config.drive);
        target.set_freqrange(config.range());
        #[cfg(rcc_cw32l052_v1)]
        target.set_pfreqrange(config.range());
        target.set_waitcycle(pac::sysctrl::vals::HseWait::Cycles262144);
        target.set_flt(false);
        target.set_detcnt(config.detector_count()?);
        Ok(target)
    }

    pub(super) fn lse_parameters(&self, config: crate::rcc::Lse) -> pac::sysctrl::regs::Lse {
        let mut target = self.lse;
        target.set_mode(config.mode == crate::rcc::LseMode::Bypass);
        target.set_driver(config.drive);
        target.set_amp(config.amplitude);
        #[cfg(rcc_cw32l052_v1)]
        {
            target.set_pdriver(config.startup_drive);
            target.set_pamp(config.startup_amplitude);
        }
        target.set_waitcycle(config.wait);
        target
    }

    pub(super) fn cold_lse_preflight(
        &mut self,
        config: crate::rcc::Lse,
        unused: bool,
        cs: critical_section::CriticalSection<'_>,
    ) -> Result<(), Error> {
        self.check_all(LsiTransition::Steady, cs)?;
        if self.entry.sources.lseen()
            || self.sources.lseen()
            || self.entry.sources.lselock()
            || self.ready[2]
            || self.entry.flags.lserdy()
            || self.entry.ier.lserdy()
            || self.entry.ier.lsefail()
            || self.entry.ier.lsefault()
            || self.clock.sysclk() == ClockSource::Lse
        {
            return Err(Error::LseClockInUse);
        }
        #[cfg(rcc_cw32l083_v1)]
        {
            let observed = self.check_identity(LsiTransition::Steady)?;
            if observed.pending || observed.fault_pending {
                return Err(Error::LseClockInUse);
            }
        }
        let snapshot = rcc_lsi_lse_preflight(
            config.mode == crate::rcc::LseMode::Bypass,
            unused,
            #[cfg(rcc_cw32l083_v1)]
            self.entry.sources.pllen(),
            config.poll_budget,
            cs,
            self.monitor_hse,
            self.monitor_lse,
        )?;
        self.check_all(LsiTransition::Steady, cs)?;
        if self
            .lse_preflight
            .is_some_and(|expected| expected != snapshot)
        {
            return Err(Error::LseClockInUse);
        }
        if self.lse_preflight.is_none() {
            self.lse_preflight = Some(snapshot);
        }
        Ok(())
    }

    pub(super) fn verify_sources(
        &mut self,
        config: Config,
        cs: critical_section::CriticalSection<'_>,
    ) -> Result<(), Error> {
        self.check_all(LsiTransition::Steady, cs)?;
        let pads = self.pads.ok_or(Error::LsiClockInUse)?;
        #[cfg(rcc_cw32l083_v1)]
        if !self.pll_stopped || self.sources.pllen() || self.pll_ready {
            return Err(Error::PllStopTimeout);
        }
        if !self.sources.lsien() || !self.ready[3] || self.lsi.trim() != self.factory_lsi {
            return Err(Error::LsiTimeout);
        }
        if !self.sources.hsien()
            || !self.ready[0]
            || self.hsi.trim() != self.factory_hsi
            || self.hsi.div() != config.hsi.div as u8
        {
            return Err(Error::ClockConfigurationTimeout);
        }
        if let Some(hse) = config.hse {
            if !self.sources.hseen()
                || !self.ready[1]
                || self.hse_parameters(hse)?.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK
                    != self.hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK
                || !pads.hse_matches(hse.mode == HseMode::Bypass)
            {
                return Err(Error::HseClockInUse);
            }
        } else if self.entry.sources.hseen() && !pads.hse_matches(self.entry.hse.mode()) {
            return Err(Error::HseClockInUse);
        }
        if let Some(lse) = config.lse {
            if !self.sources.lseen()
                || !self.sources.lseccs()
                || !self.ready[2]
                || self.lse_parameters(lse).0 & crate::RCC_LSI_LSE_PARAMETERS_MASK
                    != self.lse.0 & crate::RCC_LSI_LSE_PARAMETERS_MASK
                || !pads.lse_matches(lse.mode == crate::rcc::LseMode::Bypass, false)
                || !crate::rcc::lse::healthy(lse)
            {
                check_external_faults(self.monitor_hse, self.monitor_lse)?;
                return Err(Error::LseNotReady);
            }
        } else if self.entry.sources.lseen() && !pads.lse_matches(self.entry.lse.mode(), false) {
            return Err(Error::LseClockInUse);
        }
        self.check_all(LsiTransition::Steady, cs)
    }

    #[cfg(rcc_cw32l052_v1)]
    pub(super) fn flash_latency(
        &mut self,
        wait: u32,
        cs: critical_section::CriticalSection<'_>,
    ) -> Result<(), Error> {
        self.check_all(LsiTransition::Steady, cs)?;
        pac::FLASH.cr2().modify(|w| {
            w.set_key(0x5a5a);
            w.set_wait(wait as u8);
        });
        for _ in 0..self.timeout {
            self.check_all(LsiTransition::Steady, cs)?;
            if pac::FLASH.cr2().read().wait() == wait as u8 {
                barrier();
                return self.check_all(LsiTransition::Steady, cs);
            }
            core::hint::spin_loop();
        }
        Err(Error::FlashLatencyTimeout)
    }
    #[cfg(rcc_cw32l083_v1)]
    pub(super) fn flash_latency(
        &mut self,
        wait: u32,
        cs: critical_section::CriticalSection<'_>,
    ) -> Result<(), Error> {
        self.check_all(LsiTransition::Steady, cs)?;
        let mut system = self.cr2;
        system.set_flashwait(wait as u8);
        let mut target = self.flash_cr2.ok_or(Error::FlashLatencyTimeout)?;
        target.set_wait(wait as u8);
        pac::FLASH.cr2().modify(|w| {
            w.set_key(0x5a5a);
            w.set_wait(wait as u8);
        });
        self.wait(
            LsiTransition::FlashWait(system, target),
            self.timeout,
            Error::FlashLatencyTimeout,
            cs,
        )?;
        barrier();
        self.check_all(LsiTransition::Steady, cs)
    }
}
