#!/usr/bin/env python3
"""Verify own-L012 historical LSE and exact live-LSI sources/metadata; no HAL execution."""
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
PARTS = {"CW32L012C8T6", "CW32L012C8U6"}
PROOF = "docs/l012-lse-sysclk-qualification.json"
DETECTOR = {"lse_edges": 128, "lsi_cycles": 256, "margin_lse_edges": 1}


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


LSI_POLICY = "cw32-data/lsi-sysclk-qualified.yaml"
L012_POLICY_SHA256 = "6561702cdfc98a675a761b072e83da1aa9a11427eb1e8e6305775a08185c65ea"
EXPECTED_LSI = {'nominal_hz': 32800, 'minimum_hz': 29520, 'maximum_hz': 36080, 'supply_mv': [1700, 5500], 'temperature_c': [-40, 85], 'factory_trim_address': 1050562, 'rtc_allowed_sources': [0, 1, 2, 3], 'awt_allowed_sources': [0, 1], 'uart_allowed_sources': [0, 1, 2, 3], 'uarts': ['UART1', 'UART2', 'UART3'], 'gpio_banks': ['GPIOA', 'GPIOB', 'GPIOC', 'GPIOF'], 'gpio_filter_allowed_sources': [0, 1, 2, 3, 5, 6, 7], 'mco_allowed_sources': [0, 1, 2, 3, 4, 5, 6, 9], 'lsi_output_pin': 'PB11', 'lsi_output_allowed_af': [0, 1, 2, 3, 4, 5, 6, 7, 8, 9], 'rcc_irq': 4}
EXPECTED_LSI_PINS = [{'pin': 'PB11', 'signal': 'LSI_OUT', 'af': 4}, {'pin': 'PF3', 'signal': 'LSI_OUT', 'af': 2}]
BASELINE_SYSCTRL_PINS = [{"pin": "PC14", "signal": "LSE_IN"}, {"pin": "PC15", "signal": "LSE_OUT"}, {"pin": "PF0", "signal": "HSE_IN"}, {"pin": "PF1", "signal": "HSE_OUT"}]
EXPECTED_REQUESTERS = {'iwdt': ['IWDT'], 'rtc_awt_calendar_output': ['RTC', 'AWT', 'calendar', 'RTC_OUT'], 'uart': ['UART1', 'UART2', 'UART3'], 'i2c': ['I2C1', 'I2C2'], 'lptim': ['LPTIM'], 'ready_and_output': ['MCO', 'LSIRDY', 'SYSCTRL_IRQ4', 'NVIC_SYSCTRL_PENDING', 'PB11_LSI_OUT', 'PF3_LSI_OUT'], 'gpio': ['GPIOA', 'GPIOB', 'GPIOC', 'GPIOF'], 'analog': ['VC1', 'VC2', 'VC3', 'VC4', 'LVD', 'BGR', 'VC12REF', 'VC34REF'], 'adc_dac_opa': ['ADC1', 'ADC2', 'DAC', 'OPA1', 'OPA2'], 'clock_policy': ['HSECCS', 'LSECCS', 'HSIRDY', 'clock_policy', 'CLKFAULT_IRQ31'], 'downstream': ['GTIM1', 'GTIM2', 'GTIM3', 'GTIM4', 'ATIM', 'BTIM1', 'BTIM2', 'BTIM3', 'HALLTIM', 'IRMOD', 'external_recipients'], 'bus_domain_consumers': ['CPU', 'DMA', 'DMACHANNEL1', 'DMACHANNEL2', 'DMACHANNEL3', 'DMACHANNEL4', 'SPI1', 'SPI2', 'SPI3', 'WWDT', 'CRC', 'CORDIC', 'EAU', 'RAM', 'FLASH']}

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
    assert {name for name, line in selections.items() if line == "CW32L012"} == PARTS | {"CW32L012"}
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
    own = policy["families"]["CW32L012"]
    # Complete canonical equality rejects omitted, unknown, reordered or mistyped
    # nested facts. Source and curated IR are independently compared below.
    assert sha(canonical(own)) == L012_POLICY_SHA256, "unreviewed own-L012 live-LSI policy"
    assert canonical(own["lsi_sysclk"]) == canonical(EXPECTED_LSI)
    assert own["exact_parts"] == [{"name": "CW32L012C8T6", "package": "LQFP48"},
                                  {"name": "CW32L012C8U6", "package": "QFN48"}]
    assert set(unique_index(own["exact_parts"], "name")) == PARTS
    assert own["sysctrl_version"] == "cw32l012_v1"
    assert own["admission"] == "live_factory_reuse_only" and own["trim_write"] == "forbidden"
    assert own["watchdog_source"] == "independent_rc10k_with_documented_automatic_lsi_request"
    assert all(own[key] is True for key in ["waitcycle_preserved", "software_lsien_required", "entry_stable_required", "factory_trim_match_required"])
    assert all(own[key] is False for key in ["ordinary_por_supported", "cycle_timing_qualified", "factory_zero_is_erased"])
    assert (own["factory_read_width_bits"], own["factory_erased_raw_halfword"]) == (16, 65535)
    assert own["awt_register"] == "RTC.CR2.AWTSRC" and own["config_lse_on_lsi_target"] == "refuse_some"
    assert own["inspection_scope"] == "existing_owner_windows_only"
    assert own["lsi_parameter_preservation"] == {
        "register_byte_offset": 32, "trim_bit_offset": 0, "trim_bit_size": 9, "trim_mask": 511,
        "reserved_bit9": 9, "waitcycle_bit_offset": 10, "waitcycle_bit_size": 2, "stable_bit": 15,
        "parameter_mask": 0xffff7fff, "operation": "read_only",
        "factory_erased_check_order": "raw_u16_ffff_before_typed_trim_mask", "zero_trim_accepted": True,
    }
    sources = unique_index(own["sources"], "source_ref")
    assert set(sources) == {"vendor:CW32L012_UserManual_CN_V1.4.pdf", "vendor:CW32L012_UserManual_EN_V1.0.pdf", "vendor:CW32L012_DataSheet_CN_V1.0.pdf"}
    pages_by_source, page_receipts, original_receipts = {}, [], []
    for source_ref, source in sources.items():
        locked = authority[source_ref]
        assert source["evidence_kind"] == "external-evidence"
        status = "corroborating-language-edition" if "_EN_" in source_ref else "selected"
        assert locked["provenance"]["status"] == source["provenance_status"] == status
        assert locked["provenance"]["chip_scope"] == ["CW32L012"]
        original = (source_root / locked["path"]).read_bytes()
        assert sha(original) == source["sha256"] == locked["sha256"] and len(original) == locked["bytes"]
        text = (source_root / locked["text"]["path"]).read_bytes()
        assert sha(text) == source["text_sha256"] == locked["text"]["sha256"] and len(text) == locked["text"]["bytes"]
        pages = text.decode().split("\f")
        pages_by_source[source_ref] = pages
        ordered_pages = source["pdf_pages_1_based"]
        delta = 3 if "DataSheet" in source_ref else 26
        assert ordered_pages == sorted(set(ordered_pages)) and ordered_pages
        assert source["printed_pages"] == [p - delta for p in ordered_pages]
        for page in ordered_pages:
            assert 0 < page <= locked["provenance"]["pdf_page_count"]
            page_receipts.append({"source_ref": source_ref, "pdf_page_1_based": page,
                                  "printed_page": page - delta, "text_sha256": sha(pages[page - 1].encode())})
        original_receipts.append({"source_ref": source_ref, "sha256": locked["sha256"], "text_sha256": locked["text"]["sha256"]})
    assert len(page_receipts) == 173
    cn = pages_by_source["vendor:CW32L012_UserManual_CN_V1.4.pdf"]
    en = pages_by_source["vendor:CW32L012_UserManual_EN_V1.0.pdf"]
    ds = pages_by_source["vendor:CW32L012_DataSheet_CN_V1.0.pdf"]
    assert "LSI振荡器在启动前必须设置好所有参数，启动后禁止修改相关参数" in compact(cn[57])
    assert "itisforbiddentomodifytherelevantparametersafterstarting" in compact(en[59])
    assert all(anchor in compact(en[75]) for anchor in ["LSIENbitfieldwillnotbeset", "IWDT_KR=0xCCCC", "GPIOx_FILTER.FLTCLK=0x5(LSI)"])
    assert "dedicatedlow-speedRCoscillatorRC10K" in compact(en[494])
    assert all(anchor in compact(ds[57]) for anchor in ["fLSI", "32.8", "ACCLSI", "TA=-40℃~+85℃", "-10", "+10", "0.16"])
    assert "1.7V≤VDD＜1.8V" in compact(ds[46])
    assert all(anchor in compact(en[78]) for anchor in ["8:0TRIM", "0x001007C2", "11:10WAITCYCLE"])
    assert all(anchor in compact(en[96]) for anchor in ["0100:LSI", "1001:RC10K", "1101:8192frequencydivision", "1110:Reserved", "1111:Reserved"])
    assert all(anchor in compact(en[175]) for anchor in ["101:LSI", "110:RC10K", "111:LPTIMoverflow"])
    assert all(anchor in compact(en[167]) for anchor in ["PB11", "PF03/", "LSI_OUT"])

    sdk = own["sdk_source"]
    assert sdk["source_ref"] == "vendor:CW32L012_StandardPeripheralLib_V1.0.5.zip"
    locked_sdk = authority[sdk["source_ref"]]
    assert locked_sdk["provenance"]["status"] == "selected" and locked_sdk["provenance"]["chip_scope"] == ["CW32L012"]
    archive_bytes = (source_root / locked_sdk["path"]).read_bytes()
    assert sha(archive_bytes) == sdk["sha256"] == locked_sdk["sha256"] and len(archive_bytes) == locked_sdk["bytes"]
    for key in ["generator_svd_path", "generator_header_path"]:
        assert sdk[key] == locked_sdk["provenance"][key]
    locked_members = unique_index(locked_sdk["members"], "path")
    declared_members = unique_index(own["sdk_members"], "source_ref")
    assert len(declared_members) == 17
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
        assert sha(data) == member["sha256"] == locked["sha256"] and len(data) == member["bytes"] == locked["bytes"]
        sdk_members[source_ref] = data
    assert own["native_controls_source_ref"] == "member:" + sdk["generator_svd_path"]
    svd = ET.fromstring(sdk_members[own["native_controls_source_ref"]])
    header = sdk_members["member:" + sdk["generator_header_path"]].decode()
    assert re.search(r"SYSCTRL_IRQn\s*=\s*4\s*,", header)
    assert re.search(r"#define\s+SYSCTRL_BASE\s+0x40004000UL", header)
    gpio_header = sdk_members["member:cw32l012/Libraries/inc/cw32l012_gpio.h"].decode()
    assert "#definePB11_AFx_LSIOUT()(CW_GPIOB->AFRH_f.PIN11=4)" in compact(gpio_header)
    assert "#definePF03_AFx_LSIOUT()(CW_GPIOF->AFRL_f.PIN3=2)" in compact(gpio_header)
    native_receipt = verify_live_lsi_native(own, svd)
    package_receipt = verify_live_lsi_packages(own, sdk_members)
    requesters = unique_index(own["requester_observers"], "id")
    assert {key: row["members"] for key, row in requesters.items()} == EXPECTED_REQUESTERS
    members = [member for row in requesters.values() for member in row["members"]]
    assert len(members) == len(set(members))
    source_rows = list(requesters.values()) + own["consumer_gates"] + own["operational_access_facts"] + [own["observer_irq"]]
    source_rows += [row for row in own["positive_lsi_routes"]["source_refs"] if "pdf_pages_1_based" in row]
    for row in source_rows:
        source_ref = row["source_ref"]
        assert source_ref in sources
        assert row["pdf_pages_1_based"] == sorted(set(row["pdf_pages_1_based"]))
        assert set(row["pdf_pages_1_based"]) <= set(sources[source_ref]["pdf_pages_1_based"])
        assert row["printed_pages"] == [p - (3 if "DataSheet" in source_ref else 26) for p in row["pdf_pages_1_based"]]
    return own, {
        "policy_path": LSI_POLICY, "policy_sha256": sha(policy_bytes), "own_policy_sha256": L012_POLICY_SHA256,
        "admission": own["admission"], "originals": original_receipts, "source_pages": page_receipts,
        "sdk_archive_sha256": sdk["sha256"], "sdk_members_verified": own["sdk_members"],
        **native_receipt, **package_receipt, "requester_observer_categories": list(requesters),
        "requester_observer_members": members, "historical_lse_proof_preserved": True,
        "historical_lse_monitor": own["historical_lse_monitor"], "independent_rtc_source": own["independent_rtc_source"],
        "operational_access_facts": own["operational_access_facts"],
    }


