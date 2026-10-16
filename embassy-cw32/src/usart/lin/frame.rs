/// Invalid LIN 2.2A identifier or response encoding. No hardware is accessed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum LinFrameError {
    InvalidIdentifier,
    IdentifierParity,
    InvalidDataLength,
    DiagnosticDataLength,
    DiagnosticRequiresClassic,
    ChecksumMismatch,
}

impl core::fmt::Display for LinFrameError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::InvalidIdentifier => "LIN identifier must be in 0..=61",
            Self::IdentifierParity => "LIN protected identifier parity mismatch",
            Self::InvalidDataLength => "LIN payload must contain 1..=8 bytes",
            Self::DiagnosticDataLength => "LIN diagnostic payload must contain eight bytes",
            Self::DiagnosticRequiresClassic => "LIN diagnostic response requires classic checksum",
            Self::ChecksumMismatch => "LIN response checksum mismatch",
        })
    }
}
impl core::error::Error for LinFrameError {}

/// Non-reserved six-bit LIN identifier (0..=61), with computed parity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct LinIdentifier(u8);
impl LinIdentifier {
    pub const fn new(identifier: u8) -> Result<Self, LinFrameError> {
        if identifier > 61 {
            Err(LinFrameError::InvalidIdentifier)
        } else {
            Ok(Self(identifier))
        }
    }
    /// Validate the complete protected byte, including both parity bits.
    pub const fn from_protected(protected: u8) -> Result<Self, LinFrameError> {
        let id = Self(protected & 0x3f);
        if id.0 > 61 {
            Err(LinFrameError::InvalidIdentifier)
        } else if id.protected() != protected {
            Err(LinFrameError::IdentifierParity)
        } else {
            Ok(id)
        }
    }
    pub const fn value(self) -> u8 {
        self.0
    }
    pub const fn is_diagnostic(self) -> bool {
        self.0 == 60 || self.0 == 61
    }
    pub const fn protected(self) -> u8 {
        let id = self.0;
        let p0 = (id ^ (id >> 1) ^ (id >> 2) ^ (id >> 4)) & 1;
        let p1 = !((id >> 1) ^ (id >> 3) ^ (id >> 4) ^ (id >> 5)) & 1;
        id | (p0 << 6) | (p1 << 7)
    }
}

/// Selected by the network's frame definition, not inferred from payload bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum LinChecksum {
    Classic,
    Enhanced,
}

/// Immutable owned response bytes, with validated length and appended checksum.
/// Does not encode a break/header or establish scheduling/transport-layer validity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LinResponse {
    identifier: LinIdentifier,
    bytes: [u8; 9],
    data_len: u8,
}
impl LinResponse {
    /// Copy 1..=8 payload bytes; diagnostic IDs require eight and classic checksum.
    /// Network-specific payload length and signal layout remain caller contracts.
    pub fn new(
        identifier: LinIdentifier,
        data: &[u8],
        model: LinChecksum,
    ) -> Result<Self, LinFrameError> {
        if data.is_empty() || data.len() > 8 {
            return Err(LinFrameError::InvalidDataLength);
        }
        if identifier.is_diagnostic() {
            if data.len() != 8 {
                return Err(LinFrameError::DiagnosticDataLength);
            }
            if model != LinChecksum::Classic {
                return Err(LinFrameError::DiagnosticRequiresClassic);
            }
        }
        let mut sum = match model {
            LinChecksum::Classic => 0,
            LinChecksum::Enhanced => u16::from(identifier.protected()),
        };
        for &byte in data {
            sum += u16::from(byte);
            if sum >= 256 {
                sum -= 255;
            }
        }
        let mut bytes = [0; 9];
        bytes[..data.len()].copy_from_slice(data);
        bytes[data.len()] = !(sum as u8);
        Ok(Self {
            identifier,
            bytes,
            data_len: data.len() as u8,
        })
    }
    /// Validate a complete payload-plus-checksum slice supplied by the caller.
    /// This does not collect UART bytes, detect framing or infer checksum mode.
    pub fn from_received(
        identifier: LinIdentifier,
        encoded: &[u8],
        model: LinChecksum,
    ) -> Result<Self, LinFrameError> {
        if !(2..=9).contains(&encoded.len()) {
            return Err(LinFrameError::InvalidDataLength);
        }
        let response = Self::new(identifier, &encoded[..encoded.len() - 1], model)?;
        if response.checksum() != encoded[encoded.len() - 1] {
            return Err(LinFrameError::ChecksumMismatch);
        }
        Ok(response)
    }
    pub const fn identifier(&self) -> LinIdentifier {
        self.identifier
    }
    pub fn data(&self) -> &[u8] {
        &self.bytes[..usize::from(self.data_len)]
    }
    pub fn checksum(&self) -> u8 {
        self.bytes[usize::from(self.data_len)]
    }
    /// Payload followed by checksum; excludes sync and protected identifier.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..usize::from(self.data_len) + 1]
    }
}
