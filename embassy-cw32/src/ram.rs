//! RAM parity diagnostics and the RAM source of the shared FLASH/RAM interrupt.
//!
//! Parity checking is fixed on after power-up on every supported family. The
//! `EN` bit, where present, is read-only status. There is no parity enable,
//! disable, reset, initialization, scrub or error-injection operation here.
//! CPU RAM writes generate parity; this driver never accesses RAM contents.
//!
//! A parity error identifies potentially corrupted memory. Reading diagnostics
//! or acknowledging the flag does not repair data or make execution safe to
//! resume. In particular, stack, heap, vectors and executable RAM may be affected.
//! The manuals promise neither error correction nor a lossless error queue.
//!
//! Construction, release and drop leave all hardware unchanged. Only explicit
//! interrupt configuration and acknowledgment write RAM controller registers.
//! NVIC and FLASH registers are never modified; IRQ3 is shared with FLASH.
use crate::{Peri, pac, peripherals::RAM};

/// The RAM controller's reported absolute address, never dereferenced here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ReportedAddress(u32);
impl ReportedAddress {
    /// The complete address register value, including its high address bits.
    /// This is diagnostic data, not proof of a valid, aligned or safe Rust pointer.
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Observed fixed parity-checking state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum CheckingState {
    /// The read-only EN status bit reports enabled.
    Enabled,
    /// The read-only EN bit unexpectedly reports disabled; there is no setter.
    Disabled,
    /// This family documents always-on parity but has no readable EN status bit.
    FixedEnabled,
}

/// A sequential diagnostic observation, not an atomic hardware snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Status {
    /// Checking state observed in IER.
    pub checking: CheckingState,
    /// RAM's IER.PARITY source mask; independent of the shared NVIC enable bit.
    pub interrupt_enabled: bool,
    /// ISR.PARITY observed before reading ADDR.
    pub error_pending: bool,
    /// ADDR read only when `error_pending` was true.
    /// More errors may arrive between reads; the manual does not specify
    /// first/last-error ordering or address latching while the flag is set.
    pub reported_address: Option<ReportedAddress>,
}

/// Interrupt-mask write did not read back as requested.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct WriteRejected;

/// Exclusive owner of the RAM parity controller, not of the SRAM address space.
pub struct Ram<'d> {
    peripheral: Peri<'d, RAM>,
}
impl<'d> Ram<'d> {
    /// Take the controller token without any register or SRAM write.
    pub fn new(peripheral: Peri<'d, RAM>) -> Self {
        Self { peripheral }
    }

    /// Observe status without acknowledging errors or reading SRAM contents.
    pub fn status(&self) -> Status {
        let ier = pac::RAM.ier().read();
        #[cfg(ram_enable_status)]
        let checking = if ier.en() {
            CheckingState::Enabled
        } else {
            CheckingState::Disabled
        };
        #[cfg(not(ram_enable_status))]
        let checking = CheckingState::FixedEnabled;
        let error_pending = pac::RAM.isr().read().parity();
        Status {
            checking,
            interrupt_enabled: ier.parity(),
            error_pending,
            reported_address: error_pending.then(|| ReportedAddress(pac::RAM.addr().read().addr())),
        }
    }

    /// Read the error flag independently of its interrupt mask.
    pub fn error_pending(&self) -> bool {
        pac::RAM.isr().read().parity()
    }

    /// Read the complete address register, even if no error is currently pending.
    /// Its reset value is 0x2000_0000. With no pending error it may be stale, and
    /// repeated errors have no documented first/last-address ordering guarantee.
    pub fn reported_address(&self) -> ReportedAddress {
        ReportedAddress(pac::RAM.addr().read().addr())
    }

    /// Set only RAM's parity interrupt source mask and verify readback.
    ///
    /// This controls reporting, not parity checking. Enabling with an existing
    /// error may immediately request IRQ3. Arrange a FLASH/RAM shared handler
    /// before unmasking that NVIC vector. FLASH flags and the NVIC enable/pending
    /// state remain the caller's responsibility. Drop preserves this setting.
    pub fn set_interrupt_enabled(&mut self, enabled: bool) -> Result<(), WriteRejected> {
        pac::RAM.ier().modify(|w| w.set_parity(enabled));
        if pac::RAM.ier().read().parity() == enabled {
            Ok(())
        } else {
            Err(WriteRejected)
        }
    }

    /// Acknowledge the latched error with the documented write-zero command.
    ///
    /// Capture diagnostics first. This repairs no data, does not clear FLASH or
    /// shared NVIC pending state, and can coalesce an error arriving concurrently
    /// with acknowledgment. A later read may already show a new error. No SRAM
    /// location, including the reported address, is read or written.
    pub fn acknowledge(&mut self) {
        pac::RAM.icr().write(|w| {
            *w = pac::ram::regs::Icr::write_noop();
            w.set_parity(false);
        });
    }

    /// Return the controller token, preserving interrupt and diagnostic state.
    pub fn release(self) -> Peri<'d, RAM> {
        self.peripheral
    }
}
