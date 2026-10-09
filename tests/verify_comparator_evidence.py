#!/usr/bin/env python3
"""Read-only own-source/data/PAC audit for the external comparator subset.

No HAL harness, MMIO simulation, firmware execution, or network access.
"""
import hashlib
import json
import os
from pathlib import Path
import yaml
import re
import tomllib

ROOT = Path(__file__).resolve().parents[1]
SOURCES = Path(os.environ.get('CW32_SOURCES', '/workspace/shared/cw32-sources'))
def load(path):
    return yaml.safe_load(path.read_text()) if path.suffix == '.yaml' else json.loads(path.read_text())
sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
lock = load(ROOT / 'sources/evidence-sources.json')
nodes = {}
for artifact in lock['artifacts']:
    for item in [artifact, *artifact.get('members', []), *([artifact['text']] if 'text' in artifact else [])]:
        nodes[item['path']] = item
classic = load(ROOT / 'docs/comparator-classic-evidence.json')
low = load(ROOT / 'docs/comparator-low-evidence.json')
verified = set()
for source in [*classic['sources'], *low['sources']]:
    for item in [source, *source.get('members', []), *([source['text']] if 'text' in source else [])]:
        path = item['path']
        assert item['sha256'] == nodes[path]['sha256'], path
        assert sha(SOURCES / path) == item['sha256'], path
        verified.add(path)
print(f'PASS {len(verified)} canonical manual/datasheet/text/SDK fingerprints')

features = [x.upper() for x in tomllib.loads((ROOT / 'embassy-cw32/Cargo.toml').read_text())['features'] if x.startswith('cw32')]
profiles = {}
coverage = {}
route_count = 0
for path in sorted((ROOT / 'cw32-data/af').glob('*-comparator.yaml')):
    p = load(path)
    family = p['profile']
    profiles[family] = p
    pinout = load(ROOT / p['sources']['pinouts']['file'])
    assert sha(ROOT / p['sources']['pinouts']['file']) == p['sources']['pinouts']['sha256']
    assert pinout['source']['sha256'] == p['sources']['datasheet']['sha256']
    for key in ['reference_manual', 'datasheet', 'manual_text']:
        source = p['sources'][key]
        assert source['sha256'] == nodes[source['file']]['sha256']
        assert sha(SOURCES / source['file']) == source['sha256']
    evidence = p['limits_evidence']
    assert sha(ROOT / evidence['file']) == evidence['sha256'] and evidence['family'] == family
    pages = (SOURCES / p['sources']['manual_text']['file']).read_text().split('\f')
    if family in low['electrical']:
        electrical = low['electrical'][family]
        supply = re.search(r'([0-9.]+)\.\.([0-9.]+)V', electrical['supply'])
        bounds = [round(float(supply[i]) * 1000) for i in [1, 2]]
        uses_vdda = electrical['input'] == '0..VDDA'
        expected_max = {'INP': 3, 'INN': 1}
        bits = 2
        refs = low['source_references'][family]
        control_text = '\n'.join(pages[i-1] for i in refs['control_pdf_pages_1_based'])
        assert 'READY' not in control_text
        assert all(x in control_text for x in ['RESP', 'HYS', 'POL', 'FLTV', 'INTF'])
        assert electrical['comparator_startup_us'] == {'typical': 0.5, 'maximum': None}
        clock_text = '\n'.join(pages[i-1] for i in refs['clock_pdf_pages_1_based'])
        assert '5A5A' in clock_text.upper()
    else:
        electrical = next(e for e in classic['electrical_records'] if e['family'] == family)
        supply = electrical['supply']
        bounds = [round(supply[x] * 1000) for x in ['min_V', 'max_V']]
        uses_vdda = supply['rail'] == 'VDDA'
        expected_max = {'INP': 7, 'INN': 7}
        bits = 4
        manual = next(e for e in classic['manual_records'] if family in e['families'])
        control_text = '\n'.join(pages[i-1] for key in ['config', 'cr1', 'status'] for i in manual[key]['reference']['pdf_pages_1_based'])
        assert all(x in control_text for x in ['RESP', 'HYS', 'POL', 'FLTV', 'INTF', 'READY'])
        assert manual['status']['intf']['access'] == 'RW0'
    assert re.search(r'1[：:]\s*PCLK', control_text), (family, 'PCLK encoding')
    assert p['limits'] == {'supply_mv': bounds, 'input_uses_vdda': uses_vdda}
    assert p['external_mux_max'] == expected_max
    seen = set()
    for route in p['routes']:
        signal = route['source_signal']
        pin = route['pin']
        cell = route['manual_pin_cell']
        page = pages[cell['pdf_page']-1]
        assert cell['table'] in page
        pin_matches = [(s, raw[:2] + str(int(raw[2:]))) for s, raw in re.findall(r'(VC[1-4]_CH\d)\s+(P[A-F]\d+)', page)]
        assert (signal, pin) in pin_matches, (family, signal, pin)
        mux_cell = route['manual_mux_cell']
        mux_page = pages[mux_cell['pdf_page']-1]
        assert mux_cell['table'] in mux_page
        # The table has distinct INP and INN columns. Each binary mux is paired
        # with its source label; R031 explicitly names VC1 CH0 at hardware 0100.
        mux_pairs = re.findall(r'\b([01]{%d})\s+(VC[^\s]+)' % bits, mux_page)
        generic_signal = re.sub(r'^VC[1-4]_', 'VCx_', signal)
        matches = [(int(code, 2), label) for code, label in mux_pairs
                   if signal in label.split('/') or generic_signal in label.split('/')]
        expected_multiplicity = 2 if route['direction'] == 'INN' or route['mux'] <= expected_max['INN'] else 1
        assert sum(code == route['mux'] for code, _ in matches) >= expected_multiplicity, (family, route, matches)
        assert route['af'] is None and route['mux'] <= expected_max[route['direction']]
        assert route['direction'] in ['INP', 'INN'] and route['signal'].startswith(route['direction'])
        key = (route['peripheral'], pin, route['direction'])
        assert key not in seen
        seen.add(key)
        row = next(row for row in pinout['table_rows'] if pin in row['signals'])
        assert route['pinout_cell']['pdf_page'] == row['pdf_page_index'] + 1
        assert route['package_pins'] == {pack['name']:pinrow['position'] for pack in pinout['packages'] for pinrow in pack['pins'] if pin in pinrow['signals']}
        route_count += 1
    coverage[family] = {'register_version':p['register_version'], 'source_input_routes':len(p['routes']),
        'instances':sorted({r['peripheral'] for r in p['routes']}), 'hardware_ready':family not in low['electrical'],
        'declared_features':[], 'selected_input_routes':{}, 'scope':'External positive/negative pins, response speed, hysteresis, polarity and polling only',
        'not_implemented':['internal references/dividers/DAC', 'output pins', 'window/filter/blanking', 'timer routing', 'interrupt/async/DMA']}
