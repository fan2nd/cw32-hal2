# FLASH next-batch source audit

Audit date: 2026-10-08. Read-only engineering audit; no controller access, runtime/generator/metadata edits, publication, or silicon-validation claim. The companion [JSON](flash-next-batch-audit.json) records exact per-family fields, source SHA-256 hashes and official URLs.

## Recommendation

Start with a blocking driver for the eight exact CW32L031/R031/W031 ordering codes. Their own manuals agree on a cache-free controller, 64-KiB main array, byte programming, 512-byte erase pages and 4-KiB protection groups. Retain each family’s electrical limits. Scope the safe API to an exclusively owned linker-reserved storage region; until such a token exists, construction of a mutable region needs an unsafe contract. The FLASH peripheral token alone does not prove ownership of code/data bytes.

Do not implement MultiwriteNorFlash: hardware rejects a program access unless every addressed byte is already 0xFF. Do not add interrupt-driven async on the strength of FLASHRAM: the documented interrupts are errors, with no EOP/completion source. Program/erase stalls a FLASH-resident executor.

The first group is a source-qualified candidate, not a completed driver or hardware test. F002/F020 required the register-map narrowing recorded below; x030 adds cache handling; L01x adds distinct BUSY/protection behavior; L083 requires wider page indexing.

## Geometry and controller matrix

All main arrays begin at 0x00000000. All audited families support naturally aligned 1/2/4-byte accesses, erase 512-byte pages to 0xFF, and require erased contents for programming. The capacity column is limited to the exact catalog parts listed below, not an inferred capacity for a generic family profile.

| Family | Exact flash KiB / page counts | Observed PAC version | BUSY | Lock bits × pages | Cache / special concerns |
|---|---|---|---|---|---|
| CW32A030 | 64 / 128 | v1 | CR1[5] | 16 × 8 | FETCH/CACHE |
| CW32F002 | 16 / 32 | cw32f002_v1 | CR1[5] | 8 × 4 | none |
| CW32F003 | 20 / 40 | cw32f003_v1 | CR1[5] | 10 × 4 | none |
| CW32F020 | 32 / 64 | cw32f020_v1 | CR1[5] | 8 × 8 | FETCH/CACHE |
| CW32F030 | 32 KiB: 64 pages; 64 KiB: 128 pages | v1 | CR1[5] | 16 × 8 | FETCH/CACHE |
| CW32L010 | 64 / 128 | cw32l010_v1 | ISR[5] | 16 × 8 | none; SLIB descriptor in last page |
| CW32L011 | 64 / 128 | cw32l011_v1 | ISR[5] | 16 × 8 | none; SLIB descriptor in last page |
| CW32L012 | 64 / 128 | cw32l012_v1 | ISR[5] | 16 × 8 | FETCH/CACHE; SLIB descriptor in last page |
| CW32L031 | 64 / 128 | cw32l031_v1 | CR1[5] | 16 × 8 | none |
| CW32L052 | 64 / 128 | cw32l031_v1 | CR1[5] | 16 × 8 | none |
| CW32L083 | 128 KiB: 256 pages; 256 KiB: 512 pages | cw32l083_v1 | CR1[5] | 64 × 8 | none; four protection registers, no proven RWW banks |
| CW32R031 | 64 / 128 | cw32l031_v1 | CR1[5] | 16 × 8 | none |
| CW32W031 | 64 / 128 | cw32l031_v1 | CR1[5] | 16 × 8 | none |

Every one of the 13 generic family JSON profiles has an empty memory list. Preserve that unknown-capacity state: no guessed FLASH_SIZE or safe full-array constructor. The four existing F030 package-neutral aliases are separate compatibility entries, not four more exact ordering codes. Memory sizes must come from the selected exact part, never suffix arithmetic or an SDK maximum. In particular, F002F3 is 16 KiB and F003x4 is 20 KiB in the current evidence.

## Confirmed map problems and source disagreements

