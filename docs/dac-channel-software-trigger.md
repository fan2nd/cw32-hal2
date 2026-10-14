# L012 DAC channel borrowing and software loading

This additive extension retains the existing L012 generic/C8T6/C8U6 DAC,
external-pin, board supply/load and ADC2 quiet-peer qualification. It adds no
DMA, waveform generator, interrupt, low-power or calibrated analog behavior.

## Borrowed channels

`Dac::split(&mut self)` returns an optional fixed-channel `DacChannel` for each
constructor-enabled output. Each supports `set(Value)` and `read()`. Disabled
outputs produce None. Handles are neither Copy nor Clone and carry an exclusive
parent-borrow lifetime, without manufacturing two mutable references to the
whole DAC. Their data writes select only their own existing DHR register;
control checks are read-only. Parent use/drop remains rejected while a view is
subsequently used. Views do not reset, gate or disable hardware on Drop.
Forgetting a view stores no pointer in an ISR/hardware and adds no leak-safety
obligation. The original DAC still owns peripheral and pins and performs cleanup.

## Explicit software-trigger mode

`Dac::into_software_triggered` consumes the direct-loading owner into
`SoftwareTriggeredDac`. It selects TEN for its enabled channels and TSEL=Software,
then checks mode readback. Ordinary `Dac::set` remains direct loading; the new
owner instead exposes `stage(channel, value)`, `trigger(channel, polls)`,
`wait_loaded(polls)`, `is_pending()` and `read(channel)`.

The own authority is [CW32L012 CN user manual V1.4](https://www.whxy.com/uploads/files/20260603/CW32L012_UserManual_CN_V1.4.pdf),
2026-06-03, SHA256
`a9e54694a26f03c1f3e3041f40844900168ca2b8e6142f3328671b92e6b7a340`.
Section 26.6 printed p605 states TEN=1 holds DHR until the selected trigger;
TSEL=0 is software. Section 26.10.2 p612 states SWTRIG1/2 self-clear once the
respective DHR value has loaded into DOR (one dac_pclk). Thus the observed clear
is the documented digital-load completion predicate, not analog completion.
The analog settling/startup restrictions in [the original audit](dac-opa.md)
remain unchanged. Register geometry is already qualified in that audit.

Stage and trigger reject if either software command is pending. Trigger writes
a zero-seeded command with only its selected SWTRIG bit, never a read-modify-write
that could reissue the peer command. No pending peer bit is overwritten. Invalid
values/disabled channels/zero trigger budget submit nothing. Poll budgets count
reads, not time. `wait_loaded` only observes; it cannot resubmit an old command.
Timeout retains owner and mode; another stage/trigger is refused while a command
remains pending. Once both clear, staging may resume. No result is a promise of
analog accuracy or stable voltage, and sequential channel triggers are not a
simultaneous update API.

Conversion failure consumes the owner and uses ordinary DAC cleanup. Explicit
Drop also uses the existing output/channel-disable requests, including after a
timeout; it does not assert that a pending hardware pipeline drained or undo an
output already loaded. No reference into caller storage is retained and no DMA
is enabled. There is no conversion back to direct mode or owned token extraction.

All new control writes preserve DMAEN/DMAUDRIE and require their disabled state;
no DAC IRQ is enabled, so the existing ADC2 shared-vector quiet-peer constraint
is preserved. No historical ADC dual-ownership evidence is changed.

Examples `dac_split` and `dac_software_trigger` use the same two physical pads
and board declaration as `dac_steps`. Validation receipts report source/model,
compile and link checks separately. No hardware execution or finite autonomous
DMA-waveform completion is claimed.
