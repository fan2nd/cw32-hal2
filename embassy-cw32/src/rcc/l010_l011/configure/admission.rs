use super::*;

pub(super) fn admit(
    config: Config,
    cs: critical_section::CriticalSection<'_>,
    observed: &Entry,
    #[cfg(any(rcc_lse, all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk)))]
    targets: &Targets,
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))] expected: &Expected,
) -> Result<Admitted, Error> {
    let old_sources = observed.old_sources;
    let old_hsi = observed.old_hsi;
    let old_hse = observed.old_hse;
    #[cfg(rcc_lse)]
    let old_lsi = observed.old_lsi;
    #[cfg(rcc_lse)]
    let trim = observed.trim;
    let needs_trim = observed.needs_trim;
    // Native central rejection precedes the first configuration-gate write.
    // This phase inspects only configuration domains, never GPIO/timer work.
    #[cfg(rcc_lse)]
    let lse_admission = config
        .lse
        .map(|c| crate::rcc::lse::preflight(c, cs, false))
        .transpose()?;

    // Inspect actual central gates, restore their incoming state, never reset.
    // SOURCE owns RTC/AWT's raw clock even when the calendar START bit is zero.
    #[cfg(rcc_lse)]
    if targets.lse.is_some()
        && <crate::peripherals::RTC as crate::rcc::SealedRccPeripheral>::RCC_INFO.reset_asserted()
    {
        return Err(Error::RtcClockInUse);
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
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = targets.lsi {
        if <crate::peripherals::RTC as crate::rcc::SealedRccPeripheral>::RCC_INFO.reset_asserted() {
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
            return Err(Error::RtcClockInUse);
        }
    }
    let ((rtc_source, rtc_psc1), operational) =
        <crate::peripherals::RTC as crate::rcc::SealedRccPeripheral>::RCC_INFO
            .inspect_for_init(cs, config.timeout, || {
                let operational = true;
                #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
                let operational = operational
                    && (targets.lsi.is_none() || {
                        let info =
                            <crate::peripherals::RTC as crate::rcc::SealedRccPeripheral>::RCC_INFO;
                        info.is_enabled() && !info.reset_asserted()
                    });
                #[cfg(all(rcc_cw32l011_v1, rcc_lsi_sysclk))]
                if !operational {
                    return ((0, 0), false);
                }
                let source = pac::RTC.cr1().read().source();
                let psc1 = 0;
                #[cfg(all(rcc_cw32l011_v1, rcc_lsi_sysclk))]
                let psc1 = if targets.lsi.is_some() {
                    pac::RTC.psc().read().psc1()
                } else {
                    psc1
                };
                let value = (source, psc1);
                #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
                let operational = operational
                    && (targets.lsi.is_none() || {
                        let info =
                            <crate::peripherals::RTC as crate::rcc::SealedRccPeripheral>::RCC_INFO;
                        info.is_enabled() && !info.reset_asserted()
                    });
                (value, operational)
            })
            .map_err(|error| retained_inspection_error(config, error))?;
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    let operational = operational
        && (targets.lsi.is_none()
            || !<crate::peripherals::RTC as crate::rcc::SealedRccPeripheral>::RCC_INFO
                .reset_asserted());
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
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if targets.lsi.is_some() && !operational {
        return Err(Error::RtcClockInUse);
    }
    let _ = operational;
    if rtc_source > 3 {
        return Err(Error::RetainedRtcConfiguration);
    }
    if rtc_source == 3 && (needs_trim || !old_sources.hsien() || !old_hsi.stable()) {
        return Err(Error::RtcClockInUse);
    }
    #[cfg(all(rcc_cw32l011_v1, rcc_lsi_sysclk))]
    if targets.lsi.is_some() {
        // PSC1 is the retained raw eight-bit value; widen before adding one.
        // The raw HSI ceiling is independent of HSI DIV and public RTC defaults.
        let divisor = u32::from(rtc_psc1) + 1;
        if rtc_source == 1
            || (rtc_source == 3
                && crate::rcc::ClockBounds::hsi(1).maximum().0 > 1_000_000 * divisor)
        {
            return Err(Error::RtcClockInUse);
        }
    }
    let _ = rtc_psc1;
    let preserve_hse = rtc_source == 1;
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
    let retained_hse = if !preserve_hse {
        Ok(true)
    } else if !old_sources.hseen() || !old_hse.stable() {
        Ok(false)
    } else {
        crate::rcc_hse_pins_match(
            old_hse.mode(),
            config.timeout,
            cs,
            #[cfg(all(rcc_cw32l010_v1, rcc_lsi_sysclk))]
            targets.lsi.is_some(),
        )
    };
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = targets.lsi {
        if let Err(error @ Error::LsiConfigurationGateTimeout { .. }) = retained_hse {
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
    if !retained_hse? {
        return Err(Error::HseClockInUse);
    }
    if let Some(hse) = config.hse {
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
        let declared_hse = if !preserve_hse {
            Ok(true)
        } else {
            match hse_parameters_match(hse) {
                Ok(true) => crate::rcc_hse_pins_match(
                    hse.mode == HseMode::Bypass,
                    config.timeout,
                    cs,
                    #[cfg(all(rcc_cw32l010_v1, rcc_lsi_sysclk))]
                    targets.lsi.is_some(),
                ),
                result => result,
            }
        };
        #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
        if let Some(entry) = targets.lsi {
            if let Err(error @ Error::LsiConfigurationGateTimeout { .. }) = declared_hse {
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
        if !declared_hse? {
            return Err(Error::HseClockInUse);
        }
    }
    #[cfg(rcc_lse)]
    if targets.lse.is_some()
        && <crate::peripherals::ADC as crate::rcc::SealedRccPeripheral>::RCC_INFO.reset_asserted()
    {
        return Err(Error::AdcClockInUse);
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
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = targets.lsi {
        if <crate::peripherals::ADC as crate::rcc::SealedRccPeripheral>::RCC_INFO.reset_asserted() {
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
            return Err(Error::AdcClockInUse);
        }
    }
    let (adc_enabled, operational) =
        <crate::peripherals::ADC as crate::rcc::SealedRccPeripheral>::RCC_INFO
            .inspect_for_init(cs, config.timeout, || {
                let operational = true;
                #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
                let operational = operational
                    && (targets.lsi.is_none() || {
                        let info =
                            <crate::peripherals::ADC as crate::rcc::SealedRccPeripheral>::RCC_INFO;
                        info.is_enabled() && !info.reset_asserted()
                    });
                #[cfg(all(rcc_cw32l011_v1, rcc_lsi_sysclk))]
                if !operational {
                    return (false, false);
                }
                let value = pac::ADC.cr().read().en();
                #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
                let operational = operational
                    && (targets.lsi.is_none() || {
                        let info =
                            <crate::peripherals::ADC as crate::rcc::SealedRccPeripheral>::RCC_INFO;
                        info.is_enabled() && !info.reset_asserted()
                    });
                (value, operational)
            })
            .map_err(|error| retained_inspection_error(config, error))?;
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    let operational = operational
        && (targets.lsi.is_none()
            || !<crate::peripherals::ADC as crate::rcc::SealedRccPeripheral>::RCC_INFO
                .reset_asserted());
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
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if targets.lsi.is_some() && !operational {
        return Err(Error::AdcClockInUse);
    }
    let _ = operational;
    if adc_enabled {
        return Err(Error::AdcClockInUse);
    }
    // LVD and both comparators share the actual VC configuration gate. BGR
    // lives in ADC; none of these reads changes BGR, TSEN or reference selection.
    #[cfg(rcc_lse)]
    if targets.lse.is_some()
        && <crate::peripherals::VC1 as crate::rcc::SealedRccPeripheral>::RCC_INFO.reset_asserted()
    {
        return Err(Error::ComparatorClockInUse);
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
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = targets.lsi {
        if <crate::peripherals::VC1 as crate::rcc::SealedRccPeripheral>::RCC_INFO.reset_asserted() {
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
            return Err(Error::ComparatorClockInUse);
        }
    }
    let ((lvd0, lvd1, vc10, vc11, vc20, vc21), operational) =
        <crate::peripherals::VC1 as crate::rcc::SealedRccPeripheral>::RCC_INFO
            .inspect_for_init(cs, config.timeout, || {
                let operational = true;
                #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
                let operational = operational
                    && (targets.lsi.is_none() || {
                        let info =
                            <crate::peripherals::VC1 as crate::rcc::SealedRccPeripheral>::RCC_INFO;
                        info.is_enabled() && !info.reset_asserted()
                    });
                #[cfg(all(rcc_cw32l011_v1, rcc_lsi_sysclk))]
                if !operational {
                    return (
                        (
                            Default::default(),
                            Default::default(),
                            Default::default(),
                            Default::default(),
                            Default::default(),
                            Default::default(),
                        ),
                        false,
                    );
                }
                let values = (
                    pac::LVD.cr0().read(),
                    pac::LVD.cr1().read(),
                    pac::VC1.cr0().read(),
                    pac::VC1.cr1().read(),
                    pac::VC2.cr0().read(),
                    pac::VC2.cr1().read(),
                );
                #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
                let operational = operational
                    && (targets.lsi.is_none() || {
                        let info =
                            <crate::peripherals::VC1 as crate::rcc::SealedRccPeripheral>::RCC_INFO;
                        info.is_enabled() && !info.reset_asserted()
                    });
                (values, operational)
            })
            .map_err(|error| retained_inspection_error(config, error))?;
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    let operational = operational
        && (targets.lsi.is_none()
            || !<crate::peripherals::VC1 as crate::rcc::SealedRccPeripheral>::RCC_INFO
                .reset_asserted());
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
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if targets.lsi.is_some() && !operational {
        return Err(Error::ComparatorClockInUse);
    }
    let _ = operational;
    if let Some(hse) = config.hse.filter(|_| !preserve_hse) {
        // Stopping an inherited crystal also disturbs OSC_OUT, even when the
        // requested bypass setup only writes OSC_IN. Readback-only reuse is safe.
        let owns_output =
            hse.mode == HseMode::Oscillator || (old_sources.hseen() && !old_hse.mode());
        let intersects = |masks: [u32; 2], mux: u8| {
            (masks[0] | if owns_output { masks[1] } else { 0 }) & (1 << mux) != 0
        };
        if (vc10.en()
            && (intersects(crate::RCC_HSE_VC1_INP_MASKS, vc10.inp())
                || intersects(crate::RCC_HSE_VC1_INN_MASKS, vc10.inn())))
            || (vc20.en()
                && (intersects(crate::RCC_HSE_VC2_INP_MASKS, vc20.inp())
                    || intersects(crate::RCC_HSE_VC2_INN_MASKS, vc20.inn())))
        {
            return Err(Error::HsePinInUse);
        }
    }
    let lvd_filtered = lvd0.en() && lvd1.flttime() != 0;
    if lvd_filtered && lvd0.fltclk() {
        return Err(Error::LvdClockInUse);
    }
    // Blanking windows also use PCLK independently of filter-clock selection.
    if (vc10.en()
        && (vc11.blankatch1()
            || vc11.blankatch2()
            || vc11.blankatch3()
            || vc11.blankatch4()
            || vc11.blankatch5()
            || vc11.blankatch6()))
        || (vc20.en()
            && (vc21.blankatch1()
                || vc21.blankatch2()
                || vc21.blankatch3()
                || vc21.blankatch4()
                || vc21.blankatch5()
                || vc21.blankatch6()))
    {
        return Err(Error::ComparatorClockInUse);
    }
    let vc1_filtered = vc10.en() && vc11.flttime() != 0;
    let vc2_filtered = vc20.en() && vc21.flttime() != 0;
    if (vc1_filtered && vc11.fltclk() == pac::vc::vals::FilterClock::Pclk)
        || (vc2_filtered && vc21.fltclk() == pac::vc::vals::FilterClock::Pclk)
    {
        return Err(Error::ComparatorClockInUse);
    }
    let needs_lsi = old_sources.hseccs()
        || old_sources.lseccs()
        || lvd_filtered
        || vc1_filtered
        || vc2_filtered;
    // Freeze the FIRST request classification from entry, including automatic
    // clients with LSIEN=0. Later STABLE progress must not erase these checks.
    #[cfg(rcc_lse)]
    let first_lsi_request = targets.lse.is_some() && (needs_trim || needs_lsi) && !old_lsi.stable();
    #[cfg(rcc_lse)]
    if let Some(entry) = targets.lse {
        entry.hsi_ownership(trim)?;
        if first_lsi_request {
            entry.admit_first_lsi_request(config, cs)?;
        }
    }
    Ok(Admitted {
        preserve_hse,
        needs_lsi,
        #[cfg(rcc_lse)]
        first_lsi_request,
        #[cfg(rcc_lse)]
        lse_admission,
    })
}
