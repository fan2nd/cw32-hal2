#!/usr/bin/env python3
"""Promote only the independently reviewed oscillator capture selection.

This is an explicit maintenance command, never part of normal generation.
It reads real generic capture files and emits the existing field-access format;
it does not reinterpret SVD access, change register IR or adopt other candidates.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import tempfile

import yaml

ROOT = Path(__file__).resolve().parents[2]
SELECTION = "docs/reviewed-status-access-selection.json"
RULES = "cw32-data/field-access.yaml"
IDENTITY = ("block", "register", "fieldset", "field")


def digest(data):
    return hashlib.sha256(data).hexdigest()


def load(path):
    return yaml.safe_load(path.read_text())


def write_candidate(path, text):
    # Validate everything before replacing any existing candidate; never follow
    # an output symlink into the authored tree or a captured input file.
    path = path.resolve()
    with tempfile.NamedTemporaryFile(mode="w", dir=path.parent, delete=False) as tmp:
        temp = Path(tmp.name)
        try:
            tmp.write(text)
            tmp.close()
            os.replace(temp, path)
        finally:
            temp.unlink(missing_ok=True)


def identity(row):
    return tuple(row[k] for k in IDENTITY)


def selected_rules(root, selection, capture_root):
    """Return rules whose identity and spans come from checked capture rows."""
    assert selection["schema_version"] == 1
    assert selection["classification"] == "source_correct_import_loss"
    assert selection["expected_source_access"] == "read-only"
    assert selection["containing_register_access"] == "ReadWrite"
    assert selection["containing_register_bit_size"] == 32
    fields = selection["fields"]
    assert len(fields) == 23 and sum(len(f["manual_rows"]) for f in fields) == 30
    assert len({(f["canonical_map"], identity(f)) for f in fields}) == 23
    lock = load(root / "sources/evidence-sources.json")
    artifacts = {a["id"]: a for a in lock["artifacts"]}
    members = {"member:" + m["path"]: (m, a) for a in lock["artifacts"] for m in a.get("members", [])}
    profiles = {p["line"]: p for path in (root / "cw32-data/inputs").glob("*.yaml") if (p := load(path))}
    captures = {}
    for family, source in selection["sources"].items():
        profile = profiles[family]
        svd = profile["source"]
        member, archive = members[source["svd_source_id"]]
        assert svd["source_ref"] == source["svd_source_id"]
        assert member["sha256"] == svd["sha256"] == source["svd_sha256"]
        assert svd["url"] == archive["url"] == source["archive_url"]
        manual = artifacts[source["manual_source_id"]]
        assert manual["sha256"] == source["manual_sha256"] and manual["url"] == source["manual_url"]
        relative = Path(source["capture_path"])
        assert relative == Path(f"{family}/field-access-candidates/{family}.json")
        data = (capture_root / relative).read_bytes()
        assert digest(data) == source["capture_sha256"], f"capture changed: {family}"
        capture = json.loads(data)
        assert capture["schema_version"] == 1 and capture["review_only"] is True
        assert capture["profile"] == family
        assert capture["source"] == {k: svd[k] for k in ("path", "sha256", "url")}
        captures[family] = capture
    assert set(captures) == {m["family"] for f in fields for m in f["manual_rows"]}
    output = {}
    bindings = []
    for index, selected in enumerate(fields):
        canonical = selected["canonical_map"]
        assert selected["block"] == "SYSCTRL" and selected["field"] == "STABLE"
        families = {m["family"] for m in selected["manual_rows"]}
        consumers = {name for name, p in profiles.items() if "sysctrl_" + p["register_versions"]["sysctrl"] == canonical}
        assert families == consumers, f"incomplete own-family selection: {canonical}"
        ir = load(root / f"cw32-data/registers/{canonical}.yaml")
        registers = [r for r in ir["block/SYSCTRL"]["items"] if r["name"] == selected["register"]]
        assert len(registers) == 1
        register = registers[0]
        assert register.get("access", "ReadWrite") == "ReadWrite" and register.get("bit_size", 32) == 32
        assert register["byte_offset"] == selected["register_byte_offset"]
        assert register["fieldset"] == selected["fieldset"] and not register.get("array")
        fs = ir["fieldset/" + selected["fieldset"]]
        matches = [f for f in fs["fields"] if f["name"] == selected["field"]]
        assert len(matches) == 1 and not matches[0].get("array")
        assert all(matches[0][k] == selected[k] for k in ("bit_offset", "bit_size"))
        rule = None
        for manual_row in selected["manual_rows"]:
            family = manual_row["family"]
            rows = [r for r in captures[family]["fields"] if r["peripheral"] == "SYSCTRL" and r["projection"] == {k: selected[k] for k in IDENTITY}]
            assert len(rows) == 1, f"capture selection is not exact: {family} {identity(selected)}"
            row = rows[0]
            path = ".".join((row["peripheral"], selected["register"], row["field"]))
            assert row["declaration"] == path and row["block"] == "SYSCTRL"
            assert all(row[k] == "read-only" for k in ("raw_access", "derived_access", "effective_access"))
            assert row["origin"] == {"access": "read-only", "level": "field", "path": path}
            assert row["register_path"] == [selected["register"]]
            assert row["register_offsets"] == [selected["register_byte_offset"]]
            assert row["dimensions"] == []
            assert all(row[k] == selected[k] for k in ("bit_offset", "bit_size"))
            assert all(v is None for kind in ("field_semantics", "register_semantics") for v in row[kind].values())
            # The reviewed selection gates adoption; the actual capture supplies
            # every emitted identity and bit span, including normalized names.
            captured = {**row["projection"], **{k: row[k] for k in ("bit_offset", "bit_size")}}
            assert rule is None or rule == captured, "shared-map captured field disagreement"
            rule = captured
            bindings.append({"family": family, "declaration": path,
                             "canonical_map": canonical, "selection_pointer": f"/fields/{index}",
                             "capture_row_sha256": digest(json.dumps(row, sort_keys=True, separators=(",", ":")).encode())})
        rule["evidence"] = [f"{SELECTION}#/fields/{index}: source-correct captured RO and independently reviewed own-manual RO for {', '.join(sorted(families))}; containing register remains 32-bit RW."]
        output.setdefault(canonical, []).append(rule)
    return output, bindings


def promote(text, additions):
    """Insert checked entries without rewriting any pre-existing YAML bytes."""
    original = yaml.safe_load(text)
    assert original["schema_version"] == 1
    pending = {}
    for canonical, rows in additions.items():
        existing = original["registers"].get(canonical, [])
        for row in rows:
            matches = [r for r in existing if identity(r) == identity(row)]
            assert len(matches) <= 1
            if matches:
                assert matches[0] == row, f"conflicting active rule: {canonical} {identity(row)}"
            else:
                pending.setdefault(canonical, []).append(row)
    assert sum(map(len, pending.values())) in (0, 23), "partial promotion requires review"
    result = text
    for canonical, rows in pending.items():
        rendered = "\n".join("  " + line for line in yaml.safe_dump(rows, sort_keys=False, width=120).rstrip().splitlines()) + "\n"
        match = re.search(r"^  " + re.escape(canonical) + r":\n", result, re.M)
        if match:
            next_map = re.search(r"^  [a-z0-9_]+:\n", result[match.end():], re.M)
            pos = match.end() + next_map.start() if next_map else len(result)
            result = result[:pos] + rendered + result[pos:]
        else:
            result += f"  {canonical}:\n" + rendered
    current = yaml.safe_load(result)
    for canonical, rows in original["registers"].items():
        assert current["registers"][canonical][:len(rows)] == rows
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--capture-root", required=True, type=Path, help="per-profile generic candidate output root")
    parser.add_argument("--output", type=Path, help="write a reviewable candidate YAML outside the authored input; default checks active rules")
    parser.add_argument("--receipt", type=Path, help="optional local input/output receipt outside normal generation")
    args = parser.parse_args()
    destinations = [p.resolve() for p in (args.output, args.receipt) if p]
    assert len(set(destinations)) == len(destinations), "output and receipt must be different files"
    for destination in destinations:
        assert not destination.is_relative_to(ROOT.resolve()), "candidate/receipt must stay outside the authored tree"
        assert not destination.is_relative_to(args.capture_root.resolve()), "candidate/receipt must not overwrite capture inputs"
    selection_data = (ROOT / SELECTION).read_bytes()
    selection = json.loads(selection_data)
    additions, bindings = selected_rules(ROOT, selection, args.capture_root)
    path = ROOT / RULES
    before = path.read_text()
    after = promote(before, additions)
    if args.output:
        write_candidate(args.output, after)
    else:
        assert before == after, "reviewed rules are absent; use --output to prepare a reviewable candidate"
    if args.receipt:
        receipt = {"selection_sha256": digest(selection_data), "capture_sources": selection["sources"],
                   "script_sha256": digest(Path(__file__).read_bytes()),
                   "source_parser_sha256": digest((ROOT / "cw32-data-gen/src/svd_access.rs").read_bytes()),
                   "rules_path": RULES, "input_sha256": digest(before.encode()), "output_sha256": digest(after.encode()),
                   "selected_rule_count": 23, "family_occurrence_count": len(bindings), "bindings": bindings}
        write_candidate(args.receipt, json.dumps(receipt, indent=2) + "\n")
    print("PASS: exactly 23 captured RO rules / 30 own-manual occurrences; all existing rules preserved")


if __name__ == "__main__":
    main()
