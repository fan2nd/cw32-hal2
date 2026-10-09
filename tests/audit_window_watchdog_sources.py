#!/usr/bin/env python3
"""Verify reviewed WWDT source semantics and every selected chip's routing.

Checks official source hashes/content plus curated/normalized metadata. It does
not execute device code or attest physical watchdog timing.
"""
import hashlib
import json
import os
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
SOURCES = Path(os.environ.get("CW32_SOURCES", "/workspace/shared/cw32-sources"))
EVIDENCE = json.loads((ROOT / "docs/window-watchdog-evidence.json").read_text())
POLICY = {r["family"]: r for r in EVIDENCE["families"]}
assert len(POLICY) == 13
texts = {}
for row in EVIDENCE["sources"]:
    path = SOURCES / row["file"]
    data = path.read_bytes()
    assert hashlib.sha256(data).hexdigest() == row["sha256"], path
    if path.suffix in (".txt", ".h", ".c"):
        texts[row["file"]] = data.decode("utf-8", errors="replace")

for family, row in POLICY.items():
    if not row["wwdt_present"]:
        assert family in ("CW32L010", "CW32L011")
        header = texts[next(k for k in texts if k.endswith(f"/{family.lower()}.h"))]
        assert not re.search(r"#define\s+(?:CW_)?WWDT\b|WWDT_BASE|WWDT_TypeDef", header), family
        continue
    manual = texts[row["manual"].replace(".pdf", ".txt")]
    section = manual[manual.rindex("WWDT_CR0 控制寄存器"):]
    compact = re.sub(r"\s+", "", section)
    assert "Resetvalue:0x0000007F" in compact
    assert "7ENRW1" in compact and "使能后不能禁止" in compact
    assert "置1后不可清零" in compact
    assert "写0以清除" in compact or "W0：清除WWDT预溢出标志" in compact
    for encoding, divisor in enumerate(EVIDENCE["common"]["prs_divisors"]):
        assert f"{encoding:03b}：PCLK/{divisor}" in compact, (family, divisor)
    assert re.search(r"窗口值必须小于看门狗计数器的初始值", re.sub(r"\s+", "", manual)), family
    assert "深度休眠模式下将停止计数" in re.sub(r"\s+", "", manual), family
    sdk_family = "cw32f030" if family == "CW32A030" else family.lower()
    header_key = next(k for k in texts if k.endswith(f"/{sdk_family}.h"))
    wwdt_key = next(k for k in texts if k.endswith(f"/{sdk_family}_wwdt.h"))
    driver_key = next(k for k in texts if k.endswith(f"/{sdk_family}_wwdt.c"))
    header = texts[header_key]
    base = re.search(r"#define\s+WWDT_BASE\s+(0[xX][a-fA-F0-9]+)", header)
    assert base and int(base[1], 16) == int(row["wwdt_base"], 16), family
    for column in ["wwdt_gate", "wwdt_reset"]:
        p = row[column]
        mask = re.search(r"#define\s+SYSCTRL_" + p["register"] + r"_WWDT_Msk\s+\((0[xX][a-fA-F0-9]+)", header)
        assert mask and int(mask[1], 16) == 1 << p["bit"], (family, column)
    assert re.search(r"WDT_IRQn\s*=\s*0\b", header), family
    for field, value in EVIDENCE["common"]["field_masks"].items():
        match = re.search(r"#define\s+WWDT_" + field + r"_Msk\s+\((0[xX][a-fA-F0-9]+)(?:UL|U|L)?\)", header)
        assert match and int(match[1], 16) == value, (family, field)
    for encoding, divisor in enumerate(EVIDENCE["common"]["prs_divisors"]):
        match = re.search(r"WWDT_PRESCALER_DIV" + str(divisor) + r"\s*=\s*(0[xX][a-fA-F0-9]+)", texts[wwdt_key])
        if match: assert int(match[1], 16) == encoding
        else:
            match = re.search(r"#define\s+WWDT_Prescaler_DIV" + str(divisor) + r"\s+[^\n]*?(0[xX][a-fA-F0-9]+)", texts[wwdt_key])
            assert match and int(match[1], 16) in (encoding, encoding << 7), (family, divisor)
    clear_source = texts[wwdt_key] + texts[driver_key]
    assert re.search(r"CW_WWDT->SR\s*=\s*0|REGBITS_CLR\(CW_WWDT->SR,\s*WWDT_SR_POV_Msk\)", clear_source), (family, "W0C SDK")
    version = row["wwdt_register_version"]
    registers = json.loads((ROOT / f"cw32-data/data/registers/wwdt_{version}.json").read_text())
    assert {i["name"]: i["byte_offset"] for i in registers["block/WWDT"]["items"]} == EVIDENCE["common"]["register_offsets"]
    for field, expected in EVIDENCE["common"]["field_masks"].items():
        register, name = field.split("_")
        f = next(x for x in registers[f"fieldset/{register}"]["fields"] if x["name"] == name)
        assert ((1 << f["bit_size"]) - 1) << f["bit_offset"] == expected
    print(f"PASS {family}: own manual/SDK, masks, irreversible EN/IE, W0C, all 8 divisors")

chips = 0
for path in (ROOT / "cw32-data/data/chips").glob("*.json"):
    chip = json.loads(path.read_text())
    if chip["line"] not in POLICY: continue
    row = POLICY[chip["line"]]
    core = chip["cores"][0]
    ps = [p for p in core["peripherals"] if p["name"] == "WWDT"]
    assert bool(ps) == row["wwdt_present"], path
    if not ps: continue
    p = ps[0]
    assert p["registers"] == {"block":"WWDT", "kind":"wwdt", "version":row["wwdt_register_version"]}, path
    assert p["address"] == int(row["wwdt_base"],16), path
    assert p["rcc"]["bus_clock"] == p["rcc"]["kernel_clock"] == "PCLK", path
    for which, column in [("enable", "wwdt_gate"), ("reset", "wwdt_reset")]:
        assert p["rcc"][which] == {"field":"WWDT", "register":row[column]["register"]}, path
    assert p["interrupts"] == [{"interrupt":"WDT", "signal":"GLOBAL"}], path
    assert next(i for i in core["interrupts"] if i["name"] == "WDT")["number"] == 0, path
    sysver = next(p for p in core["peripherals"] if p["name"] == "SYSCTRL")["registers"]["version"]
    sys = json.loads((ROOT / f"cw32-data/data/registers/sysctrl_{sysver}.json").read_text())
    for column in ["wwdt_gate", "wwdt_reset"]:
        policy = row[column]
        fields = {f["name"]:f for f in sys["fieldset/" + policy["register"]]["fields"]}
        assert fields["WWDT"]["bit_offset"] == policy["bit"], path
        assert ("KEY" in fields) == (policy["key"] is not None), path
    chips += 1
print(f"WWDT source audit passed: 13 family policies, {chips} WWDT-capable chip selections; no hardware execution")
