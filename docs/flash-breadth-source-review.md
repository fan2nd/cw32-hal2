# Reserved-region FLASH breadth: independent original-source review

2026-10-08. Scope: F030/A030/F020/F002/F003/L052, preserving L031/R031/W031. This reviewer changed only this report and its companion JSON. No silicon access, production-code change, publication, or hardware-validation claim.

## Verdict

The six extension families are source-qualified for the same bounded, blocking, exclusively reserved-region API, subject to the guards below. No new documentary blocker was found. The production candidate and final normal ARM build/link evidence were independently reviewed at the hashes in the JSON; no blocking finding remains. Keep L010/L011/L012/L083 deferred for the concrete controller/security differences below.

The extension admits 17 new exact ordering codes plus the existing eight: **25 exact parts**. All 13 generic profiles still have `memory=[]`. The four F030 package-neutral compatibility aliases have nonempty memory already, but are not exact ordering codes and must not be implicitly storage-qualified by a nonempty-memory check.

## Cache and controller contract

- F030/A030 share the explicitly named CW32x030 CN Rev2.5 manual. §7.5.1/2, printed112/114 (PDF113/115), requires `CR2.CACHE=0` and `FETCH=0` before program/erase; it restores Read mode before enabling them again. F020's own Rev1.4 manual gives the same sequence at printed110/112 (PDF111/113).
- For those three families, `CACHE[4]`, `FETCH[3]` and `WAIT[2:0]` are the only low CR2 fields; bits15:5 are reserved. See x030 §7.9.2 printed121/PDF122 and F020 §7.9.2 printed119/PDF120. No invalidate register or additional invalidate operation is specified. Do not write L012's bit5 here. The manual specifies a sufficient disable/operate/Read/enable sequence but does not separately explain an automatic invalidation mechanism. Retaining the caller's previous CACHE/FETCH state instead of forcibly enabling both is compatible with the independently documented enable controls; WAIT must remain unchanged.
- Require readback that both cache bits are clear before any trigger. Wait for BUSY=0, restore and verify MODE=Read, then restore only the saved cache-enable state and verify it. A cleanup failure must be reported; cache must not be enabled over a failed Read-mode restoration.
- CR1/CR2/PAGELOCK writes require KEY[31:16]=0x5a5a in the same write. BUSY is CR1[5], SECURITY is read-only[7:6], STANDBY is[4], MODE is[1:0]. Preserve WAIT/STANDBY and never reset FLASH during acquisition. MODE must be replaced, never ORed into saved state. MODE3 is invalid on x030/F020 and chip erase on F002/F003/L052 and the baseline families.
- All nine qualified families use PC[0], PAGELOCK[1], PROG[4], mask0x13. ICR is W0C, not W1C. Use a direct clear write, never an RMW of the partly write-only register. PROG is WO on F002/F003/F020/x030 and R1W0 on L052/baseline; this does not change clear polarity. A full error-clear image0x0c retains reserved bits3:2 at the documented reset image while high reserved bits remain zero. Sources: F002 PDF99, F003 PDF101, F020 PDF122, x030 PDF124, L052 PDF124; baseline L031/R031/W031 PDF119/121/120.

## Geometry and legal masks

Every qualified variant has 512-byte erase pages and naturally aligned 1/2/4-byte programming. Each addressed byte must be erased before programming; repeated1-to0 programming is not supported.

| Family | Exact capacity | Pages per lock bit | Legal PAGELOCK mask | Own manual primary evidence, 1-based PDF pages |
|---|---:|---:|---:|---|
| F002 | 16KiB | 4 | 0x00ff | 88 geometry/WAIT;89–90 operations;92 protection;96–99 registers |
| F003 | 20KiB | 4 | 0x03ff | 90 geometry/WAIT;91–92 operations;94 protection;98–101 registers |
| F020 | 32KiB | 8 | 0x00ff | 110 geometry/WAIT;111–113 operations;114 protection;119–122 registers |
| F030 | 32/64KiB | 8 | 0xffff | x030112 geometry/WAIT;113–115 operations;116 protection;121–124 registers |
| A030 | 64KiB | 8 | 0xffff | Shared x030 sources as above, explicitly named F030/A030 in page header |
| L052 | 64KiB | 8 | 0xffff | 112 geometry/WAIT;113–114 operations;116 protection;121–124 registers |
| L031/R031/W031 | 64KiB each | 8 | 0xffff | Own geometry107/109/108; CR2+locks117/119/118 |

Compute groups from validated absolute page/range indices. F002/F003 groups are 2KiB; others4KiB. Preserve unrelated **legal** bits. For F030F6P7, only groups0–7 can ever be touched because the exact capacity is32KiB, even though the shared controller map exposes16 bits. Do not infer capacity from that map or the shared manual's die maximum.

