use super::{Error, I2c, Operation};
use crate::mode::{Async, Blocking};

/// Failures eligible for explicit whole-transaction replay.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum RetryOn {
    #[cfg(not(i2c_cw32l012_v1))]
    AddressNack,
    ArbitrationLoss,
    #[cfg(not(i2c_cw32l012_v1))]
    AddressNackOrArbitrationLoss,
    /// L012 cannot identify whether the address or payload was rejected.
    #[cfg(i2c_cw32l012_v1)]
    UnclassifiedNack,
    #[cfg(i2c_cw32l012_v1)]
    UnclassifiedNackOrArbitrationLoss,
}

/// Opt-in replay policy for the selected master backend.
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
            #[cfg(not(i2c_cw32l012_v1))]
            (RetryOn::AddressNack | RetryOn::AddressNackOrArbitrationLoss, Error::AddressNack)
            | (RetryOn::ArbitrationLoss | RetryOn::AddressNackOrArbitrationLoss, Error::Arbitration) => true,
            #[cfg(i2c_cw32l012_v1)]
            (RetryOn::UnclassifiedNack | RetryOn::UnclassifiedNackOrArbitrationLoss, Error::Nack)
            | (RetryOn::ArbitrationLoss | RetryOn::UnclassifiedNackOrArbitrationLoss, Error::Arbitration) => true,
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
    /// backoff are added. BusBusy, timeout, bus/validation/fault errors
    /// terminate immediately. Classic data NACK also terminates; L012 NACK
    /// source is unknown and can be explicitly selected. A finite attempt count is not a wall-clock deadline.
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
    /// terminal Faulted/RecoveryFailed is never retried. No backoff, deadline or fairness bound
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

impl I2c<'_, Blocking> {
    /// Explicit replay with seven-bit or ten-bit addressing.
    /// L012 NACK source is unknown; selecting it permits replay after payload NACK.
    /// Classic low-address-byte NACK is classified as AddressNack.
    /// The side-effect and retirement contract of `RetryPolicy` applies.
    pub fn blocking_transaction_with_address_and_retries(
        &mut self,
        address: super::Address,
        operations: &mut [Operation<'_>],
        policy: RetryPolicy,
    ) -> Result<u16, RetryFailure> {
        let mut attempts = 0;
        loop {
            attempts += 1;
            match self.blocking_transaction_with_address(address, operations) {
                Ok(()) => return Ok(attempts),
                Err(error) if policy.permits(error, attempts) => {}
                Err(error) => return Err(RetryFailure { error, attempts }),
            }
        }
    }
}

impl I2c<'_, Async> {
    /// Async replay with explicit address mode; see `RetryPolicy`.
    /// L012 NACK source is unknown. RecoveryFailed/Faulted is terminal, and cancellation
    /// retires only the current attempt without starting another one.
    pub async fn transaction_with_address_and_retries(
        &mut self,
        address: super::Address,
        operations: &mut [Operation<'_>],
        policy: RetryPolicy,
    ) -> Result<u16, RetryFailure> {
        let mut attempts = 0;
        loop {
            attempts += 1;
            match self.transaction_with_address(address, operations).await {
                Ok(()) => return Ok(attempts),
                Err(error) if policy.permits(error, attempts) => {}
                Err(error) => return Err(RetryFailure { error, attempts }),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replay_budget_is_finite_and_errors_are_not_broadly_retried() {
        for retries in [0, 1, 254, 255] {
            let policy = RetryPolicy {
                retries,
                on: RetryOn::ArbitrationLoss,
            };
            for attempts in 1..=256 {
                assert_eq!(
                    policy.permits(Error::Arbitration, attempts),
                    attempts <= u16::from(retries)
                );
                assert!(!policy.permits(Error::Bus, attempts));
                assert!(!policy.permits(Error::Timeout, attempts));
            }
        }
    }
}
