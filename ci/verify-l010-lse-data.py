#!/usr/bin/env python3
"""Verify current own-L010 LSE SYSCLK sources and metadata; no HAL execution."""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import re
import xml.etree.ElementTree as ET
import zipfile

import yaml

ROOT = Path(__file__).resolve().parents[1]
PARTS = {"CW32L010F8P6", "CW32L010F8U6", "CW32L010Y8M6"}
PROOF = "docs/l010-lse-sysclk-qualification.json"
DETECTOR = {"lse_edges": 128, "lsi_cycles": 256, "margin_lse_edges": 1}
RM_PAGES = [52, 53, 56, 57, 58, 59, 63, 67, 68, 69, 70, 71, 74, 75, 77, 78, 79,
            80, 81, 83, 84, 87, 104, 114, 124, 130, 139, 140, 152, 200, 202,
            234, 281, 311, 377, 420]


def load(path):
    return yaml.safe_load(path.read_text())


def sha(data):
    return hashlib.sha256(data).hexdigest()


def compact(value):
    return "".join(value.split())


def fields(ir, register):
    return {f["name"]: f for f in ir["fieldset/" + register]["fields"]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sources", type=Path, required=True,
                        help="Directory holding locally acquired, locked vendor originals")
    parser.add_argument("--data", type=Path, help="Complete generated data directory")
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    authority = {a["id"]: a for a in load(ROOT / "sources/evidence-sources.json")["artifacts"]}
    catalog = load(ROOT / "cw32-data/lse-qualified.yaml")
    proof = load(ROOT / PROOF)
    auxiliary = load(ROOT / "docs/lse-l010-qualification.json")
    rtc = load(ROOT / "docs/lse-l010-rtc-admission.json")
    assert proof["schema_version"] == 1 and len(proof["parts"]) == len(PARTS)
    assert set(proof["parts"]) == set(auxiliary["parts"]) == PARTS
    assert proof["sysclk_selector"] == 4 and proof["sysclk_detector"] == DETECTOR
    assert proof["lsi_sysclk_qualification"] is False
    assert proof["lsi_cycle_timing_qualified"] is False
    assert {p for p, v in catalog["parts"].items()
            if v["family"] == "CW32L010" and "sysclk_detector" in v["configuration"]} == PARTS
    assert "CW32L010" not in catalog["parts"]
    for path, digest in catalog["policies"].items():
        assert sha((ROOT / path).read_bytes()) == digest, path
    assert [(s["source_ref"], s["pdf_pages_1_based"]) for s in proof["sources"]] == [
        ("vendor:CW32L010_UserManual_CN_V1.2.pdf", RM_PAGES),
        ("vendor:CW32L010_DataSheet_CN_V1.3.pdf", [32, 40, 42, 43]),
    ]
    original_receipts, page_receipts, texts = [], [], {}
    for source in proof["sources"]:
        locked = authority[source["source_ref"]]
        original = args.sources / locked["path"]
        assert sha(original.read_bytes()) == locked["sha256"] == source["sha256"]
        assert original.stat().st_size == locked["bytes"]
        assert source["evidence_kind"] == "external-evidence" and source["family"] == "CW32L010"
        assert PROOF in locked["evidence"]
        assert locked["provenance"]["status"] == "selected"
        assert locked["provenance"]["chip_scope"] == ["CW32L010"]
        text_bytes = (args.sources / locked["text"]["path"]).read_bytes()
        assert sha(text_bytes) == locked["text"]["sha256"]
        assert len(text_bytes) == locked["text"]["bytes"]
        pages = text_bytes.decode().split("\f")
        texts[source["source_ref"]] = pages
        assert source["printed_pages"] == [p - 1 for p in source["pdf_pages_1_based"]]
        for page in source["pdf_pages_1_based"]:
            assert 0 < page <= locked["provenance"]["pdf_page_count"]
            page_receipts.append({"source_ref": source["source_ref"], "pdf_page_1_based": page,
                                  "text_sha256": sha(pages[page - 1].encode())})
        original_receipts.append({"evidence_kind": "external-evidence", "source_ref": source["source_ref"],
                                  "path": "sources/vendor/" + locked["path"], "sha256": locked["sha256"],
                                  "bytes": locked["bytes"], "url": locked["url"]})
    manual = texts["vendor:CW32L010_UserManual_CN_V1.2.pdf"]
    datasheet = texts["vendor:CW32L010_DataSheet_CN_V1.3.pdf"]
    # Semantic anchors read from the exact locked own sources, not HAL text or
    # reconstructed historical IR. Counts and electrical fields are checked below.
    assert "SYSCTRL_CR0.SYSCLK为4" in compact(manual[62])
    assert "100：设置SysClk的时钟源为LSE" in compact(manual[66])
    assert "256个LSI时钟周期" in compact(manual[58]) and "时钟个数为128" in compact(manual[58])
    assert "LSE运行中失效时系统时钟源会自动切换到HSI4MHz时钟" in compact(manual[58])
    assert "1：自动将SysClk的时钟源切换为HSI4MHz" in compact(manual[67])
    assert "FLASH_CR2[2:0]功能相同" in compact(manual[69])
    assert "SYSCTRL_CR2[6:4]功能相同" in compact(manual[113])
    assert "必须先使能FLASH配置时钟" in compact(manual[103])
    assert "Resetvalue:0x--------" in compact(manual[70])
    assert "0x001007C0" in compact(manual[70])
    assert "32.8" in manual[55] and "±10%" in compact(manual[55])
    assert all(value in compact(datasheet[31]) for value in ["fHCLK", "fPCLK", "1.62V≤VDD＜1.8V", "24", "48"])
    assert all(value in compact(datasheet[42]) for value in ["ACCHSI", "48", "-2.0", "+2.0", "-40℃~+85℃"])
    sdk = proof["sdk_corroboration"]
    locked_sdk = authority[sdk["source_ref"]]
    archive_path = args.sources / locked_sdk["path"]
    assert sdk["sha256"] == locked_sdk["sha256"] == sha(archive_path.read_bytes())
    assert archive_path.stat().st_size == locked_sdk["bytes"] and PROOF in locked_sdk["evidence"]
    sdk_members = {}
    with zipfile.ZipFile(archive_path) as archive:
        for member in sdk["members"]:
            data = archive.read(member["archive_member"])
            assert sha(data) == member["sha256"] and len(data) == member["bytes"]
            sdk_members[member["archive_member"]] = data
    header = next(data for name, data in sdk_members.items() if name.endswith("/cw32l010_sysctrl.h")).decode()
    for name, value in [("HSI", 0), ("HSE", 1), ("LSI", 3), ("LSE", 4)]:
        match = re.search(r"#define\s+SYSCTRL_SYSCLKSRC_" + name + r"\s+\(0x([0-9A-Fa-f]+)U\)", header)
        assert match and int(match.group(1), 16) == value
    startup = next(data for name, data in sdk_members.items() if name.endswith("/system_cw32l010.c")).decode()
    assert "CW_SYSCTRL->HSI_f.TRIM = *((volatile uint16_t *)SYSCTRL_HSI_TRIMCODEADDR);" in startup
    assert "CW_SYSCTRL->LSI_f.TRIM = *((volatile uint16_t *)SYSCTRL_LSI_TRIMCODEADDR);" in startup
    svd = ET.fromstring(next(data for name, data in sdk_members.items() if name.endswith("/CW32L010.svd")))
    sysctrl = load(ROOT / "cw32-data/registers/sysctrl_cw32l010_v1.yaml")
    flash = load(ROOT / "cw32-data/registers/flash_cw32l010_v1.yaml")
    own_fields = [
        ("SYSCTRL", sysctrl, "CR0", 0, "SYSCLK", 0, 3),
        ("SYSCTRL", sysctrl, "CR0", 0, "PCLKPRS", 3, 2),
        ("SYSCTRL", sysctrl, "CR0", 0, "HCLKPRS", 5, 3),
        ("SYSCTRL", sysctrl, "CR0", 0, "KEY", 16, 16),
        ("SYSCTRL", sysctrl, "CR1", 4, "CLKCCS", 8, 1),
        ("SYSCTRL", sysctrl, "CR2", 8, "FLASHWAIT", 4, 3),
        ("SYSCTRL", sysctrl, "AHBEN", 48, "FLASH", 1, 1),
        ("SYSCTRL", sysctrl, "HSI", 24, "TRIM", 0, 11),
        ("SYSCTRL", sysctrl, "HSI", 24, "DIV", 11, 4),
        ("SYSCTRL", sysctrl, "HSI", 24, "STABLE", 15, 1),
        ("SYSCTRL", sysctrl, "LSI", 32, "TRIM", 0, 10),
        ("SYSCTRL", sysctrl, "LSI", 32, "WAITCYCLE", 10, 2),
        ("SYSCTRL", sysctrl, "LSI", 32, "STABLE", 15, 1),
        ("FLASH", flash, "CR2", 4, "WAIT", 0, 3),
        ("FLASH", flash, "CR2", 4, "KEY", 16, 16),
    ]
    for owner, ir, reg, offset, name, bit, width in own_fields:
        assert next(r for r in ir["block/" + owner]["items"] if r["name"] == reg)["byte_offset"] == offset
        field = fields(ir, reg)[name]
        assert (field["bit_offset"], field["bit_size"]) == (bit, width)
        peripheral = next(p for p in svd.findall("./peripherals/peripheral") if p.findtext("name") == owner)
        register = next(r for r in peripheral.findall("./registers/register") if r.findtext("name") == reg)
        assert int(register.findtext("addressOffset"), 0) == offset
        source_field = next(f for f in register.findall("./fields/field") if f.findtext("name") == name)
        assert (int(source_field.findtext("lsb"), 0), int(source_field.findtext("msb"), 0)) == (bit, bit + width - 1)
    assert set(fields(sysctrl, "HSI")) == {"TRIM", "DIV", "STABLE"}
    assert {v["name"]: v["value"] for v in sysctrl["enum/Sysclk"]["variants"]} == {"HSI": 0, "HSE": 1, "LSI": 3, "LSE": 4}
    assert fields(sysctrl, "CR0")["SYSCLK"]["enum"] == "Sysclk"
    assert proof["flash_fields"] == {"sysctrl_cr2_flashwait": [8, 4, 3], "flash_cr2_wait": [4, 0, 3],
                                     "sysctrl_ahben_flash": [48, 1, 1], "key": 0x5a5a,
                                     "same_function_documented": True, "alias_timing_measured": False}
    limits = load(ROOT / "cw32-data/electrical.yaml")["profiles"]["CW32L010"]["clock_limits"]
    assert (limits["hsi_frequency_hz"], limits["hsi_error_percent"]) == (48000000, 2)
    assert limits["factory_hsi_trim_address"] == 0x1007c0
    assert "lsi_sysclk" not in limits and "pll" not in limits
    expected_electrical = {"supply_mv": limits["hsi_supply_mv"], "temperature_c": limits["hsi_temperature_c"]}
    for key in ["low_voltage_threshold_mv", "low_voltage_bus_max_hz", "high_voltage_bus_max_hz", "flash_wait_step_hz", "initial_flash_wait"]:
        expected_electrical[key] = limits[key]
    assert proof["electrical_limits"] == expected_electrical == {
        "supply_mv": [1620, 5500], "temperature_c": [-40, 85], "low_voltage_threshold_mv": 1800,
        "low_voltage_bus_max_hz": 24000000, "high_voltage_bus_max_hz": 48000000,
        "flash_wait_step_hz": 24000000, "initial_flash_wait": 1}
    fallback = proof["fallback"]
    assert fallback["metadata_source"] == "clock_limits.hse.fixed_ccs_hsi_divisor"
    divisor = limits["hse"]["fixed_ccs_hsi_divisor"]
    assert divisor == fallback["fixed_ccs_hsi_divisor"] == 12
    nominal = limits["hsi_frequency_hz"] // divisor
    maximum = nominal * (100 + limits["hsi_error_percent"]) // 100
    assert nominal == fallback["documented_nominal_hz"] == 4000000
    assert fallback["modeled_minimum_hz"] == nominal * 98 // 100 == 3920000
    assert fallback["modeled_maximum_hz"] == fallback["conservative_bus_and_flash_maximum_hz"] == maximum == 4080000
    assert fallback["hsi_oscillator_nominal_hz"] == limits["hsi_frequency_hz"]
    assert fallback["hsi_factory_error_percent"] == limits["hsi_error_percent"]
    assert maximum < min(limits["low_voltage_bus_max_hz"], limits["high_voltage_bus_max_hz"])
    assert all(fallback[key] is False for key in ["hsi_divider_credit", "ahb_divider_credit", "apb_divider_credit", "hardware_divisor_rewrite_claimed", "divisor_retention_proven"])
    assert fallback["applies_when_clkccs_disabled"] is True
    assert proof["hsi_register"] == {"factory_trim_address": 0x1007c0, "trim_bit_offset": 0, "trim_bit_size": 11,
                                     "div_bit_offset": 11, "div_bit_size": 4, "stable_bit": 15,
                                     "has_wait_field": False, "hardware_reset_factory_match_assumed": False}
    assert proof["ccs_policy"] == {"preserve": ["CLKCCS", "HSECCS", "LSELOCK"], "startup_only_requires_lseccs": False,
                                   "monitored_existing_routes_requires_lseccs": True, "automatic_monitor_preparation": False}
    pins = {p["name"]: p for p in load(ROOT / "cw32-data/pinouts/cw32l010.yaml")["packages"]}
    for part in sorted(PARTS):
        current = catalog["parts"][part]
        config = copy.deepcopy(current["configuration"])
        assert config.pop("sysclk_detector") == DETECTOR
        native = config.pop("native_low_power")
        assert native["monitor_reference"] == "inherited_legal"
        assert native["lsi_factory_trim_address"] is None
        assert native["monitored_lsi_maximum_hz"] == 36080
        assert (native["detector_lse_edges"], native["detector_lsi_cycles"], native["detector_margin_lse_edges"]) == (128, 256, 1)
        monitor = proof["native_lsi_reference"]
        assert monitor["metadata_source"] == "lse_configuration.native_low_power"
        assert monitor["monitor_reference"] == native["monitor_reference"]
        assert monitor["lsi_factory_trim_address"] == native["lsi_factory_trim_address"]
        assert monitor["maximum_hz"] == native["monitored_lsi_maximum_hz"]
        for key in ["detector_lse_edges", "detector_lsi_cycles", "detector_margin_lse_edges"]:
            assert monitor[key] == native[key]
        assert all(monitor[k] is True for k in ["stable_before_monitored_preflight", "trim_and_wait_retained", "cold_start_does_not_qualify_monitor"])
        assert monitor["software_lsien_required"] is False
        assert 18180 * 256 <= 129 * 36080 < 18181 * 256
        # Preserve the accepted migration projection without altering any
        # historical proof, register history or auxiliary qualification.
        for key in ["monitor_reference", "lsi_factory_trim_address", "detector_margin_lse_edges"]:
            native.pop(key)
        config["native_l010"] = native
        assert config == auxiliary["configuration"]
        assert {k: v for k, v in current.items() if k != "configuration"} == auxiliary["parts"][part]
        assert config["rtc_reset"] == rtc["rtc_reset"]
        assert pins[part]["package"] == current["package"]
        for pin, position in [("PB1", auxiliary["package_pins"][part]["input_position"]),
                              ("PB0", auxiliary["package_pins"][part]["output_position"])]:
            assert any(p["position"] == position and pin in p["signals"] for p in pins[part]["pins"])
    generated, generated_parts = 0, set()
    if args.data:
        paths = sorted((args.data / "chips").glob("*.json"))
        assert paths, "--data must contain generated chips"
        for path in paths:
            chip = load(path)
            owner = next(p for p in chip["cores"][0]["peripherals"] if p["name"] == "SYSCTRL")
            config = owner["clock_limits"].get("lse_configuration")
            expected = catalog["parts"].get(chip["name"], {}).get("configuration")
            assert config == expected, chip["name"]
            if chip["line"] == "CW32L010":
                assert bool(config and config.get("sysclk_detector")) == (chip["name"] in PARTS)
                if chip["name"] in PARTS:
                    generated_parts.add(chip["name"])
                    clock = owner["clock_limits"]
                    for key in ["hsi_frequency_hz", "hsi_error_percent", "factory_hsi_trim_address", "hsi_supply_mv", "hsi_temperature_c",
                                "low_voltage_threshold_mv", "low_voltage_bus_max_hz", "high_voltage_bus_max_hz", "flash_wait_step_hz", "initial_flash_wait"]:
                        assert clock[key] == limits[key]
                    assert clock["hse"]["fixed_ccs_hsi_divisor"] == divisor
                    assert clock.get("lsi_sysclk") is None
            generated += 1
        assert generated_parts == PARTS, "complete generated exact-three projection is required"
    result = {"status": "passed", "scope": "current own-L010 source and metadata qualification",
              "qualified_parts": sorted(PARTS), "originals": original_receipts,
              "sdk_archive": {"evidence_kind": "external-evidence", "source_ref": sdk["source_ref"], "sha256": sdk["sha256"]},
              "sdk_members_verified": sdk["members"], "source_pages": page_receipts,
              "native_fields_verified": len(own_fields), "auxiliary_projection_preserved": True,
              "catalog_auxiliary_profiles": len(catalog["parts"]),
              "catalog_sysclk_profiles": sum("sysclk_detector" in p["configuration"] for p in catalog["parts"].values()),
              "generated_selections_checked": generated, "generated_exact_parts_checked": sorted(generated_parts),
              "hal_execution": False, "hardware_execution": False}
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, indent=2) + "\n")
    print(f"PASS: own L010 originals, {len(sdk_members)} SDK members, {len(page_receipts)} pages, exact three profiles; {generated} generated selections; no HAL execution")


if __name__ == "__main__":
    main()
