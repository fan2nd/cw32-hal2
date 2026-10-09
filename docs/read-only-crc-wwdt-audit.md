> Historical software-verification references: HAL tests and fixture harnesses cited below were deleted on 2026-10-08. Their reported results apply to the dated source snapshot only; source facts remain unchanged. See [current HAL layout and build scope](hal-production-layout.md).

> Subsequent implementation: see [crc-evidence.md](crc-evidence.md) and [window-watchdog-evidence.md](window-watchdog-evidence.md). L011's June 2026 own manual is now acquired and independently confirms its CRC protocol; the historical missing-manual caveat below is superseded. Software/source verification does not establish silicon behavior.

> Subsequent stage5 correction: F020 FAULT ownership is now SYSCTRL, with compile-time proof. The audit below records the pre-correction finding. CRC/WWDT capability findings remain applicable.

# Read-only CRC and window-watchdog audit

**Next-batch design only. Stage 5 source is frozen.** This audit changes no HAL,
curated register/chip data, generated PAC, tests, examples, or coverage claims.
No target execution or hardware validation was performed. The companion
`read-only-crc-wwdt-audit.json` records all 13 family policies, exact official
source URLs, 99 source/archive SHA-256 records, and calculated reference vectors.
References below use **printed manual page numbers**, not PDF viewer page indices.
L012's PDF viewer numbering is particularly different from its printed numbering.

## Findings that affect implementation

1. F002/F003 have **four** CRC presets, encodings **4–7**. Reusing the current
   eight-preset enum unchanged would expose undocumented modes 0–3.
2. F020 alone combines eight CRC16 presets with native 8/16/32-bit input
   transactions. F030/A030 have those input widths plus two CRC32 presets.
   The other ten families consume **one 8-bit payload per DR write**. A 32-bit
   register container does not mean one transaction feeds four bytes.
3. L010/L011/L012 CRC clock writes require a key. Their peripheral reset
   registers do not. The current unkeyed CRC gate path cannot simply be enabled
   for those families.
4. L052's SDK header/SVD/PAC contain `INIT` at +0x10; manual V1.5 does not list
   or describe it, and its CRC library does not use it. This is unresolved
   documentation coverage, not proof the register is fictitious. Do not expose
   custom seeds or access it through the HAL.
5. All 11 families with WWDT use the same documented counter/prescaler/window
   semantics. L010/L011 have no WWDT. L012 changes its base and gate/reset bank.
6. F020's imported CRC→FAULT association is wrong against its manual. FAULT
   IRQ31 is HSE/LSE run failure under SYSCTRL, not a CRC completion/fault IRQ.
   Correct ownership in a subsequent authorized metadata batch; retain the IRQ
   vector itself. No CRC interrupt API should be inferred from that association.

## CRC family policy

All CRC bases are 0x40023000. CR is +0x00, DR +0x08, RESULT +0x0c.
MODE occupies [3:0]. All documented CR reset values are 4.

| Families | Current register version | Valid MODE | Input payload/transaction | Result |
|---|---|---|---|---|
| F030, A030 | v1 | 0–9 | native 8, 16, 32 | 16 or 32 bits |
| F020 | cw32f020_v1 | 0–7 | native 8, 16, 32 | 16 bits |
| F002, F003 | cw32f002_v1 | 4–7 | 8-bit payload | 16 bits |
| L010, L011, L012 | cw32l010_v1 | 0–7 | 8-bit payload | 16 bits |
| L031, R031, W031, L083 | cw32l031_v1 | 0–7 | 8-bit payload | 16 bits |
| L052 | cw32l052_v1 | 0–7 | 8-bit payload | 16 bits |

### Exact algorithms

`RefIn` reverses bits within each input byte. `RefOut` reverses the width-bit
remainder before XOR-out. These are fixed presets, not independently selectable
hardware parameters. Polynomial values omit their implicit leading bit.

