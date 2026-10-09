#!/usr/bin/env python3
"""Reproduce reviewed physical package maps from SHA-256-locked official PDFs.

Requires pdfplumber and pdftotext. No downloads or routing inference are performed.
The PDF grid and Poppler text are extracted independently and their complete
position columns must agree. Run with --check to compare the committed results.
"""
import argparse
import hashlib
import json
import re
import statistics
import subprocess
from pathlib import Path
import yaml
from source_provenance import LOCK, enrich_source_refs

ROOT = Path(__file__).resolve().parents[2]
# Zero-based PDF page indices, and package columns in their physical left-to-right order.
SPECS = {
    "CW32A030": (list(range(21, 25)), ["LQFP48"], [21]),
    "CW32F002": ([21, 22], ["TSSOP20", "QFN20"], [21]),
    "CW32F003": ([22, 23], ["TSSOP24", "TSSOP20", "QFN20"], [22]),
    "CW32F020": (list(range(22, 26)), ["QFN48", "QFN32", "QFN20"], [20, 23]),
    "CW32F030": (list(range(23, 28)), ["LQFP48", "LQFP32", "TSSOP20", "QFN32", "QFN20"], [21, 24, 25]),
    "CW32L010": ([23, 24], ["TSSOP20", "QFN20", "SOP16"], [23]),
    "CW32L011": ([28, 29, 30], ["LQFP32", "QFN32"], [28]),
    "CW32L012": (list(range(34, 38)), ["LQFP48", "QFN48"], [37]),
    "CW32L031": (list(range(25, 29)), ["LQFP48", "QFN48", "QFN32", "QFN20", "TSSOP20"], [25]),
    "CW32L052": (list(range(25, 30)), ["LQFP64", "LQFP48"], [25]),
    "CW32L083": (list(range(26, 33)), ["LQFP100", "LQFP80", "LQFP64"], [27, 30, 31]),
    "CW32R031": (list(range(28, 31)), ["QFN48"], [28]),
    "CW32W031": (list(range(26, 30)), ["QFN64"], [28]),
}
GPIO = re.compile(r"P[A-F](?:0|[1-9][0-9]?)$")
OSCILLATOR = re.compile(r"\bOSC(?:32)?_(?:IN|OUT)\b")
NOTES = {
    "CW32F020": ["QFN32 has 32 perimeter leads plus exposed VSS pad numbered 0, explicitly present in Table 5-2 and Figure 5-2. Preserve position 0; do not renumber it 33."],
    "CW32F030": [
        "QFN32 has 32 perimeter leads plus exposed VSS pad numbered 0, explicitly present in Table 5-2 and Figure 5-4. Preserve position 0; do not renumber it 33.",
        "Figure 5-4 (PDF page index 21) incorrectly labels QFN32 position 31 PB07, duplicating position 30. Table 5-2 (PDF page index 27) explicitly assigns position 31 PF03/BOOT; the pin-definition table is authoritative here.",
        "LQFP32 lacks PB2 and PB8, which QFN32 exposes. F6P/TSSOP20 exposes PA9 and PA10; F8V/QFN20 instead exposes PA11 and PA12. Generic K8, F6, or F8 names must not imply a package pinout.",
    ],
    "CW32F002": ["PC5/NRST is documented as I/O; reset must be disabled through SYSCTRL_CR2.RSTIO before use as GPIO. SWD uses PA2 and PA5."],
    "CW32F003": ["PC5/NRST is documented as I/O; reset must be disabled through SYSCTRL_CR2.RSTIO before use as GPIO. SWD uses PA2 and PA5. PC3, PC4, PB7 and PA3 are bonded only in TSSOP24 among these packages."],
    "CW32L010": ["PB7/NRST is input-only even when reset is disabled through SYSCTRL_CR2.RSTIO. SWD uses PA7 and PA8. OSC32_OUT is PB0 and OSC32_IN is PB1."],
    "CW32L012": ["Unlike several older CW32 families, Table 5-2 explicitly marks PF3/BOOT I/O and lists digital output functions. Do not carry over an input-only PF3 restriction from F030 or L031."],
    "CW32L031": ["QFN32 4x4 mm and 5x5 mm share the single QFN32 pin-definition column. QFN20 and TSSOP20 have different GPIO sets and positions; QFN20 exposes PC14/PC15 and PB6, whereas TSSOP20 exposes PF0/PF1 and PA2/PA3."],
    "CW32L052": ["Both LQFP64 body sizes use the single LQFP64 pin-definition column; package names remain distinct in the exact-part catalog."],
    "CW32L083": ["Both LQFP64 body sizes and both memory variants use the single LQFP64 pin-definition column. PF4, PF5 and PF7 are absent from LQFP100 even though the smaller LQFP80/LQFP64 packages expose them; do not assume larger-package GPIO sets are supersets."],
    "CW32W031": ["GPIO10 and GPIO11 belong to the RF subsystem. Their type cells contain separate I and O functional entries, retained as source_type 'I; O'; these are not MCU GPIO port/pin names. Power, RF, antenna and regulator pins retain the vendor's names."],
    "CW32R031": ["IRQ, XTAL_OCLK, RFXC1, RFXC2 and ANT are RF-subsystem signals, not MCU GPIO port/pin names. Preserve VDDRF and VSSRF separately from MCU power signals."],
}


