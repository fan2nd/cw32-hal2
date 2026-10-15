use super::*;

pub(super) fn establish(
    config: Config,
    cs: critical_section::CriticalSection<'_>,
    observed: &Entry,
    #[cfg(all(rcc_cw32l010_v1, rcc_lsi_sysclk))] targets: &Targets,
    admitted: &Admitted,
    #[cfg(all(rcc_cw32l010_v1, rcc_lsi_sysclk))] expected: &mut Expected,
) -> Result<(), Error> {
    let r = observed.r;
    let live_lsi = observed.live_lsi;
    let monitor_hse = observed.monitor_hse;
    let monitor_lse = observed.monitor_lse;
    let preserve_hse = admitted.preserve_hse;
    if let Some(hse) = config.hse.filter(|_| !preserve_hse) {
        #[cfg(all(rcc_cw32l010_v1, rcc_lsi_sysclk))]
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
            w.set_hseen(false);
        });
        #[cfg(all(rcc_cw32l010_v1, rcc_lsi_sysclk))]
        if let Some(entry) = targets.lsi {
            // Optional unowned HSE stops under verified calibrated HSI.
            let prior_0 = expected.sources.hseen();
            let target_0 = false;
            let mut acknowledged_0 = prior_0 == target_0;
            let prior_1 = expected.hse_stable;
            let target_1 = false;
            let mut acknowledged_1 = prior_1 == target_1;
            for attempt in 0..config.timeout {
                let lsi_stable = entry.retained(config)?;
                let mut current_clock = r.cr0().read();
                let mut current_sources = r.cr1().read();
                let current_hsi = r.hsi().read();
                let current_hse = r.hse().read();
                let current_hsi_stable = current_hsi.stable();
                let current_hse_stable = current_hse.stable();
                let current_wait = expected
                    .wait
                    .map(|_| (pac::FLASH.cr2().read().wait(), r.cr2().read().flashwait()));
                let observed_0 = current_sources.hseen();
                if observed_0 != target_0 && (observed_0 != prior_0 || acknowledged_0) {
                    return Err(Error::ClockConfigurationTimeout);
                }
                acknowledged_0 |= observed_0 == target_0;
                current_sources.set_hseen(prior_0);
                let observed_1 = current_hse_stable;
                if observed_1 != target_1 && (observed_1 != prior_1 || acknowledged_1) {
                    return Err(Error::ClockConfigurationTimeout);
                }
                acknowledged_1 |= observed_1 == target_1;
                current_clock.set_key(0);
                current_sources.set_key(0);
                if current_sources.0 != expected.sources.0
                    || (current_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                        != (expected.hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                    || current_hsi_stable != expected.hsi_stable
                    || (current_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                        != (expected.hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                {
                    return Err(Error::ClockConfigurationTimeout);
                }
                if !lsi_stable {
                    return Err(Error::LsiNotReady);
                }
                if current_clock.0 != expected.clock.0 {
                    return Err(Error::ClockConfigurationTimeout);
                }
                if current_wait != expected.wait {
                    return Err(Error::FlashLatencyTimeout);
                }
                if acknowledged_0 && acknowledged_1 {
                    break;
                }
                if attempt + 1 == config.timeout {
                    return Err(Error::HseStopTimeout);
                }
                core::hint::spin_loop();
            }
            expected.sources.set_hseen(target_0);
            expected.hse_stable = target_1;
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
                Error::HseStopTimeout,
                monitor_hse,
                monitor_lse,
                || !r.cr1().read().hseen() && !r.hse().read().stable(),
            )?;
        }
        #[cfg(all(rcc_cw32l010_v1, rcc_lsi_sysclk))]
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
        let configured_pads = crate::rcc_configure_hse_pins(
            hse.mode == HseMode::Bypass,
            config.timeout,
            cs,
            #[cfg(all(rcc_cw32l010_v1, rcc_lsi_sysclk))]
            targets.lsi.is_some(),
        );
        #[cfg(all(rcc_cw32l010_v1, rcc_lsi_sysclk))]
        if let Some(entry) = targets.lsi {
            if let Err(
                error @ (Error::LsiConfigurationGateTimeout { .. }
                | Error::HsePinConfigurationTimeout),
            ) = configured_pads
            {
                return Err(error);
            }
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
        configured_pads?;
        let detector = hse.detector_count()?;
        #[cfg(all(rcc_cw32l010_v1, rcc_lsi_sysclk))]
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
        r.hse().modify(|w| {
            w.set_mode(hse.mode == HseMode::Bypass);
            w.set_driver(hse.drive);
            w.set_pdriver(hse.drive);
            w.set_waitcycle(pac::sysctrl::vals::HseWait::Cycles262144);
            w.set_digflt(false);
            w.set_detcnt(detector);
        });
        #[cfg(all(rcc_cw32l010_v1, rcc_lsi_sysclk))]
        if let Some(entry) = targets.lsi {
            // Only the six declared HSE fields can progress; HEXENPOL and all other bits stay exact.
            let prior_0 = expected.hse.mode();
            let target_0 = hse.mode == HseMode::Bypass;
            let mut acknowledged_0 = prior_0 == target_0;
            let prior_1 = expected.hse.driver();
            let target_1 = hse.drive;
            let mut acknowledged_1 = prior_1 == target_1;
            let prior_2 = expected.hse.pdriver();
            let target_2 = hse.drive;
            let mut acknowledged_2 = prior_2 == target_2;
            let prior_3 = expected.hse.waitcycle();
            let target_3 = pac::sysctrl::vals::HseWait::Cycles262144;
            let mut acknowledged_3 = prior_3 == target_3;
            let prior_4 = expected.hse.digflt();
            let target_4 = false;
            let mut acknowledged_4 = prior_4 == target_4;
            let prior_5 = expected.hse.detcnt();
            let target_5 = detector;
            let mut acknowledged_5 = prior_5 == target_5;
            for attempt in 0..config.timeout {
                let lsi_stable = entry.retained(config)?;
                let mut current_clock = r.cr0().read();
                let mut current_sources = r.cr1().read();
                let current_hsi = r.hsi().read();
                let mut current_hse = r.hse().read();
                let current_hsi_stable = current_hsi.stable();
                let current_hse_stable = current_hse.stable();
                let current_wait = expected
                    .wait
                    .map(|_| (pac::FLASH.cr2().read().wait(), r.cr2().read().flashwait()));
                let observed_0 = current_hse.mode();
                if observed_0 != target_0 && (observed_0 != prior_0 || acknowledged_0) {
                    return Err(Error::ClockConfigurationTimeout);
                }
                acknowledged_0 |= observed_0 == target_0;
                current_hse.set_mode(prior_0);
                let observed_1 = current_hse.driver();
                if observed_1 != target_1 && (observed_1 != prior_1 || acknowledged_1) {
                    return Err(Error::ClockConfigurationTimeout);
                }
                acknowledged_1 |= observed_1 == target_1;
                current_hse.set_driver(prior_1);
                let observed_2 = current_hse.pdriver();
                if observed_2 != target_2 && (observed_2 != prior_2 || acknowledged_2) {
                    return Err(Error::ClockConfigurationTimeout);
                }
                acknowledged_2 |= observed_2 == target_2;
                current_hse.set_pdriver(prior_2);
                let observed_3 = current_hse.waitcycle();
                if observed_3 != target_3 && (observed_3 != prior_3 || acknowledged_3) {
                    return Err(Error::ClockConfigurationTimeout);
                }
                acknowledged_3 |= observed_3 == target_3;
                current_hse.set_waitcycle(prior_3);
                let observed_4 = current_hse.digflt();
                if observed_4 != target_4 && (observed_4 != prior_4 || acknowledged_4) {
                    return Err(Error::ClockConfigurationTimeout);
                }
                acknowledged_4 |= observed_4 == target_4;
                current_hse.set_digflt(prior_4);
                let observed_5 = current_hse.detcnt();
                if observed_5 != target_5 && (observed_5 != prior_5 || acknowledged_5) {
                    return Err(Error::ClockConfigurationTimeout);
                }
                acknowledged_5 |= observed_5 == target_5;
                current_hse.set_detcnt(prior_5);
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
                if !lsi_stable {
                    return Err(Error::LsiNotReady);
                }
                if current_clock.0 != expected.clock.0 {
                    return Err(Error::ClockConfigurationTimeout);
                }
                if current_wait != expected.wait {
                    return Err(Error::FlashLatencyTimeout);
                }
                if acknowledged_0
                    && acknowledged_1
                    && acknowledged_2
                    && acknowledged_3
                    && acknowledged_4
                    && acknowledged_5
                {
                    break;
                }
                if attempt + 1 == config.timeout {
                    return Err(Error::ClockConfigurationTimeout);
                }
                core::hint::spin_loop();
            }
            expected.hse.set_mode(target_0);
            expected.hse.set_driver(target_1);
            expected.hse.set_pdriver(target_2);
            expected.hse.set_waitcycle(target_3);
            expected.hse.set_digflt(target_4);
            expected.hse.set_detcnt(target_5);
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
                || hse_parameters_match(hse).unwrap_or(false),
                config.timeout,
                Error::ClockConfigurationTimeout,
            )?;
        }
        #[cfg(all(rcc_cw32l010_v1, rcc_lsi_sysclk))]
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
            w.set_hseen(true);
        });
        #[cfg(all(rcc_cw32l010_v1, rcc_lsi_sysclk))]
        if let Some(entry) = targets.lsi {
            // Declared HSE may become ready only with its acknowledged parameter image.
            let prior_0 = expected.sources.hseen();
            let target_0 = true;
            let mut acknowledged_0 = prior_0 == target_0;
            let prior_1 = expected.hse_stable;
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
                let current_wait = expected
                    .wait
                    .map(|_| (pac::FLASH.cr2().read().wait(), r.cr2().read().flashwait()));
                let observed_0 = current_sources.hseen();
                if observed_0 != target_0 && (observed_0 != prior_0 || acknowledged_0) {
                    return Err(Error::ClockConfigurationTimeout);
                }
                acknowledged_0 |= observed_0 == target_0;
                current_sources.set_hseen(prior_0);
                let observed_1 = current_hse_stable;
                if observed_1 != target_1 && (observed_1 != prior_1 || acknowledged_1) {
                    return Err(Error::ClockConfigurationTimeout);
                }
                acknowledged_1 |= observed_1 == target_1;
                current_clock.set_key(0);
                current_sources.set_key(0);
                if current_sources.0 != expected.sources.0
                    || (current_hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                        != (expected.hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK)
                    || current_hsi_stable != expected.hsi_stable
                    || (current_hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                        != (expected.hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK)
                {
                    return Err(Error::ClockConfigurationTimeout);
                }
                if !lsi_stable {
                    return Err(Error::LsiNotReady);
                }
                if current_clock.0 != expected.clock.0 {
                    return Err(Error::ClockConfigurationTimeout);
                }
                if current_wait != expected.wait {
                    return Err(Error::FlashLatencyTimeout);
                }
                if acknowledged_0 && acknowledged_1 {
                    break;
                }
                if attempt + 1 == config.timeout {
                    return Err(Error::HseTimeout);
                }
                core::hint::spin_loop();
            }
            expected.sources.set_hseen(target_0);
            expected.hse_stable = target_1;
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
                Error::HseTimeout,
                monitor_hse,
                monitor_lse,
                || r.cr1().read().hseen() && r.hse().read().stable(),
            )?;
        }
    }
    Ok(())
}
