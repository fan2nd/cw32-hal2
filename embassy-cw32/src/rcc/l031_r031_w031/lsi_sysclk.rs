use super::*;

// Original classification plus an intended-change ledger. This owns no MMIO
// abstraction: the existing native sequence below still performs every source
// transition. Readable identity excludes only WO keys and actual RO STABLE.
#[cfg(rcc_lsi_sysclk)]
pub(super) struct LsiSysclkState {
    pub(super) entry_clock: pac::sysctrl::regs::Cr0,
    pub(super) entry_sources: pac::sysctrl::regs::Cr1,
    pub(super) entry_hsi: pac::sysctrl::regs::Hsi,
    pub(super) clock: pac::sysctrl::regs::Cr0,
    pub(super) sources: pac::sysctrl::regs::Cr1,
    pub(super) hsi: pac::sysctrl::regs::Hsi,
    pub(super) hse: pac::sysctrl::regs::Hse,
    pub(super) lse: pac::sysctrl::regs::Lse,
    lsi: pac::sysctrl::regs::Lsi,
    ier: pac::sysctrl::regs::Ier,
    factory_trim: u16,
    pub(super) hsi_trim: u16,
    cold: bool,
    pub(super) requested: bool,
    ready: Cell<bool>,
    monitor_hse: bool,
    monitor_lse: bool,
    pub(super) consumers: Option<crate::RccFactoryLsiConsumers>,
    pub(super) awt: Option<pac::awt::regs::Cr>,
}

#[cfg(rcc_lsi_sysclk)]
pub(crate) fn lsi_inspection_error(error: crate::rcc::peripheral::ClockInspectionError) -> Error {
    match error {
        crate::rcc::peripheral::ClockInspectionError::EnableFailed {
            restore_failed: false,
        } => Error::LsiGateEnableTimeout,
        crate::rcc::peripheral::ClockInspectionError::EnableFailed {
            restore_failed: true,
        }
        | crate::rcc::peripheral::ClockInspectionError::RestoreFailed => Error::LsiGateRestoreTimeout,
    }
}

#[cfg(rcc_lsi_sysclk)]
impl LsiSysclkState {
    pub(super) fn capture(config: Config) -> Result<Self, Error> {
        let r = pac::SYSCTRL;
        let mut clock = r.cr0().read();
        let mut sources = r.cr1().read();
        let hsi = r.hsi().read();
        let hse = r.hse().read();
        let lse = r.lse().read();
        let lsi = r.lsi().read();
        let mut ier = r.ier().read();
        let flags = r.isr().read();
        let pending = cortex_m::peripheral::NVIC::is_pending(pac::Interrupt::SYSCTRL);
        let entry_ready = match clock.sysclk() {
            ClockSource::Hsi => hsi.stable() && flags.hsistable(),
            ClockSource::Hse => sources.hseen() && hse.stable() && flags.hsestable(),
            ClockSource::Lsi => lsi.stable() && flags.lsistable(),
            ClockSource::Lse => sources.lseen() && lse.stable() && flags.lsestable(),
            _ => return Err(Error::InvalidClockSource),
        };
        if !matches!(hsi.div(), 5 | 6 | 8 | 9 | 11..=15) {
            return Err(Error::InvalidHsiDivider);
        }
        if !entry_ready {
            return Err(Error::InvalidEntryClock);
        }
        // One aligned factory read per source. Erased raw words are rejected
        // before native masking; zero is a permitted LSI calibration value.
        let raw =
            unsafe { core::ptr::read_volatile(crate::RCC_LSI_FACTORY_TRIM_ADDRESS as *const u16) };
        if raw == u16::MAX {
            return Err(Error::InvalidLsiCalibration);
        }
        let mut calibrated = lsi;
        calibrated.set_trim(raw);
        let selected = clock.sysclk() == ClockSource::Lsi;
        let live = sources.lsien() || selected;
        if live && lsi.trim() != calibrated.trim() {
            return Err(Error::LsiCalibrationInUse);
        }
        if lsi.stable() != flags.lsistable() || (!live && lsi.stable()) {
            return Err(Error::LsiClockInUse);
        }
        if (sources.hseccs() || sources.lseccs())
            && !(sources.lsien() && lsi.stable() && flags.lsistable())
        {
            return Err(Error::LsiClockInUse);
        }
        if !live
            && (sources.hseccs() || sources.lseccs() || ier.lsirdy() || flags.lsirdy() || pending)
        {
            return Err(Error::LsiClockInUse);
        }
        let raw_hsi =
            unsafe { core::ptr::read_volatile(crate::RCC_FACTORY_HSI_TRIM_ADDRESS as *const u16) };
        if raw_hsi == u16::MAX {
            return Err(Error::InvalidCalibration);
        }
        let mut calibrated_hsi = hsi;
        calibrated_hsi.set_trim(raw_hsi);
        let monitor_hse =
            config.hse.is_some() || sources.hseen() || clock.sysclk() == ClockSource::Hse;
        let monitor_lse =
            config.lse.is_some() || sources.lseen() || clock.sysclk() == ClockSource::Lse;
        check_external_faults(monitor_hse, monitor_lse)?;
        clock.set_key(0);
        sources.set_key(0);
        ier.set_key(0);
        Ok(Self {
            entry_clock: clock,
            entry_sources: sources,
            entry_hsi: hsi,
            clock,
            sources,
            hsi,
            hse,
            lse,
            lsi,
            ier,
            factory_trim: calibrated.trim(),
            hsi_trim: calibrated_hsi.trim(),
            cold: !live,
            requested: false,
            ready: Cell::new(lsi.stable()),
            monitor_hse,
            monitor_lse,
            consumers: None,
            awt: None,
        })
    }

