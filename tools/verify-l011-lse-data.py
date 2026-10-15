#!/usr/bin/env python3
"""Verify own-L011 historical LSE and exact live-LSI sources/metadata; no HAL execution."""
import argparse
import sys
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
sys.path.insert(0, str(ROOT / "tests"))
from current_targets import retired_targets
PARTS = {"CW32L011K8T6", "CW32L011K8U6"}
PROOF = "docs/l011-lse-sysclk-qualification.json"
DETECTOR = {"lse_edges": 128, "lsi_cycles": 256, "margin_lse_edges": 1}
RM_PAGES = [49, 50, 51, 53, 54, 57, 60, 61, 65, 66, 67, 68, 69, 70, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 85, 102, 113, 122, 123, 126, 127, 128, 129, 130, 139, 140, 144, 151, 152, 153, 154, 182, 183, 200, 201, 202, 281, 282, 283, 325, 351, 353, 375, 376, 379, 380, 422, 423, 424, 425, 531, 539, 540, 541, 548, 549, 550]
DS_PAGES = [28, 29, 30, 31, 39, 48, 50, 51, 71]


def load(path):
    return yaml.load(path.read_text(), Loader=UniqueLoader)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def compact(value):
    return "".join(value.split())


def fields(ir, register):
    return {f["name"]: f for f in ir["fieldset/" + register]["fields"]}


LSI_POLICY = "cw32-data/lsi-sysclk-qualified.yaml"
LIVE_PROOF = "docs/l011-live-factory-lsi-qualification.json"
L011_POLICY_SHA256 = "c17d37ef0a6a2c1eef021b29a969734b3dc76935df8a414c69529e7dd2ded577"
EXPECTED_LSI = {'nominal_hz': 32800, 'minimum_hz': 29520, 'maximum_hz': 41000, 'supply_mv': [1700, 5500], 'temperature_c': [-40, 85], 'factory_trim_address': 1050562, 'rtc_allowed_sources': [0, 1, 2, 3], 'awt_allowed_sources': [0, 1], 'uart_allowed_sources': [0, 1, 2, 3], 'uarts': ['UART1', 'UART2', 'UART3'], 'gpio_banks': ['GPIOA', 'GPIOB', 'GPIOC'], 'gpio_filter_allowed_sources': [0, 1, 2, 3, 5, 7], 'mco_allowed_sources': [0, 1, 2, 3, 4, 5, 6], 'lsi_output_pin': None, 'lsi_output_allowed_af': [], 'rcc_irq': 4}
LSI_OPERATING = {'admission': 'live_factory_reuse_only', 'watchdog_source': 'lsi', 'trim_write': 'forbidden', 'lsien_write': 'forbidden', 'waitcycle_preserved': True, 'all_nonstable_lsi_bits_preserved': True, 'factory_read_width_bits': 16, 'factory_erased_raw_halfword': 65535, 'factory_zero_is_erased': False, 'software_lsien_required': True, 'entry_stable_required': True, 'factory_trim_match_required': True, 'ordinary_por_supported': False, 'cycle_timing_qualified': False, 'config_hse_on_lsi_target': 'refuse_some', 'config_lse_on_lsi_target': 'refuse_some', 'original_hseen_required': False, 'original_hseccs_required': False, 'original_lseccs_required': False, 'entry_sysclk_encodings': [0, 3], 'gpio_inspection': 'whole_bank_functional_handover', 'inspection_scope': 'existing_owner_windows_only', 'selector_inventory_semantics': 'documented_encodings_preserved_by_live_reuse', 'awt_register': 'RTC.CR2.AWTSRC'}
EXPECTED_RTC = {'calendar_divisor': 96000000, 'factory_trim_address': 1050560, 'maximum_hz': 97920000, 'minimum_hz': 94080000, 'nominal_hz': 96000000, 'prescaler_first': 120, 'prescaler_second': 400000, 'source': 'HSIOSC', 'source_encoding': 3, 'supply_mv': [1700, 5500], 'temperature_c': [-40, 85]}
EXPECTED_MASKS = {'LSI_parameters': 4294934527, 'HSI_parameters': 4294934527, 'HSE_parameters': 4294443007, 'LSE_parameters': 4294705151, 'LSI_trim': 1023, 'LSI_waitcycle': 3072, 'RTC_source': 1792, 'RTC_psc1': 267386880, 'RTC_psc2': 1048575, 'RTC_start': 128, 'RTC_cr1_wait_status': 4, 'SYSCTRL_ISR_current_status': 55296, 'SYSCTRL_ISR_ready_events': 27, 'SYSCTRL_ISR_faults': 480, 'SYSCTRL_ISR_HSE_faults': 320, 'SYSCTRL_ISR_LSE_faults': 160, 'SYSCTRL_CR1_low_half_identity': 65535, 'SYSCTRL_CR1_HSEEN': 2, 'SYSCTRL_CR1_detector_guards': 192, 'SYSCTRL_CR2_non_owned_low_half': 65423, 'SYSCTRL_CR2_FLASHWAIT': 112}
EXPECTED_REQUESTERS = {'iwdt': ['IWDT'], 'rtc_awt_calendar_output': ['RTC', 'AWT', 'calendar', 'RTC_OUT'], 'uart': ['UART1', 'UART2', 'UART3'], 'lptim': ['LPTIM'], 'ready_and_output': ['MCO', 'LSIRDY', 'SYSCTRL_IRQ4', 'NVIC_SYSCTRL_PENDING', 'HSIRDY', 'PB0_HSIOSC_OUT'], 'gpio': ['GPIOA', 'GPIOB', 'GPIOC'], 'analog': ['VC1', 'VC2', 'LVD', 'VCDIV'], 'clock_policy': ['HSECCS', 'LSECCS', 'clock_policy', 'CLKFAULT_IRQ31'], 'downstream': ['GTIM1', 'GTIM2', 'ATIM', 'BTIM1', 'BTIM2', 'BTIM3', 'ADC', 'IRMOD', 'external_recipients'], 'bus_domain_consumers': ['CPU', 'I2C', 'SPI', 'CRC', 'RAM', 'FLASH']}
SYSCTRL_PINS = [{"pin": "PB7", "signal": "HSE_OUT"}, {"pin": "PC13", "signal": "HSE_IN"},
                {"pin": "PC14", "signal": "LSE_IN"}, {"pin": "PC15", "signal": "LSE_OUT"}]


class UniqueLoader(yaml.SafeLoader):
    pass


def unique_mapping(loader, node, deep=False):
    mapping = {}
    for key_node, value_node in node.value:
        key = loader.construct_object(key_node, deep=deep)
        assert key not in mapping, f"duplicate policy/source key: {key}"
        mapping[key] = loader.construct_object(value_node, deep=deep)
    return mapping


UniqueLoader.add_constructor(yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG, unique_mapping)


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()


def unique_index(values, key):
    result = {}
    for value in values:
        identity = value[key]
        assert identity not in result, f"duplicate {key}: {identity}"
        result[identity] = value
    return result


def complete_selection_roster():
    """Current input identities, not a historical generated-file receipt."""
    document = load(ROOT / "cw32-data/parts.yaml")
    assert document["schema_version"] == 1
    exact = unique_index(document["parts"], "name")
    selections, included_parts = {}, set()
    for path in sorted((ROOT / "cw32-data/inputs").glob("*.yaml")):
        source = load(path)
        line, entries = source["line"], source["chips"]
        if source.get("parts_catalog"):
            assert source["parts_catalog"] == "cw32-data/parts.yaml"
            entries = entries + [part for part in exact.values() if part["family"] == line]
            included_parts.update(part["name"] for part in exact.values() if part["family"] == line)
        for entry in entries:
            name = entry["name"]
            assert name not in selections, f"duplicate source selection: {name}"
            selections[name] = line
    assert included_parts == set(exact)
    assert len(selections) == 54
    assert {name for name, line in selections.items() if line == "CW32L011"} == PARTS | {"CW32L011"}
    # Historical evidence keeps all 54 identities; only current outputs retire these exact two.
    assert retired_targets() <= set(selections)
    selections = {name: line for name, line in selections.items() if name not in retired_targets()}
    assert len(selections) == 52
    return selections, exact


