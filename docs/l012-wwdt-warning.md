# L012 window-watchdog early-warning notification

## Qualified contract

This is a notification extension of the existing reset-mode `WindowWatchdog`,
not automatic recovery or a watchdog-feeding policy. It is available only on
CW32L012. After successful `try_unleash`, call
`enable_early_warning(binding)` with a static `WDT => wdg::WindowInterruptHandler`
binding. `wait_early_warning(&mut self)` consumes one coalesced notification or
waits for one. The wait reports `WarningNotEnabled` until enable was attempted;
a readback failure after attempting IE leaves permanent servicing installed.
Prepared or unsuccessfully started owners return `NotRunning`.

IE cannot be cleared until reset. The static servicing marker is published before
IE is written; no failure path, future cancellation, forgotten future or owner
Drop removes it. The handler acknowledges only WWDT's W0C POV flag, even when no
waiter or owner remains. The state stores an owned Waker and booleans, never a
pointer to a user buffer or owner. Clone, drop and wake callbacks run outside
critical sections. The owner and cancelled future remove only the waiter.
The software pending latch survives cancellation and owner Drop; repeated events
coalesce. A completed wait consumes that latch, while `WindowStatus.preoverflow`
reports the separate hardware flag, which the handler may already have cleared.
Repeated enable does not discard notifications or rewrite an already set IE.

No operation feeds automatically, gates/stops/resets the running counter, or
masks/unpends the shared NVIC line. Keep the existing frozen-clock/gate contract
and static IRQ binding intact until reset. Independently enabled IWDT interrupts
need their own handler on the shared WDT vector; this handler never services IWDT.

POV is raised at count 0x40 and reset follows at 0x3f: only one counter tick
remains. Neither interrupt entry nor async task delivery is guaranteed before
reset. This is not a last-gasp persistence guarantee or safe late-feed strategy.
The example deliberately does not feed and will reset if executed on hardware.

## Own-source evidence

Pinned CW32L012 User Manual CN V1.4, SHA-256
`a9e54694a26f03c1f3e3041f40844900168ca2b8e6142f3328671b92e6b7a340`:
§20.3.5 p440 defines POV/reset thresholds and irreversible IE;
§20.6.2 p443 defines IE and §20.6.3 p444 defines sole W0C POV and reserved zero
bits. The interrupt table assigns the shared WDT vector. The existing
`window-watchdog-evidence.md` remains the historical polling qualification.
No behavior is inferred from STM32 watchdog code.

## Validation boundary

The focused example and API probes are ARM software builds, not board tests.
A temporary host harness extracts the actual new IRQ/enable/wait module and
actual owner Drop implementation, substituting register/RCC/IRQ/critical-section
models. It checks inactive dispatch, enable rejection, irreversible IE readback
uncertainty, publication-before-write, repeat enable, orphan/coalesced warnings,
cancellation, forgotten-wait replacement, owner Drop and live IRQ completion.
The model rejects counter writes and status read-modify-write; no elapsed-time,
real interrupt race, shared-vector electrical behavior or reset response is proven.
No hardware or Miri validation was performed for this extension.

Focused acceptance on 2026-10-14: the CW32L012C8T6 example's release ARM build
passed; the unoptimized debug link exceeded the genuine FLASH region and is not
claimed to pass. Four real ARM API probes gave the expected outcomes: correct
binding/sequential waits compiled; overlapping waits failed with E0499, dropping
a borrowed owner with E0505, and a missing static IRQ binding with E0277. Ten
grouped extracted-source lifecycle/register scenarios passed, including explicit
RawWaker clone/drop/wake assertions outside the modeled critical section. These
models do not execute the complete startup path or the public owner's state
guards and do not substitute for hardware validation.