- F002: observed flash_cw32f002_v1 PAGELOCK has LOCK0–9. Own UM §7.9.3 printed 96/PDF 97 declares only LOCK0–7 and bits 15:8 reserved. §7.6.1 printed 91/PDF 92 agrees on 32 pages, four pages per bit. The PDF image was inspected. Own CMSIS masks/SVD and SDK bounds incorrectly include the extra F003-sized area. Preserve F003’s ten-bit map when separating F002.
- F020: observed flash_v1 PAGELOCK has LOCK0–15. Own UM §7.9.3 printed 119/PDF 120 declares LOCK0–7 and bits 15:8 reserved; §7.6.1 printed 113/PDF 114 agrees on 64 pages, eight pages per bit. The PDF image was inspected. Current DS §6 table 6-1 printed 29/PDF 30 limits main flash to 0x0000–0x7FFF. Own CMSIS/SVD repeat the larger map and SDK accepts page 127. Separate F020 without narrowing F030/A030.
- Other audited map offsets/bit positions did not reveal a definite mismatch. PAGELOCK1 versus PAGELOCK at +0x08 is a naming alias for the L031-style controller. ICR W0C, mixed-field RO/WO, reset values and operation constraints are semantic gaps in current fieldsets, not evidence that the whole register is Write-only. ISR is already Read-only.
- L083 table 7-2 printed 124/PDF 125 has an internally inconsistent LOCK56 range, “488–455”. The documented eight-pages-per-bit rule and adjacent rows imply 448–455. Retain that discrepancy instead of silently claiming every printed row agrees.

The parent applied the reviewed split while this audit was in progress: F002 now selects its narrowed eight-bit cw32f002_v1 map; F003 selects cw32f003_v1 with ten bits; F020 selects cw32f020_v1 with eight bits. The JSON records this corrected snapshot alongside the original discrepancy. No register, runtime or generator file was changed by this audit.

## Register and operation contract

Controller base is 0x40022000. CR1/CR2 are +0x00/+0x04; protection registers start +0x08; IER/ISR/ICR are +0x20/+0x24/+0x28. L083 adds lock registers at +0x0C/+0x10/+0x14. L01x additionally has read-only SDKCFR at +0x70 (START[6:0], END[14:8]).

- CR1, CR2 and every protection-register write require KEY[31:16]=0x5A5A in the same write. KEY is WO. Lock polarity is 0 locked, 1 writable; unlock only groups covering the requested operation and preserve unrelated groups.
- All MODE fields are [1:0]: 0 Read, 1 Program, 2 PageErase. MODE=3 is invalid on F020/x030 and ChipErase on the other audited families. Always replace MODE bits; ORing a saved nonzero MODE can select mass erase. Do not expose ChipErase in ordinary storage.
- Classic controllers have STANDBY[4], RO BUSY[5] and RO SECURITY[7:6] in CR1. L010/L011/L012 have SECURITY[6:5] in CR1, no STANDBY there, and BUSY in ISR[5].
- Error bits are PC[0], PAGELOCK[1], PROG[4] (mask 0x13). L011 adds SDKERR[2] (0x17). L012 adds SDKERR[2] and CACHEON[3] (0x1F). Capture all implemented errors, not the vendor GetStatus subset.
- ICR clears by writing 0; writing 1 leaves the corresponding flag unchanged. F002/F003/F020/x030 PROG is WO while PC/PAGELOCK are R1W0; low-power-family ICR fields are R1W0. Do not RMW a mixed-WO clear register or use W1C logic.
- CR1 resets to 0x10 for classic families and 0 for L01x. CR2, locks, IER and ISR reset to 0. ICR resets to 0x0F for F002/F003/F020/x030 and 0x1F otherwise; L01x SDKCFR resets to 0x7F. These reset images do not substitute for explicit keyed writes.

An operation must validate every range before unlocking, wait idle, clear relevant errors, prepare cache if present, explicitly select MODE, trigger a naturally aligned store, wait until BUSY clears, inspect errors, return MODE to Read, and restore controlled lock/cache state. A byte program triggers through a byte store; a page erase can trigger through a byte store anywhere in that page. Never restore stale nonzero MODE. Partial writes are possible; this is not a transaction or automatic erase-before-write API.

## Cache, clock ownership, stalls and IRQs

x030 and F020 require FETCH/CACHE disabled for erase/program; L012 has the same requirement plus a CACHEON error. L012 CACHEINVALID[5] requires writing 1 then 0 to invalidate, not a self-clearing pulse. Its documented normal operation sequence does not explicitly require that toggle on every write; distinguish an implementation choice from a manual requirement. L010/L011/L031/L052/L083/R031/W031 have no FETCH/CACHE bits in the reviewed controller map.

