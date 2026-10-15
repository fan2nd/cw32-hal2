use super::*;

#[cfg(not(rcc_hse))]
fn switch(source: u8, timeout: u32) -> Result<(), Error> {
    pac::SYSCTRL.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_sysclk(source);
    });
    poll(
        || pac::SYSCTRL.cr0().read().sysclk() == source,
        timeout,
        Error::ClockSwitchTimeout,
    )?;
    barrier();
    Ok(())
}

#[cfg(not(rcc_hse))]
pub(super) fn configure(config: Config) -> Result<Clocks, Error> {
    let clocks = config.frequencies()?;
    let source = pac::SYSCTRL.cr0().read().sysclk();
    let valid_source = matches!(source, 0 | 1 | 3 | 4);
    if !valid_source {
        return Err(Error::InvalidClockSource);
    }
    if !matches!(pac::SYSCTRL.hsi().read().div(), 5 | 6 | 8 | 9 | 11..=15) {
        return Err(Error::InvalidHsiDivider);
    }
    // Factory calibration is a halfword at the source-qualified family address.
    let trim =
        unsafe { core::ptr::read_volatile(crate::RCC_FACTORY_HSI_TRIM_ADDRESS as *const u16) };
    if trim == u16::MAX {
        return Err(Error::InvalidCalibration);
    }
    // Decode the calibration field with the PAC's documented TRIM width.
    let trim = pac::sysctrl::regs::Hsi(u32::from(trim)).trim();
    let hsi = pac::SYSCTRL.hsi().read();
    let needs_trim = hsi.trim() != trim;
    let old_cr1 = pac::SYSCTRL.cr1().read();
    let old_lsi_enable = old_cr1.lsien();
    let needs_bridge = needs_trim;

    // AHBEN is unkeyed on both families. Never inject a key into reserved bits.
    pac::SYSCTRL.ahben().modify(|w| {
        w.set_flash(true);
    });
    poll(
        || pac::SYSCTRL.ahben().read().flash(),
        config.timeout,
        Error::FlashClockTimeout,
    )?;
    // WAIT=2 is documented through 72 MHz, covering legal bootloader clocks.
    // Never decrease latency before the final HCLK has been verified.
    set_flash_latency(crate::RCC_INITIAL_FLASH_WAIT, config.timeout)?;
    // A minimum /4 HCLK guard bounds every legal L052 source below 24 MHz;
    // retain a slower entry divider rather than speeding it up.
    // Avoid /128 unless already selected: LSI /128 makes initialization slow.
    // APB /8 can only slow the bus. No source is changed at this step.
    let guarded_ahb = pac::SYSCTRL
        .cr0()
        .read()
        .hclkprs()
        .max(AHBPrescaler::Div4 as u8);
    pac::SYSCTRL.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_hclkprs(guarded_ahb);
        w.set_pclkprs(APBPrescaler::Div8 as u8);
    });
    poll(
        || {
            let cr0 = pac::SYSCTRL.cr0().read();
            cr0.hclkprs() == guarded_ahb && cr0.pclkprs() == APBPrescaler::Div8 as u8
        },
        config.timeout,
        Error::ClockConfigurationTimeout,
    )?;
    barrier();

    if needs_bridge {
        // UM §4.3.4 forbids live oscillator parameter changes. Use LSI as a
        // temporary bridge even when entered from an external clock: this
        // also prevents its clock-security fallback from restarting HSI.
        // LSI TRIM and WAITCYCLE are left untouched, including if a watchdog
        // or another peripheral had already enabled LSI automatically.
        pac::SYSCTRL.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_lsien(true);
        });
        poll(
            || pac::SYSCTRL.cr1().read().lsien(),
            config.timeout,
            Error::ClockConfigurationTimeout,
        )?;
        poll(
            || pac::SYSCTRL.lsi().read().stable(),
            config.timeout,
            Error::LsiTimeout,
        )?;
        switch(3, config.timeout)?;
    }
    if needs_trim {
        pac::SYSCTRL.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_hsien(false);
        });
        poll(
            || !pac::SYSCTRL.cr1().read().hsien(),
            config.timeout,
            Error::HsiStopTimeout,
        )?;
        poll(
            || !pac::SYSCTRL.hsi().read().stable(),
            config.timeout,
            Error::HsiStopTimeout,
        )?;
        // HSI is stopped and the CPU runs from verified LSI. Preserve all
        // reserved bits; do not invent the nonexistent HSI.WAITCYCLE field
        // mentioned by the manual's generic §4.5.6 prose.
        pac::SYSCTRL.hsi().modify(|w| {
            w.set_trim(trim);
        });
        poll(
            || pac::SYSCTRL.hsi().read().trim() == trim,
            config.timeout,
            Error::ClockConfigurationTimeout,
        )?;
    }

    // Divider-only changes while HSI runs are explicitly allowed by §4.5.2.
    let divider = config.hsi.div as u8;
    pac::SYSCTRL.hsi().modify(|w| {
        w.set_div(divider);
    });
    poll(
        || {
            let hsi = pac::SYSCTRL.hsi().read();
            hsi.div() == divider && hsi.trim() == trim
        },
        config.timeout,
        Error::ClockConfigurationTimeout,
    )?;
    pac::SYSCTRL.cr1().modify(|w| {
        w.set_key(0x5a5a);
        w.set_hsien(true);
    });
    poll(
        || pac::SYSCTRL.cr1().read().hsien(),
        config.timeout,
        Error::ClockConfigurationTimeout,
    )?;
    // STABLE is the current oscillator state. HSIRDY is a clearable edge flag,
    // not a readiness requirement, and may already have been acknowledged.
    poll(
        || pac::SYSCTRL.hsi().read().stable(),
        config.timeout,
        Error::HsiTimeout,
    )?;
    switch(0, config.timeout)?;
    // L052 CLKCCS can force the 8 MHz divider if an external source faults
    // between the pre-switch HSI readback and this switch. Recheck after HSI
    // is selected, when §4.4.3.3 fallback no longer applies to the active source.
    poll(
        || {
            let hsi = pac::SYSCTRL.hsi().read();
            hsi.div() == divider && hsi.trim() == trim
        },
        config.timeout,
        Error::ClockConfigurationTimeout,
    )?;
    poll(
        || pac::SYSCTRL.hsi().read().stable(),
        config.timeout,
        Error::HsiTimeout,
    )?;

    pac::SYSCTRL.cr0().modify(|w| {
        w.set_key(0x5a5a);
        w.set_hclkprs(config.ahb_pre as u8);
        w.set_pclkprs(config.apb_pre as u8);
        w.set_sysclk(0);
    });
    poll(
        || {
            let cr0 = pac::SYSCTRL.cr0().read();
            cr0.hclkprs() == config.ahb_pre as u8
                && cr0.pclkprs() == config.apb_pre as u8
                && cr0.sysclk() == 0
        },
        config.timeout,
        Error::ClockConfigurationTimeout,
    )?;
    barrier();
    // Restore only the software LSI enable bit; hardware-dependent LSI users
    // remain supported and LSI oscillator parameters were never changed.
    if needs_bridge {
        pac::SYSCTRL.cr1().modify(|w| {
            w.set_key(0x5a5a);
            w.set_lsien(old_lsi_enable);
        });
        poll(
            || pac::SYSCTRL.cr1().read().lsien() == old_lsi_enable,
            config.timeout,
            Error::ClockConfigurationTimeout,
        )?;
    }
    let wait = (clocks.hclk_bounds().maximum().0 - 1) / crate::RCC_FLASH_WAIT_STEP_HZ;
    set_flash_latency(wait, config.timeout)?;
    Ok(clocks)
}
