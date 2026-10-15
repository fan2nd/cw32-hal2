use super::*;

// Waits remain bounded by the caller's poll budget; each predicate performs
// one read of the selected PAC register.
pub(super) fn poll(
    mut ready: impl FnMut() -> bool,
    timeout: u32,
    error: Error,
) -> Result<(), Error> {
    for _ in 0..timeout {
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

pub(super) fn set_flash_latency(wait: u32, timeout: u32) -> Result<(), Error> {
    // Both FLASH variants have WAIT only; preserve reserved bits 15:3.
    // SYSCTRL_CR2[6:4] is a mirror; using FLASH avoids writing SWD/brake fields.
    pac::FLASH.cr2().modify(|w| {
        w.set_key(0x5a5a);
        w.set_wait(wait as u8);
    });
    poll(
        || pac::FLASH.cr2().read().wait() == wait as u8,
        timeout,
        Error::FlashLatencyTimeout,
    )?;
    barrier();
    Ok(())
}

#[cfg(rcc_hse)]
pub(super) fn wait_until(
    timeout: u32,
    error: Error,
    ready: impl FnMut() -> bool,
) -> Result<(), Error> {
    poll(ready, timeout, error)
}

// faults are observed without writing ICR or disturbing unrelated events.
// Rejecting stale relevant flags is a deliberate first-batch restriction.
#[cfg(rcc_hse)]
pub(super) fn check_external_faults(hse: bool, lse: bool) -> Result<(), Error> {
    let v = pac::SYSCTRL.isr().read();
    if (hse && (v.hsefail() || v.hsefault())) || (lse && (v.lsefail() || v.lsefault())) {
        Err(Error::ExternalClockFault)
    } else {
        Ok(())
    }
}

#[cfg(rcc_hse)]
pub(super) fn hse_parameters_match(hse: Hse) -> Result<bool, Error> {
    let v = pac::SYSCTRL.hse().read();
    let range = v.freqrange();
    #[cfg(rcc_cw32l052_v1)]
    if v.pdriver() != hse.drive || v.pfreqrange() != hse.range() {
        return Ok(false);
    }
    Ok(v.mode() == (hse.mode == HseMode::Bypass)
        && v.driver() == hse.drive
        && range == hse.range()
        && v.waitcycle() == pac::sysctrl::vals::HseWait::Cycles262144
        && !v.flt()
        && v.detcnt() == hse.detector_count()?)
}

#[cfg(rcc_hse)]
pub(super) fn verify_hse_source(
    hse: Hse,
    timeout: u32,
    cs: critical_section::CriticalSection<'_>,
) -> Result<(), Error> {
    if !pac::SYSCTRL.cr1().read().hseen() || !hse_parameters_match(hse)? {
        return Err(Error::ClockConfigurationTimeout);
    }
    if !pac::SYSCTRL.hse().read().stable() {
        return Err(Error::HseTimeout);
    }
    if !crate::rcc_hse_pins_match(hse.mode == HseMode::Bypass, timeout, cs)? {
        return Err(Error::HsePinConfigurationTimeout);
    }
    check_external_faults(true, false)
}

#[cfg(rcc_pll)]
pub(super) fn verify_pll_reference(
    pll: Pll,
    config: &Config,
    trim: u16,
    cs: critical_section::CriticalSection<'_>,
) -> Result<(), Error> {
    match pll.src {
        PllSource::HSI => {
            let hsi = pac::SYSCTRL.hsi().read();
            if !pac::SYSCTRL.cr1().read().hsien()
                || hsi.div() != config.hsi.div as u8
                || hsi.trim() != trim
            {
                return Err(Error::ClockConfigurationTimeout);
            }
            if !hsi.stable() {
                return Err(Error::HsiTimeout);
            }
            Ok(())
        }
        PllSource::HSE => verify_hse_source(
            config.hse.ok_or(Error::HseNotConfigured)?,
            config.timeout,
            cs,
        ),
    }
}

#[cfg(rcc_pll)]
pub(super) fn pll_parameters_match(pll: Pll, parameters: &PllParameters) -> bool {
    let v = pac::SYSCTRL.pll().read();
    v.source() == parameters.source
        && v.freqin() == parameters.input_range
        && v.mul() == pll.mul
        && v.freqout() == parameters.output_range
        && v.waitcycle().to_bits() == crate::RCC_PLL_STARTUP_ENCODING
        && v.reserved_debug().to_bits() == crate::RCC_PLL_RESERVED_DEBUG_DEFAULT
}

// Only a requested LSE source needs this monitored-source policy. Default None
