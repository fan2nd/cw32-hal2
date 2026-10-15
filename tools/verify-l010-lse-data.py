#!/usr/bin/env python3
"""Verify own-L010 historical LSE and exact live-LSI metadata; no HAL execution."""
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
PARTS = {"CW32L010F8P6", "CW32L010F8U6", "CW32L010Y8M6"}
PROOF = "docs/l010-lse-sysclk-qualification.json"
DETECTOR = {"lse_edges": 128, "lsi_cycles": 256, "margin_lse_edges": 1}
RM_PAGES = [52, 53, 56, 57, 58, 59, 63, 67, 68, 69, 70, 71, 74, 75, 77, 78, 79,
            80, 81, 83, 84, 87, 104, 114, 124, 130, 139, 140, 152, 200, 202,
            234, 281, 311, 377, 420]
LSI_POLICY = "cw32-data/lsi-sysclk-qualified.yaml"
L010_POLICY_SHA256 = "25dff68088cdab41b33ecf4a7e77392b5e7d62eae825fd730a6149ab42db6609"
EXPECTED_LSI = {
    "nominal_hz": 32800, "minimum_hz": 31816, "maximum_hz": 33784,
    "supply_mv": [1620, 5500], "temperature_c": [-40, 85],
    "factory_trim_address": 0x001007c2,
    "rtc_allowed_sources": [0, 1, 2, 3], "awt_allowed_sources": [0, 1],
    "uart_allowed_sources": [0, 1, 2, 3], "uarts": ["UART1", "UART2"],
    "gpio_banks": ["GPIOA", "GPIOB"],
    "gpio_filter_allowed_sources": [0, 1, 2, 3, 5, 7],
    "mco_allowed_sources": [0, 1, 2, 3, 4, 5, 6],
    "lsi_output_pin": None, "lsi_output_allowed_af": [], "rcc_irq": 4,
}
LSI_OPERATING = {
    "admission": "live_factory_reuse_only", "watchdog_source": "lsi",
    "trim_write": "forbidden", "waitcycle_preserved": True,
    "factory_read_width_bits": 16, "factory_erased_raw_halfword": 65535,
    "factory_zero_is_erased": False, "software_lsien_required": True,
    "entry_stable_required": True, "factory_trim_match_required": True,
    "ordinary_por_supported": False, "cycle_timing_qualified": False,
    "config_lse_on_lsi_target": "refuse_some",
    "gpio_inspection": "whole_bank_functional_handover",
    "inspection_scope": "existing_owner_windows_only",
    "selector_inventory_semantics": "documented_encodings_preserved_by_live_reuse",
    "awt_register": "RTC.CR2.AWTSRC",
}


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


def load(path):
    return yaml.load(path.read_text(), Loader=UniqueLoader)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()


def compact(value):
    return "".join(value.split())


def fields(ir, register):
    return {f["name"]: f for f in ir["fieldset/" + register]["fields"]}


def unique_index(values, key):
    result = {}
    for value in values:
        identity = value[key]
        assert identity not in result, f"duplicate {key}: {identity}"
        result[identity] = value
    return result


