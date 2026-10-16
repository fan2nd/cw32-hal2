use super::LinFrameDefinition;

/// Invalid static LIN schedule budget. No UART or timer is accessed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinScheduleError {
    ZeroBaudrate,
    ZeroTimeBase,
    EmptySchedule,
    SlotTooShort { index: usize },
    DurationOverflow,
}
impl core::fmt::Display for LinScheduleError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::ZeroBaudrate => f.write_str("LIN nominal baudrate must be nonzero"),
            Self::ZeroTimeBase => f.write_str("LIN schedule time base must be nonzero"),
            Self::EmptySchedule => f.write_str("LIN schedule must contain an entry"),
            Self::SlotTooShort { index } => write!(f, "LIN schedule slot {index} is too short"),
            Self::DurationOverflow => f.write_str("LIN schedule duration overflow"),
        }
    }
}
impl core::error::Error for LinScheduleError {}

/// Network-declared timing inputs for LIN 2.2A frame-slot budget calculations.
/// Baud is the network nominal rate, not a measured or rounded UART rate.
/// These inputs do not establish physical baud tolerance or executor latency.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LinScheduleTiming {
    nominal_baudrate: u32,
    time_base_us: u32,
    jitter_us: u32,
}
impl LinScheduleTiming {
    pub fn new(
        nominal_baudrate: u32,
        time_base_us: u32,
        jitter_us: u32,
    ) -> Result<Self, LinScheduleError> {
        if nominal_baudrate == 0 { return Err(LinScheduleError::ZeroBaudrate); }
        if time_base_us == 0 { return Err(LinScheduleError::ZeroTimeBase); }
        Ok(Self { nominal_baudrate, time_base_us, jitter_us })
    }

    /// Smallest integer microsecond duration strictly greater than jitter plus
    /// maximum frame duration. Includes 34 nominal header bits, payload and
    /// checksum byte fields, and the specification's 7/5 duration multiplier.
    /// This is schedule admission, not a receiver timeout or measured WCET.
    pub fn minimum_slot_us(self, frame: LinFrameDefinition) -> u64 {
        let bits = 34 + 10 * (frame.data_len() as u64 + 1);
        let numerator = bits * 7 * 1_000_000;
        let denominator = u64::from(self.nominal_baudrate) * 5;
        u64::from(self.jitter_us) + numerator / denominator + 1
    }
}

/// One schedule entry; repeat identifiers are permitted in separate slots.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LinScheduleEntry {
    pub frame: LinFrameDefinition,
    /// Positive integer multiple of the schedule's common time base.
    pub time_base_ticks: u32,
}

/// Immutable, allocation-free static schedule budget validated before use.
///
/// This is a plan, not a transmitting scheduler: it never starts a frame,
/// changes tables, catches up missed slots or retries a response. Network role,
/// diagnostic/event-triggered policies and actual timing remain caller duties.
/// Borrowing excludes mutation of entries while their validated plan is alive.
pub struct LinSchedulePlan<'a> {
    timing: LinScheduleTiming,
    entries: &'a [LinScheduleEntry],
    cycle_us: u64,
}
impl<'a> LinSchedulePlan<'a> {
    pub fn new(
        timing: LinScheduleTiming,
        entries: &'a [LinScheduleEntry],
    ) -> Result<Self, LinScheduleError> {
        if entries.is_empty() { return Err(LinScheduleError::EmptySchedule); }
        let mut cycle_us = 0u64;
        for (index, entry) in entries.iter().enumerate() {
            let duration = u64::from(timing.time_base_us) * u64::from(entry.time_base_ticks);
            if duration < timing.minimum_slot_us(entry.frame) {
                return Err(LinScheduleError::SlotTooShort { index });
            }
            cycle_us = cycle_us.checked_add(duration).ok_or(LinScheduleError::DurationOverflow)?;
        }
        Ok(Self { timing, entries, cycle_us })
    }

    pub const fn cycle_us(&self) -> u64 { self.cycle_us }

    /// Validated entries in order, paired with each allocated slot duration.
    pub fn slots(&self) -> impl Iterator<Item = (LinFrameDefinition, u64)> + '_ {
        self.entries.iter().map(move |entry| {
            (entry.frame, u64::from(self.timing.time_base_us) * u64::from(entry.time_base_ticks))
        })
    }
}
