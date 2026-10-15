use super::*;

/// Immutable original-entry identity for exact native live factory-LSI reuse.
#[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
#[derive(Clone, Copy)]
pub(super) struct LsiSysclkSnapshot {
    pub(super) clock: pac::sysctrl::regs::Cr0,
    pub(super) sources: pac::sysctrl::regs::Cr1,
    pub(super) hsi: pac::sysctrl::regs::Hsi,
    pub(super) hse: pac::sysctrl::regs::Hse,
    pub(super) lsi: pac::sysctrl::regs::Lsi,
    pub(super) lse: pac::sysctrl::regs::Lse,
    pub(super) routes: pac::sysctrl::regs::Cr2,
    pub(super) interrupts: pac::sysctrl::regs::Ier,
    pub(super) mco: pac::sysctrl::regs::Mco,
    pub(super) factory: u16,
    // Attached once after the first legal Flash observation, never recaptured.
    #[cfg(rcc_cw32l011_v1)]
    pub(super) flash: Option<pac::flash::regs::Cr2>,
}

#[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
impl LsiSysclkSnapshot {
    pub(super) fn admit(self) -> Result<(), Error> {
        #[cfg(rcc_cw32l011_v1)]
        {
            if self.sources.hseen() {
                return Err(Error::HseClockInUse);
            }
            if self.sources.hseccs() || self.sources.lseccs() {
                return Err(Error::LsiClockInUse);
            }
            match self.clock.sysclk() {
                ClockSource::Hsi | ClockSource::Lsi => {}
                ClockSource::Hse | ClockSource::Lse => return Err(Error::InvalidEntryClock),
                _ => return Err(Error::InvalidClockSource),
            }
        }
        let mut factory = pac::sysctrl::regs::Lsi::default();
        factory.set_trim(self.factory);
        if self.factory == u16::MAX || self.lsi.trim() != factory.trim() {
            return Err(Error::InvalidLsiCalibration);
        }
        if !self.sources.lsien() || !self.lsi.stable() {
            return Err(Error::LsiNotReady);
        }
        Ok(())
    }

    #[cfg(rcc_cw32l011_v1)]
    pub(super) fn post_observation(self) -> Result<(), Error> {
        // HSE is excluded; stale unrelated HSE flags do not veto this slice.
        check_external_faults(false, self.sources.lseen() || self.sources.lseccs())
    }

    #[cfg(rcc_cw32l011_v1)]
    fn flash_access() -> Result<(), Error> {
        let info = <crate::peripherals::FLASH as crate::rcc::SealedRccPeripheral>::RCC_INFO;
        if !info.is_enabled() || info.reset_asserted() {
            return Err(Error::FlashClockTimeout);
        }
        Ok(())
    }

    #[cfg(rcc_cw32l011_v1)]
    pub(super) fn read_flash() -> Result<pac::flash::regs::Cr2, Error> {
        Self::flash_access()?;
        let flash = pac::FLASH.cr2().read();
        Self::flash_access()?;
        Ok(flash)
    }

    #[cfg(rcc_cw32l011_v1)]
    pub(super) fn write_flash_wait(wait: u8) -> Result<(), Error> {
        // The caller has just verified the complete settled phase. Check the
        // actual configuration gate/reset again before this typed RMW's read.
        Self::flash_access()?;
        pac::FLASH.cr2().modify(|w| {
            w.set_key(0x5a5a);
            w.set_wait(wait);
        });
        Self::flash_access()
    }

    #[cfg(rcc_cw32l011_v1)]
    pub(super) fn flash_wait(self, expected: Option<(u8, u8)>) -> Result<Option<(u8, u8)>, Error> {
        if expected.is_none() {
            return Ok(None);
        }
        // Return a buffered result: source/policy/readiness errors have priority.
        let flash = Self::read_flash();
        let mirror = pac::SYSCTRL.cr2().read().flashwait();
        let mut flash = flash?;
        let wait = flash.wait();
        let mut original = self.flash.ok_or(Error::FlashLatencyTimeout)?;
        flash.set_key(0);
        original.set_key(0);
        flash.set_wait(0);
        original.set_wait(0);
        if flash.0 != original.0 || u32::from(wait) > MAX_FLASH_WAIT {
            return Err(Error::FlashLatencyTimeout);
        }
        Ok(Some((wait, mirror)))
    }

