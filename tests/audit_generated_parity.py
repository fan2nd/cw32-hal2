#!/usr/bin/env python3
"""Independently compare every normalized register/field to the pinned vendor SVD.

Uses Python's XML parser, not the Rust importer or chiptool. The only accepted
source differences are explicit, evidence-bearing register overrides, field
removals, field-width corrections and IRQ supplements in the reviewed input manifest.
No firmware/hardware is exercised.
"""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import re
import xml.etree.ElementTree as ET
from reviewed_metadata import projection_errors, pin_projection
import yaml

ROOT = Path(__file__).resolve().parents[1]
ACCESS = {
    "read-only": "Read", "write-only": "Write", "writeOnce": "Write",
    "read-write": "ReadWrite", "read-writeOnce": "ReadWrite",
}


def number(value):
    return int(value, 0)


def inherited(nodes, name, default):
    return next((node.findtext(name) for node in nodes if node.findtext(name) is not None), default)


def field_span(field):
    if field.findtext("bitOffset") is not None:
        return number(field.findtext("bitOffset")), number(field.findtext("bitWidth"))
    if field.findtext("lsb") is not None:
        lo, hi = number(field.findtext("lsb")), number(field.findtext("msb"))
    else:
        match = re.fullmatch(r"\[(\d+):(\d+)\]", field.findtext("bitRange"))
        assert match, "Unsupported SVD field range"
        hi, lo = map(int, match.groups())
    return lo, hi - lo + 1


def expand_arrays(items, offset_key, first_index=0):
    """Compare every regular-array element with its original vendor scalar."""
    for item in items:
        array = item.get("array")
        if array is None:
            yield item
            continue
        assert set(array) == {"len", "stride"} and array["len"] > 0
        for index in range(array["len"]):
            element = dict(item)
            del element["array"]
            element["name"] = item["name"] + str(first_index + index)
            element[offset_key] = item[offset_key] + index * array["stride"]
            yield element


