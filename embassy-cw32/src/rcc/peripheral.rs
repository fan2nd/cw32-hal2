//! Generated peripheral clock ownership, following Embassy's RCC-info boundary.
//!
//! CW32 gates may require a write key and resets are active-low. Shared groups
//! with untracked owners are retained, rather than using STM32's first/last-user
//! reference counting. No STM32 Stop-mode or multi-core policy is assumed.
use critical_section::CriticalSection;

use super::ClockBounds;
use crate::time::Hertz;

pub(crate) trait SealedRccPeripheral {
    const RCC_INFO: RccInfo;
    fn bus_frequency() -> Option<Hertz>;
    fn bus_clock_bounds() -> Option<ClockBounds>;
    fn frequency() -> Option<Hertz>;
    fn kernel_clock_bounds() -> Option<ClockBounds>;
}

/// Peripheral with independently verified clock-gate control.
///
/// This does not imply that every peripheral-local kernel mux is supported.
#[allow(private_bounds)]
pub trait RccPeripheral: SealedRccPeripheral + 'static {}

/// The peripheral clock gate did not acknowledge its requested state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ClockError;

#[derive(Clone, Copy)]
pub(crate) enum Readback {
    None,
    Once,
    Poll { attempts: u32, spin: bool },
}

/// Controller protocol selected by the existing driver lifecycle contract.
#[derive(Clone, Copy)]
pub(crate) struct RccPolicy {
    pub(crate) enable_readback: Readback,
    pub(crate) disable_readback: Readback,
    pub(crate) reset_readback: bool,
    pub(crate) allow_reset: bool,
    pub(crate) allow_disable: bool,
}

/// Verified SYSCTRL register identities, generated from the selected PAC IR.
#[derive(Clone, Copy)]
pub(crate) struct RccInfo {
    enable: (u8, u8),
    reset: Option<(u8, u8)>,
    key: Option<(u32, u32)>,
    enable_active: bool,
    reset_asserted: bool,
    policy: RccPolicy,
}

impl RccInfo {
    /// # Safety
    /// Offsets are aligned 32-bit words within SYSCTRL and refer to readable,
    /// writable controller registers. Bits and optional write-key mask/value
    /// must match the selected peripheral's independently verified metadata.
    pub(crate) const unsafe fn new(
        enable: (u8, u8),
        reset: Option<(u8, u8)>,
        key: Option<(u32, u32)>,
        enable_active: bool,
        reset_asserted: bool,
        policy: RccPolicy,
    ) -> Self {
        Self {
            enable,
            reset,
            key,
            enable_active,
            reset_asserted,
            policy,
        }
    }

    fn register(self, offset: u8) -> crate::pac::common::Reg<u32, crate::pac::common::RW> {
        // Generated offsets/permissions are checked against the selected SYSCTRL
        // IR. Access is centralized here, as in upstream Embassy's RccInfo.
        unsafe {
            crate::pac::common::Reg::from_ptr(
                crate::pac::SYSCTRL
                    .as_ptr()
                    .cast::<u32>()
                    .add(usize::from(offset)),
            )
        }
    }

    pub(crate) fn is_enabled(self) -> bool {
        let value = self.register(self.enable.0).read();
        (value & (1 << self.enable.1) != 0) == self.enable_active
    }

    pub(crate) fn reset_asserted(self) -> bool {
        self.reset.is_some_and(|(offset, bit)| {
            (self.register(offset).read() & (1 << bit) != 0) == self.reset_asserted
        })
    }

    fn set_clock(self, enabled: bool, readback: Readback) -> Result<(), ClockError> {
        let gate = self.register(self.enable.0);
        gate.modify(|value| {
            let mask = 1 << self.enable.1;
            if enabled == self.enable_active {
                *value |= mask;
            } else {
                *value &= !mask;
            }
            if let Some((mask, key)) = self.key {
                *value = (*value & !mask) | key;
            }
        });
        match readback {
            Readback::None => {}
            Readback::Once => {
                let _ = gate.read();
            }
            Readback::Poll { attempts, spin } => {
                for _ in 0..attempts {
                    if self.is_enabled() == enabled {
                        return Ok(());
                    }
                    if spin {
                        core::hint::spin_loop();
                    }
                }
                return Err(ClockError);
            }
        }
        Ok(())
    }

