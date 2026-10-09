#!/usr/bin/env python3
"""Independent authored-clock to generated-Rcc boundary checks.

This tests facts/identity projection, not electrical correctness. Vendor source
hashes and exact locators are retained in the sidecar for independent review.
"""
import copy
import json
from pathlib import Path
import unittest
import yaml

ROOT = Path(__file__).resolve().parents[1]


def load(path):
    return yaml.safe_load(path.read_text()) if path.suffix == '.yaml' else json.loads(path.read_text())


def project(record, controller):
    if record['status'] == 'partial':
        if not record['blockers']:
            raise ValueError('Partial clock has no blocker')
        return None
    if record['status'] != 'supported' or record.get('blockers'):
        raise ValueError('Invalid support status')
    if not record.get('bus_clock') or not record.get('kernel_clock') or not record.get('enable'):
        raise ValueError('Missing supported clock fact')
    if record['reset_status'] not in ('verified-field', 'no-controller-reset'):
        raise ValueError('Reset status not established')
    for key in ('enable', 'reset'):
        if record.get(key) and record[key]['peripheral'] != controller:
            raise ValueError('Unscoped peripheral-local control')
    def field(value):
        return {key: value[key] for key in ('register', 'field')}
    kernel = record['kernel_clock']
    if 'clock' in kernel:
        kernel = kernel['clock']
    else:
        if kernel['mux']['peripheral'] != controller:
            raise ValueError('Unscoped peripheral-local kernel mux')
        kernel = field(kernel['mux'])
    result = {'bus_clock': record['bus_clock'], 'kernel_clock': kernel, 'enable': field(record['enable'])}
    if record.get('reset'):
        result['reset'] = field(record['reset'])
    return result


class ClockContracts(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.files = {d['profile']: d for path in sorted((ROOT / 'cw32-data/clock').glob('*.yaml')) if (d := load(path))}
        cls.chips = {d['name']: d for path in sorted((ROOT / 'cw32-data/data/chips').glob('*.json')) if (d := load(path))}

    def test_all_thirteen_profiles_have_explicit_clock_inventory(self):
        profiles = {load(path)['line'] for path in (ROOT / 'cw32-data/inputs').glob('*.yaml')}
        self.assertEqual(set(self.files), profiles)
        for profile, sidecar in self.files.items():
            names = [record['name'] for record in sidecar['peripherals']]
            self.assertEqual(len(names), len(set(names)))
            self.assertEqual(set(names), {p['name'] for p in self.chips[profile]['cores'][0]['peripherals']})

    def test_generated_records_are_exact_supported_projection(self):
        for chip in self.chips.values():
            sidecar = self.files[chip['line']]
            records = {r['name']: r for r in sidecar['peripherals']}
            for peripheral in chip['cores'][0]['peripherals']:
                with self.subTest(chip=chip['name'], peripheral=peripheral['name']):
                    self.assertEqual(peripheral.get('rcc'), project(records[peripheral['name']], sidecar['controller']))

    def test_stop_policy_is_not_misrepresented_as_hardware(self):
        for sidecar in self.files.values():
            policy = sidecar['stop_mode_policy']
            self.assertEqual(policy['value'], 'Stop1')
            self.assertEqual(policy['basis'], 'conservative-build-policy')
            self.assertTrue(policy['reason'])

    def test_partial_and_missing_kernel_cannot_gain_bus_default(self):
        sidecar = next(s for s in self.files.values() if any(r['status'] == 'supported' for r in s['peripherals']))
        record = copy.deepcopy(next(r for r in sidecar['peripherals'] if r['status'] == 'supported'))
        record.pop('kernel_clock')
        with self.assertRaisesRegex(ValueError, 'Missing supported'):
            project(record, sidecar['controller'])
        record['status'] = 'partial'
        record['blockers'] = ['Kernel remains unresolved.']
        self.assertIsNone(project(record, sidecar['controller']))

    def test_unscoped_local_mux_is_rejected(self):
        sidecar = next(s for s in self.files.values() if any(r['status'] == 'supported' for r in s['peripherals']))
        record = copy.deepcopy(next(r for r in sidecar['peripherals'] if r['status'] == 'supported'))
        record['kernel_clock'] = {'mux': {'peripheral': 'UART1', 'register': 'CR2', 'field': 'SOURCE'}}
        with self.assertRaisesRegex(ValueError, 'peripheral-local'):
            project(record, sidecar['controller'])

    def test_polarity_and_keys_remain_outside_upstream_shape(self):
        for sidecar in self.files.values():
            for record in sidecar['peripherals']:
                if record.get('reset_asserted_value') is not None:
                    self.assertIn(record['reset_asserted_value'], (0, 1))
                if record.get('enable_write_key'):
                    self.assertEqual(record['enable_write_key']['field']['register'], record['enable']['register'])
                projected = project(record, sidecar['controller'])
                if projected:
                    self.assertNotIn('reset_asserted_value', projected)
                    self.assertNotIn('enable_write_key', projected)
                    self.assertLessEqual(set(projected), {'bus_clock', 'kernel_clock', 'enable', 'reset'})


if __name__ == '__main__':
    unittest.main()
