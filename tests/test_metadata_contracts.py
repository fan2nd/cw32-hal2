#!/usr/bin/env python3
"""Metadata contracts and negative fixtures, independent of the Rust generators.

Read pinned vendor XML plus reviewed part/pinout manifests. These tests protect
source associations and represent unknown data as unknown; they do not establish
silicon correctness or substitute a manual review of the vendor sources.
"""
from copy import deepcopy
import hashlib
import json
import re
from pathlib import Path
import unittest
import xml.etree.ElementTree as ET
from reviewed_metadata import projection_errors, pin_projection, gpio_names
import yaml

ROOT = Path(__file__).resolve().parents[1]
DATA = ROOT / "cw32-data/data"


def load(path):
    return yaml.safe_load(path.read_text()) if path.suffix == '.yaml' else json.loads(path.read_text())


class ContractError(ValueError):
    pass


def require(condition, message):
    if not condition:
        raise ContractError(message)


def peripheral_contracts(manifest):
    source = ROOT / manifest["source"]["path"]
    require(source.is_file(), f"Missing pinned input {source}; run ./d fetch-sources")
    require(hashlib.sha256(source.read_bytes()).hexdigest() == manifest["source"]["sha256"],
            "Source hash mismatch")
    device = ET.fromstring(source.read_bytes())
    require(device.findtext("name") == manifest["expected_svd_name"], "Wrong source device")
    peripherals = {p.findtext("name"): p for p in device.findall("peripherals/peripheral")}
    contracts = {}
    for name, peripheral in peripherals.items():
        base = peripherals[peripheral.attrib["derivedFrom"]] if "derivedFrom" in peripheral.attrib else peripheral
        require("derivedFrom" not in base.attrib, "Nested inheritance needs explicit audit support")
        block = base.findtext("headerStructName", base.findtext("name"))
        contracts[name.strip().upper()] = {
            "address": int(peripheral.findtext("baseAddress"), 0),
            "registers": {"kind": block.strip().lower(), "version": manifest.get("register_versions", {}).get(block.strip().lower(), manifest["register_version"]), "block": block},
            # An inherited register block does not imply an inherited IRQ.
            "interrupts": sorted(("GLOBAL", irq.findtext("name").strip().upper(), int(irq.findtext("value"), 0))
                                 for irq in peripheral.findall("interrupt")),
        }
    for correction in manifest.get("interrupt_association_overrides", []):
        irq, number = correction["interrupt"], correction["number"]
        actual = sorted(name for name, value in contracts.items() if any(i[1] == irq for i in value["interrupts"]))
        require(correction["evidence"].strip() and actual == sorted(correction["expected_owners"]), "IRQ ownership correction lacks exact source evidence")
        require(all(owner in contracts for owner in correction["owners"]), "IRQ correction invents owner")
        require(all(i[2] == number for value in contracts.values() for i in value["interrupts"] if i[1] == irq), "IRQ correction changes vector")
        for name, value in contracts.items():
            value["interrupts"] = [i for i in value["interrupts"] if i[1] != irq]
            if name in correction["owners"]: value["interrupts"].append(("GLOBAL", irq, number))
            value["interrupts"].sort()
    return contracts


def validate_peripheral_contracts(chip, expected):
    require(len(chip["cores"]) == 1, "Current contract expects one core")
    core = chip["cores"][0]
    actual = {p["name"]: p for p in core["peripherals"]}
    require(len(actual) == len(core["peripherals"]), "Duplicate peripheral")
    require(actual.keys() == expected.keys(), "Missing or invented peripheral instance")
    interrupts = {irq["name"]: irq["number"] for irq in core["interrupts"]}
    require(len(interrupts) == len(core["interrupts"]), "Duplicate interrupt name")
    require(len(set(interrupts.values())) == len(interrupts), "Duplicate NVIC line")
    for name, contract in expected.items():
        peripheral = actual[name]
        require(peripheral["address"] == contract["address"], f"{name}: wrong base address")
        require(peripheral["registers"] == contract["registers"], f"{name}: incorrect peripheral register alias/reference")
        refs = peripheral["registers"]
        file = DATA / "registers" / f'{refs["kind"]}_{refs["version"]}.json'
        require(file.is_file(), f"{name}: missing register version")
        require("block/" + refs["block"] in load(file), f"{name}: unresolved register block")
        irqs = []
        for irq in peripheral.get("interrupts", []):
            require(irq["interrupt"] in interrupts, f"{name}: unresolved IRQ")
            irqs.append((irq["signal"], irq["interrupt"], interrupts[irq["interrupt"]]))
        require(sorted(irqs) == contract["interrupts"], f"{name}: wrong IRQ association")


