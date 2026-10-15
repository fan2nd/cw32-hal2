use super::*;

#[cfg(rcc_hse)]
pub(super) fn configure_hse(
    config: Config,
    cs: critical_section::CriticalSection<'_>,
) -> Result<Clocks, Error> {
    #[cfg(rcc_lsi_sysclk)]
    if config.sys == Sysclk::LSI {
        return configure_lsi(config, cs);
    }
    #[cfg(all(rcc_lse, rcc_cw32l052_v1))]
    if config.sys == Sysclk::LSE {
        return lse_sysclk::configure(config, cs);
    }
    #[cfg(all(rcc_lse, rcc_cw32l083_v1))]
    if config.sys == Sysclk::LSE {
        return l083_lse_sysclk::configure(config, cs);
    }
    let mut clocks = config.frequencies()?;
    let r = pac::SYSCTRL;
    #[cfg(rcc_lse)]
    let reuse_lse = config
        .lse
        .map(|c| crate::rcc::lse::preflight(c, cs))
        .transpose()?;
    #[cfg(rcc_lse)]
    if let Some(lse) = config.lse {
        prepare_lse_monitor(lse.poll_budget, cs)?;
    }
    let old_clock = r.cr0().read();
    let old_sources = r.cr1().read();
    let old_hse = r.hse().read();
    #[cfg(rcc_pll)]
    if config.pll.is_some()
        && r.pll().read().reserved_debug().to_bits() != crate::RCC_PLL_RESERVED_DEBUG_DEFAULT
    {
        return Err(Error::PllReservedConfiguration);
    }
    let monitor_hse = config.hse.is_some() || old_sources.hseen();
    let monitor_lse = old_sources.lseen();
    #[cfg(rcc_lse)]
    let monitor_lse = monitor_lse || config.lse.is_some();
    #[cfg(rcc_lse)]
    let old_lsi = config.lse.map(|_| r.lsi().read());
    let needs_lsi = (monitor_hse && old_sources.hseccs()) || (monitor_lse && old_sources.lseccs());
    #[cfg(rcc_lse)]
    let needs_lsi = needs_lsi || config.lse.is_some();
    // HSE/LSE CCS controls are configurable here, not reserved mandatory ones.
    let ccs_unchanged = || {
        let v = r.cr1().read();
        v.clkccs() == old_sources.clkccs()
            && v.hseccs() == old_sources.hseccs()
            && v.lseccs() == old_sources.lseccs()
            && v.lselock() == old_sources.lselock()
            && v.lseen() == old_sources.lseen()
    };
    match old_clock.sysclk() {
        ClockSource::Hsi | ClockSource::Hse | ClockSource::Lsi | ClockSource::Lse => {}
        #[cfg(rcc_cw32l083_v1)]
        ClockSource::Pll => {}
        _ => return Err(Error::InvalidClockSource),
    }
    check_external_faults(monitor_hse, monitor_lse)?;
    let old_hsi = r.hsi().read();
    if !matches!(old_hsi.div(), 5 | 6 | 8 | 9 | 11..=15) {
        return Err(Error::InvalidHsiDivider);
    }
    let factory =
        unsafe { core::ptr::read_volatile(crate::RCC_FACTORY_HSI_TRIM_ADDRESS as *const u16) };
    if factory == u16::MAX {
        return Err(Error::InvalidCalibration);
    }
    let mut calibrated = pac::sysctrl::regs::Hsi::default();
    calibrated.set_trim(factory);
    let trim = calibrated.trim();
    let needs_trim = old_hsi.trim() != trim;
    // Inspect AUTOTRIM through its real APBEN2 gate, without reset or writes.
    // Even an unchanged requested trim cannot admit active calibration: it may
    // asynchronously change HSI/LSI underneath initialization and frozen bounds.
    let autotrim = <crate::peripherals::AUTOTRIM as crate::rcc::SealedRccPeripheral>::RCC_INFO
        .inspect_for_init(cs, config.timeout, || pac::AUTOTRIM.cr().read())
        .map_err(|_| Error::RetainedClockInspectionTimeout)?;
    if autotrim.en()
        && (autotrim.md() != pac::autotrim::vals::Mode::Timer
            || autotrim.auto()
            || !matches!(
                autotrim.src(),
                pac::autotrim::vals::Source::HsiOsc
                    | pac::autotrim::vals::Source::Lsi
                    | pac::autotrim::vals::Source::Hse
                    | pac::autotrim::vals::Source::Lse
                    | pac::autotrim::vals::Source::Etr
            ))
    {
        return Err(Error::RetainedAutotrimConfiguration);
    }
    if needs_trim && autotrim.en() && autotrim.src() == pac::autotrim::vals::Source::HsiOsc {
        return Err(Error::HsiClockInUse);
    }
    if needs_trim
        && pac::LVD.cr0().read().en()
        && pac::LVD.cr1().read().flten()
        && pac::LVD.cr1().read().fltclk()
    {
        return Err(Error::HsiClockInUse);
    }
    let rtc_source = <crate::peripherals::RTC as crate::rcc::SealedRccPeripheral>::RCC_INFO
        .inspect_for_init(cs, config.timeout, || pac::RTC.cr1().read().source())
        .map_err(|_| Error::RetainedClockInspectionTimeout)?;
    if !matches!(
        rtc_source,
        pac::rtc::vals::Source::Lse
            | pac::rtc::vals::Source::Lsi
            | pac::rtc::vals::Source::HseDiv128
            | pac::rtc::vals::Source::HseDiv256
            | pac::rtc::vals::Source::HseDiv512
            | pac::rtc::vals::Source::HseDiv1024
    ) {
        return Err(Error::RetainedRtcConfiguration);
    }
    let mut preserve_hse = false;
    if let Some(hse) = config.hse {
        // AUTOTRIM ETR can route to PF0. Without a proven nonconflicting route,
        // reject the request before changing any oscillator or pad.
        if autotrim.en() && autotrim.src() == pac::autotrim::vals::Source::Etr {
            return Err(Error::HsePinInUse);
        }
        let rtc_hse = matches!(
            rtc_source,
            pac::rtc::vals::Source::HseDiv128
                | pac::rtc::vals::Source::HseDiv256
                | pac::rtc::vals::Source::HseDiv512
                | pac::rtc::vals::Source::HseDiv1024
        );
        preserve_hse =
            rtc_hse || (autotrim.en() && autotrim.src() == pac::autotrim::vals::Source::Hse);
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
        w.set_hclkprs(guard_hclk);
        w.set_pclkprs(guard_pclk);
    });
    wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
        let v = r.cr0().read();
        v.hclkprs() == guard_hclk && v.pclkprs() == guard_pclk
    })?;
    barrier();

    r.cr1().modify(|w| {
        w.set_key(0x5a5a);
        w.set_hsien(true);
    });
    wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
        let v = r.cr1().read();
        v.hsien() && ccs_unchanged()
    })?;
    wait_until(config.timeout, Error::HsiTimeout, || {
        r.hsi().read().stable()
    })?;
    // Leave the legal incoming source via unchanged HSI before any retuning.
    r.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_sysclk(ClockSource::Hsi);
    });
    wait_until(config.timeout, Error::ClockSwitchTimeout, || {
        r.cr0().read().sysclk() == ClockSource::Hsi
    })?;
    barrier();
    // Own RM section 4.5 permits PLL transitions only with HSI/HSE. The
    // unchanged-HSI escape above is therefore required before the LSI bridge.
    #[cfg(rcc_cw32l083_v1)]
    let needs_bridge = needs_trim || old_sources.pllen() || old_clock.sysclk() == ClockSource::Pll;
    #[cfg(rcc_cw32l052_v1)]
    let needs_bridge = needs_trim;
    let lsi_was_enabled = old_sources.lsien();
    // A retained enabled external detector requires unchanged legal LSI,
    // even without a trim bridge. CLKCCS alone does not enable a detector.
    if needs_bridge || needs_lsi {
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
    }
    if needs_bridge {
        r.cr0().modify(|w| {
            w.set_key(0x5a5a);
            w.set_sysclk(ClockSource::Lsi);
        });
        wait_until(config.timeout, Error::TemporaryClockSwitchTimeout, || {
            r.cr0().read().sysclk() == ClockSource::Lsi
        })?;
        barrier();
    }
    // Stop and observe PLL before changing its HSI/HSE source or HSI divider;
    // configuration fields are not written until both stop acknowledgments hold.
    #[cfg(rcc_cw32l083_v1)]
    {
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_pllen(false);
        });
        wait_until(config.timeout, Error::PllStopTimeout, || {
            !r.cr1().read().pllen()
        })?;
        wait_until(config.timeout, Error::PllStopTimeout, || {
            !r.pll().read().stable()
        })?;
    }
    if needs_trim {
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_hsien(false);
        });
        wait_until(config.timeout, Error::HsiStopTimeout, || {
            !r.cr1().read().hsien()
        })?;
        wait_until(config.timeout, Error::HsiStopTimeout, || {
            !r.hsi().read().stable()
        })?;
        r.hsi().modify(|w| w.set_trim(trim));
        wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
            r.hsi().read().trim() == trim
        })?;
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_hsien(true);
        });
        wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
            let v = r.cr1().read();
            v.hsien() && ccs_unchanged()
        })?;
        wait_until(config.timeout, Error::HsiTimeout, || {
            r.hsi().read().stable()
        })?;
    }
    if needs_bridge {
        r.cr0().modify(|w| {
            w.set_key(0x5a5a);
            w.set_sysclk(ClockSource::Hsi);
        });
        wait_until(config.timeout, Error::ClockSwitchTimeout, || {
            r.cr0().read().sysclk() == ClockSource::Hsi
        })?;
        barrier();
        if !lsi_was_enabled && !needs_lsi {
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
        let hsi = r.hsi().read();
        hsi.div() == config.hsi.div as u8
            && hsi.trim() == trim
            && r.cr0().read().sysclk() == ClockSource::Hsi
    })?;
    wait_until(config.timeout, Error::HsiTimeout, || {
        r.hsi().read().stable()
    })?;

    if let Some(hse) = config.hse.filter(|_| !preserve_hse) {
        // Even an already-ready source is stopped before pin or parameter
        // changes: STABLE alone is not continuous oscillator validity.
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_hseen(false);
        });
        wait_until(config.timeout, Error::HseStopTimeout, || {
            !r.cr1().read().hseen()
        })?;
        wait_until(config.timeout, Error::HseStopTimeout, || {
            !r.hse().read().stable()
        })?;
        crate::rcc_configure_hse_pins(hse.mode == HseMode::Bypass, config.timeout, cs)?;
        let detector = hse.detector_count()?;
        r.hse().modify(|w| {
            w.set_mode(hse.mode == HseMode::Bypass);
            w.set_driver(hse.drive);
            w.set_freqrange(hse.range());
            // L052 has distinct pre-start fields. All parameters must be set
            // before enabling; reserved bits31:24 are preserved by modify.
            #[cfg(rcc_cw32l052_v1)]
            {
                w.set_pdriver(hse.drive);
                w.set_pfreqrange(hse.range());
            }
            w.set_waitcycle(pac::sysctrl::vals::HseWait::Cycles262144);
            w.set_flt(false);
            w.set_detcnt(detector);
        });
        wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
            hse_parameters_match(hse).unwrap_or(false)
        })?;
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_hseen(true);
        });
        wait_until(config.timeout, Error::ClockConfigurationTimeout, || {
            r.cr1().read().hseen()
        })?;
        wait_until(config.timeout, Error::HseTimeout, || {
            r.hse().read().stable()
        })?;
    }
    #[cfg(rcc_pll)]
    if let Some(pll) = config.pll {
        let parameters = pll.parameters(&config)?;
        // Both PLLEN and the real STABLE latch were observed clear before any
        // reference changes above. Modify preserves reserved/debug defaults.
        r.pll().modify(|w| {
            w.set_source(parameters.source);
            w.set_freqin(parameters.input_range);
            w.set_mul(pll.mul);
            w.set_freqout(parameters.output_range);
            w.set_waitcycle(pac::sysctrl::vals::PllWait::from_bits(
                crate::RCC_PLL_STARTUP_ENCODING,
            ));
        });
        wait_until(config.timeout, Error::PllConfigurationTimeout, || {
            pll_parameters_match(pll, &parameters)
                && !r.cr1().read().pllen()
                && !r.pll().read().stable()
        })?;
        // Recheck the complete selected dependency immediately before enabling
        // PLL. STABLE alone cannot establish source mode, pads or fault state.
        verify_pll_reference(pll, &config, trim, cs)?;
        check_external_faults(monitor_hse, monitor_lse)?;
        r.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_pllen(true);
        });
        wait_until(config.timeout, Error::PllConfigurationTimeout, || {
            r.cr1().read().pllen() && ccs_unchanged() && pll_parameters_match(pll, &parameters)
        })?;
        // PLLRDY is a clearable event; only PLL.STABLE is the startup handshake.
        wait_until(config.timeout, Error::PllTimeout, || {
            r.pll().read().stable()
        })?;
    }
    check_external_faults(monitor_hse, monitor_lse)?;
    let sysclk = match config.sys {
        Sysclk::HSI => ClockSource::Hsi,
        Sysclk::HSE => ClockSource::Hse,
        #[cfg(rcc_lsi_sysclk)]
        Sysclk::LSI => return Err(Error::InvalidClockSource),
        // The explicit target branch above bypasses this old auxiliary path.
        #[cfg(rcc_lse)]
        Sysclk::LSE => return Err(Error::InvalidClockSource),
        #[cfg(rcc_pll)]
        Sysclk::PLL => ClockSource::Pll,
    };
    #[cfg(rcc_pll)]
    if let Some(pll) = config.pll {
        verify_pll_reference(pll, &config, trim, cs)?;
        if !r.cr1().read().pllen() || !pll_parameters_match(pll, &pll.parameters(&config)?) {
            return Err(Error::PllConfigurationTimeout);
        }
        if !r.pll().read().stable() {
            return Err(Error::PllTimeout);
        }
        check_external_faults(monitor_hse, monitor_lse)?;
    }
    // Keep restrictive buses while selecting the source. Install final dividers
    // only after mux acknowledgment, so a slower target never exposes old HSI
    // through newly weakened final buses.
    if config.sys != Sysclk::HSI {
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
    // The retained HSI escape can be used at the final divisors too.
    if config.sys != Sysclk::HSI || config.hse.is_some() {
        upper_hclk = upper_hclk.max(
            crate::rcc::ClockBounds::hsi(config.hsi.div.divisor())
                .divided_by(config.ahb_pre.divisor())
                .maximum()
                .0,
        );
        #[cfg(rcc_cw32l052_v1)]
        {
            upper_hclk = upper_hclk.max(
                crate::rcc::ClockBounds::hsi(crate::RCC_FIXED_CCS_HSI_DIVISOR)
                    .divided_by(config.ahb_pre.divisor())
                    .maximum()
                    .0,
            );
        }
    }
    // Verify the complete final source tree before reducing Flash latency and
    // again after the Flash handshake and the last requested LSE operation.
    // A requested LSE may change only its own enable/detector policy here.
    let verify_final_tree = |lse_started: bool| -> Result<(), Error> {
        if r.cr0().read().sysclk() != sysclk {
            return Err(Error::ClockSwitchTimeout);
        }
        check_external_faults(monitor_hse, monitor_lse)?;
        if !r.hsi().read().stable() {
            return Err(Error::HsiTimeout);
        }
        let final_sources = r.cr1().read();
        let expected_lsi = lsi_was_enabled || needs_lsi;
        if final_sources.clkccs() != old_sources.clkccs()
            || final_sources.hseccs() != old_sources.hseccs()
            || final_sources.lseccs() != (old_sources.lseccs() || lse_started)
            || final_sources.lselock() != old_sources.lselock()
            || final_sources.lseen() != (old_sources.lseen() || lse_started)
            || !final_sources.hsien()
            || final_sources.lsien() != expected_lsi
            || final_sources.hseen() != monitor_hse
            || r.hsi().read().div() != config.hsi.div as u8
            || r.hsi().read().trim() != trim
            || r.cr0().read().hclkprs() != config.ahb_pre as u8
            || r.cr0().read().pclkprs() != config.apb_pre as u8
        {
            return Err(Error::ClockConfigurationTimeout);
        }
        #[cfg(rcc_pll)]
        match config.pll {
            Some(pll) => {
                if !final_sources.pllen() || !pll_parameters_match(pll, &pll.parameters(&config)?) {
                    return Err(Error::PllConfigurationTimeout);
                }
                if !r.pll().read().stable() {
                    return Err(Error::PllTimeout);
                }
            }
            None => {
                if final_sources.pllen() || r.pll().read().stable() {
                    return Err(Error::PllStopTimeout);
                }
            }
        }
        if let Some(hse) = config.hse {
            verify_hse_source(hse, config.timeout, cs)?;
        }
        #[cfg(rcc_lse)]
        if let Some(old_lsi) = old_lsi {
            let current = r.lsi().read();
            if !r.cr1().read().lsien()
                || !current.stable()
                || current.trim() != old_lsi.trim()
                || current.waitcycle() != old_lsi.waitcycle()
            {
                return Err(Error::LsiTimeout);
            }
        }
        Ok(())
    };
    verify_final_tree(false)?;
    let final_flash_wait = (upper_hclk - 1) / crate::RCC_FLASH_WAIT_STEP_HZ;
    set_flash_latency(final_flash_wait, config.timeout)?;
    verify_final_tree(false)?;
    #[cfg(rcc_lse)]
    if let Some(lse) = config.lse {
        crate::rcc::lse::start(lse, reuse_lse.unwrap(), cs)?;
        let after = r.cr1().read();
        if after.clkccs() != old_sources.clkccs()
            || after.hseccs() != old_sources.hseccs()
            || after.lselock() != old_sources.lselock()
            || !after.lseccs()
        {
            return Err(Error::ClockConfigurationTimeout);
        }
        crate::rcc::lse::verify(lse, cs)?;
    }
    #[cfg(rcc_lse)]
    let lse_started = config.lse.is_some();
    #[cfg(not(rcc_lse))]
    let lse_started = false;
    verify_final_tree(lse_started)?;
    if !r.ahben().read().flash() {
        return Err(Error::FlashClockTimeout);
    }
    if pac::FLASH.cr2().read().wait() != final_flash_wait as u8 {
        return Err(Error::FlashLatencyTimeout);
    }
    // Freeze inherited ownership too, so safe GPIO remains blocked for the
    // entire boot even if later unsupported raw PAC writes disable HSE.
    // An inherited crystal keeps both pads reserved for the boot even if an
    // unowned source was successfully stopped and reconfigured as bypass.
    // This is conservative ownership policy, not a claim PF1 remains in use.
    if old_sources.hseen() {
        if !old_hse.mode() {
            clocks.hse = Some(HseMode::Oscillator);
        } else if clocks.hse.is_none() {
            clocks.hse = Some(HseMode::Bypass);
        }
    }
    Ok(clocks)
}
// retains the original RCC path. Factory-valid live LSI is never retuned.
#[cfg(rcc_lse)]
pub(super) fn prepare_lse_monitor(
    timeout: u32,
    cs: critical_section::CriticalSection<'_>,
) -> Result<(), Error> {
    use crate::rtc::sealed::Instance;
    // AUTOTRIM calibration can consume/mutate LSI independently of SRC. Its
    // configuration-only gate may be inspected without resuming gated work.
    if !crate::rcc_lse_monitor_can_freeze(timeout, cs)? {
        return Err(Error::LseClockInUse);
    }
    let r = pac::SYSCTRL;
    let factory = unsafe {
        core::ptr::read_volatile(crate::peripherals::RTC::FACTORY_TRIM_ADDRESS as *const u16)
    };
    if factory == u16::MAX {
        return Err(Error::LseNotReady);
    }
    let mut calibration = pac::sysctrl::regs::Lsi::default();
    calibration.set_trim(factory);
    let before = r.lsi().read();
    // An enabled or pending ready interrupt is a retained observer when this
    // request would start LSI. Never clear it or mask the peripheral's owner.
    if !r.cr1().read().lsien() && (r.ier().read().lsirdy() || r.isr().read().lsirdy()) {
        return Err(Error::LseClockInUse);
    }
    if before.trim() == calibration.trim() {
        return Ok(());
    }
    // LSIEN alone is not used as ownership proof. The generated gate-preserving
    // checks cover every reviewed direct selector and detector, twice, with
    // unchanged native trim/wait and both hardware stable indications low.
    let stopped = || {
        let control = r.cr1().read();
        let lsi = r.lsi().read();
        !control.lsien()
            && !lsi.stable()
            && !r.isr().read().lsistable()
            && !control.hseccs()
            && !control.lseccs()
            && !r.ier().read().lsirdy()
            && !r.isr().read().lsirdy()
            && lsi.trim() == before.trim()
            && lsi.waitcycle() == before.waitcycle()
    };
    for _ in 0..2 {
        if !stopped() || !crate::rcc_lsi_consumers_idle(timeout, cs)? || !stopped() {
            return Err(Error::LseClockInUse);
        }
    }
    // Own manuals require parameters before enable. LSI has no write key.
    // Keep WAIT and every unrelated/reserved field; never stop a live source.
    r.lsi().modify(|w| w.set_trim(calibration.trim()));
    wait_until(timeout, Error::LseNotReady, || {
        let lsi = r.lsi().read();
        lsi.trim() == calibration.trim() && lsi.waitcycle() == before.waitcycle()
    })
}