def verify_live_lsi_native(own, svd):
    source_peripherals = {}
    for peripheral in svd.findall("./peripherals/peripheral"):
        name = peripheral.findtext("name")
        assert name not in source_peripherals, f"duplicate source peripheral: {name}"
        source_peripherals[name] = peripheral
    assert own["peripheral_inventory"] == sorted(source_peripherals)
    assert len(own["peripheral_inventory"]) == 50 and set(own["native_peripherals"]) == set(source_peripherals)
    assert "AWT" not in source_peripherals
    source_input = load(ROOT / "cw32-data/inputs/cw32l012.yaml")
    assert source_input["line"] == source_input["expected_svd_name"] == "CW32L012"
    assert source_input["source"]["source_ref"] == own["native_controls_source_ref"]
    assert source_input["source"]["sha256"] == next(member["sha256"] for member in own["sdk_members"] if member["source_ref"] == own["native_controls_source_ref"])
    native_ir = {}
    for name, binding in own["native_peripherals"].items():
        assert int(source_peripherals[name].findtext("baseAddress"), 0) == binding["address"], name
        assert source_input["register_versions"][binding["kind"]] == binding["version"]
        ir = load(ROOT / "cw32-data/registers" / (binding["kind"] + "_" + binding["version"] + ".yaml"))
        assert "block/" + binding["block"] in ir
        native_ir[name] = ir

    def source_registers(name):
        seen = set()
        peripheral = source_peripherals[name]
        while peripheral.get("derivedFrom"):
            base = peripheral.get("derivedFrom")
            assert base not in seen, "cyclic source peripheral inheritance"
            seen.add(base)
            peripheral = source_peripherals[base]
        registers = {r.findtext("name"): r for r in peripheral.findall("./registers/register")}
        assert len(registers) == len(peripheral.findall("./registers/register"))
        return registers

    controls, field_count = {}, 0
    assert len(own["native_controls"]) == 166
    for control in own["native_controls"]:
        peripheral, register = control["peripheral"], control["register"]
        key = peripheral, register
        assert key not in controls, f"duplicate native control: {key}"
        controls[key] = control
        source_register = source_registers(peripheral)[register]
        binding, ir = own["native_peripherals"][peripheral], native_ir[peripheral]
        item = unique_index(ir["block/" + binding["block"]]["items"], "name")[register]
        assert item["byte_offset"] == control["byte_offset"] == control["raw_svd_byte_offset"] == int(source_register.findtext("addressOffset"), 0), key
        assert item.get("bit_size", 32) == control["bit_size"] == int(source_register.findtext("size", "32"), 0), key
        # Curated IR permissions and raw original permissions are separate.
        # ADC ISR has an existing RM-based Read override; KEY is raw WO.
        assert item.get("access", "ReadWrite") == control["access"], key
        assert source_register.findtext("access") == control["raw_svd_register_access"], key
        assert item.get("array") == control["array"] is None and item["fieldset"] == control["fieldset"] == register
        ir_fields = unique_index(ir["fieldset/" + item["fieldset"]]["fields"], "name")
        declared_fields = unique_index(control["fields"], "field")
        source_fields = {f.findtext("name"): f for f in source_register.findall("./fields/field")}
        assert len(source_fields) == len(source_register.findall("./fields/field"))
        assert set(ir_fields) == set(declared_fields) == set(source_fields), key
        for name, declared in declared_fields.items():
            field, raw = ir_fields[name], declared["raw_svd"]
            source_field = source_fields[raw["field"]]
            assert raw["field"] == name
            assert (field["bit_offset"], field["bit_size"]) == (declared["bit_offset"], declared["bit_size"]), (key, name)
            assert field.get("array") == declared["array"] is None and field.get("enum") == declared["enum"]
            assert (int(source_field.findtext("lsb"), 0), int(source_field.findtext("msb"), 0)) == (raw["bit_offset"], raw["bit_offset"] + raw["bit_size"] - 1), (key, name)
            assert (raw["bit_offset"], raw["bit_size"]) == (declared["bit_offset"], declared["bit_size"])
            assert source_field.findtext("access", source_register.findtext("access")) == raw["access"], (key, name)
            field_count += 1
    assert field_count == 1466
    sysctrl_ir = native_ir["SYSCTRL"]
    assert {k.removeprefix("enum/"): {"bit_size": v["bit_size"], "variants": v["variants"]}
            for k, v in sysctrl_ir.items() if k.startswith("enum/")} == own["native_enums"]
    assert {v["name"]: v["value"] for v in own["native_enums"]["Sysclk"]["variants"]} == {"HSI": 0, "HSE": 1, "LSI": 3, "LSE": 4}
    assert not {"PLL", "HEX"} & set(unique_index(sysctrl_ir["block/SYSCTRL"]["items"], "name"))
    overlays = load(ROOT / "cw32-data/field-access.yaml")["registers"]["sysctrl_cw32l012_v1"]
    assert [{key: value for key, value in row.items() if key != "evidence"} for row in overlays] == own["source_status_overlays"]
    assert len(overlays) == 4
    for row in overlays:
        field = unique_index(controls["SYSCTRL", row["register"]]["fields"], "field")[row["field"]]
        assert field["raw_svd"]["access"] == "read-only"
        assert (row["bit_offset"], row["bit_size"]) == (field["bit_offset"], field["bit_size"])
    assert [(i.findtext("name"), int(i.findtext("value"))) for i in source_peripherals["SYSCTRL"].findall("interrupt")] == [("SYSCTRL", 4)]
    assert (own["observer_irq"]["name"], own["observer_irq"]["number"]) == ("SYSCTRL", 4)
    for access in own["operational_access_facts"]:
        for register in access["registers"]:
            for name in access.get("fields", [access.get("field")]):
                field = unique_index(controls[access["peripheral"], register]["fields"], "field")[name]
                assert field["raw_svd"]["access"] == access["raw_svd_access"]
    for name in ["ADC1", "ADC2"]:
        assert controls[name, "ISR"]["access"] == "Read"
        assert controls[name, "ISR"]["raw_svd_register_access"] == "read-write"
    gates = unique_index(own["consumer_gates"], "peripheral")
    assert len(gates) == 31
    clock = load(ROOT / "cw32-data/clock/cw32l012.yaml")
    assert clock["profile"] == "CW32L012" and clock["controller"] == "SYSCTRL"
    clock_peripherals = unique_index(clock["peripherals"], "name")
    assert set(clock_peripherals) == set(source_peripherals)
    for name, gate in gates.items():
        native = clock_peripherals[name]
        for key, relation, offset_key in [("gate", "enable", "gate_byte_offset"), ("reset", "reset", "reset_byte_offset")]:
            register, field_name = gate[key + "_register"], gate[key + "_field"]
            assert native[relation] == {"peripheral": "SYSCTRL", "register": register, "field": field_name}, name
            control = controls["SYSCTRL", register]
            assert control["byte_offset"] == gate[offset_key]
            field = unique_index(control["fields"], "field")[field_name]
            assert (field["bit_offset"], field["bit_size"], field["raw_svd"]["access"]) == (gate["bit_offset"], 1, "read-write")
        key_field = unique_index(controls["SYSCTRL", gate["gate_register"]]["fields"], "field")["KEY"]
        assert (key_field["bit_offset"], key_field["bit_size"], key_field["raw_svd"]["access"]) == (16, 16, "write-only")
        assert "KEY" not in unique_index(controls["SYSCTRL", gate["reset_register"]]["fields"], "field")
        assert gate["enable_active_value"] is True and native["enable_active_value"] == 1
        assert gate["reset_asserted_value"] is False and native["reset_asserted_value"] == 0
        assert gate["key_value"] == native["enable_write_key"]["value"] == 0x5a5a
        assert native["enable_write_key"]["field"] == {"peripheral": "SYSCTRL", "register": gate["gate_register"], "field": "KEY"}
        for key in ["bus_clock", "shared_enable_group", "shared_reset_group"]:
            assert gate[key] == native.get(key), (name, key)
    routes = own["positive_lsi_routes"]
    assert routes["exact_sysctrl_pin_projection"] == EXPECTED_LSI_PINS
    assert len(routes["dedicated_lsi_gpio_af"]) == 2 and routes["mco_source"] == 4
    assert routes["mco_pads"] == [{"pin": "PA4", "af": 4}, {"pin": "PA8", "af": 4}, {"pin": "PC13", "af": 1}]
    for route, expected in zip(routes["dedicated_lsi_gpio_af"], [("PB11", 4, "GPIOB", "AFRH", 20, "PIN11"), ("PF3", 2, "GPIOF", "AFRL", 24, "PIN3")]):
        assert tuple(route[k] for k in ["pin", "af", "gpio", "register", "byte_offset", "field"]) == expected
        field = unique_index(controls[route["gpio"], route["register"]]["fields"], "field")[route["field"]]
        assert (field["bit_offset"], field["bit_size"]) == (12, 4)
        assert route["documented_af_codes"] == list(range(10)) and route["documented_af_functions"][route["af"]] == "LSI_OUT"
    return {"native_peripherals_verified": len(native_ir), "native_registers_verified": len(controls),
            "native_fields_verified": field_count, "consumer_gates_verified": len(gates),
            "source_status_overlays_verified": len(overlays), "observer_irq_verified": own["observer_irq"],
            "raw_svd_and_curated_ir_access_checked_independently": True,
            "positive_lsi_routes_verified": routes}


