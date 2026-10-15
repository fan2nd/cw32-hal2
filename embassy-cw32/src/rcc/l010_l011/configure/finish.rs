use super::*;

pub(super) fn finish(
    config: Config,
    cs: critical_section::CriticalSection<'_>,
    mut clocks: Clocks,
    observed: &Entry,
    #[cfg(any(rcc_lse, all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk)))]
    targets: &Targets,
    admitted: &Admitted,
    ccs_unchanged: &impl Fn(bool) -> bool,
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))] expected: &Expected,
) -> Result<Clocks, Error> {
    let r = observed.r;
    let old_sources = observed.old_sources;
    let old_hse = observed.old_hse;
    let old_lsi = observed.old_lsi;
    let old_lse = observed.old_lse;
    let trim = observed.trim;
    let live_lsi = observed.live_lsi;
    let monitor_hse = observed.monitor_hse;
    let monitor_lse = observed.monitor_lse;
    let needs_lsi = admitted.needs_lsi;
    #[cfg(rcc_lse)]
    let lse_admission = admitted.lse_admission;
    // Restore only the software request; automatic LSI consumers stay active.
    // Never require STABLE to clear when LSIEN is cleared.
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
            w.set_lsien(old_sources.lsien());
        });
        poll(
            || r.cr1().read().lsien() == old_sources.lsien(),
            config.timeout,
            Error::TemporaryClockRestoreTimeout,
        )?;
    }
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = targets.lsi {
        return finish_lsi_sysclk(config, clocks, entry, trim, cs);
    }
    #[cfg(rcc_lse)]
    if let Some(entry) = targets.lse {
        entry.before_lse_start(config)?;
    }
    #[cfg(rcc_lse)]
    if let (Some(lse), Some(admission)) = (config.lse, lse_admission) {
        let admission = if targets.lse.is_none() {
            admission.after_owned_flash_wait(MAX_FLASH_WAIT)?
        } else {
            admission
        };
        crate::rcc::lse::start(lse, admission, cs, false)?;
    }
    #[cfg(rcc_lse)]
    if let Some(entry) = targets.lse {
        return finish_lse_sysclk(config, clocks, entry, trim, needs_lsi, cs);
    }
    let sysclk = match config.sys {
        Sysclk::HSI => ClockSource::Hsi,
        Sysclk::HSE => ClockSource::Hse,
        #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
        Sysclk::LSI => unreachable!("live LSI target completed above"),
        #[cfg(rcc_lse)]
        Sysclk::LSE => unreachable!("LSE target completed above"),
    };
    // Keep guarded buses until the target source has acknowledged selection.
    r.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_sysclk(sysclk);
    });
    poll_clock(
        config.timeout,
        Error::ClockSwitchTimeout,
        monitor_hse,
        monitor_lse,
        || r.cr0().read().sysclk() == sysclk,
    )?;
    barrier();
    r.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_hclkprs(config.ahb_pre as u8);
        w.set_pclkprs(config.apb_pre as u8);
    });
    poll_clock(
        config.timeout,
        Error::ClockConfigurationTimeout,
        monitor_hse,
        monitor_lse,
        || {
            let v = r.cr0().read();
            v.sysclk() == sysclk
                && v.hclkprs() == config.ahb_pre as u8
                && v.pclkprs() == config.apb_pre as u8
        },
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
                .divided_by(config.ahb_pre.divisor())
                .maximum()
                .0,
        );
    set_flash_latency(
        (upper_hclk - 1) / crate::RCC_FLASH_WAIT_STEP_HZ,
        config.timeout,
    )?;
    check_external_faults(monitor_hse, monitor_lse)?;
    let final_clock = r.cr0().read();
    let final_sources = r.cr1().read();
    let final_hsi = r.hsi().read();
    let final_hse = r.hse().read();
    let final_lsi = r.lsi().read();
    let lse_preserved = r.lse().read().0 == old_lse.0;
    #[cfg(rcc_lse)]
    let lse_preserved = if config.lse.is_some() {
        crate::rcc::lse::preserved_register(old_lse.0)
    } else {
        lse_preserved
    };
    if final_clock.sysclk() != sysclk
        || !ccs_unchanged(true)
        || final_clock.hclkprs() != config.ahb_pre as u8
        || final_clock.pclkprs() != config.apb_pre as u8
        || !final_sources.hsien()
        || !final_hsi.stable()
        || final_hsi.trim() != trim
        || final_hsi.div() != config.hsi.div as u8
        || final_sources.hseen() != (config.hse.is_some() || old_sources.hseen())
        || final_sources.lsien() != old_sources.lsien()
        || final_lsi.trim() != old_lsi.trim()
        || final_lsi.waitcycle() != old_lsi.waitcycle()
        || ((needs_lsi || old_lsi.stable()) && !final_lsi.stable())
        || !lse_preserved
        || final_hse.hexenpol() != old_hse.hexenpol()
    {
        return Err(Error::ClockConfigurationTimeout);
    }
    if let Some(hse) = config.hse {
        if !final_hse.stable()
            || !hse_parameters_match(hse)?
            || !crate::rcc_hse_pins_match(
                hse.mode == HseMode::Bypass,
                config.timeout,
                cs,
                #[cfg(all(rcc_cw32l010_v1, rcc_lsi_sysclk))]
                false,
            )?
        {
            return Err(Error::ClockConfigurationTimeout);
        }
    } else if final_hse.0 != old_hse.0 {
        return Err(Error::ClockConfigurationTimeout);
    }
    // The inherited enabled crystal reserves both pads for the whole boot,
    // including after an unowned oscillator is deliberately changed to bypass.
    if old_sources.hseen() {
        if !old_hse.mode() {
            clocks.hse = Some(HseMode::Oscillator);
        } else if clocks.hse.is_none() {
            clocks.hse = Some(HseMode::Bypass);
        }
    }
    #[cfg(rcc_lse)]
    if let Some(lse) = config.lse {
        crate::rcc::lse::verify(lse, cs, false)?;
    }
    // Pad inspection may have opened/restored a gate; recheck fault/mux at freeze.
    check_external_faults(monitor_hse, monitor_lse)?;
    if r.cr0().read().sysclk() != sysclk {
        return Err(Error::ClockSwitchTimeout);
    }
    Ok(clocks)
}
