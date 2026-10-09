#!/usr/bin/env python3
"""Exact review/projection and negative contracts for the final 340 serial cells."""
from copy import deepcopy
import unittest

from reviewed_metadata import af_projection, pin_projection, projection_errors
from verify_final_serial_af import (COUNTS, DEBUG, EVIDENCE_PATH, FAMILIES, MAX_AF,
                                    ROOT, SDK_ONLY_F002_PINS, load, names, validate)


class FinalSerialRoutes(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.profiles = {f: load(f'cw32-data/af/{f.lower()}-serial.yaml') for f in FAMILIES}
        cls.proof = {p['profile']: p for p in load(EVIDENCE_PATH)['profiles']}

    def route(self, family, pin, af):
        return next(r for r in self.profiles[family]['routes'] if (r['pin'], r['af']) == (pin, af))

    def projection(self, name):
        chip = load(f'cw32-data/data/chips/{name}.json')
        manifest = load(f"cw32-data/inputs/{chip['line'].lower()}.yaml")
        _, pins = pin_projection(chip, manifest)
        # Analog ADC routes are now independently qualified too. This serial
        # suite continues to compare every UART/SPI/I2C cell and its exact AF.
        return {name: routes for name, routes in af_projection(manifest, pins).items()
                if name.startswith(("UART", "SPI", "I2C"))}

    def test_all_340_reviewed_routes_retain_exact_evidence(self):
        for family in FAMILIES:
            with self.subTest(family=family):
                validate(family, self.profiles[family], self.proof[family])
        self.assertEqual(sum(len(p['routes']) for p in self.profiles.values()), 340)

    def test_singleton_names_are_normalized_per_family(self):
        for family, expected in {'CW32F002': ('SPI', 'I2C'), 'CW32F003': ('SPI', 'I2C'),
                                 'CW32L010': ('SPI', 'I2C1'), 'CW32L011': ('SPI', 'I2C')}.items():
            instances = {r['peripheral'] for r in self.profiles[family]['routes']}
            self.assertIn(expected[0], instances)
            self.assertIn(expected[1], instances)
            self.assertNotIn('SPI1', instances)
            self.assertFalse(any(r['signal'].startswith('1') for r in self.profiles[family]['routes']))
        self.assertEqual(names('CW32F002', 'SPI_CS'), ('SPICS', 'SPI', 'NSS'))
        self.assertEqual(names('CW32L010', 'SPI_MOSI'), ('SPI1MOSI', 'SPI', 'MOSI'))
        self.assertEqual(names('CW32L011', 'I2C_SDA'), ('I2CSDA', 'I2C', 'SDA'))
        self.assertEqual(names('CW32L012', 'SPI3_CS'), ('SPI3NCS', 'SPI3', 'NSS'))

    def test_l012_af8_and_af9_are_real_distinct_cells(self):
        self.assertEqual((self.route('CW32L012', 'PA0', 8)['peripheral'],
                          self.route('CW32L012', 'PA0', 8)['signal']), ('SPI2', 'MISO'))
        self.assertEqual((self.route('CW32L012', 'PA15', 9)['peripheral'],
                          self.route('CW32L012', 'PA15', 9)['signal']), ('SPI2', 'SCK'))
        self.assertEqual((self.route('CW32L012', 'PA10', 8)['peripheral'],
                          self.route('CW32L012', 'PA10', 8)['signal']), ('SPI3', 'MISO'))
        for family in FAMILIES:
            self.assertLessEqual(max(r['af'] for r in self.profiles[family]['routes']), MAX_AF[family])

    def test_f002_sdk_extras_stay_excluded_and_f003_extras_are_tssop24_only(self):
        f002 = self.profiles['CW32F002']
        self.assertEqual(len(f002['excluded_routes']), 10)
        self.assertFalse({r['pin'] for r in f002['routes']} & SDK_ONLY_F002_PINS)
        self.assertEqual({r['pin'] for r in f002['excluded_routes']}, SDK_ONLY_F002_PINS)
        manual = self.proof['CW32F002']['excluded_confirmation']
        self.assertEqual((manual['table'], manual['pdf_page'], manual['printed_page']), ('8-2', 105, 104))
        for name in ('CW32F003', 'CW32F003F4U7', 'CW32F003F4P7'):
            routes = [r for pins in self.projection(name).values() for r in pins]
            self.assertFalse({r['pin'] for r in routes} & SDK_ONLY_F002_PINS, name)
        routes = [r for pins in self.projection('CW32F003E4P7').values() for r in pins]
        self.assertEqual(sum(r['pin'] in SDK_ONLY_F002_PINS for r in routes), 10)

    def test_every_safe_package_count_and_family_alias_intersection(self):
        expected = {
            'CW32F002': [(36, 29), (36, 29)],
            'CW32F003': [(36, 29), (36, 29), (46, 38)],
            'CW32L010': [(30, 26), (38, 31), (38, 31)],
            'CW32L011': [(62, 49), (62, 49)],
            'CW32L012': [(125, 99), (125, 99)],
        }
        for family in FAMILIES:
            packages = self.proof[family]['packages']
            self.assertEqual([(p['safe_routes'], p['hal_tx_rx_sck_mosi_miso_scl_sda_routes']) for p in packages], expected[family])
            for package in packages:
                self.assertFalse(set(package['safe_serial_pins']) & DEBUG[family])
            def identities(projection):
                return {(p, r['pin'], r['af'], r['signal']) for p, rs in projection.items() for r in rs}
            self.assertEqual(identities(self.projection(family)),
                             set.intersection(*(identities(self.projection(p['name'])) for p in packages)))

    def test_only_reviewed_serial_routes_are_promoted(self):
        for family in FAMILIES:
            candidate = load(f'cw32-data/af/{family.lower()}.yaml')
            self.assertEqual(candidate['status'], 'candidate-sdk-and-register-verified')
            self.assertTrue(any(r['peripheral'].startswith(('ATIM', 'GTIM', 'BTIM')) for r in candidate['routes']))
            self.assertTrue(all(r['peripheral'].startswith(('UART', 'SPI', 'I2C')) for r in self.profiles[family]['routes']))

    def test_every_generated_family_and_exact_projection(self):
        checked = 0
        for path in sorted((ROOT / 'cw32-data/data/chips').glob('*.json')):
            chip = load(path.relative_to(ROOT))
            if chip['line'] not in FAMILIES:
                continue
            with self.subTest(chip=chip['name']):
                manifest = load(f"cw32-data/inputs/{chip['line'].lower()}.yaml")
                self.assertFalse(projection_errors(chip, manifest))
            checked += 1
        self.assertEqual(checked, 17)

    def test_review_rejects_direction_instance_selector_or_evidence_changes(self):
        for field, value in [('signal', 'TX'), ('peripheral', 'SPI1'), ('af', 9),
                             ('gpio_field', 'PIN1'), ('source_macro', 'invented'),
                             ('source_line', 1), ('source_kind', 'datasheet')]:
            broken = deepcopy(self.profiles['CW32L012'])
            route = next(r for r in broken['routes'] if (r['pin'], r['af']) == ('PA0', 8))
            route[field] = value
            with self.subTest(field=field), self.assertRaises(AssertionError):
                validate('CW32L012', broken, self.proof['CW32L012'])
        for family in FAMILIES:
            broken = deepcopy(self.profiles[family]); broken['routes'].pop()
            with self.assertRaises(AssertionError):
                validate(family, broken, self.proof[family])

    def test_projection_rejects_wrong_direction_selector_and_unbonded_route(self):
        for name, peripheral, pin, signal, af in [
            ('CW32F003F4U7', 'UART2', 'PA3', 'TX', 1),
            ('CW32L010Y8M6', 'SPI', 'PB2', 'MISO', 3),
            ('CW32L012C8U6', 'SPI2', 'PA0', 'MOSI', 8),
            ('CW32L012C8U6', 'SPI2', 'PA0', 'MISO', 9),
        ]:
            chip = load(f'cw32-data/data/chips/{name}.json')
            p = next(p for p in chip['cores'][0]['peripherals'] if p['name'] == peripheral)
            p['pins'].append(dict(pin=pin, signal=signal, af=af))
            with self.subTest(chip=name, pin=pin, af=af):
                self.assertTrue(projection_errors(chip, load(f"cw32-data/inputs/{chip['line'].lower()}.yaml")))

    def test_existing_radio_and_wrong_selector_rejections_are_preserved(self):
        # The new L012 capability must not weaken previously reviewed 031 routes.
        from verify_remaining_serial_af import RF_PINS, validate as old_validate
        evidence = load('docs/remaining-serial-af-evidence.json')['profiles']
        for family in ('CW32L031', 'CW32R031', 'CW32W031'):
            original = load(f'cw32-data/af/{family.lower()}-serial.yaml')
            proof = next(p for p in evidence if p['profile'] == family)
            old_validate(family, original, proof)
            self.assertFalse({r['pin'] for r in original['routes']} & RF_PINS.get(family, set()))
            for af in (8, 9):
                broken = deepcopy(original)
                broken['routes'][0]['af'] = af
                with self.assertRaises(AssertionError):
                    old_validate(family, broken, proof)
            self.assertFalse(any(set(p['safe_serial_pins']) & {'PA13', 'PA14', 'PF3'} for p in proof['packages']))


if __name__ == '__main__':
    unittest.main(verbosity=2)