| MODE | Vendor name | Width | Polynomial | Seed | RefIn | RefOut | XOR-out | ASCII `123456789` reference |
|---|---|---|---|---|---|---|---|---|
| 0 | IBM | 16 | 0x8005 | 0 | yes | yes | 0 | 0xbb3d |
| 1 | MAXIM | 16 | 0x8005 | 0 | yes | yes | 0xffff | 0x44c2 |
| 2 | USB | 16 | 0x8005 | 0xffff | yes | yes | 0xffff | 0xb4c8 |
| 3 | MODBUS | 16 | 0x8005 | 0xffff | yes | yes | 0 | 0x4b37 |
| 4 | CCITT | 16 | 0x1021 | 0 | yes | yes | 0 | 0x2189 |
| 5 | CCITT-FALSE | 16 | 0x1021 | 0xffff | no | no | 0 | 0x29b1 |
| 6 | X25 | 16 | 0x1021 | 0xffff | yes | yes | 0xffff | 0x906e |
| 7 | XMODEM | 16 | 0x1021 | 0 | no | no | 0 | 0x31c3 |
| 8 | CRC32 | 32 | 0x04c11db7 | 0xffffffff | yes | yes | 0xffffffff | 0xcbf43926 |
| 9 | CRC32_MPEG2 | 32 | 0x04c11db7 | 0xffffffff | no | no | 0 | 0x0376e6e7 |

The algorithm table is directly documented by every reviewed manual, restricted
to each family's listed modes. **L011 has no manual in the acquired inputs.** Its
own datasheet §4.4 p9 lists all eight CRC16 names; its own SDK crc.h assigns 0–7,
crc.c writes CR once, feeds byte payloads through word-sized DR, and returns a
16-bit result. Its example `Examples/CRC/crc_calc/USER/src/main.c` lines80–131
provides expected results for every mode on bytes 00,11,22,33,44,55,66,77:
E16C,1E93,15D3,EA2C,868F,5CFF,05FC,6DC1. An independent bitwise host calculation
using the table above reproduced all eight comments. This supports the same
preset policy independently of L010, but is not a substitute for an L011
parameter table or a device test. Do not copy that example's generic
“CRC16 / CRC32” heading as a CRC32 capability claim.

Writing CR with the selected mode initializes the preset accumulator; the vendor
example explicitly rewrites it at the beginning of each calculation. A HAL
`reset()` must perform a write even when MODE already reads as the requested
value. Reading RESULT is a non-resetting result read; it must not be followed by
an implicit accumulator reset. The fixed seed is not the same thing as a
power-on RESULT value. Manuals print RESULT reset=0xffff for most variants,
versus 0 for L010/L012; always initialize via CR before calculation and test
empty-message behavior rather than using a POR value as the algorithm seed.

For F020/F030/A030, the manual's equivalence sequence is:

- bytes: 00,11,22,33,44,55,66,77
- halfwords: 0x1100,0x3322,0x5544,0x7766
- words: 0x33221100,0x77665544

These are genuine native-width transactions at the same DR address, processing
low byte first. Keep the PAC's DR8/DR16/DR32 access sizes. On byte-payload
variants, own-family SDKs use a **32-bit bus store containing a zero-extended
byte**, and the register documents DR[7:0], RFU[31:8]. Use that proven path:
`dr().write(...)` with only low eight bits populated. Never reinterpret the
32-bit PAC container as four-byte feeding. Keep native halfword/word feed methods
absent on those chips, or introduce separately named explicit byte-serialization
helpers; do not silently label emulation as native hardware access. Read low
16 bits through the documented RESULT view and zero-extend for the existing
u32 API. F002's field is RESULT16; the other byte-payload variants call it RESULT.

### CRC clock and reset

- Every family uses HCLK and SYSCTRL.AHBEN.CRC bit2, 1=enabled.
- L010/L011/L012 AHBEN[31:16] must be **replaced** with 0x5a5a on each write,
  preserving unrelated low bits. Do not just OR the key into an arbitrary
  previously read high half. Enable and disable both require the key.
- Every family has AHBRST.CRC bit2, **0=in reset, 1=normal**. There is no key in
  these AHBRST fields, including L010/L011/L012.
- The current CR-write `reset()` is an accumulator reset, not an AHBRST pulse.
  Keep those concepts separate. No SYSCTRL reset pulse is needed to start a
  documented fixed-preset calculation. A future constructor should check gate
  readback and held-in-reset state, failing explicitly rather than calculating
  through inaccessible hardware. Changing/resetting the peripheral is only
  valid while owning its singleton. Serialize shared gate read-modify-write.