print(f'PASS {len(profiles)} own-family source profiles; {route_count} actual pin/directional mux cells')

count = 0
for chip_name in features:
    chip = load(ROOT / 'cw32-data/data/chips' / (chip_name + '.json'))
    p = profiles[chip['line']]
    core = chip['cores'][0]
    pins = {pin['name'] for pin in core['pins']}
    expected = {(r['peripheral'], r['pin'], r['signal'], r['mux']) for r in p['routes'] if r['pin'] in pins}
    actual = set()
    comparators = [v for v in core['peripherals'] if v.get('registers',{}).get('kind') == 'vc']
    for vc in comparators:
        assert vc['comparator_limits'] == p['limits']
        control = vc['rcc_control']
        assert control['shared_enable_group'] and control['shared_reset_group']
        assert control['reset_asserted_value'] is False
        if chip['line'] in low['electrical']:
            assert control['enable_write_key']['value'] == 0x5A5A
        for pin in vc['pins']:
            if 'comparator_mux' in pin:
                assert 'af' not in pin and 'adc_mux' not in pin
                actual.add((vc['name'], pin['pin'], pin['signal'], pin['comparator_mux']))
    assert expected == actual, (chip_name, expected ^ actual)
    coverage[chip['line']]['declared_features'].append(chip_name.lower())
    coverage[chip['line']]['selected_input_routes'][chip_name.lower()] = len(actual)
    count += len(actual)
print(f'PASS {len(features)} chip selections, {count} package-qualified input route projections and shared keyed RCC facts')

seeds = load(ROOT / 'cw32-data/register-writes.yaml')['registers']
for version in {p['register_version'] for p in profiles.values()}:
    stem = 'vc_' + version
    ir = load(ROOT / 'cw32-data/data/registers' / (stem + '.json'))
    fields = {f['name']:f for f in ir['fieldset/CR0']['fields']}
    two_speed = fields['RESP']['bit_size'] == 1
    assert {v['name']:v['value'] for v in ir['enum/ResponseSpeed']['variants']} == ({'LOW':0,'HIGH':1} if two_speed else {'ULTRA_LOW':0,'LOW':1,'MEDIUM':2,'HIGH':3})
    assert {v['name']:v['value'] for v in ir['enum/Hysteresis']['variants']} == ({'NONE':0,'ENABLED':1} if two_speed else {'NONE':0,'LOW':1,'MEDIUM':2,'HIGH':3})
    assert {v['name']:v['value'] for v in ir['enum/Polarity']['variants']} == {'NORMAL':0,'INVERTED':1}
    assert {v['name']:v['value'] for v in ir['enum/FilterClock']['variants']} == {'INTERNAL_RC':0,'PCLK':1}
    for name, en in [('RESP','ResponseSpeed'),('HYS','Hysteresis'),('POL','Polarity')]:
        assert fields[name]['enum'] == en
    status = {f['name']:f['bit_offset'] for f in ir['fieldset/SR']['fields']}
    assert status == ({'INTF':0,'FLTV':1} if two_speed else {'INTF':0,'FLTV':1,'READY':2})
    seed = seeds[stem][0]
    assert seed['write_noop'] == 1 and seed['reset_value'] == 0 and seed['zero_to_clear_fields'] == ['INTF']
print('PASS six typed comparator register versions; explicit SR R1W0 command seeds')
if os.environ.get('CW32_WRITE_COMPARATOR_COVERAGE') == '1':
    (ROOT / 'docs/comparator-coverage.json').write_text(json.dumps({'schema_version':1,'hardware_validated':False,
        'scope':'Source-audited external-pin polling subset; production compile results are reported separately.', 'families':coverage},indent=2)+'\n')
