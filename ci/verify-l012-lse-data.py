#!/usr/bin/env python3
"""Verify current own-L012 LSE SYSCLK sources and metadata; no HAL execution."""
import argparse
import copy
import hashlib
import io
import json
from pathlib import Path
import re
import xml.etree.ElementTree as ET
import zipfile

import yaml

ROOT = Path(__file__).resolve().parents[1]
PARTS = {"CW32L012C8T6", "CW32L012C8U6"}
PROOF = "docs/l012-lse-sysclk-qualification.json"
DETECTOR = {"lse_edges": 128, "lsi_cycles": 256, "margin_lse_edges": 1}


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
    parser.add_argument("--sources", type=Path, required=True)
    parser.add_argument("--data", type=Path, help="Complete generated data directory")
    parser.add_argument("--baseline", type=Path, help="Accepted source tree for exact previous-profile preservation")
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    authority = {a["id"]: a for a in load(ROOT / "sources/evidence-sources.json")["artifacts"]}
    catalog = load(ROOT / "cw32-data/lse-qualified.yaml")
    proof = load(ROOT / PROOF)
    auxiliary = load(ROOT / "docs/lse-l012-qualification.json")
    rtc = load(ROOT / "docs/lse-l012-rtc-admission.json")
    assert proof["schema_version"] == 1 and set(proof["parts"]) == PARTS
    assert set(auxiliary["parts"]) == PARTS
    assert proof["sysclk_selector"] == 4 and proof["sysclk_detector"] == DETECTOR
    assert all(proof[k] is False for k in ["pll_qualification", "lsi_sysclk_qualification", "lsi_cycle_timing_qualified"])
    assert len(catalog["parts"]) == 23
    assert sum("sysclk_detector" in p["configuration"] for p in catalog["parts"].values()) == 23
    assert {p for p, v in catalog["parts"].items() if v["family"] == "CW32L012"} == PARTS
    assert "CW32L012" not in catalog["parts"] and "CW32L012F8P6" not in catalog["parts"]
    for path, digest in catalog["policies"].items():
        assert sha((ROOT / path).read_bytes()) == digest, path
    assert [s["source_ref"] for s in proof["sources"]] == [
        "vendor:CW32L012_UserManual_CN_V1.4.pdf", "vendor:CW32L012_UserManual_EN_V1.0.pdf",
        "vendor:CW32L012_DataSheet_CN_V1.0.pdf"]
    originals, page_receipts, texts = [], [], {}
    for source in proof["sources"]:
        locked = authority[source["source_ref"]]
        original = args.sources / locked["path"]
        assert sha(original.read_bytes()) == locked["sha256"] == source["sha256"]
        assert original.stat().st_size == locked["bytes"]
        assert source["evidence_kind"] == "external-evidence" and source["family"] == "CW32L012"
        assert PROOF in locked["evidence"] and locked["provenance"]["chip_scope"] == ["CW32L012"]
        assert locked["provenance"]["status"] == ("corroborating-language-edition" if "_EN_" in source["source_ref"] else "selected")
        text_bytes = (args.sources / locked["text"]["path"]).read_bytes()
        assert sha(text_bytes) == locked["text"]["sha256"] and len(text_bytes) == locked["text"]["bytes"]
        pages = text_bytes.decode().split("\f")
        texts[source["source_ref"]] = pages
        assert source["printed_pages"] == [p - (3 if "DataSheet" in source["source_ref"] else 26) for p in source["pdf_pages_1_based"]]
        for page in source["pdf_pages_1_based"]:
            assert 0 < page <= locked["provenance"]["pdf_page_count"]
            page_receipts.append({"source_ref": source["source_ref"], "pdf_page_1_based": page,
                                  "text_sha256": sha(pages[page - 1].encode())})
        originals.append({"evidence_kind": "external-evidence", "source_ref": source["source_ref"],
                          "path": "sources/vendor/" + locked["path"], "sha256": locked["sha256"],
                          "bytes": locked["bytes"], "url": locked["url"]})
    cn = texts["vendor:CW32L012_UserManual_CN_V1.4.pdf"]
    en = texts["vendor:CW32L012_UserManual_EN_V1.0.pdf"]
    ds = texts["vendor:CW32L012_DataSheet_CN_V1.0.pdf"]
    # Own original semantic anchors. Frequency tables do not prove cycle timing.
    assert "256个LSI时钟周期" in compact(cn[61]) and "时钟个数为128" in compact(cn[61])
    assert "LSE运行中失效时系统时钟源会自动切换到HSI4MHz时钟" in compact(cn[61])
    assert "HSI4MHz" in compact(en[74])
    assert "FLASH_CR2[2:0]功能相同" in compact(cn[72])
    assert "SYSCTRL_CR2[6:4]功能相同" in compact(cn[126])
    assert "samefunctionasFLASH_CR2[2:0]" in compact(en[76])
    assert "samefunctionasSYSCTRL_CR2[6:4]" in compact(en[134])
    assert all(v in compact(en[77]) for v in ["1011:HSI=HSIOSC/12", "1110:HSI=HSIOSC/24", "0000:HSI=HSIOSC/32", "10:0TRIM"])
    assert all(v in compact(en[78]) for v in ["8:0TRIM", "0x001007C2", "11:10WAITCYCLE"])
    assert "32.8kHz±10%" in compact(cn[58])
    assert "ADC模块配置时钟及工作时钟使能控制" in compact(cn[83])
    assert "ADCmoduleconfigurationclockandworkingclockenablecontrol" in compact(en[88])
    assert all(v in compact(en[75]) for v in ["VCx_CR1.FLTCLK=0x0(LSI)andVCx_CR0.EN=1", "LVD_CR0.FLTCLK=0x0(LSI)andLVD_CR0.EN=1"])
    assert all(v in compact(ds[46]) for v in ["fHCLK", "fPCLK", "1.7V≤VDD＜1.8V", "24", "96"])
    assert all(v in compact(ds[57]) for v in ["ACCHSI", "-2.0", "+2.0", "ACCLSI", "32.8", "-10", "+10", "0.16"])
    assert all(v in compact(ds[54]) for v in ["fLSE_EXT", "100", "0.7VDDIOx", "0.3VDDIOx"])
    assert "BTIM1" in cn[231] and "BTIM2" in cn[231] and "BTIM3" in cn[231]
    sdk = proof["sdk_corroboration"]
    locked_sdk = authority[sdk["source_ref"]]
    archive_path = args.sources / locked_sdk["path"]
    assert sdk["sha256"] == locked_sdk["sha256"] == sha(archive_path.read_bytes())
    assert archive_path.stat().st_size == locked_sdk["bytes"] and PROOF in locked_sdk["evidence"]
    sdk_members = {}
    with zipfile.ZipFile(archive_path) as archive:
        for member in sdk["members"]:
            data = archive.read(member["archive_members"][0])
            for nested in member["archive_members"][1:]:
                data = zipfile.ZipFile(io.BytesIO(data)).read(nested)
            assert sha(data) == member["sha256"] and len(data) == member["bytes"]
            sdk_members["/".join(member["archive_members"])] = data
    header = next(data for name, data in sdk_members.items() if name.endswith("cw32l012_sysctrl.h")).decode()
    for name, value in [("HSI", 0), ("HSE", 1), ("LSI", 3), ("LSE", 4)]:
        match = re.search(r"#define\s+SYSCTRL_SYSCLKSRC_" + name + r"\s+\(0x([0-9A-Fa-f]+)U\)", header)
        assert match and int(match.group(1), 16) == value
    assert "#define SYSCTRL_SYSCLKSRC_PLL" not in header
    for divisor, encoding in [(12, 11), (24, 14)]:
        assert re.search(r"#define\s+SYSCTRL_HSIOSC_DIV" + str(divisor) + r"\s+\(\(uint32_t\)\(" + str(encoding) + r"UL << SYSCTRL_HSI_DIV_Pos\)\)", header)
    assert re.search(r"#define\s+SYSCTRL_LSI_TRIMCODEADDR\s+\(0x001007C2U\)", header)
    svd = ET.fromstring(next(data for name, data in sdk_members.items() if name.endswith("/CW32L012.svd")))
    selected = load(ROOT / "cw32-data/inputs/cw32l012.yaml")["register_versions"]
    native_fields = NATIVE_FIELDS
    irs = {}
    for owner, reg, offset, name, bit, width in native_fields:
        kind = re.sub(r"\d+$", "", owner).lower()
        ir = irs.setdefault(kind, load(ROOT / "cw32-data/registers" / (kind + "_" + selected[kind] + ".yaml")))
        item = next(r for r in ir["block/" + kind.upper()]["items"] if r["name"] == reg)
        assert item["byte_offset"] == offset
        field = fields(ir, reg)[name]
        assert (field["bit_offset"], field["bit_size"]) == (bit, width), (owner, reg, name)
        peripheral = next(p for p in svd.findall("./peripherals/peripheral") if p.findtext("name") == owner)
        if "derivedFrom" in peripheral.attrib:
            peripheral = next(p for p in svd.findall("./peripherals/peripheral") if p.findtext("name") == peripheral.attrib["derivedFrom"])
        register = next(r for r in peripheral.findall("./registers/register") if r.findtext("name") == reg)
        assert int(register.findtext("addressOffset"), 0) == offset
        source_field = next(f for f in register.findall("./fields/field") if f.findtext("name") == name)
        assert (int(source_field.findtext("lsb"), 0), int(source_field.findtext("msb"), 0)) == (bit, bit + width - 1)
    assert set(fields(irs["sysctrl"], "HSI")) == {"TRIM", "DIV", "STABLE"}
    assert {v["name"]: v["value"] for v in irs["sysctrl"]["enum/Sysclk"]["variants"]} == {"HSI": 0, "HSE": 1, "LSI": 3, "LSE": 4}
    assert selected["sysctrl"] == selected["flash"] == "cw32l012_v1"
    assert len(fields(irs["vc"], "CR2")) == 32
    assert all(f["bit_size"] == 1 for f in fields(irs["vc"], "CR2").values())
    limits = load(ROOT / "cw32-data/electrical.yaml")["profiles"]["CW32L012"]["clock_limits"]
    assert (limits["hsi_frequency_hz"], limits["hsi_error_percent"], limits["factory_hsi_trim_address"]) == (96000000, 2, 0x1007c0)
    assert "lsi_sysclk" not in limits and "pll" not in limits
    assert proof["hsi_division"] == {"default_numeric_divisor": 12, "default_encoding": 11, "reset_fallback_numeric_divisor": 24, "reset_fallback_encoding": 14, "encoding_zero_numeric_divisor": 32}
    assert limits["default_hsi_divisor"] == 12 and limits["hse"]["fixed_ccs_hsi_divisor"] == 24
    expected = {"supply_mv": limits["hsi_supply_mv"], "temperature_c": limits["hsi_temperature_c"]}
    for key in ["low_voltage_threshold_mv", "low_voltage_bus_max_hz", "high_voltage_bus_max_hz", "flash_wait_step_hz", "initial_flash_wait"]:
        expected[key] = limits[key]
    assert proof["electrical_limits"] == expected == {"supply_mv": [1700, 5500], "temperature_c": [-40, 85], "low_voltage_threshold_mv": 1800, "low_voltage_bus_max_hz": 24000000, "high_voltage_bus_max_hz": 96000000, "flash_wait_step_hz": 24000000, "initial_flash_wait": 3}
    fallback = proof["fallback"]
    assert (fallback["fixed_ccs_hsi_divisor"], fallback["documented_nominal_hz"], fallback["modeled_minimum_hz"], fallback["modeled_maximum_hz"], fallback["conservative_bus_and_flash_maximum_hz"]) == (24, 4000000, 3920000, 4080000, 4080000)
    assert all(fallback[k] is False for k in ["hsi_divider_credit", "ahb_divider_credit", "apb_divider_credit", "hardware_divisor_rewrite_claimed", "divisor_retention_proven"])
    assert proof["flash_fields"] == {"sysctrl_cr2_flashwait": [8, 4, 3], "flash_cr2_wait": [4, 0, 3], "sysctrl_ahben_flash": [48, 1, 1], "key": 0x5a5a, "same_function_documented": True, "alias_timing_measured": False}
    assert proof["flash_preserved_fields"] == {"FETCH": 3, "CACHE": 4, "CACHEINVALID": 5}
    policy = proof["consumer_policy"]
    assert policy["adc_instances"] == ["ADC1", "ADC2"] and policy["adc_shared_gate"] == "configuration_and_work" and policy["adc_gate_handover_required"]
    assert policy["vc_instances"] == ["VC1", "VC2", "VC3", "VC4"] and policy["vc_shared_gate"] == "configuration_only"
    assert policy["i2c_sources_refused_on_hsi_or_first_lsi_disturbance"] == [1, 3] and policy["uart3_open_gate_only"]
    pins = {p["name"]: p for p in load(ROOT / "cw32-data/pinouts/cw32l012.yaml")["packages"]}
    for part in sorted(PARTS):
        current = catalog["parts"][part]
        config = copy.deepcopy(current["configuration"])
        assert config.pop("sysclk_detector") == DETECTOR
        assert config == auxiliary["configuration"]
        assert {k: v for k, v in current.items() if k != "configuration"} == auxiliary["parts"][part]
        assert config["rtc_reset"] == rtc["rtc_reset"]
        native = config["native_low_power"]
        monitor = proof["native_lsi_reference"]
        assert native["monitor_reference"] == monitor["monitor_reference"] == "factory_trim"
        assert native["lsi_factory_trim_address"] == monitor["lsi_factory_trim_address"] == 0x1007c2
        assert native["monitored_lsi_maximum_hz"] == monitor["maximum_hz"] == 36080 and monitor["trim_bit_size"] == 9
        assert (native["detector_lse_edges"], native["detector_lsi_cycles"], native["detector_margin_lse_edges"]) == (128, 256, 1)
        assert 18180 * 256 <= 129 * 36080 < 18181 * 256
        assert (native["rtc_first_divisor"], native["rtc_second_divisor"], native["rtc_calendar_divisor"]) == (1, 16384, 32768)
        assert pins[part]["package"] == current["package"]
        for pin, position in [("PC14", "3"), ("PC15", "4")]:
            assert any(str(p["position"]) == position and pin in p["signals"] for p in pins[part]["pins"])
    baseline_checked = False
    if args.baseline:
        old = load(args.baseline / "cw32-data/lse-qualified.yaml")
        assert len(old["parts"]) == 23 and sum("sysclk_detector" in p["configuration"] for p in old["parts"].values()) == 21
        projected = copy.deepcopy(catalog["parts"])
        for part in PARTS:
            assert projected[part]["configuration"].pop("sysclk_detector") == DETECTOR
        assert projected == old["parts"]
        for path in old["policies"]:
            assert (ROOT / path).read_bytes() == (args.baseline / path).read_bytes(), path
        old_authority = load(args.baseline / "sources/evidence-sources.json")
        new_authority = copy.deepcopy(load(ROOT / "sources/evidence-sources.json"))
        for artifact in new_authority["artifacts"]:
            if PROOF in artifact.get("evidence", []):
                artifact["evidence"].remove(PROOF)
        assert new_authority == old_authority
        baseline_checked = True
    generated, generated_parts, generic = 0, set(), False
    if args.data:
        paths = sorted((args.data / "chips").glob("*.json"))
        assert paths, "--data must contain generated chips"
        for path in paths:
            chip = load(path)
            owner = next(p for p in chip["cores"][0]["peripherals"] if p["name"] == "SYSCTRL")
            config = owner["clock_limits"].get("lse_configuration")
            assert config == catalog["parts"].get(chip["name"], {}).get("configuration"), chip["name"]
            if chip["line"] == "CW32L012":
                assert bool(config and config.get("sysclk_detector")) == (chip["name"] in PARTS)
                assert not owner["clock_limits"].get("lsi_sysclk") and not owner["clock_limits"].get("pll")
                if chip["name"] in PARTS:
                    generated_parts.add(chip["name"])
                    assert owner["clock_limits"]["default_hsi_divisor"] == 12
                generic |= chip["name"] == "CW32L012"
            generated += 1
        assert generated_parts == PARTS and generic, "complete exact-two and generic-negative projection required"
    result = {"status": "passed", "scope": "current own-L012 source and metadata qualification", "qualified_parts": sorted(PARTS),
              "originals": originals, "sdk_archive": {"evidence_kind": "external-evidence", "source_ref": sdk["source_ref"], "sha256": sdk["sha256"]},
              "sdk_members_verified": sdk["members"], "source_pages": page_receipts, "native_fields_verified": len(native_fields),
              "auxiliary_projection_preserved": True, "prior_21_and_auxiliary_23_baseline_checked": baseline_checked,
              "catalog_auxiliary_profiles": 23, "catalog_sysclk_profiles": 23, "generated_selections_checked": generated,
              "generated_exact_parts_checked": sorted(generated_parts), "generic_l012_absence_checked": generic,
              "hal_execution": False, "hardware_execution": False}
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, indent=2) + "\n")
    print(f"PASS: own L012 originals, {len(sdk_members)} SDK members, {len(page_receipts)} pages, {len(native_fields)} fields, exact two profiles; {generated} generated selections; no HAL execution")


