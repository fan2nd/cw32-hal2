#!/usr/bin/env python3
"""Verify the independent-watchdog source contract without hardware execution.

Raw official inputs remain external. Override CW32_SOURCES for another checkout.
"""
import hashlib
import json
import os
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
SOURCES = Path(os.environ.get("CW32_SOURCES", "/workspace/shared/cw32-sources"))
EVIDENCE = json.loads((ROOT / "docs/watchdog-evidence.json").read_text())
MASKS = {"KR_KR": 0xffff, "CR_PRS": 7, "CR_ACTION": 8, "CR_IE": 16,
         "CR_PAUSE": 32, "ARR_ARR": 0xfff, "SR_CRF": 1, "SR_ARRF": 2,
         "SR_WINRF": 4, "SR_OV": 8, "SR_RUN": 16, "SR_RELOAD": 32,
         "WINR_WINR": 0xfff, "CNT_CNT": 0xfff}
FAMILIES = {row["family"]: row for row in EVIDENCE["families"]}
assert len(FAMILIES) == 13
for family, row in FAMILIES.items():
    texts = {}
    for role, source in row["sources"].items():
        path = SOURCES / source["path"]
        assert path.exists(), f"Missing official input: {path}"
        data = path.read_bytes()
        assert hashlib.sha256(data).hexdigest() == source["sha256"], f"Changed source: {path}"
        if path.suffix != ".pdf": texts[role] = data.decode("utf-8", errors="replace")
    header = texts["sdk_iwdt_header"]
    for key, expected in [("UNLOCK", 0x5555), ("LOCK", 0x6666), ("RUN", 0xcccc), ("REFRESH", 0xaaaa), ("STOP_KEY1", 0x5a5a), ("STOP_KEY2", 0xa5a5)]:
        name = "IWDT_" + key + ("" if key.startswith("STOP") else "_KEY")
        found = re.search(r"#define\s+" + name + r"\s+(0[xX][a-fA-F0-9]+)", header)
        assert found and int(found[1], 16) == expected, (family, name)
    for encoding in range(8):
        found = re.search(r"#define\s+IWDT_Prescaler_DIV" + str(4 << encoding) + r"\s+[^\n]*?(0[xX][a-fA-F0-9]+)", header)
        assert found and int(found[1], 16) == encoding, (family, encoding)
    for field, value in MASKS.items():
        found = re.search(r"#define\s+IWDT_" + field + r"_Msk\s+\((0[xX][a-fA-F0-9]+)(?:UL|U|L)?\)", texts["sdk_cmsis_header"])
        assert found and int(found[1], 16) == value, (family, field)
    driver = texts["sdk_iwdt_driver"]
    start = driver.index("IWDT_Init(")
    init = driver[start:driver.index("__IWDT_LOCK()", start)]
    assert init.index("__IWDT_RUN()") < init.index("__IWDT_UNLOCK()") < init.index("CW_IWDT->CR"), family
    assert "IWDT_SR_CRF_Msk" in init and "IWDT_SR_ARRF_Msk" in init and "IWDT_SR_WINRF_Msk" in init
    ds = texts["datasheet_text"]
    symbol = "f" + row["clock_source"]
    found = re.search(r"\b" + symbol + r"\s+频率[^\n]*?\s([\d.]+)\s+-\s+kHz", ds)
    assert found and int(float(found[1]) * 1000) == row["clock_typical_hz"], (family, "typical clock")
    table = ds[found.start():found.start()+700]
    accuracy = re.search(r"TA\s*=\s*-40℃\s*~\s*\+" + str(row["temperature_c"][1]) + r"℃\s+(-\d+)\s+-\s+\+?(\d+)", table)
    assert accuracy and [int(accuracy[1]), int(accuracy[2])] == row["accuracy_percent"], (family, "accuracy", table)
    if row["clock_source"] == "LSI":
        assert "SYSCTRL_LSI_Enable" in driver and "LSIEN" in driver
    if "manual_text" in texts:
        manual = texts["manual_text"]
        assert re.search(r"启动\s*IWDT\s*后.*(?:才可|才能)", manual), (family, "start before config")
        assert "0x5A5A" in manual and "0xA5A5" in manual
        assert re.search(r"从\s*0xFFF\s*向下计数", manual), (family, "initial counter")
    pac = json.loads((ROOT / "cw32-data/data/registers" / ("iwdt_" + row["register_version"] + ".json")).read_text())
    offsets = {r["name"]: r["byte_offset"] for r in pac["block/IWDT"]["items"]}
    assert offsets == EVIDENCE["common"]["register_offsets"], family
    sysver = {"CW32A030":"v1", "CW32F030":"v1", "CW32R031":"cw32l031_v1", "CW32W031":"cw32l031_v1"}.get(family, family.lower()+"_v1")
    sys = json.loads((ROOT / "cw32-data/data/registers" / ("sysctrl_"+sysver+".json")).read_text())
    fields = {v["name"]: v for v in sys["fieldset/"+row["gate_register"]]["fields"]}
    assert fields["IWDT"]["bit_offset"] == row["gate_bit"]
    assert ("KEY" in fields) == (row["gate_key"] is not None)
    print(f"PASS {family}: official sources, frequency/tolerance, keys, prescalers, CMSIS masks, PAC offsets/gate")

chips = 0
for path in (ROOT / "cw32-data/data/chips").glob("*.json"):
    chip = json.loads(path.read_text())
    if chip["line"] not in FAMILIES: continue
    row = FAMILIES[chip["line"]]
    p = next(p for p in chip["cores"][0]["peripherals"] if p["name"] == "IWDT")
    assert p["registers"] == {"block":"IWDT", "kind":"iwdt", "version":row["register_version"]}
    assert p["address"] == int(row["base_address"], 16)
    assert p["rcc"]["kernel_clock"] == row["clock_source"]
    assert p["rcc"]["enable"] == {"field":"IWDT", "register":row["gate_register"]}
    chips += 1
print(f"Watchdog source audit passed: 13 families and {chips} chip metadata files; no hardware execution")
