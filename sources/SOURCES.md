# YAML 数据与官方原始资料

本页对应当前整合源码及二十三个精确料号/封装的主动 LSE/RTC 资格（2026-10-09），回答“这份 YAML 依据哪一本原厂手册、哪个 SDK、哪几页”。当前源码包附带 `approved-sdk-members/` 下的 11 份 Apache-2.0 芯片主头文件及其许可；完整 PDF、SDK ZIP、SVD 与 HTML 原件未附。下列文档链接直达官网，运行获取脚本后才会写入本地 `sources/vendor/`。

`evidence-sources.json` 是唯一 URL / SHA-256 / SDK 成员锁；本页是阅读入口，不再复制一份 JSON。原件版本是项目选定快照，不能据此声称已是厂商最新版。页码与章节从现有 YAML 及其明确引用的记录摘出，本次未重新逐页校读原件。

- RM = 该族中文用户手册；DS = 该族数据手册；SDK = 该族标准外设库 ZIP。每族下面给出完整文件名及版本。
- “PDF”从 1 起计；“书页”是页面上印刷页码，二者不一定只差 1。源记录的 `*_page_indices` 从 0 起计，本页显示时加 1；未注明基准的自由文本不换算。
- 页码集合仅涵盖已记录的引文，不能代替每条事实的适用条件。SDK 宏/行号留在对应 YAML 的 `source_macro`、`source_line`、`locator` 中；行号不是手册章节。
- SVD 是寄存器导入基线；审核后的 `registers/*.yaml` 还含人工修正。寄存器复用、构建策略、软件保守限值均不自动成为厂商硬件事实。

L010/L011 软件 ADC 中断单次与顺序扫描沿用既有 YAML。其 EOS/EOC、START 清零停止、R1W0 和结果槽依据分别是 L010 RM Rev 1.2 §20.5.1–2、§20.12（PDF 508–510、515、518、523–526）与 L011 RM Rev 1.1 §20.5.1–2、§20.11（PDF 510–512、516、519、525–528）；这些手册的书页为 PDF 页减一。配套 SDK 示例为两族 `Examples/ADC/adc_sqr_irq_sw/USER/src/` 的 `main.c` 与各自 `interrupts_cw32l010.c` / `interrupts_cw32l011.c`，准确压缩包内路径和 SHA 见唯一来源锁，事实说明见 [adc-low-async.md](../docs/adc-low-async.md)。这不补足原手册未提供的硬件排空握手或中止最长延迟。

## 当前文件级来源覆盖

只统计 `cw32-data/` 内的 295 份 authored YAML：137 份寄存器定义 + 158 份其他数据；不含 `cw32-data/data/` 等生成目录。此前新增 `af/cw32f030-atim-complementary.yaml` 与 `af/cw32a030-atim-complementary.yaml`，两份均保留自己的 DS 单元格、共享 x030 RM 单元格及精确 SDK 宏/行号。L010/L011 与 L052/L083 HSE、UART/SPI DMA 和当前异步 ADC 沿用已有硬件 YAML；主动 LSE 的 `lse-qualified.yaml` 直接记录二十三个精确料号/封装的原厂文档 ID、SHA、PDF 页码与书页；完整清单及各族来源见末节。以下四类按顺序互斥，每个文件只计一次。定位可为“官方文档 ID + 章节/页码”，也可为“锁定 SDK 成员 + 宏/行号/SVD 寄存器名”；“直接”表示文件中至少有一处这样的定位，或寄存器基线经既有 canonical/input 映射能定位原 SVD。人工覆盖不因基线可定位而自动通过审查。

| 归类 | 文件数 | 边界 |
| --- | ---: | --- |
| 可直接定位原件中的内容 | 285 | 137 份寄存器有既有 canonical/input → 官方 SVD 结构来源；148 份其他 YAML 至少有文档章节/页码、SDK 符号/行号等入口。 |
| 经明确引用的审阅记录可找到原件与章节/页码 | 10 | 下列十个根级能力文件；正文已摘出其中部分精确引文。 |
| 只找到原文件，未找到上述内容定位 | 0 | 指整个文件没有任何内容定位；不表示每条事实都已有章节。 |
| 连原文件都无法确定 | 0 | 仅表示这次文件级来源链检查没有遇到此类文件。 |

第二类是 `adc-sequences.yaml`、`classic-adc-scans.yaml`、`classic-timer-input.yaml`、`crypto.yaml`、`lcd.yaml`、`lvd-ir.yaml`、`ram-parity.yaml`、`rtc-alarms.yaml`、`rtc-calendar.yaml`、`spi.yaml`。这组计数不是逐字段覆盖率，也不是重新完成硬件审查；一份文件里可以同时有已定位的内容和仍待补全的人工修正/电气条件。

优先检查的边界：`field-access.yaml` 的 UIFCPY 已直接写明 L010/L011 RM §11.9.8（书页 184 / PDF 185）、§13.9.13（276 / 277），以及 L012 RM §14.10.8（208 / 234）、§16.10.13（303 / 329）；RAM 项经明确的同族审阅记录定位。`register-writes.yaml` 的逐命令语义、RCC 的派生文本行号、`electrical.yaml` 的 ADC 限值仍须保留各自 claim 与原文的关系，不能由上述“文件至少一处有定位”推定全部覆盖。

## 按数据主题查找

| YAML | 原始依据与定位方式 |
| --- | --- |
| `inputs/*.yaml`、`registers/*.yaml` | 本页各族的 SDK → SVD / CMSIS；输入中的修正需再看同族 RM。137 份寄存器 YAML 的 SVD 基线可按寄存器/字段名定位；人工修正仍须自己的 RM 引文，复用哈希不代替该引文。 |
| `parts.yaml`、`additional-parts.yaml` | 各族 DS 产品/存储器表与订货表 9-1；DFP `.pdsc` 只作交叉核对。网页目录是产品快照，不裁定寄存器兼容性。 |
| `pinouts/*.yaml`、`af/*.yaml` | 各族 DS 封装/复用表、RM 路由表，以及 SDK GPIO 宏；保留候选、未解决和适用封装标记。新增 F030/A030 `*-atim-complementary.yaml` 的 CH1B–3B 路由及封装定位见下表。 |
| `clock/*.yaml`、`dma/*.yaml`、`triggers/*.yaml` | 各族 RM 时钟、复位、DMA 请求与目标触发表；CMSIS 和外设头文件作编码核对。 |
| `adc-sequences.yaml`、`classic-adc-scans.yaml` | 各族 RM ADC 顺序、启动/结束/结果寄存器；当前 async 的独立 IRQ、有限转换与 R1W0 定位另列如下。电气条件另见 `electrical.yaml`；L012 ADC2 仍仅 blocking。 |
| `classic-timer-input.yaml` | 同族 RM GTIM 捕获/QEI/输入选择与 GPIO AF；SDK GTIM 仅佐证。 |
| `rtc-calendar.yaml`、`rtc-alarms.yaml` | 同族 RM RTC 访问窗口、日历与闹钟；DS 振荡器条件；SDK RTC 佐证。冲突未解决的 Alarm B 不因有引文而变成已验证功能。 |
| `ram-parity.yaml`、`field-access.yaml`、`register-writes.yaml` | 同族 RM 读写属性、状态/清零语义；后两项是字段/命令覆盖，若只写审计文件名，仍欠直接原件定位。 |
| `electrical.yaml`、`spi.yaml` | 同族 DS 电气表和 RM 分频/时序；`electrical.yaml` 大部分旧策略只间接引用审计文件，不能把这些 JSON 当成原厂来源。 |
| `hse-qualified.yaml` | F020/F030/A030/L010/L011/L031/L052/L083/R031/W031 HSE 的各自 RM/DS；L010/L011/L031/L052/L083/R031/W031 的实际版本、章节与页码见下表。保留 RM/DS 外部输入下限冲突的交集，不跨族继承电气限值或 CCS 策略。 |
| `lse-qualified.yaml` | 仅末节二十三个精确料号/封装；x030、F020、L031、R031、W031、L052、L083、L010、L011、L012 各自 RM/DS 与原件对应收据给出原件 ID/SHA、PDF/书页和 SDK 成员定位，其他料号不自动继承主动配置资格。 |
| `pll-qualified.yaml`、`electrical.yaml` 的 PLL、对应 SYSCTRL 模板与 `field-access.yaml` 的 PLL 字段 | CW32L083/F020/F030/A030 的一次性 factory-HSI- 或 HSE-fed 系统 PLL；晶振/旁路共用既有HSE板级契约，各族自己的 RM/DS、原件 SHA 与 PDF/书页见末节。F020 采用 current-datasheets 中 printed Rev1.3，输出交集12–48MHz；L083/F030/A030为12–64MHz。保留模拟档位与电气上限区别、reserved-debug默认0x5、STABLE只读及rate-only时序限制；不外推其他族。 |
| `lsi-sysclk-qualified.yaml`、`electrical.yaml` 的 `lsi_sysclk` | 仅 F020/F030/A030 的 init-only factory-LSI SYSCLK；逐族记录既锁 RM/DS 的 ID、SHA、PDF/书页，以及完整消费者、门控、复位与基址事实。F020 使用 current-datasheets 中真正 printed Rev1.3。32.8 kHz 的同源 RTC/LSI 别名均为 rate-only，其他十族不继承此资格；具体定位与兼容变化见下节。 |
| `hex-qualified.yaml` | F002/F003 各自 RM/DS 的直接 HEX 输入、PB0/PB1 与 AWT 来源；精确 PDF/书页见下表。保留 RM 4–32 MHz 与 DS 1–32 MHz 的交集及全部波形条件，不据此推定晶振、PLL 或失钟恢复能力。 |
| `gpio-interrupt.yaml` | 各族 RM GPIO ICR 和中断表；CMSIS IRQ 枚举。 |
| `reference-dividers.yaml` | L010/L011/L012 RM VC 分压器及 DS 电气范围，SDK VC 头/实现作佐证。 |
| `dac-opa.yaml` | L012 RM DAC/OPA 与 DS §§7.3.14、7.3.18；DAC 电源/参考与模拟建立时间保留原条件。 |
| `accelerators.yaml` | L012 RM Rev 1.4，书页 156–157；只有书页，无直接 PDF 页码。 |
| `crypto.yaml` | L083 RM Rev 2.0 §§26、28，TRNG 书页 536–541、AES 565–570；DS §§4.20–4.21，书页 21–22。 |
| `lcd.yaml` | L052/L083 各自 RM LCD 章节、DS 引脚与电气表；原件关键章节见下方。 |
| `lvd-ir.yaml` | 同族 RM LVD/IR 控制，DS IR 引脚表，SDK GPIO/LVD；不继承别族阈值。 |
| `register-source-aliases.yaml` | A030 DS Rev 1.1 表 6-1 书页 28，与 x030 RM Rev 2.5 表 2-1 书页 26–27、§6.5 书页 108，支持限定的 F030→A030 寄存器映射复用。 |
| `register-reuse.yaml` | 项目对完整标准化寄存器描述的相等性记录；不是原厂手册，不证明电气或行为等价。 |

## F020/F030/A030 factory-LSI SYSCLK 来源与兼容边界

