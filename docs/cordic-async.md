# L012 CORDIC asynchronous completion

Eight IRQ-backed counterparts are added to the existing operations:
`sin_cos_async`, `phase_async`, `hypot_half_async`, `atan_async`,
`sinh_cosh_half_async`, `atanh_half_async`, `ln_async`, `square_root_async`.
Each takes a typed `Binding<CORDIC, cordic::InterruptHandler<T>>` instead of a
poll budget. Ordinary `Cordic::new` and blocking operations remain available.

Own source: [CW32L012 CN manual V1.4](https://www.whxy.com/uploads/files/20260603/CW32L012_UserManual_CN_V1.4.pdf),
2026-06-03, SHA256
`a9e54694a26f03c1f3e3041f40844900168ca2b8e6142f3328671b92e6b7a340`.
Table 5-1 printed p70 identifies dedicated IRQ11. Section 12.4 p158 states
BUSY0 permits result access/new operations and a new start clears old EOC.
Section 12.6.1 p160 specifies IE completion interrupt, hardware-set EOC and
result-read clearing. Own SDK `CORDIC_SetIE` corroborates CSR enable control.
The [original numeric qualification](l012-math-evidence.md) is unchanged.

All input checks, Q1.31 formats, scaling, ITER15/32, COMP0 and operand/result
register order are identical to the blocking variants. There is no Q1.15,
full-quadrant phase, saturation correction, new accuracy or cycle-time claim.

Preparation rejects busy hardware before writes. Idle preparation and ordinary
construction retire stale waiter state before reconfiguration. IE arming and
triggering operand writes share a critical section: the ISR cannot observe an
old EOC between enabling and the new start. The ISR first checks RAM waiting
state, then CSR, masks IE and wakes outside the critical section; it never reads
result registers. The future checks BUSY0 AND EOC1 before reading results.
Completion before waker registration is observed by that same predicate.

Cancellation masks IE and retires the waker, not the hardware operation. A
subsequent operation can return Busy until computation finishes. Owner Drop
retires interrupts first, then retains the gate if busy; idle Drop preserves
the existing gate policy. No reset, abort, DMA, borrowed-memory ISR pointer,
finite deadline or cancellation-result delivery is added. Forgetting may keep
the operation/wait armed; idle reconstruction/new operation retires it.

The ordinary async firmware exercises all eight APIs on both qualified exact
packages. Builds, source checks and host lifecycle models are software evidence
only, not numeric known-answer or silicon validation.