## Electrical bounds and WAIT

These are sustained board limits, including supply transients and oscillator tolerance, not nominal RCC settings. Own datasheet temperature, power and analog-supply-equality conditions remain part of the caller contract. Program/erase times are typical only; none of these tables supplies a completion-time maximum.

| Family | Qualified supply | HCLK at VDD>=1.8V | HCLK below1.8V | Own datasheet general / FLASH table, printed(PDF) |
|---|---|---:|---:|---|
| A030 | 1.65–5.5V | 64MHz | 24MHz | 34(35) /44(45) |
| F002 | 1.65–5.5V | 48MHz | 24MHz | 30(31) /38(39) |
| F003 | 1.65–5.5V | 48MHz | 24MHz | 31(32) /39(40) |
| F020 | 1.65–5.5V | 48MHz | 24MHz | 35(36) /45(46) |
| F030 | 1.65–5.5V | 64MHz | 24MHz | 37(38) /47(48) |
| L052 | 1.65–5.5V | 48MHz | 24MHz | 42(43) /51(52) |
| L031 | 1.65–5.5V | 48MHz | 24MHz | 37(38) /47(48) |
| R031 | 2.2–3.6V | 48MHz | Not allowed | 41(42) /54(55) |
| W031 | 1.8–3.6V RF-LDO;2.0–3.6V RF-DCDC | 48MHz | Not allowed | 40(41) /53(54) |

W031's existing conservative2.0–3.6V envelope is valid for either RF mode. All nine controllers document WAIT0/1/2 ceilings of24/48/72MHz, but the device frequency limits above still apply. Reject undocumented WAIT3–7; do not treat WAIT2 as permission for a48MHz device to run72MHz. A64MHz F030/A030 board needs WAIT2. WAIT is RCC-owned; neither cache management nor cleanup may overwrite it.

## Exact metadata qualification

Own datasheet family tables and address maps were re-read: A030 PDF8/29, F0028/26, F0038/27, F0208/30, F0308/32 and L05210/36. F020 uses the corrected `current-datasheets/CW32F020_DataSheet_CN_V1.3.pdf`, not the historical root-level mislabeled copy. Table3-1 qualifies F030F6 as32KiB; the64KiB maximum in its family address map does not override it. F002F3 is16KiB and F003x4 is20KiB, despite suffix-based conventions in other vendors.

New exact parts:

- A030C8T7:64KiB
- F002F3P7/F3U7:16KiB
- F003E4P7/F4P7/F4U7:20KiB
- F020C6U7/F6U7/K6U7:32KiB
- F030F6P7:32KiB; F030C8T7/F8V7/K8T7/K8U7:64KiB
- L052C8T6/R8S6/R8T6:64KiB

All names above carry the CW32 prefix. The companion JSON lists all25 full names, page counts, source pages and generated-metadata cross-checks. Keep CW32F030C8/F6/F8/K8 aliases outside the exact-part storage allowlist, without deleting their compatibility metadata.

## SDK corroboration and hazards

Own SDKs corroborate register access but cannot override manuals or exact memory evidence. F030 flash.c:221/226 and F020:228/235 disable/restore CR2 cache bits, with no invalidate sequence. They also OR MODE, which must not be copied. F002 flash.c:166 admits page39 instead of31; F020:219 admits127 instead of63. F002/F003/F020 unlock-all0xffff exceeds their legal masks. L052:184 admits511 instead of127, and lines189–199 use a wrapping uint16 postdecrement timeout that can miss stuck BUSY. L083 has the same timeout pattern. These SDK bugs are not extension blockers when the driver uses its separately reviewed bounds, masks and wait-until-idle cleanup.

## Why the remaining four stay deferred

- **L010:** Own UM PDF110/114/117/118 documents SLIB control bytes0xfff0–0xffff, ISR[5] BUSY, CR1 SECURITY[6:5], no CR1 STANDBY, and read-only SDKCFR. The whole final512-byte erase page and any active SLIB range require exclusion. Its supply floor is1.62V and WAIT only0/1. This is a separate backend and safe-region policy, not a family-name addition.
- **L011:** Same SLIB/BUSY/security differences, plus SDKERR[2] (error mask0x17), WAIT0–3 and96MHz only at>=1.8V. Own UM PDF109/113/115–117; DS general/FLASH PDF39/52. Current manual documents authenticated ISP for SLIB removal; do not infer L010's RAM chip-erase recovery route.
- **L012:** Own UM printed90–91/PDF116–117 requires CACHE/FETCH disable; printed101/PDF127 says CACHEINVALID[5] must be written1 then0, not treated as self-clearing. Add CACHEON[3] and SDKERR[2] (mask0x1f), ISR BUSY, shifted SECURITY, and SLIB exclusions (PDF122,126,129–131). Its SDK normal erase/program routines omit required cache disable. The normal manual operation sequence does not explicitly mandate an invalidate toggle on every write; that distinction must survive any later implementation.
- **L083:** Exact128/256KiB parts need256/512-page handling and four keyed16-bit locks at+0x08/+0x0c/+0x10/+0x14. Own UM PDF121/125/129–133. The current one-lock abstraction is insufficient; test page127/128,255/256 and511 and every register transition. Protection registers are not proof of read-while-write banks. Table7-2 prints LOCK56 as488–455; the explicit eight-page rule and neighboring rows imply448–455, but retain the discrepancy. DS operating conditions still cap64MHz,24MHz below1.8V.

