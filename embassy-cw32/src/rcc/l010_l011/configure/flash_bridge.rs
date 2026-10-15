use super::*;

pub(super) fn bridge(
    config: Config,
    cs: critical_section::CriticalSection<'_>,
    observed: &Entry,
    #[cfg(any(rcc_lse, all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk)))]
    targets: &mut Targets,
    admitted: Admitted,
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))] expected: &mut Expected,
) -> Result<Admitted, Error> {
    let r = observed.r;
    let old_clock = observed.old_clock;
    #[cfg(rcc_lse)]
    let trim = observed.trim;
    let live_lsi = observed.live_lsi;
    let monitor_hse = observed.monitor_hse;
    let monitor_lse = observed.monitor_lse;
    #[cfg(rcc_lse)]
    let lse_admission = admitted.lse_admission;
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = targets.lsi {
        entry.verify(
            config,
            expected.clock,
            expected.sources,
            expected.hsi,
            expected.hsi_stable,
            expected.hse,
            expected.hse_stable,
            expected.wait,
        )?;
    }
    #[cfg(all(rcc_cw32l011_v1, rcc_lsi_sysclk))]
    if let Some(entry) = targets.lsi {
        if <crate::peripherals::FLASH as crate::rcc::SealedRccPeripheral>::RCC_INFO.reset_asserted()
        {
            entry.verify(
                config,
                expected.clock,
                expected.sources,
                expected.hsi,
                expected.hsi_stable,
                expected.hse,
                expected.hse_stable,
                expected.wait,
            )?;
            return Err(Error::FlashClockTimeout);
        }
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
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = targets.lsi {
        entry.verify(
            config,
            expected.clock,
            expected.sources,
            expected.hsi,
            expected.hsi_stable,
            expected.hse,
            expected.hse_stable,
            expected.wait,
        )?;
    }
    #[cfg(all(rcc_cw32l010_v1, rcc_lsi_sysclk))]
    if let Some(entry) = targets.lsi {
        let wait = pac::FLASH.cr2().read().wait();
        if u32::from(wait) > MAX_FLASH_WAIT || wait != entry.routes.flashwait() {
            return Err(Error::InvalidEntryClock);
        }
        expected.wait = Some((wait, wait));
        entry.verify(
            config,
            expected.clock,
            expected.sources,
            expected.hsi,
            expected.hsi_stable,
            expected.hse,
            expected.hse_stable,
            expected.wait,
        )?;
    }
    #[cfg(all(rcc_cw32l011_v1, rcc_lsi_sysclk))]
    if let Some(entry) = targets.lsi.as_mut() {
        let flash = LsiSysclkSnapshot::read_flash();
        // Buffer access failure until the complete source/fault observation.
        entry.verify(
            config,
            expected.clock,
            expected.sources,
            expected.hsi,
            expected.hsi_stable,
            expected.hse,
            expected.hse_stable,
            expected.wait,
        )?;
        let flash = flash?;
        let wait = flash.wait();
        if u32::from(wait) > MAX_FLASH_WAIT || wait != entry.routes.flashwait() {
            return Err(Error::InvalidEntryClock);
        }
        // Only this separate Flash authority is attached; no source is recaptured.
        entry.flash = Some(flash);
        expected.wait = Some((wait, wait));
        entry.verify(
            config,
            expected.clock,
            expected.sources,
            expected.hsi,
            expected.hsi_stable,
            expected.hse,
            expected.hse_stable,
            expected.wait,
        )?;
    }
    let legacy_flash_check = true;
    #[cfg(all(rcc_cw32l011_v1, rcc_lsi_sysclk))]
    let legacy_flash_check = legacy_flash_check && !live_lsi;
    if legacy_flash_check && u32::from(pac::FLASH.cr2().read().wait()) > MAX_FLASH_WAIT {
        return Err(Error::InvalidEntryClock);
    }
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = targets.lsi {
        entry.verify(
            config,
            expected.clock,
            expected.sources,
            expected.hsi,
            expected.hsi_stable,
            expected.hse,
            expected.hse_stable,
            expected.wait,
        )?;
    }
    #[cfg(all(rcc_cw32l010_v1, rcc_lsi_sysclk))]
    if live_lsi {
        pac::FLASH.cr2().modify(|w| {
            w.set_key(0x5a5a);
            w.set_wait(MAX_FLASH_WAIT as u8);
        });
    }
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = targets.lsi {
        #[cfg(rcc_cw32l011_v1)]
        let flash_write = LsiSysclkSnapshot::write_flash_wait(MAX_FLASH_WAIT as u8);
        // Both authoritative WAIT and its route mirror acknowledge the same owned write.
        let prior_0 = expected.wait.unwrap().0;
        let target_0 = MAX_FLASH_WAIT as u8;
        let mut acknowledged_0 = prior_0 == target_0;
        let prior_1 = expected.wait.unwrap().1;
        let target_1 = MAX_FLASH_WAIT as u8;
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
            let current_wait = expected
                .wait
                .map(|_| (pac::FLASH.cr2().read().wait(), r.cr2().read().flashwait()));
            #[cfg(rcc_cw32l011_v1)]
            let current_wait = entry.flash_wait(expected.wait);
            #[cfg(rcc_cw32l011_v1)]
            entry.post_observation()?;
            current_clock.set_key(0);
            current_sources.set_key(0);
            if current_sources.0 != expected.sources.0
                || (current_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                    != (expected.hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                || current_hsi_stable != expected.hsi_stable
                || (current_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                    != (expected.hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                || current_hse_stable != expected.hse_stable
            {
                return Err(Error::ClockConfigurationTimeout);
            }
            #[cfg(rcc_cw32l010_v1)]
            if !lsi_stable {
                return Err(Error::LsiNotReady);
            }
            if current_clock.0 != expected.clock.0 {
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
        expected.wait.as_mut().unwrap().0 = target_0;
        expected.wait.as_mut().unwrap().1 = target_1;
        entry.verify(
            config,
            expected.clock,
            expected.sources,
            expected.hsi,
            expected.hsi_stable,
            expected.hse,
            expected.hse_stable,
            expected.wait,
        )?;
    }
    if !live_lsi {
        set_flash_latency(MAX_FLASH_WAIT, config.timeout)?;
    } else {
        barrier();
    }
    // The unchanged legal inherited HSIOSC may not yet have factory trim.
    // /8 covers the own-family legal oscillator range even below 1.8 V.
    let guard_hclk = old_clock.hclkprs().max(AHBPrescaler::Div8 as u8);
    let guard_pclk = old_clock.pclkprs().max(APBPrescaler::Div8 as u8);
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = targets.lsi {
        entry.verify(
            config,
            expected.clock,
            expected.sources,
            expected.hsi,
            expected.hsi_stable,
            expected.hse,
            expected.hse_stable,
            expected.wait,
        )?;
    }
    r.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_hclkprs(guard_hclk);
        w.set_pclkprs(guard_pclk);
    });
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = targets.lsi {
        // Only the two guarded dividers may advance, independently and without reversal.
        let prior_0 = expected.clock.hclkprs();
        let target_0 = guard_hclk;
        let mut acknowledged_0 = prior_0 == target_0;
        let prior_1 = expected.clock.pclkprs();
        let target_1 = guard_pclk;
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
            let current_wait = expected
                .wait
                .map(|_| (pac::FLASH.cr2().read().wait(), r.cr2().read().flashwait()));
            #[cfg(rcc_cw32l011_v1)]
            let current_wait = entry.flash_wait(expected.wait);
            #[cfg(rcc_cw32l011_v1)]
            entry.post_observation()?;
            current_clock.set_key(0);
            current_sources.set_key(0);
            if current_sources.0 != expected.sources.0
                || (current_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                    != (expected.hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                || current_hsi_stable != expected.hsi_stable
                || (current_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                    != (expected.hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                || current_hse_stable != expected.hse_stable
            {
                return Err(Error::ClockConfigurationTimeout);
            }
            #[cfg(rcc_cw32l010_v1)]
            if !lsi_stable {
                return Err(Error::LsiNotReady);
            }
            let observed_0 = current_clock.hclkprs();
            if observed_0 != target_0 && (observed_0 != prior_0 || acknowledged_0) {
                return Err(Error::ClockConfigurationTimeout);
            }
            acknowledged_0 |= observed_0 == target_0;
            current_clock.set_hclkprs(prior_0);
            let observed_1 = current_clock.pclkprs();
            if observed_1 != target_1 && (observed_1 != prior_1 || acknowledged_1) {
                return Err(Error::ClockConfigurationTimeout);
            }
            acknowledged_1 |= observed_1 == target_1;
            current_clock.set_pclkprs(prior_1);
            if current_clock.0 != expected.clock.0 {
                return Err(Error::ClockConfigurationTimeout);
            }
            #[cfg(rcc_cw32l011_v1)]
            if !lsi_stable {
                return Err(Error::LsiNotReady);
            }
            #[cfg(rcc_cw32l011_v1)]
            let current_wait = current_wait?;
            if current_wait != expected.wait {
                return Err(Error::FlashLatencyTimeout);
            }
            if acknowledged_0 && acknowledged_1 {
                break;
            }
            if attempt + 1 == config.timeout {
                return Err(Error::ClockConfigurationTimeout);
            }
            core::hint::spin_loop();
        }
        expected.clock.set_hclkprs(target_0);
        expected.clock.set_pclkprs(target_1);
        entry.verify(
            config,
            expected.clock,
            expected.sources,
            expected.hsi,
            expected.hsi_stable,
            expected.hse,
            expected.hse_stable,
            expected.wait,
        )?;
    }
    if !live_lsi {
        poll(
            || {
                let v = r.cr0().read();
                v.hclkprs() == guard_hclk && v.pclkprs() == guard_pclk
            },
            config.timeout,
            Error::ClockConfigurationTimeout,
        )?;
    }
    barrier();

    // Native preflight includes all CR2, whose FLASHWAIT is the same-function
    // view of the WAIT field we just changed. Prove all ORIGINAL non-WAIT bits,
    // IER, LSE and LSI parameters before recapturing only this owned difference.
    // A monitored source already passed stable-LSI admission before any gate.
    #[cfg(rcc_lse)]
    let lse_admission = if let Some(entry) = targets.lse {
        if u32::from(pac::FLASH.cr2().read().wait()) != MAX_FLASH_WAIT {
            return Err(Error::FlashLatencyTimeout);
        }
        entry.before_lse_start(config)?;
        entry.hsi_ownership(trim)?;
        let refreshed = config
            .lse
            .map(|c| crate::rcc::lse::preflight(c, cs, false))
            .transpose()?;
        entry.before_lse_start(config)?;
        refreshed
    } else {
        lse_admission
    };

    // First leave the incoming source on unchanged HSI under guarded buses.
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = targets.lsi {
        entry.verify(
            config,
            expected.clock,
            expected.sources,
            expected.hsi,
            expected.hsi_stable,
            expected.hse,
            expected.hse_stable,
            expected.wait,
        )?;
    }
    r.cr1().modify(|w| {
        w.set_key(0x5a5a);
        w.set_hsien(true);
    });
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = targets.lsi {
        // Unchanged HSI may become requested and ready; every other field retains entry identity.
        let prior_0 = expected.sources.hsien();
        let target_0 = true;
        let mut acknowledged_0 = prior_0 == target_0;
        let prior_1 = expected.hsi_stable;
        let target_1 = true;
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
            let current_wait = expected
                .wait
                .map(|_| (pac::FLASH.cr2().read().wait(), r.cr2().read().flashwait()));
            #[cfg(rcc_cw32l011_v1)]
            let current_wait = entry.flash_wait(expected.wait);
            #[cfg(rcc_cw32l011_v1)]
            entry.post_observation()?;
            let observed_0 = current_sources.hsien();
            if observed_0 != target_0 && (observed_0 != prior_0 || acknowledged_0) {
                return Err(Error::ClockConfigurationTimeout);
            }
            acknowledged_0 |= observed_0 == target_0;
            current_sources.set_hsien(prior_0);
            let observed_1 = current_hsi_stable;
            if observed_1 != target_1 && (observed_1 != prior_1 || acknowledged_1) {
                return Err(Error::ClockConfigurationTimeout);
            }
            acknowledged_1 |= observed_1 == target_1;
            current_clock.set_key(0);
            current_sources.set_key(0);
            if current_sources.0 != expected.sources.0
                || (current_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                    != (expected.hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                || (current_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                    != (expected.hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                || current_hse_stable != expected.hse_stable
            {
                return Err(Error::ClockConfigurationTimeout);
            }
            #[cfg(rcc_cw32l010_v1)]
            if !lsi_stable {
                return Err(Error::LsiNotReady);
            }
            if current_clock.0 != expected.clock.0 {
                return Err(Error::ClockConfigurationTimeout);
            }
            #[cfg(rcc_cw32l011_v1)]
            if !lsi_stable {
                return Err(Error::LsiNotReady);
            }
            #[cfg(rcc_cw32l011_v1)]
            let current_wait = current_wait?;
            if current_wait != expected.wait {
                return Err(Error::FlashLatencyTimeout);
            }
            if acknowledged_0 && acknowledged_1 {
                break;
            }
            if attempt + 1 == config.timeout {
                return Err(Error::HsiTimeout);
            }
            core::hint::spin_loop();
        }
        expected.sources.set_hsien(target_0);
        expected.hsi_stable = target_1;
        entry.verify(
            config,
            expected.clock,
            expected.sources,
            expected.hsi,
            expected.hsi_stable,
            expected.hse,
            expected.hse_stable,
            expected.wait,
        )?;
    }
    if !live_lsi {
        poll_clock(
            config.timeout,
            Error::HsiTimeout,
            monitor_hse,
            monitor_lse,
            || r.cr1().read().hsien() && r.hsi().read().stable(),
        )?;
    }
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = targets.lsi {
        entry.verify(
            config,
            expected.clock,
            expected.sources,
            expected.hsi,
            expected.hsi_stable,
            expected.hse,
            expected.hse_stable,
            expected.wait,
        )?;
    }
    r.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_sysclk(ClockSource::Hsi);
    });
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = targets.lsi {
        // Only the acknowledged entry selector or requested HSI selector is admissible.
        let prior_0 = expected.clock.sysclk();
        let target_0 = ClockSource::Hsi;
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
            let current_wait = expected
                .wait
                .map(|_| (pac::FLASH.cr2().read().wait(), r.cr2().read().flashwait()));
            #[cfg(rcc_cw32l011_v1)]
            let current_wait = entry.flash_wait(expected.wait);
            #[cfg(rcc_cw32l011_v1)]
            entry.post_observation()?;
            current_clock.set_key(0);
            current_sources.set_key(0);
            if current_sources.0 != expected.sources.0
                || (current_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                    != (expected.hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                || current_hsi_stable != expected.hsi_stable
                || (current_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                    != (expected.hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                || current_hse_stable != expected.hse_stable
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
            if current_clock.0 != expected.clock.0 {
                return Err(Error::ClockConfigurationTimeout);
            }
            #[cfg(rcc_cw32l011_v1)]
            if !lsi_stable {
                return Err(Error::LsiNotReady);
            }
            #[cfg(rcc_cw32l011_v1)]
            let current_wait = current_wait?;
            if current_wait != expected.wait {
                return Err(Error::FlashLatencyTimeout);
            }
            if acknowledged_0 {
                break;
            }
            if attempt + 1 == config.timeout {
                return Err(Error::ClockSwitchTimeout);
            }
            core::hint::spin_loop();
        }
        expected.clock.set_sysclk(target_0);
        entry.verify(
            config,
            expected.clock,
            expected.sources,
            expected.hsi,
            expected.hsi_stable,
            expected.hse,
            expected.hse_stable,
            expected.wait,
        )?;
    }
    if !live_lsi {
        poll_clock(
            config.timeout,
            Error::ClockSwitchTimeout,
            monitor_hse,
            monitor_lse,
            || r.cr0().read().sysclk() == ClockSource::Hsi,
        )?;
    }
    barrier();
    #[cfg(rcc_lse)]
    let mut admitted = admitted;
    #[cfg(rcc_lse)]
    {
        admitted.lse_admission = lse_admission;
    }
    Ok(admitted)
}
