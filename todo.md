# 尚未完成的工作

当前实现基线：2026-10-15，提交 [`029e90b`](https://github.com/fan2nd/cw32-hal2/commit/029e90be138a5f536b363b5bf3c90deddd379646)；DMA 资格边界延续 Stage94。本文记录当前缺项、证据阻碍及后续验收条件，不是新增功能承诺，不预设阶段数或完成日期。各型号仅按自身硬件、封装及已批准模式开放；没有资格不等于硬件不支持。

## 本轮执行状态（2026-10-15）

- [x] 将已存在的 W031 退役及 L010/L011 RCC 拆分改动保存为提交 `1a2e185`；本轮未重跑历史验收。
- [x] 移除 CI 构建矩阵、检查包装器和运行器；能力清单、来源审查与打包工具迁至 `tools/`，历史记录保留。
- [ ] UART 缓冲存储消耗式回收：已实现 `BufferedUartRx::into_buffer`，经现有 Drop 解除 ISR 指针后返回原始存储；组合接口先 `split` 再回收 RX。当前仅静态审阅，未编译／测试／硬件验收，不计入已验收功能。见 [接口与限制](docs/uart-buffer-reclaim.md)。
- 本轮遵守禁止本地测试的要求，也不运行构建、生成或验证脚本。后续软件验收待用户解除限制后执行；既有硬件协议证据阻碍保持未完成。
- 当前支持集合已排除 CW32W031/CW32W031R8U6；下文 Stage 历史结果中出现 W031 不代表恢复支持。

## 已完成的边界，不重复列为待办

- 已有按型号限定的 SRAM DMA、UART TX／有限 RX／组合收发和 SPI 全双工字节主机 DMA。Stage92 已补 F020、L031、L052、R031、W031 的 14 个精确型号 UART 有限 RX；L012 UART 仍仅 TX。
- Stage93 已补上述五族的 SPI DMA；Stage94 已补 **CW32L012C8T6/C8U6 的 SPI1/2/3 DMA**，保留普通 PIO 接口和完整 SPI23 共享处理。两阶段均已完成独立软件验收及发布。
- 普通 UART 缓冲接收、已限定的异步 SPI/I²C 主机、定时器溢出事件、捕获／编码器及 ADC 异步单次／有限扫描已存在。后续工作不能把这些已实现模式重新当作缺项。
- 构造器已统一为返回 `Result` 的 `new*`，移除重复公共 `try_new*`；SPI 双 DMA 通道已在同一临界区检查后同时准入，不是整个构造过程的事务回滚。
- IRQ 缓冲 UART 已支持批量 `read`、`read_exact`、`blocking_read`、`read_ready` 和 `fill_buf`／`consume` 零复制借用。普通非 static 接收缓冲通过显式 unsafe `new_borrowed*` 借用；RX 正常 Drop 停止收集并解除 ISR 指针后，借用结束即可重用，无需另设返还接口。组合 RX/TX 共用生命周期，仍在使用的 TX 半部可能延长借用。遗忘 RX 或其所属 future 不保证 Drop，调用者必须按 unsafe 合同持续保留存储；安全 static 构造器及 DMA 暂存的资源回收是不同问题。详见 [借用缓冲验收](docs/uart-borrowed-buffer-validation.md)。
- 软件编译、链接、来源核对和干净目录复验均不等于芯片实测。历史审阅中的“主验收时 clean 未运行”是当时状态，不能据此把已完成阶段标回未完成。

## 第一批已通过软件验收并发布

以下已通过[本轮软件验收](docs/unblocked-hal-extensions-validation.md)：113 项 ARM 库构建、91 次示例构建及聚焦软件检查。已随 22efddf 合入 main；这些结果不代表硬件验收：

- SPI 既有十族 IRQ PIO 支持 4–16 位 Word；DMA 仍仅 u8，非 u8 请求在触碰硬件前拒绝，不隐式切换后端。见 [接口范围](docs/spi-async-word-widths.md)。
- 定时器已有 InputInstance 与 OverflowInstance 交集增加合并式捕获 IRQ 等待；不扩原生 classic ATIM 六半通道，不承诺无损事件历史。见 [精确范围](docs/timer-capture-irq.md)。
- L052/L083 LCD 增加异步帧等待与呈现；不扩时钟／电气模式，不承诺原子帧更新或失钟超时。见 [当前实现](docs/lcd.md#asynchronous-frame-waits)。

- L052/L083 AUTOTRIM 既有 MD3/AUTO0/HSIOSC 周期计数新增合并式 UD 等待；不扩校准／FCAP／外部源，不承诺失钟唤醒或周期计数。见 [范围](docs/autotrim-async.md)。

## 第二批已通过软件验收并发布

- L012 普通 UART 9 位无校验 PIO；缓冲／DMA 仍仅字节，其他族未开放。见 [接口范围](docs/uart-l012-nine-bit.md)。
- L012 I²C1/I²C2 显式 10 位主机地址，包括 blocking/async；classic 后端、从机、自动重试／总线修复未扩展。见 [接口范围](docs/l012-i2c-ten-bit.md)。
- L012 HALLTIM 合并式异步捕获等待，不是无损／原子历史。见 [接口范围](docs/halltim-async.md)。
- L012 CORDIC 既有八种 Q1.31 运算的异步完成，不扩数值域／Q1.15／全象限／精度；取消不 abort。见 [接口范围](docs/cordic-async.md)。

- L012 LVD／比较器增加上升／下降／双边沿的新窗口合并式等待，仅既有输入配置，不扩滤波、参考、保护或模拟就绪保证。见 [接口范围](docs/l012-analog-edge-waits.md)。

以上已通过[独立第二批软件验收](docs/l012-extensions-validation.md)：113 项 ARM 库构建、68 次示例构建、26 项接口预期、12 项 IRQ 所有权预期及聚焦宿主测试。不复用第一批验收；已随 67f280b 合入 main，无硬件实测结论。

## 第三批已通过软件验收并发布

- L012 单线半双工 UART：单个 TXD 开漏复用引脚及外部上拉，blocking/IRQ PIO，无拆分／DMA／环缓冲；接收前 flush 不等于仲裁或碰撞保护。见 [范围](docs/uart-l012-half-duplex.md)。
- L012 DAC 不重叠借用通道视图与显式软件触发模式：SWTRIG 自清仅证明数字加载，不是模拟稳定、自主 DMA 波形或同时输出。见 [范围](docs/dac-channel-software-trigger.md)。

本批通过[独立软件验收](docs/uart-dac-extensions-validation.md)：113 项库构建、54 次示例构建、33 项接口预期及聚焦宿主检查；不使用前两批记录替代；已随 c1ae33f 发布，无硬件验证。RTC、ADC 和 I²C 从机等候选仍是后续资格／实现工作，并非因本批不做而统称已阻碍。

## 第四批已通过软件验收并发布

- L052/L083 LCD 保持既有 LSE 租约、闪烁与 1–64 帧间隔事件；帧命名接口只接受间隔 1，async 帧接口改为返回 Result。事件不是无条件逐帧通知或原子显示完成。见 [范围](docs/lcd-clock-blink.md)。
- L012 WWDT 预警等待：IE 不可逆使能前建立静态服务，Drop 后仍清自己的 POV；不自动喂狗、不保证复位前任务收到通知。见 [范围](docs/l012-wwdt-warning.md)。

本批 113 项库构建、18 次示例构建、37 项 API 检查和 23 项主机测试／场景通过，见 [验收记录](docs/lcd-wwdt-extensions-validation.md)。已随 [`e75f62e`](https://github.com/fan2nd/cw32-hal2/commit/e75f62e7f792dede2b2d3c89bbaba5f9aee25531) 合入 main；未做硬件验证，不扩其他外设或低功耗／RF 范围。

## 优先证据阻碍

以下“受阻”只针对列明的功能、范围或保证，表示现有来源存在冲突，或不足以证明所需协议。解除条件必须落实为该型号自己的证据和独立验收，不能用相似寄存器、更多编译次数或其他族的结果代替。已实现的限定模式继续保留；普通未实现、用户后置及板级待验收分别列在后文。

### 1. L012 UART 多字节 DMA RX／组合收发

- [ ] 闭合 RC、ORE 与 DMA 读 RDR 的关系。自身 CN 手册 v1.4 pp457、487及 EN v1.0 pp500、534指出：下一字符到达时 RC 仍置位会触发 ORE；现有 SDK 的 RDR 读取不能证明 DMA 自动清 RC。
- 需要厂商说明 DMA 是否／何时确认并清除 RC，以及连续字符、延迟服务、ORE/NE、取消和重启时的顺序；或取得限定条件下可复核的板级证据。不能直接套用无 ORE/NE 的其他族。
- 在此之前不开放多字节 RX，不忽略 ORE，不用单字节接口替代原有限 RX 目标。**L012 SPI DMA 已完成，不受此 UART 阻碍影响。**
- 当前边界与来源入口：[L012 DMA](docs/l012-dma-owned.md)、[UART 逐帧状态](docs/uart-l012-nine-bit.md)。

### 2. ADC DMA 请求与结果序列

- [ ] 先闭合 L012 ADC1 单次 1–8 槽扫描的逐转换请求：DMAEOC 是脉冲还是依赖粘滞 EOC 的电平？谁确认／撤销请求？服务延迟时请求会排队还是合并？第 n 次搬运如何保证对应已完成的第 n 个 RESULT？
- 依据为 L012 CN v1.4 §25.5.2 pp581–582、§25.12.8–10 pp596–597、RESULT 定义 pp598–599及 DMA §8.4.4 p113。选择器和寄存器几何已有依据，握手与顺序尚未闭合；等待最终 EOS 不能修复提前读到的数据。
- 需要自身权威握手说明／有限 DMA 范例或限定板级验证，再审新增字宽、地址步进、取消和资源保留。不得用一次采样或换成 EOS/BULK 悄然替代逐槽目标。
- **现有 ADC IRQ＋CPU 复制已支持有限异步扫描**；没有证明新 DMA 收益与完整协议前，不为同一用例另造功能阶段。其他族 ADC DMA 也须各自核实，不外推 L012 结论。
- 当前扫描／结果寄存器依据：[L012 有限扫描](docs/adc-l012-scans.md)；共享 IRQ 边界：[ADC2 与 DAC 共存](docs/adc-l012-adc2-async.md)。

### 3. L012 DAC 自主有限波形 DMA

- [ ] 明确首样本预装、初始／继承补数请求、最后一次 DHR→DOR 加载，以及停用后的挂起请求处理和安全重用。
- CN v1.4 §26.6–8 pp605–607明确 DOR 加载后请求下一次 DHR 补数；普通 DMA TC 可能仅表示末值已到 DHR，不能当作最后输出完成，更不能当作模拟稳定。
- §26.10.2 p612（EN p669）的 SWTRIG 自清确能证明**软件触发的数字加载完成**；逐样本 CPU 触发不能冒充自主有限 DMA 波形。相同 DOR 数值也不能区分重复样本的最后一次加载。
- ATIM 的 REP+1／OPM／比较脉冲 TRGO 有有限触发依据（CN pp323–325、373、397）；尾部追加一次重复补数也是可能方案。两者仍需证明请求初态及末次触发对应关系。DAC 本地三拍延迟不能直接当作从定时器停止到 DAC 接受的端到端上界。
- SDK 的 `DAC_SingleCh_DMA` 是软件 BULK＋自动重启；`DAC_WaveSine_HwTrig` 确为硬件 BLOCK，但也是连续触发＋自动重启，未提供有限首尾／停用协议。需要自身有限范例、厂商明确说明或限定板级证据。
- 若始终保持 DMAUDRIE1/2 为零，可保留现有 ADC2_DAC 安静对端约束；若增加 DAC 欠载中断，必须重新审查共享分发和 ADC2 掩码失败时的整向量隔离，不能直接开启 IRQ。
- 已资格的软件触发与来源入口：[DAC 通道视图／软件触发](docs/dac-channel-software-trigger.md)、[DAC/OPA 来源审查](docs/dac-opa-source-review.md)。

## 其他已确认的协议／来源阻碍

### 4. RTC Alarm B 匹配编程

- [ ] 十一类既有 RTC 的 ALARMB 位 23/15/7 表格写“0 忽略、1 比较”，但共同 A/B 示例和 SDK 对两组使用相同掩码，与该表极性相反。缺的是消除冲突的依据，不是寄存器是否存在。
- 解除条件：取得各适用手册的厂商勘误／明确说明，或能区分两种极性的受控芯片证据，再实现 B 匹配编程。现有 Alarm A、A/B 状态与选择性清标志，以及已资格源的 B 事件等待均不受此限制。见 [Alarm B 冲突与逐族页码](docs/rtc-alarms.md#alarm-b-conflict)。

### 5. RTC 异步清理与运行中写入时限

- [ ] classic WINDOW/ACCESS RTC 尚无已证明的有界 IRQ／取消清理协议；SDK 跳过窗口的写法不能推翻手册。解除条件是证明进入、屏蔽／撤销中断、取消和错误退出均遵守窗口及 ACCESS 时限，再开放 classic async。现有受限同步事务保留。见 [RTC 访问与取消边界](docs/rtc-alarms.md#access-irq-ownership-and-cancellation)。
- [ ] L010 native live-LSI 下运行中 `set_datetime` 的一秒 ACCESS 时限未资格化，有限轮询和快速 HCLK 不能证明 WCET；现有 HSIOSC/LSE 事务在过慢或未资格 CPU 时钟下也不承诺该时限。解除条件是明确可接受工作点、总线／中断／停钟假设并证明和测量最坏事务时长，不能偷偷停表代替。七个精确 native live-LSI 源的 async alarm 仍被显式拒绝；开放前须独立完成该源的等待／取消资格，不能凭通用外设 async 元数据取消拒绝。见 [native LSI 写入与 alarm 限制](docs/native-live-factory-lsi-rtc.md#writes-alarms-and-compatibility)、[ACCESS 验收边界](docs/rtc-remaining-calendar.md)。

### 6. L012 I²C 争议时钟与 FIFO 字段

- [ ] MCR0.CLKSRC=3 在手册／时钟图中是 HSI，在 SDK 中是 LSI；RXWATER/RXCNT 手册列 12 位，SDK/SVD/PAC 列 2 位，功能摘要又称一字节 RX FIFO。不得据此扩时钟选择、非零水位或深 FIFO 接口。
- 解除条件：取得对应芯片的时钟选择、字段宽度和 FIFO 深度／计数语义勘误，或针对这些差异的可复核验证。现有冻结 PCLK、watermark=0、RXNE/MRDR.EMPTY 路径和已实现 10 位地址不受阻。见 [现存来源冲突](docs/known-source-conflicts.json)、[I²C 字段边界](docs/i2c-typed-registers.md#deliberately-unchanged-l012-discrepancies)。

### 7. IR 编码冲突与安全引脚政策

- [ ] **来源冲突：** L012 CN/EN 手册的 16 个 MOD 编码与 SDK 的 GTIM1–4/UART3 常量不一致，任一方都不足以单独证明硅片实际选择。须取得厂商澄清或逐编码路由证据，才开放 `Mode`／`set_mode`／`mode`；现有 attach、输出引脚、IRSW 和 INV 保留。见 [L012 IR 编码边界](docs/lvd-ir.md#ir-scope-register-differences-and-ownership)。
- [ ] **政策／路由限制，不是硬件故障：** R031 唯一已载明 IR 输出是 PA13 AF6，当前安全 GPIO 政策不交出调试 PA13；SDK-only PB9 不能补作已验证路径。需取得其他封装有效路由证据，或另行设计并审查显式调试引脚交接及恢复合同，才能增加安全输出；控制器配置仍可用。见 [IR 引脚与政策](docs/lvd-ir.md#ir-scope-register-differences-and-ownership)。

### 8. Flash 掉电合同、提前中止与完成上界

- [ ] `NorFlash::write` 要求掉电时同页其他数据不变，已审 CW32 手册／电气表没有证明此性质。依赖该 trait 的通用掉电一致性存储集成受阻；现有 `ReadNorFlash` 和显式 blocking 写／擦不是相同承诺。
- 解除条件：厂商权威的掉电／复位影响保证，或独立论证并验收能够满足 trait 的实现。安全 abort、复位后恢复和保证完成时间也需各自协议／时序证据，不能把状态轮询次数当作上界或把有效电压构造条件当作掉电免责。见 [NorFlash 验收阻碍](docs/flash-storage.md#deliberate-norflash-acceptance-barrier)及该文部署验收边界。

### 9. L052/L083 AUTOTRIM 校准、FCAP 与中止

- [ ] FCAP 缺测量步骤、符号／方向、有效握手与一致采样时刻；AUTO=0 只关闭自动调谐，不能自动形成频率计。自动搜索期间何时覆盖振荡器 TRIM、EN=0/AUTO=0/reset 后状态、终止上界、物理停止和重启协议也未说明。
- [ ] 校准流程／SDK 活体调谐示例与 RCC“启动后不可改参数”限制未闭合；完整 9 位搜索的安全频率包络、FLIM 公式、收敛／漂移界及新 trim 的 ClockBounds 均无足够依据。慢总线和最大 Flash WAIT 不能证明振荡器本身安全。
- 解除条件：厂商明确完整搜索／测量及中止恢复协议，覆盖活跃消费者、全搜索包络和参考误差，或取得限定条件下独立可复核的相应证据；再审所有权和数值合同。现有 MD3/AUTO0/HSIOSC 计数与 UD async 不受影响。外部 ETR 另须封装、脉冲／速率和计数所有权资格。见 [AUTOTRIM 未闭合项](docs/autotrim-counter-evidence.md#calibration-and-frequency-measurement-gaps)。

### 10. Native LSI 冷启动／重调与严格时序资格

- [ ] L010/L011/L012 的 live-only LSI 资格不能推广到冷启动或自动硬件请求：IWDT RUN/update 不能证明请求／释放历史；L012 RCC 的 IWDT 自动请求 LSI 条款与 IWDT 的 RC10K 计数源需同时尊重，不能用后者推断无 LSI 请求。解除条件是本族所有请求者的可观察准入、停止／重调顺序和失败处置证明；不得为探测而擅自停／喂／复位看门狗。见 [L010](docs/l010-live-factory-lsi-sysclk.md#preserved-consumers-and-functional-handover)、[L011](docs/l011-live-factory-lsi-sysclk.md)、[L012](docs/l012-live-factory-lsi-sysclk.md)。已完成的其他族冷启动资格不因此撤销。
- [ ] L011 DS 工厂误差 −10/+25% 与 RM 调整范围／±10% 警告及 trim 步长仍冲突。保持完整 29,520–41,000 Hz 工厂速率包络，不裁成更窄值，也不由此宣布任意 trim、监测合法性或严格逐周期界。解除条件是厂商区分工厂保证、可调范围及监测条件，另有逐周期／抖动证据才可扩严格周期消费者。见 [native LSI 源界](docs/native-live-factory-lsi-rtc.md#own-source-bounds-and-native-division)。
- [ ] 现有 PLL／factory-LSI 的平均速率界不提供绝对逐周期界；依赖采样／死区持续时间的 ADC、互补 PWM 等不能仅凭该平均值解除 `UnqualifiedCycleTiming`。须补源的严格周期界，或重新限定并独立证明消费者合同。见 [PLL 时序边界](docs/f020-x030-hsi-pll.md)、[L010 LSI 消费者限制](docs/l010-live-factory-lsi-sysclk.md)。这不等于所有该源驱动的外设都受阻。

### 11. 其他现存冲突：只阻碍扩出当前保守边界

以下每项均需本族权威说明／勘误或专门验证来消除列明差异，再审新增资格；不是删除已实现模式，也不是所有更广功能的共同前置条件。

- [ ] **HSE bypass 低于 4 MHz：** F020/F030/A030、L011、L052、L083 的 DS 允许从 1 MHz 起，RM 从 4 MHz 起；保留现有 4–32 MHz 交集，扩低频需解决两者适用条件。L010 自身 RM/DS 均允许 1–32 MHz，其 1–<4 MHz 只是未做的软件覆盖，不能归入此冲突。见 [F020/x030](docs/qualified-hse.md#own-source-admission)、[L010/L011 区分](docs/qualified-l010-l011-hse.md)、[L052](docs/qualified-l052-hse.md#board-and-electrical-contract)、[L083](docs/qualified-l083-hse.md#board-and-frequency-contract)。
- [ ] **L011/L012 LSE bypass 上限：** DS 100 kHz 与 RM 1 MHz 不同，当前按 DS 限制；扩至更高输入频率须澄清电气与功能条件，不能由字段存在推断。见 [native LSE 资格](docs/qualified-l011-l012-lse.md#source-facts-used-by-the-validator)。
- [ ] **SPI 更高速／更小分频：** F002/F003/F020 的 12 Mbit/s 摘要与 16 MHz 电气表冲突；L031/R031/W031/L052/L083 另有功能章节 PCLK/4 与 BR 表 PCLK/2 冲突，现保留 12 MHz 和各自最小分频。扩这些范围须闭合对应条件；F030/A030 既有 16 MHz 策略不因本清单改成 12 MHz。见 [逐族 SPI 策略](docs/spi-clock-bounds.md#preserved-own-family-policies)、[五族分频冲突](docs/l031-shared-serial.md#source-contradictions-and-conservative-choices)。
- [ ] **L012 ADC 更高频率／更短保证：** DS 96 MHz／405 周期与 RM 48 MHz／最长 518+15 周期冲突，RM 最高速率另限 1 MSPS；保留既有 48 MHz、1 MSPS 与明确采样编码。扩范围须消除时钟、吞吐和总转换时间的差异，不能仅按最短分频推算。见 [ADC 电气冲突](docs/adc-l012-dual-ownership.md)、[现有扫描时序](docs/adc-l012-scans.md)。
- [ ] **L012 Flash WAIT4／更高总线：** 时钟章节 WAIT4 文字与 Flash 表／寄存器和 96 MHz 总线额定上限不符；保留 WAIT0..3，扩范围需各项电气／寄存器依据一致，100 MHz 振荡器调谐目标不是合法 HCLK 证明。见 [RCC 工作包络](docs/rcc-operating-envelope.md#flash-wait-and-transitions)。
- [ ] **L012 UART3 门控与定时器路由：** CN 称 UART3 门控仅配置，EN 称配置及工作；BTIM RTC 选择器 3 与 SDK 6、ATIM TI2/TI5=6 与通用表／SDK 8 不一致。解除前不以关闭的门控、静默状态或其他族编号证明没有消费者；须取得准确门控／路由语义，再扩自动消费者检查／交接。既有受限 LSE/LSI 功能保留。见 [native LSE 剩余边界](docs/qualified-l011-l012-lse.md#remaining-runtime-and-electrical-boundaries)。
- [ ] **L011 未资格触发输入：** BTIM code5 的 UARTx 身份、code0 的 SDK AWT 常量和 GTIM TI2–4 引脚标注尚未形成一致依据；须逐目的端核源身份、编码和封装。ADC bits19–24 的 SDK 缺宏仅是待审，不单独证明手册错误。既有 BTIM1 UPDATE→BTIM2 code8／ADC bit13 不受阻。见 [L011 触发排除项](docs/trigger-routing-l011.md#own-family-evidence)。
- [ ] **L052 ATIM master/slave：** PAC 暴露 MSCR.MSM，但自身手册保留 bit4；当前写零且不开放该功能。新增前须解决该位语义，不能从共享 PAC 推出支持。见 [ATIM 来源边界](docs/atim-counter-pwm.md)。
- [ ] **L052 CRC 任意初值：** INIT+0x10 存于 header/SVD/PAC，但手册不列语义、SDK 不使用；须补初值寄存器写入／复位／生效协议再开放任意 seed。F020 SDK 的 CRC32 常量已被自身手册限制排除，不是承诺要补的 CRC32 功能。见 [CRC 证据限制](docs/crc-evidence.md#register-protocol-and-source-boundaries)。
- [ ] **L012 CORDIC 扩展数值／时序合同：** ITER 文字与明确表不一致，全象限 phase 的 atan2/atan(y/x) 描述及 Q1.31 ±1 端点舍入／饱和均不完整。须分别取得迭代编码、分支／零向量、端点和误差界依据，才扩这些保证；现有固定 ITER15、限定 Q1.31 域及八个 async 接口继续有效，Q1.15 只是待审模式。见 [数学源边界](docs/l012-math-evidence.md#source-conflicts-and-explicit-limits)、[当前 CORDIC async](docs/cordic-async.md)。

## 其余 DMA 待办：先资格审查，再决定实现

以下均是未实现／未资格化的集成，不是已证明的硬件故障；并非每个型号都有对应能力。

- [ ] I²C、ADC、DAC、BTIM/GTIM/ATIM、HALLTIM 等安全外设 DMA；逐实例核请求、端点宽度、完成／错误、共享 IRQ 和所有权。元数据中的 `dma_channels` 只表示路由。
- [ ] 定时器捕获／比较／预装载 DMA、LCD/EAU/CORDIC 等其他 DMA 用法，先核实际硬件存在和有价值的使用场景，不能由寄存器名推定支持。
- [ ] DMA 循环／连续、scatter、DMA 零复制或借用缓冲、UART idle DMA／流控、SPI 更宽 DMA 数据、从机 DMA及通用外部硬件请求接口。当前只在已声明的族／模式提供有限实现；x030 的 unsafe 借用硬件构造器不是通用安全集成。
- [ ] **协议证据受阻：** 安全提前中止、取消后立即重用和有限完成时限。没有排空依据前，保留现有资源隔离，不能仅凭 EN=0／剩余计数为零归还资源。须分别证明请求关闭、在途总线访问退休、缓存／内存可见性和错误后终态，才可解除隔离。见 [SPI DMA 取消／Drop](docs/spi-dma.md)、[四通道 DMA 所有权](docs/four-channel-dma-owned.md)。

## 非 DMA 的其余功能缺项

按当前能力清单归组；这是待审清单，不承诺所有硬件模式都要实现。

| 模块 | 尚未实现或仍待资格化 |
| --- | --- |
| UART | L012 普通 PIO 之外的九位、非 L012 半双工、LIN、自动波特率、RS485 方向、同步等高级模式；缓冲 idle 标记、运行中替换环缓冲等扩展；消耗式存储回收接口已实现、待验收（见本轮状态）；普通借用缓冲 Drop 后重用已实现 |
| SPI / I²C | SPI 从机及其他高级模式；非 L012 I²C 十位地址、各后端从机、重试、总线修复等接口 |
| ADC | 连续／周期／通用硬件触发扫描、完整外部参考、L012 同步／耦合双 ADC、其他内部触发路由；不覆盖已存在的有限软件扫描和限定 BTIM1 单次触发 |
| 定时器／触发互联 | 本轮限定交集之外的捕获 IRQ、其他计数／触发事件及跨外设级联、扇出；不覆盖已完成溢出事件、已资格捕获／编码器和明确列出的连接。其他互补输出／死区／刹车重武装、编码器索引／有符号位置和无损事件历史需另审，已有资格模式保留 |
| RTC | Alarm B 编程的手册／SDK 掩码极性冲突；补偿、其他未列时钟及更完整时间能力。classic WINDOW/ACCESS 异步等待未开放；native live-LSI 异步 alarm 被明确拒绝，L010 live-LSI 运行中改日期因写窗口未资格化而拒绝 |
| RCC / 时钟 | 运行中切换、完整校准、失钟恢复、更多受控输出资格。native 冷启动 LSI／IWDT 自动请求顺序仍须自身证据；L011 RM 调整范围警告与 DS 全温界限必须继续区分，不能把现有 live-only 支持扩大成冷启动保证 |
| DAC / OPAMP / 比较器 | DAC 硬件触发／波形／独立所有权通道拆分和校准（本轮借用视图／软件触发见第三批）；OPAMP 校准／内部共享；比较器窗口、滤波、消隐、L012 新窗口边沿等待之外的 IRQ／异步及其他参考／输出路由 |
| LCD / LPTIM / AWT | LCD 外部偏置／泵；限定 held-LSE／闪烁及区间事件见第四批，非硬件验证；LPTIM 低速源／外部触发输出；AWT 源丢失不会自动唤醒等待者或恢复计时 |
| IR / LVD / HALLTIM | L012 IR MOD 编码的手册／SDK 冲突；R031 唯一已载明 PA13 输出因调试引脚政策未开放。LVD 滤波、L012 新窗口边沿等待之外的 IRQ／异步及复位策略，HALLTIM 更完整事件历史尚未提供，限定合并式异步等待见第二批 |
| Flash / 看门狗 | Flash async、选项／安全配置及更强存储合同；非 L012 WWDT IRQ；L012 预警见第四批。不可逆看门狗启动不等于待补“停止”接口，Flash 掉电原子性也不能无硬件依据承诺 |
| AES / TRNG / EAU / CORDIC / AUTOTRIM | AES 字节互操作及其他模式（L083 无 AES/TRNG IRQ 或 DMA，不列为待补中断功能）；TRNG 熵强度／健康检验；现有 L012 EAU 无已载明 IRQ，不列为待补中断功能；CORDIC Q1.15／全象限／迭代等扩展；AUTOTRIM 校准／FCAP／外部参考及本轮 UD 之外的 IRQ。数字接口可先按自身资料审查和软件验收；数值正确性、字节互操作、熵及模拟／时序指标仍需相应独立证据或实测，不能一概视为已阻碍或已验证 |

L012 EAU 的排除仅针对当前已资格硬件：自身 CN v1.4 §5 pp70–71 完整中断表无 EAU，CSR 仅 MODE/BUSY/ZERO/OVR，无中断使能字段。不能据此构造 IRQ 异步接口，也不外推其他未来硬件。

## 当前实验性边界与硬件验收

- **验收缺口，不一概视为软件实现受阻：** L083 AES 尚无独立已知密文证据证明寄存器字序到标准字节数组的对应；官方先加密再解密的回环不足以建立互操作。扩字节 API／正确性保证前须独立标准已知答案与字节顺序验收。TRNG 未载明模拟稳定时间、健康／错误状态或最低熵，不能把完成位、SHIFT 或一次统计检验当作密码学保证；安全用途须独立熵评估、调理和失效策略。现有原始字／样本接口保留；没有 IRQ/DMA 的 L083 AES/TRNG 不列作待补中断。见 [AES/TRNG 边界](docs/l083-aes-trng-evidence.md)。
- **中止承诺的证据缺口：** AES/TRNG START 表未定义写零 abort；现有超时／busy Drop 不代表取消完成。CORDIC 的取消也只退役中断等待，不停计算。扩立即重用、密钥彻底清除或有界停止承诺前须分别证明硬件终态／内部状态清除，不能由门控或普通寄存器清零推断。见 [AES/TRNG 生命周期](docs/l083-aes-trng-evidence.md#ownership-and-bounded-failure-behavior)、[CORDIC 取消](docs/cordic-async.md)。
- [ ] **所有芯片硬件验证仍待完成。** 需按实际板卡核引脚、电压／时钟、IRQ 共享、传输完整性、错误与取消／复位行为；ADC/DAC 模拟精度和稳定时间、TRNG 熵、加密已知答案及数值运算结果另行验证。编译成功不替代这些结果。
- UART/SPI DMA 仍为 `experimental_partial`，均使用私有暂存与 CPU 复制。泛型别名缺少精确内存资格时保留 `MissingMemoryMetadata` 拒绝，不用猜测容量补齐。UART RX 的分块／重装间隙可能丢帧，部分族无丢失计数；UART 错误后恢复可能等待旧计数，TX DMA 完成与线端 flush 分开。
- 静态外设／通道／暂存区可能跨取消、forget 或未解决 Drop 被永久保留／隔离；SPI 现已在同一临界区检查双通道后再同时准入，通道拒绝不改变任一准入状态；构造器入参仍被消耗，后续 SPI 配置失败不回滚已成功的双通道准入。干净 Drop 也不承诺恢复启动期准入；不能表述为无损、零复制、任意取消安全或有界活性。
- SPI 片选由调用方负责；必须按现有合同保持原设备选中并完成 flush／既定恢复，不能将内存安全退出说成总线事务完成。
- [ ] 板级与厂商证据到位后，先形成精确资格和失败处置合同，再实现及验收；不以新增测试框架或更多编译次数弥补缺失的硬件语义。

## 用户明确后置，不计当前完成条件

- 低功耗、睡眠／深睡、唤醒／恢复，以及相应 RTC 保持和 Embassy 时间恢复。
- RF：BLE Lite、ChirpIOT 无线子系统。

后置范围内仍须保留的已知来源问题：L012 OPA 的 DS 提及 LPMODE，但所选 CN/EN 寄存器表、CMSIS 和 SVD 未定义该位，不得猜 reserved bit3。仅在以后恢复该范围时，先取得模式／位定义及电气条件；这不是当前完成条件。见 [DAC/OPA 明确冲突](docs/dac-opa-source-review.md#explicit-conflicts-and-deferred-claims)。

已有运行态 RCC/AWT/LPTIM 等接口不因后置事项而删除；也不宣布整个项目所有外设／所有模式已完成。

## 依据与维护

以 [当前能力清单](tools/hal-capabilities.yaml) 和 [能力清单解释](docs/functional-coverage.md) 为当前实现范围；原始资料身份见 [来源索引](sources/evidence-sources.json)。具体代码／来源证据路径由能力清单各模式引用。上文新 DMA 阻碍来自对已锁 L012 CN v1.4、EN v1.0、SDK v1.0.5 的有限审查；未修改原件身份或历史审阅。

以后仅在相应实现、独立软件验收及发布完成后更新待办状态；硬件项需另有真实板级结果。本文不把“未审”“硬件不存在”“资料冲突”“用户后置”混为一类。