NATIVE_FIELDS = [
    ('FLASH', 'CR2', 4, 'FETCH', 3, 1),
    ('FLASH', 'CR2', 4, 'CACHE', 4, 1),
    ('FLASH', 'CR2', 4, 'CACHEINVALID', 5, 1),
    ('SYSCTRL', 'CR1', 4, 'HSIEN', 0, 1),
    ('SYSCTRL', 'CR1', 4, 'HSEEN', 1, 1),
    ('SYSCTRL', 'CR1', 4, 'LSIEN', 3, 1),
    ('SYSCTRL', 'CR1', 4, 'HSECCS', 7, 1),
    ('SYSCTRL', 'CR1', 4, 'LSECCS', 6, 1),
    ('SYSCTRL', 'CR1', 4, 'LSELOCK', 5, 1),
    ('SYSCTRL', 'IER', 12, 'HSIRDY', 0, 1),
    ('SYSCTRL', 'IER', 12, 'LSIRDY', 3, 1),
    ('SYSCTRL', 'MCO', 112, 'SOURCE', 0, 4),
    ('SYSCTRL', 'LSI', 32, 'TRIM', 0, 9),
    ('SYSCTRL', 'LSI', 32, 'WAITCYCLE', 10, 2),
    ('SYSCTRL', 'LSI', 32, 'STABLE', 15, 1),
    ('SYSCTRL', 'APBEN1', 56, 'ADC', 0, 1),
    ('SYSCTRL', 'APBEN1', 56, 'VC', 1, 1),
    ('SYSCTRL', 'APBEN1', 56, 'UART1', 3, 1),
    ('SYSCTRL', 'APBEN1', 56, 'UART2', 4, 1),
    ('SYSCTRL', 'APBEN1', 56, 'UART3', 8, 1),
    ('SYSCTRL', 'APBRST1', 72, 'ADC', 0, 1),
    ('SYSCTRL', 'APBRST1', 72, 'VC', 1, 1),
    ('SYSCTRL', 'APBRST1', 72, 'UART1', 3, 1),
    ('SYSCTRL', 'APBRST1', 72, 'UART2', 4, 1),
    ('SYSCTRL', 'APBRST1', 72, 'UART3', 8, 1),
    ('SYSCTRL', 'APBEN2', 52, 'RTC', 1, 1),
    ('SYSCTRL', 'APBEN2', 52, 'I2C1', 6, 1),
    ('SYSCTRL', 'APBEN2', 52, 'LPTIM', 7, 1),
    ('SYSCTRL', 'APBEN2', 52, 'OPA', 9, 1),
    ('SYSCTRL', 'APBEN2', 52, 'DAC', 10, 1),
    ('SYSCTRL', 'APBEN2', 52, 'I2C2', 11, 1),
    ('SYSCTRL', 'APBRST2', 68, 'RTC', 1, 1),
    ('SYSCTRL', 'APBRST2', 68, 'I2C1', 6, 1),
    ('SYSCTRL', 'APBRST2', 68, 'LPTIM', 7, 1),
    ('SYSCTRL', 'APBRST2', 68, 'OPA', 9, 1),
    ('SYSCTRL', 'APBRST2', 68, 'DAC', 10, 1),
    ('SYSCTRL', 'APBRST2', 68, 'I2C2', 11, 1),
    ('SYSCTRL', 'APBEN1', 56, 'KEY', 16, 16),
    ('SYSCTRL', 'APBEN2', 52, 'KEY', 16, 16),
    ('RTC', 'CR1', 8, 'SOURCE', 8, 3),
    ('RTC', 'PSC', 64, 'PSC1', 20, 8),
    ('RTC', 'PSC', 64, 'PSC2', 0, 20),
    ('UART1', 'CR1', 0, 'SOURCE', 12, 2),
    ('UART2', 'CR1', 0, 'SOURCE', 12, 2),
    ('UART3', 'CR1', 0, 'SOURCE', 12, 2),
    ('I2C1', 'MCR0', 16, 'CLKSRC', 6, 2),
    ('I2C1', 'SCR0', 272, 'CLKSRC', 6, 2),
    ('I2C2', 'MCR0', 16, 'CLKSRC', 6, 2),
    ('I2C2', 'SCR0', 272, 'CLKSRC', 6, 2),
    ('LPTIM', 'CR0', 16, 'EN', 0, 1),
    ('LPTIM', 'CFGR', 12, 'ICLKSRC', 25, 2),
    ('LPTIM', 'CFGR', 12, 'TRIGSEL', 12, 4),
    ('LPTIM', 'CFGR', 12, 'TRIGEN', 17, 2),
    ('ADC1', 'CR', 0, 'EN', 0, 1),
    ('ADC1', 'CR', 0, 'CLK', 2, 2),
    ('ADC2', 'CR', 0, 'EN', 0, 1),
    ('ADC2', 'CR', 0, 'CLK', 2, 2),
    ('LVD', 'CR0', 0, 'EN', 0, 1),
    ('LVD', 'CR0', 0, 'FLTCLK', 8, 1),
    ('LVD', 'CR1', 4, 'FLTTIME', 4, 4),
    ('VC1', 'CR0', 0, 'EN', 0, 1),
    ('VC1', 'CR1', 4, 'FLTTIME', 0, 4),
    ('VC1', 'CR1', 4, 'FLTCLK', 4, 1),
    ('VC1', 'CR1', 4, 'BLANKTIME', 8, 3),
    ('VC2', 'CR0', 0, 'EN', 0, 1),
    ('VC2', 'CR1', 4, 'FLTTIME', 0, 4),
    ('VC2', 'CR1', 4, 'FLTCLK', 4, 1),
    ('VC2', 'CR1', 4, 'BLANKTIME', 8, 3),
    ('VC3', 'CR0', 0, 'EN', 0, 1),
    ('VC3', 'CR1', 4, 'FLTTIME', 0, 4),
    ('VC3', 'CR1', 4, 'FLTCLK', 4, 1),
    ('VC3', 'CR1', 4, 'BLANKTIME', 8, 3),
    ('VC4', 'CR0', 0, 'EN', 0, 1),
    ('VC4', 'CR1', 4, 'FLTTIME', 0, 4),
    ('VC4', 'CR1', 4, 'FLTCLK', 4, 1),
    ('VC4', 'CR1', 4, 'BLANKTIME', 8, 3),
    ('OPA1', 'CAL', 4, 'CALEN', 0, 1),
    ('OPA1', 'CAL', 4, 'START', 2, 1),
    ('OPA1', 'CAL', 4, 'AZRUN', 10, 1),
    ('OPA2', 'CAL', 4, 'CALEN', 0, 1),
    ('OPA2', 'CAL', 4, 'START', 2, 1),
    ('OPA2', 'CAL', 4, 'AZRUN', 10, 1),
    ('DAC', 'CR0', 0, 'EN1', 0, 1),
    ('DAC', 'CR0', 0, 'TEN1', 1, 1),
    ('DAC', 'CR0', 0, 'WAVE1', 6, 2),
    ('DAC', 'CR0', 0, 'DMAEN1', 12, 1),
    ('DAC', 'CR0', 0, 'EN2', 16, 1),
    ('DAC', 'CR0', 0, 'TEN2', 17, 1),
    ('DAC', 'CR0', 0, 'WAVE2', 22, 2),
    ('DAC', 'CR0', 0, 'DMAEN2', 28, 1),
    ('SYSCTRL', 'CR0', 0, 'SYSCLK', 0, 3),
    ('SYSCTRL', 'CR0', 0, 'PCLKPRS', 3, 2),
    ('SYSCTRL', 'CR0', 0, 'HCLKPRS', 5, 3),
    ('SYSCTRL', 'CR0', 0, 'KEY', 16, 16),
    ('SYSCTRL', 'CR1', 4, 'CLKCCS', 8, 1),
    ('SYSCTRL', 'CR2', 8, 'FLASHWAIT', 4, 3),
    ('SYSCTRL', 'AHBEN', 48, 'FLASH', 1, 1),
    ('SYSCTRL', 'HSI', 24, 'TRIM', 0, 11),
    ('SYSCTRL', 'HSI', 24, 'DIV', 11, 4),
    ('SYSCTRL', 'HSI', 24, 'STABLE', 15, 1),
    ('FLASH', 'CR2', 4, 'WAIT', 0, 3),
    ('FLASH', 'CR2', 4, 'KEY', 16, 16),
]

if __name__ == "__main__":
    main()
