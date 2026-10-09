#!/usr/bin/env python3
"""Audit primary-source hashes and generated classic timer input facts, not HAL execution."""
import argparse
import hashlib
import json
from pathlib import Path
import yaml

parser = argparse.ArgumentParser()
parser.add_argument('--sources', type=Path, required=True)
args = parser.parse_args()
root = Path(__file__).resolve().parents[1]
def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()
catalog = yaml.safe_load((root / 'cw32-data/classic-timer-input.yaml').read_text())
proof = json.loads((root / catalog['evidence']).read_text())
assert catalog['schema_version'] == 1
assert set(catalog['profiles']) == set(proof['families'])
verified_sources = set()
def verify_sources(node):
    if isinstance(node, dict):
        name = node.get('path', node.get('file'))
        if name and 'sha256' in node:
            path = args.sources / name
            assert path.is_file(), path
            assert digest(path) == node['sha256'], path
            verified_sources.add(str(path))
            if 'text_sha256' in node:
                assert digest(path.with_suffix('.txt')) == node['text_sha256'], path
        for value in node.values():
            verify_sources(value)
    elif isinstance(node, list):
        for value in node:
            verify_sources(value)
for family, profile in catalog['profiles'].items():
    facts = proof['families'][family]
    verify_sources(facts)
    assert profile['encoder_fixed_reload'] == facts['qei_arr_restriction']['value'] == 65535
    assert profile['instances'] == facts['instances']
    assert digest(root / profile['route_source']) == profile['route_source_sha256']
    keys = {(x['peripheral'], x['pin'], x['af'], x['channel']) for x in profile['routes']}
    assert len(keys) == len(profile['routes'])
    assert all(x[3] in range(1, 5) for x in keys)
chip_count = pin_count = instance_count = 0
for path in sorted((root / 'cw32-data/data/chips').glob('*.json')):
    chip = json.loads(path.read_text())
    profile = catalog['profiles'].get(chip['line'])
    core = chip['cores'][0]
    if profile is None:
        assert not any(p.get('classic_timer_input') for p in core['peripherals'])
        continue
    chip_count += 1
    pins = {p['name'] for p in core['pins']}
    for peripheral in core['peripherals']:
        facts = peripheral.get('classic_timer_input')
        if not peripheral['name'].startswith('GTIM'):
            assert facts is None
            continue
        instance_count += 1
        assert facts == {'capture_mux': peripheral['name'] + 'CAP', 'encoder_fixed_reload': 65535}
        routes = peripheral.get('pins', [])
        expected = {(x['pin'], x['af'], f"CAP{x['channel']}") for x in profile['routes']
                    if x['peripheral'] == peripheral['name'] and x['pin'] in pins}
        observed = {(p['pin'], p['af'], p['signal']) for p in routes if p['signal'].startswith('CAP')}
        assert expected == observed, path
        for pin, af, signal in observed:
            assert any(p['pin'] == pin and p.get('af') == af and p['signal'] == 'CH' + signal[3:] for p in routes)
        pin_count += len(observed)
semantics = proof['common_semantics']
assert semantics['capture']['ccr_read_clears_flag'] is False
assert semantics['qei']['overflow_bit'] != semantics['qei']['underflow_bit']
source = (root / 'embassy-cw32/src/timer/classic_input.rs').read_text()
assert '.icr().modify' not in source and '.isr().modify' not in source
assert 'set_encmode' in source and 'set_en(true)' in source
assert 'overcapture_pending' not in source
assert source.count('Icr::write_noop()') == 2
assert '.icr().write(' not in source
for path in (root / 'embassy-cw32/src/timer').rglob('*.rs'):
    text = path.read_text()
    assert '#[path' not in text and 'cfg_attr(path' not in text, path
print(json.dumps({'source_files_verified': len(verified_sources), 'classic_profiles': len(catalog['profiles']),
                  'chip_profiles': chip_count, 'timer_instances': instance_count, 'bonded_input_routes': pin_count,
                  'hal_execution': 'none; source/data audit only'}, indent=2))