def verify_live_lsi_policy(source_root, authority):
    policy_bytes = (ROOT / LSI_POLICY).read_bytes()
    electrical = load(ROOT / "cw32-data/electrical.yaml")
    assert electrical["policies"][LSI_POLICY] == sha(policy_bytes)
    policy = load(ROOT / LSI_POLICY)
    assert policy["schema_version"] == 1 and policy["source_authority"] == "sources/evidence-sources.json"
    assert sorted(policy["families"]) == [
        "CW32A030", "CW32F002", "CW32F003", "CW32F020", "CW32F030", "CW32L010", "CW32L011", "CW32L012",
        "CW32L031", "CW32L052", "CW32L083", "CW32R031", "CW32W031",
    ]
    own = policy["families"]["CW32L011"]
    # The full independently reviewed object pin rejects omitted/extra keys,
    # missing explicit nulls and changed types. It supplements source checks.
    assert sha(canonical(own)) == L011_POLICY_SHA256, "unreviewed own-L011 live-LSI policy"
    assert canonical(own["lsi_sysclk"]) == canonical(EXPECTED_LSI)
    assert own["exact_parts"] == [{"name": "CW32L011K8T6", "package": "LQFP32"},
                                  {"name": "CW32L011K8U6", "package": "QFN32"}]
    assert set(unique_index(own["exact_parts"], "name")) == PARTS
    assert own["sysctrl_version"] == "cw32l011_v1"
    for key, value in LSI_OPERATING.items():
        assert canonical(own[key]) == canonical(value), key
    assert all(own[key] is False for key in ["retained_hseen_required", "retained_hseccs_required", "retained_lseccs_required"])
    assert own["vdda_equals_vdd"] is True
    assert own["lsi_parameter_preservation"] == {
        "nonstable_mask": 0xffff7fff, "trim_mask": 0x3ff, "trim_bit_size": 10,
        "waitcycle_mask": 0xc00, "stable_mask": 0x8000, "all_nonstable_bits_including_reserved": True,
        "write_parameters": False, "write_lsien": False, "raw_factory_halfword_read_width": 16,
        "erased_raw_halfword": 65535, "zero_valid": True, "original_admission_not_later_upgrade": True,
    }
    assert own["independent_rtc_source"] == EXPECTED_RTC
    assert load(ROOT / "cw32-data/rtc-calendar.yaml")["profiles"]["CW32L011"] == EXPECTED_RTC
    admission = own["rtc_target_admission"]
    assert admission["hardware_documented_encodings"] == {"0": "LSE", "1": "HSE", "2": "LSI", "3": "HSIOSC"}
    assert admission["target_refused_documented_encodings"] == [1]
    assert admission["target_undocumented_refused_encodings"] == [4, 5, 6, 7]
    assert EXPECTED_RTC["maximum_hz"] > 97 * 1_000_000 and EXPECTED_RTC["maximum_hz"] <= 98 * 1_000_000
    assert EXPECTED_LSI["maximum_hz"] <= 1_000_000
    sources = unique_index(own["sources"], "source_ref")
    assert list(sources) == ["vendor:CW32L011_UserManual_CN_V1.1.pdf", "vendor:CW32L011_DataSheet_CN_V1.1.pdf"]
    originals, page_receipts, texts = [], [], {}
    for source_ref, source in sources.items():
        locked = authority[source_ref]
        assert source["evidence_kind"] == "external-evidence"
        assert locked["provenance"]["status"] == source["provenance_status"] == "selected"
        assert locked["provenance"]["chip_scope"] == ["CW32L011"]
        assert LIVE_PROOF in locked["evidence"] and LSI_POLICY + "#/families/CW32L011" in locked["evidence"]
        original = (source_root / locked["path"]).read_bytes()
        assert sha(original) == source["sha256"] == locked["sha256"]
        assert len(original) == source["bytes"] == locked["bytes"]
        text = (source_root / locked["text"]["path"]).read_bytes()
        assert sha(text) == source["text_sha256"] == locked["text"]["sha256"]
        assert len(text) == source["text_bytes"] == locked["text"]["bytes"]
        pages = text.decode().split("\f")
        texts[source_ref] = pages
        ordered_pages, delta = source["pdf_pages_1_based"], 3 if "DataSheet" in source_ref else 1
        assert ordered_pages == sorted(set(ordered_pages)) and ordered_pages
        assert source["printed_pages"] == [p - delta for p in ordered_pages]
        for page in ordered_pages:
            assert 0 < page <= locked["provenance"]["pdf_page_count"]
            page_receipts.append({"source_ref": source_ref, "pdf_page_1_based": page,
                                  "printed_page": page - delta, "text_sha256": sha(pages[page - 1].encode())})
        originals.append({"source_ref": source_ref, "sha256": locked["sha256"], "text_sha256": locked["text"]["sha256"]})
    assert len(page_receipts) == 119
    rm = texts["vendor:CW32L011_UserManual_CN_V1.1.pdf"]
    ds = texts["vendor:CW32L011_DataSheet_CN_V1.1.pdf"]
    # Current own authority, not a same-filename older manual or another family.
    anchors = {
        52: ["LSI振荡器在启动前必须设置好所有参数，启动后禁止修改相关参数"],
        62: ["SYSCTRL_CR0.SYSCLK为3", "SYSCTRL_LSI.STABLE"],
        65: ["011：设置SysClk的时钟源为LSI"],
        67: ["LSIEN位域不会被置位"],
        70: ["9:0TRIM", "11:10WAITCYCLE", "15STABLERO", "0x001007C2"],
        85: ["0100：LSI", "0011：HSIOSC", "0110：LSE"],
        130: ["101：LSI", "111：LPTIM", "18:16FLTCLK"],
        140: ["fRTCCLKD=fRTCCLK/(PSC1+1)", "小于等于1MHz"],
        152: ["10:8SOURCERW", "000：外部低速时钟LSE", "001：外部高速时钟HSE",
              "010：内部低速时钟LSI", "011：内部高速时钟HSIOSC", "2WAITRO"],
        154: ["27:20PSC1RW", "19:0PSC2RW", "小于等于1MHz"],
        384: ["其计数时钟源为内部低速LSI时钟", "0xCCCC", "0xAAAA"],
        422: ["13:12SOURCERW", "11：UCLK来自LSI", "10：UCLK来自LSE"],
        541: ["0：LSI", "1：PCLK", "4FLTCLKRW"],
        549: ["0：LSI", "1：SYSCLK", "8FLTCLKRW"],
    }
    for page, required in anchors.items():
        assert all(value in compact(rm[page - 1]) for value in required), (page, required)
    assert all(value in compact(ds[7]) for value in ["CW32L011K8T6", "CW32L011K8U6", "FLASH64KB", "SRAM6KB", "LQFP32", "QFN32"])
    assert all(value in compact(ds[50]) for value in ["ACCLSI", "32.8", "-10", "+25", "0.16", "ACCHSI", "-2.0", "+2.0"])
    assert "PB00UART1_RXDUART2_TXDHSIOSC_OUT" in compact(ds[31])
    assert "PC13MCO_OUT" in compact(ds[31])
    assert all(value in compact(ds[71]) for value in ["2025-05-19", "更新表7-19LSI振荡器特性", "69https://www.whxy.com"])
    locator = own["source_locator_correction"]
    assert (locator["old_printed_page"], locator["corrected_printed_page"], locator["pdf_page_1_based"]) == (1, 69, 72)
    revision = authority[locator["source_ref"]]["provenance"]["revision_history_evidence"]
    assert (revision["printed_page"], revision["pdf_page_1_based"]) == (69, 72)
    assert len(own["known_source_conflicts"]) == 2
    assert own["known_source_conflicts"][0]["pdf_pages_1_based"] == [54, 70]
    assert own["known_source_conflicts"][1]["pdf_pages_1_based"] == [68]

    sdk = own["sdk_source"]
    assert sdk["source_ref"] == "vendor:CW32L011_StandardPeripheralLib_V1.0.3.zip"
    locked_sdk = authority[sdk["source_ref"]]
    assert locked_sdk["provenance"]["status"] == "selected" and locked_sdk["provenance"]["chip_scope"] == ["CW32L011"]
    archive_bytes = (source_root / locked_sdk["path"]).read_bytes()
    assert sha(archive_bytes) == sdk["sha256"] == locked_sdk["sha256"]
    assert len(archive_bytes) == sdk["bytes"] == locked_sdk["bytes"]
    for key in ["generator_svd_path", "generator_header_path"]:
        assert sdk[key] == locked_sdk["provenance"][key]
    locked_members, declared = unique_index(locked_sdk["members"], "path"), unique_index(own["sdk_members"], "source_ref")
    assert len(declared) == 19
    sdk_members = {}
    for source_ref, member in declared.items():
        assert source_ref.startswith("member:")
        locked = locked_members[source_ref.removeprefix("member:")]
        assert member["member_chain"] == locked["members"]
        data = archive_bytes
        for archive_member in member["member_chain"]:
            with zipfile.ZipFile(io.BytesIO(data)) as archive:
                assert archive.namelist().count(archive_member) == 1
                data = archive.read(archive_member)
        assert sha(data) == member["sha256"] == locked["sha256"] and len(data) == member["bytes"] == locked["bytes"]
        sdk_members[source_ref] = data
    assert own["native_controls_source_ref"] == "member:" + sdk["generator_svd_path"]
    svd = ET.fromstring(sdk_members[own["native_controls_source_ref"]])
    header = sdk_members["member:" + sdk["generator_header_path"]].decode()
    assert re.search(r"SYSCTRL_IRQn\s*=\s*4\s*,", header)
    assert re.search(r"CLKFAULT_IRQn\s*=\s*31\s*(?:,|(?=/\*))", header)
    assert re.search(r"#define\s+SYSCTRL_BASE\s+0x40004000UL", header)
    sysctrl_header = sdk_members["member:cw32l011/Libraries/inc/cw32l011_sysctrl.h"].decode()
    assert re.search(r"#define\s+SYSCTRL_SYSCLKSRC_LSI\s+\(0x0*3U\)", sysctrl_header)
    assert re.search(r"#define\s+SYSCTRL_LSI_TRIMCODEADDR\s+\(0x001007C2U\)", sysctrl_header)
    gpio_header = compact(sdk_members["member:cw32l011/Libraries/inc/cw32l011_gpio.h"].decode())
    for macro, register, af in [("PA04", "GPIOA->AFRL_f.AFR4", 4), ("PA08", "GPIOA->AFRH_f.AFR8", 4), ("PC13", "GPIOC->AFRH_f.AFR13", 1)]:
        assert f"#define{macro}_AFx_MCOOUT()(CW_{register}={af})" in gpio_header
    native_receipt = verify_live_lsi_native(own, svd)
    package_receipt = verify_live_lsi_packages(own, sdk_members)
    requesters = unique_index(own["requester_observers"], "id")
    assert {key: row["members"] for key, row in requesters.items()} == EXPECTED_REQUESTERS
    members = [member for row in requesters.values() for member in row["members"]]
    assert len(members) == len(set(members))
    rows = list(requesters.values()) + own["operational_access_facts"] + own["known_source_conflicts"] + [own["observer_irq"], own["positive_lsi_routes"]]
    for row in rows:
        source = sources[row["source_ref"]]
        pages = row["pdf_pages_1_based"]
        assert pages == sorted(set(pages)) and pages
        assert set(pages) <= set(source["pdf_pages_1_based"])
        assert row["printed_pages"] == [p - (3 if "DataSheet" in row["source_ref"] else 1) for p in pages]
    return own, {
        "policy_path": LSI_POLICY, "policy_sha256": sha(policy_bytes), "own_policy_sha256": L011_POLICY_SHA256,
        "admission": own["admission"], "originals": originals, "source_pages": page_receipts,
        "sdk_archive_sha256": sdk["sha256"], "sdk_members_verified": own["sdk_members"],
        **native_receipt, **package_receipt, "requester_observer_categories": list(requesters),
        "requester_observer_members": members, "historical_lse_proof_preserved": True,
        "independent_rtc_source": own["independent_rtc_source"], "derived_masks": own["derived_masks"],
    }


