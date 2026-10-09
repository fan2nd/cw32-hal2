#!/usr/bin/env python3
"""Validate own-source classic ADC scan data and generated PACs; no HAL execution."""
import argparse
import hashlib
import json
from pathlib import Path
import yaml

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--sources', type=Path, required=True)
a = parser.parse_args()

def read(path):
    return yaml.safe_load((ROOT / path).read_text()) if (ROOT / path).suffix == '.yaml' else json.loads((ROOT / path).read_text())

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

catalog = read('cw32-data/classic-adc-scans.yaml')
proof = read(catalog['evidence'])
for name in ('evidence', 'electrical_policy', 'startup_policy'):
    assert sha(ROOT / catalog[name]) == catalog[name + '_sha256'], name
own = {p['family']: p for p in proof['families']}
assert len(own) == len(proof['families']) == 10
assert own.keys() == catalog['profiles'].keys()
lock = read('sources/evidence-sources.json')['artifacts']
electrical = read(catalog['electrical_policy'])['families']
verified_sources = set()

def source(path, digest, family):
    matches = [x for x in lock if x.get('path') == path and x.get('sha256') == digest]
    assert len(matches) == 1, (family, path)
    item = matches[0]
    assert item['provenance']['status'] == 'selected'
    assert family in item['provenance']['chip_scope']
    assert sha(a.sources / path) == digest, path
    verified_sources.add(path)
    return item

profiles = 0
versions = set()
for family, profile in catalog['profiles'].items():
    evidence = own[family]
    source(evidence['source_path'], evidence['source_sha256'], family)
    ds_ref = electrical[family]['adc']['datasheet_source']['source_ref']
    ds = next(x for x in lock if x.get('id') == ds_ref)
    source(ds['path'], ds['sha256'], family)
    for page in evidence['page_refs'].values():
        assert page['section'] and page['printed_pages']
        assert page['pdf_pages_1_based'] == [p + 1 for p in page['printed_pages']]
    facts = profile['facts']
    sequence_caps = profile['sequence']
    assert profile['register_version'] == evidence['register_version']
    assert sequence_caps == {'maximum_length': evidence['slots'], 'programmable_order': True, 'per_slot_sample_time': False, 'per_slot_result': True}
    assert evidence['sequence']['ordered_slots'] and not evidence['common_controls']['per_slot_controls']
    assert facts['buffered_requires_single_channel'] == evidence['conservative_policy']['buffered_requires_single_channel_single_shot']
    assert facts['internal_requires_single_channel'] == evidence['conservative_policy']['internal_requires_single_channel_single_shot']
    channels = evidence['channels']
    assert facts['first_internal_channel'] == min(c for c in (channels['supply_div3'], channels['temperature'], channels['vrefint_1v2']) if c is not None)
    for fact, name in [('supply_channel', 'supply_div3'), ('temperature_channel', 'temperature'), ('bandgap_channel', 'vrefint_1v2')]:
        assert facts[fact] == channels[name]
    if facts['temperature_channel'] is not None:
        assert facts['temperature_startup_us'] == electrical[family]['adc']['temperature_startup_max_us'] + 5 == 50
        assert facts['bandgap_startup_us'] == 25  # Reviewed software margin on approximate 20 us.
    else:
        assert facts['temperature_startup_us'] == facts['bandgap_startup_us'] == 0
    version = profile['register_version']
    versions.add(version)
    ir = read(f'cw32-data/data/registers/adc_{version}.json')
    regs = {r['name']: r for r in ir['block/ADC']['items']}
    result = regs['RESULT']
    assert result['byte_offset'] == evidence['results']['first_offset']
    assert result['array'] == {'len': evidence['slots'], 'stride': evidence['results']['stride']}
    assert result['access'] == 'Read'
    for sequence in evidence['sequence']['registers']:
        register = regs[sequence['name']]
        assert register['byte_offset'] == sequence['offset'] and 'array' not in register
        fields = {f['name']: f for f in ir['fieldset/' + register['fieldset']]['fields']}
        assert len(sequence['slot_indices']) == facts['slots_per_sequence_register']
        assert fields['SQR']['array'] == {'len': facts['slots_per_sequence_register'], 'stride': 4}
        assert fields['SQR']['bit_offset'] == 0 and fields['SQR']['bit_size'] == sequence['slot_bit_size']
        if sequence['offset'] == evidence['sequence']['length_register_offset']:
            assert fields['ENS']['bit_offset'] == evidence['sequence']['length_bit_offset']
            assert fields['ENS']['bit_size'] == evidence['sequence']['length_bit_size']
        else:
            assert set(fields) == {'SQR'}
    mode = ir['enum/Mode']
    assert {v['name']: v['value'] for v in mode['variants']} == {'SINGLE': evidence['single_mode'], 'SCAN': evidence['scan_mode']}
    controls = {f['name']: f for f in ir['fieldset/CR0']['fields']}
    for name, key in [('TSEN', 'temperature_enable_bit'), ('BGREN', 'bgr_enable_bit')]:
        assert (name in controls) == (evidence['common_controls'][key] is not None)
        if name in controls:
            assert controls[name]['bit_offset'] == evidence['common_controls'][key]
    pac = (ROOT / f'cw32-metapac/src/peripherals/adc_{version}.rs').read_text()
    assert 'pub const fn result(self, n: usize)' in pac
    assert 'pub const fn set_sqr(&mut self, n: usize, val: u8)' in pac
    assert 'pub const fn result0(' not in pac
    for chip_path in (ROOT / 'cw32-data/data/chips').glob('*.json'):
        chip = json.loads(chip_path.read_text())
        if chip['line'] != family:
            continue
        adc = next(p for p in chip['cores'][0]['peripherals'] if p['name'] == 'ADC')
        assert adc['registers']['version'] == version
        assert adc['adc_limits']['classic_scan'] == facts
        assert adc['adc_limits']['sequence'] == sequence_caps
        assert adc['adc_limits']['sample_cycles'] == evidence['common_controls']['sampling_cycles_encodings']
        profiles += 1
for path in (ROOT / 'cw32-data/data/chips').glob('*.json'):
    chip = json.loads(path.read_text())
    if chip['line'] in own:
        continue
    assert all('classic_scan' not in p.get('adc_limits', {}) for p in chip['cores'][0]['peripherals'])
print(json.dumps({'families': len(own), 'chip_profiles': profiles, 'register_versions': len(versions),
                  'official_pdfs_hashed': len(verified_sources), 'hal_executed': False}, indent=2))
