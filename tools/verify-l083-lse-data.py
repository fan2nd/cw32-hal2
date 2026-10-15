#!/usr/bin/env python3
"""Verify L083 own originals and exact native LSE/current LSI facts; no HAL execution."""
import argparse
import hashlib
import json
from pathlib import Path
import zipfile
import yaml

ROOT = Path(__file__).resolve().parents[1]
PARTS = {"CW32L083RBT6", "CW32L083RCT6", "CW32L083RCS6", "CW32L083MCT6", "CW32L083VCT6"}


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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sources", type=Path, required=True)
    parser.add_argument("--data", type=Path)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    locked_artifacts = load(ROOT / "sources/evidence-sources.json")["artifacts"]
    authority = {a["id"]: a for a in locked_artifacts}
    originals = load(ROOT / "docs/lse-l083-source-receipt.json")
    for source in originals:
        locked = authority[source["id"]]
        original = args.sources / source["path"]
        assert source["sha256"] == locked["sha256"] == sha(original.read_bytes())
        assert source["url"] == locked["url"] and source["bytes"] == original.stat().st_size
    members = load(ROOT / "docs/lse-l083-sdk-member-receipt.json")
    with zipfile.ZipFile(args.sources / "CW32L083_StandardPeripheralLib_V2.2.zip") as archive:
        for member in members:
            data = archive.read(member["member"])
            assert sha(data) == member["sha256"] and len(data) == member["bytes"]
    proof = load(ROOT / "docs/lse-active-l083.json")
    rtc = load(ROOT / "docs/lse-active-l083-rtc-admission.json")
    catalog = load(ROOT / "cw32-data/lse-qualified.yaml")
    assert set(proof["parts"]) == PARTS
    sysclk = load(ROOT / "docs/l083-lse-sysclk-qualification.json")
    assert sysclk["schema_version"] == 1 and set(sysclk["parts"]) == PARTS
    assert len(sysclk["parts"]) == len(PARTS)
    assert sysclk["sysclk_detector"] == {"lse_edges": 128, "lsi_cycles": 256, "margin_lse_edges": 1}
    assert sysclk["sysclk_selector"] == 4
    assert sysclk["lsi_sysclk_qualification"] is False and sysclk["lsi_cycle_timing_qualified"] is False
    assert [(s["source_ref"], s["pdf_pages_1_based"]) for s in sysclk["sources"]] == [
        ("vendor:CW32L083_UserManual_CN_V2.0.pdf", [59, 60, 62, 65, 67, 69, 72, 75, 76, 78, 79, 80, 81, 82, 121, 131]),
        ("vendor:CW32L083_DataSheet_CN_V1.9.pdf", [47, 55]),
    ]
    assert {part for part, profile in catalog["parts"].items()
            if part.startswith("CW32L083") and "sysclk_detector" in profile["configuration"]} == PARTS
    assert len(catalog["parts"]) == 23
    assert sum("sysclk_detector" in p["configuration"] for p in catalog["parts"].values()) == 23
    fallback = sysclk["fallback"]
    assert fallback["policy"] == "conservative_undivided_factory_hsi_bound"
    assert fallback["metadata_source"] == "clock_limits.hsi_frequency_hz and clock_limits.hsi_error_percent"
    assert (fallback["hsi_oscillator_nominal_hz"], fallback["hsi_factory_error_percent"]) == (48000000, 2)
    assert fallback["conservative_bus_and_flash_maximum_hz"] == 48960000
    assert all(fallback[key] is False for key in ["hsi_divider_credit", "ahb_divider_credit", "apb_divider_credit", "hardware_divisor_rewrite_claimed", "divisor_retention_proven"])
    assert fallback["applies_when_clkccs_disabled"] is True
    assert (fallback["minimum_admitted_supply_mv"], fallback["final_flash_wait"]) == (1800, 2)
    assert sysclk["electrical_limits"] == {
        "source_supply_mv": [1650, 5500], "sysclk_supply_mv": [1800, 5500],
        "temperature_c": [-40, 85], "low_voltage_threshold_mv": 1800,
        "low_voltage_bus_max_hz": 24000000, "high_voltage_bus_max_hz": 64000000,
        "flash_wait_step_hz": 24000000, "initial_flash_wait": 2, "final_flash_wait": 2,
    }
    for path, digest in catalog["policies"].items():
        assert sha((ROOT / path).read_bytes()) == digest
    # The historical LSE qualification remains unchanged. Independently bind
    # the current exact-five LSI addition before allowing it in generated data.
    lsi_policy_path = ROOT / "cw32-data/lsi-sysclk-qualified.yaml"
    own_lsi_policy = load(lsi_policy_path)["families"]["CW32L083"]
    assert sha(json.dumps(own_lsi_policy, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()) == "883667fa2049acd273c285808a71bb6eb19bb7035807a99d75eb7eb95f7e990c"
    assert load(ROOT / "cw32-data/electrical.yaml")["policies"]["cw32-data/lsi-sysclk-qualified.yaml"] == sha(lsi_policy_path.read_bytes())
    expected_parts = [
        {"name": "CW32L083RBT6", "package": "LQFP64（10×10mm）"},
        {"name": "CW32L083RCT6", "package": "LQFP64（10×10mm）"},
        {"name": "CW32L083RCS6", "package": "LQFP64（7×7mm）"},
        {"name": "CW32L083MCT6", "package": "LQFP80"},
        {"name": "CW32L083VCT6", "package": "LQFP100"},
    ]
    assert len(own_lsi_policy["exact_parts"]) == 5
    assert own_lsi_policy["exact_parts"] == expected_parts
    assert {p["name"] for p in expected_parts} == PARTS
    assert [(s["source_ref"], s["sha256"], s["text_sha256"]) for s in own_lsi_policy["sources"]] == [
        ("vendor:CW32L083_UserManual_CN_V2.0.pdf", "9930bf1755f3bbf8933163c2d0da57fd9a4f3250a358a4c0bfc75ed4eda3a0a3", "6abc933b3ed02659347ee557f47549e4c93c5de890530b950b20535b56a56564"),
        ("vendor:CW32L083_DataSheet_CN_V1.9.pdf", "852f772e9174cb76bf0f475f31f1e275254f8fe176bd3e7ad60d00b41db9509e", "2124f5c492e1d58b51674805b2d8a58f82b2ce462c8b73beaba63c6d93780220"),
    ]
    expected_current_lsi = {
        "nominal_hz": 32800, "minimum_hz": 31816, "maximum_hz": 33784,
        "supply_mv": [1650, 5500], "temperature_c": [-40, 85],
        "factory_trim_address": 0x00100a02,
        "rtc_allowed_sources": [0, 4, 5, 6, 7], "awt_allowed_sources": [],
        "uart_allowed_sources": [0, 1, 2],
        "uarts": [f"UART{i}" for i in range(1, 7)],
        "gpio_banks": [f"GPIO{bank}" for bank in "ABCDEF"],
        "gpio_filter_allowed_sources": [0, 1, 2, 3, 4, 6, 7],
        "mco_allowed_sources": [0, 1, 2, 3, 5, 6, 7, 8, 9],
        "lsi_output_pin": "PC4", "lsi_output_allowed_af": [0, 1, 2, 3],
        "rcc_irq": 4,
    }
    assert own_lsi_policy["lsi_sysclk"] == expected_current_lsi
    page_receipts = []
    for source in proof["sources"] + sysclk["sources"] + own_lsi_policy["sources"]:
        locked = authority[source["source_ref"]]
        assert source["sha256"] == locked["sha256"]
        assert locked["provenance"]["chip_scope"] == ["CW32L083"]
        if source in sysclk["sources"]:
            assert source["family"] == "CW32L083" and locked["provenance"]["status"] == "selected"
            assert "docs/l083-lse-sysclk-qualification.json" in locked["evidence"]
        if source in own_lsi_policy["sources"]:
            assert sum(a["id"] == source["source_ref"] for a in locked_artifacts) == 1
            assert locked["provenance"]["status"] == "selected"
            assert source["text_sha256"] == locked["text"]["sha256"]
            ordered_pages = source["pdf_pages_1_based"]
            assert ordered_pages and all(a < b for a, b in zip(ordered_pages, ordered_pages[1:]))
            assert all(0 < page <= locked["provenance"]["pdf_page_count"] for page in ordered_pages)
        text = (args.sources / locked["text"]["path"]).read_bytes()
        assert sha(text) == locked["text"]["sha256"]
        pages = text.decode().split("\f")
        assert len(source["pdf_pages_1_based"]) == len(source["printed_pages"])
        for pdf, printed in zip(source["pdf_pages_1_based"], source["printed_pages"]):
            assert pdf == printed + 1
            page_receipts.append({"source_ref": source["source_ref"], "pdf_page_1_based": pdf, "text_sha256": sha(pages[pdf - 1].encode())})
    manual = (args.sources / "CW32L083_UserManual_CN_V2.0.txt").read_text().split("\f")
    compact = lambda value: "".join(value.split())
    assert "0x0000002B" in compact(manual[80])
    assert all(field in manual[80] for field in ["DRIVER", "AMP", "WAITCYCLE", "STABLE"])
    assert all(field not in manual[80] for field in ["PDRIVER", "PAMP"])
    assert "RTC_ALARMA" in manual[207] and "0x00120000" in compact(manual[207])
    assert "使用LSE误差补偿的1Hz信号" in compact(manual[196])
    gates = compact(manual[86] + manual[88])
    for peripheral in ["LPTIM", "LCD"]:
        assert peripheral + "模块配置时钟及工作时钟使能控制" in gates
    for peripheral in ["RTC", "AUTOTRIM", *[f"UART{i}" for i in range(1, 7)]]:
        assert peripheral + "模块配置时钟使能控制" in gates
    assert "256" in manual[64] and "128" in manual[64]
    ir = load(ROOT / "cw32-data/registers/sysctrl_cw32l083_v1.yaml")
    fields = {f["name"]: f for f in ir["fieldset/LSE"]["fields"]}
    assert any(v["name"] == "LSE" and v["value"] == 4 for v in ir["enum/Sysclk"]["variants"])
    hsi = {f["name"]: f for f in ir["fieldset/HSI"]["fields"]}
    lsi = {f["name"]: f for f in ir["fieldset/LSI"]["fields"]}
    assert set(hsi) == {"TRIM", "DIV", "STABLE"}
    for bank, name, offset, width in [(hsi, "TRIM", 0, 11), (hsi, "DIV", 11, 4), (hsi, "STABLE", 15, 1), (lsi, "TRIM", 0, 10), (lsi, "WAITCYCLE", 10, 2), (lsi, "STABLE", 15, 1)]:
        assert (bank[name]["bit_offset"], bank[name]["bit_size"]) == (offset, width)
    assert set(fields) == {"DRIVER", "AMP", "WAITCYCLE", "MODE", "STABLE"}
    for name, offset, enumeration in [("DRIVER", 0, "LseDrive"), ("AMP", 2, "LseAmplitude"), ("WAITCYCLE", 4, "LseWait")]:
        assert (fields[name]["bit_offset"], fields[name]["bit_size"], fields[name]["enum"]) == (offset, 2, enumeration)
        assert [v["value"] for v in ir["enum/" + enumeration]["variants"]] == [0, 1, 2, 3]
    canonical = sha(json.dumps(ir, sort_keys=True, separators=(",", ":")).encode())
    group = next(g for g in load(ROOT / "cw32-data/register-reuse.yaml")["groups"] if g["canonical"] == "sysctrl_cw32l083_v1.yaml")
    assert group["canonical_ir_sha256"] == canonical == group["canonical_ir_history"][-1]["after_sha256"]
    assert group["source_versions"] == ["sysctrl_cw32l083_v1.yaml"]
    pins = {p["name"]: p for p in load(ROOT / "cw32-data/pinouts/cw32l083.yaml")["packages"]}
    raw_af = [entry for value in load(ROOT / "cw32-data/af/cw32l083.yaml").values() if isinstance(value, list) for entry in value if isinstance(entry, dict)]
    for part in PARTS:
        current = catalog["parts"][part]
        assert current["configuration"]["sysclk_detector"] == sysclk["sysclk_detector"]
        config = {key: value for key, value in current["configuration"].items() if key != "sysclk_detector"}
        assert config == proof["configurations"][part]
        assert {**current, "configuration": config} == {**proof["parts"][part], "configuration": config}
        assert config["rtc_reset"] == rtc["rtc_reset"]
        assert next(r for r in config["rtc_reset"] if r["register"] == "ALARMA")["value"] == 0x00120000
        assert config["awt_source"] is None and config["configurable_ccs"] is True
        native = config["startup_consumers"]
        assert native["startup_analog"] is False and native["autotrim_source"] == 3
        assert native["lptim"] == {"source": 2, "gate_controls_work": True}
        assert native["lcd"] == {"source": 1, "gate_controls_work": True}
        assert native["uarts"] == [f"UART{i}" for i in range(1, 7)]
        package = pins[part]
        bonded = {s for p in package["pins"] for s in p["signals"]}
        routes = sorted([{"pin": p["pin"], "af": p["af"]} for p in raw_af if p["function"] == "LSIOUT" and p["pin"] in bonded], key=lambda p: (p["pin"], p["af"]))
        assert native["lsi_output_routes"] == routes
        assert package["package"] == proof["parts"][part]["package"]
        expected = ("8", "9") if part == "CW32L083VCT6" else ("3", "4")
        for pin, position, signal in [("PC14", expected[0], "OSC32_IN"), ("PC15", expected[1], "OSC32_OUT")]:
            assert any(p["position"] == position and pin in p["signals"] and signal in p["signals"] for p in package["pins"])
        for route in proof["package_pins"][part]["output_routes"] + proof["lsi_output_routes_by_part"][part]:
            assert any(p["position"] == route["position"] and route["pin"] in p["signals"] for p in package["pins"])
    monitor = proof["lsi_monitor_prerequisite"]
    assert monitor["factory_frequency_hz"] == [31816, 33784]
    assert 17023 * 256 <= 129 * 33784 < 17024 * 256
    assert (2**32 - 1) * 256 < 2**40
    factory = sysclk["factory_lsi_reference"]
    assert [factory["minimum_hz"], factory["maximum_hz"]] == monitor["factory_frequency_hz"]
    assert factory["factory_trim_address"] == monitor["factory_trim_halfword_address"] == 0x00100a02
    assert factory["supply_mv"] == monitor["factory_supply_mv"] == [1650, 5500]
    assert factory["temperature_c"] == monitor["factory_temperature_c"] == [-40, 85]
    assert factory["lsi_sysclk_qualification"] is False
    electrical = load(ROOT / "cw32-data/electrical.yaml")["profiles"]["CW32L083"]["clock_limits"]
    assert electrical["hsi_frequency_hz"] == fallback["hsi_oscillator_nominal_hz"]
    assert electrical["hsi_error_percent"] == fallback["hsi_factory_error_percent"]
    assert electrical["hsi_supply_mv"] == factory["supply_mv"]
    assert electrical["hsi_temperature_c"] == factory["temperature_c"]
    assert "fixed_ccs_hsi_divisor" not in electrical["hse"]
    assert "lsi_sysclk" not in electrical
    assert (electrical["initial_flash_wait"], electrical["high_voltage_bus_max_hz"]) == (2, 64000000)
    generated = 0
    if args.data:
        for path in sorted((args.data / "chips").glob("*.json")):
            chip = load(path)
            owner = next(p for p in chip["cores"][0]["peripherals"] if p["name"] == "SYSCTRL")
            config = owner["clock_limits"].get("lse_configuration")
            if chip["line"] == "CW32L083":
                assert owner["clock_limits"].get("lsi_sysclk") == (
                    expected_current_lsi if chip["name"] in PARTS else None
                )
                assert (config is not None) == (chip["name"] in PARTS)
                if config:
                    assert config == catalog["parts"][chip["name"]]["configuration"]
                    assert owner["clock_limits"]["hse"].get("fixed_ccs_hsi_divisor") is None
            generated += 1
    result = {"status": "passed", "originals_verified": len(originals), "sdk_members_verified": len(members), "qualified_parts": sorted(PARTS), "canonical_native_ir_sha256": canonical, "sysclk_policy_sha256": sha((ROOT / "docs/l083-lse-sysclk-qualification.json").read_bytes()), "auxiliary_profiles": 23, "sysclk_profiles": 23, "source_pages": page_receipts, "generated_selections_checked": generated, "current_lsi_policy_sha256": sha(lsi_policy_path.read_bytes()), "current_lsi_exact_parts": sorted(PARTS), "hardware_execution": False}
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, indent=2) + "\n")
    print(f"PASS: {len(originals)} own originals, {len(members)} SDK members, {len(page_receipts)} page receipts, five exact parts, {generated} generated selections; no HAL execution")


if __name__ == "__main__":
    main()