def verify_live_lsi_native(own, svd):
    source_peripherals = {}
    for peripheral in svd.findall("./peripherals/peripheral"):
        name = peripheral.findtext("name")
        assert name not in source_peripherals, f"duplicate source peripheral: {name}"
        source_peripherals[name] = peripheral
    assert own["peripheral_inventory"] == sorted(source_peripherals)
    assert len(source_peripherals) == 27 and set(own["native_peripherals"]) == set(source_peripherals)
    assert not {"AWT", "GPIOF", "PLL", "HEX", "OPA", "DAC"} & set(source_peripherals)
    source_input = load(ROOT / "cw32-data/inputs/cw32l011.yaml")
    assert source_input["line"] == source_input["expected_svd_name"] == "CW32L011"
    assert source_input["source"]["source_ref"] == own["native_controls_source_ref"]
    assert source_input["source"]["sha256"] == next(m["sha256"] for m in own["sdk_members"] if m["source_ref"] == own["native_controls_source_ref"])
    native_ir = {}
    for name, binding in own["native_peripherals"].items():
        assert int(source_peripherals[name].findtext("baseAddress"), 0) == binding["address"], name
        assert source_input["register_versions"][binding["kind"]] == binding["version"]
        ir = load(ROOT / "cw32-data/registers" / (binding["kind"] + "_" + binding["version"] + ".yaml"))
        assert "block/" + binding["block"] in ir
        native_ir[name] = ir
    for name in ["GPIOA", "GPIOB", "GPIOC"]:
        assert {key: own["native_peripherals"][name][key] for key in ["kind", "version", "block"]} == {"kind": "gpio", "version": "cw32l011_v1", "block": "GPIO"}

    def source_registers(name):
        seen, peripheral = set(), source_peripherals[name]
        while peripheral.get("derivedFrom"):
            base = peripheral.get("derivedFrom")
            assert base not in seen, "cyclic source peripheral inheritance"
            seen.add(base)
            peripheral = source_peripherals[base]
        registers = {r.findtext("name"): r for r in peripheral.findall("./registers/register")}
        assert len(registers) == len(peripheral.findall("./registers/register"))
        return registers

    access_names = {"read-only": "Read", "write-only": "Write", "read-write": "ReadWrite"}
    controls, field_count = {}, 0
    assert len(own["native_controls"]) == 84
    for control in own["native_controls"]:
        owner, register = control["peripheral"], control["register"]
        key = owner, register
        assert key not in controls, f"duplicate native control: {key}"
        controls[key] = control
        source_register = source_registers(owner)[register]
        binding, ir = own["native_peripherals"][owner], native_ir[owner]
        item = unique_index(ir["block/" + binding["block"]]["items"], "name")[register]
        assert item["byte_offset"] == control["byte_offset"] == int(source_register.findtext("addressOffset"), 0), key
        assert item.get("bit_size", 32) == control["bit_size"] == int(source_register.findtext("size", "32"), 0)
        assert item.get("access", "ReadWrite") == control["access"] == access_names[source_register.findtext("access")], key
        assert item.get("array") == control["array"] is None
        assert item["fieldset"] == control["fieldset"] == register
        ir_fields, declared_fields = unique_index(ir["fieldset/" + item["fieldset"]]["fields"], "name"), unique_index(control["fields"], "field")
        source_fields = {f.findtext("name"): f for f in source_register.findall("./fields/field")}
        assert len(source_fields) == len(source_register.findall("./fields/field"))
        assert set(ir_fields) == set(declared_fields) == set(source_fields), key
        for name, declared in declared_fields.items():
            field, source_field = ir_fields[name], source_fields[name]
            assert (field["bit_offset"], field["bit_size"]) == (declared["bit_offset"], declared["bit_size"]), (key, name)
            assert field.get("array") == declared["array"] is None and field.get("enum") == declared["enum"]
            assert (int(source_field.findtext("lsb"), 0), int(source_field.findtext("msb"), 0)) == (declared["bit_offset"], declared["bit_offset"] + declared["bit_size"] - 1)
            assert source_field.findtext("access", source_register.findtext("access")) == declared["source_access"], (key, name)
            field_count += 1
    assert field_count == 630
    source_controls = {("SYSCTRL", reg) for reg in ["CR0", "CR1", "CR2", "IER", "ISR", "HSI", "HSE", "LSI", "LSE", "MCO", "AHBEN", "APBEN1", "APBEN2", "AHBRST", "APBRST1", "APBRST2"]}
    source_controls |= {("FLASH", "CR2"), ("RTC", "CR1"), ("RTC", "PSC"), ("ADC", "CR"), ("VC1", "CR0"), ("VC1", "CR1"), ("VC2", "CR0"), ("VC2", "CR1"), ("LVD", "CR0"), ("LVD", "CR1")}
    for key in source_controls:
        assert controls[key]["access"] in {"Read", "ReadWrite"}, key
    sysctrl_ir = native_ir["SYSCTRL"]
    assert {k.removeprefix("enum/"): v for k, v in sysctrl_ir.items() if k.startswith("enum/")} == own["native_enums"]
    assert {v["name"]: v["value"] for v in own["native_enums"]["Sysclk"]["variants"]} == {"HSI": 0, "HSE": 1, "LSI": 3, "LSE": 4}
    assert not {"PLL", "HEX"} & set(unique_index(sysctrl_ir["block/SYSCTRL"]["items"], "name"))
    overlays = load(ROOT / "cw32-data/field-access.yaml")["registers"]["sysctrl_cw32l011_v1"]
    assert [{key: value for key, value in row.items() if key != "evidence"} for row in overlays] == own["source_status_overlays"]
    assert len(overlays) == 4
    for row in overlays:
        field = unique_index(controls["SYSCTRL", row["register"]]["fields"], "field")[row["field"]]
        assert field["source_access"] == "read-only"
        assert (row["bit_offset"], row["bit_size"]) == (field["bit_offset"], field["bit_size"])
    assert [(i.findtext("name"), int(i.findtext("value"))) for i in source_peripherals["SYSCTRL"].findall("interrupt")] == [("SYSCTRL", 4), ("CLKFAULT", 31)]
    assert (own["observer_irq"]["name"], own["observer_irq"]["number"]) == ("SYSCTRL", 4)
    assert own["observer_irq"]["distinct_fault_irq"] == {"name": "CLKFAULT", "number": 31}
    for access in own["operational_access_facts"]:
        for register in access["registers"]:
            if "field" in access:
                field = unique_index(controls[access["peripheral"], register]["fields"], "field")[access["field"]]
                assert field["source_access"] == access["raw_svd_access"]
            else:
                assert (access["peripheral"], register, access["operational_rm_access"]) == ("SYSCTRL", "ICR", "R1W0")
                assert controls["SYSCTRL", "ICR"]["access"] == access["raw_register_access"] == "ReadWrite"

    def mask(owner, register, name):
        f = unique_index(controls[owner, register]["fields"], "field")[name]
        return ((1 << f["bit_size"]) - 1) << f["bit_offset"]

    masks = {name + "_parameters": 0xffffffff ^ mask("SYSCTRL", name, "STABLE") for name in ["LSI", "HSI", "HSE", "LSE"]}
    masks.update({key: mask(owner, register, field) for key, owner, register, field in [
        ("LSI_trim", "SYSCTRL", "LSI", "TRIM"), ("LSI_waitcycle", "SYSCTRL", "LSI", "WAITCYCLE"),
        ("RTC_source", "RTC", "CR1", "SOURCE"), ("RTC_psc1", "RTC", "PSC", "PSC1"),
        ("RTC_psc2", "RTC", "PSC", "PSC2"), ("RTC_start", "RTC", "CR0", "START"),
        ("RTC_cr1_wait_status", "RTC", "CR1", "WAIT"), ("SYSCTRL_CR1_HSEEN", "SYSCTRL", "CR1", "HSEEN"),
        ("SYSCTRL_CR2_FLASHWAIT", "SYSCTRL", "CR2", "FLASHWAIT"),
    ]})
    for key, names in [("current_status", ["HSISTABLE", "HSESTABLE", "LSISTABLE", "LSESTABLE"]),
                       ("ready_events", ["HSIRDY", "HSERDY", "LSIRDY", "LSERDY"]),
                       ("faults", ["HSEFAIL", "HSEFAULT", "LSEFAIL", "LSEFAULT"]),
                       ("HSE_faults", ["HSEFAIL", "HSEFAULT"]), ("LSE_faults", ["LSEFAIL", "LSEFAULT"])]:
        masks["SYSCTRL_ISR_" + key] = sum(mask("SYSCTRL", "ISR", name) for name in names)
    masks["SYSCTRL_CR1_low_half_identity"] = 0xffffffff ^ mask("SYSCTRL", "CR1", "KEY")
    masks["SYSCTRL_CR1_detector_guards"] = mask("SYSCTRL", "CR1", "HSECCS") | mask("SYSCTRL", "CR1", "LSECCS")
    masks["SYSCTRL_CR2_non_owned_low_half"] = 0xffffffff ^ (mask("SYSCTRL", "CR2", "KEY") | masks["SYSCTRL_CR2_FLASHWAIT"])
    assert masks == own["derived_masks"] == EXPECTED_MASKS

    gates = unique_index(own["consumer_gates"], "peripheral")
    assert len(gates) == 22
    clock = load(ROOT / "cw32-data/clock/cw32l011.yaml")
    assert clock["profile"] == "CW32L011" and clock["controller"] == "SYSCTRL"
    clock_peripherals = unique_index(clock["peripherals"], "name")
    assert set(clock_peripherals) == set(source_peripherals)
    assert set(gates) == {name for name, p in clock_peripherals.items() if p.get("enable") is not None and p.get("reset") is not None}
    for name, gate in gates.items():
        native = clock_peripherals[name]
        for key, relation, offset_key in [("gate", "enable", "gate_byte_offset"), ("reset", "reset", "reset_byte_offset")]:
            register, field_name = gate[key + "_register"], gate[key + "_field"]
            assert native[relation] == {"peripheral": "SYSCTRL", "register": register, "field": field_name}, name
            control = controls["SYSCTRL", register]
            assert control["byte_offset"] == gate[offset_key]
            field = unique_index(control["fields"], "field")[field_name]
            assert (field["bit_offset"], field["bit_size"], field["source_access"]) == (gate["bit_offset"], 1, "read-write")
        key_field = unique_index(controls["SYSCTRL", gate["gate_register"]]["fields"], "field")["KEY"]
        assert (key_field["bit_offset"], key_field["bit_size"], key_field["source_access"]) == (16, 16, "write-only")
        assert "KEY" not in unique_index(controls["SYSCTRL", gate["reset_register"]]["fields"], "field")
        assert gate["enable_active_value"] is True and native["enable_active_value"] == 1
        assert gate["reset_asserted_value"] is False and native["reset_asserted_value"] == 0
        assert gate["key_value"] == native["enable_write_key"]["value"] == 0x5a5a
        assert native["enable_write_key"]["field"] == {"peripheral": "SYSCTRL", "register": gate["gate_register"], "field": "KEY"}
        for key in ["bus_clock", "shared_enable_group", "shared_reset_group"]:
            assert gate[key] == native.get(key), (name, key)
    assert clock_peripherals["IWDT"]["kernel_clock"] == {"clock": "LSI"}
    assert "LVD" not in gates and gates["VC1"]["gate_field"] == gates["VC2"]["gate_field"] == "VC"
    routes = own["positive_lsi_routes"]
    assert routes["dedicated_lsi_gpio_af"] is None and routes["exact_sysctrl_pin_projection"] == []
    assert routes["mco_source"] == 4
    assert routes["mco_pads"] == [{"pin": "PA4", "position": "10", "af": 4}, {"pin": "PA8", "position": "18", "af": 4}, {"pin": "PC13", "position": "31", "af": 1}]
    assert routes["raw_hsi_pad"] == {"pin": "PB0", "position": "14", "af": 3}
    assert [(row["peripheral"], row["register"], row["fields"], row["encoding"]) for row in routes["timer_inputs"]] == [
        ("GTIM1", "TISEL", ["TI1SEL", "TI2SEL", "TI3SEL", "TI4SEL"], 9),
        ("GTIM2", "TISEL", ["TI1SEL", "TI2SEL", "TI3SEL", "TI4SEL"], 9),
        ("ATIM", "TISEL1", ["TI1SEL", "TI2SEL", "TI3SEL", "TI4SEL"], 9),
        ("ATIM", "TISEL2", ["TI5SEL", "TI6SEL"], 9),
    ]
    for route in routes["timer_inputs"]:
        fields = unique_index(controls[route["peripheral"], route["register"]]["fields"], "field")
        assert all(fields[name]["bit_size"] == 4 for name in route["fields"])
    return {"native_peripherals_verified": len(native_ir), "native_registers_verified": len(controls),
            "native_fields_verified": field_count, "consumer_gates_verified": len(gates),
            "source_status_overlays_verified": len(overlays), "observer_irq_verified": own["observer_irq"],
            "raw_svd_and_curated_ir_access_checked": True, "positive_lsi_routes_verified": routes}