def verify_live_lsi_packages(own, sdk_members):
    parts_document = load(ROOT / "cw32-data/parts.yaml")
    parts = unique_index(parts_document["parts"], "name")
    pin_document = load(ROOT / "cw32-data/pinouts/cw32l012.yaml")
    pinouts = unique_index(pin_document["packages"], "name")
    assert pin_document["family"] == "CW32L012" and set(pinouts) == set(own["packages"]) == PARTS
    source = parts_document["sources"]["CW32L012_datasheet"]
    assert pin_document["source"]["source_ref"] == source["source_ref"] == "vendor:CW32L012_DataSheet_CN_V1.0.pdf"
    assert pin_document["source"]["sha256"] == source["sha256"] == own["sources"][2]["sha256"]
    pdsc_source = parts_document["sources"]["CW32L012_dfp"]
    pdsc_bytes = sdk_members[pdsc_source["source_ref"]]
    assert sha(pdsc_bytes) == pdsc_source["sha256"] and pdsc_source["archive_sha256"] == own["sdk_source"]["sha256"]
    pdsc = ET.fromstring(pdsc_bytes)
    devices = {device.get("Dname"): device for device in pdsc.findall(".//device")}
    assert len(devices) == len(pdsc.findall(".//device")) and set(devices) == {"CW32L012C8"}
    memory = [{"name": "FLASH", "kind": "flash", "address": 0, "size": 65536},
              {"name": "RAM", "kind": "ram", "address": 0x20000000, "size": 8192}]
    bonded_routes = {}
    for part in sorted(PARTS):
        declared, actual, pinout = own["packages"][part], parts[part], pinouts[part]
        assert actual["family"] == "CW32L012" and actual["feature"] == part.lower()
        assert actual["memory_status"] == "verified-from-official-datasheet"
        assert declared["memory"] == actual["memory"] == memory
        assert declared["package"] == actual["package"] == pinout["package"]
        for key in ["pins", "gpio_pins", "input_only_pins"]:
            assert declared[key] == pinout[key], (part, key)
        for key in ["source_ids", "dfp_device"]:
            assert declared[key] == actual[key], (part, key)
        assert pinout["flash_bytes"] == 65536 and pinout["ram_bytes"] == 8192
        assert pinout["documented_orderable_parts"] == [part]
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
        assert declared["dfp_device"] == "CW32L012C8" and declared["source_ids"] == ["CW32L012_datasheet", "CW32L012_dfp"]
        source_memory = [(m.get("name"), int(m.get("start"), 0), int(m.get("size"), 0)) for m in devices[declared["dfp_device"]].findall("memory")]
        assert source_memory == [("IROM1", 0, 65536), ("IRAM1", 0x20000000, 8192)]
        assert declared["direct_lsi_bonds"] == [{"pin": "PB11", "position": "22", "af": 4}, {"pin": "PF3", "position": "44", "af": 2}]
        for route in declared["direct_lsi_bonds"]:
            assert route["pin"] in physical[route["position"]]["signals"] and route["pin"] in gpios
        for bank in ["GPIOA", "GPIOB", "GPIOC", "GPIOF"]:
            absent = ["P" + bank[-1] + str(i) for i in range(16) if "P" + bank[-1] + str(i) not in gpios]
            assert absent == own["unbonded_native_fields"][bank]
        bonded_routes[part] = declared["direct_lsi_bonds"]
    return {"packages_verified": sorted(PARTS), "bonded_direct_lsi_routes": bonded_routes}


