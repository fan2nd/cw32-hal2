#!/usr/bin/env python3
"""Independent facts/coverage checks of generated JSON. Does not generate Rust."""
import json
from pathlib import Path
ROOT = Path(__file__).resolve().parents[2]
DATA = ROOT / "cw32-data/data"
chip = json.loads((DATA / "chips/CW32F030.json").read_text())
core = chip["cores"][0]
assert core["nvic_priority_bits"] == 2
assert chip["memory"] == []
assert "device_id" not in chip
peripherals = {p["name"]: p for p in core["peripherals"]}
assert len(peripherals) == 37
for name, address in {"GPIOA":0x48000000,"GPIOB":0x48000400,"GPIOC":0x48000800,"GPIOF":0x48001400,"SYSCTRL":0x40010000,"UART1":0x40013800,"CRC":0x40023000}.items():
    assert peripherals[name]["address"] == address, name
irqs = {i["name"]: i["number"] for i in core["interrupts"]}
assert len(irqs) == 32 and set(irqs.values()) == set(range(32))
assert irqs["DMACH23"] == 10 and irqs["UART1"] == 27 and irqs["AWT"] == 30 and irqs["FAULT"] == 31
for name in ["CW32F030C8","CW32F030K8","CW32F030F8","CW32F030F6"]:
    c = json.loads((DATA / f"chips/{name}.json").read_text())
    memory = {m["name"]: m for m in c["memory"][0]}
    assert memory["FLASH"]["address"] == 0
    assert memory["RAM"]["address"] == 0x20000000
    assert memory["RAM"]["size"] == (6144 if name.endswith("F6") else 8192)
    assert memory["FLASH"]["size"] == (32768 if name.endswith("F6") else 65536)
# chiptool IR uses flattened block:/fieldset: keys. Assert mixed-width CRC aliases.
crc = json.loads((DATA / "registers/crc_v1.json").read_text())
regs = {r["name"]: r for r in crc["block/CRC"]["items"]}
for n, width in [("DR8",8),("DR16",16),("DR32",32)]:
    assert regs[n]["byte_offset"] == 8
    assert regs[n].get("bit_size",32) == width
for n,width in [("RESULT16",16),("RESULT32",32)]:
    assert regs[n]["byte_offset"] == 12
    assert regs[n].get("bit_size",32) == width
    assert regs[n]["access"] == "Read"
report = json.loads((DATA / "reports/CW32F030.json").read_text())
assert report["peripherals"] == 37 and report["register_blocks"] == 22
# Four physical ADC results share one register/fieldset, and four SQR fields
# share one field-array declaration. Full scalar parity is checked separately.
assert report["registers"] == 268 - 3 and report["fields"] == 1403 - 6
print("Validated source-derived addresses, IRQs, aliases, memory corrections and full F030 import counts")
