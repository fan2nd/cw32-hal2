#!/usr/bin/env python3
"""Compare pinned vendor SVD addresses with compiled CMSIS C member layouts.

This script compiles only extracted typedef declarations plus an independently
written offsetof reporter. It never runs vendor startup/driver code or accesses
peripheral registers. Raw SDK contents are read in memory, never copied into the
repository. See docs/cmsis-layout-audit.md for scope and reproducibility.
"""
from __future__ import annotations

import argparse
import hashlib
import io
import json
from pathlib import Path
import re
import shlex
import subprocess
import tempfile
import urllib.request
import xml.etree.ElementTree as ET
import zipfile

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_MANIFEST = ROOT / "tests/cmsis-layout-sources.json"
STRUCT = re.compile(r"typedef\s+struct[^\{]*\{(.*?)\}\s*(\w+_TypeDef)\s*;", re.S)
MEMBER = re.compile(r"__(?:IOM|IM|OM|IO|I|O)\s+uint(?:8|16|32)_t\s+(\w+)(?:\[[^\]]+\])?\s*;")
POINTER = re.compile(r"#define\s+(?:CW|M0P)_(\w+)\s+\(\(\s*(\w+_TypeDef)\s*\*\s*\)\s*(\w+)_BASE\s*\)")
BASE = re.compile(r"#define\s+(\w+)_BASE\s+(0x[\dA-Fa-f]+)")
PREAMBLE = """#include <stdint.h>
#include <stdio.h>
#include <stddef.h>
#define __IOM volatile
#define __IM volatile const
#define __OM volatile
#define __IO volatile
#define __I volatile const
#define __O volatile
"""


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def checked(data: bytes, expected: str, label: str) -> bytes:
    actual = sha256(data)
    if actual != expected:
        raise ValueError(f"{label}: SHA-256 mismatch, expected {expected}, got {actual}")
    return data


def member_bytes(archive: bytes, spec: dict) -> bytes:
    data = archive
    for name in spec["members"]:
        with zipfile.ZipFile(io.BytesIO(data)) as source:
            data = source.read(name)
    return checked(data, spec["sha256"], " / ".join(spec["members"]))


def archive_bytes(directory: Path, item: dict, download: bool) -> bytes:
    spec = item["archive"]
    path = directory / spec["filename"]
    if not path.exists():
        if not download:
            raise FileNotFoundError(
                f"Missing {path}. Obtain the pinned official archive from {spec['url']} "
                "or explicitly pass --download-missing."
            )
        directory.mkdir(parents=True, exist_ok=True)
        with urllib.request.urlopen(spec["url"], timeout=180) as source:
            data = checked(source.read(), spec["sha256"], spec["filename"])
        path.write_bytes(data)
    return checked(path.read_bytes(), spec["sha256"], spec["filename"])


def compile_layout(header: str, compiler: list[str], work: Path, family: str) -> dict:
    declarations = []
    members = {}
    for match in STRUCT.finditer(header):
        typename = match.group(2)
        if typename in members:
            raise ValueError(f"Duplicate typedef {typename}")
        names = list(dict.fromkeys(MEMBER.findall(match.group(1))))
        members[typename] = [name for name in names if not name.startswith("RESERVED")]
        declarations.append(match.group(0))
    if not declarations:
        raise ValueError(f"{family}: no supported CMSIS struct definitions found")
    statements = []
    for typename, names in members.items():
        for name in names:
            statements.append(
                f'printf("{typename} {name} %zu\\n", offsetof({typename}, {name}));'
            )
    source = PREAMBLE + "\n".join(declarations) + "\nint main(void) {\n"
    source += "\n".join(statements) + "\nreturn 0;\n}\n"
    cfile = work / f"{family}.c"
    executable = work / f"{family}.out"
    cfile.write_text(source, encoding="utf-8")
    subprocess.run(compiler + ["-std=c11", str(cfile), "-o", str(executable)],
                   check=True, capture_output=True, text=True)
    output = subprocess.check_output([str(executable)], text=True)
    layouts = {}
    for line in output.splitlines():
        typename, member, offset = line.split()
        layouts.setdefault(typename, {})[member] = int(offset)
    return layouts