    #[cfg(rcc_cw32l011_v1)]
    pub(super) fn retained(self, config: Config) -> Result<bool, Error> {
        let r = pac::SYSCTRL;
        self.post_observation()?;
        let mut original_sources = self.sources;
        let mut sources = r.cr1().read();
        let requested = sources.lsien();
        let excluded = config.hse.is_some()
            || config.lse.is_some()
            || self.sources.hseen()
            || self.sources.hseccs()
            || self.sources.lseccs()
            || sources.hseen()
            || sources.hseccs()
            || sources.lseccs();
        for source in [&mut original_sources, &mut sources] {
            source.set_key(0);
            source.set_hsien(false);
            source.set_clkccs(false);
        }
        let mut routes = r.cr2().read();
        let mut original_routes = self.routes;
        for route in [&mut original_routes, &mut routes] {
            route.set_key(0);
            route.set_flashwait(0);
        }
        let lsi = r.lsi().read();
        let lse = r.lse().read();
        let interrupts = r.ier().read();
        let mco = r.mco().read();
        self.post_observation()?;
        if excluded
            || !requested
            || sources.0 != original_sources.0
            || (lsi.0 & crate::RCC_LSI_PARAMETERS_MASK)
                != (self.lsi.0 & crate::RCC_LSI_PARAMETERS_MASK)
            || lse.0 != self.lse.0
            || routes.0 != original_routes.0
            || interrupts.0 != self.interrupts.0
            || mco.0 != self.mco.0
        {
            return Err(Error::ClockConfigurationTimeout);
        }
        Ok(lsi.stable())
    }

