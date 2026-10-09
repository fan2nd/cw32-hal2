#!/usr/bin/env python3
"""Check documented GPIO ISR access and compile its read-only PAC contract.

No fixture is executed and no MMIO occurs. --source-only can run before
regeneration. A full run requires regenerated JSON/PAC and checks every affected
chip record plus both forbidden methods on every bank of each corrected family.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import xml.etree.ElementTree as ET

import yaml

ROOT = Path(__file__).resolve().parents[1]
EVIDENCE = ROOT / "docs/gpio-isr-access-corrections.json"
EXPECTED = {
    "CW32F002": {"GPIO": ("gpio_cw32f002_v1", ["GPIOA", "GPIOB"]),
                 "GPIOC": ("gpioc_cw32f002_v1", ["GPIOC"])},
    "CW32F003": {"GPIO": ("gpio_cw32f002_v1", ["GPIOA", "GPIOB"]),
                 "GPIOC": ("gpioc_cw32f002_v1", ["GPIOC"])},
    "CW32L010": {"GPIO": ("gpio_cw32l010_v1", ["GPIOA"]),
                 "GPIOB": ("gpiob_cw32l010_v1", ["GPIOB"])},
    "CW32L011": {"GPIO": ("gpio_cw32l011_v1", ["GPIOA", "GPIOB", "GPIOC"])},
    "CW32L052": {"GPIO": ("gpio_cw32l052_v1", ["GPIOA", "GPIOB", "GPIOC", "GPIOD", "GPIOF"])},
}


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def isr(document, block):
    matches = [r for r in document[f"block/{block}"]["items"] if r["name"] == "ISR"]
    assert len(matches) == 1, block
    result = matches[0]
    assert result["byte_offset"] == 0x34 and result["access"] == "Read", result
    return result


def check_sources(evidence, manual_dir):
    assert evidence["status"] == "verified-own-family-manual-access-only"
    records = {record["family"]: record for record in evidence["corrections"]}
    assert len(records) == len(evidence["corrections"]) == len(EXPECTED)
    assert records.keys() == EXPECTED.keys()
    canonical_names = {canonical for blocks in EXPECTED.values() for canonical, _ in blocks.values()}
    # A source-reused block needs evidence in every profile that selects it.
    consumers = {}
    for path in sorted((ROOT / "cw32-data/inputs").glob("*.yaml")):
        profile = yaml.safe_load(path.read_text())
        for kind, version in profile["register_versions"].items():
            name = f"{kind}_{version}"
            if name in canonical_names:
                consumers.setdefault(name, set()).add(profile["line"])
    for canonical in canonical_names:
        expected_consumers = {family for family, blocks in EXPECTED.items()
                              if any(value[0] == canonical for value in blocks.values())}
        assert consumers[canonical] == expected_consumers, (canonical, consumers[canonical])
    for family, blocks in EXPECTED.items():
        record = records[family]
        assert {b["block"]: (b["canonical"], b["instances"]) for b in record["blocks"]} == blocks
        assert (record["expected_access"], record["access"], record["byte_offset"]) == ("ReadWrite", "Read", 52)
        manual = record["manual"]
        assert manual["filename"].startswith(family + "_UserManual_")
        assert manual["pdf_page"] == manual["printed_page"] + 1
        if manual_dir is not None:
            assert sha256(manual_dir / manual["filename"]) == manual["sha256"], manual["filename"]
        profile = yaml.safe_load((ROOT / f"cw32-data/inputs/{family.lower()}.yaml").read_text())
        assert record["source_svd"] == {k: profile["source"][k] for k in ("path", "sha256")}
        svd_path = ROOT / record["source_svd"]["path"]
        assert sha256(svd_path) == record["source_svd"]["sha256"], svd_path
        device = ET.parse(svd_path).getroot()
        peripherals = {p.findtext("name"): p for p in device.findall("./peripherals/peripheral")}
        for block, (canonical, instances) in blocks.items():
            overrides = [o for o in profile["register_overrides"]
                         if (o["block"], o["register"]) == (block, "ISR")]
            assert len(overrides) == 1, (family, block)
            override = overrides[0]
            assert (override["expected_access"], override["access"]) == ("ReadWrite", "Read")
            for item in (family, manual["revision"], manual["section"],
                         f'printed page {manual["printed_page"]}/PDF page {manual["pdf_page"]}',
                         manual["sha256"], manual["url"]):
                assert item in override["evidence"], (family, block, item)
            curated = yaml.safe_load((ROOT / f"cw32-data/registers/{canonical}.yaml").read_text())
            isr(curated, block)
            # Verify exact own-source block lineage and old access, including
            # F003's separate SVD and all inherited B/C/D/F instances.
            for instance in instances:
                peripheral = peripherals[instance]
                base = peripherals[peripheral.get("derivedFrom")] if peripheral.get("derivedFrom") else peripheral
                assert not base.get("derivedFrom"), (family, instance)
                assert (base.findtext("headerStructName") or base.findtext("name")) == block
                register = next(r for r in base.findall("./registers/register") if r.findtext("name") == "ISR")
                assert int(register.findtext("addressOffset"), 0) == 0x34
                assert int(register.findtext("size"), 0) == 32
                access = register.findtext("access") or base.findtext("access") or device.findtext("access")
                assert access == "read-write", (family, instance, access)
        print(f"PASS {family}: own-manual access evidence and original SVD lineage")
    changes = evidence["canonical_changes"]
    assert {Path(c["path"]).stem for c in changes} == canonical_names
    for change in changes:
        path = ROOT / change["path"]
        # Compact GPIO now has an independently qualified IDR correction.
        # Keep the ISR receipt's dated hashes as history and test its current
        # ISR contract above; do not chain inverse edits of later corrections.
        if path.stem in {"gpio_cw32f002_v1", "gpioc_cw32f002_v1"}:
            continue
        assert sha256(path) == change["after_sha256"], change["path"]
        # Reconstruct the recorded original byte-for-byte by removing only the
        # single reviewed access line. This rejects incidental field changes.
        corrected = ("  - name: ISR\n    description: Interrupt status register.\n"
                     "    byte_offset: 52\n    access: Read\n    fieldset: ISR\n")
        text = path.read_text()
        assert text.count(corrected) == 1, change["path"]
        original = text.replace(corrected, corrected.replace("    access: Read\n", ""))
        assert hashlib.sha256(original.encode()).hexdigest() == change["before_sha256"], change["path"]
        assert change["before_sha256"] != change["after_sha256"]
    assert evidence["deferred"] == []
    print("PASS six current canonical ISR contracts; four unchanged historical snapshots; all source-reuse consumers covered")


def check_generated():
    counts = {family: 0 for family in EXPECTED}
    for path in sorted((ROOT / "cw32-data/data/chips").glob("*.json")):
        chip = json.loads(path.read_text())
        blocks = EXPECTED.get(chip["line"])
        if blocks is None:
            continue
        counts[chip["line"]] += 1
        expected_instances = {p for _, instances in blocks.values() for p in instances}
        peripherals = {p["name"]: p for p in chip["cores"][0]["peripherals"] if p["name"].startswith("GPIO")}
        assert peripherals.keys() == expected_instances, chip["name"]
        for block, (canonical, instances) in blocks.items():
            generated = json.loads((ROOT / f"cw32-data/data/registers/{canonical}.json").read_text())
            isr(generated, block)
            for instance in instances:
                registers = peripherals[instance]["registers"]
                assert registers["block"] == block
                assert f'{registers["kind"]}_{registers["version"]}' == canonical
    assert all(counts.values()), counts
    print(f"PASS regenerated RO metadata and bank mapping in {sum(counts.values())} affected chip records: {counts}")


def cargo_environment():
    env = os.environ.copy()
    if (ROOT / ".cargo/bin/cargo").exists():
        env["CARGO_HOME"] = str(ROOT / ".cargo")
        env["RUSTUP_HOME"] = str(ROOT / ".rustup")
        env["PATH"] = str(ROOT / ".cargo/bin") + os.pathsep + env.get("PATH", "")
    return env


def check_pac():
    negative_count = 0
    with tempfile.TemporaryDirectory(prefix="cw32-gpio-isr-access-") as temp:
        fixture = Path(temp)
        (fixture / "src").mkdir()
        manifest = fixture / "Cargo.toml"
        source = fixture / "src/main.rs"

        def compile_case(code):
            source.write_text("use cw32_metapac as pac;\nfn main() {\n" + code + "\n}\n")
            return subprocess.run(
                ["cargo", "check", "--offline", "--message-format=json", "--manifest-path", str(manifest),
                 "--target-dir", str(ROOT / "target/gpio-isr-access-tests")],
                env=cargo_environment(), text=True, capture_output=True,
            )

        for family, blocks in EXPECTED.items():
            instances = [p for _, ports in blocks.values() for p in ports]
            manifest.write_text(
                '[package]\nname = "cw32-gpio-isr-access-tests"\nversion = "0.0.0"\nedition = "2024"\n'
                '[workspace]\n[dependencies]\n'
                f'cw32-metapac = {{ path = {json.dumps(str(ROOT / "cw32-metapac"))}, '
                f'default-features = false, features = ["pac", "{family.lower()}"] }}\n'
            )
            # Read every corrected bank; leave a RW control in the same fixture.
            positive = compile_case("\n".join(f"let _ = pac::{p}.isr().read();" for p in instances)
                                    + "\npac::GPIOA.dir().write(|_| {});\npac::GPIOA.dir().modify(|_| {});")
            assert positive.returncode == 0, f"{family} positive control failed:\n{positive.stdout}{positive.stderr}"
            print(f"PASS {family}: ISR read() on {', '.join(instances)} and DIR RW controls")
            for instance in instances:
                for method in ("write", "modify"):
                    result = compile_case(f"pac::{instance}.isr().{method}(|_| {{}});")
                    errors = []
                    for line in result.stdout.splitlines():
                        item = json.loads(line)
                        if item.get("reason") == "compiler-message" and item["message"]["level"] == "error":
                            errors.append(item["message"])
                    assert result.returncode != 0, f"{family} {instance}: forbidden {method}() compiled"
                    assert len(errors) == 1 and errors[0].get("code", {}).get("code") == "E0599", result.stdout + result.stderr
                    assert f"`{method}`" in errors[0]["message"], result.stdout + result.stderr
                    assert any(f"pac::{instance}.isr().{method}" in line["text"]
                               for span in errors[0]["spans"] for line in span["text"]), result.stdout + result.stderr
                    negative_count += 1
                    print(f"PASS {family} {instance}.ISR: compiler rejected {method}() with E0599")
    assert negative_count == 32
    print(f"PASS {len(EXPECTED)} read/RW controls and {negative_count} forbidden accesses; no firmware or MMIO executed")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-only", action="store_true", help="Verify curated sources/evidence without expecting regenerated output")
    parser.add_argument("--manual-dir", type=Path, help="Optionally recheck the five acquired manual PDF hashes")
    args = parser.parse_args()
    evidence = json.loads(EVIDENCE.read_text())
    check_sources(evidence, args.manual_dir)
    if not args.source_only:
        check_generated()
        check_pac()


if __name__ == "__main__":
    main()