def audit(manifest_path, data):
    manifest = yaml.safe_load(manifest_path.read_text())
    if manifest.get("quarantine"):
        return {"family": manifest["expected_svd_name"], "quarantined": manifest["quarantine"]}
    classic_scan = None
    if manifest.get("classic_adc_scan_metadata"):
        catalog = yaml.safe_load((ROOT / manifest["classic_adc_scan_metadata"]).read_text())
        proof = json.loads((ROOT / catalog["evidence"]).read_text())
        classic_scan = next(f for f in proof["families"] if f["family"] == manifest["line"])
    source = ROOT / manifest["source"]["path"]
    assert hashlib.sha256(source.read_bytes()).hexdigest() == manifest["source"]["sha256"], source
    device = ET.parse(source).getroot()
    assert device.findtext("name") == manifest["expected_svd_name"]
    assert not device.findall(".//dim"), "This independent auditor must be extended before accepting SVD arrays"
    assert not device.findall(".//cluster"), "This independent auditor must be extended before accepting clusters"
    assert not device.findall(".//register[@derivedFrom]")
    assert not device.findall(".//field[@derivedFrom]")
    peripherals = {p.findtext("name"): p for p in device.findall("peripherals/peripheral")}
    corrections = {(c["block"], c["register"]): c for c in manifest.get("register_overrides", [])}
    assert all(c["evidence"] for c in corrections.values())
    assert len(corrections) == len(manifest.get("register_overrides", []))
    field_removals = {(c.get("block"), c["fieldset"], c["field"]): c for c in manifest.get("field_removals", [])}
    assert all(c["evidence"] for c in field_removals.values())
    assert len(field_removals) == len(manifest.get("field_removals", []))
    register_removals = {(c["block"], c["register"]): c for c in manifest.get("register_removals", [])}
    seen_register_removals = set()
    seen_field_removals = set()
    field_width_overrides = {(c["block"], c["fieldset"], c["field"]): c for c in manifest.get("field_width_overrides", [])}
    assert all(c["evidence"].strip() for c in field_width_overrides.values())
    assert len(field_width_overrides) == len(manifest.get("field_width_overrides", []))
    seen_field_width_overrides = set()
    seen_corrections, unique_blocks = set(), set()
    register_count = field_count = 0
    expected_interrupts = {}
    expected_metadata = {}
    for name, peripheral in peripherals.items():
        base = peripherals[peripheral.attrib["derivedFrom"]] if "derivedFrom" in peripheral.attrib else peripheral
        assert "derivedFrom" not in base.attrib, "Nested peripheral inheritance needs independent audit support"
        block = base.findtext("headerStructName", base.findtext("name"))
        kind = block.strip().lower()
        version = manifest.get("register_versions", {}).get(kind, manifest["register_version"])
        expected_metadata[name] = {
            "address": number(peripheral.findtext("baseAddress")),
            "kind": kind, "version": version, "block": block,
        }
        for irq in peripheral.findall("interrupt"):
            irq_name, irq_number = irq.findtext("name").strip().upper(), number(irq.findtext("value"))
            assert irq_name not in expected_interrupts or expected_interrupts[irq_name] == irq_number
            expected_interrupts[irq_name] = irq_number
        if kind in unique_blocks:
            continue
        unique_blocks.add(kind)
        ir = json.loads((data / "registers" / f'{kind}_{version}.json').read_text())
        items = list(expand_arrays(ir[f"block/{block}"]["items"], "byte_offset"))
        if kind == "adc" and classic_scan and classic_scan["results"]["common_result_offset"] is not None:
            common = next(i for i in items if i["name"] == "COMMONRESULT")
            assert common["byte_offset"] == classic_scan["results"]["common_result_offset"]
            common["name"] = "RESULT"
        generated_regs = {r["name"]: r for r in items}
        source_regs = {r.findtext("name"): r for r in base.findall("registers/register")}
        for (removed_block, removed_name), correction in register_removals.items():
            if removed_block == block:
                assert correction['evidence'].strip()
                removed = source_regs.pop(removed_name)
                assert number(removed.findtext('addressOffset')) == correction['expected_byte_offset']
                assert correction['expected_fieldset'] not in [r.get('fieldset') for r in generated_regs.values()]
                seen_register_removals.add((removed_block, removed_name))
        assert generated_regs.keys() == source_regs.keys(), f"{name}: missing or extra registers"
        register_count += len(source_regs)
        for reg_name, source_reg in source_regs.items():
            generated_reg = generated_regs[reg_name]
            context = f"{manifest['expected_svd_name']}:{block}.{reg_name}"
            size = number(inherited([source_reg, base, device], "size", "32"))
            access = ACCESS[inherited([source_reg, base, device], "access", "read-write")]
            assert generated_reg["byte_offset"] == number(source_reg.findtext("addressOffset")), context + " offset"
            assert generated_reg.get("bit_size", 32) == size, context + " width"
            if (block, reg_name) in corrections:
                correction = corrections[block, reg_name]
                assert access == correction["expected_access"], context + " stale override precondition"
                access = correction["access"]
                seen_corrections.add((block, reg_name))
            assert generated_reg.get("access", "ReadWrite") == access, context + " access"
            source_fields = source_reg.findall("fields/field")
            if not source_fields:
                assert "fieldset" not in generated_reg, context + " invented fieldset"
                continue
            fieldset = ir["fieldset/" + generated_reg["fieldset"]]
            assert fieldset.get("bit_size", 32) == size, context + " fieldset width"
            first_slot = 0
            if kind == "adc" and classic_scan:
                sequence_register = next((r for r in classic_scan["sequence"]["registers"] if r["name"] == reg_name), None)
                if sequence_register:
                    first_slot = sequence_register["slot_indices"][0]
            generated_fields = {f["name"]: f for f in expand_arrays(fieldset["fields"], "bit_offset", first_slot)}
            expected_fields, duplicate_names = {}, Counter()
            for source_field in source_fields:
                field_name = source_field.findtext("name").strip()
                duplicate_names[field_name] += 1
                if duplicate_names[field_name] > 1:
                    field_name += str(duplicate_names[field_name])
                expected_fields[field_name] = field_span(source_field)
            for (block_name, fieldset_name, field_name), removal in field_removals.items():
                if (block_name is not None and block_name != block) or fieldset_name != generated_reg["fieldset"]:
                    continue
                assert field_name in expected_fields, context + " missing removed source field"
                assert expected_fields[field_name] == (removal["expected_bit_offset"], removal["expected_bit_size"]), context + " stale field-removal precondition"
                del expected_fields[field_name]
                seen_field_removals.add((block_name, fieldset_name, field_name))
            for (block_name, fieldset_name, field_name), correction in field_width_overrides.items():
                if block_name != block or fieldset_name != generated_reg["fieldset"]:
                    continue
                assert field_name in expected_fields, context + " missing width-corrected source field"
                assert expected_fields[field_name] == (correction["expected_bit_offset"], correction["expected_bit_size"]), context + " stale field-width precondition"
                assert 0 < correction["bit_size"] != correction["expected_bit_size"], context + " invalid field-width correction"
                assert correction["expected_bit_offset"] + correction["bit_size"] <= size, context + " corrected field exceeds register"
                expected_fields[field_name] = (correction["expected_bit_offset"], correction["bit_size"])
                seen_field_width_overrides.add((block_name, fieldset_name, field_name))
            assert generated_fields.keys() == expected_fields.keys(), context + " missing or extra fields"
            for field_name, (offset, width) in expected_fields.items():
                actual = generated_fields[field_name]
                assert actual["bit_offset"] == offset, context + "." + field_name + " bit offset"
                assert actual["bit_size"] == width, context + "." + field_name + " bit width"
            field_count += len(expected_fields)
    assert seen_register_removals == register_removals.keys(), "Unapplied register removal"
    assert seen_corrections == corrections.keys(), "Unapplied register override"
    assert seen_field_removals == field_removals.keys(), "Unapplied field removal"
    assert seen_field_width_overrides == field_width_overrides.keys(), "Unapplied field-width correction"
    for irq in manifest.get("supplemental_interrupts", []):
        assert irq["evidence"]
        assert irq["name"] not in expected_interrupts or expected_interrupts[irq["name"]] == irq["number"]
        expected_interrupts[irq["name"]] = irq["number"]
    chips = {chip["name"]: chip for chip in manifest["chips"]}
    if manifest.get("parts_catalog"):
        catalog = yaml.safe_load((ROOT / manifest["parts_catalog"]).read_text())
        for part in catalog["parts"]:
            if part["family"] != manifest["line"]:
                continue
            assert part["memory_status"] == "verified-from-official-datasheet"
            assert part["source_ids"] and all(source in catalog["sources"] for source in part["source_ids"])
            expected = {"name": part["name"], "memory": [part["memory"]], "package": part["package"]}
            assert part["name"] not in chips or chips[part["name"]]["memory"] == expected["memory"]
            chips[part["name"]] = expected
    for chip in chips.values():
        generated_chip = json.loads((data / "chips" / f'{chip["name"]}.json').read_text())
        assert generated_chip["name"] == chip["name"]
        assert generated_chip["memory"] == chip.get("memory", [])
        if "package" in chip:
            assert generated_chip["packages"] == pin_projection(generated_chip, manifest)[0]
        topology_errors = projection_errors(generated_chip, manifest)
        assert not topology_errors, chip["name"] + ": " + "; ".join(topology_errors)
        core = generated_chip["cores"][0]
        expected_priority = manifest.get("nvic_priority_bits")
        if expected_priority is None:
            expected_priority = number(device.findtext("cpu/nvicPrioBits"))
        assert core["nvic_priority_bits"] == expected_priority
        actual_metadata = {p["name"]: {
            "address": p["address"], "kind": p["registers"]["kind"], "version": p["registers"]["version"], "block": p["registers"]["block"],
        } for p in core["peripherals"]}
        assert actual_metadata == expected_metadata, chip["name"] + " peripheral map mismatch"
        assert {i["name"]: i["number"] for i in core["interrupts"]} == expected_interrupts
    return {
        "family": manifest["line"], "source_device": manifest["expected_svd_name"], "chip_profiles": len(chips),
        "peripherals": len(peripherals), "register_blocks": len(unique_blocks),
        "registers": register_count, "fields": field_count, "interrupts": len(expected_interrupts),
        "reviewed_access_overrides": len(corrections), "reviewed_field_removals": len(field_removals),
        "reviewed_field_width_overrides": len(field_width_overrides), "passed": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data", type=Path, default=ROOT / "cw32-data/data")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    results = [audit(path, args.data) for path in sorted((ROOT / "cw32-data/inputs").glob("*.yaml"))]
    report = {"check": "Independent vendor SVD to generated JSON parity", "families": results, "hardware_tested": False}
    text = json.dumps(report, indent=2) + "\n"
    if args.output:
        args.output.write_text(text)
    print(text, end="")


if __name__ == "__main__":
    main()