def validate_reviewed_topology(chip):
    manifest = load(ROOT / 'cw32-data/inputs' / (chip['line'].lower() + '.yaml'))
    errors = projection_errors(chip, manifest)
    require(not errors, '; '.join(errors))


def validate_shared_source(source, target, alias):
    require(alias["source_line"] == source["line"] and alias["target_line"] == target["line"], "Wrong source alias edge")
    require(alias["allow_register_map_reuse"] is True, "Register-map reuse not approved")
    require(alias["independent_svd_available"] is False and alias["hardware_validated"] is False,
            "Shared source must not claim independent or hardware verification")
    require(target["expected_svd_name"] == source["expected_svd_name"], "Wrong aliased source device")
    require(target["source"] == source["source"], "Shared-map inputs differ")
    require(target["register_version"] == source["register_version"], "Shared-map version differs")
    require(target.get("register_versions", {}) == source.get("register_versions", {}), "Shared-map canonical versions differ")
    require(target.get("shared_map_evidence") and alias["evidence"], "Shared map requires evidence")
    require(all(e.get("url") and e.get("description") for e in alias["evidence"]), "Incomplete alias evidence")
    for field in ("register_overrides", "register_aliases", "supplemental_interrupts", "field_removals", "field_width_overrides"):
        require(target.get(field, []) == source.get(field, []), f"Shared-map correction mismatch: {field}")