    /// Enable without asserting or releasing a shared reset.
    pub(crate) fn enable_with_cs(self, _cs: CriticalSection<'_>) -> Result<(), ClockError> {
        self.enable_with_cs_readback(_cs, self.policy.enable_readback)
    }

    pub(crate) fn enable_with_cs_readback(
        self,
        _cs: CriticalSection<'_>,
        readback: Readback,
    ) -> Result<(), ClockError> {
        self.set_clock(true, readback)
    }

    /// Read retained state before RCC initialization without resetting its owner.
    /// The temporary APB configuration gate is restored even when the read
    /// reports an incompatible oscillator. This does not gate the independent
    /// AWT/RTC working source. Only the one-time RCC initializer may call this.
    #[cfg(rcc_external_clock)]
    pub(crate) fn inspect_for_init<R>(
        self,
        _cs: CriticalSection<'_>,
        attempts: u32,
        read: impl FnOnce() -> R,
    ) -> Result<R, ClockError> {
        let enabled = self.is_enabled();
        let readback = Readback::Poll {
            attempts,
            spin: true,
        };
        if !enabled {
            self.set_clock(true, readback)?;
        }
        let result = read();
        if !enabled {
            self.set_clock(false, readback)?;
        }
        Ok(result)
    }

    /// Enable first, then pulse only an independently owned reset field.
    pub(crate) fn enable_and_reset_with_cs(
        self,
        cs: CriticalSection<'_>,
    ) -> Result<(), ClockError> {
        self.enable_with_cs(cs)?;
        if self.policy.allow_reset {
            if let Some((offset, bit)) = self.reset {
                let reset = self.register(offset);
                let mask = 1 << bit;
                // Reset registers are unkeyed. Preserve every neighboring bit.
                reset.modify(|value| {
                    if self.reset_asserted {
                        *value |= mask;
                    } else {
                        *value &= !mask;
                    }
                });
                reset.modify(|value| {
                    if self.reset_asserted {
                        *value &= !mask;
                    } else {
                        *value |= mask;
                    }
                });
                if self.policy.reset_readback {
                    let _ = reset.read();
                }
            }
        }
        Ok(())
    }

    /// Retained groups never gate off, including their last tracked owner.
    pub(crate) fn disable_with_cs(self, _cs: CriticalSection<'_>) -> Result<(), ClockError> {
        self.disable_with_cs_readback(_cs, self.policy.disable_readback)
    }

    pub(crate) fn disable_with_cs_readback(
        self,
        _cs: CriticalSection<'_>,
        readback: Readback,
    ) -> Result<(), ClockError> {
        if self.policy.allow_disable {
            self.set_clock(false, readback)?;
        }
        Ok(())
    }
}

/// Qualified bus frequency, or `None` before successful clock initialization.
pub fn bus_frequency<T: RccPeripheral>() -> Option<Hertz> {
    T::bus_frequency()
}
/// Qualified bus-clock envelope, independent of peripheral-local clock muxes.
pub fn bus_clock_bounds<T: RccPeripheral>() -> Option<ClockBounds> {
    T::bus_clock_bounds()
}
/// Known kernel frequency. Peripheral-local or multiple kernel muxes may return
/// `None`; this function never probes an unclocked peripheral to guess a source.
pub fn frequency<T: RccPeripheral>() -> Option<Hertz> {
    T::frequency()
}
/// Known kernel-clock envelope, subject to the same limits as [`frequency`].
pub fn kernel_clock_bounds<T: RccPeripheral>() -> Option<ClockBounds> {
    T::kernel_clock_bounds()
}

pub(crate) fn enable_and_reset_with_cs<T: RccPeripheral>(
    cs: CriticalSection<'_>,
) -> Result<(), ClockError> {
    T::RCC_INFO.enable_and_reset_with_cs(cs)
}
pub(crate) fn disable_with_cs<T: RccPeripheral>(cs: CriticalSection<'_>) -> Result<(), ClockError> {
    T::RCC_INFO.disable_with_cs(cs)
}
