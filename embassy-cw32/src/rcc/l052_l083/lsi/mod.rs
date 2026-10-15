use super::*;
use crate::rcc::peripheral;

mod observe;
mod state;
mod transition;

pub(super) use transition::configure_lsi;

// Exact factory-LSI targets share this bounded state machine. Each family's
// generated collectors retain its own native controls and ownership graph.
#[cfg(all(rcc_lsi_sysclk, rcc_cw32l052_v1))]
use crate::{
    rcc_l052_lsi_configure_hse_pins as rcc_lsi_configure_hse_pins,
    rcc_l052_lsi_configure_lse_pins as rcc_lsi_configure_lse_pins,
    rcc_l052_lsi_consumers as rcc_lsi_consumers,
    rcc_l052_lsi_enable_hse_pins as rcc_lsi_enable_hse_pins,
    rcc_l052_lsi_lse_preflight as rcc_lsi_lse_preflight, rcc_l052_lsi_pads as rcc_lsi_pads,
    RccL052LsiConsumers as RccLsiConsumers, RccL052LsiLseSnapshot as RccLsiLseSnapshot,
    RccL052LsiPads as RccLsiPads, RCC_L052_LSI_CR0_KEY_MASK as LSI_CR0_KEY_MASK,
    RCC_L052_LSI_CR1_KEY_MASK as LSI_CR1_KEY_MASK, RCC_L052_LSI_IER_KEY_MASK as LSI_IER_KEY_MASK,
};
#[cfg(all(rcc_lsi_sysclk, rcc_cw32l083_v1))]
use crate::{
    rcc_l083_lsi_configure_lse_pins as rcc_lsi_configure_lse_pins,
    rcc_l083_lsi_consumers as rcc_lsi_consumers,
    rcc_l083_lsi_enable_hse_pins as rcc_lsi_enable_hse_pins,
    rcc_l083_lsi_lse_preflight as rcc_lsi_lse_preflight, rcc_l083_lsi_pads as rcc_lsi_pads,
    RccL083LsiConsumers as RccLsiConsumers, RccL083LsiLseSnapshot as RccLsiLseSnapshot,
    RccL083LsiPads as RccLsiPads, RCC_L083_LSI_CR0_KEY_MASK as LSI_CR0_KEY_MASK,
    RCC_L083_LSI_CR1_KEY_MASK as LSI_CR1_KEY_MASK, RCC_L083_LSI_IER_KEY_MASK as LSI_IER_KEY_MASK,
};

// Exact L052/L083 factory-LSI target. Keep this path separate from the established
// HSI/HSE and auxiliary/system-LSE paths: admission and owned edges differ.
#[cfg(rcc_lsi_sysclk)]
#[derive(Clone, Copy, Eq, PartialEq)]
enum LsiEntryClass {
    Cold,
    LiveReady,
    LiveStarting,
}

#[cfg(rcc_lsi_sysclk)]
#[derive(Clone, Copy)]
struct LsiIdentity {
    clock: pac::sysctrl::regs::Cr0,
    sources: pac::sysctrl::regs::Cr1,
    ier: pac::sysctrl::regs::Ier,
    hsi: pac::sysctrl::regs::Hsi,
    hse: pac::sysctrl::regs::Hse,
    lse: pac::sysctrl::regs::Lse,
    lsi: pac::sysctrl::regs::Lsi,
    flags: pac::sysctrl::regs::Isr,
    pending: bool,
    #[cfg(rcc_cw32l083_v1)]
    cr2: pac::sysctrl::regs::Cr2,
    #[cfg(rcc_cw32l083_v1)]
    pll: pac::sysctrl::regs::Pll,
    #[cfg(rcc_cw32l083_v1)]
    irq_enabled: bool,
    #[cfg(rcc_cw32l083_v1)]
    fault_irq_enabled: bool,
    #[cfg(rcc_cw32l083_v1)]
    fault_pending: bool,
    #[cfg(rcc_cw32l083_v1)]
    flash_gate: bool,
    #[cfg(rcc_cw32l083_v1)]
    flash_reset: bool,
}

#[cfg(rcc_lsi_sysclk)]
impl LsiIdentity {
    fn read() -> Self {
        let r = pac::SYSCTRL;
        let mut clock = r.cr0().read();
        let mut sources = r.cr1().read();
        let mut ier = r.ier().read();
        clock.0 &= !LSI_CR0_KEY_MASK;
        sources.0 &= !LSI_CR1_KEY_MASK;
        ier.0 &= !LSI_IER_KEY_MASK;
        #[cfg(rcc_cw32l083_v1)]
        let mut cr2 = r.cr2().read();
        #[cfg(rcc_cw32l083_v1)]
        {
            cr2.0 &= !crate::RCC_L083_LSI_CR2_KEY_MASK;
        }
        #[cfg(rcc_cw32l083_v1)]
        let flash = <crate::peripherals::FLASH as crate::rcc::SealedRccPeripheral>::RCC_INFO;
        Self {
            clock,
            sources,
            ier,
            hsi: r.hsi().read(),
            hse: r.hse().read(),
            lse: r.lse().read(),
            lsi: r.lsi().read(),
            flags: r.isr().read(),
            pending: cortex_m::peripheral::NVIC::is_pending(pac::Interrupt::SYSCTRL),
            #[cfg(rcc_cw32l083_v1)]
            cr2,
            #[cfg(rcc_cw32l083_v1)]
            pll: r.pll().read(),
            #[cfg(rcc_cw32l083_v1)]
            irq_enabled: cortex_m::peripheral::NVIC::is_enabled(pac::Interrupt::SYSCTRL),
            #[cfg(rcc_cw32l083_v1)]
            fault_irq_enabled: cortex_m::peripheral::NVIC::is_enabled(pac::Interrupt::CLKFAULT),
            #[cfg(rcc_cw32l083_v1)]
            fault_pending: cortex_m::peripheral::NVIC::is_pending(pac::Interrupt::CLKFAULT),
            #[cfg(rcc_cw32l083_v1)]
            flash_gate: flash.is_enabled(),
            #[cfg(rcc_cw32l083_v1)]
            flash_reset: flash.reset_asserted(),
        }
    }