def complete_selection_roster():
    """Mirror input expansion without allowing a count to hide missing names."""
    parts = load(ROOT / "cw32-data/parts.yaml")
    assert parts["schema_version"] == 1
    exact = unique_index(parts["parts"], "name")
    selections, included_parts = {}, set()
    for path in sorted((ROOT / "cw32-data/inputs").glob("*.yaml")):
        source = load(path)
        line = source["line"]
        entries = source["chips"]
        if source.get("parts_catalog"):
            assert source["parts_catalog"] == "cw32-data/parts.yaml"
            entries = entries + [part for part in exact.values() if part["family"] == line]
            included_parts.update(part["name"] for part in exact.values() if part["family"] == line)
        for entry in entries:
            name = entry["name"]
            assert name not in selections, f"duplicate source selection: {name}"
            selections[name] = line
    assert included_parts == set(exact)
    assert len(selections) == 54, "reviewed complete generated roster must contain 54 selections"
    assert {name for name, line in selections.items() if line == "CW32L010"} == PARTS | {"CW32L010"}
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
    assert policy["schema_version"] == 1
    assert policy["source_authority"] == "sources/evidence-sources.json"
    assert sorted(policy["families"]) == [
        "CW32A030", "CW32F002", "CW32F003", "CW32F020", "CW32F030", "CW32L010", "CW32L011", "CW32L012",
        "CW32L031", "CW32L052", "CW32L083", "CW32R031", "CW32W031",
    ]
    own = policy["families"]["CW32L010"]
    # This identical whole-own-policy pin is also enforced by the Rust leaf.
    # It rejects unknown/missing nested fields and type changes, including
    # explicit null/empty declarations, rather than accepting a partial shape.
    assert sha(canonical(own)) == L010_POLICY_SHA256, "unreviewed own-L010 live-LSI policy"
    assert set(own) == set(LSI_OPERATING) | {
        "exact_parts", "sysctrl_version", "lsi_sysclk", "sources", "sdk_source", "sdk_members",
        "peripheral_inventory", "native_peripherals", "native_controls_source_ref", "native_controls",
        "source_status_overlays", "consumer_gates", "observer_irq", "requester_observers", "packages",
        "positive_lsi_routes", "historical_lse_monitor",
    }
    assert canonical({key: own[key] for key in LSI_OPERATING}) == canonical(LSI_OPERATING)
    assert canonical(own["lsi_sysclk"]) == canonical(EXPECTED_LSI)
    assert own["exact_parts"] == [
        {"name": "CW32L010F8P6", "package": "TSSOP20"},
        {"name": "CW32L010F8U6", "package": "QFN20"},
        {"name": "CW32L010Y8M6", "package": "SOP16"},
    ]
    assert set(unique_index(own["exact_parts"], "name")) == PARTS
    assert own["sysctrl_version"] == "cw32l010_v1"
    assert own["historical_lse_monitor"] == {
        "monitor_reference": "inherited_legal", "factory_trim_address": None, "maximum_hz": 36080,
    }

    sources = unique_index(own["sources"], "source_ref")
    assert set(sources) == {"vendor:CW32L010_UserManual_CN_V1.2.pdf", "vendor:CW32L010_DataSheet_CN_V1.3.pdf"}
    pages_by_source, page_receipts, original_receipts = {}, [], []
    for source_ref, source in sources.items():
        locked = authority[source_ref]
        assert source["evidence_kind"] == "external-evidence"
        assert locked["provenance"]["status"] == "selected"
        assert locked["provenance"]["chip_scope"] == ["CW32L010"]
        original = (source_root / locked["path"]).read_bytes()
        assert sha(original) == source["sha256"] == locked["sha256"]
        assert len(original) == locked["bytes"]
        text = (source_root / locked["text"]["path"]).read_bytes()
        assert sha(text) == source["text_sha256"] == locked["text"]["sha256"]
        assert len(text) == locked["text"]["bytes"]
        pages = text.decode().split("\f")
        pages_by_source[source_ref] = pages
        ordered_pages = source["pdf_pages_1_based"]
        assert ordered_pages == sorted(set(ordered_pages)) and ordered_pages
        assert source["printed_pages"] == [p - 1 for p in ordered_pages]
        for page in ordered_pages:
            assert 0 < page <= locked["provenance"]["pdf_page_count"]
            page_receipts.append({"source_ref": source_ref, "pdf_page_1_based": page,
                                  "printed_page": page - 1, "text_sha256": sha(pages[page - 1].encode())})
        original_receipts.append({"source_ref": source_ref, "sha256": locked["sha256"],
                                  "text_sha256": locked["text"]["sha256"]})
    manual = pages_by_source["vendor:CW32L010_UserManual_CN_V1.2.pdf"]
    datasheet = pages_by_source["vendor:CW32L010_DataSheet_CN_V1.3.pdf"]
    assert "LSI振荡器在启动前必须设置好所有参数，启动后禁止修改相关参数" in compact(manual[53])
    assert "0x001007C2-0x001007C3" in compact(manual[55])
    assert "SYSCTRL_CR1.LSIEN位域不会被置位" in compact(manual[68])
    assert all(anchor in compact(manual[68]) for anchor in [
        "SYSCTRL_CR1.HSECCS=1", "SYSCTRL_CR1.LSECCS=1", "GPIOx_FILTER.FLTCLK=0x5(LSI)",
        "VCx_CR1.FLTCLK=0x0(LSI)", "LVD_CR0.FLTCLK=0x0(LSI)", "IWDT_KR=0xCCCC",
    ])
    assert all(anchor in compact(manual[71]) for anchor in ["Addressoffset:0x20", "STABLERO", "WAITCYCLERW", "TRIMRW"])
    assert all(anchor in compact(datasheet[42]) for anchor in ["fLSI", "32.8", "ACCLSI", "-40℃~+85℃", "-3", "+3"])
    assert all(part in datasheet[7] for part in PARTS)
    assert "010：内部低速时钟LSI" in compact(manual[151])
    assert "0：RTCCLKD" in compact(manual[152]) and "1：TICKCLK" in compact(manual[152])
    assert "11：LSI" in compact(manual[199])
    assert "101：LSI" in compact(manual[129])
    assert "0100：LSI" in compact(manual[86])
    # The RM governs the write-key operation; preserve the raw SDK access
    # annotations independently below, including the AHBEN.KEY discrepancy.
    for page, register in [(79, "AHBEN"), (80, "APBEN1"), (81, "APBEN2")]:
        page_text = compact(manual[page - 1])
        assert "SYSCTRL_" + register in page_text
        assert "31:16KEYWO仅当KEY为0x5A5A时，对该寄存器的写操作有效" in page_text
    # The AF absence is narrow: positive MCO and internal timer routes survive.
    assert "LSI_OUT" not in manual[122]
    assert "LSI_OUT" not in datasheet[23] + datasheet[24]
    assert all(pin in manual[122] for pin in ["PA04", "PA08", "PB03"])
    assert "1001" in manual[233] and "LSI_OUT" in manual[233]
    assert "1001" in manual[310] and "LSI_OUT" in manual[310]

    sdk = own["sdk_source"]
    assert sdk["source_ref"] == "vendor:CW32L010_StandardPeripheralLib_V1.0.9.zip"
    locked_sdk = authority[sdk["source_ref"]]
    assert locked_sdk["provenance"]["status"] == "selected"
    assert locked_sdk["provenance"]["chip_scope"] == ["CW32L010"]
    archive_bytes = (source_root / locked_sdk["path"]).read_bytes()
    assert sha(archive_bytes) == sdk["sha256"] == locked_sdk["sha256"]
    assert len(archive_bytes) == locked_sdk["bytes"]
    for key in ["generator_svd_path", "generator_header_path"]:
        assert sdk[key] == locked_sdk["provenance"][key]
    locked_members = unique_index(locked_sdk["members"], "path")
    declared_members = unique_index(own["sdk_members"], "source_ref")
    assert len(declared_members) == 8
    sdk_members = {}
    for source_ref, member in declared_members.items():
        assert source_ref.startswith("member:")
        locked = locked_members[source_ref.removeprefix("member:")]
        assert member["member_chain"] == locked["members"]
        data = archive_bytes
        for archive_member in member["member_chain"]:
            with zipfile.ZipFile(io.BytesIO(data)) as archive:
                assert archive.namelist().count(archive_member) == 1
                data = archive.read(archive_member)
        assert sha(data) == member["sha256"] == locked["sha256"]
        assert len(data) == member["bytes"] == locked["bytes"]
        sdk_members[source_ref] = data
    assert own["native_controls_source_ref"] == "member:" + sdk["generator_svd_path"]
    svd = ET.fromstring(sdk_members[own["native_controls_source_ref"]])
    header = sdk_members["member:" + sdk["generator_header_path"]].decode()
    assert re.search(r"SYSCTRL_IRQn\s*=\s*4\s*,", header)
    assert re.search(r"#define\s+SYSCTRL_BASE\s+0x40004000UL", header)
    sysctrl_header = next(data for name, data in sdk_members.items() if name.endswith("/cw32l010_sysctrl.h")).decode()
    assert re.search(r"#define\s+SYSCTRL_LSI_TRIMCODEADDR\s+\(0x001007C2U\)", sysctrl_header)
    sysctrl_source = next(data for name, data in sdk_members.items() if name.endswith("/cw32l010_sysctrl.c")).decode()
    assert "CW_SYSCTRL->LSI=*((uint16_t*)SYSCTRL_LSI_TRIMCODEADDR)&0x3FF;" in compact(sysctrl_source)
    assert "CW_SYSCTRL->LSI|=SYSCTRL_LSI_WAITCYCLE_258;" in compact(sysctrl_source)

    native_receipt = verify_live_lsi_native(own, svd)
    package_receipt = verify_live_lsi_packages(own, sdk_members)
    expected_requesters = {
        "iwdt": ["IWDT"], "rtc_awt_calendar_output": ["RTC", "AWT", "calendar", "RTC_OUT"],
        "uart": ["UART1", "UART2"], "lptim": ["LPTIM"],
        "ready_and_output": ["MCO", "LSIRDY", "SYSCTRL_IRQ4", "NVIC_SYSCTRL_PENDING"],
        "gpio": ["GPIOA", "GPIOB"], "analog": ["VC1", "VC2", "LVD"],
        "clock_policy": ["HSECCS", "LSECCS", "clock_policy"],
        "downstream": ["GTIM1", "ATIM", "BTIM1", "BTIM2", "BTIM3", "ADC", "external_recipients"],
    }
    requesters = unique_index(own["requester_observers"], "id")
    assert {key: row["members"] for key, row in requesters.items()} == expected_requesters
    members = [member for row in requesters.values() for member in row["members"]]
    assert len(members) == len(set(members)), "duplicate requester/observer membership"
    for row in list(requesters.values()) + [own["positive_lsi_routes"]]:
        assert row["source_ref"] == "vendor:CW32L010_UserManual_CN_V1.2.pdf"
        assert row["pdf_pages_1_based"] == sorted(set(row["pdf_pages_1_based"]))
        assert set(row["pdf_pages_1_based"]) <= set(sources[row["source_ref"]]["pdf_pages_1_based"])
        assert row["printed_pages"] == [p - 1 for p in row["pdf_pages_1_based"]]
    return own, {
        "policy_path": LSI_POLICY, "policy_sha256": sha(policy_bytes), "own_policy_sha256": L010_POLICY_SHA256,
        "admission": own["admission"], "originals": original_receipts, "source_pages": page_receipts,
        "sdk_archive_sha256": sdk["sha256"], "sdk_members_verified": own["sdk_members"],
        **native_receipt, **package_receipt, "requester_observer_categories": sorted(requesters),
        "requester_observer_members": members, "historical_lse_proof_preserved": True,
        "historical_lse_monitor": own["historical_lse_monitor"],
    }