def audit_family(item: dict, archive: bytes, compiler: list[str], work: Path) -> dict:
    header_data = member_bytes(archive, item["header"])
    header = header_data.decode("utf-8-sig")
    svd = ET.fromstring(member_bytes(archive, item["svd"]))
    bases = {name: int(value, 16) for name, value in BASE.findall(header)}
    pointers = {name: (kind, base) for name, kind, base in POINTER.findall(header)}
    layouts = compile_layout(header, compiler, work, item["family"])
    peripherals = {node.findtext("name"): node for node in svd.findall("./peripherals/peripheral")}

    def registers(name: str, seen: frozenset[str] = frozenset()) -> dict:
        if name in seen:
            raise ValueError(f"Cyclic SVD derivedFrom: {name}")
        node = peripherals[name]
        result = registers(node.get("derivedFrom"), seen | {name}) if node.get("derivedFrom") else {}
        result.update({r.findtext("name"): int(r.findtext("addressOffset"), 0)
                       for r in node.findall("./registers/register")})
        return result

    comparisons = []
    missing = []
    for name, peripheral in peripherals.items():
        if name not in pointers:
            missing.append({"peripheral": name, "reason": "No matching supported header pointer macro"})
            continue
        typename, basename = pointers[name]
        if typename not in layouts or basename not in bases:
            raise ValueError(f"{item['family']}.{name}: missing parsed type or base")
        svd_registers = registers(name)
        header_registers = layouts[typename]
        svd_base = int(peripheral.findtext("baseAddress"), 0)
        header_base = bases[basename]
        shared = sorted(svd_registers.keys() & header_registers.keys())
        matches = []
        mismatches = []
        for register in shared:
            expected = header_base + header_registers[register]
            actual = svd_base + svd_registers[register]
            record = {"register": register, "svd_address": actual, "cmsis_address": expected}
            (matches if expected == actual else mismatches).append(record)
        comparisons.append({
            "peripheral": name,
            "header_type": typename,
            "matched_register_names": len(shared),
            "matches": matches,
            "absolute_address_mismatches": mismatches,
            "svd_only_register_names": sorted(svd_registers.keys() - header_registers.keys()),
            "header_only_register_names": sorted(header_registers.keys() - svd_registers.keys()),
        })
    count = sum(x["matched_register_names"] for x in comparisons)
    if count != item["expected_shared_register_instances"]:
        raise ValueError(f"{item['family']}: expected {item['expected_shared_register_instances']} comparisons, got {count}")
    return {"family": item["family"], "archive_sha256": item["archive"]["sha256"],
            "header_sha256": item["header"]["sha256"], "svd_sha256": item["svd"]["sha256"],
            "shared_register_instances": count,
            "mismatch_count": sum(len(x["absolute_address_mismatches"]) for x in comparisons),
            "skipped_peripherals": missing, "comparisons": comparisons}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sources", required=True, type=Path,
                        help="Directory containing the official pinned SDK ZIP archives")
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    parser.add_argument("--cc", default="cc", help="Host C compiler command, default: cc")
    parser.add_argument("--output", type=Path, help="Write the complete path-portable JSON result")
    parser.add_argument("--expect", type=Path, help="Compare the result with a saved JSON result")
    parser.add_argument("--download-missing", action="store_true",
                        help="Download missing official archives into --sources, verifying hashes")
    args = parser.parse_args()
    manifest_bytes = args.manifest.read_bytes()
    manifest = json.loads(manifest_bytes)
    results = []
    with tempfile.TemporaryDirectory(prefix="cw32-cmsis-layout-") as temp:
        for item in manifest["families"]:
            archive = archive_bytes(args.sources, item, args.download_missing)
            result = audit_family(item, archive, shlex.split(args.cc), Path(temp))
            results.append(result)
            print(f"{result['family']}: {result['shared_register_instances']} compared; {result['mismatch_count']} mismatches")
    report = {
        "schema_version": 1,
        "input_manifest_sha256": sha256(manifest_bytes),
        "method": "Hash-pinned official SVD absolute addresses versus compiled C offsetof from selected official CMSIS struct declarations",
        "limitations": [
            "Only register names shared by supported SVD/header representations are compared; skipped/one-sided names are reported.",
            "Host C ABI layout check for fixed-width integer structs; not a hardware execution or target silicon test.",
            "No bitfield positions, access permissions, reset values, reserved-bit behavior, atomicity or peripheral semantics are validated.",
            "The comparison targets raw pinned vendor sources, not generated Rust PAC parity or corrected register YAML.",
        ],
        "family_count": len(results),
        "shared_register_instances": sum(r["shared_register_instances"] for r in results),
        "mismatch_count": sum(r["mismatch_count"] for r in results),
        "families": results,
    }
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2) + "\n")
    if args.expect and report != json.loads(args.expect.read_text()):
        raise ValueError(f"Audit differs from saved result: {args.expect}")
    print(f"Total: {report['shared_register_instances']} shared registers across {report['family_count']} families; {report['mismatch_count']} mismatches")
    return int(report["mismatch_count"] != 0)


if __name__ == "__main__":
    raise SystemExit(main())