FLASH configuration clock must be enabled before register configuration. AHBEN.FLASH is bit 1; AHBEN itself is keyed on L01x. The control gate is not a license to disable ordinary code fetch. Preserve RCC-managed WAIT, clock enables and the recommended STANDBY configuration. SYSCTRL_AHBRST.FLASH is active-low, but a generic enable-and-reset constructor would reset WAIT while code may run above 24 MHz. Do not reset the controller just to take its ownership token.

Read wait thresholds are 24/48/72 MHz for classic families,24/48 MHz for L010, and 24/48/72/96 MHz for L011/L012. The corresponding WAIT values are 0/1/2(/3). These are encoding ceilings, not device frequency permissions. L031/L052/L083/R031/W031 and L01x mirror WAIT through SYSCTRL_CR2[6:4].

A CPU executing from FLASH stops subsequent instruction fetch until the current erase/program completes. RAM execution may proceed, but software must poll the correct BUSY field. A software timeout located in FLASH cannot run while the hardware has stalled instruction fetch. No completion IRQ, erase suspend, independently operable RWW bank, or safe in-flight abort was established. FLASH shares IRQ3 with RAM; L052 calls it FLASH_RAM and other profiles FLASHRAM. An eventual combined handler must not clear or monopolize RAM errors.

Before DeepSleep require BUSY=0 and MODE=Read. Real-time/IRQ/watchdog latency during an operation remains a board validation requirement. A RAM-resident flow would need its entire transitive instruction/literal/vector path audited, not just one function attribute. Do not return an async future to an executor in FLASH and claim it is nonblocking.

## Electrical and timing limits

All durations below are typical values; the data sheets provide no guaranteed maximum. Program values correspond to 8/16/32-bit physical stores. Current/time ratings are source specifications, not measurements made by this project.

| Family | Program µs | Page erase ms | FLASH Vprog V | Device HCLK max MHz | Flash table printed/PDF |
|---|---|---|---|---|---|
| CW32A030 | 31/37/53 | 4.5 | 1.65–5.5 | 64 | 44/45 |
| CW32F002 | 31/37/53 | 4.5 | 1.65–5.5 | 48 | 38/39 |
| CW32F003 | 31/37/53 | 4.5 | 1.65–5.5 | 48 | 39/40 |
| CW32F020 | 31/37/53 | 4.5 | 1.65–5.5 | 48 | 45/46 |
| CW32F030 | 31/37/53 | 4.5 | 1.65–5.5 | 64 | 47/48 |
| CW32L010 | 31/39/55 | 2.5 | not separately specified | 48 | 43/44 |
| CW32L011 | 31/39/55 | 2.5 | not separately specified | 96 | 49/52 |
| CW32L012 | 31/39/55 | 2.5 | not separately specified | 96 | 56/59 |
| CW32L031 | 30/37/51 | 4.6 | 1.65–5.5 | 48 | 47/48 |
| CW32L052 | 30/37/51 | 4.6 | 1.65–5.5 | 48 | 51/52 |
| CW32L083 | 30/37/51 | 4.6 | 1.65–5.5 | 64 | 56/57 |
| CW32R031 | 30/37/51 | 4.6 | 2.2–3.6 | 48 | 54/55 |
| CW32W031 | 30/37/51 | 4.6 | 1.8–3.6 | 48 | 53/54 |

Below 1.8 V, non-radio families are limited to 24 MHz: lower VDD bound 1.62 V on L010,1.7 V on L011/L012,1.65 V on the other non-radio families. Their higher maxima require VDD≥1.8 V; maximum supply is 5.5 V. L01x general VDD limits are not separately characterized FLASH Vprog rows. R031 requires 2.2–3.6 V at up to 48 MHz. W031 allows 1.8–3.6 V in RF LDO mode, but RF DCDC mode requires at least 2.0 V; it remains limited to 48 MHz. Follow the own datasheet general operating conditions and power/temperature limits throughout the operation.

L01x endurance is specified as at least 10,000 cycles and retention 25 years across −40..85°C. Others specify 20,000 cycles; retention 100 years at 25°C and 25 years at 85°C, with 10 years at 105°C on F002/F003/F020/F030/A030. Programming needs an internal boost circuit, not an externally supplied programming voltage. No endurance claim excuses repeated writes without erase.

The typical mass-erase timings in the JSON are reference data only, not approval to expose a chip-erase API. Timeouts may report a diagnostic only after a safe state exists: there is no documented right to change MODE, relock, reset or retry while BUSY remains set. Supply-loss/interrupted-operation contents and transaction atomicity are unverified.

