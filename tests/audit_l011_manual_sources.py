#!/usr/bin/env python3
"""Own-L011-manual corroboration of already implemented peripheral contracts.

No device execution and no source/metadata mutation. GPIO IRQ/PAC access policy
and CRC are independently reviewed in their own evidence and tests.
"""
import hashlib
import json
import os
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
SOURCES = Path(os.environ.get("CW32_SOURCES", "/workspace/shared/cw32-sources"))
E = json.loads((ROOT / "docs/l011-manual-follow-up-evidence.json").read_text())
for role in ["manual", "manual_text"]:
    source = E[role]
    assert hashlib.sha256((SOURCES / source["file"]).read_bytes()).hexdigest() == source["sha256"]
TEXT = (SOURCES / E["manual_text"]["file"]).read_text()


def section(number):
    hits = list(re.finditer(r"^" + re.escape(number) + r"\s+", TEXT, re.M))
    assert hits, number
    tail = TEXT[hits[-1].end():]
    end = re.search(r"^\d+\.\d+(?:\.\d+)*\s", tail, re.M)
    return tail[:end.start()] if end else tail


def compact(number):
    return re.sub(r"\s+", "", section(number))


def require(number, *needles):
    text = compact(number)
    for needle in needles:
        assert needle in text, (number, needle)


def field(number, bits, name, access):
    assert re.search(r"\b" + bits + r"\s+" + name + r"\s+" + access + r"\b", section(number)), (number, bits, name, access)


require("4.3.4", "频率固定为96MHz", "启动后禁止修改相关参数", "上电后默认值为24")
require("4.7.4", "96.00MHz校准值地址：0x001007C0–0x001007C1")
for encoding, divisor in enumerate([32, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 12, 16, 20, 24, 28]):
    require("4.7.4", f"{encoding:04b}：HSI=HSIOSC/{divisor}")
field("4.7.4", "10:0", "TRIM", "RW")
require("4.7.2", "能够硬件自动开启LSI时钟", "LSIEN位域不会被置位", "IWDT_KR=0xCCCC")
for number in ["4.7.1", "4.7.2", "4.7.11", "4.7.12", "4.7.13"]:
    field(number, "31:16", "KEY", "WO")
    require(number, "0x5A5A")
for number in ["4.7.14", "4.7.15", "4.7.16"]:
    assert not re.search(r"\bKEY\s+WO\b", section(number))
    require(number, "0：模块处于复位状态", "1：模块正常工作")
for bits, name in [("4", "GPIOA"), ("5", "GPIOB"), ("6", "GPIOC"), ("1", "FLASH")]:
    field("4.7.11", bits, name, "RW")
for bits, name in [("2", "SPI"), ("3", "UART1"), ("4", "UART2"), ("8", "UART3")]:
    field("4.7.12", bits, name, "RW")
    field("4.7.15", bits, name, "RW")
for bits, name in [("4", "IWDT"), ("6", "I2C")]:
    field("4.7.13", bits, name, "RW")
    field("4.7.16", bits, name, "RW")
for encoding, mhz in [("000", 24), ("001", 48), ("010", 72), ("011", 96)]:
    require("7.4", encoding, f"HCLK≤{mhz}MHz")
print("PASS L011 RCC/FLASH: own clock dividers, calibration, LSI requests, keys/routes/reset and waits")

require("8.3.2", "GPIOx_DIR[y]清零", "推挽输出或开漏输出")
require("8.3.3", "GPIOx_DIR[y]置位", "GPIOx_PUR")
require("8.3.4", "端口的数字功能关闭", "内部上拉被断开")
for number in ["8.6.4", "8.6.5"]:
    for value in range(8):
        require(number, f"{value:04b}：" + ("GPIO" if value == 0 else f"AF{value}"))
    require(number, "31:28")
# The manual allocates full nibbles but does not define AF8..AF15. No inference
# that all nibble high bits are explicitly declared reserved is made here.
print("PASS L011 basic GPIO: direction, open drain, pull-up/analog policy and AF0..7")

require("15.3.2", "0xCCCC", "从0xFFF向下计数", "PAUSE", "深度休眠模式时保持IWDT定时器运行")
require("15.3.3", "默认值是0x0FFF", "修改IWDT_WINR寄存器的值，会触发重载操作")
require("15.3.4", "0x5555", "其他任何值，启动锁定保护")
require("15.3.5", "0xCCCC", "0xAAAA", "0x5A5A、0xA5A5")
require("15.3.7", "32.8kHz", "ARR+1")
require("15.4.1", "需要启动IWDT后，才可对相关寄存器进行修改")
require("15.4.3", "RELOAD", "变为0")
for bits, name in [("5", "RELOAD"), ("4", "RUN"), ("2", "WINRF"), ("1", "ARRF"), ("0", "CRF")]:
    field("15.6.6", bits, name, "RO")
print("PASS L011 IWDT: LSI/count formula, startup, lock/reload keys and synchronization flags")

require("16.8.1", "Baud=UCLK/(BRRI×16+BRRF)", "Baud=UCLK/(BRRI×8)", "Baud=UCLK/(BRRI×4)", "字符的最后1Bit为奇偶校验位")
for bits, name in [("8", "FE"), ("9", "PE"), ("10", "NE"), ("11", "ORE"), ("14", "TXBUSY")]:
    field("16.8.11", bits, name, "RO")
require("16.8.12", "Resetvalue:0x00001FFF")
for bits, name in [("1", "TC"), ("2", "RC"), ("8", "FE"), ("9", "PE"), ("10", "NE"), ("11", "ORE")]:
    field("16.8.12", bits, name, "R1W0")
print("PASS L011 UART: PCLK sampling formulas, parity length, live flags and R1W0 ICR")

require("17.7.1", "延后约20ns", "只有当SPI_CR2.EN为0时，才可以修改本寄存器")
for encoding in range(8):
    require("17.7.1", f"{encoding:03b}：PCLK/{2 << encoding}")
field("17.7.2", "0", "EN", "RW")
for bits, name in [("0", "TXE"), ("1", "RXNE"), ("5", "OV"), ("7", "MODF"), ("8", "BUSY")]:
    field("17.7.6", bits, name, "RO")
require("17.7.7", "Resetvalue:0x000000FF")
for bits, name in [("0", "FLUSH"), ("1", "RXNE"), ("5", "OV"), ("7", "MODF")]:
    field("17.7.7", bits, name, "R1W0")
print("PASS L011 SPI: eight divisors, disabled-only config, separate EN, 20ns SMP and R1W0 flags")

require("18.4.2", "fSCL=fPCLK/8/(BRR+1)", "BRR有效范围为1~255")
require("18.4.3", "BRR的值小于或等于9", "FLT为1")
require("18.7.3", "000：I2C_SDA引脚", "000：I2C_SCL引脚", "W0：清除I2C中断标志并使状态机执行下一个动作", "需要用户手动清除STA", "硬件会在完成STOP信号发送后自动对STO清0")
for code in ["08H", "10H", "18H", "20H", "28H", "30H", "38H", "40H", "48H", "50H", "58H", "F8H", "00H"]:
    require("18.4.12", code)
print("PASS L011 I2C: BRR/filter/input mux, SI semantics, START/STOP and master state codes")
print("L011 own-manual corroboration passed; no production drivers changed and no hardware execution")