def verify_live_lsi_packages(own, sdk_members):
    document = load(ROOT / "cw32-data/parts.yaml")
    parts = unique_index(document["parts"], "name")
    pin_document = load(ROOT / "cw32-data/pinouts/cw32l011.yaml")
    pinouts = unique_index(pin_document["packages"], "name")
    assert pin_document["family"] == "CW32L011" and set(pinouts) == set(own["packages"]) == PARTS
    source = document["sources"]["CW32L011_datasheet"]
    assert pin_document["source"]["source_ref"] == source["source_ref"] == "vendor:CW32L011_DataSheet_CN_V1.1.pdf"
    assert pin_document["source"]["sha256"] == source["sha256"] == own["sources"][1]["sha256"]
    pdsc_source = document["sources"]["CW32L011_dfp"]
    pdsc_bytes = sdk_members[pdsc_source["source_ref"]]
    assert sha(pdsc_bytes) == pdsc_source["sha256"] and pdsc_source["archive_sha256"] == own["sdk_source"]["sha256"]
    pdsc = ET.fromstring(pdsc_bytes)
    devices = {device.get("Dname"): device for device in pdsc.findall(".//device")}
    assert len(devices) == len(pdsc.findall(".//device")) and set(devices) == {"CW32L011K8"}
    memory = [{"name": "FLASH", "kind": "flash", "address": 0, "size": 65536}, {"name": "RAM", "kind": "ram", "address": 0x20000000, "size": 6144}]
    for part in sorted(PARTS):
        declared, actual, pinout = own["packages"][part], parts[part], pinouts[part]
        assert actual["family"] == "CW32L011" and actual["feature"] == part.lower()
        assert actual["memory_status"] == "verified-from-official-datasheet"
        assert declared["memory"] == actual["memory"] == memory
        assert declared["package"] == actual["package"] == pinout["package"]
        for key in ["pins", "gpio_pins", "input_only_pins"]:
            assert declared[key] == pinout[key], (part, key)
        for key in ["source_ids", "dfp_device"]:
            assert declared[key] == actual[key], (part, key)
        assert pinout["flash_bytes"] == 65536 and pinout["ram_bytes"] == 6144
        assert pinout["documented_orderable_parts"] == [part]
        physical = unique_index(declared["pins"], "position")
        table = {}
        for row in pin_document["table_rows"]:
            position = row["positions"][pinout["table_column"]]
            if position is not None:
                assert position not in table
                table[position] = row["signals"]
        assert {position: row["signals"] for position, row in physical.items()} == table
        assert set(physical) == {str(index) for index in range(1, 33)}
        gpios = {signal for row in physical.values() for signal in row["signals"] if re.fullmatch(r"P[A-Z][0-9]+", signal)}
        assert len(gpios) == 26 and gpios == set(declared["gpio_pins"]) and declared["input_only_pins"] == []
        assert declared["dfp_device"] == "CW32L011K8" and declared["source_ids"] == ["CW32L011_datasheet", "CW32L011_dfp"]
        source_memory = [(m.get("id"), int(m.get("start"), 0), int(m.get("size"), 0)) for m in devices[declared["dfp_device"]].findall("memory")]
        assert source_memory == [("IROM1", 0, 65536), ("IRAM1", 0x20000000, 6144)]
        assert declared["direct_lsi_bonds"] == []
        for route in own["positive_lsi_routes"]["mco_pads"] + [own["positive_lsi_routes"]["raw_hsi_pad"]]:
            assert route["pin"] in physical[route["position"]]["signals"]
    return {"packages_verified": sorted(PARTS), "direct_lsi_output_absent": True}


