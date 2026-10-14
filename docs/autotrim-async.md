# AUTOTRIM periodic underflow async extension

This extends only the existing L052/L083 MD=3, AUTO=0, HSIOSC periodic
counter. `AutotrimTimer<'d, T, M = Blocking>` retains `new` and all polling
methods; `new_async` requires a typed interrupt binding and exposes `wait`.
No calibration, trim, FCAP measurement, new clock source or low-power mode
is added. Existing configuration/start/stop and clock qualification remain.

The source basis is the already qualified UD event, local IER.UD and W0C
ICR.UD in [the original counter audit](autotrim-counter-evidence.md).
Its shared-interrupt section establishes IRQ30 AUTOTRIM_LCD and independent
local sources. Both own SDK `AUTOTRIM_ITConfig` implementations modify IER
without stopping the counter; this extension does not copy their ICR RMW.
The existing typed write-noop command still acknowledges only UD.

Bind each enabled client in one vector, for example:

```rust
bind_interrupts!(struct Irqs {
    AUTOTRIM_LCD => autotrim::InterruptHandler<peripherals::AUTOTRIM>,
        lcd::InterruptHandler<peripherals::LCD>;
});
```

The AUTOTRIM handler checks a RAM-backed waiting flag before any MMIO.
It checks enabled/pending UD, masks only UD and takes the registered waker;
waking occurs outside the critical section. It never acknowledges LCD or
other AUTOTRIM flags and never disables or unpends the shared NVIC line.
When no wait is armed it performs no register access, including after gate-off.

`wait` first consumes an already pending UD, otherwise registers its waker
and arms UD in the same critical section. Hardware can set UD during this
section; enabling its request then supplies the wake. The hardware flag remains
pending through the ISR until the future consumes it. Several underflows
coalesce into one event, and an underflow concurrent with acknowledgment can
coalesce too. There is no elapsed-period count or guaranteed event latency.

Dropping a wait masks only UD, removes its waker and preserves counter and
pending flag. Forgetting a wait may leave its request armed but stores no
caller-buffer or future pointer. A later wait or owner Drop retires the stale
waker. Owner Drop retires the waiter before stopping, clearing and gating only
its own block, as before. A stopped counter without pending UD can wait forever;
there is no timeout, source-loss wake or low-power wake guarantee.

Validation is reported separately from source qualification. Ordinary ARM
library/example compilation checks API integration, not event timing or silicon
behavior. No board execution is claimed.
