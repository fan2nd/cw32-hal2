use super::*;

/// Invalid header or response in an explicitly configured monitored frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum LinAssemblyError {
    InvalidSync,
    Frame(LinFrameError),
}
impl core::fmt::Display for LinAssemblyError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidSync => f.write_str("LIN sync byte must be 0x55"),
            Self::Frame(error) => core::fmt::Display::fmt(error, f),
        }
    }
}
impl core::error::Error for LinAssemblyError {}

#[derive(Clone, Copy)]
enum Phase {
    AwaitBreak,
    Sync,
    Identifier,
    Response(usize),
}

/// Allocation-free assembler for one network-defined identifier and payload length.
///
/// Feed ordered receive events. On UART error, timeout or known data loss, call
/// reset before feeding further bytes. This cannot detect loss, measure deadlines,
/// validate physical break duration or handle diagnostic transport segmentation.
/// A new Break always abandons the previous partial frame. Unselected identifiers
/// and trailing bytes are ignored until the next Break. Output owns its bytes.
pub struct LinFrameAssembler {
    identifier: LinIdentifier,
    data_len: usize,
    model: LinChecksum,
    bytes: [u8; 9],
    phase: Phase,
}
impl LinFrameAssembler {
    /// The length/model must come from the network definition, never the PID.
    /// Applies the same diagnostic constraints as LinResponse before accepting.
    pub fn new(
        identifier: LinIdentifier,
        data_len: usize,
        model: LinChecksum,
    ) -> Result<Self, LinFrameError> {
        if !(1..=8).contains(&data_len) {
            return Err(LinFrameError::InvalidDataLength);
        }
        LinResponse::new(identifier, &[0; 8][..data_len], model)?;
        Ok(Self {
            identifier,
            data_len,
            model,
            bytes: [0; 9],
            phase: Phase::AwaitBreak,
        })
    }

    /// Abandon any partial header/response; require a fresh Break to restart.
    pub fn reset(&mut self) {
        self.phase = Phase::AwaitBreak;
    }

    /// Consume one event, returning a response only after checksum validation.
    /// Every error retires the partial frame. No bytes from it are reused.
    pub fn push(&mut self, event: LinRxEvent) -> Result<Option<LinResponse>, LinAssemblyError> {
        let byte = match event {
            LinRxEvent::Break => {
                self.phase = Phase::Sync;
                return Ok(None);
            }
            LinRxEvent::Data(byte) => byte,
        };
        let phase = self.phase;
        self.phase = Phase::AwaitBreak;
        match phase {
            Phase::AwaitBreak => {}
            Phase::Sync => {
                if byte != 0x55 {
                    return Err(LinAssemblyError::InvalidSync);
                }
                self.phase = Phase::Identifier;
            }
            Phase::Identifier => {
                let identifier = LinIdentifier::from_protected(byte)
                    .map_err(LinAssemblyError::Frame)?;
                if identifier == self.identifier {
                    self.phase = Phase::Response(0);
                }
            }
            Phase::Response(index) => {
                // Only internally generated indices 0..=data_len reach here.
                self.bytes[index] = byte;
                if index == self.data_len {
                    return LinResponse::from_received(
                        self.identifier,
                        &self.bytes[..self.data_len + 1],
                        self.model,
                    )
                    .map(Some)
                    .map_err(LinAssemblyError::Frame);
                }
                self.phase = Phase::Response(index + 1);
            }
        }
        Ok(None)
    }
}
