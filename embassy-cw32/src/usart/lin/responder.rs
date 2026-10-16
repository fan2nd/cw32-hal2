use super::*;

/// Header-based response selection from an immutable, network-owned table.
///
/// Every entry must be exclusively published by this node on the attached bus.
/// A new Break abandons a partial header. A valid Sync/PID selects at most one
/// owned response; unknown IDs and trailing bytes are ignored until another
/// Break. Call reset after UART errors, known loss or a header deadline.
/// This selector does not measure time or validate transceiver echo.
pub struct LinResponder<'a> {
    responses: &'a [LinResponse],
    phase: Phase,
}

#[derive(Clone, Copy)]
enum Phase {
    Break,
    Sync,
    Identifier,
}

impl<'a> LinResponder<'a> {
    /// Empty tables are permitted and select no response. Duplicate IDs are
    /// rejected even if the corresponding payloads are identical.
    pub fn new(responses: &'a [LinResponse]) -> Result<Self, LinDuplicateIdentifier> {
        let mut seen = 0u64;
        for response in responses {
            let bit = 1u64 << response.identifier().value();
            if seen & bit != 0 {
                return Err(LinDuplicateIdentifier(response.identifier()));
            }
            seen |= bit;
        }
        Ok(Self { responses, phase: Phase::Break })
    }

    pub fn reset(&mut self) {
        self.phase = Phase::Break;
    }

    /// Errors retire the partial header. Output owns its payload and checksum.
    pub fn push(&mut self, event: LinRxEvent) -> Result<Option<LinResponse>, LinAssemblyError> {
        let byte = match event {
            LinRxEvent::Break => {
                self.phase = Phase::Sync;
                return Ok(None);
            }
            LinRxEvent::Data(byte) => byte,
        };
        match core::mem::replace(&mut self.phase, Phase::Break) {
            Phase::Break => Ok(None),
            Phase::Sync if byte == 0x55 => {
                self.phase = Phase::Identifier;
                Ok(None)
            }
            Phase::Sync => Err(LinAssemblyError::InvalidSync),
            Phase::Identifier => {
                let id = LinIdentifier::from_protected(byte).map_err(LinAssemblyError::Frame)?;
                Ok(self.responses.iter().find(|response| response.identifier() == id).copied())
            }
        }
    }
}

/// Automatic response admission, header decoding or UART failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinRespondError {
    DuplicateIdentifier(LinDuplicateIdentifier),
    Header(LinAssemblyError),
    Uart(Error),
}
impl core::fmt::Display for LinRespondError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::DuplicateIdentifier(error) => core::fmt::Display::fmt(error, f),
            Self::Header(error) => core::fmt::Display::fmt(error, f),
            Self::Uart(error) => core::fmt::Display::fmt(error, f),
        }
    }
}
impl core::error::Error for LinRespondError {}
