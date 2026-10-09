#!/usr/bin/env python3
"""Offline invariants for the independently reviewed official package pin maps."""
import importlib.util
import json
import re
import sys
import unittest
from pathlib import Path
import yaml

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "cw32-data/tools"))
GPIO = re.compile(r"P[A-F](?:0|[1-9][0-9]?)$")


def read(path):
    return yaml.safe_load(path.read_text()) if path.suffix == '.yaml' else json.loads(path.read_text())


class PhysicalPinoutTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.catalog = read(ROOT / "cw32-data/parts.yaml")
        cls.parts = {p["name"]: p for p in cls.catalog["parts"]}
        cls.families = {p["family"]: read(ROOT / "cw32-data/pinouts" / (p["family"].lower() + ".yaml")) for p in cls.parts.values()}
        cls.packages = {p["name"]: p for f in cls.families.values() for p in f["packages"]}

    def signals(self, part, position):
        return next(p["signals"] for p in self.packages[part]["pins"] if p["position"] == str(position))

    def test_all_37_exact_parts_and_13_families(self):
        self.assertEqual(len(self.parts), 37)
        self.assertEqual(len(self.families), 13)
        self.assertEqual(self.packages.keys(), self.parts.keys())
        self.assertEqual(sum(len(f["packages"]) for f in self.families.values()), 37)

    def test_hash_locked_page_evidence(self):
        for family, data in self.families.items():
            with self.subTest(family=family):
                self.assertEqual(data["schema_version"], 1)
                self.assertEqual(data["status"], "verified-from-official-datasheet")
                source = data["source"]
                locked = self.catalog["sources"][source["source_id"]]
                for field in ("url", "sha256", "document_filename"):
                    self.assertEqual(source[field], locked[field])
                self.assertRegex(source["sha256"], r"^[0-9a-f]{64}$")
                self.assertTrue(source["visually_reviewed_pdf_page_indices"])
                self.assertTrue(source["pinout_evidence"])
                self.assertEqual(sorted(set(r["pdf_page_index"] for r in data["table_rows"])), source["table_pdf_page_indices"])

    def test_upstream_package_pin_shape_and_complete_position_sets(self):
        for name, package in self.packages.items():
            with self.subTest(part=name):
                self.assertEqual(package["package"], self.parts[name]["package"])
                self.assertEqual(package["lead_count"], int(re.search(r"\d+", package["package"])[0]))
                positions = [p["position"] for p in package["pins"]]
                expected = package["additional_numbered_pads"] + [str(i) for i in range(1, package["lead_count"] + 1)]
                self.assertEqual(positions, expected)
                self.assertEqual(len(positions), len(set(positions)))
                seen_gpio = set()
                for pin in package["pins"]:
                    self.assertEqual(set(pin), {"position", "signals"})
                    self.assertIsInstance(pin["position"], str)
                    self.assertTrue(pin["signals"])
                    self.assertEqual(len(pin["signals"]), len(set(pin["signals"])))
                    for signal in pin["signals"]:
                        self.assertRegex(signal, r"^[A-Za-z][A-Za-z0-9_]*$")
                        if GPIO.fullmatch(signal):
                            self.assertNotIn(signal, seen_gpio)
                            seen_gpio.add(signal)
                self.assertEqual(seen_gpio, set(package["gpio_pins"]) | set(package["input_only_pins"]))

    def test_every_physical_pin_exactly_matches_its_source_table_column(self):
        for family, data in self.families.items():
            for package in data["packages"]:
                with self.subTest(part=package["name"]):
                    column = package["table_column"]
                    expected = [{"position": r["positions"][column], "signals": r["signals"]} for r in data["table_rows"] if r["positions"][column] is not None]
                    expected.sort(key=lambda p: int(p["position"]))
                    self.assertEqual(package["pins"], expected)
                    for row in data["table_rows"]:
                        self.assertEqual(list(row["positions"]), data["table_columns"])
                        self.assertIn(row["pin_type"], {"I", "O", "I/O", "S", "-"})

    def test_gpio_capabilities_follow_family_specific_type_rows(self):
        for data in self.families.values():
            for package in data["packages"]:
                with self.subTest(part=package["name"]):
                    selected = [r for r in data["table_rows"] if r["positions"][package["table_column"]] is not None]
                    gpio = {s for r in selected if r["pin_type"] == "I/O" for s in r["signals"] if GPIO.fullmatch(s)}
                    inputs = {s for r in selected if r["pin_type"] == "I" for s in r["signals"] if GPIO.fullmatch(s)}
                    debug = {s for r in selected if {"SWCLK", "SWDIO"} & set(r["signals"]) for s in r["signals"] if GPIO.fullmatch(s)}
                    self.assertEqual(set(package["gpio_pins"]), gpio)
                    self.assertEqual(package["gpio_count"], len(gpio))
                    self.assertEqual(set(package["input_only_pins"]), inputs)
                    self.assertTrue(gpio.isdisjoint(inputs))
                    self.assertEqual(set(package["debug_pins"]), debug)
                    self.assertEqual(set(package["gpio_pins_preserving_swd"]), gpio - debug)
                    self.assertEqual(len(debug), 2)

    def test_numbered_exposed_pads_are_not_lost_or_invented(self):
        expected = {"CW32F020K6U7", "CW32F030K8U7"}
        actual = {n for n, p in self.packages.items() if p["additional_numbered_pads"]}
        self.assertEqual(actual, expected)
        for name in expected:
            self.assertEqual(self.signals(name, 0), ["VSS"])
            self.assertEqual(len(self.packages[name]["pins"]), 33)

    def test_f030_package_distinctions_and_diagram_conflict(self):
        qfn = set(self.packages["CW32F030K8U7"]["gpio_pins"])
        lqfp = set(self.packages["CW32F030K8T7"]["gpio_pins"])
        self.assertEqual(qfn - lqfp, {"PB2", "PB8"})
        self.assertFalse(lqfp - qfn)
        self.assertEqual(self.signals("CW32F030K8U7", 31), ["PF3", "BOOT"])
        self.assertEqual(self.signals("CW32F030K8U7", 30), ["PB7"])
        self.assertEqual(self.signals("CW32F030K8T7", 32), ["VSS"])
        for pos, signal in [(17, "PA9"), (18, "PA10")]:
            self.assertEqual(self.signals("CW32F030F6P7", pos), [signal])
        for pos, signal in [(14, "PA11"), (15, "PA12")]:
            self.assertEqual(self.signals("CW32F030F8V7", pos), [signal])
        self.assertIn("incorrectly labels QFN32 position 31", " ".join(self.families["CW32F030"]["notes"]))

    def test_input_only_and_debug_pins_are_not_shared_family_assumptions(self):
        for name, package in self.packages.items():
            if self.parts[name]["family"] == "CW32L012":
                self.assertIn("PF3", package["gpio_pins"])
                self.assertNotIn("PF3", package["input_only_pins"])
            elif any("PF3" in p["signals"] for p in package["pins"]):
                self.assertNotIn("PF3", package["gpio_pins"])
                self.assertIn("PF3", package["input_only_pins"])
        self.assertEqual(self.signals("CW32L010Y8M6", 3), ["PB7", "NRST"])
        self.assertEqual(self.packages["CW32L010Y8M6"]["input_only_pins"], ["PB7"])
        self.assertEqual(self.packages["CW32L010F8P6"]["debug_pins"], ["PA7", "PA8"])
        for name in ("CW32F002F3P7", "CW32F003F4P7"):
            self.assertEqual(self.signals(name, 4), ["PC5", "NRST"])
            self.assertIn("PC5", self.packages[name]["gpio_pins"])
            self.assertEqual(self.packages[name]["debug_pins"], ["PA2", "PA5"])

    def test_l083_largest_package_is_not_gpio_superset(self):
        large = set(self.packages["CW32L083VCT6"]["gpio_pins"])
        small = set(self.packages["CW32L083RCT6"]["gpio_pins"])
        self.assertEqual(small - large, {"PF4", "PF5", "PF7"})
        self.assertEqual(self.signals("CW32L083VCT6", 74), ["VSS"])
        self.assertEqual(self.signals("CW32L083MCT6", 60), ["PF7"])

    def test_explicit_common_package_columns_do_not_erase_exact_names(self):
        for names in [
            ["CW32L031K8V6", "CW32L031K8U6"],
            ["CW32L052R8S6", "CW32L052R8T6"],
            ["CW32L083RBT6", "CW32L083RCS6", "CW32L083RCT6"],
            ["CW32L012C8T6", "CW32L012C8U6"],
        ]:
            for name in names[1:]:
                self.assertEqual(self.packages[names[0]]["pins"], self.packages[name]["pins"])
                self.assertNotEqual(self.packages[names[0]]["name"], self.packages[name]["name"])
        self.assertEqual(self.signals("CW32L031F8U6", 1), ["PC14", "OSC32_IN"])
        self.assertEqual(self.signals("CW32L031F8P6", 1), ["Vcore"])

    def test_rf_and_oscillator_pin_names_are_preserved(self):
        for name, position, signals in [
            ("CW32R031C8U6", 11, ["RFXC1"]),
            ("CW32R031C8U6", 15, ["ANT"]),
            ("CW32R031C8U6", 14, ["VSSRF"]),
            ("CW32W031R8U6", 44, ["VDDPA_LDO"]),
            ("CW32W031R8U6", 49, ["GPIO10"]),
            ("CW32W031R8U6", 50, ["GPIO11"]),
            ("CW32L010F8P6", 11, ["PB0", "OSC32_OUT"]),
            ("CW32L010F8P6", 12, ["PB1", "OSC32_IN"]),
        ]:
            self.assertEqual(self.signals(name, position), signals)
        self.assertNotIn("GPIO10", self.packages["CW32W031R8U6"]["gpio_pins"])
        rows = {r["source_name"]: r for r in self.families["CW32W031"]["table_rows"]}
        self.assertEqual(rows["GPIO10"]["source_type"], "I; O")
        self.assertEqual(rows["GPIO11"]["source_type"], "I; O")

    def test_normalization_and_no_af_number_claims(self):
        spec = importlib.util.spec_from_file_location("pinout_extraction", ROOT / "cw32-data/tools/extract_pinouts.py")
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)
        self.assertEqual(mod.normalize_signal("PA00"), "PA0")
        self.assertEqual(mod.normalize_signal("PF03"), "PF3")
        self.assertEqual(mod.normalize_signal("GPIO10"), "GPIO10")
        self.assertEqual(mod.normalize_signal("Vcore"), "Vcore")
        for family in self.families.values():
            for package in family["packages"]:
                for pin in package["pins"]:
                    self.assertNotIn("af", pin)


if __name__ == "__main__":
    unittest.main()
