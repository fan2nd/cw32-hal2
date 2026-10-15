//! Ordered initialization phases. Transport values contain only already-read
//! register words and plain facts; no constructor performs an observation.
use super::*;

mod admission;
mod calibrate_hsi;
mod establish_hse;
mod finish;
mod flash_bridge;

// Immutable entry observations. This carrier has no Drop implementation.
struct Entry {
    r: pac::sysctrl::Sysctrl,
    old_clock: pac::sysctrl::regs::Cr0,
    old_sources: pac::sysctrl::regs::Cr1,
    old_hsi: pac::sysctrl::regs::Hsi,
    old_hse: pac::sysctrl::regs::Hse,
    old_lsi: pac::sysctrl::regs::Lsi,
    old_lse: pac::sysctrl::regs::Lse,
    trim: u16,
    needs_trim: bool,
    live_lsi: bool,
    monitor_hse: bool,
    monitor_lse: bool,
}

// Mutable expectations are private stack state, never a register rollback.
#[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
struct Expected {
    clock: pac::sysctrl::regs::Cr0,
    sources: pac::sysctrl::regs::Cr1,
    hsi: pac::sysctrl::regs::Hsi,
    hsi_stable: bool,
    hse: pac::sysctrl::regs::Hse,
    hse_stable: bool,
    wait: Option<(u8, u8)>,
}

// The snapshots retain their original conditional capture and cfg boundaries.
#[cfg(any(rcc_lse, all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk)))]
struct Targets {
    #[cfg(rcc_lse)]
    lse: Option<LseSysclkSnapshot>,
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    lsi: Option<LsiSysclkSnapshot>,
}

// Admission facts are frozen once. Only the owned-WAIT LSE admission refresh
// replaces lse_admission, at its original position in the Flash/bridge phase.
struct Admitted {
    preserve_hse: bool,
    needs_lsi: bool,
    #[cfg(rcc_lse)]
    first_lsi_request: bool,
    #[cfg(rcc_lse)]
    lse_admission: Option<crate::rcc::lse::Admission>,
}

