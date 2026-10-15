use super::*;

pub(super) fn configure(config: Config, cs: critical_section::CriticalSection<'_>) -> Result<Clocks, Error> {
    let mut clocks = config.frequencies()?;
    let r = pac::SYSCTRL;
    #[cfg(rcc_lsi_sysclk)]
    let permanent_lsi = config.sys == Sysclk::LSI;
    #[cfg(not(rcc_lsi_sysclk))]
    let permanent_lsi = false;
    // Immutable target capture precedes every old LSE helper and source write.
    #[cfg(rcc_lsi_sysclk)]
    let mut lsi_sysclk = if permanent_lsi {
        Some(LsiSysclkState::capture(config)?)
    } else {
        None
    };
    // Only the new target classifies the monitor before any parameter/request
    // change. Old auxiliary LSE retains its original matching-trim shortcut.
    #[cfg(rcc_lse)]
    let lse_sysclk = if config.sys == Sysclk::LSE {
        Some(LseSysclkState::admit()?)
    } else {
        None
    };
    #[cfg(rcc_lse)]
    let reuse_lse = config
        .lse
        .map(|c| crate::rcc::lse::preflight(c, cs))
        .transpose()?;
    #[cfg(rcc_lse)]
    if let Some(lse) = config.lse.filter(|_| !permanent_lsi) {
        prepare_lse_monitor(lse.poll_budget, cs)?;
    }
    #[cfg(rcc_lsi_sysclk)]
    let old_clock = lsi_sysclk
        .as_ref()
        .map(|state| state.entry_clock)
        .unwrap_or_else(|| r.cr0().read());
    #[cfg(not(rcc_lsi_sysclk))]
    let old_clock = r.cr0().read();
    #[cfg(rcc_lsi_sysclk)]
    let old_sources = lsi_sysclk
        .as_ref()
        .map(|state| state.entry_sources)
        .unwrap_or_else(|| r.cr1().read());
    #[cfg(not(rcc_lsi_sysclk))]
    let old_sources = r.cr1().read();
    let monitor_hse = config.hse.is_some() || old_sources.hseen();
    let monitor_lse = old_sources.lseen();
    #[cfg(rcc_lse)]
    let monitor_lse = monitor_lse || config.lse.is_some();
    #[cfg(rcc_lse)]
    let old_lsi = config.lse.map(|_| r.lsi().read());
    let needs_lsi = (monitor_hse && old_sources.hseccs()) || (monitor_lse && old_sources.lseccs());
    #[cfg(rcc_lse)]
    let needs_lsi = needs_lsi || config.lse.is_some();
    // Before LSE startup every inherited CCS bit must remain identical.
    // New-start policy changes only LSECCS at the dedicated LSE start step.
    let ccs_unchanged = || {
        let v = r.cr1().read();
        v.clkccs() == old_sources.clkccs()
            && v.hseccs() == old_sources.hseccs()
            && v.lseccs() == old_sources.lseccs()
    };
    if !matches!(
        old_clock.sysclk(),
        ClockSource::Hsi | ClockSource::Hse | ClockSource::Lsi | ClockSource::Lse
    ) {
        return Err(Error::InvalidClockSource);
    }
    check_external_faults(monitor_hse, monitor_lse)?;
    #[cfg(rcc_lsi_sysclk)]
    let old_hsi = lsi_sysclk
        .as_ref()
        .map(|state| state.entry_hsi)
        .unwrap_or_else(|| r.hsi().read());
    #[cfg(not(rcc_lsi_sysclk))]
    let old_hsi = r.hsi().read();
    if !matches!(old_hsi.div(), 5 | 6 | 8 | 9 | 11..=15) {
        return Err(Error::InvalidHsiDivider);
    }
    let read_hsi_trim = || {
        let factory =
            unsafe { core::ptr::read_volatile(crate::RCC_FACTORY_HSI_TRIM_ADDRESS as *const u16) };
        if factory == u16::MAX {
            return Err(Error::InvalidCalibration);
        }
        let mut calibrated = pac::sysctrl::regs::Hsi::default();
        calibrated.set_trim(factory);
        Ok(calibrated.trim())
    };
    #[cfg(rcc_lsi_sysclk)]
    let trim = match lsi_sysclk.as_ref() {
        Some(state) => state.hsi_trim,
        None => read_hsi_trim()?,
    };
    #[cfg(not(rcc_lsi_sysclk))]
    let trim = read_hsi_trim()?;
    let needs_trim = old_hsi.trim() != trim;
    // APB gates control register access, not the independent AWT/RTC source.
    // Inspect with their original gates restored; never reset retained owners.
    let awt = if needs_trim || config.hse.is_some() || permanent_lsi {
        #[cfg(rcc_lsi_sysclk)]
        let target_awt = lsi_sysclk
            .as_ref()
            .map(|state| state.inspect_awt(config.timeout, cs))
            .transpose()?;
        #[cfg(not(rcc_lsi_sysclk))]
        let target_awt: Option<pac::awt::regs::Cr> = None;
        Some(match target_awt {
            Some(value) => value,
            None => <crate::peripherals::AWT as crate::rcc::SealedRccPeripheral>::RCC_INFO
                .inspect_for_init(cs, config.timeout, || pac::AWT.cr().read())
                .map_err(|_| Error::RetainedClockInspectionTimeout)?,
        })
    } else {
        None
    };
    #[cfg(rcc_lsi_sysclk)]
    if let Some(state) = lsi_sysclk.as_mut() {
        state.awt = awt;
        state.check_identity()?;
    }
    if needs_trim && awt.is_some_and(|v| v.en() && v.src() == pac::awt::vals::Source::Hsiosc) {
        return Err(Error::HsiClockInUse);
    }
    if needs_trim
        && pac::LVD.cr0().read().en()
        && pac::LVD.cr1().read().flten()
        && pac::LVD.cr1().read().fltclk()
    {
        return Err(Error::HsiClockInUse);
    }
    let mut preserve_hse = false;
    if let Some(hse) = config.hse {
        // AWT ETR can route to PF0. Without a proven nonconflicting route,
        // reject the request before changing any oscillator or pad.
        if awt.is_some_and(|v| v.en() && v.src() == pac::awt::vals::Source::Etr) {
            return Err(Error::HsePinInUse);
        }
        let rtc_source = <crate::peripherals::RTC as crate::rcc::SealedRccPeripheral>::RCC_INFO
            .inspect_for_init(cs, config.timeout, || pac::RTC.cr1().read().source())
            .map_err(|_| Error::RetainedClockInspectionTimeout)?;
        let rtc_hse = matches!(
            rtc_source,
            pac::rtc::vals::Source::HseDiv128
                | pac::rtc::vals::Source::HseDiv256
                | pac::rtc::vals::Source::HseDiv512
                | pac::rtc::vals::Source::HseDiv1024
        );
        preserve_hse =
            rtc_hse || awt.is_some_and(|v| v.en() && v.src() == pac::awt::vals::Source::Hse);
        if preserve_hse
            && (!r.cr1().read().hseen()
                || !r.hse().read().stable()
                || !hse_parameters_match(hse)?
                || !crate::rcc_hse_pins_match(hse.mode == HseMode::Bypass, config.timeout, cs)?)
        {
            return Err(Error::HseClockInUse);
        }
    }

    // Establish conservative Flash and monotonic bus guards before increasing
    // any source. Preserve an inherited divider already stronger than /4,/8.
    <crate::peripherals::FLASH as crate::rcc::SealedRccPeripheral>::RCC_INFO
        .enable_with_cs_readback(
            cs,
            crate::rcc::Readback::Poll {
                attempts: config.timeout,
                spin: true,
            },
        )
        .map_err(|_| Error::FlashClockTimeout)?;
    set_flash_latency(crate::RCC_INITIAL_FLASH_WAIT, config.timeout)?;
    let guard_hclk = old_clock.hclkprs().max(AHBPrescaler::Div4 as u8);
    let guard_pclk = old_clock.pclkprs().max(APBPrescaler::Div8 as u8);
    r.cr0().modify(|w| {
        w.set_key(0x5a5a);
        if permanent_lsi {
            w.set_sysclk(old_clock.sysclk());
        }
        w.set_hclkprs(guard_hclk);
        w.set_pclkprs(guard_pclk);
    });
    wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
        let v = r.cr0().read();
        v.hclkprs() == guard_hclk && v.pclkprs() == guard_pclk
    })?;
    barrier();
    #[cfg(rcc_lsi_sysclk)]
    if let Some(state) = lsi_sysclk.as_mut() {
        let mut expected = state.clock;
        expected.set_hclkprs(guard_hclk);
        expected.set_pclkprs(guard_pclk);
        wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
            let mut actual = r.cr0().read();
            actual.set_key(0);
            actual.0 == expected.0
        })?;
        state.clock = expected;
        state.prepare(config.timeout, cs)?;
    }

    // The first source write for an LSI target permanently requests LSI and HSI
    // together, before escaping even an incoming selected LSI with LSIEN=0.
    r.cr1().modify(|w| {
        w.set_key(0x5a5a);
        w.set_hsien(true);
        if permanent_lsi {
            w.set_lsien(true);
        }
    });
    wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
        let v = r.cr1().read();
        v.hsien() && ccs_unchanged()
    })?;
    #[cfg(rcc_lsi_sysclk)]
    if let Some(state) = lsi_sysclk.as_mut() {
        let mut expected = state.sources;
        expected.set_hsien(true);
        expected.set_lsien(true);
        wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
            let mut actual = r.cr1().read();
            actual.set_key(0);
            actual.0 == expected.0
        })?;
        state.sources = expected;
        state.requested = true;
        state.wait_ready(config.timeout)?;
        state.check_target(config.timeout, cs)?;
    }
    wait_until(config.timeout, Error::HsiTimeout, || {
        r.hsi().read().stable()
    })?;
    #[cfg(rcc_lsi_sysclk)]
    if let Some(state) = lsi_sysclk.as_ref() {
        state.check_identity()?;
        if !r.isr().read().hsistable() {
            return Err(Error::HsiTimeout);
        }
    }
    // Leave the legal incoming source via unchanged HSI before any retuning.
    r.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_sysclk(ClockSource::Hsi);
    });
    wait_until(config.timeout, Error::ClockSwitchTimeout, || {
        r.cr0().read().sysclk() == ClockSource::Hsi
    })?;
    barrier();
    #[cfg(rcc_lsi_sysclk)]
    if let Some(state) = lsi_sysclk.as_mut() {
        let mut expected = state.clock;
        expected.set_sysclk(ClockSource::Hsi);
        wait_until(config.timeout, Error::ClockSwitchTimeout, || {
            let mut actual = r.cr0().read();
            actual.set_key(0);
            actual.0 == expected.0
        })?;
        state.clock = expected;
        state.check_identity()?;
    }
    let lsi_was_enabled = r.cr1().read().lsien();
    // A retained enabled external detector requires unchanged legal LSI,
    // even without a trim bridge. CLKCCS alone does not enable a detector.
    if (needs_trim || needs_lsi) && !permanent_lsi {
        #[cfg(rcc_lse)]
        if let Some(state) = lse_sysclk {
            // Gate inspection and the HSI escape may have waited. Reclassify
            // the current request/readiness before enabling this reference.
            state.admit_request()?;
        }
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_lsien(true);
        });
        wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
            let v = r.cr1().read();
            v.lsien() && ccs_unchanged()
        })?;
        wait_until(config.timeout, Error::LsiTimeout, || {
            r.lsi().read().stable()
        })?;
        #[cfg(rcc_lse)]
        if let Some(state) = lse_sysclk {
            // A mixed transitional poll is not readiness. No HSI stop/trim or
            // LSE detector start is admitted until the complete predicate holds.
            wait_until(config.timeout, Error::LsiTimeout, || {
                state.monitor_ready(false)
            })?;
        }
    }
    if needs_trim {
        #[cfg(rcc_lsi_sysclk)]
        if let Some(state) = lsi_sysclk.as_ref() {
            state.check_target(config.timeout, cs)?;
            state.check_hsi_owners(config.timeout, cs)?;
        }
        r.cr0().modify(|w| {
            w.set_key(0x5a5a);
            w.set_sysclk(ClockSource::Lsi);
        });
        wait_until(config.timeout, Error::TemporaryClockSwitchTimeout, || {
            r.cr0().read().sysclk() == ClockSource::Lsi
        })?;
        barrier();
        #[cfg(rcc_lsi_sysclk)]
        if let Some(state) = lsi_sysclk.as_mut() {
            let mut expected = state.clock;
            expected.set_sysclk(ClockSource::Lsi);
            wait_until(config.timeout, Error::TemporaryClockSwitchTimeout, || {
                let mut actual = r.cr0().read();
                actual.set_key(0);
                actual.0 == expected.0
            })?;
            state.clock = expected;
            state.check_identity()?;
        }
        #[cfg(rcc_lsi_sysclk)]
        if let Some(state) = lsi_sysclk.as_ref() {
            state.check_target(config.timeout, cs)?;
            state.check_hsi_owners(config.timeout, cs)?;
        }
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_hsien(false);
            if permanent_lsi {
                w.set_lsien(true);
            }
        });
        wait_until(config.timeout, Error::HsiStopTimeout, || {
            !r.cr1().read().hsien()
        })?;
        wait_until(config.timeout, Error::HsiStopTimeout, || {
            !r.hsi().read().stable()
        })?;
        #[cfg(rcc_lsi_sysclk)]
        if let Some(state) = lsi_sysclk.as_mut() {
            let mut expected = state.sources;
            expected.set_hsien(false);
            wait_until(config.timeout, Error::HsiStopTimeout, || {
                let mut actual = r.cr1().read();
                actual.set_key(0);
                actual.0 == expected.0
            })?;
            state.sources = expected;
            state.check_identity()?;
        }
        r.hsi().modify(|w| w.set_trim(trim));
        wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
            r.hsi().read().trim() == trim
        })?;
        #[cfg(rcc_lsi_sysclk)]
        if let Some(state) = lsi_sysclk.as_mut() {
            let mut expected = state.hsi;
            expected.set_trim(trim);
            wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
                r.hsi().read().0 & crate::RCC_LSI_HSI_PARAMETERS_MASK
                    == expected.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK
            })?;
            state.hsi = expected;
            state.check_identity()?;
        }
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_hsien(true);
            if permanent_lsi {
                w.set_lsien(true);
            }
        });
        wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
            let v = r.cr1().read();
            v.hsien() && ccs_unchanged()
        })?;
        wait_until(config.timeout, Error::HsiTimeout, || {
            r.hsi().read().stable()
        })?;
        #[cfg(rcc_lsi_sysclk)]
        if let Some(state) = lsi_sysclk.as_mut() {
            let mut expected = state.sources;
            expected.set_hsien(true);
            wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
                let mut actual = r.cr1().read();
                actual.set_key(0);
                actual.0 == expected.0
            })?;
            state.sources = expected;
            state.check_identity()?;
        }
        #[cfg(rcc_lsi_sysclk)]
        if let Some(state) = lsi_sysclk.as_ref() {
            state.check_target(config.timeout, cs)?;
            if !r.isr().read().hsistable() {
                return Err(Error::HsiTimeout);
            }
        }
        r.cr0().modify(|w| {
            w.set_key(0x5a5a);
            w.set_sysclk(ClockSource::Hsi);
        });
        wait_until(config.timeout, Error::ClockSwitchTimeout, || {
            r.cr0().read().sysclk() == ClockSource::Hsi
        })?;
        barrier();
        #[cfg(rcc_lsi_sysclk)]
        if let Some(state) = lsi_sysclk.as_mut() {
            let mut expected = state.clock;
            expected.set_sysclk(ClockSource::Hsi);
            wait_until(config.timeout, Error::ClockSwitchTimeout, || {
                let mut actual = r.cr0().read();
                actual.set_key(0);
                actual.0 == expected.0
            })?;
            state.clock = expected;
            state.check_identity()?;
        }
        if !lsi_was_enabled && !needs_lsi && !permanent_lsi {
            r.cr1().modify(|w| {
                w.set_key(0x5a5a);
                w.set_lsien(false);
            });
            wait_until(config.timeout, Error::LsiRestoreTimeout, || {
                !r.cr1().read().lsien()
            })?;
        }
    }
    r.hsi().modify(|w| w.set_div(config.hsi.div as u8));
    wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
        r.hsi().read().div() == config.hsi.div as u8
    })?;
    wait_until(config.timeout, Error::HsiTimeout, || {
        r.hsi().read().stable()
    })?;

    #[cfg(rcc_lsi_sysclk)]
    if let Some(state) = lsi_sysclk.as_mut() {
        let mut expected = state.hsi;
        expected.set_div(config.hsi.div as u8);
        wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
            r.hsi().read().0 & crate::RCC_LSI_HSI_PARAMETERS_MASK
                == expected.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK
        })?;
        state.hsi = expected;
        state.check_target(config.timeout, cs)?;
        if !r.isr().read().hsistable() {
            return Err(Error::HsiTimeout);
        }
    }
    if let Some(hse) = config.hse.filter(|_| !preserve_hse) {
        #[cfg(rcc_lsi_sysclk)]
        if let Some(state) = lsi_sysclk.as_ref() {
            state.check_target(config.timeout, cs)?;
        }
        // Even an already-ready source is stopped before pin or parameter
        // changes: STABLE alone is not continuous oscillator validity.
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_hseen(false);
            if permanent_lsi {
                w.set_lsien(true);
            }
        });
        wait_until(config.timeout, Error::HseStopTimeout, || {
            !r.cr1().read().hseen()
        })?;
        wait_until(config.timeout, Error::HseStopTimeout, || {
            !r.hse().read().stable()
        })?;
        #[cfg(rcc_lsi_sysclk)]
        if let Some(state) = lsi_sysclk.as_mut() {
            let mut expected = state.sources;
            expected.set_hseen(false);
            wait_until(config.timeout, Error::HseStopTimeout, || {
                let mut actual = r.cr1().read();
                actual.set_key(0);
                actual.0 == expected.0
            })?;
            state.sources = expected;
            state.check_identity()?;
        }
        crate::rcc_configure_hse_pins(hse.mode == HseMode::Bypass, config.timeout, cs)?;
        #[cfg(rcc_lsi_sysclk)]
        if let Some(state) = lsi_sysclk.as_mut() {
            // This existing helper leaves only its actual projected pad-bank
            // gates enabled. Bypass owns input only; unavailable pads own none.
            if let Some(consumers) = state.consumers.as_mut() {
                let (input, output) = crate::RCC_LSI_HSE_GPIO_GATE_INDICES;
                if let Some(index) = input {
                    consumers.1[index] = true;
                }
                if hse.mode == HseMode::Oscillator {
                    if let Some(index) = output {
                        consumers.1[index] = true;
                    }
                }
            }
            state.check_target(config.timeout, cs)?;
        }
        let detector = hse.detector_count()?;
        r.hse().modify(|w| {
            w.set_mode(hse.mode == HseMode::Bypass);
            w.set_driver(hse.drive);
            w.set_freqrange(hse.range());
            w.set_waitcycle(pac::sysctrl::vals::HseWait::Cycles262144);
            w.set_flt(false);
            w.set_detcnt(detector);
        });
        wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
            let v = r.hse().read();
            let range = v.freqrange();
            v.mode() == (hse.mode == HseMode::Bypass)
                && v.driver() == hse.drive
                && range == hse.range()
                && v.waitcycle() == pac::sysctrl::vals::HseWait::Cycles262144
                && !v.flt()
                && v.detcnt() == detector
        })?;
        #[cfg(rcc_lsi_sysclk)]
        if let Some(state) = lsi_sysclk.as_mut() {
            let mut expected = state.hse;
            expected.set_mode(hse.mode == HseMode::Bypass);
            expected.set_driver(hse.drive);
            expected.set_freqrange(hse.range());
            expected.set_waitcycle(pac::sysctrl::vals::HseWait::Cycles262144);
            expected.set_flt(false);
            expected.set_detcnt(detector);
            wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
                r.hse().read().0 & crate::RCC_LSI_HSE_PARAMETERS_MASK
                    == expected.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK
            })?;
            state.hse = expected;
            state.check_target(config.timeout, cs)?;
        }
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_hseen(true);
            if permanent_lsi {
                w.set_lsien(true);
            }
        });
        wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
            r.cr1().read().hseen()
        })?;
        wait_until(config.timeout, Error::HseTimeout, || {
            r.hse().read().stable()
        })?;
        #[cfg(rcc_lsi_sysclk)]
        if let Some(state) = lsi_sysclk.as_mut() {
            let mut expected = state.sources;
            expected.set_hseen(true);
            wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
                let mut actual = r.cr1().read();
                actual.set_key(0);
                actual.0 == expected.0
            })?;
            state.sources = expected;
            state.check_identity()?;
        }
        #[cfg(rcc_lsi_sysclk)]
        if let Some(state) = lsi_sysclk.as_ref() {
            state.check_target(config.timeout, cs)?;
            if !r.isr().read().hsestable() {
                return Err(Error::HseTimeout);
            }
        }
    }
    check_external_faults(monitor_hse, monitor_lse)?;
    #[cfg(rcc_lsi_sysclk)]
    if let Some(mut state) = lsi_sysclk {
        // P7: the existing auxiliary helper runs on guarded HSI, with a
        // permanently requested factory-LSI monitor. Its bounded before/after
        // proof does not change the helper's pad-window or monitor contract.
        state.check_target(config.timeout, cs)?;
        if let Some(lse) = config.lse {
            let reused = reuse_lse.unwrap();
            crate::rcc::lse::start(lse, reused, cs)?;
            if !reused {
                // Native L031's established helper writes a default register,
                // not a modify. Model exactly that deliberate legacy behavior.
                let mut expected_lse = pac::sysctrl::regs::Lse::default();
                expected_lse.set_mode(lse.mode == crate::rcc::LseMode::Bypass);
                expected_lse.set_driver(lse.drive);
                expected_lse.set_amp(lse.amplitude);
                expected_lse.set_waitcycle(lse.wait);
                let mut expected_sources = state.sources;
                expected_sources.set_lseen(true);
                expected_sources.set_lseccs(true);
                wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
                    let mut sources = r.cr1().read();
                    sources.set_key(0);
                    sources.0 == expected_sources.0
                        && r.lse().read().0 & crate::RCC_LSI_LSE_PARAMETERS_MASK
                            == expected_lse.0 & crate::RCC_LSI_LSE_PARAMETERS_MASK
                })?;
                state.sources = expected_sources;
                state.lse = expected_lse;
                // LSE pad inspection restores GPIOC; it advances no gate.
            }
            state.check_target(config.timeout, cs)?;
        }
        // P8: qualify actual owners and pads, then execute the independently
        // validated configured HSI under final buses before selecting LSI.
        state.verify_tree(config, cs)?;
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
        wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
            let mut actual = r.cr0().read();
            actual.set_key(0);
            actual.0 == final_hsi.0
        })?;
        barrier();
        state.clock = final_hsi;
        state.verify_tree(config, cs)?;
        // P9: latency includes retained HSI even though the final source is
        // slower. No oscillator/source/divider write follows the LSI selection.
        let upper_hclk = clocks.hclk_bounds().maximum().0.max(
            crate::rcc::ClockBounds::hsi(config.hsi.div.divisor())
                .divided_by(config.ahb_pre.divisor())
                .maximum()
                .0,
        );
        let flash_wait = (upper_hclk - 1) / crate::RCC_FLASH_WAIT_STEP_HZ;
        set_flash_latency(flash_wait, config.timeout)?;
        state.verify_tree(config, cs)?;
        if u32::from(pac::FLASH.cr2().read().wait()) != flash_wait {
            return Err(Error::FlashLatencyTimeout);
        }
        let mut final_lsi = state.clock;
        final_lsi.set_sysclk(ClockSource::Lsi);
        r.cr0().modify(|w| {
            w.set_key(0x5a5a);
            w.set_sysclk(ClockSource::Lsi);
        });
        wait_until(config.timeout, Error::ClockSwitchTimeout, || {
            let mut actual = r.cr0().read();
            actual.set_key(0);
            actual.0 == final_lsi.0
        })?;
        barrier();
        state.clock = final_lsi;
        state.verify_tree(config, cs)?;
        if u32::from(pac::FLASH.cr2().read().wait()) != flash_wait {
            return Err(Error::FlashLatencyTimeout);
        }
        if clocks.hse.is_none() && state.entry_sources.hseen() {
            clocks.hse = Some(if state.hse.mode() {
                HseMode::Bypass
            } else {
                HseMode::Oscillator
            });
        }
        // Last hardware observation is the final selector and both dividers.
        let mut final_clock = r.cr0().read();
        final_clock.set_key(0);
        if final_clock.0 != final_lsi.0 {
            return Err(Error::ClockSwitchTimeout);
        }
        return Ok(clocks);
    }
    #[cfg(rcc_lse)]
    if let Some(state) = lse_sysclk {
        let lse = config.lse.ok_or(Error::LseNotConfigured)?;
        if !state.monitor_ready(false) || r.cr0().read().sysclk() != ClockSource::Hsi {
            return Err(Error::LsiTimeout);
        }
        // Fresh/exact LSE is established while HSI still executes. Only a fresh
        // start may add LSECCS; CLKCCS/HSECCS and LSELOCK remain inherited.
        crate::rcc::lse::start(lse, reuse_lse.unwrap(), cs)?;
        let verify_tree = |source, final_buses| {
            state.verify_tree(config, lse, trim, reuse_lse.unwrap(), cs)?;
            let clock = r.cr0().read();
            if clock.sysclk() != source
                || (final_buses
                    && (clock.hclkprs() != config.ahb_pre as u8
                        || clock.pclkprs() != config.apb_pre as u8))
            {
                return Err(Error::ClockConfigurationTimeout);
            }
            Ok(())
        };
        verify_tree(ClockSource::Hsi, false)?;
        // Pure validation proved this final HSI tree legal. Explicit HSI also
        // avoids reasserting a sampled external selector during divider setup.
        r.cr0().modify(|w| {
            w.set_key(0x5a5a);
            w.set_sysclk(ClockSource::Hsi);
            w.set_hclkprs(config.ahb_pre as u8);
            w.set_pclkprs(config.apb_pre as u8);
        });
        wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
            let clock = r.cr0().read();
            clock.sysclk() == ClockSource::Hsi
                && clock.hclkprs() == config.ahb_pre as u8
                && clock.pclkprs() == config.apb_pre as u8
        })?;
        barrier();
        let upper_hclk = clocks.hclk_bounds().maximum().0.max(
            crate::rcc::ClockBounds::hsi(config.hsi.div.divisor())
                .divided_by(config.ahb_pre.divisor())
                .maximum()
                .0,
        );
        let flash_wait = (upper_hclk - 1) / crate::RCC_FLASH_WAIT_STEP_HZ;
        set_flash_latency(flash_wait, config.timeout)?;
        verify_tree(ClockSource::Hsi, true)?;
        if u32::from(pac::FLASH.cr2().read().wait()) != flash_wait {
            return Err(Error::FlashLatencyTimeout);
        }
        r.cr0().modify(|w| {
            w.set_key(0x5a5a);
            w.set_sysclk(ClockSource::Lse);
        });
        wait_until(config.timeout, Error::ClockSwitchTimeout, || {
            let clock = r.cr0().read();
            clock.sysclk() == ClockSource::Lse
                && clock.hclkprs() == config.ahb_pre as u8
                && clock.pclkprs() == config.apb_pre as u8
        })?;
        barrier();
        // No CR0/source/divider write follows this final selector write.
        verify_tree(ClockSource::Lse, true)?;
        if u32::from(pac::FLASH.cr2().read().wait()) != flash_wait {
            return Err(Error::FlashLatencyTimeout);
        }
        // Preserve the old common tail's inherited HSE pad ownership before
        // returning through the dedicated target sequence.
        if clocks.hse.is_none() && state.sources.hseen() {
            clocks.hse = Some(if state.hse.mode() {
                HseMode::Bypass
            } else {
                HseMode::Oscillator
            });
        }
        let final_clock = r.cr0().read();
        if final_clock.sysclk() != ClockSource::Lse
            || final_clock.hclkprs() != config.ahb_pre as u8
            || final_clock.pclkprs() != config.apb_pre as u8
        {
            return Err(Error::ClockSwitchTimeout);
        }
        // No fallible work remains. Failed RCC init never installs this marker;
        // the shared monitor snapshot alone does not strengthen old leaf paths.
        LSE_SYSCLK_MONITOR.borrow(cs).set(Some(state));
        return Ok(clocks);
    }
    let sysclk = match config.sys {
        Sysclk::HSI => ClockSource::Hsi,
        Sysclk::HSE => ClockSource::Hse,
        #[cfg(rcc_lsi_sysclk)]
        Sysclk::LSI => unreachable!("LSI returns through its dedicated sequence"),
        #[cfg(rcc_lse)]
        Sysclk::LSE => unreachable!("LSE returns through its dedicated sequence"),
    };
    // Keep restrictive buses while selecting the source. Install final dividers
    // only after mux acknowledgment, so a slower target never exposes old HSI
    // through newly weakened final buses.
    if config.sys == Sysclk::HSE {
        r.cr0().modify(|w| {
            w.set_key(0x5a5a);
            w.set_sysclk(sysclk);
        });
        wait_until(config.timeout, Error::ClockSwitchTimeout, || {
            r.cr0().read().sysclk() == sysclk
        })?;
        barrier();
    }
    r.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_sysclk(sysclk);
        w.set_hclkprs(config.ahb_pre as u8);
        w.set_pclkprs(config.apb_pre as u8);
    });
    wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
        let v = r.cr0().read();
        v.sysclk() == sysclk
            && v.hclkprs() == config.ahb_pre as u8
            && v.pclkprs() == config.apb_pre as u8
    })?;
    barrier();
    wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
        let v = r.hsi().read();
        v.div() == config.hsi.div as u8 && v.trim() == trim
    })?;
    wait_until(config.timeout, Error::HsiTimeout, || {
        r.hsi().read().stable()
    })?;
    if config.hse.is_some() {
        wait_until(config.timeout, Error::HseTimeout, || {
            r.hse().read().stable()
        })?;
    }
    let mut upper_hclk = clocks.hclk_bounds().maximum().0;
    if config.hse.is_some() {
        upper_hclk = upper_hclk.max(
            crate::rcc::ClockBounds::hsi(config.hsi.div.divisor())
                .divided_by(config.ahb_pre.divisor())
                .maximum()
                .0,
        );
    }
    set_flash_latency(
        (upper_hclk - 1) / crate::RCC_FLASH_WAIT_STEP_HZ,
        config.timeout,
    )?;
    // Detect mux changes during the final Flash handshake before publishing.
    if r.cr0().read().sysclk() != sysclk {
        return Err(Error::ClockSwitchTimeout);
    }
    check_external_faults(monitor_hse, monitor_lse)?;
    if !ccs_unchanged() {
        return Err(Error::ClockConfigurationTimeout);
    }
    #[cfg(rcc_lse)]
    if let Some(old_lsi) = old_lsi {
        let lsi = r.lsi().read();
        if !r.cr1().read().lsien()
            || !lsi.stable()
            || lsi.trim() != old_lsi.trim()
            || lsi.waitcycle() != old_lsi.waitcycle()
        {
            return Err(Error::LsiTimeout);
        }
    }
    #[cfg(rcc_lse)]
    if let Some(lse) = config.lse {
        crate::rcc::lse::start(lse, reuse_lse.unwrap(), cs)?;
        let final_sources = r.cr1().read();
        if final_sources.clkccs() != old_sources.clkccs()
            || final_sources.hseccs() != old_sources.hseccs()
            || !final_sources.lseccs()
        {
            return Err(Error::ClockConfigurationTimeout);
        }
        crate::rcc::lse::verify(lse, cs)?;
    }
    // Freeze inherited ownership too, so safe GPIO remains blocked for the
    // entire boot even if later unsupported raw PAC writes disable HSE.
    if clocks.hse.is_none() && old_sources.hseen() {
        clocks.hse = Some(if r.hse().read().mode() {
            HseMode::Bypass
        } else {
            HseMode::Oscillator
        });
    }
    Ok(clocks)
}

