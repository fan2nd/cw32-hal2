# Explicit I2C replay

`blocking_transaction_with_retries` and `transaction_with_retries` are opt-in
seven-bit wrappers over the existing classic or L012 master. Classic policies
permit address NACK, arbitration loss, or either. L012 policies instead expose
`UnclassifiedNack` and `UnclassifiedNackOrArbitrationLoss`: NACK may come from
an address or payload. Both backends permit arbitration-only replay. All
policies allow 0..255 additional attempts.
Success returns the number of calls; `RetryFailure` returns the final error and
count. Even validation failures count as one call. Ordinary APIs never retry.

The full operation list is replayed. Earlier writes and partial reads are not
rolled back; an address NACK can happen after a completed write at a repeated
START. Only use replay when the target protocol permits repeating all effects.
A failed overall operation can leave partially overwritten read buffers.

Each attempt invokes the existing transaction and cleanup path. Async retirement
finishes before the next attempt; Faulted/RecoveryFailed, BusBusy, bus error,
timeout and invalid inputs terminate immediately. Classic data NACK terminates;
L012 unclassified NACK is eligible only when explicitly selected. Cancellation retires the active
transaction and starts no later attempt. Forgetting retains only the original
owner marker; the ISR stores no caller-buffer pointer.

There is no retry delay, scheduling fairness, GPIO recovery pulse or time bound.
Use application-managed retries when backoff or cross-attempt state is needed.
L012 additionally exposes `blocking_transaction_with_address_and_retries` and
`transaction_with_address_and_retries` for explicit seven/ten-bit addresses.
Every attempt repeats the complete address sequence through the existing
engine. Its FIFO and clock-source restrictions still apply. The wrappers add
no MMIO protocol beyond the existing engines. Local compilation and tests
remain prohibited; attempt boundaries, partial effects and cleanup acceptance
are pending. The example only repeats address probes on a consenting target.