## WWDT inventory and common engine

F002/F003/F020/F030/A030 use register version v1. L031/L052/L083/R031/W031 use
cw32l031_v1. L012 uses cw32l012_v1. These three PAC modules have identical field
positions; differences in generated descriptions are not behavior differences.

| Property | All WWDT families except L012 | L012 |
|---|---|---|
| Base | 0x40002c00 | 0x40005400 |
| Configuration/work clock | PCLK | PCLK |
| Clock gate | APBEN1 bit4 | APBEN2 bit5, key 0x5a5a in [31:16] |
| Reset | APBRST1 bit4, active-low, unkeyed | APBRST2 bit5, active-low, unkeyed |
| IRQ | WDT external IRQ0, shared with IWDT | same |
| Debug freeze | SYSCTRL.DEBUG bit10, 1=pause | same |
| Reset-cause flag | SYSCTRL.RESETFLAG bit5, write-zero-clear | same |

L010 and L011 must not expose a WindowWatchdog type, WWDT token, gate, or IRQ
handler. Their IWDT window feature is a different peripheral feature and is
outside this WWDT implementation plan.

### Registers and irreversible effects

- CR0 +0x00 reset=0x7f: WCNT[6:0] is the live down-counter; writing reloads it.
  EN[7] is one-way set until reset. Writing zero must not be represented as a
  supported stop operation.
- CR1 +0x04 reset=0x7f: WINR[6:0], PRS[9:7], IE[10]. IE is also one-way set
  until reset, despite manuals labeling its access RW. There is no reversible
  `set_interrupt_enabled(false)` abstraction.
- SR +0x08 reset=0: POV[0], set when counter reaches 0x40; **write zero clears**.
  It is not W1C. Reserved bits remain zero. Generic `modify()` need not be used.
- PRS=0..7 means PCLK divided by 4096,8192,16384,32768,65536,131072,262144,524288.
- Start only after configuring prescaler/window/counter. Once started, keep the
  gate enabled. Never stop, reset, feed, change the clock or clear reset flags
  from Drop. Do not use APBRST as a claimed way around the irreversible-start
  contract: peripheral-reset registers exist, while the WWDT chapter promises
  stop only on reset, and no safe watchdog API needs a reset escape hatch.
- Valid refresh time is **0x40 ≤ live WCNT ≤ WINR**. Current WCNT>WINR at a write
  is an early-refresh reset. WCNT reaches 0x3f at overflow/reset. Writing a new
  WCNT≤0x3f triggers reset. Manuals additionally require **WINR<initial WCNT**.
  Thus use 0x40≤WINR<reload≤0x7f, with reload at least 0x41. Equality at the live
  window boundary is valid; equality of configured window and reload is not a
  documented disable-window option.
- With IE=1, POV at 0x40 raises WDT IRQ0. Only one counter tick remains before
  reset. ISR refresh is documented but needs real interrupt latency analysis;
  no async callback can promise service before reset. IRQ0 must coexist with
  IWDT and inspect the correct peripheral flag.
- Normal Sleep retains WWDT interrupt/reset behavior. DeepSleep **stops the
  counter**; it resumes after wake. A timeout is PCLK-running time, not elapsed
  wall time. Automatic wake clock selection can also change the rate. DEBUG[10]
  controls counting during debug halt; it resets set on the reviewed WWDT
  families. Preserve the user's existing setting and report it if needed.
  Do not claim watchdog coverage during a debug freeze or DeepSleep.

With divisor D=4096×2^PRS and fixed PCLK frequency f:

- Nominal closed interval: D×(reload−window)/f
- Nominal reset interval: D×(reload−0x3f)/f
- Nominal open interval: D×(window−0x3f)/f

The manual's 24 MHz, PRS=1, reload=0x6f, window=0x4f example gives
10.922666… ms until opening and 16.384 ms until reset. These formulas do not
bound MMIO latency, prescaler phase on reload, interrupt preemption, clock
accuracy, debug halts, or sleep. Prescaler phase on refresh is not specified
well enough to promise a wall-time deadline from the Rust call.

## Concrete implementation sequence

### 1. Curated evidence and capability selection