def remove_stage75_evidence_append(authority):
    # Historical --baseline covers the old 21-to-23 LSE delta. Remove only the
    # separately reviewed Stage75 reference at these exact existing records.
    reference = "docs/l012-live-factory-lsi-qualification.json"
    records = [('/artifacts/6/evidence', 'vendor:CW32L012_StandardPeripheralLib_V1.0.5.zip', 10), ('/artifacts/6/members/17/evidence', 'member:cw32l012/IDEsupport/MDK/WHXY.CW32L012_DFP.1.0.2/SVD/CW32L012.svd', 2), ('/artifacts/6/members/18/evidence', 'member:cw32l012/IDEsupport/MDK/WHXY.CW32L012_DFP.1.0.2/WHXY.CW32L012_DFP.pdsc', 1), ('/artifacts/6/members/19/evidence', 'member:cw32l012/Libraries/inc/cw32l012.h', 12), ('/artifacts/6/members/21/evidence', 'member:cw32l012/Libraries/inc/cw32l012_btim.h', 1), ('/artifacts/6/members/27/evidence', 'member:cw32l012/Libraries/inc/cw32l012_flash.h', 1), ('/artifacts/6/members/28/evidence', 'member:cw32l012/Libraries/inc/cw32l012_gpio.h', 2), ('/artifacts/6/members/29/evidence', 'member:cw32l012/Libraries/inc/cw32l012_gtim.h', 2), ('/artifacts/6/members/31/evidence', 'member:cw32l012/Libraries/inc/cw32l012_iwdt.h', 2), ('/artifacts/6/members/32/evidence', 'member:cw32l012/Libraries/inc/cw32l012_lpi2c.h', 2), ('/artifacts/6/members/33/evidence', 'member:cw32l012/Libraries/inc/cw32l012_lptim.h', 2), ('/artifacts/6/members/36/evidence', 'member:cw32l012/Libraries/inc/cw32l012_rtc.h', 2), ('/artifacts/6/members/38/evidence', 'member:cw32l012/Libraries/inc/cw32l012_sysctrl.h', 8), ('/artifacts/6/members/39/evidence', 'member:cw32l012/Libraries/inc/cw32l012_uart.h', 3), ('/artifacts/6/members/53/evidence', 'member:cw32l012/Libraries/src/cw32l012_iwdt.c', 2), ('/artifacts/6/members/54/evidence', 'member:cw32l012/Libraries/src/cw32l012_lpi2c.c', 2), ('/artifacts/6/members/59/evidence', 'member:cw32l012/Libraries/src/cw32l012_sysctrl.c', 4), ('/artifacts/6/members/67/evidence', 'member:cw32l012/Libraries/inc/cw32l012_atim.h', 1), ('/artifacts/24/evidence', 'vendor:CW32L012_DataSheet_CN_V1.0.pdf', 17), ('/artifacts/25/evidence', 'vendor:CW32L012_UserManual_CN_V1.4.pdf', 29), ('/artifacts/42/evidence', 'vendor:CW32L012_UserManual_EN_V1.0.pdf', 5)]
    for pointer, source_ref, old_count in records:
        components = pointer.strip("/").split("/")
        record = authority
        for key in components[:-1]:
            record = record[int(key)] if isinstance(record, list) else record[key]
        identity = record.get("id", "member:" + record.get("path", ""))
        assert identity == source_ref, (pointer, identity)
        evidence = record[components[-1]]
        assert len(evidence) == old_count + 1 and evidence[-1] == reference
        assert evidence.count(reference) == 1
        evidence.pop()


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
        remove_stage75_evidence_append(new_authority)
        for artifact in new_authority["artifacts"]:
            if PROOF in artifact.get("evidence", []):
                artifact["evidence"].remove(PROOF)
        assert new_authority == old_authority
        baseline_checked = True
    own_lsi, live_lsi_receipt = verify_live_lsi_policy(args.sources, authority)
    selections, exact_parts = complete_selection_roster()
    generated, generated_parts, generic, lsi_owners = 0, set(), False, 0
    if args.data:
        paths = sorted((args.data / "chips").glob("*.json"))
        assert len(paths) == len(selections) == 52, "--data must contain all 52 active generated selections"
        assert {path.stem for path in paths} == set(selections), "missing or unexpected generated filename"
        seen = set()
        for path in paths:
            chip = load(path)
            name = chip["name"]
            assert path.stem == name and name not in seen and chip["line"] == selections[name], name
            seen.add(name)
            assert len(chip["cores"]) == 1
            core = chip["cores"][0]
            peripherals = unique_index(core["peripherals"], "name")
            owners = [p for p in peripherals.values() if p.get("clock_limits") is not None]
            assert len(owners) == 1 and owners[0]["name"] == "SYSCTRL", name
            owner = owners[0]
            clock = owner["clock_limits"]
            config = clock.get("lse_configuration")
            assert config == catalog["parts"].get(name, {}).get("configuration"), name
            if chip["line"] == "CW32L012":
                assert name in PARTS | {"CW32L012"}
                assert bool(config and config.get("sysclk_detector")) == (name in PARTS)
                assert canonical(clock.get("lsi_sysclk")) == canonical(EXPECTED_LSI if name in PARTS else None), name
                assert clock.get("pll") is None, name
                routes = [p for p in owner.get("pins", []) if p["signal"] == "LSI_OUT"]
                assert routes == (EXPECTED_LSI_PINS if name in PARTS else []), name
                assert all(not any(pin["signal"] == "LSI_OUT" for pin in p.get("pins", []))
                           for p in peripherals.values() if p["name"] != "SYSCTRL"), name
                assert peripherals["RTC"]["rtc_calendar"] == own_lsi["independent_rtc_source"], name
                if name in PARTS:
                    generated_parts.add(name)
                    lsi_owners += 1
                    assert owner["pins"] == BASELINE_SYSCTRL_PINS + EXPECTED_LSI_PINS, name
                    assert clock["default_hsi_divisor"] == 12 and clock["hsi_frequency_hz"] == 96000000
                    package = own_lsi["packages"][name]
                    assert chip["memory"] == [package["memory"]]
                    assert chip["packages"] == [{"name": name, "package": package["package"], "pins": package["pins"]}]
                    assert sorted(p["name"] for p in core["pins"]) == sorted(package["gpio_pins"] + package["input_only_pins"])
                    for route in package["direct_lsi_bonds"]:
                        assert sum(p["name"] == route["pin"] for p in core["pins"]) == 1
                generic |= name == "CW32L012"
            generated += 1
        assert seen == set(selections)
        assert generated_parts == PARTS and generic and lsi_owners == 2, "complete exact-two and generic-negative projection required"
    result = {"status": "passed", "scope": "own-L012 historical LSE and exact live factory-LSI source/metadata qualification", "qualified_parts": sorted(PARTS),
              "originals": originals, "sdk_archive": {"evidence_kind": "external-evidence", "source_ref": sdk["source_ref"], "sha256": sdk["sha256"]},
              "sdk_members_verified": sdk["members"], "source_pages": page_receipts, "native_fields_verified": len(native_fields),
              "auxiliary_projection_preserved": True, "prior_21_and_auxiliary_23_baseline_checked": baseline_checked,
              "catalog_auxiliary_profiles": 23, "catalog_sysclk_profiles": 23, "generated_selections_checked": generated,
              "generated_exact_parts_checked": sorted(generated_parts), "generic_l012_absence_checked": generic,
              "live_factory_lsi": {**live_lsi_receipt, "expected_projection": EXPECTED_LSI,
                                   "expected_pin_projection": EXPECTED_LSI_PINS,
                                   "generated_clock_owners_checked": lsi_owners,
                                   "complete_selection_roster": selections,
                                   "generated_generic_absence_checked": generic},
              "hal_execution": False, "hardware_execution": False}
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, indent=2) + "\n")
    print(f"PASS: own L012 historical LSE and exact-two live factory-LSI; originals, {len(sdk_members)} SDK members, {len(page_receipts)} pages, {len(native_fields)} fields, exact two profiles; {generated} generated selections; no HAL execution")


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
