use super::{Error, I2c, Operation};
use crate::mode::{Async, Blocking};

/// Failures eligible for explicit whole-transaction replay.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum RetryOn {
    AddressNack,
    ArbitrationLoss,
    AddressNackOrArbitrationLoss,
}

/// Opt-in replay policy for the classic seven-bit master.
///
/// Replay can repeat target side effects, including writes completed before a
/// later read-address NACK or arbitration loss. Use only when the target protocol
/// permits replay of the entire operation list. Partial read buffers are not
/// rolled back; only a successful final attempt establishes the full read result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct RetryPolicy {
    /// Additional attempts after the first, 0..=255. Zero still tries once.
    pub retries: u8,
    pub on: RetryOn,
}
impl RetryPolicy {
    fn permits(self, error: Error, attempts: u16) -> bool {
        if attempts > u16::from(self.retries) {
            return false;
        }
        match (self.on, error) {
            (RetryOn::AddressNack | RetryOn::AddressNackOrArbitrationLoss, Error::AddressNack)
            | (RetryOn::ArbitrationLoss | RetryOn::AddressNackOrArbitrationLoss, Error::Arbitration) => true,
            _ => false,
        }
    }
}

/// Terminal error and number of transaction calls made, including validation failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct RetryFailure {
    pub error: Error,
    pub attempts: u16,
}

impl I2c<'_, Blocking> {
    /// Explicitly replay a permitted whole transaction, returning attempt count.
    /// Ordinary blocking_transaction remains single-attempt. Each attempt uses
    /// its existing idle, STOP and error cleanup. No GPIO recovery pulses or
    /// backoff are added. BusBusy, data NACK, timeout, bus/validation/fault errors
    /// terminate immediately. A finite attempt count is not a wall-clock deadline.
    pub fn blocking_transaction_with_retries(
        &mut self,
        address: u8,
        operations: &mut [Operation<'_>],
        policy: RetryPolicy,
    ) -> Result<u16, RetryFailure> {
        let mut attempts = 0;
        loop {
            attempts += 1;
            match self.blocking_transaction(address, operations) {
                Ok(()) => return Ok(attempts),
                Err(error) if policy.permits(error, attempts) => {}
                Err(error) => return Err(RetryFailure { error, attempts }),
            }
        }
    }
}
impl I2c<'_, Async> {
    /// Explicit async whole-transaction replay; see RetryPolicy's side-effect contract.
    /// Every failed attempt completes existing retirement before another begins;
    /// terminal Faulted is never retried. No backoff, deadline or fairness bound
    /// is added. Cancellation uses the current transaction's cleanup, and never
    /// starts a subsequent attempt. Forgetting is handled by the original owner's
    /// retained transaction marker, with no pointer to caller buffers in the ISR.
    pub async fn transaction_with_retries(
        &mut self,
        address: u8,
        operations: &mut [Operation<'_>],
        policy: RetryPolicy,
    ) -> Result<u16, RetryFailure> {
        let mut attempts = 0;
        loop {
            attempts += 1;
            match self.transaction(address, operations).await {
                Ok(()) => return Ok(attempts),
                Err(error) if policy.permits(error, attempts) => {}
                Err(error) => return Err(RetryFailure { error, attempts }),
            }
        }
    }
}