In a new authorized generation batch, annotate CRC presets/payload widths and
WWDT EN/IE/POV semantics in curated `cw32-data/registers`; correct F020 FAULT
ownership in chip-generation inputs and regenerate normalized data/PAC once.
Preserve L052 INIT provenance while marking its semantics unreviewed; the HAL
must not use it. Do not remove it solely because a manual omits it.

Make build.rs assert the exact selected CRC/WWDT register version before enabling
the driver. Emit capabilities for actual hardware, e.g. `crc_32bit`,
`crc_poly_8005`, `crc_input_16bit`, `crc_input_32bit`; use existing actual chip
cfgs for the keyed SYSCTRL policy. WWDT presence comes from reviewed chip/IP
metadata, never a catch-all family name. No `hal_*` enable labels or renamed
chip aliases. F002/F003 valid mode numbers stay 4,5,6,7; never renumber them to
0,1,2,3 merely because that enum has four members.

### 2. Extend CRC through one driver

Keep the file-backed `crc/mod.rs` public entry, with file-backed private backend
or `tests/mod.rs` only where useful. Share ownership, Config, reset, slice feed,
and result logic. Gate individual mode variants and native-width feed methods
by the checked capabilities. Two DR/result adapter forms suffice: the existing
native-width views, and the byte-payload/word-container form. Keyed gate writes
are a separate SYSCTRL policy, not a duplicated algorithm engine.

Keep CRC32 as the existing F030/A030 default; select CCITT for all CRC16-only
families. Keep `read()->u32` for compatibility, zero-extending CRC16. If adding a
fallible constructor for gate/reset-state failures, keep `new` as a thin panic
wrapper consistent with the rest of the HAL. No programmable polynomial,
arbitrary seed/reflection/XOR, INIT writes, DMA, interrupt, CRC32 on F020, or
native-width feeds on byte-only variants are supported by this plan.

### 3. Add a WWDT driver within wdg

Use a real file `wdg/window/mod.rs`, re-export `WindowWatchdog` and uniquely
named config/error/timing types through `wdg/mod.rs` only on WWDT-capable chips.
Do not replace or fork the established independent-watchdog engine. Use
`Peri<'d, WWDT>` (or a sealed instance trait) and the common pac::wwdt type,
with one family gate adapter. Reuse the project's bounded-poll and mock-I/O
patterns, not IWDT's start keys or asynchronous update flags.

Recommended public contract:

- `WindowConfig::from_counts(prescaler, reload, window)` validates the strict
  invariant above and exposes the exact represented counts. Count-based config
  is the primary contract; no false real-time guarantee.
- `WindowTiming::for_intervals(timeout_us, closed_us, frozen_pclk)` may be a
  checked convenience: closed_us>0, closed_us<timeout_us; choose PRS0..7 with
  reset_ticks=ceil(timeout_us×f/(D×1e6)) in 2..64, then closed_ticks=ceil(
  closed_us×f/(D×1e6)) in 1..reset_ticks−1. Set reload=63+reset_ticks and
  window=reload−closed_ticks. Reject values with no representable nonempty
  window. Use u64 throughout and publish the represented rational times.
  This rounding expresses requested nominal intervals; it is not a timing proof.
- `try_new(instance, config)` validates/configures the owned Rust object without
  MMIO. Obtain the selected family's `rcc::try_clocks().pclk`, failing explicitly
  if no verified clock snapshot exists. No invented default PCLK.
- `try_unleash()` enables/readbacks the correct gate, reads CR0/CR1/SR and refuses
  running or non-reset hardware. Never reset a bootloader watchdog to take it
  over. Write CR1 with IE=0 and validated PRS/WINR, verify it, preload WCNT while
  EN=0, verify preload, then write EN=1 plus reload. Mark state Starting before
  the irreversible write, so an uncertain outcome is not retried as fresh.
  Verify EN only after start; exact WCNT equality is invalid once counting.
- A repeated `unleash()` must return AlreadyRunning or no-op without MMIO. Do
  **not** inherit IWDT's repeat-start-is-feed behavior: that can reset WWDT
  inside its closed window.
