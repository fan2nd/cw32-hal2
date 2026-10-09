//! 12-bit ADCs with Embassy ownership and package-qualified channels.
//! Blocking APIs cover every backend. Software async reads/scans cover classic ADCs,
//! L010/L011 and L012 ADC1; L012 ADC2 remains blocking on its shared DAC vector.
//!
//! The public hardware-version facade selects the verified classic, ordered-sequence or independent dual
//! converter implementation. Their sample times, divider range, reference and
//! internal-source capabilities reflect the actual selected peripheral.
//! See the selected implementation documentation for electrical requirements.
#[cfg(any(adc_cw32l010_v1, adc_cw32l011_v1))]
mod sequence;
#[cfg(any(adc_cw32l010_v1, adc_cw32l011_v1))]
pub use sequence::*;
#[cfg(adc_cw32l012_v1)]
mod l012;
#[cfg(adc_cw32l012_v1)]
pub use l012::*;
#[cfg(not(any(adc_cw32l010_v1, adc_cw32l011_v1, adc_cw32l012_v1)))]
mod classic;
#[cfg(not(any(adc_cw32l010_v1, adc_cw32l011_v1, adc_cw32l012_v1)))]
pub use classic::*;
