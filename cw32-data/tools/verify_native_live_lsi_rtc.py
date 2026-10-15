#!/usr/bin/env python3
"""Finite current-own-source/data qualification; no HAL execution or historical byte gate."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess

import yaml

POLICY = "cw32-data/rtc-live-lsi-qualified.yaml"
PROOF = "docs/native-live-factory-lsi-rtc-qualification.json"
LSI_POLICY = "cw32-data/lsi-sysclk-qualified.yaml"
PARTS = {
    "CW32L010": {"CW32L010F8P6": "TSSOP20", "CW32L010F8U6": "QFN20", "CW32L010Y8M6": "SOP16"},
    "CW32L011": {"CW32L011K8T6": "LQFP32", "CW32L011K8U6": "QFN32"},
    "CW32L012": {"CW32L012C8T6": "LQFP48", "CW32L012C8U6": "QFN48"},
}
RATES = {"CW32L010": (31816, 33784, 1620), "CW32L011": (29520, 41000, 1700), "CW32L012": (29520, 36080, 1700)}
QUALIFICATION = {
    "source": "LSI", "source_encoding": 2,
    "prescaler_first": 1, "prescaler_second": 16400, "calendar_divisor": 32800,
    "admission": "live_factory_reuse_only", "source_identity": "independent_of_sysclk",
    "factory_read_width_bits": 16, "factory_erased_raw_halfword": 65535,
    "factory_zero_valid": True, "software_lsien_required": True,
    "stable_required": True, "masked_factory_trim_match_required": True,
    "oscillator_writes": "forbidden", "rate_only": True, "cycle_timing_qualified": False,
    "attach_compensation_enabled": False, "initialization_compensation": "all_zero",
    "initialization": "unset_and_stopped_only", "l010_running_set_datetime": "UnqualifiedWriteTiming",
    "async_alarm_wait": "UnsupportedAsyncAlarmSource", "rtc_domain_loss": "ClockDomainUnavailable",
    "domain_loss_cleanup": "suppress_further_key_writes",
    "source_switch": "verify_source_before_reducing_first_divisor",
}
RTC_FIELDS = [
    ("KEY", 0, "KEY", 0, 8), ("CR0", 4, "START", 7, 1), ("CR0", 4, "H24", 3, 1),
    ("CR1", 8, "SOURCE", 8, 3), ("CR1", 8, "WAIT", 2, 1),
    ("PSC", 64, "PSC1", 20, 8), ("PSC", 64, "PSC2", 0, 20),
    ("COMPCFR1", 16, "EN", 15, 1), ("COMPCFR1", 16, "SIGN", 14, 1),
    ("COMPCFR1", 16, "PERIOD", 12, 2), ("COMPCFR1", 16, "COMP", 0, 12),
]


class UniqueLoader(yaml.SafeLoader):
    pass


def unique_mapping(loader, node, deep=False):
    result = {}
    for key_node, value_node in node.value:
        key = loader.construct_object(key_node, deep=deep)
        assert key not in result, f"duplicate key: {key}"
        result[key] = loader.construct_object(value_node, deep=deep)
    return result


UniqueLoader.add_constructor(yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG, unique_mapping)


def load(path):
    return yaml.load(path.read_text(), Loader=UniqueLoader)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def compact(value):
    return "".join(value.split())


def index(values, key):
    result = {}
    for value in values:
        identity = value[key]
        assert identity not in result, f"duplicate {key}: {identity}"
        result[identity] = value
    return result


def current_roster(root):
    parts = index(load(root / "cw32-data/parts.yaml")["parts"], "name")
    roster, included = {}, set()
    for path in sorted((root / "cw32-data/inputs").glob("*.yaml")):
        profile = load(path)
        entries = list(profile["chips"])
        if profile.get("parts_catalog"):
            assert profile["parts_catalog"] == "cw32-data/parts.yaml"
            selected = [p for p in parts.values() if p["family"] == profile["line"]]
            entries.extend(selected)
            included.update(p["name"] for p in selected)
        for entry in entries:
            assert entry["name"] not in roster
            roster[entry["name"]] = profile["line"]
    assert included == set(parts) and len(roster) == 54
    for family, exact in PARTS.items():
        assert {name for name, line in roster.items() if line == family} == set(exact) | {family}
        for name, package in exact.items():
            assert parts[name]["family"] == family and parts[name]["package"] == package
    # The historical catalog remains complete; only active generated support changes.
    from importlib.util import spec_from_file_location, module_from_spec
    spec = spec_from_file_location("current_targets", root / "tests/current_targets.py")
    support = module_from_spec(spec)
    spec.loader.exec_module(support)
    assert support.retired_targets(root) <= set(roster)
    return {name: line for name, line in roster.items() if not support.is_retired(name, root)}


def locked_bytes(sources, record):
    data = (sources / record["path"]).read_bytes()
    assert len(data) == record["bytes"] and sha(data) == record["sha256"], record["path"]
    return data


def source_pages(sources, record, fact, shift):
    locked_bytes(sources, record)
    assert record["provenance"]["status"] == fact["provenance_status"] == "selected"
    assert fact["sha256"] == record["sha256"]
    # Re-extract the actual locked PDF. Cached text is never the only source read.
    result = subprocess.run(["pdftotext", "-layout", str(sources / record["path"]), "-"],
                            check=True, capture_output=True)
    pages = result.stdout.decode("utf-8").split("\f")
    selected = sorted({n for v in fact["fact_pages"].values() for n in (v if isinstance(v, list) else [v])})
    assert fact["pdf_pages_1_based"] == selected
    assert fact["printed_pages"] == [n - shift for n in selected]
    assert all(0 < n <= len(pages) and pages[n - 1].strip() for n in selected)
    return [compact(p) for p in pages]


def contains(page, *tokens):
    for token in tokens:
        assert compact(token) in page, f"missing own-source page anchor: {token}"


def verify_sources(sources, lock, proof):
    artifacts = index(lock["artifacts"], "id")
    bindings = []
    for family, expected_parts in PARTS.items():
        own = proof["families"][family]
        refs = own["sources"]
        assert set(refs) == {"rm", "ds", "rtc_header"}
        texts = {}
        for kind in ("rm", "ds"):
            fact = refs[kind]
            record = artifacts[fact["source_ref"]]
            version = {"CW32L010": ("1.2", "1.3"), "CW32L011": ("1.1", "1.1"), "CW32L012": ("1.4", "1.0")}[family][kind == "ds"]
            assert record["path"] == f'{family}_{"UserManual" if kind == "rm" else "DataSheet"}_CN_V{version}.pdf'
            assert {POLICY, PROOF} <= set(record["evidence"])
            shift = (26 if family == "CW32L012" else 1) if kind == "rm" else (1 if family == "CW32L010" else 3)
            texts[kind] = source_pages(sources, record, fact, shift)
            bindings.append({"source_ref": fact["source_ref"], "sha256": record["sha256"], "pdf_pages_1_based": fact["pdf_pages_1_based"]})
        rm, ds = texts["rm"], texts["ds"]
        rp, dp = refs["rm"]["fact_pages"], refs["ds"]["fact_pages"]
        page = lambda key: rm[rp[key] - 1]
        contains(page("division"), "fRTCCLKD=fRTCCLK/(PSC1+1)", "fTICKCLK=fRTCCLKD/(PSC2+1)", "2Hz", "1MHz", "0x3FFF", "32768Hz", "1s")
        contains(page("source_wait"), "10:8SOURCE", "000：外部低速时钟LSE", "001：外部高速时钟HSE", "010：内部低速时钟LSI", "011：内部高速时钟HSIOSC", "2WAITRO")
        contains(page("prescaler"), "0x40", "27:20PSC1", "19:0PSC2")
        contains(page("key_start"), "7:0KEYWO", "7STARTRW", "3H24RW", "二分频")
        contains(page("compensation_fields"), "0x10", "15ENRW", "14SIGNRW", "13:12PERIODRW", "11:0COMPRW")
        trim = 9 if family == "CW32L012" else 10
        contains(page("lsi_fields"), "0x20", f"{trim-1}:0TRIM", "11:10WAITCYCLE", "15STABLERO", "0x001007C2", "0x001007C3")
        enable, request = rp["software_enable_and_requests"]
        contains(rm[enable - 1], "3LSIENRW")
        contains(rm[request - 1], "LSIEN位域不会被置位", "只有软件写", "IWDT")
        contains(page("access"), "RTC没有运行时", "所有寄存器可直接读写", "RTC_DATE", "RTC_TIME", "RTC_AWTARR", "WAIT为0")
        if family == "CW32L010":
            contains(page("access"), "无需通过ACCESS访问机制", "总执行时间不可超出1s")
            contains(page("source_wait"), "0ACCESSRW", "1WINDOWRO", "超过1秒")
            contains(page("rtclpm"), "RTCLPM")
        else:
            contains(page("source_wait"), "1:0RFU")
            assert "ACCESS" not in page("source_wait") and "WINDOW" not in page("source_wait")
        lsi_page = ds[dp["lsi"] - 1]
        contains(lsi_page, "7-19", "fLSI", "32.8", "ACCLSI", "0.16", "-40", "+85")
        contains(lsi_page, *(('-3', '+3') if family == "CW32L010" else ('-10', '+25') if family == "CW32L011" else ('-10', '+10')))
        contains(ds[dp["conditions"] - 1], "1.62" if family == "CW32L010" else "1.7", "5.5", "-40", "85")
        if family != "CW32L010": contains(ds[dp["conditions"] - 1], "VDDA", "须等于VDD")
        for name, package in expected_parts.items(): contains(ds[dp["parts"] - 1], name, package)
        if family == "CW32L011":
            contains(page("factory_calibration"), "32.8kHz±10%")
            contains(page("lsi_fields"), "30kHz~36kHz", "0.4%")
        header = refs["rtc_header"]
        archive_ref, member_path = {
            "CW32L010": ("vendor:CW32L010_StandardPeripheralLib_V1.0.9.zip",
                          "cw32l010/CW32L010_StandardPeripheralLib_V1.0.9/Libraries/inc/cw32l010_rtc.h"),
            "CW32L011": ("vendor:CW32L011_StandardPeripheralLib_V1.0.3.zip",
                          "cw32l011/Libraries/inc/cw32l011_rtc.h"),
            "CW32L012": ("vendor:CW32L012_StandardPeripheralLib_V1.0.5.zip",
                          "cw32l012/Libraries/inc/cw32l012_rtc.h"),
        }[family]
        assert header["artifact_ref"] == archive_ref and header["source_ref"] == "member:" + member_path
        assert header["role"] == "corroborating_source_encoding_only"
        archive = artifacts[header["artifact_ref"]]
        locked_bytes(sources, archive)
        member = next(m for m in archive["members"] if "member:" + m["path"] == header["source_ref"])
        assert header["sha256"] == member["sha256"] and {POLICY, PROOF} <= set(member["evidence"])
        text = locked_bytes(sources, member).decode("utf-8-sig")
        for name, value in [("LSE", 0), ("HSE", 1), ("LSI", 2), ("HSIOSC", 3)]:
            pattern = rf"#define\s+RTC_RTCCLK_FROM_{name}\s+\({value}UL" + (r"\s*<<\s*8\)" if value else r"\)")
            assert re.search(pattern, text), (family, name)
        bindings.append({"source_ref": header["source_ref"], "sha256": member["sha256"]})
    return bindings


def field(ir, block, register, offset, name, bit, width):
    item = index(ir["block/" + block]["items"], "name")[register]
    assert item["byte_offset"] == offset and "array" not in item
    assert item.get("access", "ReadWrite") == ("Write" if register == "KEY" else "ReadWrite")
    assert item.get("bit_size", 32) == 32
    f = index(ir["fieldset/" + item["fieldset"]]["fields"], "name")[name]
    assert f["bit_offset"] == bit and f["bit_size"] == width and "array" not in f
    if block == "RTC" and register == "CR1" and name == "SOURCE": assert "enum" not in f


def verify_data(root, data, policy, proof):
    roster = current_roster(root)
    paths = {p.stem: p for p in (data / "chips").glob("*.json")}
    assert set(paths) == set(roster), "all52 current chip membership mismatch"
    shared = load(root / LSI_POLICY)
    old_calendar = load(root / "cw32-data/rtc-calendar.yaml")["profiles"]
    alarms = load(root / "cw32-data/rtc-alarms.yaml")["profiles"]
    overlays = load(root / "cw32-data/field-access.yaml")["registers"]
    writes = load(root / "cw32-data/register-writes.yaml")["registers"]
    qualified, generic, other = [], [], []
    for name, path in sorted(paths.items()):
        chip = load(path)
        line = roster[name]
        assert chip["name"] == name and chip["line"] == line and len(chip["cores"]) == 1
        peripherals = index(chip["cores"][0]["peripherals"], "name")
        native_ip = any(p.get("registers", {}).get("kind") in ("rtc", "sysctrl") and p["registers"]["version"] in
                        ("cw32l010_v1", "cw32l011_v1", "cw32l012_v1") for p in peripherals.values())
        assert native_ip == (line in PARTS), "native RTC IP/family mismatch"
        if line not in PARTS:
            assert name not in {n for exact in PARTS.values() for n in exact}
            other.append(name)
            continue
        assert chip["family"] == "CW32L0"
        version = line.lower() + "_v1"
        for owner, kind, address in [("RTC", "rtc", 0x40004400), ("SYSCTRL", "sysctrl", 0x40004000)]:
            p = peripherals[owner]
            assert p["address"] == address and p["registers"] == {"block": owner, "kind": kind, "version": version}
        clock = peripherals["SYSCTRL"]["clock_limits"]
        assert sum("clock_limits" in p for p in peripherals.values()) == 1
        assert peripherals["RTC"]["rtc_calendar"] == old_calendar[line]
        old = old_calendar[line]
        assert old["source"] == "HSIOSC" and old["source_encoding"] == 3
        assert old["factory_trim_address"] == clock["factory_hsi_trim_address"] == 0x1007c0
        assert old["nominal_hz"] == old["calendar_divisor"] == clock["hsi_frequency_hz"]
        assert (old["prescaler_first"], old["prescaler_second"]) == (60 if line == "CW32L010" else 120, 400000)
        assert peripherals["RTC"]["rtc_alarms"]["async_wait"] == alarms[line]["async_wait"] is True
        assert peripherals["RTC"]["rtc_alarms"]["direct_register_access"] is True
        exact = name in PARTS[line]
        assert (clock.get("lsi_sysclk") is not None) == exact
        assert (clock.get("lse_configuration") is not None) == exact
        if exact:
            assert len(chip["packages"]) == 1 and chip["packages"][0]["name"] == name
            assert chip["packages"][0]["package"] == PARTS[line][name]
            lsi = clock["lsi_sysclk"]
            assert lsi == shared["families"][line]["lsi_sysclk"]
            minimum, maximum, supply = RATES[line]
            assert (lsi["nominal_hz"], lsi["minimum_hz"], lsi["maximum_hz"], lsi["supply_mv"], lsi["temperature_c"], lsi["factory_trim_address"]) == (32800, minimum, maximum, [supply, 5500], [-40, 85], 0x1007c2)
            assert lsi["rtc_allowed_sources"] == [0, 1, 2, 3]
            assert maximum <= QUALIFICATION["prescaler_first"] * 1_000_000
            assert lsi["nominal_hz"] == QUALIFICATION["prescaler_first"] * QUALIFICATION["prescaler_second"] * 2 == QUALIFICATION["calendar_divisor"]
            qualified.append(name)
        else:
            assert name == line and chip["packages"] == []
            generic.append(name)
    for line, exact in PARTS.items():
        own, family_proof = policy["families"][line], proof["families"][line]
        version = line.lower() + "_v1"
        trim = 9 if line == "CW32L012" else 10
        assert own == {"exact_parts": [{"name": n, "package": p} for n, p in exact.items()], "rtc_version": version,
                       "sysctrl_version": version, "envelope_pointer": f"/families/{line}/lsi_sysclk", "trim_bit_size": trim}
        assert own["exact_parts"] == shared["families"][line]["exact_parts"]
        assert family_proof["calibration"] == {"address": 0x1007c2, "read_width_bits": 16, "trim_bit_size": trim,
                                              "trim_mask": (1 << trim) - 1, "reject_raw_erased_before_mask": True, "zero_valid": True}
        assert family_proof["rate_reference"] == f"{LSI_POLICY}#/families/{line}/lsi_sysclk"
        minimum, maximum, _ = RATES[line]
        assert family_proof["calendar_rate_ratio"] == {"minimum": [minimum, 32800], "nominal": [32800, 32800], "maximum": [maximum, 32800]}
        assert family_proof["source_encoding"] == {"0": "LSE", "1": "HSE", "2": "LSI", "3": "HSIOSC"}
        assert family_proof["access"] == {
            "stopped": "direct_after_unlock_wait_still_required",
            "running_timing_registers": ["DATE", "TIME", "AWTARR"],
            "running_timing_protocol": "ACCESS_WINDOW_one_second" if line == "CW32L010" else "WAIT_direct_WAIT",
            "running_other_registers": "direct_after_unlock",
            "access_window_fields": line == "CW32L010",
        }
        for origin in [root / "cw32-data/registers", data / "registers"]:
            suffix = ".yaml" if origin == root / "cw32-data/registers" else ".json"
            rtc = load(origin / ("rtc_" + version + suffix))
            sysctrl = load(origin / ("sysctrl_" + version + suffix))
            for args in RTC_FIELDS: field(rtc, "RTC", *args)
            cr1 = index(rtc["fieldset/CR1"]["fields"], "name")
            for name, bit in [("ACCESS", 0), ("WINDOW", 1)]:
                assert (name in cr1) == (line == "CW32L010")
                if line == "CW32L010": field(rtc, "RTC", "CR1", 8, name, bit, 1)
            for args in [("CR1", 4, "LSIEN", 3, 1), ("LSI", 32, "TRIM", 0, trim),
                         ("LSI", 32, "WAITCYCLE", 10, 2), ("LSI", 32, "STABLE", 15, 1)]:
                field(sysctrl, "SYSCTRL", *args)
        stable = [f for f in overlays["sysctrl_" + version] if f["register"] == "LSI" and f["field"] == "STABLE"]
        assert len(stable) == 1 and (stable[0]["bit_offset"], stable[0]["bit_size"]) == (15, 1)
        icr = [r for r in writes["rtc_" + version] if r["register"] == "ICR"]
        assert len(icr) == 1 and icr[0]["write_noop"] == icr[0]["reset_value"] == 127
    assert len(qualified) == 7 and len(generic) == 3 and len(other) == 44
    return {"qualified_exact_parts": qualified, "excluded_native_generics": generic, "excluded_other_chips": other,
            "current_chip_count": len(roster), "preserved_hsiosc_profiles": 10}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--sources", type=Path, required=True)
    parser.add_argument("--data", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    root, sources, data = args.root.resolve(), args.sources.resolve(), args.data.resolve()
    policy, proof = load(root / POLICY), load(root / PROOF)
    assert set(policy) == {"schema_version", "source_authority", "evidence", "envelope_policy", "qualification", "families"}
    assert policy["schema_version"] == proof["schema_version"] == 1
    assert policy["source_authority"] == "sources/evidence-sources.json" and policy["evidence"] == PROOF
    assert policy["envelope_policy"] == LSI_POLICY and policy["qualification"] == QUALIFICATION
    assert set(policy["families"]) == set(proof["families"]) == set(PARTS)
    assert proof["policy"] == policy
    lock = load(root / "sources/evidence-sources.json")
    bindings = verify_sources(sources, lock, proof)
    report = verify_data(root, data, policy, proof)
    report.update({"status": "passed", "scope": "current own-source and data qualification only",
                   "policy_sha256": sha((root / POLICY).read_bytes()), "proof_sha256": sha((root / PROOF).read_bytes()),
                   "source_bindings": bindings, "hal_execution": False, "hardware_validation": False,
                   "implementation_acceptance": "not_established_by_this_gate"})
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n")
    print(f"native live-LSI RTC source/data: 7 qualified, 3 generic and 42 other chips excluded; {args.out}")


if __name__ == "__main__":
    main()