- `try_pet()` reads CR0 live, verifies running, returns TooEarly without a write
  if WCNT>WINR, and rejects WCNT<0x40. In the open window it writes **EN|reload**
  in one register transaction, not read-modify-write of the decaying counter.
  Serialize the short read/check/write against interrupt preemption. This still
  cannot prevent a counter tick/reset between read and write, especially at
  0x40; document the scheduling margin requirement. Never spin until the window
  opens, silently feed early, or pretend a software check guarantees no reset.
- Provide live counter / window-open / POV polling if useful. First implementation
  keeps IE=0 and does not touch WDT NVIC, SYSCTRL.DEBUG, reset-cause flags, or
  clock/sleep settings. No stop, Drop side effects, takeover, dynamic prescaler,
  always-open-window shortcut, async auto-feeder, or interrupt-disable API.

An early-warning extension is separable: bind the shared WDT IRQ through the
existing multiple-handler `bind_interrupts!` architecture, enable IE once only
after binding, clear POV by writing zero, and never promise an interrupt or
executor will beat the one-tick reset deadline. It should not block delivering
a complete polling driver.

### Embassy adaptation boundary

Pinned Embassy revision f16efeffe37581092ec184718e6fdb1620393214,
`embassy-stm32/src/wdg/mod.rs`, already contains WindowWatchdog and u64 tick
arithmetic. Adapt ownership, instance traits, count formula and API vocabulary.
Do not copy STM32 addresses, register names, version-specific 2/3-bit divider
logic, unconditional `enable_and_reset`, immediate start inside new, or its
window_us=0 / W=T behavior. The CW32 plan intentionally separates preparation
from irreversible start, preserves a nonempty closed window, and checks refresh
state without promising race-free timing. Its independent watchdog remains
separate and continues supporting all 13 families.

## Required tests and release evidence

### Host/source tests

1. Iterate all chip metadata, including exact package parts and family aliases:
   assert the 13 policies in the JSON, supported CRC modes, all transaction
   widths, result widths, gate bank/bit/key and WWDT presence. Assert no L010/L011
   WWDT symbols. Assert F020 CRC has no FAULT association after correction and
   SYSCTRL owns FAULT without changing the IRQ number.
2. CRC mode-discriminant tests must cover 4–7 without assuming enumeration starts
   at zero. Compile-fail tests: IBM/USB etc on F002/F003; CRC32 on every family
   except F030/A030; native word/halfword feed on all byte-payload families.
3. CRC mock-I/O tests: unconditional CR reset write, no reset between slices or
   result reads, correct transaction width/address, zero high bits on byte-payload
   stores, zero-extended CRC16 results, keyed enable and keyed drop/disable,
   preserved unrelated gate bits, held reset/gate failure paths, and **no INIT**.
4. A standalone bitwise reference must validate empty input, ASCII123456789,
   the own-SDK 00..77 examples, all single bytes, split streams, multiple reads,
   reset reuse and native 16/32-bit low-byte-first equivalence. The companion
   JSON records host-calculated expected values; it does not attest hardware.
5. WWDT calculation tests exhaust PRS0..7, reload0x41..0x7f and every valid
   window0x40..reload−1. Check formulas, ceil boundaries, zero/overflow/too-short
   intervals, representability, no empty open window and no W=reload shortcut.
6. WWDT mock sequences verify no MMIO from new; gate key preservation/readback;
   rejection of inherited EN, IE, non-reset config and unexpected SR; CR1 before
   preload before irreversible EN; no config writes after EN; bounded failure
   paths with poisoned Starting state; no post-start WCNT-equality poll.
7. Refresh tests at window+1, window, 0x41,0x40,0x3f and EN=0; early/late failures
   must perform no refresh write. Accepted paths write exactly EN|reload. Include
   tick-between-read/write simulation to ensure documentation never claims the
   race is eliminated. Repeat unleash and Drop must never feed, clear EN/IE,
   gate clocks, reset hardware, or clear reset flags. IRQ extension tests must
   verify W0C and IWDT coexistence before any release claiming IRQ support.
8. Re-run generation determinism, curated/normalized/PAC parity, formatting,
   host tests, compile-fail tests and thumbv6m builds across **all 13** family
   selections plus exact-part regression matrix. No hardware-tested claim follows
   from compilation or these mock/reference tests.

### Required hardware validation, not yet run

