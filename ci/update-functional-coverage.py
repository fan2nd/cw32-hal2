#!/usr/bin/env python3
"""Render maintained HAL scope against regenerated hardware, without build receipts."""
from __future__ import annotations

import argparse
from collections import Counter
from copy import deepcopy
import hashlib
import json
from pathlib import Path
import sys
import tomllib

import yaml

ROOT = Path(__file__).resolve().parents[1]
DECLARATIONS = "ci/hal-capabilities.yaml"
STATUSES = {
    "hardware_absent": "No such block or explicitly checked register-controlled mode in the reviewed hardware inventory.",
    "pac_only": "Register definitions exist, but no maintained HAL implementation scope is declared for this peripheral.",
    "unimplemented": "This API/integration is absent; unspecified hardware-mode presence remains unaudited.",
    "partial": "Only the explicitly maintained source-level scope is implemented, subject to the listed restrictions.",
    "experimental_partial": "Unsafe experimental scope with unresolved lifecycle restrictions.",
    "source_disputed": "A source conflict prevents exposing this mode; related APIs do not resolve it.",
    "route_unqualified": "Implementation or controller exists, but a required package/pad route is not qualified.",
    "deferred_by_user": "Hardware exists; the user has explicitly deferred this implementation.",
}
IMPLEMENTED = {"partial", "experimental_partial"}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


