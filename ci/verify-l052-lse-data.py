#!/usr/bin/env python3
"""Verify L052 own originals and exact native LSE facts; no HAL execution."""
import argparse
import hashlib
import json
from pathlib import Path
import zipfile
import yaml

ROOT = Path(__file__).resolve().parents[1]
PARTS = {"CW32L052C8T6", "CW32L052R8S6", "CW32L052R8T6"}


def load(path):
    return yaml.safe_load(path.read_text())


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sources", type=Path, required=True)
    parser.add_argument("--data", type=Path)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    authority = {a["id"]: a for a in load(ROOT / "sources/evidence-sources.json")["artifacts"]}
    originals = load(ROOT / "docs/lse-l052-source-receipt.json")
    for source in originals:
        locked = authority[source["id"]]
        assert source["sha256"] == locked["sha256"] == sha((args.sources / source["path"]).read_bytes())
        assert source["url"] == locked["url"] and source["bytes"] == (args.sources / source["path"]).stat().st_size
    members = load(ROOT / "docs/lse-l052-sdk-member-receipt.json")
    with zipfile.ZipFile(args.sources / "CW32L052_StandardPeripheralLib_V1.4.zip") as archive:
        for member in members:
            data = archive.read(member["member"])
            assert sha(data) == member["sha256"] and len(data) == member["bytes"]
    proof = load(ROOT / "docs/lse-active-l052.json")
    rtc = load(ROOT / "docs/lse-active-l052-rtc-admission.json")
    catalog = load(ROOT / "cw32-data/lse-qualified.yaml")
    assert set(proof["parts"]) == PARTS
    for path, digest in catalog["policies"].items():
        assert sha((ROOT / path).read_bytes()) == digest
    page_receipts = []
    for source in proof["sources"]:
        locked = authority[source["source_ref"]]
        assert source["sha256"] == locked["sha256"]
        assert locked["provenance"]["chip_scope"] == ["CW32L052"]
        text = (args.sources / locked["text"]["path"]).read_bytes()
        assert sha(text) == locked["text"]["sha256"]
        pages = text.decode().split("\f")
        for pdf, printed in zip(source["pdf_pages_1_based"], source["printed_pages"]):
            assert pdf == printed + 1
            page_receipts.append({"source_ref": source["source_ref"], "pdf_page_1_based": pdf, "text_sha256": sha(pages[pdf - 1].encode())})
    manual = (args.sources / "CW32L052_UserManual_CN_V1.5.txt").read_text().split("\f")
    compact = lambda value: "".join(value.split())
    assert "0x00000A2B" in compact(manual[75])
    assert all(field in manual[75] for field in ["PDRIVER", "PAMP", "WAITCYCLE", "STABLE"])
    assert "0x04120000" in compact(manual[195])
    gates = compact(manual[80] + manual[81] + manual[82])
    assert "LPTIM模块配置时钟及工作时钟" in gates and "LCD模块配置时钟及工作时钟" in gates
    assert "AUTOTRIM模块配置时钟使能" in gates
    ir = load(ROOT / "cw32-data/registers/sysctrl_cw32l052_v1.yaml")
    fields = {f["name"]: f for f in ir["fieldset/LSE"]["fields"]}
    for name, offset, enumeration in [("DRIVER", 0, "LseDrive"), ("AMP", 2, "LseAmplitude"), ("WAITCYCLE", 4, "LseWait"), ("PDRIVER", 8, "LseDrive"), ("PAMP", 10, "LseAmplitude")]:
        assert (fields[name]["bit_offset"], fields[name]["bit_size"], fields[name]["enum"]) == (offset, 2, enumeration)
        assert [v["value"] for v in ir["enum/" + enumeration]["variants"]] == [0, 1, 2, 3]
    canonical = sha(json.dumps(ir, sort_keys=True, separators=(",", ":")).encode())
    group = next(g for g in load(ROOT / "cw32-data/register-reuse.yaml")["groups"] if g["canonical"] == "sysctrl_cw32l052_v1.yaml")
    assert group["canonical_ir_sha256"] == canonical == group["canonical_ir_history"][-1]["after_sha256"]
    assert group["source_versions"] == ["sysctrl_cw32l052_v1.yaml"]
    pins = {p["name"]: p for p in load(ROOT / "cw32-data/pinouts/cw32l052.yaml")["packages"]}
    for part in PARTS:
        config = proof["configurations"][part]
        assert catalog["parts"][part] == {**proof["parts"][part], "configuration": config}
        assert config["rtc_reset"] == rtc["rtc_reset"]
        assert next(r for r in config["rtc_reset"] if r["register"] == "ALARMA")["value"] == 0x04120000
        assert config["awt_source"] is None and config["configurable_ccs"] is True
        native = config["startup_consumers"]
        assert native["startup_analog"] is True and native["autotrim_source"] == 3
        assert native["lptim"] == {"source": 2, "gate_controls_work": True}
        assert native["lcd"] == {"source": 1, "gate_controls_work": True}
        assert native["uarts"] == ["UART1", "UART2", "UART3"]
        routes = [] if part == "CW32L052C8T6" else [{"pin": "PC4", "af": 6}]
        assert native["lsi_output_routes"] == routes
        package = pins[part]
        assert package["package"] == proof["parts"][part]["package"]
        for pin, position, signal in [("PC14", "3", "OSC32_IN"), ("PC15", "4", "OSC32_OUT")]:
            assert any(p["position"] == position and pin in p["signals"] and signal in p["signals"] for p in package["pins"])
        assert any("PC4" in p["signals"] for p in package["pins"]) == bool(routes)
        for route in proof["package_pins"][part]["output_routes"] + proof["lsi_output_routes_by_part"][part]:
            assert any(p["position"] == route["position"] and route["pin"] in p["signals"] for p in package["pins"])
    generated = 0
    if args.data:
        for path in sorted((args.data / "chips").glob("*.json")):
            chip = load(path)
            owner = next(p for p in chip["cores"][0]["peripherals"] if p["name"] == "SYSCTRL")
            config = owner["clock_limits"].get("lse_configuration")
            if chip["line"] == "CW32L052":
                assert (config is not None) == (chip["name"] in PARTS)
                if config:
                    assert config == proof["configurations"][chip["name"]]
            generated += 1
    result = {"status": "passed", "originals_verified": len(originals), "sdk_members_verified": len(members), "qualified_parts": sorted(PARTS), "canonical_native_ir_sha256": canonical, "source_pages": page_receipts, "generated_selections_checked": generated, "hardware_execution": False}
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, indent=2) + "\n")
    print(f"PASS: {len(originals)} own originals, {len(members)} SDK members, {len(page_receipts)} page receipts, three exact parts, {generated} generated selections; no HAL execution")


if __name__ == "__main__":
    main()