## Security, special areas and recovery

SECURITY is read-only in FLASH. Level0 permits ISP/SWD reads; Level1 downgrade through ISP/SWD erases the flash; Level2 allows downgrade through ISP with an erase and disables SWD; Level3 disallows both downgrade paths. CPU reads/fetches remain usable under read protection. F002/F003 additionally limit protection-level changes to 48. Do not add the SDK magic-address protection setter to a safe storage trait. An API named “unlock” must mean temporary page-group access, not protection-level downgrade.

Exclude factory trim, OTP, BootLoader and option/protection state from ordinary NorFlash. Separate BootLoader regions are 2 KiB on L01x and 2.5 KiB on the classic families; their presence does not add writable storage capacity.

L010/L011/L012 main-array bytes0xFFF0..0xFFFF contain the SLIB descriptor: password, protected start/end page, signature and CRC16_X25. Exclude that entire containing512-byte erase page and any active SLIB range from default safe storage. A conventional last-page settings partition would otherwise risk changing security. Protected library code can execute but cannot be read out; L012 explicitly covers DMA reads. L010 documents authenticated ISP or RAM-executed chip erase to disable SLIB. Current L011/L012 only document the authenticated ISP route; SDK RAM chip-erase examples do not establish the missing guarantee.

The reset chapters document active-low peripheral reset. They do not establish it as a safe erase/program abort, preservation of interrupted data, or a recovery sequence after a stuck BUSY. Recovery remains an explicit unsupported behavior, not an inferred automatic reset.

## SDK audit: do not copy these behaviors

- CW32F002: Page limit39 and address0x4FFF belong to 20 KiB, but F002 exact parts are 16 KiB/32 pages. LOCK8/9 and unlock-all 0xFFFF exceed documented map. Source: cw32f002_flash.c / cw32f002_flash.h / cw32f002.h.
- CW32F020: Page limit127, address0xFFFF, sixteen lock bits and unlock-all 0xFFFF exceed documented32 KiB/64-page/eight-bit map. Source: cw32f020_flash.c / cw32f020_flash.h / cw32f020.h.
- CW32L031, CW32R031, CW32W031: Header accepts through 511 although current main array has 128 pages and ErasePage body limits127. Do not inherit capacity from assertions. Source: own *_flash.h IS_FLASH_PAGE_Number and *_flash.c FLASH_ErasePage.
- CW32L052: SDK accepts pages through 511 despite64 KiB/128 pages in own manual. Source: cw32l052_flash.c:184 FLASH_ErasePage.
- CW32L012: Normal routines never disable CACHE/FETCH, contrary to current UM7.5.1–2 printed 90–91/PDF 116–117. Do not copy the routines. Source: cw32l012_flash.c FLASH_ErasePage/FLASH_Write*.
- CW32L012: Bounds add element count rather than byte count, permitting overrun by 2x/4x. Source: cw32l012_flash.c:343,389 FLASH_WriteHalfWords/Words.
- CW32L011, CW32L012: Ignores SDKERR; L012 additionally ignores CACHEON and may report OK with real errors pending. Source: own *_flash.c FLASH_GetStatus.
- CW32L052, CW32L083: uint16_t busy && timeout-- exhaustion wraps0 to 0xFFFF; following timeout==0 misses the stuck-busy terminal path. Some error returns do not restore state. No safe timeout recovery follows. Source: cw32l052_flash.c:189–199 / cw32l083_flash.c:200–215 and related routines.
- CW32F002, CW32F003, CW32F020, CW32F030, CW32L010, CW32L011, CW32L012, CW32L031, CW32L052, CW32L083, CW32R031, CW32W031: ORs1/2 into saved MODE rather than replacing bits. Stale MODE may produce3, documented ChipErase except x030/F020 where invalid. Never copy this sequencing. Source: own *_flash.c CR1BAK | operation writes.
- CW32L010, CW32L011, CW32L012: uint32_t trigger store to supplied StartAddr has no4-byte-alignment enforcement; unaligned requests may HardFault. Source: own *_flash.c FLASH_ErasePages.
- CW32F002, CW32F003, CW32L010, CW32L011, CW32L012, CW32L052: Incrementing an unaligned original address by 512 can omit the final crossed page/group (erase start0x1FF,end0x200 is a concrete crossed-page case). Compute page/group indices instead. Source: own *_flash.c inclusive while(StartAddr<=EndAddr) loops.
- CW32F002, CW32F003, CW32F020, CW32F030, CW32L010, CW32L011, CW32L012, CW32L031, CW32L052, CW32L083, CW32R031, CW32W031: Special control writes and reset lie outside ordinary storage operations. SECURITY in FLASH is RO; manual directs protection configuration to ISP. Magic SDK addresses are insufficient basis for a safe HAL setter. Source: own *_flash.c FLASH_SetReadOutLevel.