新增的 `lsi-sysclk-qualified.yaml` 只引用已有锁定原件，不增加来源或修改其 SHA。F020 RM Rev1.4 的 CR1、LSI、ready/IRQ、门控分别见 PDF 69、72、76–79、80–85；GPIO、RTC、AWT、UART 的选择器与消费者定位分别在 PDF 140–155、174–186、163–170、267–295，IWDT 独立 RC10K 见 PDF 250–251。F030/A030 共同使用 x030 RM Rev2.5，对应页为 71、74、78–81、82–87，以及 143–158、177–189、166–173、329–357、312–313。这两本 RM 在上述定位的书页均为 PDF 页减一；完整离散页集合与逐字段定位保留在 YAML。

LSI 电气表使用 F020 current DS Rev1.3 PDF 44（书页 43）、F030 DS Rev1.9 PDF 46（45）及 A030 DS Rev1.1 PDF 43（42）。三族标称均为 32800 Hz，适用 1.65–5.5 V、−40–105 °C；F020 速率区间为 31160–34440 Hz，F030/A030 为 31816–33784 Hz。这些是速率界限，没有逐边沿周期或抖动保证。三族现有 `LsiClock`、`CalendarClock::Lsi` 和 RTC 分频后界限统一保留 rate-only 属性，即使 SYSCLK 选 HSI 也如此；严格周期与死区时序请求会被拒绝。其他十族的 RTC/LSI 资格不变。完整 admission、whole-GPIO 功能移交与失败边界见 [factory-lsi-sysclk.md](../docs/factory-lsi-sysclk.md)。

## 13 个芯片族对应的原件版本

共 45 项固定来源：12 个 SDK ZIP、31 个 PDF、2 个 HTML。下表是各族主要来源，补充/历史原件另列；文件名、内部版本和日期分别记录，不能互相替代。

默认获取与硬件审计要求 43 个 PDF/SDK 原件及其锁定派生文件；两份 HTML 是历史发现记录，单独使用 `./d refresh-discovery` 检查当前页面。追加 `--include-discovery` 可严格复验全部 45 项所选原件，仍拒绝缺失或变更的 HTML。此范围划分不改变下列硬件引用，详见 [发现记录策略](../docs/a030-discovery-policy.json)。

