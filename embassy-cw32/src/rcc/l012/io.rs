use super::{pac, Error, Hse, HseMode};

// Waits remain bounded by the caller's CPU-iteration budget while execution
// continues. A predicate may inspect several related hardware fields.
pub(super) fn poll(
    mut ready: impl FnMut() -> bool,
    timeout: u32,
    error: Error,
    monitor_hse: bool,
    monitor_lse: bool,
) -> Result<(), Error> {
    for _ in 0..timeout {
        check_external_faults(monitor_hse, monitor_lse)?;
        if ready() {
            return Ok(());
        }
        core::hint::spin_loop();
    }
    Err(error)
}

pub(super) fn barrier() {
    #[cfg(target_arch = "arm")]
    {
        cortex_m::asm::dsb();
        cortex_m::asm::isb();
    }
}

pub(super) fn set_flash_latency(
    wait: u32,
    timeout: u32,
    monitor_hse: bool,
    monitor_lse: bool,
) -> Result<(), Error> {
    // L012 has FETCH, CACHE and CACHEINVALID plus RFU. Preserve all of them;
    // change only WAIT through FLASH, never the SYSCTRL brake/debug mirror.
    pac::FLASH.cr2().modify(|w| {
        w.set_key(0x5a5a);
        w.set_wait(wait as u8);
    });
    poll(
        || pac::FLASH.cr2().read().wait() == wait as u8,
        timeout,
        Error::FlashLatencyTimeout,
        monitor_hse,
        monitor_lse,
    )?;
    barrier();
    Ok(())
}

pub(super) fn check_external_faults(hse: bool, lse: bool) -> Result<(), Error> {
    let v = pac::SYSCTRL.isr().read();
    if (hse && (v.hsefail() || v.hsefault())) || (lse && (v.lsefail() || v.lsefault())) {
        Err(Error::ExternalClockFault)
    } else {
        Ok(())
    }
}

pub(super) fn poll_clock(
    timeout: u32,
    error: Error,
    hse: bool,
    lse: bool,
    mut ready: impl FnMut() -> bool,
) -> Result<(), Error> {
    for _ in 0..timeout {
        check_external_faults(hse, lse)?;
        if ready() {
            return Ok(());
        }
        core::hint::spin_loop();
    }
    Err(error)
}

pub(super) fn hse_parameters_match(hse: Hse) -> Result<bool, Error> {
    let v = pac::SYSCTRL.hse().read();
    Ok(v.mode() == (hse.mode == HseMode::Bypass)
        && v.driver() == hse.drive
        && v.pdriver() == hse.drive
        && v.waitcycle() == pac::sysctrl::vals::HseWait::Cycles262144
        && !v.digflt()
        && v.detcnt() == hse.detector_count()?)
}