class Inventory:
    def __init__(self, root):
        self.root = root
        self.inputs = set()
        self.evidence_cache = {}
        self.yaml_cache = {}

    def read(self, name):
        path = self.root / name
        require(path.is_file(), f"Missing {name}; regenerate hardware with ./d gen when applicable")
        self.inputs.add(name)
        return path.read_text()

    def yaml(self, name):
        if name not in self.yaml_cache:
            self.yaml_cache[name] = yaml.safe_load(self.read(name))
        return self.yaml_cache[name]

    def validate_declarations(self, declarations, families):
        require(set(declarations) == {"schema_version", "scope", "families", "functions", "trigger_routes", "pin_policy"},
                "Unknown or missing declaration keys")
        function_keys = {"register_kinds", "evidence", "modes", "virtual", "virtual_instances"}
        mode_keys = {"id", "status", "scope", "limitations", "evidence", "families", "implemented_families",
                     "overrides", "pins_all", "pins_any", "pin_signal_sets", "facts", "registers_any"}
        for name, function in declarations["functions"].items():
            require(set(function) <= function_keys, f"Unknown function key: {name}")
            require(function["modes"], f"Empty function: {name}")
            require(function.get("virtual") in {None, "ir", "time_driver"}, f"Unknown virtual function: {name}")
            require(set(function.get("virtual_instances", {})) <= families, f"Unknown virtual family: {name}")
            self.evidence(function["evidence"])
            for mode in function["modes"]:
                require(set(mode) <= mode_keys, f"Unknown mode key: {name}/{mode.get('id')}")
                require(mode["status"] in STATUSES, f"Unknown status in {name}/{mode['id']}")
                for key in ("families", "implemented_families"):
                    require(set(mode.get(key, [])) <= families, f"Unknown family in {name}/{key}")
                self.evidence(mode.get("evidence", []))
                for override in mode.get("overrides", []):
                    require(set(override) <= {"families", "peripherals", "status", "scope", "limitations"}, "Unknown override key")
                    require(set(override["families"]) <= families, "Unknown override family")
                    if "peripherals" in override:
                        names = override["peripherals"]
                        require(isinstance(names, list) and names
                                and all(isinstance(n, str) and n for n in names)
                                and len(names) == len(set(names)), "Invalid override peripheral selector")
        policy = declarations["pin_policy"]
        require(set(policy["excluded_by_family"]) == families, "Pad exclusion scope must name every family")
        self.evidence(policy["evidence"])
        trigger_keys = {"family", "source", "source_event", "destination", "signal",
                        "status", "scope", "limitations", "evidence"}
        require(isinstance(declarations["trigger_routes"], list), "Trigger declarations must be a list")
        for route in declarations["trigger_routes"]:
            require(isinstance(route, dict) and set(route) == trigger_keys,
                    "Unknown or missing trigger declaration keys")
            require(route["family"] in families, "Unknown trigger declaration family")
            require(route["status"] in STATUSES, "Unknown trigger declaration status")
            require(all(isinstance(route[key], str) and route[key].strip()
                        for key in ("source", "source_event", "destination", "signal", "scope")),
                    "Trigger identity and scope must be nonempty strings")
            require(isinstance(route["limitations"], list) and route["limitations"]
                    and all(isinstance(item, str) and item.strip() for item in route["limitations"]),
                    "Trigger limitations must be nonempty strings")
            evidence = self.evidence(route["evidence"])
            require(evidence, "Trigger declarations require source evidence")
            if route["status"] in IMPLEMENTED:
                require(any(ref["path"].startswith("embassy-cw32/src/") for ref in evidence),
                        "Implemented trigger declaration requires driver evidence")

    def validate_projections(self, family, peripherals, profile):
        """Catch stale normalized fields used to qualify the bounded declarations."""
        sequences = self.yaml("cw32-data/adc-sequences.yaml")["profiles"]
        classic = self.yaml("cw32-data/classic-adc-scans.yaml")["profiles"]
        alarms = self.yaml("cw32-data/rtc-alarms.yaml")["profiles"]
        infrared = self.yaml("cw32-data/lvd-ir.yaml")["profiles"][family]["ir"]
        hse = self.yaml("cw32-data/hse-qualified.yaml")["families"].get(family, {}).get("hse")
        hex_input = self.yaml("cw32-data/hex-qualified.yaml")["families"].get(family, {}).get("hex")
        for p in peripherals:
            if p.get("registers", {}).get("kind") == "adc":
                sequence = sequences[family]["facts"] if family in sequences else classic[family]["sequence"]
                require(p.get("adc_limits", {}).get("sequence") == sequence, f"Stale ADC sequence on {family}/{p['name']}; run ./d gen")
                if family in classic:
                    require(p["adc_limits"].get("classic_scan") == classic[family]["facts"], f"Stale classic scan on {family}")
            if p.get("ir"):
                require(p["ir"] == infrared, f"Stale IR facts on {family}")
            if p["name"] == "RTC":
                a = alarms[family]
                expected = {"alarm_a_ignore_bit": a["alarm_a_ignore_bit"],
                            "alarm_b_configuration_supported": a["alarm_b_configuration_supported"],
                            "async_wait": a["async_wait"], "direct_register_access": a["alarm_register_access"] == "direct"}
                require(p.get("rtc_alarms") == expected, f"Stale RTC alarm facts on {family}")
            if p["name"] == "ATIM" and profile.get("atim_complementary_metadata"):
                expected = self.yaml(profile["atim_complementary_metadata"])["capability"]
                require(p.get("atim_complementary") == expected, f"Stale ATIM complementary facts on {family}")
            if p["name"] == "SYSCTRL":
                require(p.get("clock_limits", {}).get("hse") == hse, f"Stale HSE facts on {family}")
                require(p.get("clock_limits", {}).get("hex") == hex_input, f"Stale HEX facts on {family}")

    def evidence(self, refs):
        require(isinstance(refs, list), "Evidence must be a list of source references")
        result = []
        for ref in refs:
            require(isinstance(ref, dict) and set(ref) <= {"path", "anchor"}
                    and isinstance(ref.get("path"), str) and ref["path"], "Invalid source reference")
            require("anchor" not in ref or isinstance(ref["anchor"], str) and ref["anchor"], "Invalid source anchor")
            key = (ref["path"], ref.get("anchor"))
            if key not in self.evidence_cache:
                text = self.read(key[0])
                require(key[1] is None or key[1] in text, f"Stale source anchor: {key}")
                self.evidence_cache[key] = dict(ref, sha256=digest(self.root / key[0]))
            result.append(self.evidence_cache[key])
        return result

    def generate(self):
        declarations = self.yaml(DECLARATIONS)
        require(declarations["schema_version"] == 1, "Unsupported capability declaration schema")
        profiles = {}
        for path in sorted((self.root / "cw32-data/inputs").glob("*.yaml")):
            profile = self.yaml(path.relative_to(self.root).as_posix())
            require(not profile.get("quarantine"), f"Qualify quarantined input explicitly: {path.name}")
            profiles[profile["line"]] = profile
        require(set(profiles) == set(declarations["families"]), "Family inventory changed; review capability declarations")
        self.validate_declarations(declarations, set(profiles))
        features = tomllib.loads(self.read("embassy-cw32/Cargo.toml"))["features"]
        chips = {}
        for feature in sorted(f for f in features if f.startswith("cw32")):
            name = f"cw32-data/data/chips/{feature.upper()}.json"
            chip = json.loads(self.read(name))
            require(chip["line"] in profiles, f"Unknown family in {name}")
            chips[chip["name"]] = chip
        require(set(profiles) <= set(chips), "Missing normalized family profiles; run ./d gen")
        # Hash canonical YAML and generator inputs as well as consumed normalized data.
        # This fingerprint identifies inputs; it is not a proof of implementation.
        for folder, pattern in [("cw32-data", "*.yaml"), ("cw32-data-gen", "*.rs"),
                                ("cw32-data-serde", "*.rs"), ("embassy-cw32/src", "*.rs")]:
            for path in sorted((self.root / folder).rglob(pattern)):
                if "data" not in path.relative_to(self.root / folder).parts:
                    self.inputs.add(path.relative_to(self.root).as_posix())
        self.read("sources/evidence-sources.json")
        self.read("ci/update-functional-coverage.py")
        self.read("embassy-cw32/build.rs")
        self.read("Cargo.lock")
        self.read("Cargo.toml")
        self.read("cw32-data-gen/Cargo.toml")
        functions = declarations["functions"]
        registered = [kind for f in functions.values() for kind in f["register_kinds"]]
        require(len(registered) == len(set(registered)), "A register kind belongs to multiple function declarations")
        kinds = set(registered)
        rows = []
        peripherals = []
        hardware_inventory = {}
        for family in sorted(profiles):
            family_chip = chips[family]
            ps = family_chip["cores"][0]["peripherals"]
            hardware_inventory[family] = [{k: p[k] for k in ("name", "address", "registers") if k in p} for p in ps]
            self.validate_projections(family, ps, profiles[family])
            require(all(p.get("registers", {}).get("kind") in kinds for p in ps),
                    f"Undeclared peripheral kind in {family}; review scope rather than silently omitting it")
            selection_ps = {name: {p["name"]: p for p in chip["cores"][0]["peripherals"]}
                            for name, chip in chips.items() if chip["line"] == family}
            for function, declaration in functions.items():
                common_evidence = self.evidence(declaration["evidence"])
                instances = [p for p in ps if p.get("registers", {}).get("kind") in declaration["register_kinds"]]
                pac_instances = instances
                if declaration.get("virtual") == "ir":
                    instances = [p for p in ps if p.get("ir")]
                    require(len(instances) == 1, f"Missing or ambiguous IR hardware facts on {family}")
                elif declaration.get("virtual") == "time_driver":
                    name = "GTIM" if family in {"CW32F002", "CW32F003"} else "GTIM1"
                    instances = [p for p in ps if p["name"] == name]
                    require(len(instances) == 1, f"Missing reserved timebase {family}/{name}")
                elif "virtual_instances" in declaration:
                    instances = [{"name": n} for n in declaration["virtual_instances"].get(family, [])]
                for mode in declaration["modes"]:
                    for override in mode.get("overrides", []):
                        if family in override["families"] and "peripherals" in override:
                            require(set(override["peripherals"]) <= {p["name"] for p in instances},
                                    f"Unknown override peripheral in {family}/{function}/{mode['id']}")
                if not instances:
                    rows.append(dict(family=family, function=function, peripheral=None, mode=None,
                                     block_present=False, mode_hardware_present=False, status="hardware_absent",
                                     scope="No matching block in the reviewed family inventory.", limitations=[],
                                     hardware_input=f"cw32-data/data/chips/{family}.json",
                                     authored_input=f"cw32-data/inputs/{family.lower()}.yaml",
                                     evidence=common_evidence, hardware_validated=False))
                    continue
                for peripheral in instances:
                    name = peripheral["name"]
                    record = dict(family=family, function=function, peripheral=name,
                                  registers=peripheral.get("registers"), block_present=True,
                                  hardware_input=f"cw32-data/data/chips/{family}.json",
                                  authored_input=f"cw32-data/inputs/{family.lower()}.yaml")
                    if declaration.get("virtual") == "ir":
                        record["pac_instances_or_aliases"] = [p["name"] for p in pac_instances]
                    reg = peripheral.get("registers")
                    register_names = set()
                    if reg:
                        register_path = f"cw32-data/registers/{reg['kind']}_{reg['version']}.yaml"
                        definitions = self.yaml(register_path)
                        register_names = {item["name"] for key, block in definitions.items()
                                          if key.startswith("block/") for item in block["items"]}
                        record["register_source"] = register_path
                    # Report selected-chip routes without mistaking family alias intersections
                    # for the union of package support, or routes for functional implementation.
                    routes = {}
                    for chip, chip_ps in selection_ps.items():
                        p = chip_ps.get(name)
                        if p:
                            excluded = declarations["pin_policy"]["excluded_by_family"][family]
                            routes[chip] = sorted({pin["signal"] for pin in p.get("pins", []) if pin["pin"] not in excluded})
                    record["routed_signals_by_chip"] = routes
                    peripherals.append(record)
                    mode_ids = set()
                    for declared in declaration["modes"]:
                        require(declared["id"] not in mode_ids, f"Duplicate mode in {function}")
                        mode_ids.add(declared["id"])
                        if family not in declared.get("families", profiles):
                            continue
                        mode = deepcopy(declared)
                        for override in mode.pop("overrides", []):
                            if family in override["families"] and name in override.get("peripherals", [name]):
                                mode.update({k: v for k, v in override.items()
                                             if k not in {"families", "peripherals"}})
                        status = mode["status"]
                        require(status in STATUSES, f"Unknown status {status}")
                        require(mode["scope"] and mode["limitations"], f"Scope and limitations required: {function}/{mode['id']}")
                        evidence = common_evidence + self.evidence(mode.get("evidence", []))
                        physical = True if status in IMPLEMENTED | {"source_disputed", "deferred_by_user"} else None
                        if "implemented_families" in mode and family not in mode["implemented_families"]:
                            status = "unimplemented"
                            physical = None
                            mode["scope"] = "No implementation declared for this family/instance."
                        if "registers_any" in mode:
                            physical = bool(register_names.intersection(mode["registers_any"]))
                            if not physical:
                                status = "hardware_absent"
                                mode["scope"] = "No corresponding register-controlled mode in this family's reviewed register inventory."
                        qualifying = sorted(routes)
                        def accepts(signals):
                            signals = set(signals)
                            return (set(mode.get("pins_all", [])) <= signals
                                    and (not mode.get("pins_any") or bool(signals.intersection(mode["pins_any"])))
                                    and (not mode.get("pin_signal_sets") or any(set(s) <= signals for s in mode["pin_signal_sets"])))
                        pin_limited = any(k in mode for k in ["pins_all", "pins_any", "pin_signal_sets"])
                        if pin_limited:
                            qualifying = sorted(chip for chip, signals in routes.items() if accepts(signals))
                            if status in IMPLEMENTED and not qualifying:
                                status = "route_unqualified"
                                mode["scope"] = "Required package/pad signals are not qualified on any declared chip selection."
                        if status in IMPLEMENTED:
                            require(any(e["path"].startswith("embassy-cw32/src/") for e in evidence),
                                    f"Implementation declaration needs driver evidence: {function}/{mode['id']}")
                        if function == "ir" and mode["id"] == "mode_selector":
                            require((status in IMPLEMENTED) == peripheral["ir"]["mode_configurable"],
                                    f"IR selector declaration disagrees with hardware policy on {family}")
                        if function == "rtc" and mode["id"] == "alarm_async_wait":
                            require((status in IMPLEMENTED) == peripheral["rtc_alarms"]["async_wait"],
                                    f"RTC async declaration disagrees with hardware policy on {family}")
                        if function == "rtc" and mode["id"] == "alarm_b_programming":
                            policy = self.yaml("cw32-data/rtc-alarms.yaml")["profiles"][family]
                            require(status == "source_disputed" and not policy["alarm_b_configuration_supported"]
                                    and policy["alarm_b_mask_polarity"] == "unresolved_manual_sdk_conflict",
                                    f"Review changed Alarm B source policy on {family}")
                        facts = {}
                        for key in mode.get("facts", []):
                            value = peripheral
                            for component in key.split("."):
                                value = value.get(component) if isinstance(value, dict) else None
                            facts[key] = value
                        row = dict(family=family, function=function, peripheral=name, mode=mode["id"],
                                   block_present=True, mode_hardware_present=physical, status=status,
                                   scope=mode["scope"], limitations=mode["limitations"], evidence=evidence,
                                   hardware_input=record["hardware_input"], authored_input=record["authored_input"],
                                   register_source=record.get("register_source"), hardware_validated=False)
                        if pin_limited:
                            row["chip_selections_with_required_signals"] = qualifying
                        if facts:
                            row["hardware_facts"] = facts
                        rows.append(row)
        # Enumerate documented edges, including future edges with no implementation
        # declaration. Missing declarations never inherit support from a neighbour.
        route_declarations = {self.route_key(r): r for r in declarations["trigger_routes"]}
        require(len(route_declarations) == len(declarations["trigger_routes"]), "Duplicate trigger declaration")
        seen = set()
        for path in sorted((self.root / "cw32-data/triggers").glob("*.yaml")):
            relative = path.relative_to(self.root).as_posix()
            data = self.yaml(relative)
            for route in data["routes"]:
                hardware = dict(route, family=data["family"])
                key = self.route_key(hardware)
                seen.add(key)
                dec = route_declarations.get(key)
                rows.append(dict(family=hardware["family"], function="internal_trigger_interconnect",
                                 peripheral=hardware["destination"], mode=" <- ".join([hardware["signal"], hardware["source"] + "." + hardware["source_event"]]),
                                 block_present=True, mode_hardware_present=True,
                                 status=dec["status"] if dec else "unimplemented",
                                 scope=dec["scope"] if dec else "Documented route without a HAL scope declaration.",
                                 limitations=dec["limitations"] if dec else ["HAL ownership and runtime semantics are unqualified."],
                                 evidence=self.evidence(dec["evidence"] if dec else [{"path": relative}]),
                                 hardware_route=hardware, hardware_validated=False))
        require(set(route_declarations) <= seen, "Declared HAL trigger route missing from canonical hardware YAML")
        rows.sort(key=lambda r: (r["family"], r["function"], r["peripheral"] or "", r["mode"] or ""))
        fingerprints = {name: digest(self.root / name) for name in sorted(self.inputs)}
        fingerprint = hashlib.sha256(json.dumps(fingerprints, sort_keys=True).encode()).hexdigest()
        report = dict(schema_version=2, scope=declarations["scope"],
                      basis="Explicit maintained source declarations plus regenerated hardware inventory. No compiler receipts are required or interpreted as functional completeness.",
                      validation=dict(source_anchors="checked for existence only; semantic review remains required",
                                      compilation="not assessed by this inventory", silicon="not assessed; all hardware_validated values are false"),
                      status_definitions=STATUSES,
                      limitations=["No completion percentage or exhaustive advanced-mode hardware claim.",
                                   "mode_hardware_present=null means that specific mode has not been fully audited; block presence does not prove every mode.",
                                   "Pin signal availability is necessary, not proof of a simultaneously constructible board/pin combination. Exact chip and owner restrictions still apply.",
                                   "Generated hardware must be refreshed with ./d gen after authored input changes. Input hashes identify data used; they do not substitute for regeneration.",
                                   "No standalone BGR register does not imply absence of an internal bandgap.",
                                   "RF is explicitly user-deferred. SYSCTRL sleep/wake, most trigger routes and peripheral DMA beyond staged x030 UART TX and paired SPI master remain open."],
                      input_sha256=fingerprints, inventory_fingerprint=fingerprint,
                      families=sorted(profiles), hardware_inventory=hardware_inventory,
                      pin_policy=declarations["pin_policy"], peripherals=peripherals, rows=rows)
        summary = dict(schema_version=2, inventory_fingerprint=fingerprint,
                       detail_report="build/reports/hal-functional-coverage.json",
                       scope="Source declaration summary; no last_verified_drivers or historical build/test counts.",
                       hardware_validated=False, families={})
        for family in sorted(profiles):
            family_rows = [r for r in rows if r["family"] == family]
            instances = {}
            for p in (p for p in peripherals if p["family"] == family):
                rs = [r for r in family_rows if r["function"] == p["function"] and r["peripheral"] == p["peripheral"]]
                statuses = {r["status"] for r in rs}
                status = ("partial" if "partial" in statuses else "experimental_partial" if "experimental_partial" in statuses
                          else "deferred_by_user" if "deferred_by_user" in statuses else "pac_only" if p["registers"] else "unimplemented")
                p["status"] = status
                instances[f"{p['function']}:{p['peripheral']}"] = {"status": status, "modes": {r["mode"]:r["status"] for r in rs}}
            summary["families"][family] = dict(peripheral_scope=instances, mode_status_counts=dict(sorted(Counter(r["status"] for r in family_rows).items())))
        return {"hal-functional-coverage.json": report, "hal-coverage.json": summary}

    @staticmethod
    def route_key(route):
        return tuple(route[k] for k in ("family", "source", "source_event", "destination", "signal"))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="Fail if local generated reports differ")
    parser.add_argument("--validate-only", action="store_true", help="Validate source declarations without writing reports")
    args = parser.parse_args()
    reports = Inventory(ROOT).generate()
    for name, report in reports.items():
        path = ROOT / "build/reports" / name
        content = json.dumps(report, indent=2, ensure_ascii=False) + "\n"
        if args.check:
            require(path.is_file() and path.read_text() == content, f"Stale {path.relative_to(ROOT)}; regenerate coverage")
        elif not args.validate_only:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content)
    detail = reports["hal-functional-coverage.json"]
    print(json.dumps({"families": len(detail["families"]), "peripheral_functions": len(detail["peripherals"]),
                      "rows": len(detail["rows"]), "inventory_fingerprint": detail["inventory_fingerprint"],
                      "validation": "source inventory only; no compilation or runtime validation claimed"}))


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError, OSError) as error:
        sys.exit(str(error))
