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

    /// Start a cyclic execution cursor at an application-selected time-base tick.
    /// The cursor makes decisions only; the caller owns the clock and UART.
    pub fn cursor(&self, start_us: u64) -> LinScheduleCursor<'_> {
        LinScheduleCursor {
            timing: self.timing,
            entries: self.entries,
            pending_entries: None,
            index: 0,
            next_us: start_us,
            active_end_us: None,
            last_observed_us: None,
            fault: None,
        }
    }

    /// Validated entries in order, paired with each allocated slot duration.
    pub fn slots(&self) -> impl Iterator<Item = (LinFrameDefinition, u64)> + '_ {
        self.entries.iter().map(move |entry| {
            (entry.frame, u64::from(self.timing.time_base_us) * u64::from(entry.time_base_ticks))
        })
    }
}

/// Schedule execution/admission failure. Timing faults latch until restart.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinScheduleRunError {
    SlotActive,
    NoActiveSlot,
    IncompatibleTiming,
    ClockWentBackwards,
    StartTooLate { index: usize },
    FinishTooLate { index: usize },
    TimeOverflow,
    Aborted,
}
impl core::fmt::Display for LinScheduleRunError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::SlotActive => f.write_str("LIN schedule slot is still active"),
            Self::NoActiveSlot => f.write_str("LIN schedule has no active slot"),
            Self::IncompatibleTiming => f.write_str("LIN schedule timing differs from the active network"),
            Self::ClockWentBackwards => f.write_str("LIN schedule clock went backwards"),
            Self::StartTooLate { index } => write!(f, "LIN slot {index} missed its start budget"),
            Self::FinishTooLate { index } => write!(f, "LIN slot {index} exceeded its duration"),
            Self::TimeOverflow => f.write_str("LIN schedule timestamp overflow"),
            Self::Aborted => f.write_str("LIN schedule was aborted"),
        }
    }
}
impl core::error::Error for LinScheduleRunError {}

/// An admitted slot. These timestamps are application-clock observations, not
/// measured wire edges. Admission does not transmit or authorize a network role.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LinScheduledSlot {
    pub index: usize,
    pub frame: LinFrameDefinition,
    pub scheduled_start_us: u64,
    pub end_us: u64,
}

/// Cyclic absolute-time schedule state with explicit in-flight completion.
///
/// The caller supplies nondecreasing microsecond timestamps from one clock.
/// Start admission uses the declared jitter as an upper bound on software
/// lateness; any subsequent delay to the physical Break must also fit the
/// application's real timing budget. There is no internal timer, UART access,
/// automatic retry, skipped slot or catch-up burst.
pub struct LinScheduleCursor<'a> {
    timing: LinScheduleTiming,
    entries: &'a [LinScheduleEntry],
    pending_entries: Option<&'a [LinScheduleEntry]>,
    index: usize,
    next_us: u64,
    active_end_us: Option<u64>,
    last_observed_us: Option<u64>,
    fault: Option<LinScheduleRunError>,
}
impl<'a> LinScheduleCursor<'a> {
    /// Current slot's nominal start, including while that slot is active.
    pub const fn next_slot_us(&self) -> u64 { self.next_us }

    /// Request a validated table for the next admitted slot. Never interrupts
    /// an active slot or changes its end time. The latest accepted request wins.
    /// Baud, time base and jitter must match; no hardware reconfiguration occurs.
    /// The new table begins at entry zero at the existing next slot boundary.
    pub fn request_plan(&mut self, plan: &LinSchedulePlan<'a>) -> Result<(), LinScheduleRunError> {
        if let Some(error) = self.fault { return Err(error); }
        if plan.timing != self.timing { return Err(LinScheduleRunError::IncompatibleTiming); }
        self.pending_entries = Some(plan.entries);
        Ok(())
    }

    /// Withdraw a queued switch without changing the active table or any fault.
    pub fn cancel_plan_change(&mut self) { self.pending_entries = None; }

    /// Return None before the nominal start. A returned slot remains active
    /// until finish_slot or abort; another start cannot overlap it.
    pub fn start_slot(&mut self, now_us: u64) -> Result<Option<LinScheduledSlot>, LinScheduleRunError> {
        self.observe_time(now_us)?;
        if self.active_end_us.is_some() { return Err(LinScheduleRunError::SlotActive); }
        if now_us < self.next_us { return Ok(None); }
        let (entries, index) = match self.pending_entries {
            Some(entries) => (entries, 0),
            None => (self.entries, self.index),
        };
        if now_us - self.next_us > u64::from(self.timing.jitter_us) {
            return Err(self.fail(LinScheduleRunError::StartTooLate { index }));
        }
        let entry = entries[index];
        let duration = u64::from(self.timing.time_base_us) * u64::from(entry.time_base_ticks);
        let Some(end_us) = self.next_us.checked_add(duration) else {
            return Err(self.fail(LinScheduleRunError::TimeOverflow));
        };
        // Commit only after successful timing admission. Early polls, an active
        // slot, incompatible requests and overflow cannot partially switch.
        self.entries = entries;
        self.index = index;
        self.pending_entries = None;
        self.active_end_us = Some(end_us);
        Ok(Some(LinScheduledSlot {
            index: self.index, frame: entry.frame, scheduled_start_us: self.next_us, end_us,
        }))
    }

    /// Confirm complete frame/bus handling, not merely queued header bytes.
    /// Early finish preserves the original next deadline; finishing at the slot
    /// boundary is allowed, but a later finish latches failure. On transmission
    /// error/cancellation call abort rather than reporting successful completion.
    pub fn finish_slot(&mut self, now_us: u64) -> Result<(), LinScheduleRunError> {
        self.observe_time(now_us)?;
        let end_us = self.active_end_us.ok_or(LinScheduleRunError::NoActiveSlot)?;
        if now_us > end_us {
            return Err(self.fail(LinScheduleRunError::FinishTooLate { index: self.index }));
        }
        self.active_end_us = None;
        self.next_us = end_us;
        self.index = if self.index + 1 == self.entries.len() { 0 } else { self.index + 1 };
        Ok(())
    }

    /// Stop decisions after a transfer error, cancellation or network fault.
    /// Does not stop hardware or discard/retire a transmitted prefix.
    pub fn abort(&mut self) { self.fail(LinScheduleRunError::Aborted); }

    /// Explicitly abandon old state and begin at entry zero. Call only after
    /// application-controlled bus retirement/recovery; this performs no I/O.
    /// A queued table change is discarded; the last admitted table is retained.
    pub fn restart(&mut self, start_us: u64) {
        self.index = 0;
        self.next_us = start_us;
        self.active_end_us = None;
        self.pending_entries = None;
        self.last_observed_us = None;
        self.fault = None;
    }

    fn observe_time(&mut self, now_us: u64) -> Result<(), LinScheduleRunError> {
        if let Some(error) = self.fault { return Err(error); }
        if self.last_observed_us.is_some_and(|last| now_us < last) {
            return Err(self.fail(LinScheduleRunError::ClockWentBackwards));
        }
        self.last_observed_us = Some(now_us);
        Ok(())
    }

    fn fail(&mut self, error: LinScheduleRunError) -> LinScheduleRunError {
        let retained = *self.fault.get_or_insert(error);
        retained
    }
}
