# AUTOTRIM: owned HSIOSC polling counter

Reviewed 2026-10-08 for CW32L052 and CW32L083. The supported safe scope is the
independent periodic down-counter: MD=3, AUTO=0, SRC=0 (raw HSIOSC), in run mode.
It neither searches nor writes oscillator trim. The corresponding
[structured evidence](autotrim-counter-evidence.json) records each own-family
source, exact hash, archive member chain, register map, source conflict and limit.

## Own sources and version boundaries

| Family | Selected CN manual | Corroborating EN manual | Factory tolerance source | SDK |
| --- | --- | --- | --- | --- |
| L052 | [Rev 1.5](https://www.whxy.com/uploads/files/20240920/CW32L052_UserManual_CN_V1.5.pdf), §11 printed pp164–178 / PDF165–179 | [Rev 1.0](https://www.whxy.com/uploads/files/20240920/CW32L052_UserManual_EN_V1.0.pdf), §11 printed pp175–190; revision history 2023-06-20 | [DS Rev 1.3](https://www.whxy.com/uploads/files/20251229/CW32L052_DataSheet_CN_V1.3.pdf), §7.3.8 Tables7-17/18 printed p50 / PDF51 | [V1.4](https://www.whxy.com/uploads/files/20260309/CW32L052_StandardPeripheralLib_V1.4.zip) |
| L083 | [Rev 2.0](https://www.whxy.com/uploads/files/20240920/CW32L083_UserManual_CN_V2.0.pdf), §11 printed pp176–190 / PDF177–191 | [Rev 1.0](https://www.whxy.com/uploads/files/20240923/CW32L083_UserManual_EN_V1.0.pdf), §11 printed pp188–203; revision history 2022-10-10 | [DS Rev 1.9](https://www.whxy.com/uploads/files/20251229/CW32L083_DataSheet_CN_V1.9.pdf), §7.3.8 Tables7-17/18 printed p54 / PDF55 | [V2.2](https://www.whxy.com/uploads/files/20240821/CW32L083_StandardPeripheralLib_V2.2.zip) |

PDF page numbers are one-based. The URL date is not a printed document revision
or acquisition date. The canonical URL/hash/member lock is
[`sources/evidence-sources.json`](../sources/evidence-sources.json); generated
source-lock and catalog files are compatibility views. Own AUTOTRIM headers and
implementations, RCC/device/consumer headers, and timer/calibration example
`main.c` members corroborate the manuals. The SDK timer examples use HSIOSC,
PRS=15 and ARR=145 and poll UD. Their nominal interval is
`146 * 32768 / 48000000 = 99.669333… ms`.

These are external review inputs. No vendor PDF, extracted manual text, header,
SVD, SDK source or upstream repository is bundled by this change. The
[independently rewritten schema provenance](schema-reimplementation-review.md)
and reviewed schema structs are unchanged. Source qualification is a technical
record, not a new redistribution grant or legal-clearance claim.

## Counter and clock contract

The hardware counts down from the 16-bit ARR, sets UD at underflow, reloads ARR
and continues. One sticky UD bit can represent several missed periods. It is
not a lossless elapsed-period accumulator. An ARR write changes the next period
without changing current CNT (CN §11.3.1: L052 pp165–166 / PDF166–167; L083
pp177–178 / PDF178–179).

PRS encodings 1 through 15 mean source division by 2 through 32768. Encoding zero
is reserved; there is no divide-by-one mode. Each period is exactly
`(ARR + 1) * 2^PRS` source cycles, up to 2^31 cycles. OST selects single versus
continuous calibration and does not establish a one-shot timer mode.

The selected source is undivided nominal 48 MHz HSIOSC. The divided HSI display
frequency, HCLK and PCLK are different clock domains. Successful frozen RCC
initialization, unchanged factory trim and its qualified 1.65–5.5 V supply and
temperature conditions are preconditions; RCC bus/flash limits still apply. Source enable/STABLE checks establish readiness,
not measured frequency. A new source-envelope accessor must use the original
factory source bounds; multiplying rounded divided-HSI bounds back up loses the
original evidence relationship.

The qualified full-temperature factory HSI tolerance is ±2% over −40…85°C,
giving 47.04–48.96 MHz. For a period of C raw source cycles, outward nanosecond
bounds are `floor(C * 10^9 / 48960000)` through
`ceil(C * 10^9 / 47040000)`. Keep integer fractions until final rounding.
The datasheets' ±0.5% at 25°C does not replace the full-temperature envelope.
These bounds are conditional on the existing RCC operating envelope; they do
not qualify a new calibration code or debug-halt/deep-sleep elapsed wall time.

Only LSI/LSE operation in deep sleep is documented. Supporting LSI/LSE/HSE later
requires retained RCC source ownership and appropriate reference bounds and
startup contracts. ETR requires a package-valid owned pin and electrical evidence.
Those modes remain outside this safe driver scope, even though the PAC describes
the documented source encodings.

## Register access and selective clearing

Both own CN maps place AUTOTRIM at 0x40014c00 with nine 32-bit registers:

| Register | Offset | Access | Hardware reset |
| --- | ---: | --- | ---: |
| CR | 0x00 | RW | 0x00000000 |
| ARR | 0x04 | RW | 0x0000ffff |
| CNT | 0x08 | RO | 0x00000000 |
| IER | 0x10 | RW | 0x00000000 |
| ISR | 0x14 | RO | 0x00000000 |
| ICR | 0x1c | readable R1W0 | 0x0000003f |
| FCAP | 0x80 | RO | 0x0000ffff |
| TVAL | 0x84 | RO | 0x00000100 |
| FLIM | 0x88 | RO | 0x00000100 |

ICR writes zero to clear and one to preserve an event. Defined flags are END bit0,
OK bit1, UD bit3, OV bit4 and MISS bit5; bit2 is reserved and must retain its reset
one. The precise own CN ICR evidence is §11.8.6, L052 printed p177 / PDF178 and
L083 printed p189 / PDF190. Own EN ICR tables agree (p189/PDF190 and p202/PDF203).

`Icr::Default` remains the generator's generic zero value. It is not a selective
command seed. The reviewed sidecar adds `reset_value()` and `write_noop()` equal
to 0x3f. Start from `Icr::write_noop()`, clear only the intended fields and use
`write_value`. Do not use generic zero-seeded `Reg::write` or read-modify-write for
selective acknowledgement. For a requested mask contained in 0x3b, the command is
`0x3f & !mask`: UD alone is 0x37; all five defined flags is 0x04. CNT, ISR, FCAP,
TVAL and FLIM are read-only. The source/data/PAC validator found that L083
canonical data had incorrectly omitted Read access on FCAP/TVAL/FLIM and thus
generated RW; L052 was already correct. Own L083 CN §11.8.7–9 (p189–190 /
PDF190–191) and EN (p202–203 / PDF203–204) establish RO for all three, so the
L083 authored access and generated PAC are corrected. The earlier audit
inventory correctly recorded RO but its no-access-mismatch conclusion was
incomplete. No writable capture/trim result API is justified.

OV means the calibration TrimCode reached zero or 0x1ff; it is not counter
arithmetic overflow. END alone means calibration ended, not that OK was achieved.

## Ownership, sequencing and bounded control

Configure CR[11:1] before setting EN=1 (CN §11.8.1, L052 p174/PDF175;
L083 p186/PDF187). Timer construction must reject inherited active calibration,
retain exclusive AUTOTRIM ownership, use MD=3/AUTO=0/SRC=0, and keep IER=0 for
polling. It must not change oscillator trim, HSI division or oscillator enables.

APBEN2 bit13 gates configuration PCLK, not the selected working clock. APBRST2
bit13 is active low: zero asserts only AUTOTRIM reset and one releases it. An
owned reset/reconfiguration establishes a fresh starting phase; stop/restart
phase preservation is not specified. Cleanup must preserve LCD and unrelated
clock/reset fields. DEBUG bit6=1 freezes counting under debug.

The timer programming flow explicitly stops through EN=0. There is no BUSY,
stopped, abort-complete or read-synchronization acknowledgement. EN readback
proves the control bit, not a physical drain event. A finite blocking poll budget
means iterations, not microseconds. Failure cleanup masks this block's IER,
requests EN=0 and clears only its own flags with the reviewed command seed.

AUTOTRIM shares IRQ30 (`AUTOTRIM_LCD`, vector offset 0xb8) with LCD. Polling
requires no NVIC ownership. Future async support must dispatch enabled pending
sources for both clients. One driver's stop/drop/error must never disable or
unpend the shared NVIC line or clear LCD flags.

## Conflicts retained rather than hidden

Both own EN Rev 1.0 CR.MD rows say “Please write 11” (L052 p186/PDF187;
L083 p199/PDF200). Their own functional descriptions and calibration flows,
both selected own CN manuals and both SDKs nevertheless describe MD=0 HSIOSC
calibration, MD=1 LSI calibration and MD=3 timer. MD=2 is reserved. This is a
register-table conflict; it is not evidence that calibration was removed.
Typed PAC encodings follow the corroborated own CN/SDK facts without expanding
the safe HAL's timer-only contract.

Both own EN TVAL tables also label 31:6 reserved while labeling 15:0 TVAL
(L052 p190/PDF191; L083 p203/PDF204). The overlap is contradictory. Own CN and SDK
agree on reserved 31:16 and TVAL 15:0. TVAL stays 16-bit; it must not be shrunk to
six bits because of the EN typo.

## Calibration and frequency-measurement gaps

These findings remain unresolved and are not made safe by this counter work:

- AUTO=0 only disables automatic HSIOSC/LSI trim. No reviewed own manual or SDK
  specifies a complete measurement sequence, FCAP signedness/direction,
  capture-valid handshake or coherent sample time. FCAP is a 16-bit frequency
  error capture register, not a documented complete frequency-meter interface.
- The calibration block diagram routes TrimCode into the oscillator, while
  prose also requires software persistence from TVAL into SYSCTRL TRIM. Whether
  or when hardware overrides trim during search, and its state after
  EN=0/AUTO=0/reset, remain unspecified. Starting AUTO=1 must itself be treated
  as potentially changing the target oscillator.
- No calibration termination bound, physical abort/quiescence timing or restart
  state is provided. EN=0 cannot promise restoration. A calibration abort leaves
  oscillator state unqualified until independently valid recovery or reset.
- RCC §§4.3.4/4.3.6 prohibit oscillator parameter changes after startup. §4.4.2
  gives HSIOSC's 32–48 MHz safe range and LSI's 32.8 kHz ±10% trim range. The
  calibration descriptions and live SDK examples do not reconcile these
  restrictions or bound a full 9-bit search. A slower bus and maximum flash
  latency do not prove the oscillator itself stays safe. This also blocks a
  claimed safe boot calibration based solely on these public sources.
- FLIM is read-only and automatically derived from ARR. The advertised 0.4%
  accuracy has no exact FLIM integer formula or guaranteed convergence/drift
  envelope. Reference tolerance, ARR quantization, subsequent temperature and
  supply changes, and search transients matter. Typical trim steps (HSI 0.2%,
  LSI 1%) are not guaranteed maxima. A new trim code does not inherit the
  factory RCC ClockBounds.
- ETR SDK routes are only candidates. Package validity, input pulse/rate limits
  and coherent external-count measurement still need evidence and ownership.

Documented calibration uses TVAL algorithm codes 0…0x1ff, while HSI TRIM is
11-bit and LSI TRIM is 10-bit. Final software persistence must preserve initial
HSI bits10:9 and LSI bit9 and other reserved bits. Vendor helpers write SYSCTRL
trim directly. Their HSI examples deliberately perturb live trim, wait forever
for END and copy TVAL without testing OK; they establish neither safe ownership
nor bounded completion. No safe calibration API is justified by owning
AUTOTRIM/SYSCTRL singletons after other drivers have frozen clock assumptions.

## Reproducible validation boundary

Run `python tests/verify_autotrim_evidence.py --sources /path/to/cw32-sources`.
If the two newly acquired EN PDFs are in another external cache, pass
`--extra-source-dir /path/to/that/cache`. Archive members are read directly from
the locked SDK ZIPs, so extracted vendor files need not be bundled or copied.

The validator checks exact source hashes/byte counts and archive member chains,
claims on the actual cited PDF pages (including the conflicting EN rows),
canonical YAML/generated JSON layouts, enum values including reserved encodings,
the ICR command sidecar, own SYSCTRL fields, every L052/L083 chip's address and
shared IRQ, and generated PAC register access plus explicit command constants.
It reads no HAL implementation and creates no HAL test harness or mock. These
are source/data/PAC checks, not firmware execution, a hardware timing experiment,
calibration qualification or a board-level electrical pass.