def verify_live_lsi_projection(chip, own):
    # Reuse existing independent current-sidecar projections. No historical
    # tree/hash or generated snapshot is a recurring validation dependency.
    import sys
    sys.path.insert(0, str(ROOT / "tests"))
    from reviewed_metadata import pin_projection, af_projection
    manifest = load(ROOT / "cw32-data/inputs/cw32l011.yaml")
    packages, pins = pin_projection(chip, manifest)
    assert chip["packages"] == packages and chip["cores"][0]["pins"] == pins
    expected_pins = af_projection(manifest, pins)
    assert "SYSCTRL" not in expected_pins
    expected_pins["SYSCTRL"] = SYSCTRL_PINS
    peripherals = unique_index(chip["cores"][0]["peripherals"], "name")
    assert set(peripherals) == set(own["native_peripherals"])
    for name, expected in own["native_peripherals"].items():
        actual = peripherals[name]
        assert actual["address"] == expected["address"]
        assert actual["registers"] == {key: expected[key] for key in ["kind", "version", "block"]}
        assert actual.get("pins", []) == expected_pins.get(name, []), (chip["name"], name, "current Pin projection")
        assert actual.get("rtc_calendar") == (EXPECTED_RTC if name == "RTC" else None)
    gates = unique_index(own["consumer_gates"], "peripheral")
    assert {name for name, p in peripherals.items() if p.get("rcc_control") is not None} == set(gates)
    for name, gate in gates.items():
        expected = {"controller": "SYSCTRL", "bus_clock": gate["bus_clock"],
                    "enable": {"register": gate["gate_register"], "field": gate["gate_field"]},
                    "enable_active_value": True,
                    "enable_write_key": {"field": {"register": gate["gate_register"], "field": "KEY"}, "value": 0x5a5a},
                    "reset": {"register": gate["reset_register"], "field": gate["reset_field"]}, "reset_asserted_value": False}
        for key in ["shared_enable_group", "shared_reset_group"]:
            if gate[key] is not None:
                expected[key] = gate[key]
        assert peripherals[name]["rcc_control"] == expected, (chip["name"], name)
    assert unique_index(chip["cores"][0]["interrupts"], "name")["SYSCTRL"]["number"] == 4
    assert unique_index(chip["cores"][0]["interrupts"], "name")["CLKFAULT"]["number"] == 31
    assert chip["cores"][0]["dma_channels"] == []
    assert all(p.get("clock_limits") is None for name, p in peripherals.items() if name != "SYSCTRL")
    assert peripherals["SYSCTRL"]["clock_limits"].get("pll") is None
    assert peripherals["SYSCTRL"]["clock_limits"].get("hex") is None
    if chip["name"] in PARTS:
        package = own["packages"][chip["name"]]
        assert chip["memory"] == [package["memory"]]
        assert packages == [{"name": chip["name"], "package": package["package"], "pins": package["pins"]}]
    else:
        assert chip["name"] == "CW32L011" and chip["memory"] == [] and packages == []


