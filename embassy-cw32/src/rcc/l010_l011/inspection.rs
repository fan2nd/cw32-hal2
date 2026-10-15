use super::*;

// Existing non-target retained-inspection error classes stay unchanged.
pub(super) fn retained_inspection_error(
    config: Config,
    error: crate::rcc::peripheral::ClockInspectionError,
) -> Error {
    #[cfg(all(any(rcc_cw32l010_v1, rcc_cw32l011_v1), rcc_lsi_sysclk))]
    if config.sys == Sysclk::LSI {
        return match error {
            crate::rcc::peripheral::ClockInspectionError::EnableFailed { restore_failed } => {
                Error::LsiConfigurationGateTimeout {
                    enable_failed: true,
                    restore_failed,
                }
            }
            crate::rcc::peripheral::ClockInspectionError::RestoreFailed => {
                Error::LsiConfigurationGateTimeout {
                    enable_failed: false,
                    restore_failed: true,
                }
            }
        };
    }
    #[cfg(rcc_lse)]
    if config.sys == Sysclk::LSE {
        return lse_target_inspection_error(error);
    }
    let _ = (config, error);
    Error::RetainedClockInspectionTimeout
}

#[cfg(rcc_lse)]
pub(super) fn lse_target_inspection_error(
    error: crate::rcc::peripheral::ClockInspectionError,
) -> Error {
    match error {
        crate::rcc::peripheral::ClockInspectionError::EnableFailed { restore_failed } => {
            Error::LseConfigurationGateTimeout {
                enable_failed: true,
                restore_failed,
            }
        }
        crate::rcc::peripheral::ClockInspectionError::RestoreFailed => {
            Error::LseConfigurationGateTimeout {
                enable_failed: false,
                restore_failed: true,
            }
        }
    }
}
