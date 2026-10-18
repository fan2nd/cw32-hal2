# 待办

- [ ] 时钟：确认 native LSI 启动／重调／IWDT 顺序、PLL／factory-LSI 周期界及 HSE／LSE 边界；证据不足的配置保持禁用。
- [ ] Flash：解决 L012 WAIT4／频率冲突，确认掉电、复位、abort、完成上界及 NorFlash 写入恢复约束。
- [ ] UART：解决 L012 UART3 门控冲突；未确认前保持实例限制。
- [ ] I²C／SPI：解决 L012 I²C 时钟／FIFO 与各族 SPI 速率／分频冲突，确保现有主机模式参数可靠。
- [ ] ADC：确认 L012 时钟／吞吐边界及连续采集结果一致性；无可靠覆盖检测的模式保持禁用。
- [ ] 定时器：核实 L012 BTIM／ATIM、L011 BTIM／GTIM／ADC 触发及 L052 ATIM MSM 冲突，避免开放未确认路由。
- [ ] RTC：确认 Alarm B 掩码、classic WINDOW／ACCESS 清理及 L010 live-LSI 日期写入约束；未确认模式保持禁用。
- [ ] 数学／加密：核实已开放 CORDIC 的数值边界、AES 字节顺序及 AES／TRNG／CORDIC 安全重用；TRNG 未经评估不得宣称密码学安全。
- [ ] 禁令解除后重新生成并验收 L010 bypass 3,908,159–4 MHz 数据／PAC，核查三个封装、3.95 MHz 示例及 retained-HSE。
- [ ] 硬件验收：验证已支持模式的引脚、电气／时钟、传输与 LIN 时序、错误／取消／复位、模拟精度及数学／AES 已知答案。
