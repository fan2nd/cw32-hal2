# Qualified native CW32L011/CW32L012 LSE facts

2026-10-09. This source/data change implements the accepted bounded design. It is not electrical, silicon, runtime, or build acceptance. Cargo, the generator and tests were not run while authoring this slice. The seven original PDFs/SDK archives were checked against the current canonical source-lock hashes; vendor payloads remain local evidence and are not copied into this deliverable.

## Exact scope and independent sources

| Family | Exact parts and packages | OSC32_IN / OUT | Factory monitor upper bound | Native LSI TRIM |
|---|---|---|---:|---:|
| CW32L011 | K8T6 LQFP32; K8U6 QFN32 | PC14 pin 2 / PC15 pin 3 | 41000 Hz | 10 bits |
| CW32L012 | C8T6 LQFP48; C8U6 QFN48 | PC14 pin 3 / PC15 pin 4 | 36080 Hz | 9 bits |

L011 uses its CN RM Rev1.1 June 2026 (`b245887e9037caf267f739e25bd3c354c2579d23315fc06389e055f371159f4f`), DS CN Rev1.1 (`0b7414049824881920fc38f829029e3ba0af88feb4536fb27351df0d60f688a5`), and SDK1.0.3 (`76adfe39360eb1d05ef58c25f26a8c1f99f2bc2f8fef677214aaca85cffc679e`). L012 uses its CN RM Rev1.4 (`a9e54694a26f03c1f3e3041f40844900168ca2b8e6142f3328671b92e6b7a340`), current EN RM Rev1.0 (`f56d5ed899dd090b469fac6a09084aab9f5068ae3b56cf7562b83997bd2f1088`), DS CN Rev1.0 (`08a605fe8843d1eefa2b1115474b5a74df2833068f0ad6f35225e4f39377da76`), and SDK1.0.5 (`8b0a4c0ee865642d08f353aec98906c900a8b10f672120e649e6b1bf5e29c18f`). The current EN identity is mandatory; its same-filename predecessor is not substituted.

Page notation is one-based PDF/printed. L011 RM pages have offset 1, both datasheets offset 3, and both L012 manuals offset 26. Full selected page rosters and canonical source IDs are in the two `lse-l011-*` and two `lse-l012-*` JSON receipts. Their current content hashes are bound by `lse-qualified.yaml`; these are current qualification inputs, not historical inverse-hash gates.

## Source facts used by the validator

Both own manuals define LSE at +0x24 with DRIVER[3:0], WAITCYCLE[5:4], MODE6, PDRIVER[11:8], PINLOCK17 and STABLE18, without amplitude fields. The added `LseDrive` enums encode every value 0..15; `LseWait` encodes 256/1024/4096/16384 edges as 0..3. Sources are L011 RM72/71 and L012 CN77/51, EN82/56. Own startup-count prose establishes CCS0 readiness separately from failure monitoring: L011 RM51/50,55–57/54–56; L012 CN57/31,61–62/35–36, EN59/33,65–66/39–40,70/44. STABLE is latched readiness, not a live frequency or continued-operation measurement.

The new profiles qualify nominal 32768 Hz, the datasheet bypass ceiling of 100000 Hz, 1.7–5.5 V and −40..85 °C. Datasheet and manual claims remain distinct: manual 1 MHz bypass is excluded; crystal startup 1.50 s is typical with no guaranteed maximum. Bypass requires board-qualified duty 45–55%, valid voltage levels, minimum high/low pulses 450 ns and maximum rise/fall 50 ns. Sources are L011 DS39/36,48/45,50–51/47–48 and L012 DS47/44,55/52,57–58/54–55.

Monitored admission uses a pre-existing stable factory-trim-matching LSI. The explicit own LSI halfword address is 0x001007C2, sourced from L011 RM54/53,70/69 and SDK sysctrl header249; L012 CN59/33,75/49, EN63/37,79/53 and SDK sysctrl header217. It is not derived from HSIOSC's 0x001007C0 field. No trim, WAITCYCLE or LSI enable write is qualified. Non-erased factory data, native trim mask and frozen trim/wait are required.

L011 DS51/48 gives factory −10/+25%, hence 29520..41000 Hz; its RM legal/calibration ±10% remains a separate conflicting statement. L012 DS58/55 gives factory ±10%, hence 29520..36080 Hz. Factory matching does not reconcile L011's source discrepancy or establish arbitrary-trim frequency. Hardware detector threshold 128 LSE edges over 256 LSI periods and the engineering margin of 1 edge are separate facts. The sufficient policy is `256 * u64(LSE_min) > (128 + 1) * u64(LSI_max)`, requiring minimum integral declarations 20661 Hz and 18181 Hz. Board/source per-window qualification remains necessary; accuracy tables are not a formal jitter bound.

