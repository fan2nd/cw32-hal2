use super::*;

mod sealed {
    pub trait Source {
        fn count(&self) -> u32;
        fn period(&self) -> u32;
    }
}
/// An owned, source-qualified encoder that cannot be reconfigured while tracked.
#[allow(private_bounds)]
pub trait PositionSource: sealed::Source {}
impl<T: InputInstance> sealed::Source for Qei<'_, T> {
    fn count(&self) -> u32 { self.count() }
    fn period(&self) -> u32 { self.period_ticks() }
}
impl<T: InputInstance> PositionSource for Qei<'_, T> {}
#[cfg(atim_classic_input)]
impl<T: ClassicAtimInputInstance> sealed::Source for ClassicAtimQei<'_, T> {
    fn count(&self) -> u32 { self.count() }
    fn period(&self) -> u32 { self.period_ticks() }
}
#[cfg(atim_classic_input)]
impl<T: ClassicAtimInputInstance> PositionSource for ClassicAtimQei<'_, T> {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
pub enum PositionError {
    /// Period must be 2..=65536 and twice maximum_step must be below it.
    InvalidStepBound,
    /// A count outside the configured modulus was observed.
    InvalidCounter,
    /// Exactly half a period has no unique signed interpretation.
    AmbiguousStep,
    /// Observed shortest displacement exceeds the declared bound.
    StepOutOfRange,
    /// The signed accumulated position no longer fits i64.
    PositionOverflow,
    /// An earlier failed sample invalidated tracking; explicitly rebase it.
    PositionLost,
}

/// Signed position accumulated from successive modulo counter samples.
///
/// The application must ensure absolute net displacement between every pair of
/// samples is at most maximum_step, strictly below half a period. Excess motion
/// can alias to a small valid-looking step and is NOT reliably detectable. IRQ
/// latency, task delays and reverse motion must be considered by the application.
/// This is not an interrupt wrap counter, index input or lossless edge history.
///
/// The decoder is consumed so safe code cannot reset CNT or change its period
/// behind the tracker. Sampling reads CNT once; direction and coalescing wrap
/// flags are neither read nor cleared. Any detected sample failure latches loss
/// until rebase. Dropping this wrapper drops the original decoder normally.
pub struct TrackedQei<E: PositionSource> {
    inner: E,
    period: u32,
    maximum_step: u32,
    previous: u32,
    position: i64,
    valid: bool,
}
impl<E: PositionSource> TrackedQei<E> {
    pub(super) fn new(
        inner: E,
        origin: i64,
        maximum_step: u32,
    ) -> Result<Self, (PositionError, E)> {
        let period = inner.period();
        if !(2..=65536).contains(&period)
            || u64::from(maximum_step) * 2 >= u64::from(period)
        {
            return Err((PositionError::InvalidStepBound, inner));
        }
        let previous = inner.count();
        if previous >= period {
            return Err((PositionError::InvalidCounter, inner));
        }
        Ok(Self { inner, period, maximum_step, previous, position: origin, valid: true })
    }

    /// Read one counter sample and update the signed position.
    /// Detected errors preserve the last valid numeric value but invalidate it.
    pub fn sample(&mut self) -> Result<i64, PositionError> {
        if !self.valid {
            return Err(PositionError::PositionLost);
        }
        let count = self.inner.count();
        self.valid = false;
        if count >= self.period {
            return Err(PositionError::InvalidCounter);
        }
        let period = i64::from(self.period);
        let forward = (i64::from(count) - i64::from(self.previous)).rem_euclid(period);
        if forward * 2 == period {
            return Err(PositionError::AmbiguousStep);
        }
        let delta = if forward * 2 > period { forward - period } else { forward };
        if delta.unsigned_abs() > u64::from(self.maximum_step) {
            return Err(PositionError::StepOutOfRange);
        }
        let position = self.position.checked_add(delta).ok_or(PositionError::PositionOverflow)?;
        self.previous = count;
        self.position = position;
        self.valid = true;
        Ok(position)
    }

    /// Last successful position without reading hardware. It can already be stale.
    pub fn position(&self) -> Result<i64, PositionError> {
        if self.valid { Ok(self.position) } else { Err(PositionError::PositionLost) }
    }

    /// Assign a new position to one current CNT sample, abandoning prior history.
    /// Does not reset the hardware counter, change pins or acknowledge flags.
    pub fn rebase(&mut self, origin: i64) -> Result<(), PositionError> {
        let count = self.inner.count();
        self.valid = false;
        if count >= self.period {
            return Err(PositionError::InvalidCounter);
        }
        self.previous = count;
        self.position = origin;
        self.valid = true;
        Ok(())
    }

    /// Return the original decoder without a register write or pin change.
    pub fn into_inner(self) -> E { self.inner }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::cell::Cell;

    struct Counter<'a>(&'a Cell<u32>, u32);
    impl sealed::Source for Counter<'_> {
        fn count(&self) -> u32 {
            self.0.get()
        }
        fn period(&self) -> u32 {
            self.1
        }
    }
    impl PositionSource for Counter<'_> {}

    #[test]
    fn wraps_both_directions_and_latches_loss() {
        let count = Cell::new(98);
        let mut tracker = TrackedQei::new(Counter(&count, 100), 0, 49).ok().unwrap();
        count.set(2);
        assert_eq!(tracker.sample(), Ok(4));
        count.set(97);
        assert_eq!(tracker.sample(), Ok(-1));
        count.set(47);
        assert_eq!(tracker.sample(), Err(PositionError::AmbiguousStep));
        count.set(48);
        assert_eq!(tracker.sample(), Err(PositionError::PositionLost));
        tracker.rebase(10).unwrap();
        count.set(49);
        assert_eq!(tracker.sample(), Ok(11));
    }

    #[test]
    fn rejects_invalid_bounds_samples_and_position_overflow() {
        let count = Cell::new(0);
        for (period, step) in [(1, 0), (65537, 1), (100, 50), (100, u32::MAX)] {
            assert!(matches!(
                TrackedQei::new(Counter(&count, period), 0, step),
                Err((PositionError::InvalidStepBound, _))
            ));
        }
        let mut tracker = TrackedQei::new(Counter(&count, 100), i64::MAX, 2)
            .ok()
            .unwrap();
        count.set(1);
        assert_eq!(tracker.sample(), Err(PositionError::PositionOverflow));
        tracker.rebase(i64::MIN).unwrap();
        count.set(0);
        assert_eq!(tracker.sample(), Err(PositionError::PositionOverflow));
        tracker.rebase(0).unwrap();
        count.set(3);
        assert_eq!(tracker.sample(), Err(PositionError::StepOutOfRange));
        count.set(100);
        assert_eq!(tracker.rebase(0), Err(PositionError::InvalidCounter));
        assert_eq!(tracker.position(), Err(PositionError::PositionLost));
    }
}