| 芯片族 | 用户手册 RM | 数据手册 DS | SDK ZIP |
| --- | --- | --- | --- |
| CW32A030 | [CW32x030_UserManual_CN_V2.5.pdf](https://www.whxy.com/uploads/files/20240920/CW32x030_UserManual_CN_V2.5.pdf)；Rev 2.5，封面 2024-09，修订表 2024-07-24 | [CW32A030_DataSheet_CN_V1.1.pdf](https://www.whxy.com/uploads/files/20251230/CW32A030_DataSheet_CN_V1.1.pdf)；Rev 1.1，发布/修订 2025-12-30 | [CW32F030_StandardPeripheralLib_V2.2.zip](https://www.whxy.com/uploads/files/20241111/CW32F030_StandardPeripheralLib_V2.2.zip)；Rev 2.2，发布/修订 2024-11-11 |
| CW32F002 | [CW32F002_UserManual_CN_V1.4.pdf](https://www.whxy.com/uploads/files/20240920/CW32F002_UserManual_CN_V1.4.pdf)；Rev 1.4，封面 2024-09，修订表 2024-07-19 | [CW32F002_DataSheet_CN_V1.2.pdf](https://www.whxy.com/uploads/files/20251230/CW32F002_DataSheet_CN_V1.2.pdf)；Rev 1.2，发布/修订 2025-12-30 | [CW32F002_StandardPeripheralLib_V1.2.zip](https://www.whxy.com/uploads/files/20240115/CW32F002_StandardPeripheralLib_V1.2.zip)；Rev 1.2，发布/修订 2024-01-15 |
| CW32F003 | [CW32F003_UserManual_CN_V2.3.pdf](https://www.whxy.com/uploads/files/20240920/CW32F003_UserManual_CN_V2.3.pdf)；Rev 2.3，封面 2024-09，修订表 2024-07-24 | [CW32F003_DataSheet_CN_V1.9.pdf](https://www.whxy.com/uploads/files/20251226/CW32F003_DataSheet_CN_V1.9.pdf)；Rev 1.9，发布/修订 2025-12-25 | [CW32F003_StandardPeripheralLib_V1.7.zip](https://www.whxy.com/uploads/files/20250606/CW32F003_StandardPeripheralLib_V1.7.zip)；压缩包名版本 1.7；未核实内部发布版本，发布日期未记录 |
| CW32F020 | [CW32F020_UserManual_CN_V1.4.pdf](https://www.whxy.com/uploads/files/20240920/CW32F020_UserManual_CN_V1.4.pdf)；Rev 1.4，封面 2024-09，修订表 2024-07-19 | [current-datasheets/CW32F020_DataSheet_CN_V1.3.pdf](https://www.whxy.com/uploads/files/20251230/CW32F020_DataSheet_CN_V1.3.pdf)；Rev 1.3，发布/修订 2025-12-30 | [CW32F020_StandardPeripheralLib_V1.2.zip](https://www.whxy.com/uploads/files/20240115/CW32F020_StandardPeripheralLib_V1.2.zip)；Rev 1.2，发布/修订 2024-01-15 |
| CW32F030 | [CW32x030_UserManual_CN_V2.5.pdf](https://www.whxy.com/uploads/files/20240920/CW32x030_UserManual_CN_V2.5.pdf)；Rev 2.5，封面 2024-09，修订表 2024-07-24 | [CW32F030_DataSheet_CN_V1.9.pdf](https://www.whxy.com/uploads/files/20251229/CW32F030_DataSheet_CN_V1.9.pdf)；Rev 1.9，发布/修订 2025-12-29 | [CW32F030_StandardPeripheralLib_V2.2.zip](https://www.whxy.com/uploads/files/20241111/CW32F030_StandardPeripheralLib_V2.2.zip)；Rev 2.2，发布/修订 2024-11-11 |
| CW32L010 | [CW32L010_UserManual_CN_V1.2.pdf](https://www.whxy.com/uploads/files/20260626/CW32L010_UserManual_CN_V1.2.pdf)；Rev 1.2，封面 2026-05，修订表 2025-09-19 | [CW32L010_DataSheet_CN_V1.3.pdf](https://www.whxy.com/uploads/files/20260416/CW32L010_DataSheet_CN_V1.3.pdf)；Rev 1.3，发布/修订 2026-04-16 | [CW32L010_StandardPeripheralLib_V1.0.9.zip](https://www.whxy.com/uploads/files/20260806/CW32L010_StandardPeripheralLib_V1.0.9.zip)；Rev 1.0.9，发布日期未记录 |
| CW32L011 | [CW32L011_UserManual_CN_V1.1.pdf](https://www.whxy.com/uploads/files/20260602/CW32L011_UserManual_CN_V1.1.pdf)；Rev 1.1，封面 2026-06，修订表 2025-09-19 | [CW32L011_DataSheet_CN_V1.1.pdf](https://www.whxy.com/uploads/files/20250724/CW32L011_DataSheet_CN_V1.1.pdf)；Rev 1.1，封面 2025-07，修订表 2025-05-19 | [CW32L011_StandardPeripheralLib_V1.0.3.zip](https://www.whxy.com/uploads/files/20251016/CW32L011_StandardPeripheralLib_V1.0.3.zip)；压缩包名版本 1.0.3；未核实内部发布版本，发布日期未记录 |
| CW32L012 | [CW32L012_UserManual_CN_V1.4.pdf](https://www.whxy.com/uploads/files/20260603/CW32L012_UserManual_CN_V1.4.pdf)；Rev 1.4，封面 2026-06，修订表 2026-06-03 | [CW32L012_DataSheet_CN_V1.0.pdf](https://www.whxy.com/uploads/files/20250717/CW32L012_DataSheet_CN_V1.0.pdf)；Rev 1.0，封面 2025-07，修订表 2025-07-16 | [CW32L012_StandardPeripheralLib_V1.0.5.zip](https://www.whxy.com/uploads/files/20260701/CW32L012_StandardPeripheralLib_V1.0.5.zip)；压缩包名版本 1.0.5；未核实内部发布版本，发布日期未记录 |
| CW32L031 | [CW32L031_UserManual_CN_V1.6.pdf](https://www.whxy.com/uploads/files/20240920/CW32L031_UserManual_CN_V1.6.pdf)；Rev 1.6，封面 2024-09，修订表 2024-07-24 | [CW32L031_DataSheet_CN_V1.9.pdf](https://www.whxy.com/uploads/files/20251229/CW32L031_DataSheet_CN_V1.9.pdf)；Rev 1.9，发布/修订 2025-12-29 | [CW32L031_StandardPeripheralLib_V1.4.zip](https://www.whxy.com/uploads/files/20250721/CW32L031_StandardPeripheralLib_V1.4.zip)；Rev 1.4，发布/修订 2025-07-21 |
| CW32L052 | [CW32L052_UserManual_CN_V1.5.pdf](https://www.whxy.com/uploads/files/20240920/CW32L052_UserManual_CN_V1.5.pdf)；Rev 1.5，封面 2024-09，修订表 2024-07-24 | [CW32L052_DataSheet_CN_V1.3.pdf](https://www.whxy.com/uploads/files/20251229/CW32L052_DataSheet_CN_V1.3.pdf)；Rev 1.3，发布/修订 2025-12-29 | [CW32L052_StandardPeripheralLib_V1.4.zip](https://www.whxy.com/uploads/files/20260309/CW32L052_StandardPeripheralLib_V1.4.zip)；Rev 1.4，发布/修订 2026-03-09 |
| CW32L083 | [CW32L083_UserManual_CN_V2.0.pdf](https://www.whxy.com/uploads/files/20240920/CW32L083_UserManual_CN_V2.0.pdf)；Rev 2.0，封面 2024-09，修订表 2024-07-24 | [CW32L083_DataSheet_CN_V1.9.pdf](https://www.whxy.com/uploads/files/20251229/CW32L083_DataSheet_CN_V1.9.pdf)；Rev 1.9，发布/修订 2025-12-29 | [CW32L083_StandardPeripheralLib_V2.2.zip](https://www.whxy.com/uploads/files/20240821/CW32L083_StandardPeripheralLib_V2.2.zip)；Rev 2.2，发布/修订 2024-08-21 |
| CW32R031 | [CW32R031_UserManual_CN_V1.3.pdf](https://www.whxy.com/uploads/files/20240920/CW32R031_UserManual_CN_V1.3.pdf)；Rev 1.3，封面 2024-09，修订表 2024-07-25 | [CW32R031_DataSheet_CN_V1.2.pdf](https://www.whxy.com/uploads/files/20251230/CW32R031_DataSheet_CN_V1.2.pdf)；Rev 1.2，发布/修订 2025-12-30 | [CW32R031_StandardPeripheralLib_V1.1.zip](https://www.whxy.com/uploads/files/20240115/CW32R031_StandardPeripheralLib_V1.1.zip)；Rev 1.1，发布/修订 2023-11-06 |
| CW32W031 | [CW32W031_UserManual_CN_V1.4.pdf](https://www.whxy.com/uploads/files/20240920/CW32W031_UserManual_CN_V1.4.pdf)；Rev 1.4，封面 2024-09，修订表 2024-07-25 | [CW32W031_DataSheet_CN_V1.3.pdf](https://www.whxy.com/uploads/files/20251230/CW32W031_DataSheet_CN_V1.3.pdf)；Rev 1.3，发布/修订 2025-12-30 | [CW32W031_StandardPeripheralLib_V1.3.zip](https://www.whxy.com/uploads/files/20240119/CW32W031_StandardPeripheralLib_V1.3.zip)；Rev 1.3，发布/修订 2024-01-15 |

A030 没有独立选定 SDK/SVD。只在 `register-source-aliases.yaml` 明确允许的寄存器范围内借用 F030 SDK；A030 的封装、引脚和电气仍以自己的 DS 为准。

## SDK 内的 SVD 和芯片头到底来自哪里

下表路径均从对应 ZIP 根开始；`→` 表示先打开包内 `.pack`，再找其内部成员，不是拼接出来的目录。SVD 未附。本包 11 份可分发芯片主头位于 `approved-sdk-members/sdk-members/<ZIP 名去掉 .zip>/<原始成员路径>`；F002 主头未附。其他外设头文件/实现必须从原 ZIP 取得，YAML 的 `source_ref` 可在唯一来源锁内查到完整成员链。

| SDK 家族 | 芯片主头原始成员 | SVD 原始成员链 |
| --- | --- | --- |
| CW32F002 | `Libraries/inc/cw32f002.h` | `IdeSupport/EWARM/arm/config/debugger/WHXY/CW32F002.svd` |
| CW32F003 | `Libraries/inc/cw32f003.h` | `IdeSupport/MDK/WHXY.CW32F003_DFP.1.0.2.pack` → `SVD/CW32F003.svd` |
| CW32F020 | `Libraries/inc/cw32f020.h` | `IdeSupport/EWARM/arm/config/debugger/WHXY/CW32F020.svd` |
| CW32F030 | `Libraries/inc/cw32f030.h` | `IdeSupport/EWARM/arm/config/debugger/CW/CW32F030.svd` |
| CW32L010 | `CW32L010_StandardPeripheralLib_V1.0.9/Libraries/inc/cw32l010.h` | `CW32L010_StandardPeripheralLib_V1.0.9/IDEsupport/MDK/WHXY.CW32L010_DFP/SVD/CW32L010.svd` |
| CW32L011 | `Libraries/inc/cw32l011.h` | `IDEsupport/MDK/WHXY.CW32L011_DFP.1.0.1.pack` → `SVD/CW32L011.svd` |
| CW32L012 | `Libraries/inc/cw32l012.h` | `IDEsupport/MDK/WHXY.CW32L012_DFP.1.0.2.pack` → `SVD/CW32L012.svd` |
| CW32L031 | `Libraries/inc/cw32l031.h` | `IdeSupport/EWARM/arm/config/debugger/WHXY/CW32L031.svd` |
| CW32L052 | `CW32L052_StandardPeripheralLib_V1.4/Libraries/inc/cw32l052.h` | `CW32L052_StandardPeripheralLib_V1.4/IdeSupport/EWARM/arm/config/debugger/WHXY/CW32L052.svd` |
| CW32L083 | `Libraries/inc/cw32l083.h` | `IdeSupport/MDK/WHXY.CW32L083_DFP.1.0.9.pack` → `SVD/CW32L083.svd` |
| CW32R031 | `Libraries/inc/cw32r031.h` | `IdeSupport/MDK/WHXY.CW32R031_DFP.1.0.2.pack` → `SVD/CW32R031.svd` |
| CW32W031 | `Libraries/inc/cw32w031.h` | `IdeSupport/MDK/WHXY.CW32W031_DFP.1.0.2.pack` → `SVD/CW32W031.svd` |

例如 L010 的 ZIP 原来就含 `CW32L010_StandardPeripheralLib_V1.0.9/` 顶层目录，所以交付路径里 SDK 名会出现两次；这是保留原成员路径，并非多复制了一份。

## 已记录的型号、引脚和 DMA 页码

PDF 页从 1 起计；“书页”沿用印刷页码。以下各行使用上表同族原件，另标英文手册或历史 DS 的除外。pinouts 行只定位物理引脚表，不能替代 AF/模拟输入/比较器路由的独立事实。

| 芯片族 | `parts.yaml` / `additional-parts.yaml` | `pinouts/<family>.yaml` | `dma/<family>.yaml` |
| --- | --- | --- | --- |
| CW32A030 | DS：PDF 8, 29, 63；书页 62；表 Table 9-1 (minimum order quantities) | DS：PDF 22–25 | CW32x030_UserManual_EN_V1.0.pdf：PDF 98–99, 131, 148–149；书页 97–98, 130, 147–148；§5.4, Table 5-1; 8.2; 8.8.4; 8.8.4, DMA_TRIGy.HARDSRC[7:2] |
| CW32F002 | DS：PDF 8, 26, 58；书页 57；表 Table 9-1 (minimum order quantities) | DS：PDF 22–23 | DS：PDF 26；§6, Table 6-1: complete memory/peripheral map |
| CW32F003 | DS：PDF 8, 27, 62；书页 61；表 Table 9-1 (minimum order quantities) | DS：PDF 23–24 | DS：PDF 27；§6, Table 6-1: complete memory/peripheral map |
| CW32F020 | DS：PDF 8, 30, 69；书页 68；表 Table 9-1 (minimum order quantities) | DS：PDF 21, 23–26 | RM：PDF 94, 123, 138；书页 93, 122, 137；§5.4, Table 5-1; 8.2; 8.8.4; 8.8.4, DMA_TRIGy.HARDSRC[7:2] |
| CW32F030 | DS：PDF 8, 32, 75–77；书页 75；表 Table 9-1 (minimum order quantities) | DS：PDF 22, 24–28 | CW32x030_UserManual_EN_V1.0.pdf：PDF 98–99, 131, 148–149；书页 97–98, 130, 147–148；§5.4, Table 5-1; 8.2; 8.8.4; 8.8.4, DMA_TRIGy.HARDSRC[7:2] |
| CW32L010 | DS：PDF 8, 27, 66；书页 65；表 Table 9-1 (minimum order quantities) | DS：PDF 24–25 | DS：PDF 27；§6, Table 6-1: complete memory/peripheral map |
| CW32L011 | DS：PDF 8, 34, 71；书页 68；表 Table 9-1 (minimum order quantities) | DS：PDF 29–31 | DS：PDF 34；§6, Table 6-1: complete memory/peripheral map |
| CW32L012 | DS：PDF 8, 41, 81；书页 78；表 Table 9-1 (minimum order quantities) | DS：PDF 35–38 | RM：PDF 96, 133, 147；书页 70, 107, 121；§5.4, Table 5-1; 8.2; 8.8.4; 8.8.4, DMA_TRIGy.HARDSRC[7:2] |
| CW32L031 | DS：PDF 10, 32, 78；书页 77；表 Table 9-1 (minimum order quantities) | DS：PDF 26–29 | RM：PDF 91, 120, 134；书页 90, 119, 133；§5.4, Table 5-1; 8.2; 8.8.4; 8.8.4, DMA_TRIGy.HARDSRC[7:2] |
| CW32L052 | DS：PDF 10, 36, 79；书页 78；表 Table 9-1 (minimum order quantities) | DS：PDF 26–30 | RM：PDF 96, 125, 139；书页 95, 124, 138；§5.4, Table 5-1; 8.2; 8.8.4; 8.8.4, DMA_TRIGy.HARDSRC[7:2] |
| CW32L083 | DS：PDF 10, 40, 86；书页 85；表 Table 9-1 (minimum order quantities) | DS：PDF 27–33 | RM：PDF 105, 135, 148–150；书页 104, 134, 147–149；§5.4, Table 5-1; 8.2; 8.8.4; 8.8.4, DMA_TRIGy.HARDSRC[7:2] |
| CW32R031 | DS：PDF 11, 35, 73；书页 72；表 Table 9-1 (minimum order quantities) | DS：PDF 29–31 | RM：PDF 93, 122, 136；书页 92, 121, 135；§5.4, Table 5-1; 8.2; 8.8.4; 8.8.4, DMA_TRIGy.HARDSRC[7:2] |
| CW32W031 | DS：PDF 9, 34, 71；书页 70；表 Table 9-1 (minimum order quantities) | DS：PDF 27–30 | RM：PDF 92, 121, 135；书页 91, 120, 134；§5.4, Table 5-1; 8.2; 8.8.4; 8.8.4, DMA_TRIGy.HARDSRC[7:2] |

## 当前异步 ADC 的补充依据

13 个芯片族已有软件触发的有限单次/扫描 async：L010/L011 的 ADC、L012 的 ADC1，以及 F002/F003/F020/F030/A030/L031/L052/L083/R031/W031。L012 ADC2 的转换证据齐全，但 ADC2_DAC 共享 IRQ 缺少可约束 DAC 同伴的所有权/故障处理，仍只提供 blocking；不能由 ADC1 的支持推定 ADC2 async。现行说明见 [adc-remaining-async.md](../docs/adc-remaining-async.md)，精确来源 ID 与逐项页码见其 [证据记录](../docs/adc-remaining-async-evidence.json)。这些记录引用上表已经锁定的手册和 CMSIS 芯片头，没有新增原厂文件或 SDK 示例依赖。

下表从各族当前记录摘出专用 ADC IRQ12、MODE0 单次、MODE4 扫描及 START/IER/ISR/ICR 的页码；均为“PDF / 书页”。版本和文件名沿用上方各族原件表；F030/A030 明确共用 x030 RM，其余逐族使用自己的 RM。

| 芯片族 / RM 版本 | IRQ（§5.4） | MODE0 / MODE4 | START、IER / ISR / R1W0 ICR |
| --- | --- | --- | --- |
| F030、A030 / x030 CN Rev 2.5 | 96 / 95 | 440 / 439；447 / 446 | 467 / 466；468 / 467；469 / 468 |
| F002 / CN Rev 1.4 | 72 / 71 | 307 / 306；314 / 313 | 332 / 331；333 / 332；334 / 333 |
| F003 / CN Rev 2.3 | 74 / 73 | 367 / 366；374 / 373 | 394 / 393；395 / 394；396 / 395 |
| F020 / CN Rev 1.4 | 94 / 93 | 378 / 377；385 / 384 | 405 / 404；406 / 405；407 / 406 |
| L031 / CN Rev 1.6 | 91 / 90 | 437 / 436；444 / 443 | 466 / 465；467 / 466；468 / 467 |
| L052 / CN Rev 1.5 | 96 / 95 | 474 / 473；481 / 480 | 506 / 505；507 / 506；508 / 507 |
| L083 / CN Rev 2.0 | 105 / 104 | 479 / 478；486 / 485 | 508 / 507；509 / 508；510 / 509 |
| R031 / CN Rev 1.3 | 93 / 92 | 440 / 439；447 / 446 | 468 / 467；469 / 468；470 / 469 |
| W031 / CN Rev 1.4 | 92 / 91 | 440 / 439；447 / 446 | 469 / 468；470 / 469；471 / 470 |

L012 使用 `CW32L012_UserManual_CN_V1.4.pdf` Rev 1.4：§5.4 IRQ 表 PDF 96–97 / 书页 70–71；§25.5.2 有限扫描 607–608 / 581–582；§25.5.1 与 §25.12.2 停止/游标复位 605、616 / 579、590；§25.10、§25.12.8–10 中断访问 613、622–623 / 587、596–597；结果/BGR 624–625 / 598–599；DAC 共享中断源 §26.8 表 26-2 为 633 / 607。经典系列 START=0 只采用其手册所述停止语义，不移植 L010/L011 的显式游标复位结论；这批功能均未据此承诺独立排空握手、最长模拟中止延迟或硬件实测结果。

## 已记录的外设引文入口

这部分摘取明确归属原件的页码。它是查阅入口，不是对每条字段/电气值的完整覆盖证明；详细 claim 仍应保留在相邻 YAML 或其明确引用记录中。这里只复用已记录的定位，本次没有重新逐页校读 PDF。

| YAML / 芯片族 | 原件定位 |
| --- | --- |
| `af/cw32f030-atim-complementary.yaml`、`af/cw32a030-atim-complementary.yaml` / 互补 PWM | 共享 RM `CW32x030_UserManual_CN_V2.5.pdf` Rev 2.5：控制/预装载/时钟与命令语义书页 253–255、268、271、277–278、295–307（PDF 254–256、269、272、278–279、296–308）；死区公式 PDF 279 / 书页 278，DTR 表 PDF 306 / 书页 305，ICR R1W0 为 PDF 301 / 书页 300。死区采用 TCLK=PCLK/PRS；最大 1010 ticks；第四段下界按公式和 DTR 表取 514 ticks，保留正文 63.25 µs 与正确 64.25 µs 的冲突。范围仅 F030/A030 三组完整 A+B 对、固定死区与全局 MOE，不推出 BK、逐对门控或制动恢复保证。 |
| 同上 / F030 与 A030 各自 AF 和封装 | `CW32F030_DataSheet_CN_V1.9.pdf` Rev 1.9：表 5-3/5-4 PDF 29–30 / 书页 28–29，已用封装单元格 PDF 25–27；`CW32A030_DataSheet_CN_V1.1.pdf` Rev 1.1：表 5-3/5-4 PDF 26–27 / 书页 25–26，封装单元格 PDF 23–24。共享 x030 CN Rev 2.5 RM 路由单元格 PDF 147–148 / 书页 146–147；F030 SDK V2.2 的 `Libraries/inc/cw32f030_gpio.h` 宏行 833、892、900、908、924、932、1003、1011、1019 作佐证。两族各 9 条 CH1B–3B 路由，精确封装投影和原件哈希保留在 YAML；不能把 F030 封装套到 A030。 |
| `register-writes.yaml` / classic ATIM ICR | 原件 R1W0 页码：x030 CN Rev 2.5 PDF 301 / 书页 300；F003 CN Rev 2.3 234 / 233；L031 CN Rev 1.6 292 / 291；L052 CN Rev 1.5 329 / 328；L083 CN Rev 2.0 341 / 340；R031 CN Rev 1.3、W031 CN Rev 1.4 均 295 / 294。各自完整 RM 文件名见上表；三种既有 ATIM 寄存器变体的命令旁表保留逐项直接引文，不扩展这些族的互补 PWM HAL 范围。 |
| `dma/cw32f030.yaml`、`dma/cw32a030.yaml` / staged UART、SPI DMA | 当前行为复核使用 `CW32x030_UserManual_CN_V2.5.pdf` Rev 2.5：§8.4.1、§8.4.4 表 8-2、§8.5–8.6、§8.8.2–3 为 PDF 128、131–133、135–136、139–140 / 书页 127、130–132、134–135、138–139；UART TX/RX §18.6、§18.7.1.5–6 为 PDF 345、350–351 / 书页 344、349–350，接收/ICR 为 PDF 339、359–361 / 书页 338、358–360；SPI 成对字节流程 §19.6.1.3 为 PDF 382–383 / 书页 381–382，状态/错误/请求为 PDF 376–379、389–394 / 书页 375–378、388–393。YAML 原有 EN V1.0 路由页码仍保留，不能改标成 CN V2.5。 |
| `dma/cw32l083.yaml` / staged UART1–6、SPI1–2 DMA | 独立依据 `CW32L083_UserManual_CN_V2.0.pdf` Rev 2.0：§8.4.4/8.5 为 PDF 141、143 / 书页 140、142；§8.6/8.8.3–4 为 PDF 144、148–150 / 书页 143、147–149；§5.4 IRQ 表为 PDF 105–106 / 书页 104–105；UART §19.6/19.7.1.5–6 为 PDF 385、390–391 / 书页 384、389–390，接收/ICR 为 PDF 379、384、401 / 书页 378、383、400；SPI §20.5/20.6.1.3 为 PDF 418、421–422 / 书页 417、420–421，错误/破坏性操作为 PDF 416–417、432–433 / 书页 415–416、431–432。SDK V2.2 `Libraries/inc/cw32l083_dma.h` 仅核对选择码，不从 x030 或软件搬运支持外推外设 DMA。 |
| `hex-qualified.yaml` / CW32F002 | RM：PDF 42, 47, 54, 55, 57, 129；书页 41, 46, 53, 54, 56, 128 / DS：PDF 22, 30, 31, 36, 37, 40；书页 21, 29, 30, 35, 36, 39 |
| `hex-qualified.yaml` / CW32F003 | RM：PDF 44, 49, 56, 57, 59, 131；书页 43, 48, 55, 56, 58, 130 / DS：PDF 23, 31, 32, 37, 38, 41；书页 22, 30, 31, 36, 37, 40 |
| `dma/cw32l083.yaml` / CW32L083，软件 BLOCK 补充 | RM：§§8.4.1–8.4.2、8.5 PDF 137–139, 143 / 书页 136–138, 142；§§8.6、8.8.3、8.8.4 PDF 144, 148, 150 / 书页 143, 147, 149；§§2.3、6.1–6.3 PDF 33, 115–116 / 书页 32, 114–115；§§4.7.12、4.7.15 PDF 86, 90 / 书页 85, 89。仅定位软件搬运、成功完成/SRAM/共享门控证据，不把 EN 清零或 TE 当作总线排空证明。 |
| `adc-sequences.yaml` / CW32L010 | RM：PDF 507–510, 517–519, 523–526；书页 506–509, 516–518, 522–525 |
| `adc-sequences.yaml` / CW32L011 | RM：PDF 509–512, 518–520, 525–528；书页 508–511, 517–519, 524–527 |
| `adc-sequences.yaml` / CW32L012 | DS：PDF 63–64, 66；书页 60–61, 63 / RM：PDF 601–608, 611–612, 615–617, 623–625；书页 575–582, 585–586, 589–591, 597–599 |
| `classic-adc-scans.yaml` / CW32A030, CW32F030 | RM：PDF 436, 438, 447–448, 460–464, 467–470, 473；书页 435, 437, 446–447, 459–463, 466–469, 472；§22.12; 22.13 ADC_ICR; 22.13 ADC_ISR; 22.13 ADC_RESULT0-ADC_RESULT3; 22.13 ADC_START/ADC_IER; 22.13.1; 22.13.2; 22.13.4; 22.4.1; 22.4.3-22.4.4; 22.5.5; 23.3.1 |
| `classic-adc-scans.yaml` / CW32F002 | RM：PDF 303, 305, 314–315, 326–329, 332–335；书页 302, 304, 313–314, 325–328, 331–334；§19.11; 19.12 ADC_ICR; 19.12 ADC_ISR; 19.12 ADC_RESULT0-ADC_RESULT3; 19.12 ADC_START/ADC_IER; 19.12.1; 19.12.2; 19.12.4; 19.4.1; 19.4.3-19.4.4; 19.5.5 |
| `classic-adc-scans.yaml` / CW32F003 | RM：PDF 363, 365, 374–375, 387–391, 394–397, 400；书页 362, 364, 373–374, 386–390, 393–396, 399；§20.12; 20.13 ADC_ICR; 20.13 ADC_ISR; 20.13 ADC_RESULT0-ADC_RESULT3; 20.13 ADC_START/ADC_IER; 20.13.1; 20.13.2; 20.13.4; 20.4.1; 20.4.3-20.4.4; 20.5.5; 21.3.1 |
| `classic-adc-scans.yaml` / CW32F020 | RM：PDF 374, 376, 385–386, 398–402, 405–408, 411；书页 373, 375, 384–385, 397–401, 404–407, 410；§21.12; 21.13 ADC_ICR; 21.13 ADC_ISR; 21.13 ADC_RESULT0-ADC_RESULT3; 21.13 ADC_START/ADC_IER; 21.13.1; 21.13.2; 21.13.4; 21.4.1; 21.4.3-21.4.4; 21.5.5; 22.3.1 |
| `classic-adc-scans.yaml` / CW32L031 | RM：PDF 433, 435, 444–445, 458–463, 466–470, 473；书页 432, 434, 443–444, 457–462, 465–469, 472；§22.12; 22.13 ADC_ICR; 22.13 ADC_ISR; 22.13 ADC_RESULT0-ADC_RESULT7; 22.13 ADC_START/ADC_IER; 22.13.1; 22.13.2; 22.13.4-22.13.5; 22.4.1; 22.4.3-22.4.4; 22.5.5; 23.3.1 |
| `classic-adc-scans.yaml` / CW32L052 | RM：PDF 470, 472, 481–482, 496–501, 506–510, 513；书页 469, 471, 480–481, 495–500, 505–509, 512；§23.12; 23.13 ADC_ICR; 23.13 ADC_ISR; 23.13 ADC_RESULT0-ADC_RESULT7; 23.13 ADC_START/ADC_IER; 23.13.1; 23.13.2; 23.13.4-23.13.5; 23.4.1; 23.4.3-23.4.4; 23.5.5; 24.3.1 |
| `classic-adc-scans.yaml` / CW32L083 | RM：PDF 475, 477, 486–487, 500–505, 508–512, 515；书页 474, 476, 485–486, 499–504, 507–511, 514；§23.12; 23.13 ADC_ICR; 23.13 ADC_ISR; 23.13 ADC_RESULT0-ADC_RESULT7; 23.13 ADC_START/ADC_IER; 23.13.1; 23.13.2; 23.13.4-23.13.5; 23.4.1; 23.4.3-23.4.4; 23.5.5; 24.3.1 |
| `classic-adc-scans.yaml` / CW32R031 | RM：PDF 436, 438, 447–448, 460–465, 468–472, 475；书页 435, 437, 446–447, 459–464, 467–471, 474；§22.12; 22.13 ADC_ICR; 22.13 ADC_ISR; 22.13 ADC_RESULT0-ADC_RESULT7; 22.13 ADC_START/ADC_IER; 22.13.1; 22.13.2; 22.13.4-22.13.5; 22.4.1; 22.4.3-22.4.4; 22.5.5; 23.3.1 |
| `classic-adc-scans.yaml` / CW32W031 | RM：PDF 436, 438, 447–448, 461–466, 469–473, 476；书页 435, 437, 446–447, 460–465, 468–472, 475；§22.12; 22.13 ADC_ICR; 22.13 ADC_ISR; 22.13 ADC_RESULT0-ADC_RESULT7; 22.13 ADC_START/ADC_IER; 22.13.1; 22.13.2; 22.13.4-22.13.5; 22.4.1; 22.4.3-22.4.4; 22.5.5; 23.3.1 |
| `rtc-alarms.yaml` / CW32A030, CW32F030 | RM：PDF 179, 182, 188–189, 191–192, 194–196；书页 178, 181, 187–188, 190–191, 193–195 |
| `rtc-alarms.yaml` / CW32F020 | RM：PDF 176, 179, 185–186, 188–189, 191–193；书页 175, 178, 184–185, 187–188, 190–192 |
| `rtc-alarms.yaml` / CW32L010, CW32L011 | RM：PDF 143, 146, 152–153, 156–160；书页 142, 145, 151–152, 155–159 |
| `rtc-alarms.yaml` / CW32L012 | RM：PDF 194, 197, 202–203, 206–210；书页 168, 171, 176–177, 180–184 |
| `rtc-alarms.yaml` / CW32L031 | RM：PDF 171, 174, 180–181, 183–184, 186–188；书页 170, 173, 179–180, 182–183, 185–187 |
| `rtc-alarms.yaml` / CW32L052 | RM：PDF 184, 187, 193–194, 196–197, 199–201；书页 183, 186, 192–193, 195–196, 198–200 |
| `rtc-alarms.yaml` / CW32L083 | RM：PDF 196, 199, 205–206, 208–209, 211–213；书页 195, 198, 204–205, 207–208, 210–212 |
| `rtc-alarms.yaml` / CW32R031 | RM：PDF 173, 176, 182–183, 185–186, 188–190；书页 172, 175, 181–182, 184–185, 187–189 |
| `rtc-alarms.yaml` / CW32W031 | RM：PDF 172, 175, 181–182, 184–185, 187–189；书页 171, 174, 180–181, 183–184, 186–188 |
| `ram-parity.yaml` / CW32A030, CW32F030 | DS：PDF 4, 9；书页 3, 8 / RM：PDF 106–111；书页 105–110 |
| `ram-parity.yaml` / CW32F002 | DS：PDF 4, 9；书页 3, 8 / RM：PDF 82–87；书页 81–86 |
| `ram-parity.yaml` / CW32F003 | DS：PDF 4, 9；书页 3, 8 / RM：PDF 84–89；书页 83–88 |
| `ram-parity.yaml` / CW32F020 | DS旧版：PDF 4, 9；书页 3, 8 / RM：PDF 104–109；书页 103–108 |
| `ram-parity.yaml` / CW32L010 | DS：PDF 4, 9；书页 3, 8 / RM：PDF 99–103；书页 98–102 |
| `ram-parity.yaml` / CW32L011 | DS：PDF 4, 10；书页 1, 7 / RM：PDF 97–101；书页 96–100 |
| `ram-parity.yaml` / CW32L012 | DS：PDF 4, 10；书页 1, 7 / RM：PDF 106–111；书页 80–85 |
| `ram-parity.yaml` / CW32L031 | DS：PDF 5, 11；书页 4, 10 / RM：PDF 101–106；书页 100–105 |
| `ram-parity.yaml` / CW32L052 | DS：PDF 5, 11；书页 4, 10 / RM：PDF 106–111；书页 105–110 |
| `ram-parity.yaml` / CW32L083 | DS：PDF 5, 11；书页 4, 10 / RM：PDF 115–120；书页 114–119 |
| `ram-parity.yaml` / CW32R031 | DS：PDF 5, 12；书页 4, 11 / RM：PDF 103–108；书页 102–107 |
| `ram-parity.yaml` / CW32W031 | DS：PDF 4, 10；书页 3, 9 / RM：PDF 102–107；书页 101–106 |
| `reference-dividers.yaml` / CW32L010 | DS：PDF 52；书页 51 / RM：PDF 80, 83, 529, 534–536；书页 79, 82, 528, 533–535 |
| `reference-dividers.yaml` / CW32L011 | DS：PDF 58；书页 55 / RM：PDF 78, 81, 531, 536–538；书页 77, 80, 530, 535–537 |
| `reference-dividers.yaml` / CW32L012 | DS：PDF 66；书页 63 / RM：PDF 83–84, 87, 645–646, 651–653；书页 57–58, 61, 619–620, 625–627 |
| `accelerators.yaml` / CW32L012 | RM 书页 156–157；Q1.31 数学加速器定义域。未记录直接 PDF 页码。 |
| `dac-opa.yaml` / CW32L012 | DS §7.3.14 表 7-30 书页 62、§7.3.18 表 7-34 书页 65；RM §26.2 书页 601、§25.12.19 书页 599。这里保留 YAML 自由文本页码，未换算 PDF 页码。 |
| `crypto.yaml` / CW32L083 | RM §§26.1–26.6 书页 536–541（TRNG）、§§28.1–28.5 书页 565–570（AES）；DS §§4.20–4.21 书页 21–22。 |
| `lcd.yaml` / CW32L052 | RM §§26.3.2–26.3.7; 26.4; 26.6–26.7; 4.3.6, 4.5.5, 4.7.2, 4.7.5 (LSI enable, stable and prohibition on live trim changes)；DS §§5.2 Table 5-2; 7.3.8 Table 7-18; 7.3.17 Table 7-33 |
| `lcd.yaml` / CW32L083 | RM §§27.3.2–27.3.7; 27.4; 27.6–27.7; 4.3.6, 4.5.5, 4.7.2, 4.7.5 (LSI enable, stable and prohibition on live trim changes)；DS §§5.2 Table 5-2; 7.3.8 Table 7-18; 7.3.18 Table 7-34 |
| `hse-qualified.yaml`、`registers/sysctrl_cw32l010_v1.yaml`、`field-access.yaml` / CW32L010 | RM `CW32L010_UserManual_CN_V1.2.pdf` Rev 1.2：HSE/HSI/CCS 为 PDF 49–59、67–73 / 书页 48–58、66–72；HSE 原生字段与 STABLE RO 为 PDF 73 / 书页 72；故障/就绪标志为 PDF 76–78 / 书页 75–77。Flash 为 PDF 114 / 书页 113；GPIO 为 PDF 126 / 书页 125；RTC/AWT 为 PDF 148–153 / 书页 147–152；ADC/BGR 为 PDF 504、513、517 / 书页 503、512、516；VC 输入与 PCLK blanking 为 PDF 537–539 / 书页 536–538；LVD 为 PDF 543、547–548 / 书页 542、546–547。DS `CW32L010_DataSheet_CN_V1.3.pdf` Rev 1.3：OSC_IN PA0 / OSC_OUT PA1、PA0 与 VC1_CH1 交叉为表5-2 PDF24 / 书页23；供电/HSE/HSI/LSI 电气表为 PDF32、39–43 / 书页31、38–42。SDK V1.0.9 的 `Libraries/inc/cw32l010_sysctrl.h`、`Libraries/src/cw32l010_sysctrl.c` 只佐证；驱动码8–15未资格。初期旁路4–32MHz是对原件1–32MHz的明确软件收窄，无 FREQRANGE/PDR，继承CCS强制4MHz回退。 |
| `hse-qualified.yaml`、`registers/sysctrl_cw32l011_v1.yaml`、`field-access.yaml` / CW32L011 | RM `CW32L011_UserManual_CN_V1.1.pdf` Rev 1.1（选定2026-06-02上传字节）：HSE/HSI/CCS 为 PDF47–57、65–71 / 书页46–56、64–70；HSE 原生字段与 STABLE RO 为 PDF71 / 书页70；故障/就绪标志为 PDF74–76 / 书页73–75。Flash 为 PDF113 / 书页112；GPIO 为 PDF126 / 书页125；RTC/AWT 为 PDF148–153 / 书页147–152；ADC/BGR 为 PDF506、518 / 书页505、517；VC 输入/blanking 为 PDF539–541 / 书页538–540；LVD 为 PDF545、549–550 / 书页544、548–549。DS `CW32L011_DataSheet_CN_V1.1.pdf` Rev 1.1：OSC_IN PC13 / OSC_OUT PB7、无VC输入交叉为表5-2 PDF31 / 书页28；供电/HSE/HSI/LSI 为 PDF39、47–51 / 书页36、44–48。SDK V1.0.3 的 `Libraries/inc/cw32l011_sysctrl.h`、`Libraries/src/cw32l011_sysctrl.c` 只佐证。旁路取RM4MHz与DS1MHz下限的4–32MHz交集；LSI RM±10%与DS−10/+25%冲突保留，检测器算术用41kHz上界；无 FREQRANGE/PDR，继承CCS强制4MHz回退。 |
| `hse-qualified.yaml` / CW32L031 | RM `CW32L031_UserManual_CN_V1.6.pdf` Rev 1.6：§4.3.3（HSE）、§4.4.3（稳定/启动失败/运行失效）、§4.7（寄存器）、§11.8.1（AWT 来源）；PDF 49–50、56–59、67–68、71–72、165，书页 48–49、55–58、66–67、70–71、164。DS `CW32L031_DataSheet_CN_V1.9.pdf` Rev 1.9：§7.3.1（运行条件）、§7.3.7（外部时钟）、§7.3.8（保留 HSI）；PDF 26、36、38、44、46–47、50，书页 25、35、37、43、45–46、49。QFN20 无 PF0/PF1。 |
| `hse-qualified.yaml` / CW32R031 | RM `CW32R031_UserManual_CN_V1.3.pdf` Rev 1.3：§4.3.3、§4.4.3、§4.7、§11.8.1；PDF 51–52、58–61、69–70、73–74、167，书页 50–51、57–60、68–69、72–73、166。DS `CW32R031_DataSheet_CN_V1.2.pdf` Rev 1.2：§7.3.1、§7.3.8（外部时钟）、§7.3.9（保留 HSI）；PDF 29、42、51、53–54，书页 28、41、50、52–53。HSE 供电范围 2.2–3.6 V，不据此配置 RF。 |
| `hse-qualified.yaml` / CW32W031 | RM `CW32W031_UserManual_CN_V1.4.pdf` Rev 1.4：§4.3.3、§4.4.3、§4.7、§11.8.1；PDF 50–51、57–60、68–69、72–73、166，书页 49–50、56–59、67–68、71–72、165。DS `CW32W031_DataSheet_CN_V1.3.pdf` Rev 1.3：§7.3.1、§7.3.8（外部时钟）、§7.3.9（保留 HSI）；PDF 30、41、50、52–53，书页 29、40、49、51–52。HSE 保守取 LDO/DCDC 交集 2.0–3.6 V，未扩大 RF 范围，也未缩窄既有 HSI-only 条件。 |
| `hse-qualified.yaml` / CW32L052 | RM `CW32L052_UserManual_CN_V1.5.pdf` Rev 1.5：HSE/预启动参数书页 51–52、74–75（PDF 52–53、75–76），CCS/选择器书页 61、69–70（PDF 62、70–71），Flash 书页 111（PDF 112），AUTOTRIM/RTC/LVD 保留来源书页 174、192、532（PDF 175、193、533）。DS `CW32L052_DataSheet_CN_V1.3.pdf` Rev 1.3：引脚表书页 25（PDF 26），运行条件书页 42（PDF 43），外部时钟/波形/HSI 书页 47、49–50、53（PDF 48、50–51、54）。三种已建模精确封装均有 PF0/PF1。保留旁路 RM 4–32 MHz 与 DS 1–32 MHz 的交集；CLKCCS 固定 HSI/6（标称 8 MHz），不继承 L083 的配置分频回退。自身 SOURCE 枚举、原始锁和完整限制见 [L052 HSE](../docs/qualified-l052-hse.md) 及其[来源记录](../docs/qualified-l052-hse-source-receipt.json)。 |
| `hse-qualified.yaml` / CW32L083 | RM `CW32L083_UserManual_CN_V2.0.pdf` Rev 2.0：HSE/启动 PDF 53–54、62–63、65、70、80 / 书页 52–53、61–62、64、69、79；选择器/CCS/切换 PDF 63、67–69、75–76、79–80、84–86 / 书页 62、66–68、74–75、78–79、83–85；Flash PDF 121、131 / 书页 120、130；GPIO PDF 162–169 / 书页 161–168；RTC/AUTOTRIM/LVD 依赖 PDF 87、89、178、187、205–206、530、534–535 / 书页 86、88、177、186、204–205、529、533–534。DS `CW32L083_DataSheet_CN_V1.9.pdf` Rev 1.9：PF0/PF1 封装表 PDF 27 / 书页 26，电气 PDF 47、52、54–55、58 / 书页 46、51、53–54、57。保留 4–32 MHz 的 RM/DS 交集与独立波形条件；不外推 L052 HSE 或独立 PLL_OUT；本族当前有界系统 PLL 见末节。 |
| `registers/rtc_cw32l031_v1.yaml` / SOURCE 枚举 | 既有完整布局复用组恰为 L031/L083/R031/W031。各自 RM §12.5.3 分别是 Rev 1.6 PDF 180 / 书页 179、Rev 2.0 PDF 205 / 书页 204、Rev 1.3 PDF 182 / 书页 181、Rev 1.4 PDF 181 / 书页 180；均给出 LSE=0、LSI=2、HSE/128,/256,/512,/1024=4,5,6,7。只补类型名，未改变布局、访问属性或复用成员。 |

其他直接引文：`af/*` 的 `datasheet_cell`、`manual_cell`、`manual_pin_cell` 等记录逐路由页码/表格；`triggers/*` 保留逐触发关系的原件记录；`gpio-interrupt.yaml` 有每族 ICR 的手册章节/页码；`hse-qualified.yaml` 有 F020/F030/A030/L031/L052/L083/R031/W031 自己的 RM/DS 页码。它们均对应上表同族原件，原 SDK GPIO/外设头文件由同族 ZIP 提供。

上述 staged DMA 的完整出处与适用条件见 [UART TX](../docs/uart-dma-tx.md)、[UART RX](../docs/uart-dma-rx.md)、[SPI](../docs/spi-dma.md) 和 [L083 独立资格](../docs/l083-peripheral-dma.md)。干净 TC 与 Complete 状态支持成功完成路径；EN 清零、TE 或请求门关闭本身不是总线排空证明。互补 PWM 的精确来源见 [classic ATIM 说明](../docs/classic-atim-complementary-pwm.md)，L083 HSE 逐项来源见 [来源记录](../docs/qualified-l083-hse-source-receipt.json)。这些项目记录是查阅索引，不是原厂资料。

## 补充、历史和非芯片原件

| 原件 | 版本及用途 |
| --- | --- |
| [CW32F020_DataSheet_CN_V1.3.pdf](https://www.whxy.com/uploads/files/20250821/CW32F020_DataSheet_CN_V1.3.pdf) | Rev 1.2，发布/修订 2023-02-14。历史原件：文件名 V1.3，但内页 Rev 1.2，不能写成 Rev 1.3；仍被旧 AF/ADC/RAM 审阅引文使用。 |
| [CW32x030_UserManual_EN_V1.0.pdf](https://www.whxy.com/uploads/files/20240920/CW32x030_UserManual_EN_V1.0.pdf) | Rev 1.0，发布/修订 2022-08-23。补充语言版本，不能替代同族当前选定中文手册的行为修订。 |
| [a030-official-manuals.html](https://www.whxy.com/tongyonggaoxingnengMCU/CW32A030C8T7.html?act=doc&cid=21) | 官网 A030 文档/SDK 分类页的固定捕获；不是产品手册。 |
| [a030-official-sdk.html](https://www.whxy.com/tongyonggaoxingnengMCU/CW32A030C8T7.html?act=doc&cid=22) | 官网 A030 文档/SDK 分类页的固定捕获；不是产品手册。 |
| [NXP_UM10204_Rev7.pdf](https://cache.nxp.com/docs/en/user-guide/UM10204.pdf) | Rev 7.0，封面 2021-10-01，修订表 2021-10-01。I²C 总线协议外部标准，不是 CW32 寄存器手册。 |
| [CW32L012_UserManual_EN_V1.0.pdf](https://www.whxy.com/uploads/files/20260603/CW32L012_UserManual_EN_V1.0.pdf) | Rev 1.0，当前封面 2026-06，修订表仍为 2026-01-16。同一官方 URL 已替换原 January 封面版本；旧身份与有限迁移见 docs/l012-english-source-update.json。补充语言版本，不能替代同族当前选定中文手册的行为修订。 |
| [CW32L052_UserManual_EN_V1.0.pdf](https://www.whxy.com/uploads/files/20240920/CW32L052_UserManual_EN_V1.0.pdf) | Rev 1.0，发布/修订 2023-06-20。补充语言版本，不能替代同族当前选定中文手册的行为修订。 |
| [CW32L083_UserManual_EN_V1.0.pdf](https://www.whxy.com/uploads/files/20240923/CW32L083_UserManual_EN_V1.0.pdf) | Rev 1.0，发布/修订 2022-10-10。补充语言版本，不能替代同族当前选定中文手册的行为修订。 |

CW32L011 中文 Rev 1.1 曾有封面 2025-09 的旧字节快照；本页选定的是封面 2026-06、SHA-256 `b245887e…` 的版本。旧快照未作为现行 YAML 输入，不纳入上述原件清单。文件名或相同 Rev 号不足以替代 SHA-256。

## 仍需补足的引用粒度

- `registers/*.yaml` 共 137 份可通过既有映射定位官方 SVD 基线及寄存器/字段名。需要补的是尚未附明确 RM 出处的人工修正，不要求正常 SVD 字段重复抄一份手册页码。`register-reuse.yaml` 的相等性不证明每个人工修正。
- `clock/*.yaml` 共 13 份已有原件/派生文本哈希，但大部分 claim 使用文本行号，没有对应原 PDF 页码；`source_ref` 只能定位文件，不能当章节。
- `af/cw32f002.yaml`、`af/cw32f003.yaml`、`af/cw32f020.yaml`、`af/cw32f020-serial.yaml`、`af/cw32l010.yaml`、`af/cw32l011.yaml`、`af/cw32l012.yaml`、`af/cw32l031.yaml`、`af/cw32l052.yaml`、`af/cw32l083.yaml`、`af/cw32r031.yaml`、`af/cw32w031.yaml` 在文件内只列 SDK GPIO 宏/行号，缺独立 RM/DS 页码。不能与已逐格核对的同族 `*-serial`、`*-pwm` 等路由混同。
- `register-writes.yaml` 的 classic ATIM ICR 已补同族 RM 直接页码，其余多数命令语义仍只指项目审阅记录；`field-access.yaml` 的 RAM 项只记 §6.6.2，计时器 UIFCPY 项已有明确页码。没有直接页码的条目应回填到相邻 YAML。
- `adc-sequences.yaml`、`classic-adc-scans.yaml`、`classic-timer-input.yaml`、`rtc-calendar.yaml`、`rtc-alarms.yaml`、`ram-parity.yaml`、`lcd.yaml`、`lvd-ir.yaml`、`spi.yaml` 目前主要通过项目审阅记录间接归属原件；上表只能帮助打开原页。精简这些旧记录前，应先把所需 claim 级原件定位并回 YAML。
- `electrical.yaml` 大部分 ADC、时钟、Flash、I²C、IWDT 限值仍经 `policies` 间接引用项目 JSON。少数 factory trim SDK 行号和 HSE 页码不等于所有值都有直接引文。
- `accelerators.yaml`、`dac-opa.yaml` 有原件版本与书页；前者缺直接 PDF 页码，后者部分定位仍是自由文本。
- SDK F003 V1.7、L011 V1.0.3、L012 V1.0.5 目前仅确认 ZIP 文件名版本；未建立内部发布级版本/日期。不能拿某个头文件或 SVD 的版本替代 SDK 发布版本。

## 维护与获取

保留既有 `evidence-sources.json` 一份 URL/hash/member 锁；不新增每族 JSON，也不把项目审计记录冒充原厂资料。下载使用该锁的精确 URL 和 SHA-256；`sources/vendor/` 是获取后的本地缓存目标，不表示本补充包附带完整原件。

`catalog.json` 是已记录的网页产品快照，`layout-history.json` 是旧路径解释记录，均不是寄存器原件。它们仍被维护工具读取，本次整合不改写或新增副本。后续移入维护档案时须同步现有读取路径。

## Inherited LSE pad ownership source additions

`cw32-data/lse-ownership.yaml` adds read-only inherited ownership facts, not LSE
initialization. The original paths below resolve through the existing source
lock, which retains exact URLs and hashes. Page numbers are absolute PDF pages.

| Own family / manual | CR1 / LSE / mode pages | Own datasheet pad page |
|---|---|---|
| F002 CN1.4 | CR1 55, clock tree 40: LSE absent | No OSC32 rows |
| F003 CN2.3 | CR1 57, clock tree 42: LSE absent | No OSC32 rows |
| F020 CN1.4 | 69 / 74 / 49 | CN1.3 current 2025-12-30 PDF 23 |
| F030/A030 x030 CN2.5 (both named) | 71 / 76 / 51 | F030 CN1.9 24; A030 CN1.1 22 |
| L010 CN1.2 | 68 / 74 / 52 | CN1.3 24; PB1/PB0 |
| L011 CN1.1 current revision | 66 / 72 / 50 | CN1.1 29 |
| L012 CN1.4 | 71 / 77 / 56 | CN1.0 35 |
| L031 CN1.6 | 68 / 73 / 52 | CN1.9 26 |
| L052 CN1.5 | 71 / 76 / 55 | CN1.3 26 |
| L083 CN2.0 | 76 / 81 / 56 | CN1.9 27 |
| R031 own CN1.3, MCU only | 70 / 75 / 54 | CN1.2 29 |
| W031 own CN1.4, MCU only | 69 / 74 / 53 | CN1.3 30 |

L010 PINLOCK is independently applicable; L011/L012 require PINLOCK and LSELOCK.
Every present LSE uses MODE=0 crystal and MODE=1 external digital input. A pad
lock reserves both pins even with EN=0; unstable/fault states never release the
software reservation. Other families have no pad-lock field. The exact source
paths and page locators are in the authored YAML and
`docs/lse-pad-source-evidence.json`; the latter is a project evidence receipt,
not an original manufacturer document. Existing own pinout YAML supplies each
package's OSC32 bond-out. `ci/verify-lse-ownership-data.py` checks these projections
against originals without HAL tests or hardware execution.

## 当前有界 PLL 候选的逐项来源

当前 PLL 初始化资格为 CW32L083/CW32F020/CW32F030/CW32A030 的一次性 factory-HSI- 或 HSE-fed 系统模式：`pll-qualified.yaml` 与 `electrical.yaml` 分族记录自己的原件 SHA/页码，并投影为 `ClockLimits.pll`。HSE使用既有Config.hse的晶振或旁路声明，来源码由模式派生为0或1。独立 PLL_OUT、运行时重调、DeepSleep 恢复和保证失钟恢复均不在此范围。L052没有系统PLL，不沿共用HAL文件开放API。

L083 原有资格与来源保持不变：

- 启停、来源、MUL、模拟档位、WAITCYCLE和保留位：L083 RM Rev2.0 §4.3.7、§4.7.8，PDF59–60、82/书页58–59、81。
- 过渡、HSI校准、CCS/状态：同一RM PDF63–65、67、69、71–73、75–78/书页62–64、66、68、70–72、74–77；原始HSI trim地址保持既有权威。
- retained AUTOTRIM/RTC/LVD与输出：同一RM PDF87、89、103、157、159、187、205–206、530、534–535；书页各减一。Flash WAIT/KEY：PDF121、131/书页120、130。
- 工作电压、factory HSI误差和PLL输入/输出/周期间抖动：L083 DS Rev1.9表7-4、7-17、7-21，PDF47、55、56/书页46、54、55。300ps周期间抖动不等于绝对周期误差，rate-only标记保持到严格时序调用点。

F020/F030/A030 的独立新增来源见 [原件对应收据](x030-f020-hsi-pll-source-receipt.json)：

- F020 RM `CW32F020_UserManual_CN_V1.4.pdf` Rev1.4：PLL §4.3.7 PDF52–53/书页51–52，寄存器 §4.7.8 PDF75/74；HSI factory trim PDF55、71/54、70；Flash PDF110、120/109、119。
- F030/A030 共用的 `CW32x030_UserManual_CN_V2.5.pdf` Rev2.5 明确覆盖两族：PLL PDF54–55/53–54，寄存器 PDF77/76；HSI factory trim PDF57、73/56、72；Flash PDF112、122/111、121。F030 SDK仅佐证共用布局，不冒充独立A030 SDK。
- F020 采用 `vendor:current-datasheets/CW32F020_DataSheet_CN_V1.3.pdf`，printed Rev1.3，SHA-256 `1e330d800f10114c654b97cbd45b64d56a99c3cb39138d4ef20de3c12968dab0`：运行条件/HSI/PLL为PDF36、44、45/书页35、43、44；原始输出8–48MHz，factory HSI±5%。同名根目录历史文件实为printed Rev1.2，不用作本项选定来源。
- F030 DS Rev1.9对应PDF38、46、47/书页37、45、46；A030自身DS Rev1.1对应PDF35、43、44/书页34、42、43。两者原始PLL输出8–64MHz、factory HSI±2%。三族采用−40…105°C、1.65–5.5V；低于1.8V时HCLK/PCLK≤24MHz，最终总线仍独立检查。
- 既有HSI路径使用HSI分频后的时钟；新增HSE路径使用未分频的HSE。MUL字面值2–12，输入4–24MHz，WAITCYCLE=7。整段实际输入和倍频输出必须各自落在单一模拟档位，最高输出码取4。F020九组、F030/A030各十二组资格组合逐族列于YAML，未接纳的组合不因此被断言为硬件无效。
- 两个自身手册均给出PLL复位值`0x00053483`、debug[19:16]默认0x5和STABLE只读；SVD继承零复位，F020 SVD称debug字段为RFU，x030 SVD遗漏它。保留/检查默认值的类型化修订、访问旁表和复用账本原子对应；当前IR没有reset槽，手册复位权威记入收据并由来源核验检查，不添加PAC reset API。SDK整寄存器写入清掉debug默认值的做法不采用。

HSE新增功能依据见[七份原件对应收据](../docs/hse-pll-source-receipt.json)及[独立晶振组合审阅](../docs/hse-pll-crystal-contract.md)，旧HSI收据保留其历史范围：

- F020 RM Rev1.4：HSE电路/模式PDF46–47、PLL来源/档位52–53、启动握手55、§4.5.8晶振来源例程66；书页各减一。选定current-datasheets的F020 DS printed Rev1.3：供电36、旁路波形41、晶振说明/电气42–43、PLL表7-21为45；书页各减一。
- x030 RM Rev2.5明确覆盖F030/A030：HSE电路/模式PDF48–49、PLL来源/档位54–55、启动握手57、§4.5.8晶振来源例程68；书页各减一。F030 DS Rev1.9对应供电38、旁路43、晶振44–45、PLL表7-21为47；A030 DS Rev1.1对应35、40、41–42、PLL表7-20为44；书页各减一。
- L083 RM Rev2.0：HSE电路/模式PDF53–54、PLL来源/档位59–60、启动握手62、§4.5.8晶振来源例程73；书页各减一。DS Rev1.9对应供电47、旁路52、晶振53–54、PLL表7-21为56；书页各减一。
- 四族PLL输入条件均为4–24MHz、40–60%占空比。晶振功能接纳依赖厂商明确推荐的内部组合和既有谐振器/负载/驱动/布局契约，不独立证明隐藏节点的数值占空比。旁路须在OSC_IN同时满足40–60%占空比、电平、每高低脉冲≥15ns与边沿≤20ns；STABLE不测量这些条件。全部实际频率端点仍各落单一模拟档位并独立服从raw上限，输出始终rate-only；不保证参考丢失后的PLL频率、回退或CPU继续运行。

来源锁的目录总数仍为45项：硬件出处验证要求43个原件，另2项A030 HTML发现页面按既有 discovery-only 策略单列；11份已批准头文件与许可证边界不变。新增的来源说明和审阅记录均为项目撰写，不重新分发完整PDF、SDK、SVD或其全文抽取。

## Oscillator status access preservation

`cw32-data/field-access.yaml` and `docs/oscillator-status-access-evidence.json` preserve already-correct original SVD field access for L012 HSI/LSI/HSE and all 11 LSE-present families. Each evidence row gives its own RM section, printed/PDF page, URL/hash and selected SVD member/archive. The containing registers remain RW; no vendor SVD, normalized register IR or reuse hash is corrected. This closes a field-access lowering omission only.

## CW32L012 direct HSE qualification

The L012 direct-HSE candidate uses the existing locked own RM CN V1.4 and DS CN
V1.0. The source policy is `cw32-data/hse-qualified.yaml`; its exact projection
and digest are in `cw32-data/electrical.yaml`. The detailed receipt is
`docs/qualified-l012-hse-source-receipt.json`. HSE drive/wait and SYSCLK enums
are semantic additions, with canonical singleton history in `register-reuse.yaml`;
no source offset/width/access change is attributed to these additions. The original
SVD already marks oscillator STABLE fields read-only; its separately accepted
mixed-register access projection is recorded in `docs/oscillator-status-access.md`.

The optional `fixed_ccs_hsi_divisor` describes only an explicitly qualified fixed
HSIOSC fallback divisor: L010 /12, L011 /24, L052 /6, L012 /24. Absence provides
no default/configured fallback inference. `hsi_operating_range_hz` records L012's
own legal incoming 90–100 MHz HSIOSC requirement (RM §4.4.2 PDF59), not a bound
for arbitrary TRIM. Generated L012 startup protection requires that fact.

## 二十三个精确料号/封装的主动 LSE 与 RTC 来源

当前资格以 [lse-qualified.yaml](../cw32-data/lse-qualified.yaml) 的二十三个条目为准：CW32F030C8T7、CW32A030C8T7（均为 LQFP48），CW32F020C6U7（QFN48），CW32L031C8T6（LQFP48）、CW32L031C8U6（QFN48）、CW32L031F8U6（QFN20），CW32R031C8U6（QFN48）、CW32W031R8U6（QFN64），以及 CW32L052C8T6（LQFP48）、CW32L052R8S6（LQFP64 7×7 mm）、CW32L052R8T6（LQFP64 10×10 mm）；另有 CW32L083RBT6、CW32L083RCT6（LQFP64 10×10 mm）、CW32L083RCS6（LQFP64 7×7 mm）、CW32L083MCT6（LQFP80）、CW32L083VCT6（LQFP100）；另有 CW32L010F8P6（TSSOP20）、CW32L010F8U6（QFN20）、CW32L010Y8M6（SOP16）；新增 CW32L011K8T6/K8U6（LQFP32/QFN32）、CW32L012C8T6/C8U6（LQFP48/QFN48）。原有十六个条目的独立监测/入场资格保持不变；原生七款按下节各自契约资格化，L010 维持 inherited_legal，L011/L012 使用 factory_trim，其他料号、封装和族别不自动继承主动配置资格。板级契约及运行边界见 [qualified-lse.md](../docs/qualified-lse.md)。

### x030 两款 LQFP48 的专属引文

本小节只覆盖 CW32F030C8T7 与 CW32A030C8T7。共享 RM 为 `CW32x030_UserManual_CN_V2.5.pdf` Rev 2.5：LSE 控制、电气流程、保留源及 RTC 寄存器相关 PDF 页为 51–52、71、76、94、173、184、187–195、357，书页各减一；RTC 补偿独立消费 LSE 的补充依据见 [lse-active-rtc-admission.json](../docs/lse-active-rtc-admission.json)，不因 RTC.SOURCE 选择 LSI/HSE 而忽略。这些 x030 页码不作为其他族的依据。

各自 DS 为 F030 Rev 1.9（PDF 24、26、30、38、44–45）与 A030 Rev 1.1（PDF 22–23、27、35、41–42），书页各减一。PC14/PC15 引脚、供电/温度、晶体与旁路条件分别取自身表格。这两款 x030 的标称 32768 Hz、精确料号范围、板级每周期边界和保守的全 RTC 复位态入场属于软件资格策略；典型启动时间不作为最长等待保证。

共享 AWT 的 `Source::LSE=3` 还核对了 F020 自身 RM Rev 1.4 §11.8.1（PDF 170、书页169）；F020 的主动配置资格另由下节自身原件审核支持。

### F020 C6U7 QFN48 的自身原件

CW32F020C6U7 使用 `CW32F020_UserManual_CN_V1.4.pdf` 与选定的 `current-datasheets/CW32F020_DataSheet_CN_V1.3.pdf`。原件 ID/SHA、PDF/书页和 SDK 成员定位见 [F020原件对应收据](../docs/lse-f020-source-receipt.json)，逐项事实及 RTC 入场依据见 [F020主动LSE记录](../docs/lse-active-f020.json) 和 [F020 RTC记录](../docs/lse-active-f020-rtc-admission.json)。它不借用 x030 的页码或封装依据。

该精确 QFN48 的 PC14/PC15 为引脚3/4；SDK 的 PC13/PC14 配方不采纳，以自身 DS 为准。F020 的 CLKCCS/HSECCS/LSECCS 必须写1，晶体及旁路均保留自身文档的1 MHz上限。F020F6U7、F020K6U7及族别别名仍无主动LSE资格。

### L031 / R031 / W031 的自身原件与差异

- L031 的 C8T6 LQFP48、C8U6 QFN48、F8U6 QFN20：`CW32L031_UserManual_CN_V1.6.pdf` 与 `CW32L031_DataSheet_CN_V1.9.pdf`；[原件对应收据](../docs/lse-l031-source-receipt.json)、[主动LSE记录](../docs/lse-active-l031.json)、[RTC入场依据](../docs/lse-active-l031-rtc-admission.json)。
- R031 的 C8U6 QFN48：`CW32R031_UserManual_CN_V1.3.pdf` 与 `CW32R031_DataSheet_CN_V1.2.pdf`；[原件对应收据](../docs/lse-r031-source-receipt.json)、[主动LSE记录](../docs/lse-active-r031.json)、[RTC入场依据](../docs/lse-active-r031-rtc-admission.json)。
- W031 的 R8U6 QFN64：`CW32W031_UserManual_CN_V1.4.pdf` 与 `CW32W031_DataSheet_CN_V1.3.pdf`；[原件对应收据](../docs/lse-w031-source-receipt.json)、[主动LSE记录](../docs/lse-active-w031.json)、[RTC入场依据](../docs/lse-active-w031-rtc-admission.json)。

这些收据分别给出自身原件 ID/SHA、PDF/书页与 SDK 成员定位；供电、温度、引脚和寄存器要求按各族记录，不能沿用 x030 的电气条件。三族 CCS 控制可配置，GPIO 均无 SPEED、LOCK、HIGHIE、LOWIE 寄存器；L031F8U6 QFN20 没有直接 LSE 输出 AF 路由，其空路由表是精确封装结论。

三族持有源能力的软件策略要求 LSE 监测。仅在重复确认 LSI 停止且无文档所列使用者时，才允许载入原厂 trim；正在使用的 LSI 不停机、不重调。精确已启用 LSE 的复用不重启或更改 CCS。前述八个条目仍以标称32768 Hz和板级每周期边界资格化；没有自动RTC回退、失钟后的日历连续性或低功耗恢复保证。上述原件、数据、源码及编译检查均不替代实板验证。

### L052 三个精确封装的自身原件

CW32L052C8T6、CW32L052R8S6 与 CW32L052R8T6 使用自己的 `CW32L052_UserManual_CN_V1.5.pdf` Rev 1.5 与 `CW32L052_DataSheet_CN_V1.3.pdf` Rev 1.3；英文 RM Rev 1.0 仅作佐证。原件 ID/SHA、PDF/书页与适用范围见 [L052 原件对应收据](../docs/lse-l052-source-receipt.json)、[专属资格说明](../docs/qualified-l052-lse.md)、[主动 LSE 事实](../docs/lse-active-l052.json) 和 [RTC 入场记录](../docs/lse-active-l052-rtc-admission.json)。SDK V1.4 的精确成员定位与哈希见 [SDK 成员收据](../docs/lse-l052-sdk-member-receipt.json)，SDK 初始化捷径不覆盖自身手册要求。

DS PDF 10、26–27、32–34、43、49–51、79（书页各减一）分别支持精确封装、PC14/PC15 引脚3/4、输出路由及电气边界；PC4/AF6 直接 LSI 输出仅在两款64引脚封装上存在。RM PDF 55–56、61–62、70–83、94、146–147、151–157、167–175、184–200、224–225、235–237、359、390、536、553–554 支持原生 LSE 双模拟参数组、可配置 CCS 与 LSI 检测依赖、GPIO、AUTOTRIM/UART/LPTIM/LCD 使用者及 RTC 复位状态；LSE 复位值0x0A2B、ALARMA 复位值0x04120000不沿用旧族值。逐项定位以相邻收据为准，这些页码不作为其他族的依据。

三款 L052 均只按标称32768 Hz及板级每周期边界资格化；供电1.65–5.5 V、环境−40…85 °C及各自波形条件必须同时满足。启动/运行 drive 与 amplitude 在使能前独立写入，使能后不重写。AUTOTRIM 校准或自动模式阻止入场；已关闭的 LPTIM/LCD 工作时钟门保持关闭，不能为检查而恢复工作。默认 None 路径不新增检查或源/引脚写入。失败保留使能、预留与诊断状态；普通复位未必清除 LSE 控制，可能需要 POR。未建立实板启动保证、自动 RTC 回退或失钟后的经过时间连续性。

### L083 五个精确封装的自身原件

CW32L083RBT6、CW32L083RCT6、CW32L083RCS6、CW32L083MCT6 与 CW32L083VCT6 使用自己的 `CW32L083_UserManual_CN_V2.0.pdf` Rev 2.0 与 `CW32L083_DataSheet_CN_V1.9.pdf` Rev 1.9；英文 RM Rev 1.0 仅作佐证。原件 ID/SHA、PDF/书页与适用范围见 [L083 原件对应收据](../docs/lse-l083-source-receipt.json)、[专属资格说明](../docs/qualified-l083-lse.md)、[主动 LSE 事实](../docs/lse-active-l083.json) 和 [RTC 入场记录](../docs/lse-active-l083-rtc-admission.json)。SDK V2.2 的五个精确成员及哈希见 [SDK 成员收据](../docs/lse-l083-sdk-member-receipt.json)；SDK 未先准备 LSI 的启用捷径不覆盖自身手册的检测依赖。

DS PDF 10、27–32、35–38、53–55、86（书页各减一）支持封装、引脚、AF 和振荡器条件。PC14/PC15 在64/80引脚封装为3/4，在100引脚封装为8/9；LSI 输出按完整封装表区分 PC4/AF6、PD5/AF6、PF2/AF4。RM PDF 56–58、61–65、75–89、163–172、178、180–181、187、197、204–212、238、247–249、370、396–397、544、562–563（书页各减一）支持单模拟参数组、GPIO LCKR、六个 UART 与 AUTOTRIM/LPTIM/LCD 使用者、RTC 补偿和复位态。LSE 复位值0x2B、ALARMA复位值0x00120000不借用L052双模拟参数组的值；逐项定位以相邻记录为准。

五款 L083 仅按标称32768 Hz、板级每周期边界、供电1.65–5.5 V和环境−40…85 °C资格化。DS表7-18（PDF55/书页54）的原厂LSI范围31816–33784 Hz用于检测器；软件在获取外设或执行RCC写入前要求 `256 × LSE_min_hz > 129 × 33784`，最小整数下界为17024 Hz，声明区间仍须包含标称32768 Hz。额外一沿是保守计数相位裕量，不是新的厂商指标。晶体负载/驱动或旁路电平、45–55%占空比、≥450 ns高低脉宽及≤50 ns边沿仍须独立满足；1.5 s启动值仅为典型值。

此前L083扩展时，原有十一个资格JSON、schema身份、HSI/HSE/PLL后端与电气资格保持不变。默认None不增加LSE检查或源/引脚写入。活跃源不停止或重调，关闭的LPTIM/LCD工作门不为检查而打开；失败保留使能、预留与诊断状态，普通复位未必清除LSE控制。源码审查与ARM编译/链接不构成实板启动、频率精度、自动RTC回退或失钟连续性保证。来源锁仅追加这些项目证据的关联，不更换原件、SDK成员或许可范围。

### L010 三个精确封装的自身原件与原生契约

CW32L010F8P6、CW32L010F8U6、CW32L010Y8M6 使用自身 `CW32L010_UserManual_CN_V1.2.pdf` Rev1.2 与 `CW32L010_DataSheet_CN_V1.3.pdf` Rev1.3。当前原件 ID/SHA、PDF/书页及逐项寄存器/观察者事实见 [L010资格记录](../docs/lse-l010-qualification.json)、[RTC入场记录](../docs/lse-l010-rtc-admission.json) 与 [运行契约](../docs/qualified-l010-lse.md)。RM PDF74/书页73给出四位独立 DRIVER/PDRIVER、WAITCYCLE、PINLOCK与STABLE，没有AMP；DS PDF24–25、27、32、40、42（书页各减一）支持封装、AF、供电及振荡器条件。PB1/PB0 在 TSSOP20 为12/11、QFN20为9/8、SOP16为10/9；没有直接LSE输出路由，PB4/PB6的AF2是独立RTC输出路由，按实际封装裁剪。

三款只按标称32768Hz、每周期板级边界、1.62–5.5V及−40…85°C资格化。StartupOnly 要求继承LSECCS为0且保持0，失钟后STABLE可能仍为1；MonitoredExistingRoutes 要求合法、已稳定且trim/wait不变的LSI，上界36080Hz，满足 `256 × LSE_min_hz > 129 × 36080`。原生CCS可自动请求LSI，因此LSIEN=0不等于没有运行监测；该路径不冷启动或校准LSI。既有保护、IRQ及定时器故障路由刻意保留，不承诺隔离或经过时间连续性。

RTC source0的新源启动另需公开的RTC_OUT/RTC_1Hz观察者移交与整个GPIOB工作窗口契约；配置门尝试有界恢复，恢复失败明确报错；休眠定时器工作门不为检查打开。三款原生LSE/HSI日历例程位于 `examples/l010-lse-clock`。三份共享模型采用已接受的可选原生事实扩展；11份许可头文件、43个硬件原件加2个discovery-only条目与发布边界均不变。


### L011/L012 四个精确封装的自身原件与原生契约

L011 使用当前 CN RM Rev1.1（2026年6月封面）、DS Rev1.1；L012 使用 CN RM Rev1.4、当前 EN RM Rev1.0（2026年6月封面，锁中状态为 corroborating-language-edition）及 DS Rev1.0。完整自身原件 ID/SHA、PDF/书页见 [L011资格记录](../docs/lse-l011-qualification.json)、[L012资格记录](../docs/lse-l012-qualification.json) 与 [范围说明](../docs/qualified-l011-l012-lse.md)。没有新增原件、SDK示例依赖或许可结论。

L011 PC14/PC15 为封装脚2/3，L012为3/4。两族各自支持四位 DRIVER/PDRIVER、StartupOnly 与 CCS监测区分；新监测政策要求已稳定、原厂trim匹配且TRIM/WAITCYCLE不变的LSI，显式自身半字地址0x001007C2。L011 DS PDF51/书页48给出的原厂环境上界是41000Hz，不能被RM±10%偷换；L012 DS PDF58/书页55给出36080Hz。硬件128沿/256个LSI周期与工程额外一沿分开保存；稳定/原厂匹配不是频率或窗口抖动测量。旧L010的九项事实、36080Hz inherited_legal 行为以及此前十六款保持不变。三模型将native_l010显式重命名为native_low_power，不提供别名或掩盖所需事实的默认值。

L011 RM PDF152/书页151及L012 CN PDF202/书页176、EN PDF219/书页193确认 RTC无ACCESS字段。LSE SOURCE0、PSC1=0、PSC2=0x3fff保持实际RTCCLKD≤1MHz；HSIOSC仍96MHz。切源先把第一分频数设置为当前与目标的较大值，验证后切SOURCE，再写入目标分频，避免转换期间提高输入一级频率。保留式attach不通过复位、解锁、停源或重调制造兼容。

GPIOC检查允许整bank采样/滤波/事件推进；门恢复不撤销此前副作用。未开启输出bank、外部RTC/LSE接收者、保留定时器根与不可访问L012 UART3属于未完全运行验证的功能移交条件。L012 UART3中英文门语义以及BTIM/ATIM映射冲突仍明确保留。无额外safe-Rust内存安全义务、自动RTC回退、低功耗恢复或失钟后连续性保证。
