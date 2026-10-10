#!/usr/bin/env python3
"""Verify current F020 LSE source/data contracts; no HAL execution or mocks."""
import argparse
import hashlib
import json
from pathlib import Path
import zipfile

import yaml

ROOT = Path(__file__).resolve().parents[1]


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
    receipt = load(ROOT / "docs/lse-f020-source-receipt.json")
    checked = []
    for source in receipt["sources"]:
        authority = lock[source["source_ref"]]
        assert authority["provenance"]["status"] == "selected"
        assert authority["provenance"]["chip_scope"] == ["CW32F020"]
        assert authority["provenance"]["printed_revision"] == source["printed_revision"]
        assert authority["url"] == source["url"]
        assert authority["path"] == source["path"]
        assert sha((args.sources / source["path"]).read_bytes()) == authority["sha256"] == source["sha256"]
        text = (args.sources / source["text"]["path"]).read_bytes()
        assert sha(text) == source["text"]["sha256"] == authority["text"]["sha256"]
        pages = text.decode().split("\f")
        for page in source["pages"]:
            assert page["pdf_page_1_based"] == page["printed_page"] + 1
            assert sha(pages[page["pdf_page_1_based"] - 1].encode()) == page["extracted_page_sha256"]
        checked.append(source["source_ref"])
    sdk = receipt["sdk"]
    archive = lock[sdk["sdk_source_ref"]]
    assert sha((args.sources / archive["path"]).read_bytes()) == sdk["sdk_sha256"] == archive["sha256"]
    with zipfile.ZipFile(args.sources / archive["path"]) as z:
        member = z.read(sdk["member"])
    assert sha(member) == sdk["member_sha256"]
    assert member == (args.sources / "cw32f020" / sdk["member"]).read_bytes()

    catalog = load(ROOT / "cw32-data/lse-qualified.yaml")
    expected = {"CW32A030C8T7", "CW32F030C8T7", "CW32F020C6U7",
                "CW32L031C8T6", "CW32L031C8U6", "CW32L031F8U6", "CW32R031C8U6", "CW32W031R8U6", "CW32L052C8T6", "CW32L052R8S6", "CW32L052R8T6", "CW32L083RBT6", "CW32L083RCT6", "CW32L083RCS6", "CW32L083MCT6", "CW32L083VCT6", "CW32L010F8P6", "CW32L010F8U6", "CW32L010Y8M6",
                "CW32L011K8T6", "CW32L011K8U6", "CW32L012C8T6", "CW32L012C8U6"}
    assert set(catalog["parts"]) == expected
    profile = catalog["parts"]["CW32F020C6U7"]
    proof = load(ROOT / "docs/lse-active-f020.json")
    rtc = load(ROOT / "docs/lse-active-f020-rtc-admission.json")
    assert profile["package"] == "QFN48"
    assert (profile["input_pin"], profile["output_pin"]) == ("PC14", "PC15")
    assert profile["sources"][0]["source_ref"] == "vendor:CW32F020_UserManual_CN_V1.4.pdf"
    assert profile["sources"][1]["source_ref"] == "vendor:current-datasheets/CW32F020_DataSheet_CN_V1.3.pdf"
    sysclk_path = "docs/classic-lse-sysclk-qualification.json"
    sysclk = load(ROOT / sysclk_path)
    assert sha((ROOT / sysclk_path).read_bytes()) == catalog["policies"][sysclk_path]
    assert set(sysclk["parts"]) == {"CW32A030C8T7", "CW32F020C6U7", "CW32F030C8T7"}
    # Own RM: 128 LSE edges / 256 LSI cycles; the extra edge is software policy.
    assert sysclk["sysclk_detector"] == {"lse_edges": 128, "lsi_cycles": 256, "margin_lse_edges": 1}
    assert "sysclk_detector" not in proof["configuration"]
    assert profile["configuration"] == {
        **proof["configuration"], "sysclk_detector": sysclk["sysclk_detector"]
    }
    assert {k: v for k, v in profile.items() if k != "configuration"} == proof["parts"]["CW32F020C6U7"]
    assert profile["configuration"]["rtc_reset"] == rtc["rtc_reset"]
    assert rtc["source"]["source_ref"] == profile["sources"][0]["source_ref"]
    for path, digest in catalog["policies"].items():
        assert sha((ROOT / path).read_bytes()) == digest

    name = "sysctrl_cw32f020_v1"
    ir = load(ROOT / f"cw32-data/registers/{name}.yaml")
    fields = {f["name"]: f for f in ir["fieldset/LSE"]["fields"]}
    for _, field, enumeration, bit, width in proof["native_enum_curation"]["fields"]:
        assert (fields[field]["enum"], fields[field]["bit_offset"], fields[field]["bit_size"]) == (enumeration, bit, width)
        variants = ir["enum/" + enumeration]["variants"]
        assert [v["name"] for v in variants] == proof["native_enum_curation"]["encodings"][enumeration]
        assert [v["value"] for v in variants] == [0, 1, 2, 3]
    group = next(g for g in load(ROOT / "cw32-data/register-reuse.yaml")["groups"] if g["canonical"] == name + ".yaml")
    assert group["source_versions"] == [name + ".yaml"]
    assert normalized(ir) == group["canonical_ir_sha256"]
    generated_count = None
    if args.data:
        generated = load(args.data / f"registers/{name}.json")
        assert normalized(generated) == group["canonical_ir_sha256"]
        chips = sorted((args.data / "chips").glob("*.json"))
        qualified = set()
        for path in chips:
            chip = load(path)
            core = chip["cores"][0]
            sysctrl = next(p for p in core["peripherals"] if p["name"] == "SYSCTRL")
            active = sysctrl["clock_limits"].get("lse_configuration")
            if active is not None:
                qualified.add(chip["name"])
                assert active == catalog["parts"][chip["name"]]["configuration"]
            if chip["name"] == "CW32F020C6U7":
                assert sysctrl["registers"]["version"] == "cw32f020_v1"
                assert chip["packages"][0]["package"] == "QFN48"
                for pin, position, signal in [("PC14", "3", "OSC32_IN"), ("PC15", "4", "OSC32_OUT")]:
                    assert any(p["position"] == position and pin in p["signals"] and signal in p["signals"] for p in chip["packages"][0]["pins"])
        assert qualified == expected
        generated_count = len(chips)
    result = {"status": "passed", "source_originals": checked, "sdk_member_verified": sdk["member"],
              "native_enum_canonical_sha256": group["canonical_ir_sha256"], "qualified": sorted(expected),
              "generated_selections_checked": generated_count, "hardware_execution": False}
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