    pub(super) fn check_identity(&self) -> Result<(), Error> {
        let r = pac::SYSCTRL;
        let mut clock = r.cr0().read();
        let mut sources = r.cr1().read();
        let mut ier = r.ier().read();
        let hsi = r.hsi().read();
        let hse = r.hse().read();
        let lse = r.lse().read();
        let lsi = r.lsi().read();
        let flags = r.isr().read();
        clock.set_key(0);
        sources.set_key(0);
        ier.set_key(0);
        check_external_faults(self.monitor_hse, self.monitor_lse)?;
        if clock.0 != self.clock.0
            || sources.0 != self.sources.0
            || ier.0 != self.ier.0
            || hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK
                != self.hsi.0 & crate::RCC_LSI_HSI_PARAMETERS_MASK
            || hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK
                != self.hse.0 & crate::RCC_LSI_HSE_PARAMETERS_MASK
            || lse.0 & crate::RCC_LSI_LSE_PARAMETERS_MASK
                != self.lse.0 & crate::RCC_LSI_LSE_PARAMETERS_MASK
            || lsi.0 & crate::RCC_LSI_PARAMETERS_MASK != self.lsi.0 & crate::RCC_LSI_PARAMETERS_MASK
            || lsi.stable() != flags.lsistable()
        {
            return Err(Error::LsiClockInUse);
        }
        if self.ready.get() && !lsi.stable() {
            return Err(Error::LsiTimeout);
        }
        if lsi.stable() {
            // Any coherent observation is sticky, including before prepare's
            // polling loop. A later low status never earns a new startup wait.
            self.ready.set(true);
        }
        let selected_ready = match clock.sysclk() {
            ClockSource::Hsi => hsi.stable() && flags.hsistable(),
            ClockSource::Hse => sources.hseen() && hse.stable() && flags.hsestable(),
            ClockSource::Lsi => lsi.stable() && flags.lsistable(),
            ClockSource::Lse => sources.lseen() && lse.stable() && flags.lsestable(),
            _ => false,
        };
        if !selected_ready {
            return Err(Error::InvalidEntryClock);
        }
        if self.cold
            && !self.requested
            && (sources.lsien()
                || clock.sysclk() == ClockSource::Lsi
                || lsi.stable()
                || flags.lsistable()
                || sources.hseccs()
                || sources.lseccs()
                || ier.lsirdy()
                || flags.lsirdy()
                || cortex_m::peripheral::NVIC::is_pending(pac::Interrupt::SYSCTRL))
        {
            return Err(Error::LsiClockInUse);
        }
        if self.requested && !sources.lsien() {
            return Err(Error::LsiConfigurationTimeout);
        }
        Ok(())
    }

    fn check_consumers(
        &self,
        timeout: u32,
        cs: critical_section::CriticalSection<'_>,
    ) -> Result<(), Error> {
        if let Some(expected) = self.consumers {
            let actual =
                crate::rcc_factory_lsi_consumers(timeout, cs, self.monitor_hse, self.monitor_lse)?;
            if actual != expected {
                return Err(Error::LsiClockInUse);
            }
        }
        Ok(())
    }

    pub(super) fn check_target(
        &self,
        timeout: u32,
        cs: critical_section::CriticalSection<'_>,
    ) -> Result<(), Error> {
        self.check_identity()?;
        self.check_consumers(timeout, cs)?;
        self.check_identity()
    }