def verify_live_lsi_native(own, svd):
    source_peripherals = {}
    for peripheral in svd.findall("./peripherals/peripheral"):
        name = peripheral.findtext("name")
        assert name not in source_peripherals, f"duplicate source peripheral: {name}"
        source_peripherals[name] = peripheral
    assert own["peripheral_inventory"] == sorted(source_peripherals)
    assert len(own["peripheral_inventory"]) == 24
    assert set(own["native_peripherals"]) == set(source_peripherals)
    assert "AWT" not in source_peripherals
    source_input = load(ROOT / "cw32-data/inputs/cw32l010.yaml")
    assert source_input["source"]["source_ref"] == own["native_controls_source_ref"]
    assert source_input["source"]["sha256"] == next(
        member["sha256"] for member in own["sdk_members"]
        if member["source_ref"] == own["native_controls_source_ref"])
    native_ir = {}
    for name, binding in own["native_peripherals"].items():
        assert int(source_peripherals[name].findtext("baseAddress"), 0) == binding["address"], name
        assert source_input["register_versions"][binding["kind"]] == binding["version"]
        ir_name = binding["kind"] + "_" + binding["version"]
        ir = load(ROOT / "cw32-data/registers" / (ir_name + ".yaml"))
        assert "block/" + binding["block"] in ir
        native_ir[name] = ir
    assert own["native_peripherals"]["GPIOA"]["kind"] == "gpio"
    assert own["native_peripherals"]["GPIOB"]["kind"] == "gpiob"

    def source_registers(name):
        seen = set()
        peripheral = source_peripherals[name]
        while peripheral.get("derivedFrom"):
            base = peripheral.get("derivedFrom")
            assert base not in seen, "cyclic source peripheral inheritance"
            seen.add(base)
            peripheral = source_peripherals[base]
        registers = {}
        for register in peripheral.findall("./registers/register"):
            name = register.findtext("name")
            assert name not in registers
            registers[name] = register
        return registers

    access_names = {"read-only": "Read", "write-only": "Write", "read-write": "ReadWrite"}
    controls, field_count = {}, 0
    assert len(own["native_controls"]) == 72
    for control in own["native_controls"]:
        peripheral, register = control["peripheral"], control["register"]
        key = peripheral, register
        assert key not in controls, f"duplicate native control: {key}"
        controls[key] = control
        source_register = source_registers(peripheral)[register]
        binding, ir = own["native_peripherals"][peripheral], native_ir[peripheral]
        items = unique_index(ir["block/" + binding["block"]]["items"], "name")
        item = items[register]
        assert item["byte_offset"] == control["byte_offset"] == int(source_register.findtext("addressOffset"), 0), key
        assert item.get("bit_size", 32) == control["bit_size"] == int(source_register.findtext("size", "32"), 0), key
        assert item.get("access", "ReadWrite") == control["access"] == access_names[source_register.findtext("access")], key
        ir_fields = unique_index(ir["fieldset/" + item["fieldset"]]["fields"], "name")
        declared_fields = unique_index(control["fields"], "field")
        source_fields = {}
        for field in source_register.findall("./fields/field"):
            name = field.findtext("name")
            assert name not in source_fields
            source_fields[name] = field
        assert set(ir_fields) == set(declared_fields) == set(source_fields), key
        for name, declared in declared_fields.items():
            field, source_field = ir_fields[name], source_fields[name]
            assert (field["bit_offset"], field["bit_size"]) == (declared["bit_offset"], declared["bit_size"]), (key, name)
            assert (int(source_field.findtext("lsb"), 0), int(source_field.findtext("msb"), 0)) == (
                declared["bit_offset"], declared["bit_offset"] + declared["bit_size"] - 1), (key, name)
            assert source_field.findtext("access", source_register.findtext("access")) == declared["source_access"], (key, name)
            field_count += 1
    assert field_count == 462
    overlays = load(ROOT / "cw32-data/field-access.yaml")["registers"]["sysctrl_cw32l010_v1"]
    assert [{key: value for key, value in row.items() if key != "evidence"} for row in overlays] == own["source_status_overlays"]
    assert len(overlays) == 4
    for row in overlays:
        field = unique_index(controls["SYSCTRL", row["register"]]["fields"], "field")[row["field"]]
        assert field["source_access"] == "read-only"
        assert (row["bit_offset"], row["bit_size"]) == (field["bit_offset"], field["bit_size"])
    irq = own["observer_irq"]
    assert irq == {"name": "SYSCTRL", "number": 4, "peripheral": "SYSCTRL",
                   "source_ref": "vendor:CW32L010_UserManual_CN_V1.2.pdf",
                   "header_source_ref": "member:" + own["sdk_source"]["generator_header_path"],
                   "pdf_pages_1_based": [89], "printed_pages": [88]}
    assert [(i.findtext("name"), int(i.findtext("value")))
            for i in source_peripherals["SYSCTRL"].findall("interrupt")] == [("SYSCTRL", 4)]

    gates = unique_index(own["consumer_gates"], "peripheral")
    assert set(gates) == {"ADC", "ATIM", "BTIM1", "BTIM2", "BTIM3", "FLASH", "GPIOA", "GPIOB",
                          "GTIM1", "IWDT", "LPTIM", "RTC", "UART1", "UART2", "VC1", "VC2"}
    clock = load(ROOT / "cw32-data/clock/cw32l010.yaml")
    assert clock["profile"] == "CW32L010" and clock["controller"] == "SYSCTRL"
    clock_peripherals = unique_index(clock["peripherals"], "name")
    assert set(clock_peripherals) == set(source_peripherals)
    # Exact selected-SVD annotations: AHBEN differs from the RM's WO label.
    # This validates source identity, not KEY readback or a new IR correction.
    key_source_access = {"AHBEN": "read-write", "APBEN1": "write-only", "APBEN2": "write-only"}
    assert {gate["gate_register"] for gate in gates.values()} == set(key_source_access)
    for name, gate in gates.items():
        native = clock_peripherals[name]
        for key, relation, offset_key in [("gate", "enable", "gate_byte_offset"), ("reset", "reset", "reset_byte_offset")]:
            register, field_name = gate[key + "_register"], gate[key + "_field"]
            assert native[relation] == {"peripheral": "SYSCTRL", "register": register, "field": field_name}, name
            control = controls["SYSCTRL", register]
            assert control["byte_offset"] == gate[offset_key]
            field = unique_index(control["fields"], "field")[field_name]
            assert (field["bit_offset"], field["bit_size"], field["source_access"]) == (gate["bit_offset"], 1, "read-write")
        gate_control = controls["SYSCTRL", gate["gate_register"]]
        key_field = unique_index(gate_control["fields"], "field")["KEY"]
        assert (key_field["bit_offset"], key_field["bit_size"], key_field["source_access"]) == (
            16, 16, key_source_access[gate["gate_register"]])
        assert gate["enable_active_value"] is True and native["enable_active_value"] == 1
        assert gate["reset_asserted_value"] is False and native["reset_asserted_value"] == 0
        assert gate["key_value"] == native["enable_write_key"]["value"] == 0x5a5a
        assert native["enable_write_key"]["field"] == {
            "peripheral": "SYSCTRL", "register": gate["gate_register"], "field": "KEY"}
        for key in ["bus_clock", "shared_enable_group", "shared_reset_group"]:
            assert gate[key] == native.get(key), (name, key)
    assert clock_peripherals["IWDT"]["kernel_clock"] == {"clock": "LSI"}
    routes = own["positive_lsi_routes"]
    assert routes["dedicated_lsi_gpio_af"] is None and routes["mco_source"] == 4
    assert routes["mco_pads"] == [{"pin": "PA4", "af": 4}, {"pin": "PA8", "af": 3}, {"pin": "PB3", "af": 4}]
    assert [(row["peripheral"], row["register"], row["fields"], row["encoding"]) for row in routes["timer_inputs"]] == [
        ("GTIM1", "TISEL", ["TI1SEL", "TI2SEL", "TI3SEL", "TI4SEL"], 9),
        ("ATIM", "TISEL1", ["TI1SEL", "TI2SEL", "TI3SEL", "TI4SEL"], 9),
        ("ATIM", "TISEL2", ["TI5SEL", "TI6SEL"], 9),
    ]
    for route in routes["timer_inputs"]:
        available = unique_index(controls[route["peripheral"], route["register"]]["fields"], "field")
        for field in route["fields"]:
            assert available[field]["bit_size"] == 4
    return {"native_peripherals_verified": len(native_ir), "native_registers_verified": len(controls),
            "native_fields_verified": field_count, "consumer_gates_verified": len(gates),
            "source_status_overlays_verified": len(overlays), "observer_irq_verified": irq,
            "gate_key_source_access": key_source_access, "gate_key_manual_access": "write-only",
            "gate_key_operation": "write_0x5a5a_without_key_readback_claim",
            "positive_lsi_routes_verified": routes}