CRC: every newly enabled silicon family, every supported mode, empty reset and
split-stream results, native-width equivalence only where documented, keyed
clock lifecycle, read-without-reset, byte-store path, and endianness. L011's
own-SDK vectors and L052's no-INIT path deserve explicit regressions.

WWDT: controlled reset harness for valid feed, early feed, no feed, low reload,
strict boundary equality, repeated start, bootloader-running rejection, and reset
cause; measure PCLK and count transitions rather than assuming nominal HSI.
Observe normal Sleep, DeepSleep stop/resume, debugger freeze both settings,
and wake clock changes. If IRQ support is later added, measure worst-case IRQ
latency versus the single remaining tick and simultaneous IWDT IRQ behavior.

## Primary-reference map

Detailed SHA-256s, URLs and hashes of the reviewed curated/generated metadata
and HAL snapshot are in the companion JSON; these are the exact local
PDFs read for this audit, not guessed newer revisions.

| Family | Manual | CRC | WWDT |
|---|---|---|---|
| F002 | CN V1.4 | §9 pp115–120 | §14 pp193–200 |
| F003 | CN V2.3 | §9 pp117–122 | §15 pp253–260 |
| F020 | CN V1.4 | §10 pp156–161 | §16 pp258–265 |
| F030/A030 | x030 CN V2.5 | §10 pp159–164 | §17 pp320–327 |
| L010 | CN V1.2 | §9 pp131–136 | absent |
| L011 | own DS CN V1.1 + SDK V1.0.3 | DS§4.4 p9, crc.h/c + example | absent |
| L012 | CN V1.4 | §10 pp139–145 | §20 pp436–444 |
| L031 | CN V1.6 | §10 pp151–156 | §17 pp311–318 |
| L052 | CN V1.5 | §10 pp158–163 | §18 pp348–355 |
| L083 | CN V2.0 | §10 pp170–175 | §18 pp360–367 |
| R031 | CN V1.3 | §10 pp153–158 | §17 pp314–321 |
| W031 | CN V1.4 | §10 pp152–157 | §17 pp314–321 |

For all manuals, CRC algorithm table is §x.3.1, transaction width §x.3.2,
initialization example §x.4, and exact register fields §x.6. WWDT sleep/count
behavior is §y.3.1–.3.2, refresh/bounds §y.3.3–.3.4, IRQ/reset §y.3.5, setup
sequence §y.4, and EN/IE/PRS/POV fields §y.6. Normal Sleep versus DeepSleep
IRQ/reset eligibility is also documented by power-mode tables in chapter3.

Clock/reset/debug SYSCTRL registers:

- F002: §4.7.10 AHBEN p59, .11 APBEN1 p60, .13 AHBRST p62,
  .14 APBRST1 p63, .17 DEBUG p66.
- F003: the same section numbers, pp61,62,64,65,68.
- F020: §4.7.12/.13/.15/.16/.19, pp79,80,82,83,86.
- F030/A030: §4.7.12/.13/.15/.16/.19, pp81,82,84,85,88.
- L010: §4.7.11 AHBEN/key p78, .14 AHBRST p81; own L010 SDK gate/reset
  routines independently agree. L011 own sysctrl.c lines681–699 and788–800
  show keyed clock writes and unkeyed reset operations; its own device header
  supplies CRC bit2 in AHBEN/AHBRST.
- L012: §4.7.11 AHBEN/key p56, .13 APBEN2/key p59, .14 AHBRST p60,
  .16 APBRST2 p63, .18 DEBUG p65.
- L031: §4.7.11/.12/.14/.15/.18, pp76,77,79,80,83.
- L052: the same section numbers, pp80,81,83,84,87.
- L083: §4.7.12/.13/.15/.16/.19, pp85,86,89,90,94.
- R031: §4.7.11/.12/.14/.15/.18, pp78,79,81,82,85.
- W031: the same section numbers, pp77,78,80,81,84.

F020 IRQ contradiction: **§4.7.9 p75** expressly routes HSEFAULT/LSEFAULT
to vector47 (external IRQ31); **§5.4 p94**, FAULT row and footnote4, confirms it.
L052 INIT discrepancy: **§10.5 p162 and §10.6 p163** versus the own-SDK
`cw32l052.h` lines1014–1021 and selected SVD/PAC, with no access in crc.c.
