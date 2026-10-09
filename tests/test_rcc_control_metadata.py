#!/usr/bin/env python3
"""Source-sidecar to independently typed RCC-control transport contracts."""
import json
from pathlib import Path
import unittest
import yaml

ROOT = Path(__file__).resolve().parents[1]


def load(path):
    return yaml.safe_load(path.read_text()) if path.suffix == '.yaml' else json.loads(path.read_text())


def controls(record, controller):
    if not record.get('enable') or not record.get('bus_clock') or record.get('enable_active_value') is None:
        return None
    def field(value):
        return {key: value[key] for key in ('register', 'field')}
    result = dict(controller=controller, bus_clock=record['bus_clock'],
                  enable=field(record['enable']), enable_active_value=bool(record['enable_active_value']))
    if record.get('reset'):
        result['reset'] = field(record['reset'])
        result['reset_asserted_value'] = bool(record['reset_asserted_value'])
    if record.get('enable_write_key'):
        key = record['enable_write_key']
        result['enable_write_key'] = dict(field=field(key['field']), value=key['value'])
    for key in ('shared_enable_group', 'shared_reset_group'):
        if record.get(key):
            result[key] = record[key]
    return result


class RccControlMetadata(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.profiles = {d['profile']: d for path in (ROOT / 'cw32-data/clock').glob('*.yaml') if (d := load(path))}
        cls.chips = [load(path) for path in (ROOT / 'cw32-data/data/chips').glob('*.json')]

    def test_every_control_is_exact_verified_projection(self):
        self.assertEqual(len(self.chips), 54)
        self.assertEqual(len(self.profiles), 13)
        for chip in self.chips:
            profile = self.profiles[chip['line']]
            source = {r['name']: r for r in profile['peripherals']}
            for p in chip['cores'][0]['peripherals']:
                with self.subTest(chip=chip['name'], peripheral=p['name']):
                    r = source[p['name']]
                    self.assertEqual(p.get('rcc_control'), controls(r, profile['controller']))
                    if r['status'] == 'partial':
                        self.assertNotIn('rcc', p)
                    if p.get('rcc_control'):
                        self.assertNotIn('kernel_clock', p['rcc_control'])

    def test_partial_uart_and_l012_i2c_retain_bus_without_kernel(self):
        count = 0
        for chip in self.chips:
            for p in chip['cores'][0]['peripherals']:
                if p['name'].startswith('UART') or (chip['line'] == 'CW32L012' and p['name'].startswith('I2C')):
                    self.assertIn('rcc_control', p)
                    self.assertNotIn('rcc', p)
                    self.assertEqual(p['rcc_control']['bus_clock'], 'PCLK')
                    count += 1
        self.assertGreater(count, 100)

    def test_source_artifact_identity_matches_canonical_lock(self):
        known = set()
        for artifact in load(ROOT / 'sources/evidence-sources.json')['artifacts']:
            known.add((artifact['url'], artifact['path'], artifact['sha256']))
            for member in artifact.get('members', []) + ([artifact['text']] if artifact.get('text') else []):
                known.add((artifact['url'], member['path'], member['sha256']))
        for profile in self.profiles.values():
            for source in profile['sources'].values():
                self.assertIn((source['url'], source['artifact'], source['sha256']), known)


if __name__ == '__main__':
    unittest.main()
