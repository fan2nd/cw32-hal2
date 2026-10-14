# L012 WWDT notification

Build for one exact package, for example:

    cargo build --release --target thumbv6m-none-eabi --no-default-features --features cw32l012c8t6

The example intentionally does not feed the watchdog: running it causes reset.
The early-warning task may never run before that reset. Do not use this as a
late-feed or persistent-storage guarantee. See `docs/l012-wwdt-warning.md`.
