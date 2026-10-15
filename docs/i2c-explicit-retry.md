# Explicit classic I2C replay

`blocking_transaction_with_retries` and `transaction_with_retries` are opt-in
seven-bit wrappers over the existing classic master. `RetryPolicy` permits
address NACK, arbitration loss, or either, with 0..255 additional attempts.
Success returns the number of calls; `RetryFailure` returns the final error and
count. Even validation failures count as one call. Ordinary APIs never retry.

The full operation list is replayed. Earlier writes and partial reads are not
rolled back; an address NACK can happen after a completed write at a repeated
START. Only use replay when the target protocol permits repeating all effects.
A failed overall operation can leave partially overwritten read buffers.

Each attempt invokes the existing transaction and cleanup path. Async retirement
finishes before the next attempt; Faulted, BusBusy, data NACK, bus error, timeout
and invalid inputs terminate immediately. Cancellation retires the active
transaction and starts no later attempt. Forgetting retains only the original
owner marker; the ISR stores no caller-buffer pointer.

There is no retry delay, scheduling fairness, GPIO recovery pulse or time bound.
Use application-managed retries when backoff or cross-attempt state is needed.
L012 command/FIFO mode is excluded. The wrapper adds no MMIO protocol beyond
the [existing classic engine](classic-i2c-async.md). Local compilation and tests
remain prohibited; attempt boundaries, partial effects and cleanup acceptance
are pending. The example only repeats address probes on a consenting target.