The native register validator binds own SYSCTRL/RTC/GPIO selected versions, exact gate/reset fields, native UART SOURCE[13:12], LPTIM controls, native RTC controls/PSC and all oscillator/output AF fields. Both new RTCs have only SOURCE[10:8] and WAIT2 in CR1; no ACCESS/WINDOW. Their six read-only admission masks are CR0=0xe7, CR1=0x704, CR2=0x6ff, COMPCFR1=0xffff, IER=ISR=0x5f. LSE calendar divisors are 1,16384,32768, with encoded PSC1=0 and PSC2=0x3fff. Quiet controls do not establish disconnected RTC outputs. Sources: L011 RM139–140/138–139,151–154/150–153; L012 CN190/164,201–204/175–178, EN207–208/181–182,218–221/192–195.

L011 RTC/UART1/2/3/LPTIM gates are configuration-only. L012 RTC/UART1/2/LPTIM/I2C1/2 are configuration-only. L012 UART3 remains disputed: CN83/57 says configuration-only while EN88/62 says configuration-and-work. Its closed gate is never opened merely to inspect. GPIO gates operate the entire bank. Resets are active low; admission does not assert or release them. Own gate bits/offsets are recorded in each receipt and checked against the selected native layout.

L011 LPTIM TRIGSEL[15:13] RTC codes are 1..4; code5 is PC13. L012 TRIGSEL[15:12] RTC codes are 1..5, including RTC_1Hz. L012 I2C1/2 each use master MCR0+0x10 and slave SCR0+0x110 CLKSRC[7:6]=2 for LSE; reserved1 is rejected, while code3's master HSI/LSI conflict is preserved. L011's native I2C CR+0x08 has no corresponding CLKSRC. Sources: L011 RM200–202/199–201,493–496/492–495; L012 CN253/227,573/547,583/557, EN273/247,626/600,636/610.

AF qualification checks complete source rosters and exact physical bonding. L011 RTC_OUT is PA1/PA3 AF3, with no direct LSE_OUT. L012 RTC_OUT is PA1/PA3 AF3 plus PB14/PB15/PC13 AF4; direct LSE_OUT is PB12 AF4, PF1 AF3, PF3/BOOT AF1. Sources: L011 RM122–123/121–122 and DS28–31/25–28; L012 CN154–155/128–129, EN167–168/141–142 and DS35–40/32–37. New oscillator pins use AFRH; L011 native selectors are 3-bit AFR fields and L012 selectors are 4-bit PIN fields.

## Explicit schema migration and preservation

The serialized record is now `native_low_power`, matching `LseNativeLowPower`; no alias accepts `native_l010`. Each native record requires explicit monitor semantics and engineering margin. All three L010 profiles retain their nine original values and routes, `inherited_legal`, 36080 Hz, margin 1 and a null factory address. The original L010 proof documents remain unchanged. After mandatory validation of the new three facts, only comparison to that historical proof projects the record back to its old name and nine fields. This is an explicit evidence comparison, not deserialization compatibility or requalification.

The prior 16 active-LSE profiles keep no native low-power record. Exactly four new parts are added, so the active roster is 23 and the native roster is 7. Existing data tests now cover cross-family rejection, missing profile, missing required monitor/margin, wrong factory address or policy, swapped monitor bounds, changed admission mask/divisors, and unchanged L010 semantics. They remain unexecuted until the parent grants the combined validation lease.

## Remaining runtime and electrical boundaries

Qualification does not resolve PINLOCK fault-time pad semantics, L012 UART3 gating, L012 BTIM RTC selectors3 versus SDK6 or native ATIM TI2/TI5=6 versus generic/SDK8. No positive selector-based absence proof is supplied across these conflicts. Whole-GPIOC sampling/filter/edge capture may advance during allowed inspection. Unopened output banks, RTC/timer roots/cascades, external recipients and inaccessible L012 UART3 ownership require the accepted functional handover. Monitored mode intentionally permits preserved fault routes to capture flags, request IRQs and affect braking/timers. Neither gate restoration nor an init error undoes these asynchronous effects. Safe-call memory safety cannot be delegated to this functional prose.
