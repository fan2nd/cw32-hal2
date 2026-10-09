#!/usr/bin/env python3
"""Verify own official ADC scan evidence, curated arrays and metadata projection.

Source/data audit only. Never loads a HAL, simulates registers or runs firmware.
Full independent SVD element parity is checked by audit_generated_parity.py.
"""
import argparse
import hashlib
import json
from pathlib import Path
import yaml

ROOT = Path(__file__).resolve().parents[1]
p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--sources', type=Path, default=Path('/workspace/shared/cw32-sources'))
args = p.parse_args()

def read(path):
    return yaml.safe_load(path.read_text()) if path.suffix == '.yaml' else json.loads(path.read_text())

def verify(path, expected):
    assert hashlib.sha256((args.sources / path).read_bytes()).hexdigest() == expected, path

catalog = read(ROOT / 'cw32-data/adc-sequences.yaml')
proof = read(ROOT / catalog['evidence'])
lock = read(ROOT / 'sources/evidence-sources.json')
assert catalog['schema_version'] == proof['schema_version'] == 1
low_families = {'CW32L010', 'CW32L011'}
assert low_families <= set(catalog['profiles'])
assert set(catalog['profiles']) <= low_families | {'CW32L012'}
classic = read(ROOT / 'cw32-data/classic-adc-scans.yaml')['profiles']
verified = set()
for family in sorted(low_families):
    profile = catalog['profiles'][family]
    own = proof['families'][family]
    assert own['family'] == family and own['facts'] == profile['facts']
    assert own['register_version'] == profile['register_version']
    manual = own['manual']
    assert any(a['kind'] == 'pdf' and a['provenance']['status'] == 'selected'
               and family in a['provenance']['chip_scope']
               and a['path'] == manual['path'] and a['url'] == manual['url']
               and a['sha256'] == manual['sha256'] for a in lock['artifacts'])
    verify(manual['path'], manual['sha256'])
    verify(manual['derived_text_path'], manual['derived_text_sha256'])
    verified.update([manual['path'], manual['derived_text_path']])
    archive = own['sdk']['archive']
    verify(archive['path'], archive['sha256'])
    verified.add(archive['path'])
    for member in own['sdk']['members']:
        verify(member['path'], member['sha256'])
        verified.add(member['path'])
    version = 'adc_' + profile['register_version']
    authored = yaml.safe_load((ROOT / f'cw32-data/registers/{version}.yaml').read_text())
    generated = read(ROOT / f'cw32-data/data/registers/{version}.json')
    assert authored == generated
    items = {x['name']: x for x in generated['block/ADC']['items']}
    result = items['RESULT']
    assert result['byte_offset'] == 64 and result['access'] == 'Read'
    assert result['array'] == {'len': 8, 'stride': 4}
    field = generated['fieldset/RESULT']['fields'][0]
    assert (field['name'], field['bit_offset'], field['bit_size']) == ('RESULT', 0, 16)
    for name, offset in [('SAMPLE', 40), ('SQRCFR', 44)]:
        assert items[name]['byte_offset'] == offset
        fields = generated[f'fieldset/{name}']['fields']
        assert len(fields) == 1
        assert (fields[0]['name'], fields[0]['bit_offset'], fields[0]['bit_size']) == ('SQRCH', 0, 4)
        assert fields[0]['array'] == {'len': 8, 'stride': 4}
    cr = {f['name']: f for f in generated['fieldset/CR']['fields']}
    assert (cr['ENS']['bit_offset'], cr['ENS']['bit_size']) == (6, 3)
    assert (cr['CONT']['bit_offset'], cr['CONT']['bit_size']) == (3, 1)
    assert items['ISR']['access'] == 'Read'
    for flag in ['EOC', 'EOS']:
        expected_bit = 0 if flag == 'EOC' else 1
        for name in ['ISR', 'ICR']:
            f = next(f for f in generated[f'fieldset/{name}']['fields'] if f['name'] == flag)
            assert (f['bit_offset'], f['bit_size']) == (expected_bit, 1)
    assert profile['facts'] == {'maximum_length': 8, 'programmable_order': True,
                                'per_slot_sample_time': True, 'per_slot_result': True}
    print(f'PASS {family}: own manual/SDK bytes; eight ordered sample, mux and read-only result entries')

selected = 0
for path in (ROOT / 'cw32-data/data/chips').glob('*.json'):
    chip = read(path)
    family = chip['line']
    for peripheral in chip['cores'][0]['peripherals']:
        if 'adc_limits' not in peripheral:
            continue
        actual = peripheral['adc_limits'].get('sequence')
        if family in catalog['profiles']:
            assert actual == catalog['profiles'][family]['facts']
            selected += family in low_families
        elif family in classic:
            assert actual == classic[family]['sequence']
        else:
            assert actual is None, path
assert selected == 7
print(f'PASS {len(verified)} external artifact hashes and all seven low-family metadata projections; classic/L012 sources are qualified by their separate validators')