    #[cfg(rcc_cw32l011_v1)]
    #[allow(clippy::too_many_arguments)]
    pub(super) fn verify(
        self,
        config: Config,
        mut expected_clock: pac::sysctrl::regs::Cr0,
        mut expected_sources: pac::sysctrl::regs::Cr1,
        expected_hsi: pac::sysctrl::regs::Hsi,
        expected_hsi_stable: bool,
        expected_hse: pac::sysctrl::regs::Hse,
        expected_hse_stable: bool,
        expected_wait: Option<(u8, u8)>,
    ) -> Result<(), Error> {
        let lsi_stable = self.retained(config)?;
        let r = pac::SYSCTRL;
        let mut clock = r.cr0().read();
        let mut sources = r.cr1().read();
        let hsi = r.hsi().read();
        let hse = r.hse().read();
        let flash = self.flash_wait(expected_wait);
        let initial_mirror = if expected_wait.is_none() {
            Some(r.cr2().read().flashwait())
        } else {
            None
        };
        self.post_observation()?;
        clock.set_key(0);
        expected_clock.set_key(0);
        sources.set_key(0);
        expected_sources.set_key(0);
        if clock.0 != expected_clock.0
            || sources.0 != expected_sources.0
            || (hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                != (expected_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
            || hsi.stable() != expected_hsi_stable
            || (hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                != (expected_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
            || hse.stable() != expected_hse_stable
            || initial_mirror.is_some_and(|mirror| mirror != self.routes.flashwait())
        {
            return Err(Error::ClockConfigurationTimeout);
        }
        if !lsi_stable {
            return Err(Error::LsiNotReady);
        }
        if flash? != expected_wait {
            return Err(Error::FlashLatencyTimeout);
        }
        Ok(())
    }

    // These invariants have no acknowledgment interval. In particular an LSI
    // request/readiness loss is never repaired or polled into eligibility.
    #[cfg(rcc_cw32l010_v1)]
    pub(super) fn retained(self, config: Config) -> Result<bool, Error> {
        let r = pac::SYSCTRL;
        check_external_faults(
            config.hse.is_some() || self.sources.hseen() || self.sources.hseccs(),
            self.sources.lseen() || self.sources.lseccs(),
        )?;
        let mut original_sources = self.sources;
        let mut sources = r.cr1().read();
        let requested = sources.lsien();
        // Exactly these three controls are owned in the enclosing sequence;
        // their phase-specific full identity is checked at each use site.
        original_sources.set_key(0);
        sources.set_key(0);
        for source in [&mut original_sources, &mut sources] {
            source.set_hsien(false);
            source.set_hseen(false);
            source.set_clkccs(false);
        }
        let mut routes = r.cr2().read();
        let mut original_routes = self.routes;
        routes.set_flashwait(0);
        original_routes.set_flashwait(0);
        let lsi = r.lsi().read();
        if !requested
            || sources.0 != original_sources.0
            || (lsi.0 & crate::RCC_LSI_PARAMETERS_MASK)
                != (self.lsi.0 & crate::RCC_LSI_PARAMETERS_MASK)
            || r.lse().read().0 != self.lse.0
            || routes.0 != original_routes.0
            || r.ier().read().0 != self.interrupts.0
            || r.mco().read().0 != self.mco.0
        {
            return Err(Error::ClockConfigurationTimeout);
        }
        Ok(lsi.stable())
    }

    // Full settled phase identity. Pending fields are handled only by the
    // concrete local prior-to-target loops; no source-wide ignore mask exists.
    #[cfg(rcc_cw32l010_v1)]
    #[allow(clippy::too_many_arguments)]
    pub(super) fn verify(
        self,
        config: Config,
        mut expected_clock: pac::sysctrl::regs::Cr0,
        mut expected_sources: pac::sysctrl::regs::Cr1,
        expected_hsi: pac::sysctrl::regs::Hsi,
        expected_hsi_stable: bool,
        expected_hse: pac::sysctrl::regs::Hse,
        expected_hse_stable: bool,
        expected_wait: Option<(u8, u8)>,
    ) -> Result<(), Error> {
        let lsi_stable = self.retained(config)?;
        let r = pac::SYSCTRL;
        let mut clock = r.cr0().read();
        let mut sources = r.cr1().read();
        let hsi = r.hsi().read();
        let hse = r.hse().read();
        clock.set_key(0);
        expected_clock.set_key(0);
        sources.set_key(0);
        expected_sources.set_key(0);
        if sources.0 != expected_sources.0
            || (hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                != (expected_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
            || hsi.stable() != expected_hsi_stable
            || (hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                != (expected_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
            || hse.stable() != expected_hse_stable
        {
            return Err(Error::ClockConfigurationTimeout);
        }
        if !lsi_stable {
            return Err(Error::LsiNotReady);
        }
        if clock.0 != expected_clock.0 {
            return Err(Error::ClockConfigurationTimeout);
        }
        if let Some((wait, mirror)) = expected_wait {
            if pac::FLASH.cr2().read().wait() != wait || r.cr2().read().flashwait() != mirror {
                return Err(Error::FlashLatencyTimeout);
            }
        } else if r.cr2().read().flashwait() != self.routes.flashwait() {
            return Err(Error::ClockConfigurationTimeout);
        }
        Ok(())
    }
}

/// Final native live-LSI preparation: factory HSI carries the final divisors
/// and conservative WAIT before the last clock-tree mutation selects LSI.
#[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
pub(super) fn finish_lsi_sysclk(
    config: Config,
    clocks: Clocks,
    entry: LsiSysclkSnapshot,
    trim: u16,
    cs: critical_section::CriticalSection<'_>,
) -> Result<Clocks, Error> {
    #[cfg(rcc_cw32l010_v1)]
    let mut clocks = clocks;
    let r = pac::SYSCTRL;
    #[cfg(rcc_cw32l011_v1)]
    {
        let _ = cs;
        if config.hse.is_some()
            || config.lse.is_some()
            || clocks.hse.is_some()
            || entry.sources.hseen()
            || entry.sources.hseccs()
            || entry.sources.lseccs()
        {
            return Err(Error::ClockConfigurationTimeout);
        }
    }
    let mut expected_clock = entry.clock;
    expected_clock.set_key(0);
    expected_clock.set_sysclk(ClockSource::Hsi);
    expected_clock.set_hclkprs(entry.clock.hclkprs().max(AHBPrescaler::Div8 as u8));
    expected_clock.set_pclkprs(entry.clock.pclkprs().max(APBPrescaler::Div8 as u8));
    let mut expected_sources = entry.sources;
    expected_sources.set_key(0);
    expected_sources.set_hsien(true);
    expected_sources.set_hseen(config.hse.is_some() || entry.sources.hseen());
    let mut expected_hsi = entry.hsi;
    expected_hsi.set_trim(trim);
    expected_hsi.set_div(config.hsi.div as u8);
    let expected_hsi_stable = true;
    let expected_hse = entry.hse;
    #[cfg(rcc_cw32l010_v1)]
    let mut expected_hse = expected_hse;
    #[cfg(rcc_cw32l010_v1)]
    if let Some(hse) = config.hse {
        expected_hse.set_mode(hse.mode == HseMode::Bypass);
        expected_hse.set_driver(hse.drive);
        expected_hse.set_pdriver(hse.drive);
        expected_hse.set_waitcycle(pac::sysctrl::vals::HseWait::Cycles262144);
        expected_hse.set_digflt(false);
        expected_hse.set_detcnt(hse.detector_count()?);
    }
    let expected_hse_stable = config.hse.is_some() || entry.hse.stable();
    let mut expected_wait = Some((MAX_FLASH_WAIT as u8, MAX_FLASH_WAIT as u8));
    entry.verify(
        config,
        expected_clock,
        expected_sources,
        expected_hsi,
        expected_hsi_stable,
        expected_hse,
        expected_hse_stable,
        expected_wait,
    )?;
    r.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_hclkprs(config.ahb_pre as u8);
        w.set_pclkprs(config.apb_pre as u8);
    });
    expected_clock.set_hclkprs(config.ahb_pre as u8);
    expected_clock.set_pclkprs(config.apb_pre as u8);
    // Final dividers are settled identity, not readiness to poll back to health.
    entry.verify(
        config,
        expected_clock,
        expected_sources,
        expected_hsi,
        expected_hsi_stable,
        expected_hse,
        expected_hse_stable,
        expected_wait,
    )?;
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
        .max(
            crate::rcc::ClockBounds::hsi(crate::RCC_FIXED_CCS_HSI_DIVISOR)
                .maximum()
                .0,
        );
    let final_wait = (upper_hclk - 1) / crate::RCC_FLASH_WAIT_STEP_HZ;
    if final_wait > MAX_FLASH_WAIT {
        return Err(Error::FlashLatencyTimeout);
    }
    entry.verify(
        config,
        expected_clock,
        expected_sources,
        expected_hsi,
        expected_hsi_stable,
        expected_hse,
        expected_hse_stable,
        expected_wait,
    )?;
    #[cfg(rcc_cw32l010_v1)]
    pac::FLASH.cr2().modify(|w| {
        w.set_key(0x5a5a);
        w.set_wait(final_wait as u8);
    });
    {
        #[cfg(rcc_cw32l011_v1)]
        let flash_write = LsiSysclkSnapshot::write_flash_wait(final_wait as u8);
        // Final WAIT and mirror acknowledge under verified final-divided factory HSI.
        let prior_0 = expected_wait.unwrap().0;
        let target_0 = final_wait as u8;
        let mut acknowledged_0 = prior_0 == target_0;
        let prior_1 = expected_wait.unwrap().1;
        let target_1 = final_wait as u8;
        let mut acknowledged_1 = prior_1 == target_1;
        for attempt in 0..config.timeout {
            let lsi_stable = entry.retained(config)?;
            let mut current_clock = r.cr0().read();
            let mut current_sources = r.cr1().read();
            let current_hsi = r.hsi().read();
            let current_hse = r.hse().read();
            let current_hsi_stable = current_hsi.stable();
            let current_hse_stable = current_hse.stable();
            #[cfg(rcc_cw32l010_v1)]
            let current_wait =
                expected_wait.map(|_| (pac::FLASH.cr2().read().wait(), r.cr2().read().flashwait()));
            #[cfg(rcc_cw32l011_v1)]
            let current_wait = entry.flash_wait(expected_wait);
            #[cfg(rcc_cw32l011_v1)]
            entry.post_observation()?;
            current_clock.set_key(0);
            current_sources.set_key(0);
            if current_sources.0 != expected_sources.0
                || (current_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                    != (expected_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                || current_hsi_stable != expected_hsi_stable
                || (current_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                    != (expected_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                || current_hse_stable != expected_hse_stable
            {
                return Err(Error::ClockConfigurationTimeout);
            }
            #[cfg(rcc_cw32l010_v1)]
            if !lsi_stable {
                return Err(Error::LsiNotReady);
            }
            if current_clock.0 != expected_clock.0 {
                return Err(Error::ClockConfigurationTimeout);
            }
            #[cfg(rcc_cw32l011_v1)]
            if !lsi_stable {
                return Err(Error::LsiNotReady);
            }
            #[cfg(rcc_cw32l011_v1)]
            let current_wait = flash_write.and(current_wait)?;
            #[cfg(rcc_cw32l011_v1)]
            if current_wait.unwrap().0 != current_wait.unwrap().1 {
                return Err(Error::FlashLatencyTimeout);
            }
            let observed_0 = current_wait.unwrap().0;
            if observed_0 != target_0 && (observed_0 != prior_0 || acknowledged_0) {
                return Err(Error::FlashLatencyTimeout);
            }
            acknowledged_0 |= observed_0 == target_0;
            let observed_1 = current_wait.unwrap().1;
            if observed_1 != target_1 && (observed_1 != prior_1 || acknowledged_1) {
                return Err(Error::FlashLatencyTimeout);
            }
            acknowledged_1 |= observed_1 == target_1;
            if acknowledged_0 && acknowledged_1 {
                break;
            }
            if attempt + 1 == config.timeout {
                return Err(Error::FlashLatencyTimeout);
            }
            core::hint::spin_loop();
        }
        expected_wait.as_mut().unwrap().0 = target_0;
        expected_wait.as_mut().unwrap().1 = target_1;
        entry.verify(
            config,
            expected_clock,
            expected_sources,
            expected_hsi,
            expected_hsi_stable,
            expected_hse,
            expected_hse_stable,
            expected_wait,
        )?;
    }
    barrier();
    entry.verify(
        config,
        expected_clock,
        expected_sources,
        expected_hsi,
        expected_hsi_stable,
        expected_hse,
        expected_hse_stable,
        expected_wait,
    )?;
    // Last CR0/CR1/oscillator/FLASH mutation for this target.
    r.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_sysclk(ClockSource::Lsi);
    });
    {
        // Only HSI or the requested final LSI selector may appear while acknowledgment is pending.
        let prior_0 = expected_clock.sysclk();
        let target_0 = ClockSource::Lsi;
        let mut acknowledged_0 = prior_0 == target_0;
        for attempt in 0..config.timeout {
            let lsi_stable = entry.retained(config)?;
            let mut current_clock = r.cr0().read();
            let mut current_sources = r.cr1().read();
            let current_hsi = r.hsi().read();
            let current_hse = r.hse().read();
            let current_hsi_stable = current_hsi.stable();
            let current_hse_stable = current_hse.stable();
            #[cfg(rcc_cw32l010_v1)]
            let current_wait =
                expected_wait.map(|_| (pac::FLASH.cr2().read().wait(), r.cr2().read().flashwait()));
            #[cfg(rcc_cw32l011_v1)]
            let current_wait = entry.flash_wait(expected_wait);
            #[cfg(rcc_cw32l011_v1)]
            entry.post_observation()?;
            current_clock.set_key(0);
            current_sources.set_key(0);
            if current_sources.0 != expected_sources.0
                || (current_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                    != (expected_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                || current_hsi_stable != expected_hsi_stable
                || (current_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                    != (expected_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                || current_hse_stable != expected_hse_stable
            {
                return Err(Error::ClockConfigurationTimeout);
            }
            #[cfg(rcc_cw32l010_v1)]
            if !lsi_stable {
                return Err(Error::LsiNotReady);
            }
            let observed_0 = current_clock.sysclk();
            if observed_0 != target_0 && (observed_0 != prior_0 || acknowledged_0) {
                return Err(Error::ClockConfigurationTimeout);
            }
            acknowledged_0 |= observed_0 == target_0;
            current_clock.set_sysclk(prior_0);
            if current_clock.0 != expected_clock.0 {
                return Err(Error::ClockConfigurationTimeout);
            }
            #[cfg(rcc_cw32l011_v1)]
            if !lsi_stable {
                return Err(Error::LsiNotReady);
            }
            #[cfg(rcc_cw32l011_v1)]
            let current_wait = current_wait?;
            if current_wait != expected_wait {
                return Err(Error::FlashLatencyTimeout);
            }
            if acknowledged_0 {
                break;
            }
            if attempt + 1 == config.timeout {
                return Err(Error::LsiClockSwitchTimeout);
            }
            core::hint::spin_loop();
        }
        expected_clock.set_sysclk(target_0);
        entry.verify(
            config,
            expected_clock,
            expected_sources,
            expected_hsi,
            expected_hsi_stable,
            expected_hse,
            expected_hse_stable,
            expected_wait,
        )?;
    }
    barrier();
    entry.verify(
        config,
        expected_clock,
        expected_sources,
        expected_hsi,
        expected_hsi_stable,
        expected_hse,
        expected_hse_stable,
        expected_wait,
    )?;
    #[cfg(rcc_cw32l010_v1)]
    {
        let final_hse_mode = config.hse.map(|hse| hse.mode).or_else(|| {
            entry.sources.hseen().then_some(if entry.hse.mode() {
                HseMode::Bypass
            } else {
                HseMode::Oscillator
            })
        });
        if let Some(mode) = final_hse_mode {
            let pads = crate::rcc_hse_pins_match(mode == HseMode::Bypass, config.timeout, cs, true);
            if let Err(error @ Error::LsiConfigurationGateTimeout { .. }) = pads {
                return Err(error);
            }
            // Successful restoration precedes faults/identity, then buffered pads.
            entry.verify(
                config,
                expected_clock,
                expected_sources,
                expected_hsi,
                expected_hsi_stable,
                expected_hse,
                expected_hse_stable,
                expected_wait,
            )?;
            if !pads? {
                return Err(Error::ClockConfigurationTimeout);
            }
        }
        if entry.sources.hseen() {
            if !entry.hse.mode() {
                clocks.hse = Some(HseMode::Oscillator);
            } else if clocks.hse.is_none() {
                clocks.hse = Some(HseMode::Bypass);
            }
        }
    }
    #[cfg(rcc_cw32l011_v1)]
    {
        let current_sources = r.cr1().read();
        entry.post_observation()?;
        let final_hse_mode = config.hse.map(|hse| hse.mode).or_else(|| {
            entry.sources.hseen().then_some(if entry.hse.mode() {
                HseMode::Bypass
            } else {
                HseMode::Oscillator
            })
        });
        if config.hse.is_some()
            || final_hse_mode.is_some()
            || clocks.hse.is_some()
            || entry.sources.hseen()
            || current_sources.hseen()
            || current_sources.hseccs()
            || current_sources.lseccs()
        {
            return Err(Error::ClockConfigurationTimeout);
        }
    }
    entry.verify(
        config,
        expected_clock,
        expected_sources,
        expected_hsi,
        expected_hsi_stable,
        expected_hse,
        expected_hse_stable,
        expected_wait,
    )?;
    Ok(clocks)
}