    pub(super) fn prepare(
        &mut self,
        timeout: u32,
        cs: critical_section::CriticalSection<'_>,
    ) -> Result<(), Error> {
        let r = pac::SYSCTRL;
        if !self.cold {
            return self.wait_ready(timeout);
        }
        for _ in 0..2 {
            self.check_identity()?;
            let consumers =
                crate::rcc_factory_lsi_consumers(timeout, cs, self.monitor_hse, self.monitor_lse)?;
            self.check_identity()?;
            if self.consumers.is_some_and(|previous| previous != consumers) {
                return Err(Error::LsiClockInUse);
            }
            self.consumers = Some(consumers);
        }
        self.check_identity()?;
        let mut expected = self.lsi;
        expected.set_trim(self.factory_trim);
        if self.lsi.trim() != self.factory_trim {
            r.lsi().modify(|w| w.set_trim(self.factory_trim));
        }
        wait_until(timeout, Error::LsiConfigurationTimeout, || {
            r.lsi().read().0 == expected.0 && !r.isr().read().lsistable()
        })?;
        self.lsi = expected;
        // Matching TRIM skips only its write. Both complete passes and this
        // third request-use-edge comparison remain mandatory for cold entry.
        self.check_target(timeout, cs)
    }

    pub(super) fn wait_ready(&mut self, timeout: u32) -> Result<(), Error> {
        for _ in 0..timeout {
            self.check_identity()?;
            let r = pac::SYSCTRL;
            let lsi = r.lsi().read();
            let mirror = r.isr().read().lsistable();
            if lsi.stable() != mirror {
                return Err(Error::LsiClockInUse);
            }
            if lsi.stable() {
                self.ready.set(true);
                return self.check_identity();
            }
            core::hint::spin_loop();
        }
        Err(Error::LsiTimeout)
    }

    pub(super) fn inspect_awt(
        &self,
        timeout: u32,
        cs: critical_section::CriticalSection<'_>,
    ) -> Result<pac::awt::regs::Cr, Error> {
        let info = <crate::peripherals::AWT as crate::rcc::SealedRccPeripheral>::RCC_INFO;
        let gate = info.is_enabled();
        if info.reset_asserted() {
            return Err(Error::LsiClockInUse);
        }
        let buffered = info
            .inspect_for_init(cs, timeout, || {
                if !info.is_enabled() || info.reset_asserted() {
                    return Err(Error::LsiClockInUse);
                }
                let value = pac::AWT.cr().read();
                if !info.is_enabled() || info.reset_asserted() {
                    return Err(Error::LsiClockInUse);
                }
                Ok(value)
            })
            .map_err(lsi_inspection_error)?;
        if info.is_enabled() != gate {
            return Err(Error::LsiGateRestoreTimeout);
        }
        check_external_faults(self.monitor_hse, self.monitor_lse)?;
        if info.reset_asserted() {
            return Err(Error::LsiClockInUse);
        }
        buffered
    }

    pub(super) fn check_hsi_owners(
        &self,
        timeout: u32,
        cs: critical_section::CriticalSection<'_>,
    ) -> Result<(), Error> {
        let awt = self.inspect_awt(timeout, cs)?;
        if self.awt.is_none_or(|original| original.0 != awt.0) {
            return Err(Error::HsiClockInUse);
        }
        if (awt.en() && awt.src() == pac::awt::vals::Source::Hsiosc)
            || (pac::LVD.cr0().read().en()
                && pac::LVD.cr1().read().flten()
                && pac::LVD.cr1().read().fltclk())
        {
            return Err(Error::HsiClockInUse);
        }
        self.check_identity()
    }

    pub(super) fn verify_tree(
        &self,
        config: Config,
        cs: critical_section::CriticalSection<'_>,
    ) -> Result<(), Error> {
        self.check_target(config.timeout, cs)?;
        let awt = self.inspect_awt(config.timeout, cs)?;
        if self.awt.is_none_or(|original| original.0 != awt.0) {
            return Err(Error::LsiClockInUse);
        }
        if self.sources.hseen() && !crate::rcc_hse_pins_match(self.hse.mode(), config.timeout, cs)?
        {
            return Err(Error::HseClockInUse);
        }
        if let Some(lse) = config.lse {
            crate::rcc::lse::verify(lse, cs)?;
        } else if self.sources.lseen()
            && !crate::rcc_lse_pins_match(self.lse.mode(), false, config.timeout, cs)?
        {
            return Err(Error::LseClockInUse);
        }
        self.check_target(config.timeout, cs)?;
        let r = pac::SYSCTRL;
        let flags = r.isr().read();
        if !self.ready.get()
            || !self.sources.lsien()
            || !r.lsi().read().stable()
            || !flags.lsistable()
        {
            return Err(Error::LsiTimeout);
        }
        if !self.sources.hsien() || !r.hsi().read().stable() || !flags.hsistable() {
            return Err(Error::HsiTimeout);
        }
        if self.sources.hseen() && (!r.hse().read().stable() || !flags.hsestable()) {
            return Err(Error::HseTimeout);
        }
        if self.sources.lseen() && (!r.lse().read().stable() || !flags.lsestable()) {
            return Err(Error::LseNotReady);
        }
        check_external_faults(self.monitor_hse, self.monitor_lse)
    }
}

