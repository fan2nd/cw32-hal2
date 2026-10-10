#!/usr/bin/env python3
"""Verify exact-package own LSE source facts and optional generated data; no HAL tests."""
import argparse
import hashlib
import json
from pathlib import Path
import zipfile
import yaml

ROOT = Path(__file__).resolve().parents[1]
PARTS = {"CW32A030C8T7", "CW32F030C8T7", "CW32F020C6U7", "CW32L031C8T6", "CW32L031C8U6", "CW32L031F8U6", "CW32R031C8U6", "CW32W031R8U6", "CW32L052C8T6", "CW32L052R8S6", "CW32L052R8T6"}


def load(path):
    return yaml.safe_load(path.read_text())


def sha(data):
    return hashlib.sha256(data).hexdigest()


def normalized(value):
    return sha(json.dumps(value, sort_keys=True, separators=(",", ":")).encode())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sources", type=Path, required=True)
    parser.add_argument("--data", type=Path)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    lock = {a["id"]: a for a in load(ROOT / "sources/evidence-sources.json")["artifacts"]}
    catalog = load(ROOT / "cw32-data/lse-qualified.yaml")
    assert set(catalog["parts"]) == PARTS
    for path, digest in catalog["policies"].items():
        assert sha((ROOT / path).read_bytes()) == digest
    source_count = page_count = member_count = 0
    for family in ["l031", "r031", "w031"]:
        proof = load(ROOT / f"docs/lse-active-{family}.json")
        receipt = load(ROOT / f"docs/lse-{family}-source-receipt.json")
        rtc = load(ROOT / f"docs/lse-active-{family}-rtc-admission.json")
        for source in receipt["sources"]:
            authority = lock[source["source_ref"]]
            assert authority["provenance"]["status"] == "selected"
            assert authority["provenance"]["chip_scope"] == ["CW32" + family.upper()]
            assert authority["url"] == source["url"] and authority["path"] == source["path"]
            assert sha((args.sources / source["path"]).read_bytes()) == source["sha256"] == authority["sha256"]
            text = (args.sources / source["text"]["path"]).read_bytes()
            assert sha(text) == source["text"]["sha256"] == authority["text"]["sha256"]
            pages = text.decode().split("\f")
            for page in source["pages"]:
                assert page["pdf_page_1_based"] == page["printed_page"] + 1
                assert sha(pages[page["pdf_page_1_based"] - 1].encode()) == page["extracted_page_sha256"]
                page_count += 1
            source_count += 1
        sdk = receipt["sdk"]
        assert sha((args.sources / sdk["path"]).read_bytes()) == sdk["sha256"] == lock[sdk["source_ref"]]["sha256"]
        with zipfile.ZipFile(args.sources / sdk["path"]) as archive:
            for member in sdk["members"]:
                assert sha(archive.read(member["member"])) == member["member_sha256"]
                member_count += 1
        for part, profile in proof["parts"].items():
            assert catalog["parts"][part] == {**profile, "configuration": proof["configurations"][part]}
            c = catalog["parts"][part]["configuration"]
            assert c["configurable_ccs"] is True and c["gpio_speed_offset"] is None
            assert c["rtc_reset"] == rtc["rtc_reset"] and len(c["rtc_reset"]) == 13
            assert c["maximum_hz"] == 1_000_000 and c["nominal_hz"] == 32768
            assert c["output_routes"] == [{k: v for k, v in route.items() if k != "position"} for route in proof["package_pins"][part]["output_routes"]]
            assert (not c["output_routes"]) == (part == "CW32L031F8U6")
        gpio = load(ROOT / "cw32-data/registers/gpio_cw32l031_v1.yaml")
        assert [{"register": r["name"], "byte_offset": r["byte_offset"]} for r in gpio["block/GPIO"]["items"]] == proof["gpio_registers"]
        assert set(proof["gpio_absent_registers"]) == {"SPEED", "LOCK", "HIGHIE", "LOWIE"}
        monitor = proof["lsi_monitor_prerequisite"]
        assert monitor["factory_trim_halfword_address"] == 0x100A02
        assert monitor["selector_allowlists_before_disabled_LSI_trim_change"]["SYSCTRL.CR0.SYSCLK"] == [0, 1, 4]
        assert monitor["selector_allowlists_before_disabled_LSI_trim_change"]["SYSCTRL.MCO.SOURCE"] == [0, 1, 2, 3, 5, 6, 8, 9]
    ir = load(ROOT / "cw32-data/registers/sysctrl_cw32l031_v1.yaml")
    fields = {f["name"]: f for f in ir["fieldset/LSE"]["fields"]}
    for field, name in [("DRIVER", "LseDrive"), ("AMP", "LseAmplitude"), ("WAITCYCLE", "LseWait")]:
        assert fields[field]["enum"] == name
        assert [v["value"] for v in ir["enum/" + name]["variants"]] == [0, 1, 2, 3]
    group = next(g for g in load(ROOT / "cw32-data/register-reuse.yaml")["groups"] if g["canonical"] == "sysctrl_cw32l031_v1.yaml")
    assert group["source_versions"] == ["sysctrl_cw32l031_v1.yaml", "sysctrl_cw32r031_v1.yaml", "sysctrl_cw32w031_v1.yaml"]
    assert normalized(ir) == group["canonical_ir_sha256"] == group["canonical_ir_history"][-1]["after_sha256"]
    generated_count = None
    if args.data:
        assert normalized(load(args.data / "registers/sysctrl_cw32l031_v1.json")) == group["canonical_ir_sha256"]
        chips = sorted((args.data / "chips").glob("*.json"))
        actual = set()
        for path in chips:
            chip = load(path)
            core = chip["cores"][0]
            owner = next(p for p in core["peripherals"] if p["name"] == "SYSCTRL")
            active = owner["clock_limits"].get("lse_configuration")
            if active is not None:
                actual.add(chip["name"])
                assert active == catalog["parts"][chip["name"]]["configuration"]
            if chip["name"] in PARTS and chip["line"] in ["CW32L031", "CW32R031", "CW32W031"]:
                proof = load(ROOT / f"docs/lse-active-{chip['line'][4:].lower()}.json")
                pins = proof["package_pins"][chip["name"]]
                assert chip["packages"][0]["package"] == catalog["parts"][chip["name"]]["package"]
                for pin, position, signal in [("PC14", pins["input_position"], "OSC32_IN"), ("PC15", pins["output_position"], "OSC32_OUT")]:
                    assert any(p["position"] == position and pin in p["signals"] and signal in p["signals"] for p in chip["packages"][0]["pins"])
        assert actual == PARTS
        generated_count = len(chips)
    result = {"status": "passed", "source_originals": source_count, "page_hashes": page_count, "sdk_members": member_count, "qualified_parts": sorted(PARTS), "generated_selections_checked": generated_count, "native_enum_canonical_sha256": group["canonical_ir_sha256"], "hardware_execution": False}
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
