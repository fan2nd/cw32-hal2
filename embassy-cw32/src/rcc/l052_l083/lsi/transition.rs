use super::*;

#[cfg(rcc_lsi_sysclk)]
pub(in crate::rcc::l052_l083) fn configure_lsi(
    config: Config,
    cs: critical_section::CriticalSection<'_>,
) -> Result<Clocks, Error> {
    // P0/P1: pure admission, immutable capture, then two complete equal passes.
    let mut clocks = config.frequencies()?;
    let mut state = LsiSysclkState::capture(config)?;
    let r = pac::SYSCTRL;
    let (consumers, pads) = state.collect(LsiTransition::Steady, cs)?;
    state.consumers = Some(consumers);
    state.pads = Some(pads);
    state.check_all(LsiTransition::Steady, cs)?;
    state.external_owners(config)?;
    #[cfg(rcc_cw32l083_v1)]
    state.admit_l083(config, clocks)?;
    let needs_hsi_trim = state.entry.hsi.trim() != state.factory_hsi;
    if needs_hsi_trim {
        state.hsi_owners(cs)?;
    }
    if let Some(lse) = config.lse {
        if state.entry.sources.lseen() {
            let target = state.lse_parameters(lse);
            if !state.entry.sources.lseccs()
                || target.0 & crate::RCC_LSI_LSE_PARAMETERS_MASK
                    != state.lse.0 & crate::RCC_LSI_LSE_PARAMETERS_MASK
                || !pads.lse_matches(lse.mode == crate::rcc::LseMode::Bypass, false)
            {
                return Err(Error::LseClockInUse);
            }
            state.check_all(LsiTransition::Steady, cs)?;
        } else {
            state.cold_lse_preflight(lse, true, cs)?;
        }
    }

    // P2: central Flash gate, WAIT2 and monotonic divider-only guard. FLASH is
    // not in the twelve saved consumer-gate slots; GPIO ownership is unchanged.
    state.check_all(LsiTransition::Steady, cs)?;
    let flash = <crate::peripherals::FLASH as crate::rcc::SealedRccPeripheral>::RCC_INFO;
    flash
        .enable_with_cs_readback(
            cs,
            crate::rcc::Readback::Poll {
                attempts: config.timeout,
                spin: true,
            },
        )
        .map_err(|_| Error::FlashClockTimeout)?;
    #[cfg(rcc_cw32l083_v1)]
    {
        state.flash_gate = true;
    }
    state.check_all(LsiTransition::Steady, cs)?;
    if !flash.is_enabled() || flash.reset_asserted() {
        return Err(Error::FlashClockTimeout);
    }
    #[cfg(rcc_cw32l083_v1)]
    {
        let mut local = pac::FLASH.cr2().read();
        local.0 &= !crate::RCC_L083_LSI_FLASH_CR2_KEY_MASK;
        if local.wait() != state.entry.cr2.flashwait() {
            return Err(Error::FlashLatencyTimeout);
        }
        state.flash_cr2 = Some(local);
        state.check_all(LsiTransition::Steady, cs)?;
    }
    state.flash_latency(crate::RCC_INITIAL_FLASH_WAIT, cs)?;
    let mut guarded = state.clock;
    guarded.set_hclkprs(state.entry.clock.hclkprs().max(AHBPrescaler::Div4 as u8));
    guarded.set_pclkprs(state.entry.clock.pclkprs().max(APBPrescaler::Div8 as u8));
    #[cfg(rcc_cw32l083_v1)]
    if state.entry.sources.clkccs()
        && matches!(
            state.entry.clock.sysclk(),
            ClockSource::Hse | ClockSource::Lse
        )
    {
        // Admission proved unchanged factory HSI at the entry divisors and
        // raw fallback envelope. Never replay the external selector.
        guarded.set_sysclk(ClockSource::Hsi);
    }
    state.check_all(LsiTransition::Steady, cs)?;
    r.cr0().modify(|w| {
        w.set_key(0x5a5a);
        #[cfg(rcc_cw32l083_v1)]
        w.set_sysclk(guarded.sysclk());
        w.set_hclkprs(guarded.hclkprs());
        w.set_pclkprs(guarded.pclkprs());
    });
    state.wait(
        LsiTransition::Clock(guarded),
        config.timeout,
        Error::ClockConfigurationTimeout,
        cs,
    )?;
    barrier();

    #[cfg(rcc_cw32l083_v1)]
    state.stop_l083_pll(config, cs)?;

    // P3: even matching cold TRIM takes the final stopped use-edge proof.
    if state.class == LsiEntryClass::Cold {
        state.check_all(LsiTransition::Steady, cs)?;
        let mut target = state.lsi;
        target.set_trim(state.factory_lsi);
        if state.lsi.trim() != state.factory_lsi {
            r.lsi().modify(|w| w.set_trim(state.factory_lsi));
            state.wait(
                LsiTransition::LsiTrim(target),
                config.timeout,
                Error::LsiConfigurationTimeout,
                cs,
            )?;
        }
    }
    // Distinct request edge after optional TRIM acknowledgment. No observer,
    // consumer or parameter is reclassified from a later hardware sample.
    state.check_all(LsiTransition::Steady, cs)?;

    // P4: the first owned source request changes LSIEN alone and is permanent.
    let mut requested = state.sources;
    requested.set_lsien(true);
    state.requested = true;
    #[cfg(rcc_cw32l083_v1)]
    {
        state.event_owned[3] = true;
    }
    r.cr1().modify(|w| {
        w.set_key(0x5a5a);
        w.set_lsien(true);
    });
    state.wait(
        LsiTransition::LsiRequest(requested),
        config.timeout,
        Error::LsiConfigurationTimeout,
        cs,
    )?;
    state.wait(
        LsiTransition::LsiReady,
        config.timeout,
        Error::LsiTimeout,
        cs,
    )?;
    state.check_all(LsiTransition::Steady, cs)?;

    // P5: directly select established factory LSI before stopping a mismatching
    // HSI. No inherited, unqualified HSI trim is enabled or selected as a bridge.
    if needs_hsi_trim || cfg!(rcc_cw32l083_v1) {
        if needs_hsi_trim {
            state.hsi_owners(cs)?;
        }
        let mut target_clock = state.clock;
        target_clock.set_sysclk(ClockSource::Lsi);
        r.cr0().modify(|w| {
            w.set_key(0x5a5a);
            w.set_sysclk(ClockSource::Lsi);
        });
        state.wait(
            LsiTransition::Clock(target_clock),
            config.timeout,
            Error::TemporaryClockSwitchTimeout,
            cs,
        )?;
        barrier();
    }
    if needs_hsi_trim {
        // Repeat frozen HSI-owner equality at the actual stop use edge.
        state.hsi_owners(cs)?;
        if state.sources.hsien() || cfg!(rcc_cw32l052_v1) {
            let mut stopped = state.sources;
            stopped.set_hsien(false);
            r.cr1().modify(|w| {
                w.set_key(0x5a5a);
                w.set_hsien(false);
            });
            state.wait(
                LsiTransition::HsiStopRequest(stopped),
                config.timeout,
                Error::HsiStopTimeout,
                cs,
            )?;
            // HSIEN can acknowledge before either STABLE latch falls. This is its
            // own bounded stop phase, with LSI and every non-owned bit still fixed.
            state.wait(
                LsiTransition::HsiStopped,
                config.timeout,
                Error::HsiStopTimeout,
                cs,
            )?;
        }
        state.check_all(LsiTransition::Steady, cs)?;
        let mut calibrated = state.hsi;
        calibrated.set_trim(state.factory_hsi);
        r.hsi().modify(|w| w.set_trim(state.factory_hsi));
        state.wait(
            LsiTransition::HsiTrim(calibrated),
            config.timeout,
            Error::ClockConfigurationTimeout,
            cs,
        )?;
    } else {
        // Matching trim skips only stop/trim/restart; owner configuration must
        // still equal the immutable complete snapshot.
        state.check_all(LsiTransition::Steady, cs)?;
    }
    if !state.sources.hsien() {
        let mut enabled = state.sources;
        enabled.set_hsien(true);
        state.check_all(LsiTransition::Steady, cs)?;
        #[cfg(rcc_cw32l083_v1)]
        {
            state.event_owned[0] = true;
        }
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_hsien(true);
        });
        state.wait(
            LsiTransition::HsiRequest(enabled),
            config.timeout,
            Error::ClockConfigurationTimeout,
            cs,
        )?;
        state.wait(
            LsiTransition::HsiReady,
            config.timeout,
            Error::HsiTimeout,
            cs,
        )?;
    }
    state.check_all(LsiTransition::Steady, cs)?;
    #[cfg(rcc_cw32l052_v1)]
    {
        let mut hsi_clock = state.clock;
        hsi_clock.set_sysclk(ClockSource::Hsi);
        r.cr0().modify(|w| {
            w.set_key(0x5a5a);
            w.set_sysclk(ClockSource::Hsi);
        });
        state.wait(
            LsiTransition::Clock(hsi_clock),
            config.timeout,
            Error::ClockSwitchTimeout,
            cs,
        )?;
        barrier();
    }
    let mut divided = state.hsi;
    divided.set_div(config.hsi.div as u8);
    state.check_all(LsiTransition::Steady, cs)?;
    r.hsi().modify(|w| w.set_div(config.hsi.div as u8));
    state.wait(
        LsiTransition::HsiDivider(divided),
        config.timeout,
        Error::ClockConfigurationTimeout,
        cs,
    )?;

    #[cfg(rcc_cw32l083_v1)]
    {
        // Native L083 changes DIV while still on qualified LSI, with PLL
        // permanently stopped, then proves the actual factory-HSI use edge.
        state.check_all(LsiTransition::Steady, cs)?;
        LsiSysclkState::validate_edge(
            config,
            clocks,
            crate::rcc::ClockBounds::hsi(config.hsi.div.divisor()),
            state.clock,
            2,
        )?;
        let mut hsi_clock = state.clock;
        hsi_clock.set_sysclk(ClockSource::Hsi);
        r.cr0().modify(|w| {
            w.set_key(0x5a5a);
            w.set_sysclk(ClockSource::Hsi);
        });
        state.wait(
            LsiTransition::Clock(hsi_clock),
            config.timeout,
            Error::ClockSwitchTimeout,
            cs,
        )?;
        barrier();
    }

    // P6: an enabled inherited HSE is preserved, never stopped for reuse.
    if let Some(hse) = config.hse {
        state.check_all(LsiTransition::Steady, cs)?;
        state.external_owners(config)?;
        if !state.entry.sources.hseen() {
            let bypass = hse.mode == HseMode::Bypass;
            let old_pads = state.pads.ok_or(Error::LsiClockInUse)?;
            let old_consumers = state.consumers.ok_or(Error::LsiClockInUse)?;
            // The exact projected GPIOF gate delta is verified before any pad
            // programming. No fresh peripheral sample supplies expectations.
            rcc_lsi_enable_hse_pins(config.timeout, cs, state.monitor_hse, state.monitor_lse)?;
            state.pads = Some(old_pads.hse_gate_enabled());
            state.consumers = Some(old_consumers.hse_gate_enabled());
            state.lse_preflight = state
                .lse_preflight
                .map(|snapshot| snapshot.hse_gate_enabled());
            state.check_all(LsiTransition::Steady, cs)?;
            #[cfg(rcc_cw32l052_v1)]
            {
                rcc_lsi_configure_hse_pins(
                    bypass,
                    config.timeout,
                    cs,
                    state.monitor_hse,
                    state.monitor_lse,
                )?;
                state.pads = state.pads.map(|saved| saved.hse_configured(bypass));
                state.consumers = state.consumers.map(|saved| saved.hse_configured(bypass));
                state.lse_preflight = state
                    .lse_preflight
                    .map(|saved| saved.hse_configured(bypass));
                state.check_all(LsiTransition::Steady, cs)?;
            }
            #[cfg(rcc_cw32l083_v1)]
            for step in 0..crate::RCC_L083_LSI_HSE_PIN_STEPS * if bypass { 1 } else { 2 } {
                state.check_all(LsiTransition::Steady, cs)?;
                crate::rcc_l083_lsi_configure_hse_pin_step(
                    bypass,
                    step,
                    config.timeout,
                    cs,
                    state.monitor_hse,
                    state.monitor_lse,
                )?;
                state.wait(
                    LsiTransition::HsePad(bypass, step),
                    config.timeout,
                    Error::HsePinConfigurationTimeout,
                    cs,
                )?;
            }
            let target = state.hse_parameters(hse)?;
            r.hse().modify(|w| {
                w.set_mode(target.mode());
                w.set_driver(target.driver());
                #[cfg(rcc_cw32l052_v1)]
                w.set_pdriver(target.pdriver());
                w.set_freqrange(target.freqrange());
                #[cfg(rcc_cw32l052_v1)]
                w.set_pfreqrange(target.pfreqrange());
                w.set_waitcycle(target.waitcycle());
                w.set_flt(target.flt());
                w.set_detcnt(target.detcnt());
            });
            state.wait(
                LsiTransition::HseParameters(target),
                config.timeout,
                Error::ClockConfigurationTimeout,
                cs,
            )?;
            state.check_all(LsiTransition::Steady, cs)?;
            let mut enabled = state.sources;
            enabled.set_hseen(true);
            #[cfg(rcc_cw32l083_v1)]
            {
                enabled.set_hseccs(true);
                state.event_owned[1] = true;
            }
            r.cr1().modify(|w| {
                w.set_key(0x5a5a);
                w.set_hseen(true);
                #[cfg(rcc_cw32l083_v1)]
                w.set_hseccs(true);
            });
            state.wait(
                LsiTransition::HseRequest(enabled),
                config.timeout,
                Error::ClockConfigurationTimeout,
                cs,
            )?;
            state.wait(
                LsiTransition::HseReady,
                config.timeout,
                Error::HseTimeout,
                cs,
            )?;
        }
    }

    // P7: optional LSE completes while guarded factory HSI executes. Its strict
    // generated collector retains the old reset-like admission without the old
    // Boolean helpers' loss of restoration/fault error priority.
    if let Some(lse) = config.lse {
        if state.entry.sources.lseen() {
            state.check_all(LsiTransition::Steady, cs)?;
            crate::rcc::lse::freeze_sysclk_monitor(cs)?;
            state.check_all(LsiTransition::Steady, cs)?;
        } else {
            state.cold_lse_preflight(lse, true, cs)?;
            let bypass = lse.mode == crate::rcc::LseMode::Bypass;
            rcc_lsi_configure_lse_pins(
                bypass,
                lse.poll_budget,
                cs,
                state.monitor_hse,
                state.monitor_lse,
            )?;
            state.pads = state.pads.map(|saved| saved.lse_configured(bypass));
            #[cfg(rcc_cw32l083_v1)]
            {
                state.consumers = state.consumers.map(|saved| saved.lse_configured(bypass));
            }
            state.lse_preflight = state
                .lse_preflight
                .map(|saved| saved.lse_configured(bypass));
            state.lse_pads_configured = true;
            state.check_all(LsiTransition::Steady, cs)?;
            let target = state.lse_parameters(lse);
            r.lse().modify(|w| {
                w.set_mode(target.mode());
                w.set_driver(target.driver());
                w.set_amp(target.amp());
                #[cfg(rcc_cw32l052_v1)]
                w.set_pdriver(target.pdriver());
                #[cfg(rcc_cw32l052_v1)]
                w.set_pamp(target.pamp());
                w.set_waitcycle(target.waitcycle());
            });
            state.wait(
                LsiTransition::LseParameters(target),
                lse.poll_budget,
                Error::LseNotReady,
                cs,
            )?;
            // A distinct pre-enable proof includes the known bypass pad delta
            // and all original consumer/work-gate/reset/observer identities.
            state.cold_lse_preflight(lse, false, cs)?;
            crate::rcc::lse::freeze_sysclk_monitor(cs)?;
            state.check_all(LsiTransition::Steady, cs)?;
            let mut enabled = state.sources;
            enabled.set_lseen(true);
            enabled.set_lseccs(true);
            state.lse_requested = true;
            #[cfg(rcc_cw32l083_v1)]
            {
                state.event_owned[2] = true;
            }
            r.cr1().modify(|w| {
                w.set_key(0x5a5a);
                w.set_lseen(true);
                w.set_lseccs(true);
            });
            state.wait(
                LsiTransition::LseRequest(enabled),
                lse.poll_budget,
                Error::LseNotReady,
                cs,
            )?;
            state.wait(
                LsiTransition::LseReady,
                lse.poll_budget,
                Error::LseNotReady,
                cs,
            )?;
        }
    }

    // P8: install final buses while executing the independently qualified HSI
    // tree. All requested source, detector and pad operations are now complete.
    state.verify_sources(config, cs)?;
    let mut final_hsi = state.clock;
    final_hsi.set_sysclk(ClockSource::Hsi);
    final_hsi.set_hclkprs(config.ahb_pre as u8);
    final_hsi.set_pclkprs(config.apb_pre as u8);
    r.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_sysclk(ClockSource::Hsi);
        w.set_hclkprs(config.ahb_pre as u8);
        w.set_pclkprs(config.apb_pre as u8);
    });
    state.wait(
        LsiTransition::Clock(final_hsi),
        config.timeout,
        Error::ClockConfigurationTimeout,
        cs,
    )?;
    barrier();
    state.verify_sources(config, cs)?;

    // P9: latency is finalized before the final source-only CR0 commit. Fixed
    // HSI /6 escape gets no post-fault AHB divisor credit.
    #[cfg(rcc_cw32l052_v1)]
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
    #[cfg(rcc_cw32l052_v1)]
    let final_wait = (upper_hclk - 1) / crate::RCC_FLASH_WAIT_STEP_HZ;
    #[cfg(rcc_cw32l052_v1)]
    state.flash_latency(final_wait, cs)?;
    #[cfg(rcc_cw32l083_v1)]
    let final_wait = crate::RCC_INITIAL_FLASH_WAIT;
    state.verify_sources(config, cs)?;
    let mut final_lsi = state.clock;
    final_lsi.set_sysclk(ClockSource::Lsi);
    r.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_sysclk(ClockSource::Lsi);
    });
    state.wait(
        LsiTransition::Clock(final_lsi),
        config.timeout,
        Error::ClockSwitchTimeout,
        cs,
    )?;
    barrier();
    // No subsequent source/parameter/divider/Flash/policy write is permitted.
    // Temporary inspection gates retain their precise restoration contract.
    state.verify_sources(config, cs)?;
    if !flash.is_enabled() || flash.reset_asserted() {
        return Err(Error::FlashClockTimeout);
    }
    if pac::FLASH.cr2().read().wait() != final_wait as u8 {
        return Err(Error::FlashLatencyTimeout);
    }
    state.verify_sources(config, cs)?;
    // Preserve inherited HSE pad ownership from the immutable entry mode.
    if state.entry.sources.hseen() {
        if !state.entry.hse.mode() {
            clocks.hse = Some(HseMode::Oscillator);
        } else if clocks.hse.is_none() {
            clocks.hse = Some(HseMode::Bypass);
        }
    }
    // This normalized CR0 read is deliberately the very last hardware
    // observation. Publication happens only in the existing successful caller.
    let last_clock = r.cr0().read().0 & !LSI_CR0_KEY_MASK;
    if last_clock != final_lsi.0 {
        return Err(Error::ClockSwitchTimeout);
    }
    Ok(clocks)
}

// STABLE is a startup latch, not a continuous source-valid indication. Sticky