## Embassy-compatible API boundary

- Keep the flash module, Peri<FLASH> lifetime ownership, explicit Blocking marker, blocking_read/write/erase and embedded-storage error mapping. Use Error::OutOfBounds/NotAligned equivalents plus errors for protection/programming/cache/SLIB where relevant.
- Safe NorFlash over a proven, exclusive storage region can use READ_SIZE=1, WRITE_SIZE=1, ERASE_SIZE=512. An unsafe constructor must document that no code, vector, live immutable object, DMA reader or overlapping storage owner uses the region. General whole-image mutation stays unsafe/unavailable.
- Capacity and region tables come from exact ordering-code memory, not the peripheral-version name. Generic profiles have no proven capacity. Read/write bounds and erase-exclusive-end arithmetic must use checked addition/multiplication; reject reversed or unaligned erase ranges.
- CW32 main flash starts at 0. Do not copy STM32’s slice::from_raw_parts(base+offset,len), which creates an invalid null reference at offset 0. Use an audited zero-address-safe raw hardware access primitive, including empty-read edge cases.
- Upstream FlashSector.index_in_bank and FlashRegion::sectors() are u8. L083 has 256 or512 pages and needs u16/usize. Its four PAGELOCK registers divide protection, not proof of independent RWW banks. Do not blindly copy generated STM32 bank/region types.
- No MultiwriteNorFlash, successful-completion interrupt handler, DMA programming, automatic erase-before-write, secure-library/OTP write API or timeout-abort semantics is justified. A future cooperative wrapper may yield between completed operations, but that does not remove per-operation stalls.

Upstream compared: Embassy commit f16efeffe37581092ec184718e6fdb1620393214, embassy-stm32/src/flash/mod.rs, common.rs and asynch.rs; exact hashes/URLs are in the JSON.

## Exact 37 ordering-code memory table

| Part | Package | Flash KiB | Pages | SRAM KiB |
|---|---|---:|---:|---:|
| CW32A030C8T7 | LQFP48 | 64 | 128 | 8 |
| CW32F002F3P7 | TSSOP20 | 16 | 32 | 2 |
| CW32F002F3U7 | QFN20 | 16 | 32 | 2 |
| CW32F003E4P7 | TSSOP24 | 20 | 40 | 3 |
| CW32F003F4P7 | TSSOP20 | 20 | 40 | 3 |
| CW32F003F4U7 | QFN20 | 20 | 40 | 3 |
| CW32F020C6U7 | QFN48 | 32 | 64 | 8 |
| CW32F020F6U7 | QFN20 | 32 | 64 | 8 |
| CW32F020K6U7 | QFN32 | 32 | 64 | 8 |
| CW32F030C8T7 | LQFP48 | 64 | 128 | 8 |
| CW32F030F6P7 | TSSOP20 | 32 | 64 | 6 |
| CW32F030F8V7 | QFN20 | 64 | 128 | 8 |
| CW32F030K8T7 | LQFP32 | 64 | 128 | 8 |
| CW32F030K8U7 | QFN32 | 64 | 128 | 8 |
| CW32L010F8P6 | TSSOP20 | 64 | 128 | 4 |
| CW32L010F8U6 | QFN20 | 64 | 128 | 4 |
| CW32L010Y8M6 | SOP16 | 64 | 128 | 4 |
| CW32L011K8T6 | LQFP32 | 64 | 128 | 6 |
| CW32L011K8U6 | QFN32 | 64 | 128 | 6 |
| CW32L012C8T6 | LQFP48 | 64 | 128 | 8 |
| CW32L012C8U6 | QFN48 | 64 | 128 | 8 |
| CW32L031C8T6 | LQFP48 | 64 | 128 | 8 |
| CW32L031C8U6 | QFN48 | 64 | 128 | 8 |
| CW32L031F8P6 | TSSOP20 | 64 | 128 | 8 |
| CW32L031F8U6 | QFN20 | 64 | 128 | 8 |
| CW32L031K8U6 | QFN32（5×5mm） | 64 | 128 | 8 |
| CW32L031K8V6 | QFN32（4×4mm） | 64 | 128 | 8 |
| CW32L052C8T6 | LQFP48 | 64 | 128 | 8 |
| CW32L052R8S6 | LQFP64（7×7mm） | 64 | 128 | 8 |
| CW32L052R8T6 | LQFP64（10×10mm） | 64 | 128 | 8 |
| CW32L083MCT6 | LQFP80 | 256 | 512 | 24 |
| CW32L083RBT6 | LQFP64（10×10mm） | 128 | 256 | 24 |
| CW32L083RCS6 | LQFP64（7×7mm） | 256 | 512 | 24 |
| CW32L083RCT6 | LQFP64（10×10mm） | 256 | 512 | 24 |
| CW32L083VCT6 | LQFP100 | 256 | 512 | 24 |
| CW32R031C8U6 | QFN48 | 64 | 128 | 8 |
| CW32W031R8U6 | QFN64 | 64 | 128 | 8 |

