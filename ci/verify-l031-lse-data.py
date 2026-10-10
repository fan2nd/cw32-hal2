#!/usr/bin/env python3
"""Verify exact-package own LSE source facts and optional generated data; no HAL tests."""
import argparse
import hashlib
import json
from pathlib import Path
import zipfile
import yaml

ROOT = Path(__file__).resolve().parents[1]
PARTS = {"CW32L031C8T6", "CW32L031C8U6", "CW32L031F8U6", "CW32R031C8U6", "CW32W031R8U6"}
FAMILIES = {"CW32L031", "CW32R031", "CW32W031"}
DETECTOR = {"lse_edges": 128, "lsi_cycles": 256, "margin_lse_edges": 1}


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
    # This audit owns exactly its five parts. Other families retain their own
    # qualification; generated projection below still checks the full catalog.
    assert {part for part, row in catalog["parts"].items() if row["family"] in FAMILIES} == PARTS
    sysclk_path = "docs/l031-r031-w031-lse-sysclk-qualification.json"
    sysclk = load(ROOT / sysclk_path)
    assert catalog["policies"][sysclk_path] == sha((ROOT / sysclk_path).read_bytes())
    assert len(sysclk["parts"]) == len(PARTS) and set(sysclk["parts"]) == PARTS
    assert sysclk["sysclk_detector"] == DETECTOR and sysclk["sysclk_selector"] == 4
    assert sysclk["factory_lsi_reference"]["metadata_source"] == "rtc_calendar"
    assert sysclk["factory_lsi_reference"]["lsi_sysclk_qualification"] is False
    sysclk_sources = {source["source_ref"]: source for source in sysclk["sources"]}
    assert len(sysclk["sources"]) == len(sysclk_sources) == 6
    checked_sysclk_sources = set()
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
            # Bind the new target proof to these same own locked originals,
            # independently of the historical auxiliary profile projection.
            target_source = sysclk_sources[source["source_ref"]]
            assert target_source["sha256"] == source["sha256"]
            assert target_source["family"] == "CW32" + family.upper()
            target_pages = target_source["pdf_pages_1_based"]
            assert target_pages and len(target_pages) == len(set(target_pages))
            assert target_source["printed_pages"] == [page - 1 for page in target_pages]
            assert all(1 <= page <= len(pages) for page in target_pages)
            checked_sysclk_sources.add(source["source_ref"])
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
            assert part in PARTS
            row = catalog["parts"][part]
            c = row["configuration"]
            auxiliary = dict(c)
            assert auxiliary.pop("sysclk_detector") == DETECTOR
            assert {**row, "configuration": auxiliary} == {**profile, "configuration": proof["configurations"][part]}
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
    assert checked_sysclk_sources == set(sysclk_sources)
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
            if chip["name"] in PARTS:
                proof = load(ROOT / f"docs/lse-active-{chip['line'][4:].lower()}.json")
                pins = proof["package_pins"][chip["name"]]
                assert chip["packages"][0]["package"] == catalog["parts"][chip["name"]]["package"]
                for pin, position, signal in [("PC14", pins["input_position"], "OSC32_IN"), ("PC15", pins["output_position"], "OSC32_OUT")]:
                    assert any(p["position"] == position and pin in p["signals"] and signal in p["signals"] for p in chip["packages"][0]["pins"])
        assert actual == set(catalog["parts"])
        generated_count = len(chips)
    result = {"status": "passed", "source_originals": source_count, "page_hashes": page_count, "sdk_members": member_count, "qualified_parts": sorted(PARTS), "sysclk_originals": len(checked_sysclk_sources), "generated_catalog_parts": len(catalog["parts"]), "generated_selections_checked": generated_count, "native_enum_canonical_sha256": group["canonical_ir_sha256"], "hardware_execution": False}
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