def verify_live_lsi_packages(own, sdk_members):
    parts_document = load(ROOT / "cw32-data/parts.yaml")
    parts = unique_index(parts_document["parts"], "name")
    pin_document = load(ROOT / "cw32-data/pinouts/cw32l010.yaml")
    pinouts = unique_index(pin_document["packages"], "name")
    assert pin_document["family"] == "CW32L010"
    assert set(pinouts) == set(own["packages"]) == PARTS
    source = parts_document["sources"]["CW32L010_datasheet"]
    assert pin_document["source"]["source_ref"] == source["source_ref"] == "vendor:CW32L010_DataSheet_CN_V1.3.pdf"
    assert pin_document["source"]["sha256"] == source["sha256"] == own["sources"][1]["sha256"]
    pdsc_source = parts_document["sources"]["CW32L010_dfp"]
    pdsc_bytes = sdk_members[pdsc_source["source_ref"]]
    assert sha(pdsc_bytes) == pdsc_source["sha256"]
    assert pdsc_source["archive_sha256"] == own["sdk_source"]["sha256"]
    pdsc = ET.fromstring(pdsc_bytes)
    devices = {device.get("Dname"): device for device in pdsc.findall(".//device")}
    assert len(devices) == len(pdsc.findall(".//device"))
    assert set(devices) == {"CW32L010F8"}
    memory = [{"name": "FLASH", "kind": "flash", "address": 0, "size": 65536},
              {"name": "RAM", "kind": "ram", "address": 0x20000000, "size": 4096}]
    bonded_routes = {}
    for part in sorted(PARTS):
        declared, actual, pinout = own["packages"][part], parts[part], pinouts[part]
        assert actual["family"] == "CW32L010" and actual["feature"] == part.lower()
        assert actual["memory_status"] == "verified-from-official-datasheet"
        assert declared["memory"] == actual["memory"] == memory
        assert declared["package"] == actual["package"] == pinout["package"]
        for key in ["pins", "gpio_pins", "input_only_pins"]:
            assert declared[key] == pinout[key], (part, key)
        for key in ["source_ids", "dfp_device"]:
            assert declared[key] == actual[key], (part, key)
        assert pinout["flash_bytes"] == 65536 and pinout["ram_bytes"] == 4096
        assert pinout["documented_orderable_parts"] == [part]
        assert declared["input_only_pins"] == ["PB7"]
        physical = unique_index(declared["pins"], "position")
        table = {}
        for row in pin_document["table_rows"]:
            position = row["positions"][pinout["table_column"]]
            if position is not None:
                assert position not in table, (part, "duplicate source-table pin position", position)
                table[position] = row["signals"]
        assert {position: row["signals"] for position, row in physical.items()} == table
        assert set(physical) == {str(index) for index in range(1, pinout["lead_count"] + 1)}
        gpios = {signal for row in physical.values() for signal in row["signals"] if re.fullmatch(r"P[A-Z][0-9]+", signal)}
        assert gpios == set(declared["gpio_pins"]) | set(declared["input_only_pins"])
        assert len(gpios) == (13 if part == "CW32L010Y8M6" else 17)
        if part == "CW32L010Y8M6":
            assert declared["dfp_device"] is None and declared["source_ids"] == ["CW32L010_datasheet"]
        else:
            assert declared["dfp_device"] == "CW32L010F8"
            assert declared["source_ids"] == ["CW32L010_datasheet", "CW32L010_dfp"]
            source_memory = [(m.get("name"), int(m.get("start"), 0), int(m.get("size"), 0))
                             for m in devices[declared["dfp_device"]].findall("memory")]
            assert source_memory == [("IROM1", 0, 65536), ("IRAM1", 0x20000000, 4096)]
        bonded_routes[part] = [route for route in own["positive_lsi_routes"]["mco_pads"] if route["pin"] in gpios]
    return {"packages_verified": sorted(PARTS), "bonded_mco_lsi_routes": bonded_routes,
            "y8m6_qualification": "datasheet_only_no_matching_dfp_device"}


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
    # This is a separate present-day capability. The historical LSE proof and
    # inherited-legal monitor above remain independent of factory-LSI SYSCLK.
    own_lsi, live_lsi_receipt = verify_live_lsi_policy(args.sources, authority)
    expected_lsi = own_lsi["lsi_sysclk"]
    selections, exact_parts = complete_selection_roster()
    generated, generated_parts, generated_names = 0, set(), set()
    lsi_owners, generic_l010_seen = 0, False
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
            assert "SYSCTRL" in peripherals and peripherals["SYSCTRL"].get("clock_limits") is not None, name
            owners = [p for p in peripherals.values() if p.get("clock_limits") is not None]
            expected = catalog["parts"].get(chip["name"], {}).get("configuration")
            for owner in owners:
                clock = owner["clock_limits"]
                config = clock.get("lse_configuration")
                assert config == expected, (name, owner["name"])
                if chip["line"] == "CW32L011":
                    # Separate own-L011 live qualification; retain this complete
                    # record comparison and the generic-alias exclusion.
                    expected_l011 = {'nominal_hz': 32800, 'minimum_hz': 29520, 'maximum_hz': 41000, 'supply_mv': [1700, 5500], 'temperature_c': [-40, 85], 'factory_trim_address': 1050562, 'rtc_allowed_sources': [0, 1, 2, 3], 'awt_allowed_sources': [0, 1], 'uart_allowed_sources': [0, 1, 2, 3], 'uarts': ['UART1', 'UART2', 'UART3'], 'gpio_banks': ['GPIOA', 'GPIOB', 'GPIOC'], 'gpio_filter_allowed_sources': [0, 1, 2, 3, 5, 7], 'mco_allowed_sources': [0, 1, 2, 3, 4, 5, 6], 'lsi_output_pin': None, 'lsi_output_allowed_af': [], 'rcc_irq': 4}
                    assert name in {"CW32L011", "CW32L011K8T6", "CW32L011K8U6"}
                    assert canonical(clock.get("lsi_sysclk")) == canonical(expected_l011 if name != "CW32L011" else None), (name, owner["name"])
                if chip["line"] == "CW32L010":
                    assert bool(config and config.get("sysclk_detector")) == (name in PARTS)
                    # This check deliberately sits outside the exact-only
                    # branch so generic L010 cannot inherit the capability.
                    assert canonical(clock.get("lsi_sysclk")) == canonical(expected_lsi if name in PARTS else None), (name, owner["name"])
                    if name not in PARTS:
                        continue
                    lsi_owners += 1
                    for key in ["hsi_frequency_hz", "hsi_error_percent", "factory_hsi_trim_address", "hsi_supply_mv", "hsi_temperature_c",
                                "low_voltage_threshold_mv", "low_voltage_bus_max_hz", "high_voltage_bus_max_hz", "flash_wait_step_hz", "initial_flash_wait"]:
                        assert clock[key] == limits[key]
                    assert clock["hse"]["fixed_ccs_hsi_divisor"] == divisor
            if chip["line"] == "CW32L010":
                if name in PARTS:
                    generated_parts.add(name)
                    package = own_lsi["packages"][name]
                    assert chip["memory"] == [exact_parts[name]["memory"]] == [package["memory"]]
                    assert chip["packages"] == [{"name": name, "package": package["package"], "pins": package["pins"]}]
                else:
                    assert name == "CW32L010"
                    generic_l010_seen = True
            generated += 1
        assert generated_names == set(selections), "complete generated selection identities are required"
        assert generated_parts == PARTS, "complete generated exact-three projection is required"
        assert lsi_owners == len(PARTS) == 3, "exactly three newly qualified L010 clock records are required"
        assert generic_l010_seen, "generic L010 exclusion must be checked"
    result = {"status": "passed", "scope": "own-L010 historical LSE and exact live factory-LSI source/metadata qualification",
              "qualified_parts": sorted(PARTS), "originals": original_receipts,
              "sdk_archive": {"evidence_kind": "external-evidence", "source_ref": sdk["source_ref"], "sha256": sdk["sha256"]},
              "sdk_members_verified": sdk["members"], "source_pages": page_receipts,
              "native_fields_verified": len(own_fields), "auxiliary_projection_preserved": True,
              "catalog_auxiliary_profiles": len(catalog["parts"]),
              "catalog_sysclk_profiles": sum("sysclk_detector" in p["configuration"] for p in catalog["parts"].values()),
              "generated_selections_checked": generated, "generated_exact_parts_checked": sorted(generated_parts),
              "live_factory_lsi": {**live_lsi_receipt, "expected_projection": expected_lsi,
                                   "generated_exact_parts_checked": sorted(generated_parts),
                                   "generated_clock_owners_checked": lsi_owners,
                                   "generic_l010_exclusion_checked": generic_l010_seen,
                                   "complete_selection_roster": sorted(selections),
                                   "generated_complete_roster_checked": bool(args.data)},
              "hal_execution": False, "hardware_execution": False}
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, indent=2) + "\n")
    print(f"PASS: own L010 historical LSE and exact-three live factory-LSI sources/profiles; {generated} generated selections; no HAL execution")


if __name__ == "__main__":
    main()
