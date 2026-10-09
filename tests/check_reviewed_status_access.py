#!/usr/bin/env python3
"""Check current source/PAC facts for the reviewed oscillator access selection.

Optional compiler cases are PAC consumers only. They do not execute MMIO or
test HAL behavior. Historical source archives are not required.
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
SELECTION = ROOT / "docs/reviewed-status-access-selection.json"


def load(path):
    return yaml.safe_load(path.read_text())


def verified(path, expected):
    data = path.read_bytes()
    assert hashlib.sha256(data).hexdigest() == expected, path
    return data


def impl(text, fieldset):
    # These reviewed scalar SYSCTRL names have the same pinned sanitizer form.
    found = re.findall(r"    impl " + fieldset.title() + r" \{(.*?)\n    \}", text, re.S)
    assert len(found) == 1
    return found[0]


def check_sources(selection, sources):
    lock = load(ROOT / "sources/evidence-sources.json")
    artifacts = {a["id"]: a for a in lock["artifacts"]}
    members = {"member:" + m["path"]: m for a in lock["artifacts"] for m in a.get("members", [])}
    rules = load(ROOT / "cw32-data/field-access.yaml")["registers"]
    devices, manuals = {}, {}
    for family, binding in selection["sources"].items():
        svd = members[binding["svd_source_id"]]
        assert svd["sha256"] == binding["svd_sha256"]
        devices[family] = ET.fromstring(verified(sources / svd["path"], svd["sha256"]))
        manual = artifacts[binding["manual_source_id"]]
        assert manual["sha256"] == binding["manual_sha256"] and manual["url"] == binding["manual_url"]
        verified(sources / manual["path"], manual["sha256"])
        text = manual["text"]
        manuals[family] = verified(sources / text["path"], text["sha256"]).decode().split("\f")
    for selected in selection["fields"]:
        canonical, name = selected["canonical_map"], selected["register"]
        rows = [r for r in rules[canonical] if all(r[k] == selected[k] for k in ("block", "register", "fieldset", "field"))]
        assert len(rows) == 1 and rows[0]["evidence"]
        assert all(rows[0][k] == selected[k] for k in ("bit_offset", "bit_size"))
        ir = load(ROOT / f"cw32-data/registers/{canonical}.yaml")
        register = next(r for r in ir["block/SYSCTRL"]["items"] if r["name"] == name)
        assert register["byte_offset"] == selected["register_byte_offset"]
        assert register.get("access", "ReadWrite") == "ReadWrite" and register.get("bit_size", 32) == 32
        assert register["fieldset"] == selected["fieldset"] and not register.get("array")
        field = next(f for f in ir["fieldset/" + selected["fieldset"]]["fields"] if f["name"] == "STABLE")
        assert all(field[k] == selected[k] for k in ("bit_offset", "bit_size")) and not field.get("array")
        for row in selected["manual_rows"]:
            family = row["family"]
            p = next(p for p in devices[family].findall("./peripherals/peripheral") if p.findtext("name") == "SYSCTRL")
            reg = next(r for r in p.findall("./registers/register") if r.findtext("name") == name)
            assert int(reg.findtext("addressOffset"), 0) == selected["register_byte_offset"]
            assert reg.findtext("access") == "read-write" and int(reg.findtext("size"), 0) == 32
            raw = next(f for f in reg.findall("./fields/field") if f.findtext("name") == "STABLE")
            assert raw.findtext("access") == "read-only"
            if raw.findtext("bitOffset") is not None:
                offset, width = int(raw.findtext("bitOffset"), 0), int(raw.findtext("bitWidth"), 0)
            else:
                offset = int(raw.findtext("lsb"), 0)
                width = int(raw.findtext("msb"), 0) - offset + 1
            assert (offset, width) == (selected["bit_offset"], selected["bit_size"])
            page = manuals[family][row["manual_pdf_page"] - 1]
            start = re.search(re.escape(row["manual_section"]) + r"\s+SYSCTRL_" + name + r"\b", page)
            assert start, (family, name, row)
            section = re.split(r"\n4\.7\.\d+\s+", page[start.end():], maxsplit=1)[0]
            assert re.search(r"\b" + str(selected["bit_offset"]) + r"\s+STABLE\s+RO\b", section), (family, name)
    assert len(selection["fields"]) == 23 and sum(len(r["manual_rows"]) for r in selection["fields"]) == 30
    print("PASS 23 current exact rules / 30 original SVD and own-manual RO rows; register offsets/widths/RW preserved")


def check_generated():
    rules = load(ROOT / "cw32-data/field-access.yaml")["registers"]
    count = 0
    for canonical, entries in rules.items():
        if not canonical.startswith("sysctrl_"):
            continue
        projection = load(ROOT / f"cw32-data/data/field-access/{canonical}.json")
        assert projection == {"schema_version": 1, "registers": {canonical: entries}}
        assert load(ROOT / f"cw32-data/data/registers/{canonical}.json") == load(ROOT / f"cw32-data/registers/{canonical}.yaml")
        text = (ROOT / f"cw32-metapac/src/peripherals/{canonical}.rs").read_text()
        for row in entries:
            assert row["field"] == "STABLE"
            body = impl(text, row["fieldset"])
            assert "pub const fn stable(" in body and "fn set_stable(" not in body
            assert f'pub const fn {row["register"].lower()}(self) -> crate::common::Reg<regs::{row["fieldset"].title()}, crate::common::RW>' in text
            count += 1
    assert count == 41
    print("PASS all 41 SYSCTRL STABLE getters/restrictions and RW containers, including 18 preserved rules")


def check_compile(selection):
    assert os.environ.get("CARGO_TARGET_DIR"), "set the reserved CARGO_TARGET_DIR before compiler checks"
    with tempfile.TemporaryDirectory(prefix="cw32-reviewed-status-pac-") as tmp:
        root = Path(tmp)
        (root / "src").mkdir()
        source, manifest = root / "src/main.rs", root / "Cargo.toml"
        def compile_case(code):
            source.write_text("use cw32_metapac as pac;\nfn main() {\n" + code + "\n}\n")
            return subprocess.run(["cargo", "check", "--offline", "--message-format=json", "--manifest-path", str(manifest)], text=True, capture_output=True)
        negatives = 0
        for family in selection["sources"]:
            manifest.write_text('[package]\nname="reviewed-status-access"\nversion="0.0.0"\nedition="2024"\n[workspace]\n[dependencies]\n' +
                                f'cw32-metapac={{path={json.dumps(str(ROOT / "cw32-metapac"))},default-features=false,features=["pac","{family.lower()}"]}}\n')
            selected = [f for f in selection["fields"] if family in {m["family"] for m in f["manual_rows"]}]
            positive = []
            for f in selected:
                reg = f["register"].lower()
                text = (ROOT / f'cw32-metapac/src/peripherals/{f["canonical_map"]}.rs').read_text()
                setters = re.findall(r"pub (?:const )?fn set_(\w+)\(&mut self, val:", impl(text, f["fieldset"]))
                assert setters and "stable" not in setters
                code = " ".join(f"v.set_{s}(v.{s}());" for s in setters)
                positive += [f"let _ = pac::SYSCTRL.{reg}().read().stable();",
                             f"pac::SYSCTRL.{reg}().write(|v| {{ {code} }});",
                             f"pac::SYSCTRL.{reg}().modify(|v| {{ {code} }});",
                             f"pac::SYSCTRL.{reg}().write_value(Default::default());"]
            result = compile_case("\n".join(positive))
            assert result.returncode == 0, result.stdout + result.stderr
            for f in selected:
                code = f'pac::SYSCTRL.{f["register"].lower()}().modify(|v| v.set_stable(true));'
                result = compile_case(code)
                messages = [json.loads(line) for line in result.stdout.splitlines()]
                errors = [m["message"] for m in messages if m.get("reason") == "compiler-message" and m["message"]["level"] == "error"]
                assert result.returncode != 0 and len(errors) == 1, result.stdout + result.stderr
                error = errors[0]
                assert error.get("code", {}).get("code") == "E0599" and "`set_stable`" in error["message"], error
                assert any(code in line["text"] for span in error["spans"] for line in span["text"]), error
                negatives += 1
            print(f"PASS {family}: getters, every remaining configuration setter and register writes compile; selected set_stable calls fail with E0599")
        assert negatives == 30
    print("PASS 12 positive PAC consumers / 30 exact negative controls; no firmware or HAL tests")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sources", type=Path, default=Path(os.environ.get("CW32_SOURCES", ROOT / "sources/vendor")))
    parser.add_argument("--generated", action="store_true")
    parser.add_argument("--compile", action="store_true")
    args = parser.parse_args()
    selection = load(SELECTION)
    check_sources(selection, args.sources)
    if args.generated or args.compile:
        check_generated()
    if args.compile:
        check_compile(selection)


if __name__ == "__main__":
    main()