def center_x(word):
    return (word["x0"] + word["x1"]) / 2


def center_y(word):
    return (word["top"] + word["bottom"]) / 2


def normalize_signal(name):
    match = re.fullmatch(r"(P[A-F])([0-9]+)", name)
    return match[1] + str(int(match[2])) if match else name


def gpio_sort(name):
    return name[:2], int(name[2:])


def extract_rows(path, family):
    import pdfplumber
    pages, columns, _ = SPECS[family]
    rows = []
    with pdfplumber.open(path) as pdf:
        for index in pages:
            page = pdf.pages[index]
            words = page.extract_words()
            # Rotated package labels are extracted backwards by pdfplumber.
            tables = [t for t in page.find_tables() if any(
                marker in str(t.extract()[:2]) for marker in ("NFQ", "PFQL", "POSST")
            )]
            assert len(tables) == 1, (family, index, "ambiguous table")
            table = tables[0]
            words = [w for w in words if table.bbox[1] < center_y(w) < table.bbox[3]]
            gpio_words = [w for w in words if re.fullmatch(r"P[A-F][0-9]{2}/?", w["text"]) and center_x(w) < 300]
            name_center = statistics.median(center_x(w) for w in gpio_words)
            type_center = statistics.median(center_x(w) for w in words if w["text"] == "I/O" and name_center < center_x(w) < name_center + 70)
            half_width = (type_center - name_center) / 2
            for row in table.rows:
                if row.cells[0] is None:
                    continue
                box = row.cells[0]
                # Character centers avoid including text touching the previous row's border.
                selected = [w for w in words if box[1] < center_y(w) < box[3]]
                numbers = sorted([w for w in selected if center_x(w) < name_center - half_width and re.fullmatch(r"[0-9]+|-", w["text"])], key=center_x)
                if len(numbers) != len(columns):
                    continue  # table headers, not data rows
                names = sorted([w for w in selected if name_center - half_width < center_x(w) < name_center + half_width], key=lambda w: (round(center_y(w)), center_x(w)))
                source_name = "".join(w["text"] for w in names)
                assert re.fullmatch(r"[A-Za-z0-9_/]+", source_name), (family, index, source_name)
                types = sorted([w for w in selected if abs(center_x(w) - type_center) < 9], key=center_y)
                source_type = "; ".join(w["text"] for w in types)
                pin_type = "I/O" if source_type == "I; O" else source_type
                assert pin_type in ("I", "O", "I/O", "S", "-"), (family, source_name, source_type)
                structures = {w["text"] for w in selected if type_center + 10 < center_x(w) < type_center + 50 and w["text"] in ("TTa", "TC", "RST", "B", "RF")}
                assert len(structures) <= 1, (family, source_name, structures)
                signals = [normalize_signal(n) for n in source_name.split("/")]
                signals.extend(sorted(set(OSCILLATOR.findall(" ".join(w["text"] for w in selected)))))
                assert len(set(signals)) == len(signals), (family, source_name)
                rows.append({
                    "positions": {col: None if w["text"] == "-" else w["text"] for col, w in zip(columns, numbers)},
                    "source_name": source_name,
                    "signals": signals,
                    "source_type": source_type,
                    "pin_type": pin_type,
                    "io_structure": next(iter(structures), "-"),
                    "pdf_page_index": index,
                })
    # Independent extraction through Poppler must match every physical-position column.
    text_pages = subprocess.check_output(["pdftotext", "-layout", str(path), "-"], text=True).split("\f")
    pattern = re.compile(r"^\s*((?:\d+|-)(?:\s+(?:\d+|-)){" + str(len(columns) - 1) + r"})\s+(?:P[A-F]\d|V(?:DD|SS|core)|NRST|I(?:/O)?\s|O\s|GPIO|IRQ|XTAL|RF|ANT|DVDD|VLX|VFB|TX_)")
    positions = []
    for index in pages:
        for line in text_pages[index].splitlines():
            match = pattern.match(line)
            if match:
                positions.append([None if item == "-" else item for item in match[1].split()])
    assert positions == [list(row["positions"].values()) for row in rows], (family, "independent extraction mismatch")
    return rows