class MetadataContracts(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.manifests = {m["line"]: m for p in sorted((ROOT / "cw32-data/inputs").glob("*.yaml"))
                         if not (m := load(p)).get("quarantine")}
        cls.chips = {p.stem: load(p) for p in sorted((DATA / "chips").glob("*.json"))}
        cls.expected = {line: peripheral_contracts(m) for line, m in cls.manifests.items()}
        cls.parts = load(ROOT / "cw32-data/parts.yaml")["parts"]
        cls.pinout = load(ROOT / "cw32-data/pinouts/cw32f030.yaml")

    def test_every_instance_matches_pinned_source(self):
        for name, chip in self.chips.items():
            with self.subTest(chip=name):
                validate_peripheral_contracts(chip, self.expected[chip["line"]])

    def test_wrong_existing_block_alias_is_rejected(self):
        chip = deepcopy(self.chips["CW32F030"])
        peripherals = {p["name"]: p for p in chip["cores"][0]["peripherals"]}
        # GPIOC is a valid block, but it is not GPIOA's IP block.
        peripherals["GPIOA"]["registers"] = deepcopy(peripherals["GPIOC"]["registers"])
        with self.assertRaisesRegex(ContractError, "incorrect peripheral register alias/reference"):
            validate_peripheral_contracts(chip, self.expected["CW32F030"])

    def test_wrong_existing_ip_version_is_rejected(self):
        chip = deepcopy(self.chips["CW32F030"])
        # Choose an existing incompatible version, even after equal IRs deduplicate.
        replacement = next(
            (peripheral, other["registers"])
            for peripheral in chip["cores"][0]["peripherals"]
            for other_chip in self.chips.values()
            for other in other_chip["cores"][0]["peripherals"]
            if peripheral["registers"]["kind"] == other["registers"]["kind"]
            and peripheral["registers"]["version"] != other["registers"]["version"]
        )
        replacement[0]["registers"] = deepcopy(replacement[1])
        with self.assertRaisesRegex(ContractError, "incorrect peripheral register alias/reference"):
            validate_peripheral_contracts(chip, self.expected["CW32F030"])

    def test_shared_irq_keeps_all_documented_owners(self):
        core = self.chips["CW32F030"]["cores"][0]
        for irq, owners in {"DMACH23": {"DMACHANNEL2", "DMACHANNEL3"},
                            "DMACH45": {"DMACHANNEL4", "DMACHANNEL5"},
                            "FLASHRAM": {"FLASH", "RAM"}, "WDT": {"IWDT", "WWDT"}}.items():
            with self.subTest(irq=irq):
                actual = {p["name"] for p in core["peripherals"]
                          if any(i["interrupt"] == irq for i in p.get("interrupts", []))}
                self.assertEqual(actual, owners)
                self.assertEqual(sum(i["name"] == irq for i in core["interrupts"]), 1)

    def test_wrong_but_resolvable_shared_irq_is_rejected(self):
        chip = deepcopy(self.chips["CW32F030"])
        dma3 = next(p for p in chip["cores"][0]["peripherals"] if p["name"] == "DMACHANNEL3")
        dma3["interrupts"][0]["interrupt"] = "DMACH45"
        with self.assertRaisesRegex(ContractError, "wrong IRQ association"):
            validate_peripheral_contracts(chip, self.expected["CW32F030"])

    def test_shared_source_requires_explicit_documented_edge(self):
        alias = load(ROOT / "cw32-data/register-source-aliases.yaml")["aliases"][0]
        validate_shared_source(self.manifests["CW32F030"], self.manifests["CW32A030"], alias)
        invalid = deepcopy(alias)
        invalid["source_line"] = "CW32F020"
        with self.assertRaisesRegex(ContractError, "Wrong source alias edge"):
            validate_shared_source(self.manifests["CW32F030"], self.manifests["CW32A030"], invalid)
        invalid = deepcopy(self.manifests["CW32A030"])
        invalid["register_overrides"] = []
        with self.assertRaisesRegex(ContractError, "correction mismatch"):
            validate_shared_source(self.manifests["CW32F030"], invalid, alias)

    def test_reviewed_topology_is_exact_and_unknowns_stay_absent(self):
        for name, chip in self.chips.items():
            with self.subTest(chip=name):
                validate_reviewed_topology(chip)

    def test_dma_peripheral_does_not_invent_request_routing(self):
        chip = deepcopy(self.chips["CW32F020"])
        peripheral_names = {p["name"] for p in chip["cores"][0]["peripherals"]}
        self.assertEqual({n for n in peripheral_names if n.startswith("DMACHANNEL")}, {"DMACHANNEL1", "DMACHANNEL2"})
        chip["cores"][0]["dma_channels"] = [{"name": "DMA1_CH1", "dma": "DMA", "channel": 1}]
        with self.assertRaisesRegex(ContractError, "Core DMA projection differs"):
            validate_reviewed_topology(chip)
        chip = deepcopy(self.chips["CW32F030"])
        uart = next(p for p in chip["cores"][0]["peripherals"] if p["name"] == "UART1")
        uart["dma_channels"] = [{"signal": "TX", "channel": "DMACHANNEL1"}]
        with self.assertRaisesRegex(ContractError, "UART1: DMA projection differs"):
            validate_reviewed_topology(chip)

    def test_exact_package_labels_and_memory_match_catalog(self):
        for part in self.parts:
            with self.subTest(part=part["name"]):
                chip = self.chips[part["name"]]
                self.assertEqual(chip["packages"], pin_projection(chip, self.manifests[chip["line"]])[0])
                self.assertEqual(chip["memory"], [part["memory"]])

    def test_documented_order_aliases_do_not_inflate_hardware_coverage(self):
        aliases = load(ROOT / "cw32-data/additional-parts.yaml")
        for alias in aliases["parts"] + aliases["shipping_sku_aliases"]:
            with self.subTest(alias=alias["name"]):
                self.assertFalse(alias["generate_chip_selection"])
                self.assertFalse(alias["count_as_distinct_hardware"])
                self.assertNotIn(alias["name"], self.chips)
                target = self.chips[alias["alias_of"]]
                self.assertEqual(alias["feature"], alias["alias_of"].lower())
                self.assertEqual(alias["recommended_base_pac_feature"], alias["feature"])
                self.assertEqual(target["packages"][0]["package"], alias["package"])
                self.assertEqual(target["memory"], [alias["memory"]])
                self.assertTrue(alias["evidence"])
                self.assertTrue(all(e["source_id"] in aliases["sources"] for e in alias["evidence"]))

    def test_package_pin_lists_are_distinct_from_die_capabilities(self):
        family_pins = set.union(*(gpio_names(p) for p in self.pinout["packages"]))
        self.assertTrue(self.pinout["source"]["sha256"] and self.pinout["source"]["pinout_evidence"])
        for package in self.pinout["packages"]:
            with self.subTest(package=package["key"]):
                pins = set(package["gpio_pins"])
                self.assertEqual(len(pins), len(package["gpio_pins"]))
                self.assertEqual(len(pins), package["gpio_count"])
                self.assertTrue(pins <= family_pins)
                self.assertTrue(pins.isdisjoint(package["input_only_pins"]))
                self.assertEqual(set(package["gpio_pins_preserving_swd"]), pins - set(package["debug_pins"]))
                self.assertNotIn("PF3", pins)
                catalogued = set(package["documented_orderable_parts"]) & self.chips.keys()
                self.assertTrue(catalogued, "No generated part for this reviewed package")
                for name in catalogued:
                    chip = self.chips[name]
                    self.assertEqual(chip["packages"][0]["package"], package["package"])
                    memory = {m["name"]: m["size"] for m in chip["memory"][0]}
                    self.assertEqual(memory, {"FLASH": package["flash_bytes"], "RAM": package["ram_bytes"]})
                    self.assertEqual(chip["packages"][0]["pins"], package["pins"])
                    self.assertEqual({p["name"] for p in chip["cores"][0]["pins"]}, gpio_names(package))

    def test_package_ambiguous_aliases_do_not_collapse_distinct_pinouts(self):
        packages = {p["key"]: set(p["gpio_pins"]) for p in self.pinout["packages"]}
        self.assertEqual(packages["cw32f030k8u"] - packages["cw32f030k8t"], {"PB2", "PB8"})
        self.assertEqual(packages["cw32f030k8t"] - packages["cw32f030k8u"], set())
        self.assertEqual(packages["cw32f030f6p"] - packages["cw32f030f8v"], {"PA9", "PA10"})
        self.assertEqual(packages["cw32f030f8v"] - packages["cw32f030f6p"], {"PA11", "PA12"})
        self.assertEqual(self.chips["CW32F030K8"]["packages"], [])

    def test_canonical_ip_reuse_ledger_hashes_and_mapping(self):
        ledger = load(ROOT / "cw32-data/register-reuse.yaml")
        groups = ledger["groups"]
        self.assertEqual(len(groups), ledger["canonical_template_count"])
        source_mapping = {}
        for group in groups:
            canonical = group["canonical"]
            ir = load(DATA / "registers" / canonical.replace(".yaml", ".json"))
            digest = hashlib.sha256(json.dumps(ir, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
            self.assertEqual(digest, group["canonical_ir_sha256"], canonical)
            self.assertTrue((ROOT / "cw32-data/registers" / canonical).is_file())
            for original in group["source_versions"]:
                self.assertNotIn(original, source_mapping, "Source IP assigned twice")
                self.assertEqual(original.split("_", 1)[0], canonical.split("_", 1)[0], "Cross-kind alias")
                source_mapping[original] = canonical
        self.assertEqual(len(source_mapping), ledger["source_template_count"])
        observed_sources = set()
        for line, manifest in self.manifests.items():
            for peripheral in self.expected[line].values():
                kind = peripheral["registers"]["kind"]
                original = f"{kind}_{manifest['register_version']}.yaml"
                canonical = f"{kind}_{peripheral['registers']['version']}.yaml"
                self.assertEqual(source_mapping[original], canonical)
                observed_sources.add(original)
        self.assertEqual(observed_sources, source_mapping.keys())

    def test_f020_crc_has_only_sixteen_valid_result_bits(self):
        manifest = self.manifests["CW32F020"]
        self.assertEqual(manifest["register_versions"]["crc"], "cw32f020_v1")
        corrections = manifest["field_width_overrides"]
        self.assertEqual(len(corrections), 1)
        correction = corrections[0]
        self.assertEqual((correction["block"], correction["fieldset"], correction["field"]),
                         ("CRC", "RESULT32", "RESULT32"))
        self.assertEqual((correction["expected_bit_offset"], correction["expected_bit_size"], correction["bit_size"]),
                         (0, 32, 16))
        self.assertIn("section10.6.3", correction["evidence"])
        ir = load(DATA / "registers/crc_cw32f020_v1.json")
        registers = {r["name"]: r for r in ir["block/CRC"]["items"]}
        for name, width in (("DR8", 8), ("DR16", 16), ("DR32", 32)):
            self.assertEqual(registers[name].get("bit_size", 32), width)
            self.assertEqual(ir["fieldset/" + name]["fields"][0]["bit_size"], width)
        for name in ("RESULT16", "RESULT32"):
            self.assertEqual(registers[name]["byte_offset"], 12)
            self.assertEqual(registers[name]["access"], "Read")
            self.assertEqual(ir["fieldset/" + name]["fields"][0]["bit_size"], 16)
        self.assertEqual(registers["RESULT32"].get("bit_size", 32), 32,
                         "The corrected field must not remove 32-bit bus access")
        self.assertEqual(ir["fieldset/CR"]["fields"][0]["bit_size"], 4,
                         "F020 MODE is physically four bits despite only modes0-7 being documented")
        x030 = load(DATA / "registers/crc_v1.json")
        self.assertEqual(x030["fieldset/RESULT32"]["fields"][0]["bit_size"], 32,
                         "F020 correction must not narrow other families")

    def test_static_metadata_shape_matches_pinned_upstream(self):
        # Pinned stm32-data 37a22f3, stm32-metapac-gen/res/src/metadata/mod.rs.
        # Package records belong in chip JSON, not an additive static API.
        expected = {
            "name": "&'staticstr", "family": "&'staticstr", "line": "&'staticstr",
            "memory": "&'static[&'static[MemoryRegion]]",
            "peripherals": "&'static[Peripheral]", "nvic_priority_bits": "Option<u8>",
            "interrupts": "&'static[Interrupt]", "dma_channels": "&'static[DmaChannel]",
            "pins": "&'static[Pin]",
        }
        for path in (ROOT / "cw32-metapac-gen/res/src/metadata.rs", ROOT / "cw32-metapac/src/metadata.rs"):
            with self.subTest(path=str(path.relative_to(ROOT))):
                body = re.search(r"pub struct Metadata \{(.*?)\n\}", path.read_text(), re.S).group(1)
                fields = dict(re.findall(r"pub (\w+): ([^\n]+),", body))
                self.assertEqual({k: re.sub(r"\s+", "", v) for k, v in fields.items()}, expected)

    def test_analog_muxes_are_explicit_peripheral_pin_schema_extensions(self):
        expected = {"pin": "&'staticstr", "signal": "&'staticstr", "af": "Option<u8>", "adc_mux": "Option<u8>", "comparator_mux": "Option<u8>"}
        for path in (ROOT / "cw32-metapac-gen/res/src/metadata.rs", ROOT / "cw32-metapac/src/metadata.rs"):
            body = re.search(r"pub struct PeripheralPin \{(.*?)\n\}", path.read_text(), re.S).group(1)
            fields = dict(re.findall(r"pub (\w+): ([^\n]+),", body))
            self.assertEqual({k: re.sub(r"\s+", "", v) for k, v in fields.items()}, expected)

    def test_r031_logical_source_cannot_substitute_for_hardware_mux(self):
        for key, value in (("adc_mux", 0), ("signal", "IN4"), ("adc_mux", None)):
            chip = deepcopy(self.chips["CW32R031C8U6"])
            adc = next(p for p in chip['cores'][0]['peripherals'] if p['name'] == 'ADC')
            route = next(p for p in adc['pins'] if p['pin'] == 'PA4')
            self.assertEqual((route['signal'], route['adc_mux']), ('IN0', 4))
            if value is None: route.pop(key)
            else: route[key] = value
            with self.assertRaisesRegex(ContractError, 'ADC: AF projection differs'):
                validate_reviewed_topology(chip)

    def test_r031_comparator_mux_is_not_its_logical_source_number(self):
        chip = deepcopy(self.chips["CW32R031C8U6"])
        vc = next(p for p in chip['cores'][0]['peripherals'] if p['name'] == 'VC1')
        route = next(p for p in vc['pins'] if p['signal'] == 'INP0')
        self.assertEqual((route['pin'], route['comparator_mux']), ('PA4', 4))
        route['comparator_mux'] = 0
        with self.assertRaisesRegex(ContractError, 'VC1: AF projection differs'):
            validate_reviewed_topology(chip)

    def test_unreviewed_physical_pin_position_is_rejected(self):
        chip = deepcopy(self.chips["CW32F030C8T7"])
        chip["packages"][0]["pins"] = [{"position": "1", "signals": ["PA0"]}]
        with self.assertRaisesRegex(ContractError, "Physical package projection differs"):
            validate_reviewed_topology(chip)


    def test_alias_pins_are_conservative_and_package_free(self):
        for chip in self.chips.values():
            if chip['packages']:
                continue
            packages, pins = pin_projection(chip, self.manifests[chip['line']])
            self.assertEqual(packages, [])
            self.assertEqual(chip['cores'][0]['pins'], pins)

    def test_wrong_pin_scope_and_rcc_are_rejected(self):
        chip = deepcopy(self.chips['CW32F030K8T7'])
        chip['cores'][0]['pins'].append({'name': 'PB8'})
        with self.assertRaisesRegex(ContractError, 'Core GPIO projection differs'):
            validate_reviewed_topology(chip)
        chip = deepcopy(self.chips['CW32F030'])
        next(p for p in chip['cores'][0]['peripherals'] if p.get('rcc'))['rcc']['enable']['field'] = 'ABSENT'
        with self.assertRaisesRegex(ContractError, 'RCC projection differs'):
            validate_reviewed_topology(chip)



    def test_wrong_af_selector_and_unbonded_route_are_rejected(self):
        chip = deepcopy(self.chips['CW32F030K8T7'])
        # Analog routes have no AF selector; mutate an actual digital route.
        route = next(route for p in chip['cores'][0]['peripherals']
                     for route in p.get('pins', []) if route.get('af') is not None)
        route['af'] = (route['af'] + 1) % 16
        with self.assertRaisesRegex(ContractError, 'AF projection differs'):
            validate_reviewed_topology(chip)
        chip = deepcopy(self.chips['CW32F030K8T7'])
        next(p for p in chip['cores'][0]['peripherals'] if p['name'] == 'UART1').setdefault('pins', []).append({'pin': 'PB8', 'signal': 'TX', 'af': 1})
        with self.assertRaisesRegex(ContractError, 'AF projection differs'):
            validate_reviewed_topology(chip)


if __name__ == "__main__":
    unittest.main(verbosity=2)
