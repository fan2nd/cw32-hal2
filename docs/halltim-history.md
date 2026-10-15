# HALLTIM observation history

`start_history` enables persistent CAPIE service on a running L012 HALLTIM with
the existing typed shared-vector binding. The ISR uses the same flag, WIDTH,
STATE and overflow sampling sequence as ordinary capture. Eight copied results
fit in the singleton queue; no owner/buffer pointer is retained. `try_history`
and `next_history` return the oldest record.

When full, the queue retains its prefix and drops newer observations until the
prefix is drained and `HistoryOverflow` is consumed. Subsequent delivery starts
with `FirstSample`, so a reported transition never silently spans that software
gap. The overflow error neither counts losses nor detects pre-ISR overwrites.
Counter overflow remains separately sticky until `restart`.

Cancelling `next_history` removes its waiter but keeps collection active.
Forgetting it can retain a waker; the next wait, IRQ or stop retires it without
retaining caller data. `stop_history` discards queued records and preserves
latest observation state and hardware flags. `stop`, `restart`, ordinary capture
and Drop stop recording. A zero-budget blocking wait does not poll or change
history. Starting again accepts already pending CAPF and does not reset CNT.

The ISR services only HALLTIM and never disables/unpends BTIM3_HALLTIM. Bind
BTIM3's own handler when independently enabled. All queue mutations and source
mask changes share critical sections; wakers are invoked outside them.

The [existing source review](halltim-evidence.md) still bounds every record:
CAPF coalesces, WIDTH can be overwritten, STATE is live and not latched with
WIDTH, and there is no edge counter/channel tag/overcapture indication. This is
a software observation log, not lossless motor feedback or a physical history.
No timing, throughput or hardware validation follows. The new queue and example
have not been compiled or tested.
