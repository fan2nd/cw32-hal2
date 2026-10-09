#!/usr/bin/env python3
"""Verify pinned official CRC inputs and every chip's documented CRC metadata."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]
SOURCES = Path(os.environ.get("CW32_SOURCES", "/workspace/shared/cw32-sources"))
EVIDENCE = json.loads((ROOT / "docs/crc-evidence.json").read_text())
FAMILIES = {row["family"]: row for row in EVIDENCE["families"]}
assert len(FAMILIES) == 13
texts = {}
for source in EVIDENCE["sources"]:
    path = SOURCES / source["file"]
    assert path.is_file(), f"Missing official source: {path}"
    content = path.read_bytes()
    assert hashlib.sha256(content).hexdigest() == source["sha256"], f"Changed source: {path}"
    if path.suffix in {".h", ".c"}:
        texts[source["file"]] = content.decode("utf-8", errors="replace")

# The A030 product page explicitly links the shared manual. Its empty SDK
# category is not positive evidence for a separate A030 vendor SDK.
a030_listing = (SOURCES / 'a030-official-manuals.html').read_text()
assert 'CW32F030/CW32A030 用户手册' in a030_listing
assert '/uploads/files/20240920/CW32x030_UserManual_EN_V1.0.pdf' in a030_listing
assert 'CW32A030C8T7' in a030_listing

# Parse the now-acquired own-family L011 manual directly from its verified PDF.
# This checks the parameter table and register protocol, rather than relying on
# another family's manual or merely finding algorithm names in an SDK header.
l011_manual = subprocess.run(
    ["pdftotext", "-layout", str(SOURCES / "CW32L011_UserManual_CN_V1.1.pdf"), "-"],
    check=True, text=True, capture_output=True,
).stdout
manual_presets = re.findall(
    r"CRC16_(IBM|MAXIM|USB|MODBUS|CCITT|CCITT_False|X25|XMODEM)\s+"
    r"(0x[0-9a-fA-F]+)\s+(0x[0-9a-fA-F]+)\s+(True|False)\s+(True|False)\s+(0x[0-9a-fA-F]+)",
    l011_manual,
)
assert len(manual_presets) == 8
for row, algorithm in zip(manual_presets, EVIDENCE["algorithms"][:8]):
    _, polynomial, seed, ref_in, ref_out, xor_out = row
    assert int(polynomial, 16) == int(algorithm["polynomial"], 16)
    assert int(seed, 16) == int(algorithm["seed"], 16)
    assert (ref_in == "True") == algorithm["reflect_input_bits_per_byte"]
    assert (ref_out == "True") == algorithm["reflect_output"]
    assert int(xor_out, 16) == int(algorithm["xor_out"], 16)

def manual_section(start, end):
    begin = list(re.finditer(r"^" + re.escape(start) + r"\s", l011_manual, re.MULTILINE))[-1].start()
    finish = re.search(r"^" + re.escape(end) + r"\s", l011_manual[begin:], re.MULTILINE)
    return l011_manual[begin:begin + finish.start()]

control = manual_section("9.6.1", "9.6.2")
assert [int(bits, 2) for bits in re.findall(r"([01]{4})：CRC16_", control)] == list(range(8))
assert re.search(r"7:0\s+DR\s+RW", manual_section("9.6.2", "9.6.3"))
assert re.search(r"15:0\s+RESULT\s+RO", manual_section("9.6.3", "10"))
assert "支持 8bit 输入数据位宽" in manual_section("9.3.2", "9.4")
gate = manual_section("4.7.11", "4.7.12")
assert re.search(r"31:16\s+KEY\s+WO[^\n]*0x5A5A", gate)
assert re.search(r"2\s+CRC\s+RW", gate)
reset = manual_section("4.7.14", "4.7.15")
assert re.search(r"2\s+CRC\s+RW\s+0：模块处于复位状态\s+1：模块正常工作", reset)
assert "KEY" not in reset

def sdk(family, suffix):
    # A030 uses the documented x030-compatible peripheral map and F030 SDK.
    stem = "cw32f030" if family == "CW32A030" else family.lower()
    return next(text for path, text in texts.items() if path.endswith("/" + stem + suffix))

for family, row in FAMILIES.items():
    narrow = family in {"CW32F002", "CW32F003"}
    native = family in {"CW32F020", "CW32F030", "CW32A030"}
    crc32 = family in {"CW32F030", "CW32A030"}
    assert row["crc_modes"] == list(range(4 if narrow else 0, 10 if crc32 else 8)), family
    assert row["crc_input_payload_bits"] == ([8, 16, 32] if native else [8]), family
    header, driver, cmsis = sdk(family, "_crc.h"), sdk(family, "_crc.c"), sdk(family, ".h")
    modes16 = [int(value, 16) for value in re.findall(r"#define\s+CRC16_\w+\s+(0x[0-9A-Fa-f]+)", header)]
    assert sorted(modes16) == list(range(4 if narrow else 0, 8)), family
    assert re.search(r"CW_CRC->CR\s*=\s*CrcMode", driver), family
    if native:
        for bits, variable in [(8, "pByteBuf"), (16, "pHWBuf"), (32, "pWBuf")]:
            assert re.search(r"CW_CRC->DR" + str(bits) + r"\s*=\s*\*" + variable, driver), (family, bits)
    if not native:
        assert re.search(r"CW_CRC->DR\s*=\s*\*pByteBuf", driver), family
        assert "CRC16_Calc_16bit" not in driver and "CRC16_Calc_32bit" not in driver, family
    for field, value in [("MODE", 15)]:
        match = re.search(r"#define\s+CRC_CR_" + field + r"_Msk\s+\((0x[0-9A-Fa-f]+)(?:UL|U|L)?\)", cmsis)
        assert match and int(match[1], 16) == value, (family, field)
    version = row["crc_register_version"]
    pac = json.loads((ROOT / "cw32-data/data/registers" / f"crc_{version}.json").read_text())
    regs = {r["name"]: r for r in pac["block/CRC"]["items"]}
    assert regs["CR"]["byte_offset"] == 0
    if native:
        for bits in [8, 16, 32]:
            assert regs[f"DR{bits}"]["byte_offset"] == 8
            assert regs[f"DR{bits}"].get("bit_size", 32) == bits
        assert regs["RESULT16"]["byte_offset"] == 12 and regs["RESULT16"]["access"] == "Read"
        assert "RESULT32" in regs
        assert pac["fieldset/RESULT32"]["fields"][0]["bit_size"] == (32 if crc32 else 16)
    else:
        assert regs["DR"]["byte_offset"] == 8
        assert regs["RESULT"]["byte_offset"] == 12 and regs["RESULT"]["access"] == "Read"
        assert pac["fieldset/DR"]["fields"][0]["bit_size"] == 8
        assert pac["fieldset/RESULT"]["fields"][0]["bit_size"] == 16
    assert ("INIT" in regs) == (family == "CW32L052")
    if family == "CW32L052":
        assert "INIT" not in driver
    sysver = {"CW32A030":"v1", "CW32F030":"v1", "CW32R031":"cw32l031_v1", "CW32W031":"cw32l031_v1"}.get(family, family.lower() + "_v1")
    sys = json.loads((ROOT / "cw32-data/data/registers" / f"sysctrl_{sysver}.json").read_text())
    for register in ["AHBEN", "AHBRST"]:
        fields = {f["name"]: f for f in sys[f"fieldset/{register}"]["fields"]}
        assert fields["CRC"]["bit_offset"] == 2
        assert ("KEY" in fields) == (register == "AHBEN" and family in {"CW32L010", "CW32L011", "CW32L012"})
    print(f"PASS {family}: source hashes, mode encodings, payload/result widths, gate/reset metadata")

chips = 0
for path in (ROOT / "cw32-data/data/chips").glob("*.json"):
    chip = json.loads(path.read_text())
    row = FAMILIES.get(chip["line"])
    if row is None:
        continue
    p = next(p for p in chip["cores"][0]["peripherals"] if p["name"] == "CRC")
    assert p["registers"] == {"block":"CRC", "kind":"crc", "version":row["crc_register_version"]}
    assert p["address"] == 0x40023000
    assert p["rcc"]["kernel_clock"] == "HCLK"
    assert p["rcc"]["enable"] == {"field":"CRC", "register":"AHBEN"}
    assert p["rcc"]["reset"] == {"field":"CRC", "register":"AHBRST"}
    assert not p.get("interrupts"), f"CRC has no documented IRQ: {path}"
    chips += 1

# Verify the independent parameter table using right-shift reflected division,
# separately from the MSB-first Rust reference, and pin L011's SDK comments.
def reverse(value, width):
    return int(f"{value:0{width}b}"[::-1], 2)

def calculate(algorithm, payload):
    width = algorithm["width"]
    mask = (1 << width) - 1
    poly = int(algorithm["polynomial"], 16)
    result = int(algorithm["seed"], 16)
    reflected = algorithm["reflect_input_bits_per_byte"]
    if reflected:
        poly = reverse(poly, width)
        for byte in payload:
            result ^= byte
            for _ in range(8):
                result = (result >> 1) ^ (poly if result & 1 else 0)
    else:
        for byte in payload:
            result ^= byte << (width - 8)
            for _ in range(8):
                result = ((result << 1) ^ (poly if result & (1 << (width - 1)) else 0)) & mask
    if reflected != algorithm["reflect_output"]:
        result = reverse(result, width)
    return result ^ int(algorithm["xor_out"], 16)

payloads = {"empty": b"", "ascii_123456789": b"123456789",
            "bytes_00_11_22_33_44_55_66_77": bytes(range(0, 0x88, 0x11))}
for algorithm in EVIDENCE["algorithms"]:
    for name, payload in payloads.items():
        assert calculate(algorithm, payload) == int(algorithm["host_reference_vectors"][name], 16)
example = texts["cw32l011/Examples/CRC/crc_calc/USER/src/main.c"]
expected = [int(value, 16) for value in re.findall(r"CRC16_Calc_8bit\([^\n]+?0x([0-9A-Fa-f]+)", example)]
assert expected == [int(a["host_reference_vectors"]["bytes_00_11_22_33_44_55_66_77"], 16)
                    for a in EVIDENCE["algorithms"][:8]]

# The independent test-only calculator is not linked into the driver.
production = (ROOT / "embassy-cw32/src/crc.rs").read_text()
assert ".init()" not in production
assert "reverse_bits" not in production
print(f"CRC source audit passed: {len(EVIDENCE['sources'])} pinned files and {chips} chip records; no hardware execution")