def build_family(path, family, catalog):
    pages, columns, reviewed_pages = SPECS[family]
    source_id = family + "_datasheet"
    source = catalog["sources"][source_id]
    assert hashlib.sha256(path.read_bytes()).hexdigest() == source["sha256"], (family, "source hash mismatch")
    rows = extract_rows(path, family)
    packages = []
    for part in catalog["parts"]:
        if part["family"] != family:
            continue
        column = re.match(r"[A-Z]+[0-9]+", part["package"])[0]
        assert column in columns, (family, part["package"])
        selected = [row for row in rows if row["positions"][column] is not None]
        pins = sorted([{"position": row["positions"][column], "signals": row["signals"]} for row in selected], key=lambda pin: int(pin["position"]))
        lead_count = int(re.search(r"[0-9]+", column)[0])
        extra = ["0"] if family in ("CW32F020", "CW32F030") and column == "QFN32" else []
        assert [p["position"] for p in pins] == extra + [str(i) for i in range(1, lead_count + 1)], (family, column, "pin count or duplicate position")
        gpio = sorted({s for row in selected if row["pin_type"] == "I/O" for s in row["signals"] if GPIO.fullmatch(s)}, key=gpio_sort)
        input_only = sorted({s for row in selected if row["pin_type"] == "I" for s in row["signals"] if GPIO.fullmatch(s)}, key=gpio_sort)
        debug = sorted({s for row in selected if {"SWDIO", "SWCLK"} & set(row["signals"]) for s in row["signals"] if GPIO.fullmatch(s)}, key=gpio_sort)
        memories = {m["name"]: m["size"] for m in part["memory"]}
        documented = [part["name"]]
        if part["name"] in ("CW32F030C8T7", "CW32F030F6P7"):
            documented.append(part["name"][:-1] + "6")  # Existing Table 9-1 orderable audit retained.
        packages.append({
            "name": part["name"],
            "key": part["name"][:-1].lower(),
            "family_part": part["name"][:-2],
            "package": part["package"],
            "table_column": column,
            "documented_orderable_parts": documented,
            "flash_bytes": memories["FLASH"],
            "ram_bytes": memories["RAM"],
            "lead_count": lead_count,
            "additional_numbered_pads": extra,
            "pins": pins,
            "gpio_pins": gpio,
            "gpio_count": len(gpio),
            "input_only_pins": input_only,
            "debug_pins": debug,
            "gpio_pins_preserving_swd": [p for p in gpio if p not in debug],
        })
    return {
        "schema_version": 1,
        "family": family,
        "status": "verified-from-official-datasheet",
        "source": {
            "source_id": source_id,
            "url": source["url"],
            "sha256": source["sha256"],
            "document_filename": source["document_filename"],
            "pinout_evidence": "Table 5-2: complete physical-position, pin-name and pin-type columns; oscillator aliases only when explicitly stated in the same row.",
            "table_pdf_page_indices": pages,
            "visually_reviewed_pdf_page_indices": reviewed_pages,
            "validation": "Independent PDF-grid and pdftotext position extraction; rendered-page review of per-family tables and ambiguous cases; not silicon-tested.",
        },
        "notes": [
            "Package pins retain physical position strings, including vendor-numbered exposed pads. A dash in the source is unbonded and becomes null only in table_rows; it is never emitted as a package pin.",
            "GPIO names are normalized by removing leading zeroes (PA00 to PA0). Other pin labels preserve source spelling. Direct pin-name aliases and OSC/OSC32 analog aliases are included; numbered alternate-function routing is outside this dataset.",
            "gpio_pins lists output-capable MCU GPIO pins according to this family's pin-type table, including debug/reset pads that require explicit reconfiguration. input_only_pins contains MCU GPIO names only. Neither list claims safe electrical configuration or silicon verification.",
            "Unnumbered thermal pads are not assigned invented positions or signals. Numbered pad 0 is included only where explicitly documented.",
        ] + NOTES.get(family, []),
        "table_columns": columns,
        "table_rows": rows,
        "packages": packages,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("sources", type=Path, help="directory containing the locked official PDFs")
    parser.add_argument("--check", action="store_true", help="compare parsed facts without modifying authored YAML")
    parser.add_argument("--family", choices=list(SPECS))
    args = parser.parse_args()
    catalog = yaml.safe_load((ROOT / "cw32-data/parts.yaml").read_text())
    lock = json.loads((ROOT / LOCK).read_text())
    count = 0
    for family in ([args.family] if args.family else SPECS):
        name = catalog["sources"][family + "_datasheet"]["document_filename"]
        candidates = [args.sources / name, args.sources / "current-datasheets" / name]
        expected = catalog["sources"][family + "_datasheet"]["sha256"]
        matching = [p for p in candidates if p.is_file() and hashlib.sha256(p.read_bytes()).hexdigest() == expected]
        assert matching, f"Missing hash-matching source for {family}: {expected}"
        data = enrich_source_refs(build_family(matching[0], family, catalog), lock)
        target = ROOT / "cw32-data/pinouts" / (family.lower() + ".yaml")
        if args.check:
            assert yaml.safe_load(target.read_text()) == data, f"Out-of-date pinout: {target}"
        else:
            target.write_text(yaml.safe_dump(data, allow_unicode=True, sort_keys=False, width=100))
        count += len(data["packages"])
        print(f"{family}: {len(data['table_rows'])} table rows, {len(data['packages'])} exact parts")
    print(f"{'Checked' if args.check else 'Wrote'} {count} exact-part package maps")


if __name__ == "__main__":
    main()
