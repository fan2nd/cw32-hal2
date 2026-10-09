#!/usr/bin/env python3
"""Own-source/data/PAC validation only; no HAL reads, harness, mocks or execution."""
from __future__ import annotations

import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import re
import zipfile

import fitz
import yaml

ROOT = Path(__file__).resolve().parents[1]
EVIDENCE_PATH = "docs/autotrim-counter-evidence.json"


def load(path):
    return yaml.safe_load(path.read_text()) if path.suffix == '.yaml' else json.loads(path.read_text())


def check(condition, message):
    if not condition:
        raise AssertionError(message)


def fingerprint(raw, spec, label):
    check(len(raw) == spec["bytes"], f"Source size differs: {label}")
    check(hashlib.sha256(raw).hexdigest() == spec["sha256"], f"Source SHA-256 differs: {label}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sources", type=Path,
                        default=Path(os.environ.get("CW32_SOURCES", "/workspace/shared/cw32-sources")))
    parser.add_argument("--extra-source-dir", type=Path, action="append", default=[],
                        help="Additional external cache, e.g. newly acquired own EN PDFs")
    args = parser.parse_args()
    source_roots = [args.sources, *args.extra_source_dir]
    evidence = load(ROOT / EVIDENCE_PATH)
    lock = load(ROOT / "sources/evidence-sources.json")
    artifacts = {a["id"]: a for a in lock["artifacts"]}
    artifact_bytes = {}
    source_bytes = {}

    def external(path):
        matches = [root / path for root in source_roots if (root / path).is_file()]
        check(bool(matches), f"Missing external source {path}; use --sources/--extra-source-dir")
        return matches[0].read_bytes()

    for key, source in evidence["sources"].items():
        artifact = artifacts[source["artifact_id"]]
        check(source["url"] == artifact["url"], f"Source URL disagrees with lock: {key}")
        check(EVIDENCE_PATH in artifact["evidence"], f"Missing canonical evidence reference: {key}")
        check(not artifact["license"]["bundle_raw"] and not artifact["license"]["bundle_derived_text"],
              f"Vendor redistribution policy changed: {key}")
        if artifact["id"] not in artifact_bytes:
            artifact_bytes[artifact["id"]] = external(artifact["path"])
            fingerprint(artifact_bytes[artifact["id"]], artifact, artifact["path"])
        raw = artifact_bytes[artifact["id"]]
        if "members" in source:
            pinned = next(m for m in artifact["members"] if m["path"] == source["path"])
            for field in ("path", "members", "sha256", "bytes"):
                check(source[field] == pinned[field], f"SDK member differs from lock: {key}/{field}")
            check(EVIDENCE_PATH in pinned["evidence"], f"Missing SDK member evidence reference: {key}")
            for member in source["members"]:
                with zipfile.ZipFile(io.BytesIO(raw)) as archive:
                    check(archive.namelist().count(member) == 1, f"Ambiguous/missing archive member: {member}")
                    raw = archive.read(member)
        else:
            for field in ("path", "sha256", "bytes"):
                check(source[field] == artifact[field], f"PDF source differs from lock: {key}/{field}")
        fingerprint(raw, source, key)
        source_bytes[key] = raw
    print(f"PASS: {len(source_bytes)} own-source hashes, {len(artifact_bytes)} official archives/PDFs, and SDK member chains")

    # Read exact cited physical PDF pages. Compact whitespace only; preserve the
    # table's symbols and values, including contradictory EN rows.
    def page(document, printed):
        text = re.sub(r"\s+", "", document[printed].get_text())
        check(text.startswith(str(printed) + "/"), f"Printed/PDF page mismatch: {printed}")
        return text

    for family, delta, en_cr, ds_page in (("CW32L052", 0, 186, 50), ("CW32L083", 12, 199, 54)):
        citations = evidence["citations"][family]
        cn = fitz.open(stream=artifact_bytes[citations["manual_cn"]["source_id"]], filetype="pdf")
        en = fitz.open(stream=artifact_bytes[citations["manual_en"]["source_id"]], filetype="pdf")
        ds = fitz.open(stream=artifact_bytes[citations["datasheet"]["source_id"]], filetype="pdf")
        check("Rev" + citations["manual_cn"]["printed_revision"] in page(cn, 174 + delta),
              f"Wrong own CN printed revision: {family}")
        cr = page(cn, 174 + delta)
        for claim in ("0000：保留，不可配置", "000：HSIOSC", "001：LSI", "010：HSE", "011：LSE", "100：ETR",
                      "00：HSIOSC校准模式", "01：LSI校准模式", "11：自动唤醒定时器模式", "AUTOTRIM_CR[11:1]"):
            check(claim in cr, f"Own CN CR claim differs: {family}: {claim}")
        timer = page(cn, 165 + delta)
        check("新设置的值将在下个定时或者计数周期开始起作用" in timer, f"Next-period ARR claim differs: {family}")
        check("T=(2PRS/RCLK)×(ARR+1)" in page(cn, 166 + delta), f"Own period formula differs: {family}")
        icr = page(cn, 177 + delta).split("11.8.7")[0]
        check("Resetvalue:0x0000003F" in icr and icr.count("R1W0") == 5 and icr.count("W1：无功能") == 5,
              f"Own CN ICR command semantics differ: {family}")
        check("2RFU-保留位，请保持默认值" in icr, f"ICR reserved-bit policy differs: {family}")
        check("15:0FCAPRO" in page(cn, 177 + delta), f"Own FCAP access differs: {family}")
        results = page(cn, 178 + delta)
        for claim in ("31:16RFU", "15:0TVALRO", "15:0FLIMRO", "0.4%"):
            check(claim in results, f"Own CN result-register claim differs: {family}: {claim}")
        check("Pleasewrite11" in page(en, en_cr), f"EN MD conflict disappeared: {family}")
        en_tval = page(en, en_cr + 4)
        check("31:6RFU" in en_tval and "15:0TVALRO" in en_tval, f"EN TVAL conflict disappeared: {family}")
        en_pin = artifacts[citations["manual_en"]["source_id"]]["provenance"]
        check(len(en) == en_pin["pdf_page_count"] and "Rev1.0" in page(en, en_cr),
              f"Wrong own EN printed revision/page count: {family}")
        history = page(en, en_pin["revision_history_evidence"]["printed_page"])
        history_date = "June20,2023" if family == "CW32L052" else "October10,2022"
        check(history_date in history, f"Wrong own EN printed history date: {family}")
        for edition in (cn, en):
            check("Rev" in edition[0].get_text(), f"Missing manual printed cover revision: {family}")
        factory = page(ds, ds_page)
        check("Rev" + citations["datasheet"]["printed_revision"] in factory, f"Wrong datasheet revision: {family}")
        for claim in ("表7-17", "fHSI频率--48-MHz", "TA=-40℃~+85℃-2.0-+2.0%", "TA=+25℃-0.5-+0.5%",
                      "HSI用户修正步长--0.2-%", "表7-18", "LSI用户修正步长--1-%"):
            check(claim in factory, f"Own datasheet clock claim differs: {family}: {claim}")
        cn.close()
        en.close()
        ds.close()
    print("PASS: cited own PDF pages, printed revisions, counter formula, reserved PRS, read-only results, R1W0 flags, factory tolerance and retained CN/EN conflicts")

    hardware = evidence["hardware"]
    expected_enums = {
        "Mode": {"HSI_CALIBRATION": 0, "LSI_CALIBRATION": 1, "TIMER": 3},
        "Source": {"HSI_OSC": 0, "LSI": 1, "HSE": 2, "LSE": 3, "ETR": 4},
        "Prescaler": {f"DIV{1 << n}": n for n in range(1, 16)},
    }
    flag_bits = {"END": 0, "OK": 1, "UD": 3, "OV": 4, "MISS": 5}
    sidecar = load(ROOT / "cw32-data/register-writes.yaml")["registers"]
    selected_chips = 0
    for short in ("cw32l052", "cw32l083"):
        name = "autotrim_" + short + "_v1"
        authored = yaml.safe_load((ROOT / "cw32-data/registers" / (name + ".yaml")).read_text())
        generated = load(ROOT / "cw32-data/data/registers" / (name + ".json"))
        regenerated_yaml = yaml.safe_load((ROOT / "cw32-data/data/registers" / (name + ".yaml")).read_text())
        check(authored == generated == regenerated_yaml, f"Canonical/generated AUTOTRIM data differ: {name}")
        items = {r["name"]: r for r in generated["block/AUTOTRIM"]["items"]}
        check(set(items) == {r["name"] for r in hardware["registers"]}, f"Register set differs: {name}")
        cmsis = source_bytes[short + "_h"].decode("utf-8", errors="replace")
        for register in hardware["registers"]:
            reg = register["name"]
            item = items[reg]
            access = "Read" if register["access"] == "RO" else "ReadWrite"
            check(item["byte_offset"] == register["offset"], f"Offset differs: {name}.{reg}")
            check(item.get("access", "ReadWrite") == access, f"Access differs: {name}.{reg}")
            fields = {f["name"]: (f["bit_offset"], f["bit_size"]) for f in generated["fieldset/" + reg]["fields"]}
            expected = {f["name"]: (f["offset"], f["width"]) for f in register["fields"]}
            check(fields == expected, f"Field layout differs: {name}.{reg}")
            for field, (offset, width) in fields.items():
                macro = f"AUTOTRIM_{reg}_{field}_Msk"
                match = re.search(r"#define\s+" + macro + r"\s+\((0x[0-9a-fA-F]+)UL\)", cmsis)
                check(match and int(match[1], 16) == ((1 << width) - 1) << offset,
                      f"Own CMSIS mask differs: {short}.{macro}")
        for field in ("IER", "ISR", "ICR"):
            check({f["name"]: f["bit_offset"] for f in generated["fieldset/" + field]["fields"]} == flag_bits,
                  f"Flag bits differ: {name}.{field}")
        for enum, expected in expected_enums.items():
            check({v["name"]: v["value"] for v in generated["enum/" + enum]["variants"]} == expected,
                  f"Typed encodings differ: {name}.{enum}")
        cr_fields = {f["name"]: f for f in generated["fieldset/CR"]["fields"]}
        for field, enum in (("MD", "Mode"), ("PRS", "Prescaler"), ("SRC", "Source")):
            check(cr_fields[field]["enum"] == enum, f"Untyped CR field: {name}.{field}")

        # Corroborate every mode/source/prescaler encoding against that family's own SDK.
        sdk = source_bytes[short + "_autotrim_h"].decode("utf-8", errors="replace")
        macro_groups = [
            ("MODE", {"HSIOSC": 0, "LSI": 1, "TIMECNT": 3}, 1),
            ("CLKSOURCE", {"HSIOSC": 0, "LSI": 1, "HSE": 2, "LSE": 3, "ETR": 4}, 8),
            ("PRS", {f"DIV{1 << n}": n for n in range(1, 16)}, 4),
        ]
        for group, values, shift in macro_groups:
            found = {m[1]: (int(m[2], 16), int(m[3])) for m in re.finditer(
                r"#define\s+AUTOTRIM_" + group + r"_(\w+)\s+\(\s*uint32_t\s*\)\s*\((0x[0-9a-fA-F]+)UL\s*<<\s*(\d+)\)", sdk)}
            check(found == {key: (value, shift) for key, value in values.items()}, f"Own SDK encodings differ: {short}.{group}")

        command = next(r for r in sidecar[name] if r["register"] == "ICR")
        check(command["block"] == "AUTOTRIM" and command["fieldset"] == "ICR", f"Wrong ICR sidecar target: {name}")
        check(command["reset_value"] == command["write_noop"] == 63, f"Wrong ICR command seed: {name}")
        check(set(command["zero_to_clear_fields"]) == set(flag_bits), f"Wrong zero-to-clear fields: {name}")
        check(all("autotrim-counter-evidence" in ref for ref in command["evidence"]), f"Untraceable ICR sidecar: {name}")

        pac = (ROOT / "cw32-metapac/src/peripherals" / (name + ".rs")).read_text()
        for register in hardware["registers"]:
            reg = register["name"]
            match = re.search(r"pub const fn " + reg.lower() + r"\(self\) -> crate::common::Reg<regs::\w+, crate::common::(R|RW)>\s*\{\s*unsafe \{ crate::common::Reg::from_ptr\(self.ptr.wrapping_add\((0x[0-9a-f]+)usize\)", pac)
            check(match and int(match[2], 16) == register["offset"], f"Generated PAC offset missing/different: {name}.{reg}")
            check(match[1] == ("R" if register["access"] == "RO" else "RW"), f"Generated PAC access differs: {name}.{reg}")
        for method in ("reset_value", "write_noop"):
            check(re.search(r"pub const fn " + method + r"\(\) -> Self\s*\{\s*Self\(63\)", pac),
                  f"Generated PAC explicit ICR command missing/different: {name}.{method}")
        check(re.search(r"impl Default for Icr\s*\{.*?Icr\(0\)", pac, re.S), f"Unexpected generic ICR Default: {name}")
        for field, enum in (("md", "Mode"), ("prs", "Prescaler"), ("src", "Source")):
            check(f"pub const fn {field}(&self) -> super::vals::{enum}" in pac, f"Untyped PAC field: {name}.{field}")
        for enum, reserved in (("Mode", [2]), ("Prescaler", [0]), ("Source", [5, 6, 7])):
            body = re.search(r"pub enum " + enum + r"\s*\{(.*?)\n    \}", pac, re.S)
            check(body is not None, f"Missing generated enum: {name}.{enum}")
            for value in reserved:
                check(re.search(r"_RESERVED_\w+\s*=\s*0x0*" + format(value, "x") + r",", body[1]),
                      f"Reserved encoding became usable: {name}.{enum}={value}")

        chips = sorted((ROOT / "cw32-data/data/chips").glob(short.upper() + "*.json"))
        check(bool(chips), f"No chip selections: {short}")
        for path in chips:
            core = load(path)["cores"][0]
            peripherals = {p["name"]: p for p in core["peripherals"]}
            timer = peripherals["AUTOTRIM"]
            check(timer["address"] == hardware["base_address"], f"Wrong AUTOTRIM address: {path.name}")
            check(timer["registers"] == {"block": "AUTOTRIM", "kind": "autotrim", "version": short + "_v1"},
                  f"Wrong own IP variant: {path.name}")
            check(timer["interrupts"] == [{"interrupt": "AUTOTRIM_LCD", "signal": "GLOBAL"}], f"Wrong shared IRQ: {path.name}")
            check(next(i["number"] for i in core["interrupts"] if i["name"] == "AUTOTRIM_LCD") == 30,
                  f"Wrong IRQ number: {path.name}")
            controls = timer["rcc_control"]
            check(controls["bus_clock"] == "PCLK" and controls["controller"] == "SYSCTRL", f"Wrong control clock: {path.name}")
            check(controls["enable"] == {"register": "APBEN2", "field": "AUTOTRIM"} and controls["enable_active_value"] is True,
                  f"Wrong enable control: {path.name}")
            check(controls["reset"] == {"register": "APBRST2", "field": "AUTOTRIM"} and controls["reset_asserted_value"] is False,
                  f"Wrong reset polarity: {path.name}")
            sysctrl = peripherals["SYSCTRL"]
            limits = sysctrl["clock_limits"]
            check(limits["hsi_frequency_hz"] == 48_000_000 and limits["hsi_error_percent"] == 2,
                  f"Wrong factory source envelope: {path.name}")
            check(limits["hsi_temperature_c"] == [-40, 85] and limits["hsi_supply_mv"] == [1650, 5500],
                  f"Wrong factory qualification: {path.name}")
            sys_ir = load(ROOT / "cw32-data/data/registers" / ("sysctrl_" + sysctrl["registers"]["version"] + ".json"))
            for fieldset, offset in (("APBEN2", 13), ("APBRST2", 13), ("DEBUG", 6)):
                field = next(f for f in sys_ir["fieldset/" + fieldset]["fields"] if f["name"] == "AUTOTRIM")
                check(field["bit_offset"] == offset and field["bit_size"] == 1, f"Wrong SYSCTRL field: {path.name}.{fieldset}")
            selected_chips += 1
    check(evidence["safe_scope"]["mode"] == 3 and evidence["safe_scope"]["auto"] == 0 and evidence["safe_scope"]["source"] == 0,
          "Safe evidence scope expanded beyond HSIOSC timer mode")
    print(f"PASS: two own-family register maps and SDK encodings; {selected_chips} chip selections; RO captures/status, ICR commands, typed PAC enums and shared IRQ metadata")
    print("Scope: source/data/PAC only; no HAL execution, firmware run or hardware qualification")


if __name__ == "__main__":
    main()
