#!/usr/bin/env python3
"""Independent positive/negative contracts for the five newly reviewed AF maps."""
from copy import deepcopy
import json
from pathlib import Path
import unittest

from reviewed_metadata import projection_errors
from verify_remaining_serial_af import FAMILIES, load, validate, ROOT, RF_PINS


class SerialRoutes(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.evidence = {p['profile']: p for p in load('docs/remaining-serial-af-evidence.json')['profiles']}
        cls.profiles = {family: load(f'cw32-data/af/{family.lower()}-serial.yaml') for family in FAMILIES}

    def route(self, family, pin, af):
        return next(r for r in self.profiles[family]['routes'] if (r['pin'], r['af']) == (pin, af))

    def test_all_675_routes_keep_source_and_package_proof(self):
        for family in FAMILIES:
            with self.subTest(family=family):
                validate(family, self.profiles[family], self.evidence[family])
        self.assertEqual(sum(len(p['routes']) for p in self.profiles.values()), 675)

    def test_explicit_sdk_disagreements(self):
        for family in ('CW32L031', 'CW32R031', 'CW32W031'):
            route = self.route(family, 'PA10', 6)
            self.assertEqual((route['peripheral'], route['signal'], route['sdk_function']), ('UART3', 'TX', 'SUART3TXD'))
        route = self.route('CW32R031', 'PA15', 4)
        self.assertEqual((route['signal'], route['sdk_function']), ('RX', 'UART2TXD'))
        route = self.route('CW32W031', 'PA2', 5)
        self.assertEqual((route['peripheral'], route['signal'], route['source_kind']), ('UART3', 'RX', 'datasheet'))
        self.assertIsNone(route['source_macro'])
        self.assertIsNone(route['source_line'])

    def test_silicon_families_keep_their_different_cells(self):
        # L031/R031 direction differs from L052/L083 on the same PA15 AF4 cell.
        for family in ('CW32L031', 'CW32R031'):
            self.assertEqual(self.route(family, 'PA15', 4)['signal'], 'RX')
        for family in ('CW32L052', 'CW32L083'):
            self.assertEqual(self.route(family, 'PA15', 4)['signal'], 'TX')
        self.assertEqual(self.route('CW32L052', 'PA2', 2)['peripheral'], 'UART2')
        self.assertEqual(self.route('CW32L083', 'PA2', 2)['peripheral'], 'UART6')
        self.assertEqual(self.route('CW32L031', 'PB6', 3)['peripheral'], 'I2C1')
        self.assertEqual(self.route('CW32L052', 'PB6', 4)['peripheral'], 'I2C1')

    def test_rf_and_unbonded_sdk_pads_are_not_promoted(self):
        for family, forbidden in RF_PINS.items():
            pins = {r['pin'] for r in self.profiles[family]['routes']}
            self.assertFalse(pins & forbidden)
        r = self.profiles['CW32R031']
        self.assertFalse({r['pin'] for r in r['routes']} & {'PB8', 'PB9', 'PF6', 'PF7'})
        self.assertNotIn('PA15', {r['pin'] for r in self.profiles['CW32W031']['routes']})

    def test_all_generated_feature_projections(self):
        count = 0
        for path in sorted((ROOT / 'cw32-data/data/chips').glob('*.json')):
            chip = json.loads(path.read_text())
            if chip['line'] not in FAMILIES:
                continue
            manifest = load(f"cw32-data/inputs/{chip['line'].lower()}.yaml")
            with self.subTest(chip=chip['name']):
                self.assertFalse(projection_errors(chip, manifest))
                for peripheral in chip['cores'][0]['peripherals']:
                    for route in peripheral.get('pins', []):
                        self.assertNotIn(route['pin'], RF_PINS.get(chip['line'], set()))
                        self.assertNotEqual(route['pin'], 'PF3')
            count += 1
        self.assertGreaterEqual(count, 21)

    def test_review_rejects_wrong_selector_signal_macro_or_missing_route(self):
        cases = [('af', 8), ('signal', 'TX'), ('source_macro', 'invented'), ('source_line', 1)]
        for field, value in cases:
            profile = deepcopy(self.profiles['CW32W031'])
            route = next(r for r in profile['routes'] if (r['pin'], r['af']) == ('PA2', 5))
            route[field] = value
            with self.subTest(field=field), self.assertRaises(AssertionError):
                validate('CW32W031', profile, self.evidence['CW32W031'])
        profile = deepcopy(self.profiles['CW32L083'])
        profile['routes'].pop()
        with self.assertRaises(AssertionError):
            validate('CW32L083', profile, self.evidence['CW32L083'])

    def test_generated_route_negatives_are_detected(self):
        family = 'CW32R031'
        chip = load(f'cw32-data/data/chips/{family}C8U6.json')
        manifest = load('cw32-data/inputs/cw32r031.yaml')
        uart2 = next(p for p in chip['cores'][0]['peripherals'] if p['name'] == 'UART2')
        corrected = next(r for r in uart2['pins'] if r['pin'] == 'PA15' and r['signal'] == 'RX')
        corrected['signal'] = 'TX'
        self.assertTrue(projection_errors(chip, manifest))
        chip = load(f'cw32-data/data/chips/{family}C8U6.json')
        spi = next(p for p in chip['cores'][0]['peripherals'] if p['name'] == 'SPI1')
        spi['pins'].append({'pin': 'PA0', 'signal': 'MISO', 'af': 5})
        self.assertTrue(projection_errors(chip, manifest))


if __name__ == '__main__':
    unittest.main(verbosity=2)
