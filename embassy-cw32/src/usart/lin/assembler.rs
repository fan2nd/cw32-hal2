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

/// Validated network definition; length and checksum model are never inferred.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LinFrameDefinition {
    identifier: LinIdentifier,
    data_len: usize,
    model: LinChecksum,
}
impl LinFrameDefinition {
    pub fn new(
        identifier: LinIdentifier,
        data_len: usize,
        model: LinChecksum,
    ) -> Result<Self, LinFrameError> {
        if !(1..=8).contains(&data_len) {
            return Err(LinFrameError::InvalidDataLength);
        }
        LinResponse::new(identifier, &[0; 8][..data_len], model)?;
        Ok(Self { identifier, data_len, model })
    }
    pub const fn identifier(self) -> LinIdentifier {
        self.identifier
    }
    pub const fn data_len(self) -> usize {
        self.data_len
    }
    pub const fn checksum_model(self) -> LinChecksum {
        self.model
    }
}

/// Repeated ID in a dispatch table, even when both definitions are identical.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LinDuplicateIdentifier(pub LinIdentifier);
impl core::fmt::Display for LinDuplicateIdentifier {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "duplicate LIN identifier {}", self.0.value())
    }
}
impl core::error::Error for LinDuplicateIdentifier {}

/// Allocation-free assembler for one network-defined identifier and payload length.
///
/// Feed ordered receive events. On UART error, timeout or known data loss, call
/// reset before feeding further bytes. This cannot detect loss, measure deadlines,
/// validate physical break duration or handle diagnostic transport segmentation.
/// A new Break always abandons the previous partial frame. Unselected identifiers
/// and trailing bytes are ignored until the next Break. Output owns its bytes.
pub struct LinFrameAssembler {
    definition: LinFrameDefinition,
    decoder: Decoder,
}
impl LinFrameAssembler {
    pub fn new(
        identifier: LinIdentifier,
        data_len: usize,
        model: LinChecksum,
    ) -> Result<Self, LinFrameError> {
        Ok(Self {
            definition: LinFrameDefinition::new(identifier, data_len, model)?,
            decoder: Decoder::new(),
        })
    }
    /// Abandon any partial header/response; require a fresh Break to restart.
    pub fn reset(&mut self) {
        self.decoder.reset();
    }
    /// Every error retires the partial frame; output owns its validated bytes.
    pub fn push(&mut self, event: LinRxEvent) -> Result<Option<LinResponse>, LinAssemblyError> {
        let definition = self.definition;
        self.decoder.push(event, |id| {
            (id == definition.identifier).then_some(definition)
        })
    }
}

/// Multi-ID receive assembly using a borrowed, immutable network definition table.
///
/// The same event/error/reset contract as LinFrameAssembler applies. At most one
/// response is assembled at a time. Lookup happens after PID validation; unknown
/// IDs are skipped until the next Break. No scheduling or response transmission
/// is performed. An empty table is allowed and selects no responses.
pub struct LinFrameDispatcher<'a> {
    definitions: &'a [LinFrameDefinition],
    decoder: Decoder,
}
impl<'a> LinFrameDispatcher<'a> {
    /// Reject duplicates before retaining the table. Borrowing prevents mutation
    /// of its layouts during a partial response; no heap storage is required.
    pub fn new(definitions: &'a [LinFrameDefinition]) -> Result<Self, LinDuplicateIdentifier> {
        let mut seen = 0u64;
        for definition in definitions {
            let bit = 1u64 << definition.identifier.value();
            if seen & bit != 0 {
                return Err(LinDuplicateIdentifier(definition.identifier));
            }
            seen |= bit;
        }
        Ok(Self { definitions, decoder: Decoder::new() })
    }
    /// Required after UART error, known loss or an application deadline expiry.
    pub fn reset(&mut self) {
        self.decoder.reset();
    }
    pub fn push(&mut self, event: LinRxEvent) -> Result<Option<LinResponse>, LinAssemblyError> {
        let definitions = self.definitions;
        self.decoder.push(event, |id| {
            definitions.iter().find(|entry| entry.identifier == id).copied()
        })
    }
}

#[derive(Clone, Copy)]
enum Phase {
    AwaitBreak,
    Sync,
    Identifier,
    Response { definition: LinFrameDefinition, index: usize },
}
struct Decoder {
    bytes: [u8; 9],
    phase: Phase,
}
impl Decoder {
    fn new() -> Self {
        Self { bytes: [0; 9], phase: Phase::AwaitBreak }
    }
    fn reset(&mut self) {
        self.phase = Phase::AwaitBreak;
    }
    fn push(
        &mut self,
        event: LinRxEvent,
        select: impl FnOnce(LinIdentifier) -> Option<LinFrameDefinition>,
    ) -> Result<Option<LinResponse>, LinAssemblyError> {
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
                if let Some(definition) = select(identifier) {
                    self.phase = Phase::Response { definition, index: 0 };
                }
            }
            Phase::Response { definition, index } => {
                // Validated lengths are 1..=8; internally generated indices
                // are 0..=data_len. Every output byte is written in this frame.
                self.bytes[index] = byte;
                if index == definition.data_len {
                    return LinResponse::from_received(
                        definition.identifier,
                        &self.bytes[..definition.data_len + 1],
                        definition.model,
                    )
                    .map(Some)
                    .map_err(LinAssemblyError::Frame);
                }
                self.phase = Phase::Response { definition, index: index + 1 };
            }
        }
        Ok(None)
    }
}
