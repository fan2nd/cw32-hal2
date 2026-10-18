# Sampled signed encoder position

`Qei::track_position(origin, maximum_step)` and the classic ATIM equivalent
consume a decoder into `TrackedQei`. Existing pin, counter, clock and filtering
qualification applies. No register is configured or reset by the conversion.
Invalid parameters return the original decoder. The wrapper prevents safe CNT
reset or period changes while accumulation is active.

`sample` reads CNT once and adds the shortest signed modulo displacement to an
`i64` position. The application must guarantee absolute net movement between
every pair of samples is at most `maximum_step`, strictly below half the period.
The permitted period is 2..65536. A long scheduling gap, complete wrap or excess
motion can alias to an apparently valid small step; software cannot detect all
violations. This is not an IRQ wrap counter, velocity estimate or edge log.

An exact half-period difference is ambiguous. That case, an observed step above
the declared bound, an out-of-range CNT or i64 arithmetic overflow invalidates
tracking. Further samples return `PositionLost` without reading hardware.
`rebase` assigns a chosen signed origin to a current CNT sample and explicitly
abandons prior history. `position` reports only the last successful cached value.
`into_inner` returns the decoder unchanged; Drop uses its existing shutdown.

Direction and wrap flags are not consumed, because their independent/coalescing
observations cannot reconstruct signed wrap counts. With period 65536, samples
65535 then 0 yield +1; samples 0 then 65535 yield -1, subject to the bound.
This is a software arithmetic contract over the [existing encoder](timer-input.md).
Counter-boundary, loss and overflow host tests plus release example links are
recorded in [software acceptance](validation-scope.md). Physical counter behavior
and board scheduling remain hardware acceptance work.