L01x own FLASH tables have no separate Vprog row; their general VDD conditions must not be mislabeled as independently specified FLASH programming voltage.

## Validation and completion boundary

All61 selected original PDF/archive/FLASH-source hashes matched the prior audit's pinned hashes; full SHA-256 and official URLs are in the JSON. Fresh `pdftotext -layout` extraction of25 unique critical register/electrical pages matched stored text. The x030 CR2/protection PDF122 image was inspected directly. All25 selected exact-part generated memory records match source-qualified sizes; all13 generic profiles remain empty.

Keep explicit blocking writes/erases and ReadNorFlash only. Source review does not establish the power-loss containment required by NorFlash, safe timeout abort, DMA programming, completion IRQ, independently operable banks, or board timing/coherency. No such support should be inferred. The source and production review do not replace the final normal ARM build/link matrix.

## Accepted pre-flattening production-candidate review

Read-only review completed at the nine production/example file hashes pinned in the JSON (2026-10-08T14:07:12Z). The exact allowlist mechanically matches all25 source-qualified parts. No Flash test modules, test attributes, or test-only hooks are present; this reviewer created no unit/integration harness and ran no HAL tests.

One review finding was corrected and rechecked: cache restore originally recaptured the current WAIT value, potentially accepting a failed disable readback as a new baseline. The final candidate saves original CR2 once and uses the original WAIT for disable, restore and failed-Read cleanup comparison. Cache bits remain disabled if Read-mode restoration fails. No blocking finding remains in the pinned candidate.

The already-built F002 real example was inspected read-only: storage0x3000–0x4000, stack0x20000800, maximum file-backed FLASH LOAD end0x0e2c. Its image does not overlap the reserved partition. This is an observed earlier link, not a claim that the final25-part matrix has completed. The implementation task owns that normal ARM build/link result and must match the pinned production files.

## Final normal build/link evidence

Final evidence review on 2026-10-08T14:12:46Z confirms 42 successful normal ARM library checks (25 exact, nine generic, four aliases, four excluded families) and 25 successful real-example release links. All nine pinned production/example hashes remain unchanged. Summary SHA-256: `e47ca7de1b42c7e43f08568045af030be4c5a7278683067db4a24ad208f13eae` (`docs/verification-logs/flash-breadth/summary.json`). All summary capacities, final-4-KiB reservation arithmetic, stack bounds and build-log success endings were checked; representative full readelf logs independently confirm:

| Exact part | FLASH / SRAM | Reserved half-open range | Last file-backed FLASH LOAD end | Stack symbol |
|---|---|---|---|---|
| CW32F002F3P7 | 16 / 2 KiB | 0x3000–0x4000 | 0x0e64 | 0x20000800 |
| CW32F003E4P7 | 20 / 3 KiB | 0x4000–0x5000 | 0x0e64 | 0x20000c00 |
| CW32F030F6P7 | 32 / 6 KiB | 0x7000–0x8000 | 0x0f58 | 0x20001800 |
| CW32A030C8T7 | 64 / 8 KiB | 0xf000–0x10000 | 0x0f50 | 0x20002000 |

Every representative load ends below its partition, and each stack symbol equals the exact SRAM upper bound. The JSON binds the representative readelf log hashes and matrix-recorded ELF hashes. Generic profiles lack memory metadata; F030 compatibility aliases retain verified memory metadata but are not qualified for this storage API. The four deferred families have no qualified Flash module. No tests or harnesses were created or run by this reviewer, and no firmware was executed on hardware.

## Flattened-layout rebase

The prior production and matrix hashes above identify the accepted pre-flattening
packet. The next batch moves its unchanged backend implementation to backend.rs
and reapplies only Flash hooks to the current shared build.rs/lib.rs. See
flash-breadth-flat-review.md and flash-breadth-flat-verification.json for the
rebased paths and current validation. F030 compatibility aliases have verified
memory sizes; their continued rejection is an exact-ordering-code qualification
policy for this storage API, not absent capacity evidence.