def verify_live_lsi_proof(own, receipt, source_root, authority):
    proof = load(ROOT / LIVE_PROOF)
    assert proof["schema_version"] == 1 and proof["parts"] == sorted(PARTS)
    assert proof["qualification_policy"] == {"path": LSI_POLICY, "family": "CW32L011", "canonical_family_sha256": L011_POLICY_SHA256}
    assert proof["projected_lsi_sysclk"] == EXPECTED_LSI
    admission = {key: own[key] for key in LSI_OPERATING}
    admission.update({key: own[key] for key in ["retained_hseen_required", "retained_hseccs_required", "retained_lseccs_required", "vdda_equals_vdd"]})
    assert proof["admission"] == admission
    for key in ["lsi_parameter_preservation", "derived_masks", "independent_rtc_source", "rtc_target_admission",
                "historical_lse_monitor", "own_fallback_source", "requester_observers", "observer_irq",
                "positive_lsi_routes", "operational_access_facts", "known_source_conflicts", "sources", "sdk_source", "sdk_members", "source_locator_correction"]:
        assert proof[key] == own[key], key
    assert proof["package_binding"] == own["packages"]
    binding = proof["native_binding"]
    for key in ["peripheral_inventory", "native_peripherals", "native_controls_source_ref", "consumer_gates", "source_status_overlays", "native_enums"]:
        assert binding[key] == own[key], key
    assert binding["native_control_count"] == len(own["native_controls"]) == 84
    assert binding["native_field_count"] == sum(len(row["fields"]) for row in own["native_controls"]) == 630
    assert binding["consumer_gate_count"] == len(own["consumer_gates"]) == 22
    assert proof["source_page_receipts"] == receipt["source_pages"]
    # Corroboration is inside the same locked SDK, not a new authority repin.
    archive = (source_root / authority[own["sdk_source"]["source_ref"]]["path"]).read_bytes()
    members = proof["archive_member_corroboration"]
    assert [row["archive_members"] for row in members] == [["Libraries/src/system_cw32l011.c"], ["Examples/RCC/rcc_hsi_lsi/USER/src/main.c"]]
    for row in members:
        assert row["archive_source_ref"] == own["sdk_source"]["source_ref"]
        assert row["archive_sha256"] == own["sdk_source"]["sha256"]
        data = archive
        for name in row["archive_members"]:
            with zipfile.ZipFile(io.BytesIO(data)) as zipped:
                assert zipped.namelist().count(name) == 1
                data = zipped.read(name)
        assert sha(data) == row["sha256"] and len(data) == row["bytes"]
    # Pending implementation/build status is not source or hardware acceptance.
    return {"path": LIVE_PROOF, "sha256": sha((ROOT / LIVE_PROOF).read_bytes()), "status": proof["status"]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sources", type=Path, required=True,
                        help="Directory holding locally acquired, locked vendor originals")
    parser.add_argument("--data", type=Path, help="Complete generated data directory")
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    authority = unique_index(load(ROOT / "sources/evidence-sources.json")["artifacts"], "id")
    catalog = load(ROOT / "cw32-data/lse-qualified.yaml")
    proof = load(ROOT / PROOF)
    auxiliary = load(ROOT / "docs/lse-l011-qualification.json")
    rtc = load(ROOT / "docs/lse-l011-rtc-admission.json")
    assert proof["schema_version"] == 1 and len(proof["parts"]) == len(PARTS)
    assert set(proof["parts"]) == set(auxiliary["parts"]) == PARTS
    assert proof["sysclk_selector"] == 4 and proof["sysclk_detector"] == DETECTOR
    assert proof["lsi_sysclk_qualification"] is False
    assert proof["lsi_cycle_timing_qualified"] is False
    assert {p for p, v in catalog["parts"].items()
            if v["family"] == "CW32L011" and "sysclk_detector" in v["configuration"]} == PARTS
    assert "CW32L011" not in catalog["parts"]
    for path, digest in catalog["policies"].items():
        assert sha((ROOT / path).read_bytes()) == digest, path
    assert [(s["source_ref"], s["pdf_pages_1_based"]) for s in proof["sources"]] == [
        ("vendor:CW32L011_UserManual_CN_V1.1.pdf", RM_PAGES),
        ("vendor:CW32L011_DataSheet_CN_V1.1.pdf", DS_PAGES),
    ]
    original_receipts, page_receipts, texts = [], [], {}
    for source in proof["sources"]:
        locked = authority[source["source_ref"]]
        original = args.sources / locked["path"]
        assert sha(original.read_bytes()) == locked["sha256"] == source["sha256"]
        assert original.stat().st_size == locked["bytes"]
        assert source["evidence_kind"] == "external-evidence" and source["family"] == "CW32L011"
        assert PROOF in locked["evidence"]
        assert locked["provenance"]["status"] == "selected"
        assert locked["provenance"]["chip_scope"] == ["CW32L011"]
        text_bytes = (args.sources / locked["text"]["path"]).read_bytes()
        assert sha(text_bytes) == locked["text"]["sha256"]
        assert len(text_bytes) == locked["text"]["bytes"]
        pages = text_bytes.decode().split("\f")
        texts[source["source_ref"]] = pages
        assert source["printed_pages"] == [p - (3 if "DataSheet" in source["source_ref"] else 1) for p in source["pdf_pages_1_based"]]
        for page in source["pdf_pages_1_based"]:
            assert 0 < page <= locked["provenance"]["pdf_page_count"]
            page_receipts.append({"source_ref": source["source_ref"], "pdf_page_1_based": page,
                                  "text_sha256": sha(pages[page - 1].encode())})
        original_receipts.append({"evidence_kind": "external-evidence", "source_ref": source["source_ref"],
                                  "path": "sources/vendor/" + locked["path"], "sha256": locked["sha256"],
                                  "bytes": locked["bytes"], "url": locked["url"]})
    manual = texts["vendor:CW32L011_UserManual_CN_V1.1.pdf"]
    datasheet = texts["vendor:CW32L011_DataSheet_CN_V1.1.pdf"]
    # Exact own-original semantic anchors; no HAL text or historical IR hashes.
    assert "SYSCTRL_CR0.SYSCLK为4" in compact(manual[60])
    assert "100：设置SysClk的时钟源为LSE" in compact(manual[64])
    assert "256个LSI时钟周期" in compact(manual[56]) and "时钟个数为128" in compact(manual[56])
    assert "LSE运行中失效时系统时钟源会自动切换到HSI4MHz时钟" in compact(manual[56])
    assert "1：自动将SysClk的时钟源切换为HSI4MHz" in compact(manual[65])
    assert "FLASH_CR2[2:0]功能相同" in compact(manual[67])
    assert "SYSCTRL_CR2[6:4]功能相同" in compact(manual[112])
    assert "必须先使能FLASH配置时钟" in compact(manual[101])
    assert all(value in compact(manual[68]) for value in ["Resetvalue:0x--------", "0x001007C0", "1110：HSI=HSIOSC/24", "0000：HSI=HSIOSC/32"])
    assert all(value in compact(manual[69]) for value in ["Resetvalue:0x--------", "0x001007C2", "9:0TRIM", "11:10WAITCYCLE", "0.4%", "30kHz~36kHz"])
    assert "32.8kHz±10%" in compact(manual[53])
    assert all(value in compact(datasheet[38]) for value in ["fHCLK", "fPCLK", "1.7V≤VDD＜1.8V", "24", "96"])
    assert all(value in compact(datasheet[50]) for value in ["ACCHSI", "96", "-2.0", "+2.0", "-40℃~+85℃", "ACCLSI", "-10", "+25", "0.16"])
    assert all(value in compact(datasheet[47]) for value in ["fLSE_EXT", "100", "0.7VDDIOx", "0.3VDDIOx", "450", "50"])
    assert "1.50" in datasheet[49]
    assert "HSIOSC_OUT" in manual[122] and "PB0" in manual[122]
    assert "UART3" in manual[77] and "UART3" in manual[80]
    assert "11：UCLK来自LSI" in compact(manual[421])
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
    header = next(data for name, data in sdk_members.items() if name.endswith("cw32l011_sysctrl.h")).decode()
    for name, value in [("HSI", 0), ("HSE", 1), ("LSI", 3), ("LSE", 4)]:
        match = re.search(r"#define\s+SYSCTRL_SYSCLKSRC_" + name + r"\s+\(0x([0-9A-Fa-f]+)U\)", header)
        assert match and int(match.group(1), 16) == value
    assert re.search(r"#define\s+SYSCTRL_HSIOSC_DIV24\s+\(\(uint32_t\)\(14UL << SYSCTRL_HSI_DIV_Pos\)\)", header)
    assert re.search(r"#define\s+SYSCTRL_HSIOSC_DIV32\s+\(\(uint32_t\)\(0UL\)\)", header)
    assert re.search(r"#define\s+SYSCTRL_LSI_TRIMCODEADDR\s+\(0x001007C2U\)", header)
    startup = next(data for name, data in sdk_members.items() if name.endswith("system_cw32l011.c")).decode()
    assert "CW_SYSCTRL->HSI_f.TRIM = *((volatile uint16_t *)SYSCTRL_HSI_TRIMCODEADDR);" in startup
    assert "CW_SYSCTRL->LSI_f.TRIM = *((volatile uint16_t *)SYSCTRL_LSI_TRIMCODEADDR);" in startup
    assert all(value in startup for value in ["0x7FF", "0x780", "0x3FF", "0x007F"])
    svd = ET.fromstring(next(data for name, data in sdk_members.items() if name.endswith("/CW32L011.svd")))
    sysctrl = load(ROOT / "cw32-data/registers/sysctrl_cw32l011_v1.yaml")
    flash = load(ROOT / "cw32-data/registers/flash_cw32l011_v1.yaml")
    uart = load(ROOT / "cw32-data/registers/uart_cw32l010_v1.yaml")
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
        ("SYSCTRL", sysctrl, "APBEN1", 56, "UART3", 8, 1),
        ("SYSCTRL", sysctrl, "APBRST1", 72, "UART3", 8, 1),
        ("UART3", uart, "CR1", 0, "SOURCE", 12, 2),
    ]
    for owner, ir, reg, offset, name, bit, width in own_fields:
        assert next(r for r in ir["block/" + ("UART" if owner == "UART3" else owner)]["items"] if r["name"] == reg)["byte_offset"] == offset
        field = fields(ir, reg)[name]
        assert (field["bit_offset"], field["bit_size"]) == (bit, width)
        peripheral = next(p for p in svd.findall("./peripherals/peripheral") if p.findtext("name") == owner)
        if "derivedFrom" in peripheral.attrib:
            peripheral = next(p for p in svd.findall("./peripherals/peripheral") if p.findtext("name") == peripheral.attrib["derivedFrom"])
        register = next(r for r in peripheral.findall("./registers/register") if r.findtext("name") == reg)
        assert int(register.findtext("addressOffset"), 0) == offset
        source_field = next(f for f in register.findall("./fields/field") if f.findtext("name") == name)
        assert (int(source_field.findtext("lsb"), 0), int(source_field.findtext("msb"), 0)) == (bit, bit + width - 1)
    uart_source = fields(uart, "CR1")["SOURCE"]["enum"]
    assert {v["name"]: v["value"] for v in uart["enum/" + uart_source]["variants"]}["LSI"] == 3
    assert set(fields(sysctrl, "HSI")) == {"TRIM", "DIV", "STABLE"}
    assert {v["name"]: v["value"] for v in sysctrl["enum/Sysclk"]["variants"]} == {"HSI": 0, "HSE": 1, "LSI": 3, "LSE": 4}
    assert fields(sysctrl, "CR0")["SYSCLK"]["enum"] == "Sysclk"
    assert proof["flash_fields"] == {"sysctrl_cr2_flashwait": [8, 4, 3], "flash_cr2_wait": [4, 0, 3],
                                     "sysctrl_ahben_flash": [48, 1, 1], "key": 0x5a5a,
                                     "same_function_documented": True, "alias_timing_measured": False}
    limits = load(ROOT / "cw32-data/electrical.yaml")["profiles"]["CW32L011"]["clock_limits"]
    assert (limits["hsi_frequency_hz"], limits["hsi_error_percent"]) == (96000000, 2)
    assert limits["factory_hsi_trim_address"] == 0x1007c0
    assert "lsi_sysclk" not in limits and "pll" not in limits
    expected_electrical = {"supply_mv": limits["hsi_supply_mv"], "temperature_c": limits["hsi_temperature_c"]}
    for key in ["low_voltage_threshold_mv", "low_voltage_bus_max_hz", "high_voltage_bus_max_hz", "flash_wait_step_hz", "initial_flash_wait"]:
        expected_electrical[key] = limits[key]
    assert proof["electrical_limits"] == expected_electrical == {
        "supply_mv": [1700, 5500], "temperature_c": [-40, 85], "low_voltage_threshold_mv": 1800,
        "low_voltage_bus_max_hz": 24000000, "high_voltage_bus_max_hz": 96000000,
        "flash_wait_step_hz": 24000000, "initial_flash_wait": 3}
    assert proof["hsi_division"] == {"default_numeric_divisor": 24, "default_encoding": 14, "encoding_zero_numeric_divisor": 32}
    assert limits["default_hsi_divisor"] == 24
    assert proof["uart3_fields"] == {"configuration_gate": ["APBEN1", 56, 8, 1], "reset": ["APBRST1", 72, 8, 1], "source": ["CR1", 0, 12, 2], "lsi_source_encoding": 3}
    fallback = proof["fallback"]
    assert fallback["metadata_source"] == "clock_limits.hse.fixed_ccs_hsi_divisor"
    divisor = limits["hse"]["fixed_ccs_hsi_divisor"]
    assert divisor == fallback["fixed_ccs_hsi_divisor"] == 24
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
    pins = {p["name"]: p for p in load(ROOT / "cw32-data/pinouts/cw32l011.yaml")["packages"]}
    for part in sorted(PARTS):
        current = catalog["parts"][part]
        config = copy.deepcopy(current["configuration"])
        assert config.pop("sysclk_detector") == DETECTOR
        native = config["native_low_power"]
        assert native["monitor_reference"] == "factory_trim"
        assert native["lsi_factory_trim_address"] == 0x1007c2
        assert native["monitored_lsi_maximum_hz"] == 41000
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
        assert monitor["trim_bit_size"] == 10
        assert 20660 * 256 <= 129 * 41000 < 20661 * 256
        # Remove only the newly qualified SYSCLK detector; retain native facts.
        assert config == auxiliary["configuration"]
        assert {k: v for k, v in current.items() if k != "configuration"} == auxiliary["parts"][part]
        assert config["rtc_reset"] == rtc["rtc_reset"]
        assert pins[part]["package"] == current["package"]
        for pin, position in [("PC14", auxiliary["package_pins"][part]["input_position"]),
                              ("PC15", auxiliary["package_pins"][part]["output_position"])]:
            assert any(p["position"] == position and pin in p["signals"] for p in pins[part]["pins"])
    # Separate present-day live capability; historical LSE proof scope stays
    # false and its monitor/fallback/electrical facts remain independently bound.
    own_lsi, live_lsi_receipt = verify_live_lsi_policy(args.sources, authority)
    live_lsi_receipt["qualification_proof"] = verify_live_lsi_proof(own_lsi, live_lsi_receipt, args.sources, authority)
    historical = own_lsi["historical_lse_monitor"]
    assert historical["historical_proof"] == PROOF
    assert historical["historical_lsi_sysclk_qualification"] is proof["lsi_sysclk_qualification"] is False
    assert historical["historical_lsi_cycle_timing_qualified"] is proof["lsi_cycle_timing_qualified"] is False
    for part in PARTS:
        native = catalog["parts"][part]["configuration"]["native_low_power"]
        for key in ["monitor_reference", "lsi_factory_trim_address", "monitored_lsi_maximum_hz", "detector_lse_edges", "detector_lsi_cycles", "detector_margin_lse_edges"]:
            assert historical[key] == native[key]
    fallback_source = own_lsi["own_fallback_source"]
    assert (fallback_source["raw_hsi_hz"], fallback_source["factory_error_percent"], fallback_source["divisor"]) == (limits["hsi_frequency_hz"], limits["hsi_error_percent"], divisor)
    assert (fallback_source["nominal_hz"], fallback_source["minimum_hz"], fallback_source["maximum_hz"]) == (4000000, 3920000, 4080000)
    assert (fallback_source["default_hsi_divisor"], fallback_source["default_hsi_divider_encoding"], fallback_source["encoding_zero_divisor"]) == (24, 14, 32)
    assert fallback_source["new_lsi_monitor_qualification"] is False
    selections, exact_parts = complete_selection_roster()
    generated, generated_parts, generated_names = 0, set(), set()
    lsi_owners, generic_l011_seen = 0, False
    if args.data:
        paths = sorted((args.data / "chips").glob("*.json"))
        assert len(paths) == len(selections) == 52, "--data must contain all 52 active generated selections"
        for path in paths:
            chip = load(path)
            name = chip["name"]
            assert name not in generated_names, f"duplicate generated selection: {name}"
            generated_names.add(name)
            assert name in selections and chip["line"] == selections[name], name
            assert path.stem == name, f"generated filename/identity mismatch: {path.name}"
            assert len(chip["cores"]) == 1, name
            peripherals = unique_index(chip["cores"][0]["peripherals"], "name")
            owners = [p for p in peripherals.values() if p.get("clock_limits") is not None]
            assert len(owners) == 1 and owners[0]["name"] == "SYSCTRL", name
            owner, clock = owners[0], owners[0]["clock_limits"]
            config = clock.get("lse_configuration")
            expected = catalog["parts"].get(name, {}).get("configuration")
            assert config == expected, name
            if chip["line"] == "CW32L011":
                assert bool(config and config.get("sysclk_detector")) == (name in PARTS)
                # Full equality including explicit null/empty output fields;
                # generic absence is outside the exact-part branch.
                assert canonical(clock.get("lsi_sysclk")) == canonical(EXPECTED_LSI if name in PARTS else None), name
                verify_live_lsi_projection(chip, own_lsi)
                if name in PARTS:
                    generated_parts.add(name)
                    lsi_owners += 1
                    assert chip["memory"] == [exact_parts[name]["memory"]]
                    for key in ["hsi_frequency_hz", "hsi_error_percent", "factory_hsi_trim_address", "default_hsi_divisor", "hsi_supply_mv", "hsi_temperature_c",
                                "low_voltage_threshold_mv", "low_voltage_bus_max_hz", "high_voltage_bus_max_hz", "flash_wait_step_hz", "initial_flash_wait"]:
                        assert clock[key] == limits[key]
                    assert clock["hse"]["fixed_ccs_hsi_divisor"] == divisor
                else:
                    assert name == "CW32L011"
                    generic_l011_seen = True
            generated += 1
        assert generated_names == set(selections), "complete generated selection identities are required"
        assert generated_parts == PARTS and lsi_owners == len(PARTS) == 2
        assert generic_l011_seen, "generic L011 exclusion must be checked"
    result = {"status": "passed", "scope": "own-L011 historical LSE and exact live factory-LSI source/metadata qualification",
              "qualified_parts": sorted(PARTS), "originals": original_receipts,
              "sdk_archive": {"evidence_kind": "external-evidence", "source_ref": sdk["source_ref"], "sha256": sdk["sha256"]},
              "sdk_members_verified": sdk["members"], "source_pages": page_receipts,
              "native_fields_verified": len(own_fields), "auxiliary_projection_preserved": True,
              "catalog_auxiliary_profiles": len(catalog["parts"]),
              "catalog_sysclk_profiles": sum("sysclk_detector" in p["configuration"] for p in catalog["parts"].values()),
              "generated_selections_checked": generated, "generated_exact_parts_checked": sorted(generated_parts),
              "live_factory_lsi": {**live_lsi_receipt, "expected_projection": EXPECTED_LSI,
                                   "generated_exact_parts_checked": sorted(generated_parts),
                                   "generated_clock_owners_checked": lsi_owners,
                                   "generic_l011_exclusion_checked": generic_l011_seen,
                                   "complete_selection_roster": sorted(selections),
                                   "generated_complete_roster_checked": bool(args.data)},
              "hal_execution": False, "hardware_execution": False}
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, indent=2) + "\n")
    print(f"PASS: own L011 historical LSE and exact-two live factory-LSI sources/profiles; {generated} generated selections; no HAL execution")


if __name__ == "__main__":
    main()
