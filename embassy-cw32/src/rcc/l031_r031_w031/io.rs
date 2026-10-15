use super::*;

pub(super) fn barrier() {
    #[cfg(target_arch = "arm")]
    {
        cortex_m::asm::dsb();
        cortex_m::asm::isb();
    }
}

pub(super) fn wait_until(timeout: u32, error: Error, mut ready: impl FnMut() -> bool) -> Result<(), Error> {
    for _ in 0..timeout {
        if ready() {
            return Ok(());
        }
        core::hint::spin_loop();
    }
    Err(error)
}

pub(super) fn set_flash_latency(wait: u32, timeout: u32) -> Result<(), Error> {
    pac::FLASH.cr2().modify(|w| {
        w.set_key(0x5a5a);
        w.set_wait(wait as u8);
    });
    wait_until(timeout, Error::FlashLatencyTimeout, || {
        let r = pac::FLASH.cr2().read();
        u32::from(r.wait()) == wait
    })?;
    barrier();
    Ok(())
}

// STABLE is a startup latch, not a continuous source-valid indication. Sticky
// faults are observed without writing ICR or disturbing unrelated events.
// Rejecting stale relevant flags is a deliberate first-batch restriction.
pub(super) fn check_external_faults(hse: bool, lse: bool) -> Result<(), Error> {
    let v = pac::SYSCTRL.isr().read();
    if (hse && (v.hsefail() || v.hsefault())) || (lse && (v.lsefail() || v.lsefault())) {
        Err(Error::ExternalClockFault)
    } else {
        Ok(())
    }
}

pub(super) fn hse_parameters_match(hse: Hse) -> Result<bool, Error> {
    let v = pac::SYSCTRL.hse().read();
    let range = v.freqrange();
    Ok(v.mode() == (hse.mode == HseMode::Bypass)
        && v.driver() == hse.drive
        && range == hse.range()
        && v.waitcycle() == pac::sysctrl::vals::HseWait::Cycles262144
        && !v.flt()
        && v.detcnt() == hse.detector_count()?)
}