    fn external_faults(self, hse: bool, lse: bool) -> Result<(), Error> {
        #[cfg(rcc_cw32l083_v1)]
        if (hse || lse) && self.fault_pending {
            return Err(Error::ExternalClockFault);
        }
        if (hse && (self.flags.hsefail() || self.flags.hsefault()))
            || (lse && (self.flags.lsefail() || self.flags.lsefault()))
        {
            Err(Error::ExternalClockFault)
        } else {
            Ok(())
        }
    }

    fn selected_ready(self) -> bool {
        match self.clock.sysclk() {
            ClockSource::Hsi => self.sources.hsien() && self.hsi.stable() && self.flags.hsistable(),
            ClockSource::Hse => self.sources.hseen() && self.hse.stable() && self.flags.hsestable(),
            ClockSource::Lsi => self.sources.lsien() && self.lsi.stable() && self.flags.lsistable(),
            ClockSource::Lse => self.sources.lseen() && self.lse.stable() && self.flags.lsestable(),
            #[cfg(rcc_cw32l083_v1)]
            ClockSource::Pll => self.sources.pllen() && self.pll.stable() && self.flags.pllstable(),
            _ => false,
        }
    }
}

// Each pending value is one explicit owned write. Equality permits its exact
// old or target word until acknowledgment, never an arbitrary field encoding.
// Readiness permissions are phase-local; none relax the permanent LSI latch.
#[cfg(rcc_lsi_sysclk)]
#[derive(Clone, Copy)]
enum LsiTransition {
    Steady,
    #[cfg(rcc_cw32l083_v1)]
    PllStopping(pac::sysctrl::regs::Cr1),
    #[cfg(rcc_cw32l083_v1)]
    FlashWait(pac::sysctrl::regs::Cr2, pac::flash::regs::Cr2),
    #[cfg(rcc_cw32l083_v1)]
    HsePad(bool, u8),
    Clock(pac::sysctrl::regs::Cr0),
    LsiTrim(pac::sysctrl::regs::Lsi),
    LsiRequest(pac::sysctrl::regs::Cr1),
    LsiReady,
    HsiStopRequest(pac::sysctrl::regs::Cr1),
    HsiStopped,
    HsiTrim(pac::sysctrl::regs::Hsi),
    HsiRequest(pac::sysctrl::regs::Cr1),
    HsiReady,
    HsiDivider(pac::sysctrl::regs::Hsi),
    HseParameters(pac::sysctrl::regs::Hse),
    HseRequest(pac::sysctrl::regs::Cr1),
    HseReady,
    LseParameters(pac::sysctrl::regs::Lse),
    LseRequest(pac::sysctrl::regs::Cr1),
    LseReady,
}

#[cfg(rcc_lsi_sysclk)]
struct LsiSysclkState {
    // This record and class are immutable, including through owned pad changes.
    entry: LsiIdentity,
    class: LsiEntryClass,
    factory_lsi: u16,
    factory_hsi: u16,
    clock: pac::sysctrl::regs::Cr0,
    sources: pac::sysctrl::regs::Cr1,
    hsi: pac::sysctrl::regs::Hsi,
    hse: pac::sysctrl::regs::Hse,
    lse: pac::sysctrl::regs::Lse,
    lsi: pac::sysctrl::regs::Lsi,
    // HSI, HSE, LSE, LSI. A true observation is sticky except owned HSI stop.
    ready: [bool; 4],
    requested: bool,
    lsirdy_seen: bool,
    pending_seen: bool,
    cold_lse_requested: bool,
    lse_config: Option<crate::rcc::Lse>,
    lse_pads_configured: bool,
    lse_requested: bool,
    lserdy_seen: bool,
    monitor_hse: bool,
    monitor_lse: bool,
    timeout: u32,
    consumers: Option<RccLsiConsumers>,
    pads: Option<RccLsiPads>,
    lse_preflight: Option<RccLsiLseSnapshot>,
    #[cfg(rcc_cw32l083_v1)]
    pll_stopped: bool,
    #[cfg(rcc_cw32l083_v1)]
    pll_ready: bool,
    #[cfg(rcc_cw32l083_v1)]
    pll_bridge: Option<ClockSource>,
    #[cfg(rcc_cw32l083_v1)]
    cr2: pac::sysctrl::regs::Cr2,
    #[cfg(rcc_cw32l083_v1)]
    flash_gate: bool,
    #[cfg(rcc_cw32l083_v1)]
    flash_cr2: Option<pac::flash::regs::Cr2>,
    #[cfg(rcc_cw32l083_v1)]
    events: [bool; 4],
    #[cfg(rcc_cw32l083_v1)]
    event_owned: [bool; 4],
}

#[cfg(rcc_lsi_sysclk)]
pub(crate) fn lsi_inspection_error(error: peripheral::ClockInspectionError) -> Error {
    match error {
        peripheral::ClockInspectionError::EnableFailed {
            restore_failed: false,
        } => Error::LsiGateEnableTimeout,
        peripheral::ClockInspectionError::EnableFailed {
            restore_failed: true,
        }
        | peripheral::ClockInspectionError::RestoreFailed => Error::LsiGateRestoreTimeout,
    }
}
