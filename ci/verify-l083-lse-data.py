#!/usr/bin/env python3
"""Verify L083 own originals and exact native LSE facts; no HAL execution."""
import argparse
import hashlib
import json
from pathlib import Path
import zipfile
import yaml

ROOT = Path(__file__).resolve().parents[1]
PARTS = {"CW32L083RBT6", "CW32L083RCT6", "CW32L083RCS6", "CW32L083MCT6", "CW32L083VCT6"}


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
    for path, digest in catalog["policies"].items():
        assert sha((ROOT / path).read_bytes()) == digest
    page_receipts = []
    for source in proof["sources"]:
        locked = authority[source["source_ref"]]
        assert source["sha256"] == locked["sha256"]
        assert locked["provenance"]["chip_scope"] == ["CW32L083"]
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
        config = proof["configurations"][part]
        assert catalog["parts"][part] == {**proof["parts"][part], "configuration": config}
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
    generated = 0
    if args.data:
        for path in sorted((args.data / "chips").glob("*.json")):
            chip = load(path)
            owner = next(p for p in chip["cores"][0]["peripherals"] if p["name"] == "SYSCTRL")
            config = owner["clock_limits"].get("lse_configuration")
            if chip["line"] == "CW32L083":
                assert (config is not None) == (chip["name"] in PARTS)
                if config:
                    assert config == proof["configurations"][chip["name"]]
            generated += 1
    result = {"status": "passed", "originals_verified": len(originals), "sdk_members_verified": len(members), "qualified_parts": sorted(PARTS), "canonical_native_ir_sha256": canonical, "source_pages": page_receipts, "generated_selections_checked": generated, "hardware_execution": False}
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, indent=2) + "\n")
    print(f"PASS: {len(originals)} own originals, {len(members)} SDK members, {len(page_receipts)} page receipts, five exact parts, {generated} generated selections; no HAL execution")


if __name__ == "__main__":
    main()
