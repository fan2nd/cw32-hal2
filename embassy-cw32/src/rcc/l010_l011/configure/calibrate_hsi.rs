use super::*;

pub(super) fn calibrate(
    config: Config,
    #[cfg(rcc_lse)] cs: critical_section::CriticalSection<'_>,
    observed: &Entry,
    #[cfg(any(rcc_lse, all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk)))]
    targets: &Targets,
    admitted: &Admitted,
    ccs_unchanged: &impl Fn(bool) -> bool,
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))] expected: &mut Expected,
) -> Result<(), Error> {
    let r = observed.r;
    let old_sources = observed.old_sources;
    let trim = observed.trim;
    let needs_trim = observed.needs_trim;
    let live_lsi = observed.live_lsi;
    let monitor_hse = observed.monitor_hse;
    let monitor_lse = observed.monitor_lse;
    let needs_lsi = admitted.needs_lsi;
    #[cfg(rcc_lse)]
    let first_lsi_request = admitted.first_lsi_request;
    if needs_trim || needs_lsi {
        #[cfg(rcc_lse)]
        if let Some(entry) = targets.lse {
            check_external_faults(monitor_hse, monitor_lse)?;
            entry.before_lse_start(config)?;
            if entry.lsi.stable() && !r.lsi().read().stable() {
                return Err(Error::LsiTimeout);
            }
        }
        #[cfg(rcc_lse)]
        if first_lsi_request {
            // Repeat selectors/resets/IRQ/analog ownership at the use edge;
            // use the latched entry classification even if LSI is now stable.
            targets.lse.unwrap().admit_first_lsi_request(config, cs)?;
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
        if !live_lsi {
            r.cr1().modify(|w| {
                w.set_key(0x5a5a);
                w.set_lsien(true);
            });
            poll_clock(
                config.timeout,
                Error::LsiTimeout,
                monitor_hse,
                monitor_lse,
                || r.cr1().read().lsien() && r.lsi().read().stable(),
            )?;
        }
    }
    if needs_trim {
        // Automatic switching must not select HSI while stopped for retrim.
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
            w.set_clkccs(false);
        });
        #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
        if let Some(entry) = targets.lsi {
            // Only temporary clock-security disable may acknowledge here.
            let prior_0 = expected.sources.clkccs();
            let target_0 = false;
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
                let observed_0 = current_sources.clkccs();
                if observed_0 != target_0 && (observed_0 != prior_0 || acknowledged_0) {
                    return Err(Error::ClockConfigurationTimeout);
                }
                acknowledged_0 |= observed_0 == target_0;
                current_sources.set_clkccs(prior_0);
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
                let current_wait = current_wait?;
                if current_wait != expected.wait {
                    return Err(Error::FlashLatencyTimeout);
                }
                if acknowledged_0 {
                    break;
                }
                if attempt + 1 == config.timeout {
                    return Err(Error::ClockConfigurationTimeout);
                }
                core::hint::spin_loop();
            }
            expected.sources.set_clkccs(target_0);
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
                || !r.cr1().read().clkccs(),
                config.timeout,
                Error::ClockConfigurationTimeout,
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
            w.set_sysclk(ClockSource::Lsi);
        });
        #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
        if let Some(entry) = targets.lsi {
            // The already-live LSI bridge changes only the system selector.
            let prior_0 = expected.clock.sysclk();
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
                    return Err(Error::TemporaryClockSwitchTimeout);
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
                Error::TemporaryClockSwitchTimeout,
                monitor_hse,
                monitor_lse,
                || r.cr0().read().sysclk() == ClockSource::Lsi,
            )?;
        }
        barrier();
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
            w.set_hsien(false);
        });
        #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
        if let Some(entry) = targets.lsi {
            // HSI request and readiness may only fall while selected live LSI stays strict.
            let prior_0 = expected.sources.hsien();
            let target_0 = false;
            let mut acknowledged_0 = prior_0 == target_0;
            let prior_1 = expected.hsi_stable;
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
                    return Err(Error::HsiStopTimeout);
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
                Error::HsiStopTimeout,
                monitor_hse,
                monitor_lse,
                || !r.cr1().read().hsien() && !r.hsi().read().stable(),
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
        r.hsi().modify(|w| {
            w.set_trim(trim);
            w.set_div(config.hsi.div as u8);
        });
        #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
        if let Some(entry) = targets.lsi {
            // Factory TRIM and requested DIV acknowledge independently only while HSI is stopped.
            let prior_0 = expected.hsi.trim();
            let target_0 = trim;
            let mut acknowledged_0 = prior_0 == target_0;
            let prior_1 = expected.hsi.div();
            let target_1 = config.hsi.div as u8;
            let mut acknowledged_1 = prior_1 == target_1;
            for attempt in 0..config.timeout {
                let lsi_stable = entry.retained(config)?;
                let mut current_clock = r.cr0().read();
                let mut current_sources = r.cr1().read();
                let mut current_hsi = r.hsi().read();
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
                let observed_0 = current_hsi.trim();
                if observed_0 != target_0 && (observed_0 != prior_0 || acknowledged_0) {
                    return Err(Error::ClockConfigurationTimeout);
                }
                acknowledged_0 |= observed_0 == target_0;
                current_hsi.set_trim(prior_0);
                let observed_1 = current_hsi.div();
                if observed_1 != target_1 && (observed_1 != prior_1 || acknowledged_1) {
                    return Err(Error::ClockConfigurationTimeout);
                }
                acknowledged_1 |= observed_1 == target_1;
                current_hsi.set_div(prior_1);
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
            expected.hsi.set_trim(target_0);
            expected.hsi.set_div(target_1);
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
                    let v = r.hsi().read();
                    v.trim() == trim && v.div() == config.hsi.div as u8
                },
                config.timeout,
                Error::ClockConfigurationTimeout,
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
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_hsien(true);
        });
        #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
        if let Some(entry) = targets.lsi {
            // Restarted HSI retains the exact acknowledged factory parameters.
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
            // Return from live LSI only to the verified calibrated HSI.
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
            w.set_clkccs(old_sources.clkccs());
        });
        #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
        if let Some(entry) = targets.lsi {
            // Restore only the original clock-security policy; LSIEN remains asserted.
            let prior_0 = expected.sources.clkccs();
            let target_0 = old_sources.clkccs();
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
                let observed_0 = current_sources.clkccs();
                if observed_0 != target_0 && (observed_0 != prior_0 || acknowledged_0) {
                    return Err(Error::ClockConfigurationTimeout);
                }
                acknowledged_0 |= observed_0 == target_0;
                current_sources.set_clkccs(prior_0);
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
                let current_wait = current_wait?;
                if current_wait != expected.wait {
                    return Err(Error::FlashLatencyTimeout);
                }
                if acknowledged_0 {
                    break;
                }
                if attempt + 1 == config.timeout {
                    return Err(Error::ClockConfigurationTimeout);
                }
                core::hint::spin_loop();
            }
            expected.sources.set_clkccs(target_0);
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
    } else {
        // Both own §4.5.2 allow live divider changes, without changing TRIM.
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
        r.hsi().modify(|w| w.set_div(config.hsi.div as u8));
        #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
        if let Some(entry) = targets.lsi {
            // Legal live HSI DIV-only update preserves factory TRIM and asserted readiness.
            let prior_0 = expected.hsi.div();
            let target_0 = config.hsi.div as u8;
            let mut acknowledged_0 = prior_0 == target_0;
            for attempt in 0..config.timeout {
                let lsi_stable = entry.retained(config)?;
                let mut current_clock = r.cr0().read();
                let mut current_sources = r.cr1().read();
                let mut current_hsi = r.hsi().read();
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
                let observed_0 = current_hsi.div();
                if observed_0 != target_0 && (observed_0 != prior_0 || acknowledged_0) {
                    return Err(Error::ClockConfigurationTimeout);
                }
                acknowledged_0 |= observed_0 == target_0;
                current_hsi.set_div(prior_0);
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
                let current_wait = current_wait?;
                if current_wait != expected.wait {
                    return Err(Error::FlashLatencyTimeout);
                }
                if acknowledged_0 {
                    break;
                }
                if attempt + 1 == config.timeout {
                    return Err(Error::ClockConfigurationTimeout);
                }
                core::hint::spin_loop();
            }
            expected.hsi.set_div(target_0);
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
    if !live_lsi {
        poll_clock(
            config.timeout,
            Error::ClockConfigurationTimeout,
            monitor_hse,
            monitor_lse,
            || {
                let v = r.hsi().read();
                v.trim() == trim
                    && v.div() == config.hsi.div as u8
                    && v.stable()
                    && r.cr1().read().hsien()
                    && ccs_unchanged(false)
                    && r.cr0().read().sysclk() == ClockSource::Hsi
            },
        )?;
    }
    Ok(())
}
