#!/usr/bin/env python3
"""Verify the own-source compact GPIO IDR permission and its PAC contract.

Source checks use external pinned evidence. Optional PAC cases compile ordinary
crate consumers; they do not execute MMIO or test a HAL implementation.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import xml.etree.ElementTree as ET

import yaml

ROOT = Path(__file__).resolve().parents[1]
RECEIPT = ROOT / "docs/compact-gpio-idr-access-corrections.json"
FAMILIES = {"CW32F002", "CW32F003"}
BLOCKS = {"GPIO": ("gpio_cw32f002_v1", ["GPIOA", "GPIOB"], 8),
          "GPIOC": ("gpioc_cw32f002_v1", ["GPIOC"], 6)}


def load(path):
    return yaml.safe_load(path.read_text())


def sha(data):
    return hashlib.sha256(data).hexdigest()


def normalized(value):
    return sha(json.dumps(value, sort_keys=True, separators=(",", ":")).encode())


def idr(data, block):
    register = next(r for r in data[f"block/{block}"]["items"] if r["name"] == "IDR")
    assert register["byte_offset"] == 80 and register["access"] == "Read"
    assert register.get("bit_size", 32) == 32 and register["fieldset"] == "IDR"
    return register


def check_sources(receipt, sources):
    assert {c["family"] for c in receipt["corrections"]} == FAMILIES
    assert len(receipt["corrections"]) == 2
    lock = load(ROOT / "sources/evidence-sources.json")
    artifacts = {a["id"]: a for a in lock["artifacts"]}
    members = {"member:" + m["path"]: m for a in lock["artifacts"] for m in a.get("members", [])}
    consumers = {name: set() for name, _, _ in BLOCKS.values()}
    for path in (ROOT / "cw32-data/inputs").glob("*.yaml"):
        profile = load(path)
        for kind, version in profile["register_versions"].items():
            if f"{kind}_{version}" in consumers:
                consumers[f"{kind}_{version}"].add(profile["line"])
    assert all(families == FAMILIES for families in consumers.values()), consumers
    for correction in receipt["corrections"]:
        family = correction["family"]
        manual = correction["manual"]
        original = artifacts[manual["source_ref"]]
        assert all(manual[k] == original[k] for k in ("sha256", "url"))
        assert sha((sources / original["path"]).read_bytes()) == manual["sha256"]
        text = (sources / original["text"]["path"]).read_bytes()
        assert sha(text) == original["text"]["sha256"]
        page = text.decode().split("\f")[manual["pdf_page"] - 1]
        section = page.split("8.6.14", 1)[1].split("8.6.15", 1)[0]
        assert "GPIOx_IDR" in section and "0x50" in section
        assert re.search(r"7:0\s+RO", section) and "x =A, B, C" in section
        assert manual["section"] == "8.6.14" and manual["pdf_page"] == manual["printed_page"] + 1
        profile = load(ROOT / f"cw32-data/inputs/{family.lower()}.yaml")
        assert profile["source"] == correction["source_svd"]
        member = members[profile["source"]["source_ref"]]
        svd = (sources / member["path"]).read_bytes()
        assert sha(svd) == member["sha256"] == profile["source"]["sha256"]
        device = ET.fromstring(svd)
        peripherals = {p.findtext("name"): p for p in device.findall("./peripherals/peripheral")}
        assert {b["block"] for b in correction["blocks"]} == BLOCKS.keys()
        for b in correction["blocks"]:
            block = b["block"]
            canonical, instances, pin_count = BLOCKS[block]
            assert (b["canonical"], b["instances"], b["existing_pin_fields"]) == (canonical, instances, pin_count)
            assert profile["register_versions"][block.lower()] == "cw32f002_v1"
            assert b["svd_register_access"] == "read-write"
            expected_field_access = "read-only" if family == "CW32F003" and block == "GPIO" else "read-write"
            assert b["svd_field_access"] == expected_field_access
            rules = [r for r in profile["register_overrides"] if (r["block"], r["register"]) == (block, "IDR")]
            assert len(rules) == 1
            rule = rules[0]
            assert (rule["expected_access"], rule["access"]) == ("ReadWrite", "Read")
            assert all(s in rule["evidence"] for s in (family, manual["revision"], manual["url"], manual["sha256"], "8.6.14", f'printed page {manual["printed_page"]}/PDF page {manual["pdf_page"]}'))
            for instance in instances:
                peripheral = peripherals[instance]
                base = peripherals[peripheral.get("derivedFrom")] if peripheral.get("derivedFrom") else peripheral
                assert not base.get("derivedFrom")
                assert base.findtext("headerStructName") == block
                register = next(r for r in base.findall("./registers/register") if r.findtext("name") == "IDR")
                assert int(register.findtext("addressOffset"), 0) == 80
                assert int(register.findtext("size"), 0) == 32
                assert register.findtext("access") == "read-write"
                fields = register.findall("./fields/field")
                assert {f.findtext("name") for f in fields} == {f"PIN{i}" for i in range(pin_count)}
                assert all(f.findtext("access") == expected_field_access for f in fields)
        print(f"PASS {family}: own-manual RO, original SVD register/field distinctions, all A/B/C banks")
    ledger = load(ROOT / "cw32-data/register-reuse.yaml")
    assert len(receipt["canonical_changes"]) == 2
    for change in receipt["canonical_changes"]:
        path = ROOT / change["path"]
        data = path.read_bytes()
        current = yaml.safe_load(data)
        idr(current, change["block"])
        pin_count = BLOCKS[change["block"]][2]
        assert {(f["name"], f["bit_offset"], f["bit_size"]) for f in current["fieldset/IDR"]["fields"]} == {(f"PIN{i}", i, 1) for i in range(pin_count)}
        group = next(g for g in ledger["groups"] if g["canonical"] == path.name)
        assert group["canonical_ir_sha256"] == normalized(current)
        assert group["source_versions"] == [path.name, path.name.replace("f002", "f003")]
        assert any(c.get("register") == "IDR" and c.get("evidence") == "docs/compact-gpio-idr-access-corrections.json" for c in group["reviewed_corrections"])
    for path in (ROOT / "embassy-cw32/src").rglob("*.rs"):
        assert not re.search(r"\.idr\(\)\s*\.\s*(?:write|modify|write_value)\s*\(", path.read_text()), path
    print("PASS current IDR access/field contracts, source reuse identities and HAL IDR read-only use")


def check_generated(receipt, baseline):
    for change in receipt["canonical_changes"]:
        canonical = Path(change["path"]).stem
        generated = ROOT / f"cw32-data/data/registers/{canonical}.json"
        assert load(generated) == load(ROOT / change["path"])
        idr(load(generated), change["block"])
        pac = ROOT / f"cw32-metapac/src/peripherals/{canonical}.rs"
        text = pac.read_text()
        signature = "pub const fn idr(self) -> crate::common::Reg<regs::Idr, crate::common::R>"
        assert signature in text
        if baseline:
            before = (baseline / pac.relative_to(ROOT)).read_text()
            assert before.count(signature.replace("::R>", "::RW>")) == 1
            assert text == before.replace(signature.replace("::R>", "::RW>"), signature)
    counts = {family: 0 for family in FAMILIES}
    for path in (ROOT / "cw32-data/data/chips").glob("*.json"):
        chip = load(path)
        if chip["line"] not in FAMILIES:
            continue
        counts[chip["line"]] += 1
        peripherals = {p["name"]: p for p in chip["cores"][0]["peripherals"] if p["name"].startswith("GPIO")}
        assert peripherals.keys() == {"GPIOA", "GPIOB", "GPIOC"}
        for block, (canonical, instances, _) in BLOCKS.items():
            for instance in instances:
                register = peripherals[instance]["registers"]
                assert register["block"] == block
                assert f'{register["kind"]}_{register["version"]}' == canonical
    assert counts == {"CW32F002": 3, "CW32F003": 4}, counts
    print(f"PASS all seven generated chip selections, offsets, RO accessor signatures{'; exact PAC accessor-only delta' if baseline else ''}")


def check_compile():
    count = 0
    with tempfile.TemporaryDirectory(prefix="cw32-compact-idr-pac-") as tmp:
        root = Path(tmp)
        (root / "src").mkdir()
        manifest = root / "Cargo.toml"
        source = root / "src/main.rs"
        def compile_case(code):
            source.write_text("use cw32_metapac as pac;\nfn main() {\n" + code + "\n}\n")
            return subprocess.run(["cargo", "check", "--offline", "--message-format=json", "--manifest-path", str(manifest)], text=True, capture_output=True)
        for family in sorted(FAMILIES):
            manifest.write_text('[package]\nname="compact-idr-access"\nversion="0.0.0"\nedition="2024"\n[workspace]\n[dependencies]\n' +
                                f'cw32-metapac={{path={json.dumps(str(ROOT / "cw32-metapac"))},default-features=false,features=["pac","{family.lower()}"]}}\n')
            positive = compile_case("\n".join(f"let _ = pac::{bank}.idr().read().pin0();\npac::{bank}.dir().write(|_| {{}});\npac::{bank}.dir().modify(|_| {{}});\npac::{bank}.dir().write_value(Default::default());" for bank in ("GPIOA", "GPIOB", "GPIOC")))
            assert positive.returncode == 0, positive.stdout + positive.stderr
            for bank in ("GPIOA", "GPIOB", "GPIOC"):
                for method in ("write", "modify", "write_value"):
                    argument = "Default::default()" if method == "write_value" else "|_| {}"
                    result = compile_case(f"pac::{bank}.idr().{method}({argument});")
                    messages = [json.loads(line) for line in result.stdout.splitlines()]
                    errors = [m["message"] for m in messages if m.get("reason") == "compiler-message" and m["message"]["level"] == "error"]
                    assert result.returncode != 0 and len(errors) == 1, result.stdout + result.stderr
                    error = errors[0]
                    assert error.get("code", {}).get("code") == "E0599" and f"`{method}`" in error["message"], error
                    assert any(f"pac::{bank}.idr().{method}" in line["text"] for span in error["spans"] for line in span["text"]), error
                    count += 1
            print(f"PASS {family}: input getters and configuration writes compile; IDR write/modify/write_value fail with E0599")
    assert count == 18
    print("PASS two ordinary PAC controls and 18 forbidden-access cases; no execution or MMIO")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sources", type=Path, default=Path(os.environ.get("CW32_SOURCES", "/workspace/shared/cw32-sources")))
    parser.add_argument("--source-only", action="store_true")
    parser.add_argument("--compile", action="store_true")
    parser.add_argument("--baseline", type=Path, help="Optional pre-correction generated tree for exact accessor-only proof")
    args = parser.parse_args()
    receipt = load(RECEIPT)
    check_sources(receipt, args.sources)
    if not args.source_only:
        check_generated(receipt, args.baseline)
    if args.compile:
        check_compile()


if __name__ == "__main__":
    main()