The JSON records per-part memory-table and address-map PDF pages. Sizes were cross-checked with the repository’s separately verified parts.json; no new part capacity was inferred by this audit.

## Per-family source locators

Physical PDF page numbers below are 1-based. Printed pages are usually PDF − 1, except L012 manual (PDF − 26) and L011/L012 data sheets (PDF − 3); the JSON explicitly records printed electrical pages. For exact source identity, use the filename plus SHA-256 in the source registry, not the basename alone. In particular, F020 uses the current-datasheets copy, not the older root copy with the same nominal V1.3 filename.

| Family | FLASH chapter PDF pages | Geometry/wait §7.3–4 PDF | Control section PDF | Stalls/DeepSleep PDF | Clock enable/reset PDF |
|---|---|---|---|---|---|
| CW32A030 | 112–124 | 112 | 7.9: 121 | 119 | 82 / 85 |
| CW32F002 | 88–99 | 88 | 7.9: 96 | 94 | 60 / 63 |
| CW32F003 | 90–101 | 90 | 7.9: 98 | 96 | 62 / 65 |
| CW32F020 | 110–122 | 110 | 7.9: 119 | 117 | 80 / 83 |
| CW32F030 | 112–124 | 112 | 7.9: 121 | 119 | 82 / 85 |
| CW32L010 | 104–118 | 104 | 7.10: 114 | 112 | 79 / 82 |
| CW32L011 | 102–117 | 102 | 7.10: 113 | 111 | 77 / 80 |
| CW32L012 | 114–131 | 114 | 7.10: 126 | 124 | 82 / 86 |
| CW32L031 | 107–119 | 107 | 7.9: 116 | 114 | 77 / 80 |
| CW32L052 | 112–124 | 112 | 7.9: 121 | 119 | 81 / 84 |
| CW32L083 | 121–134 | 121 | 7.9: 130 | 128 | 86 / 90 |
| CW32R031 | 109–121 | 109 | 7.9: 118 | 116 | 79 / 82 |
| CW32W031 | 108–120 | 108 | 7.9: 117 | 115 | 78 / 81 |

Official documents (exact hashes and SDK-member hashes are in the companion JSON):