// Keep the original entry bindings intact; later mutation is carried by phases.
#[allow(unused_mut)]
pub(super) fn configure(
    config: Config,
    cs: critical_section::CriticalSection<'_>,
) -> Result<Clocks, Error> {
    let mut clocks = config.frequencies()?;
    let r = pac::SYSCTRL;
    let old_clock = r.cr0().read();
    let old_sources = r.cr1().read();
    let old_hsi = r.hsi().read();
    let old_hse = r.hse().read();
    let old_lsi = r.lsi().read();
    let old_lse = r.lse().read();
    #[cfg(rcc_lse)]
    let lse_target = (config.sys == Sysclk::LSE).then(|| LseSysclkSnapshot {
        sources: old_sources,
        hsi: old_hsi,
        hse: old_hse,
        lsi: old_lsi,
        lse: old_lse,
        routes: r.cr2().read(),
        interrupts: r.ier().read(),
        mco: r.mco().read(),
    });
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    let lsi_target = (config.sys == Sysclk::LSI).then(|| LsiSysclkSnapshot {
        clock: old_clock,
        sources: old_sources,
        hsi: old_hsi,
        hse: old_hse,
        lsi: old_lsi,
        lse: old_lse,
        routes: r.cr2().read(),
        interrupts: r.ier().read(),
        mco: r.mco().read(),
        factory: unsafe {
            core::ptr::read_volatile(crate::RCC_LSI_FACTORY_TRIM_ADDRESS as *const u16)
        },
        #[cfg(rcc_cw32l011_v1)]
        flash: None,
    });
    #[cfg(all(rcc_cw32l011_v1, rcc_lsi_sysclk))]
    let mut lsi_target = lsi_target;
    #[cfg(all(rcc_cw32l011_v1, rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        // Classify the immutable original entry before generic readiness errors.
        // Only inherited LSE is relevant to the new HSE-off slice's fault check.
        entry.post_observation()?;
        entry.admit()?;
    }
    let monitor_hse = config.hse.is_some() || old_sources.hseen() || old_sources.hseccs();
    let monitor_lse = old_sources.lseen() || old_sources.lseccs();
    #[cfg(rcc_lse)]
    let monitor_lse = monitor_lse || config.lse.is_some();
    check_external_faults(monitor_hse, monitor_lse)?;
    // Frequency legality is an entry precondition; STABLE only proves startup.
    // Every HSI divider encoding is legal on these own register versions.
    let entry_ready = match old_clock.sysclk() {
        ClockSource::Hsi => old_sources.hsien() && old_hsi.stable(),
        ClockSource::Hse => old_sources.hseen() && old_hse.stable(),
        ClockSource::Lsi => old_lsi.stable(),
        ClockSource::Lse => old_sources.lseen() && old_lse.stable(),
        _ => return Err(Error::InvalidClockSource),
    };
    if !entry_ready {
        return Err(Error::InvalidEntryClock);
    }
    if old_sources.hseen() && !old_hse.stable() {
        return Err(Error::InvalidEntryClock);
    }
    if old_sources.hseen()
        && (old_hse.driver().to_bits() > HseDrive::Level7.to_bits()
            || old_hse.pdriver().to_bits() > HseDrive::Level7.to_bits())
    {
        return Err(Error::InvalidHseConfiguration);
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

    // Phase expectations are separate from the immutable original observation.
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    let (
        mut expected_clock,
        mut expected_sources,
        mut expected_hsi,
        mut expected_hsi_stable,
        mut expected_wait,
    ) = (old_clock, old_sources, old_hsi, old_hsi.stable(), None);
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    let (expected_hse, expected_hse_stable) = (old_hse, old_hse.stable());
    #[cfg(all(rcc_cw32l010_v1, rcc_lsi_sysclk))]
    let (mut expected_hse, mut expected_hse_stable) = (expected_hse, expected_hse_stable);
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if let Some(entry) = lsi_target {
        entry.admit()?;
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
        let changing_hsi = needs_trim || !old_sources.hsien() || !old_hsi.stable();
        if changing_hsi && (entry.mco.source() == 3 || entry.interrupts.hsirdy()) {
            return Err(Error::HsiClockInUse);
        }
    }

    let live_lsi = false;
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    let live_lsi = live_lsi || lsi_target.is_some();
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    {
        expected_clock.set_key(0);
        expected_sources.set_key(0);
    }

    let observed = Entry {
        r,
        old_clock,
        old_sources,
        old_hsi,
        old_hse,
        old_lsi,
        old_lse,
        trim,
        needs_trim,
        live_lsi,
        monitor_hse,
        monitor_lse,
    };
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    let mut expected = Expected {
        clock: expected_clock,
        sources: expected_sources,
        hsi: expected_hsi,
        hsi_stable: expected_hsi_stable,
        hse: expected_hse,
        hse_stable: expected_hse_stable,
        wait: expected_wait,
    };
    #[cfg(any(rcc_lse, all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk)))]
    let mut targets = Targets {
        #[cfg(rcc_lse)]
        lse: lse_target,
        #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
        lsi: lsi_target,
    };
    let admitted = admission::admit(
        config,
        cs,
        &observed,
        #[cfg(any(rcc_lse, all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk)))]
        &targets,
        #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
        &expected,
    )?;
    let ccs_unchanged = |after_lse: bool| {
        #[cfg(rcc_lse)]
        let expected_lse = if after_lse { config.lse } else { None };
        #[cfg(rcc_lse)]
        let (lseen, lseccs) = expected_lse
            .map_or((old_sources.lseen(), old_sources.lseccs()), |c| {
                (true, c.monitored())
            });
        #[cfg(not(rcc_lse))]
        let (lseen, lseccs) = {
            let _ = after_lse;
            (old_sources.lseen(), old_sources.lseccs())
        };
        let v = r.cr1().read();
        v.clkccs() == old_sources.clkccs()
            && v.hseccs() == old_sources.hseccs()
            && v.lseccs() == lseccs
            && v.lselock() == old_sources.lselock()
            && v.lseen() == lseen
    };
    let admitted = flash_bridge::bridge(
        config,
        cs,
        &observed,
        #[cfg(any(rcc_lse, all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk)))]
        &mut targets,
        admitted,
        #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
        &mut expected,
    )?;
    calibrate_hsi::calibrate(
        config,
        #[cfg(rcc_lse)]
        cs,
        &observed,
        #[cfg(any(rcc_lse, all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk)))]
        &targets,
        &admitted,
        &ccs_unchanged,
        #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
        &mut expected,
    )?;
    establish_hse::establish(
        config,
        cs,
        &observed,
        #[cfg(all(rcc_cw32l010_v1, rcc_lsi_sysclk))]
        &targets,
        &admitted,
        #[cfg(all(rcc_cw32l010_v1, rcc_lsi_sysclk))]
        &mut expected,
    )?;
    finish::finish(
        config,
        cs,
        clocks,
        &observed,
        #[cfg(any(rcc_lse, all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk)))]
        &targets,
        &admitted,
        &ccs_unchanged,
        #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
        &expected,
    )
}
