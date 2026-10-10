# Qualified LSE calendar examples

These two firmware examples target only CW32F030C8T7 and CW32A030C8T7.
Crystal uses PC14/PC15; bypass consumes PC14 and leaves PC15 available.
RCC init requests the source before peripheral tokens are exposed. LseClock
subsequently acquires SYSCTRL and the pins without reconfiguring the oscillator.

The example declares nominal 32768 Hz with **every-cycle** bounds32766..32770Hz
at3.0..3.6V and−20..70°C. Replace these declarations and drive/amplitude settings
with board-qualified data; average oscillator ppm alone is insufficient.
Qualify crystal load, drive, startup and the own datasheet bypass input levels,
duty, pulse widths and edges. Nothing here measures or validates a real board.

The explicit20000000poll budget counts attempts, not milliseconds. Crystal
startup is slow; the published1.5s is typical, not an upper bound. A timeout
retains EN and the permanent pad reservation. Reset before retrying.

Cold provisioning deliberately writes2026-10-09 12:00:00. Replace it with the
intended time, or use Rtc::attach_preserving_state for a retained calendar.
PA4 toggles on observed second transitions. LSE faults make operations fail;
there is no automatic RTC LSI replacement or post-fault elapsed-time guarantee.

Build with cargo build --locked --release --bins --target thumbv6m-none-eabi
--manifest-path examples/lse-clock/Cargo.toml --no-default-features
--features cw32f030c8t7 (or cw32a030c8t7). No firmware execution is claimed.