- [CW32A030_DataSheet_CN_V1.1.pdf](https://www.whxy.com/uploads/files/20251230/CW32A030_DataSheet_CN_V1.1.pdf)
- [CW32F002_DataSheet_CN_V1.2.pdf](https://www.whxy.com/uploads/files/20251230/CW32F002_DataSheet_CN_V1.2.pdf)
- [CW32F002_UserManual_CN_V1.4.pdf](https://www.whxy.com/uploads/files/20240920/CW32F002_UserManual_CN_V1.4.pdf)
- [CW32F003_DataSheet_CN_V1.9.pdf](https://www.whxy.com/uploads/files/20251226/CW32F003_DataSheet_CN_V1.9.pdf)
- [CW32F003_UserManual_CN_V2.3.pdf](https://www.whxy.com/uploads/files/20240920/CW32F003_UserManual_CN_V2.3.pdf)
- [CW32F020_UserManual_CN_V1.4.pdf](https://www.whxy.com/uploads/files/20240920/CW32F020_UserManual_CN_V1.4.pdf)
- [CW32F030_DataSheet_CN_V1.9.pdf](https://www.whxy.com/uploads/files/20251229/CW32F030_DataSheet_CN_V1.9.pdf)
- [CW32L010_DataSheet_CN_V1.3.pdf](https://www.whxy.com/uploads/files/20260416/CW32L010_DataSheet_CN_V1.3.pdf)
- [CW32L010_UserManual_CN_V1.2.pdf](https://www.whxy.com/uploads/files/20260626/CW32L010_UserManual_CN_V1.2.pdf)
- [CW32L011_DataSheet_CN_V1.1.pdf](https://www.whxy.com/uploads/files/20250724/CW32L011_DataSheet_CN_V1.1.pdf)
- [CW32L011_UserManual_CN_V1.1.pdf](https://www.whxy.com/uploads/files/20260602/CW32L011_UserManual_CN_V1.1.pdf)
- [CW32L012_DataSheet_CN_V1.0.pdf](https://www.whxy.com/uploads/files/20250717/CW32L012_DataSheet_CN_V1.0.pdf)
- [CW32L012_UserManual_CN_V1.4.pdf](https://www.whxy.com/uploads/files/20260603/CW32L012_UserManual_CN_V1.4.pdf)
- [CW32L031_DataSheet_CN_V1.9.pdf](https://www.whxy.com/uploads/files/20251229/CW32L031_DataSheet_CN_V1.9.pdf)
- [CW32L031_UserManual_CN_V1.6.pdf](https://www.whxy.com/uploads/files/20240920/CW32L031_UserManual_CN_V1.6.pdf)
- [CW32L052_DataSheet_CN_V1.3.pdf](https://www.whxy.com/uploads/files/20251229/CW32L052_DataSheet_CN_V1.3.pdf)
- [CW32L052_UserManual_CN_V1.5.pdf](https://www.whxy.com/uploads/files/20240920/CW32L052_UserManual_CN_V1.5.pdf)
- [CW32L083_DataSheet_CN_V1.9.pdf](https://www.whxy.com/uploads/files/20251229/CW32L083_DataSheet_CN_V1.9.pdf)
- [CW32L083_UserManual_CN_V2.0.pdf](https://www.whxy.com/uploads/files/20240920/CW32L083_UserManual_CN_V2.0.pdf)
- [CW32R031_DataSheet_CN_V1.2.pdf](https://www.whxy.com/uploads/files/20251230/CW32R031_DataSheet_CN_V1.2.pdf)
- [CW32R031_UserManual_CN_V1.3.pdf](https://www.whxy.com/uploads/files/20240920/CW32R031_UserManual_CN_V1.3.pdf)
- [CW32W031_DataSheet_CN_V1.3.pdf](https://www.whxy.com/uploads/files/20251230/CW32W031_DataSheet_CN_V1.3.pdf)
- [CW32W031_UserManual_CN_V1.4.pdf](https://www.whxy.com/uploads/files/20240920/CW32W031_UserManual_CN_V1.4.pdf)
- [CW32x030_UserManual_CN_V2.5.pdf](https://www.whxy.com/uploads/files/20240920/CW32x030_UserManual_CN_V2.5.pdf)
- [CW32F020_DataSheet_CN_V1.3.pdf](https://www.whxy.com/uploads/files/20251230/CW32F020_DataSheet_CN_V1.3.pdf)

## Verification and next implementation gate

- Verified the pinned hashes for 25 selected PDFs and 12 SDK archives; recorded 88 source/index/member fingerprints. Inspected F002/F020 lock-table PDF images and L052 MODE table image.
- Checked 13 generic profiles and all 37 exact ordering-code entries, including 32-KiB F030 and 128-KiB L083 capacity variants. No compile result is presented as hardware proof.
- Before implementing: test every byte/page/protection-group edge, checked integer overflow, empty requests, unsupported generic capacity, partial-error cleanup, all implemented error bits and exact MODE replacement in a host model.
- Before hardware testing: obtain board and destructive storage-partition authorization; ensure no code/data overlap; validate own VDD/HCLK/cache combinations and neighbor preservation. Test PC/locked/protected errors, IRQ/watchdog latency, last legal addresses and L083 page 255/256/511 transitions.
- Reset/power-fail recovery, guaranteed execution latency and irreversible protection changes remain outside the first batch. This audit neither executed nor scheduled a destructive test.
