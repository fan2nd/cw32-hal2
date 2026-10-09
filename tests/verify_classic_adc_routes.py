#!/usr/bin/env python3
"""Qualify classic ADC sources and package projections; never executes silicon.

--sources rehashes primary files, parses own-manual mux tables, independent
original PDF pin-grid ADC labels/positions, and exact SDK macro/comment lines.
--sidecars-only checks curated sources before generated data is replaced.
"""
import argparse
from functools import lru_cache
import hashlib
import json
from pathlib import Path
import re
import statistics
import subprocess
import sys
import yaml
from route_metadata import hardware_facts

ROOT = Path(__file__).resolve().parents[1]
FAMILIES = ('F002', 'F003', 'L031', 'R031', 'W031', 'L052', 'L083')
PINS_SMALL = ['PB2', 'PA1', 'PA4', 'PA6', 'PA7', 'PC0', 'PC1', 'PC2', 'PB0', 'PB1', 'PB6', 'PB5', 'PB3']
PINS_031 = [f'PA{i}' for i in range(8)] + ['PB0', 'PB1', 'PB2', 'PB10', 'PB11']
PINS_LARGE = [f'PA{i}' for i in range(8)] + ['PC4', 'PC5', 'PB0', 'PB1', 'PB2']
# Own-family manual table/PDF page; SDK first macro/comment line; register version.
SPECS = {'F002': ('19-5', 307, 206, 189, 'cw32f002_v1'),
         'F003': ('20-5', 367, 208, 191, 'cw32f003_v1'),
         'L031': ('22-5', 437, 212, 195, 'cw32l031_v1'),
         'R031': ('22-5', 440, 208, 195, 'cw32l031_v1'),
         'W031': ('22-5', 440, 203, 186, 'cw32l031_v1'),
         'L052': ('23-5', 474, 218, 201, 'cw32l052_v1'),
         'L083': ('23-5', 479, 227, 210, 'cw32l083_v1')}


def read(path): return hardware_facts(yaml.safe_load(path.read_text()) if path.suffix == '.yaml' else json.loads(path.read_text()))
def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()
def norm(pin): return pin[:2] + str(int(pin[2:]))
def pin_map(family):
    return PINS_SMALL if family in ('F002', 'F003') else PINS_LARGE if family in ('L052', 'L083') else PINS_031[4:] if family == 'R031' else PINS_031


@lru_cache(maxsize=None)
def qualified_routes(profile):
    family = profile.removeprefix('CW32')
    assert family in FAMILIES
    data = read(ROOT / f'cw32-data/af/{profile.lower()}-analog.yaml')
    pinouts = read(ROOT / f'cw32-data/pinouts/{profile.lower()}.yaml')
    audit = read(ROOT / 'docs/adc-timer-next-batch-evidence.json')
    sources = data['sources']
    table, page, macro_start, comment_start, version = SPECS[family]
    assert (data['schema_version'], data['profile'], data['kind'], data['register_version']) == (1, profile, 'analog', version)
    assert data['status'] == 'verified-sdk-datasheet-reference-manual'
    assert data['alias_pin_policy'] == 'common-package-intersection'
    for key, source in sources.items():
        if key == 'pinouts':
            assert source['file'] == f'cw32-data/pinouts/{profile.lower()}.yaml'
            assert sha(ROOT / source['file']) == source['sha256']
        else:
            assert source['sha256'] == audit['sources'][source['file']]['sha256']
            assert family.lower() in source['file'].lower()
    assert pinouts['family'] == profile and pinouts['source']['sha256'] == sources['datasheet']['sha256']
    assert len(data['routes']) == len(pin_map(family))
    for channel, (route, pin) in enumerate(zip(data['routes'], pin_map(family))):
        mux = channel + (4 if family == 'R031' else 0)
        source_signal = f"ADC_{'AIN' if family in ('F002', 'F003', 'L052', 'L083') else 'IN'}{channel}"
        assert (route['peripheral'], route['pin'], route['signal'], route['source_signal'], route['channel'], route['mux'], route['af']) == ('ADC', pin, f'IN{channel}', source_signal, channel, mux, None)
        assert route['source_macro'] == f'ADC_ExInputCH{channel}'
        assert route['source_line'] == macro_start + channel
        assert route['sdk_pin_comment_line'] == comment_start + channel
        comment_pin = PINS_LARGE[channel] if family == 'W031' else pin
        assert route['sdk_comment_pin'] == comment_pin
        assert route['source_kind'] == ('sdk-mux-and-datasheet-reference-manual' if comment_pin == pin else 'datasheet-reference-manual-corrected-sdk-pin-comment')
        assert route['source_sha256'] == sources['adc_header']['sha256']
        assert route['manual_cell'] == dict(table=table, pdf_page=page, printed_page=page-1,
            source_signal=f'AIN{channel}', mux_bits=f'{mux:04b}', pin=pin, sha256=sources['reference_manual']['sha256'])
        row = next(r for r in pinouts['table_rows'] if pin in r['signals'])
        assert row['pin_type'] == 'I/O' and not set(row['signals']) & {'SWCLK', 'SWDIO', 'NRST', 'BOOT'}
        assert route['pin_cell'] == dict(pin=pin, source_name=row['source_name'], source_signal=source_signal,
            table='5-2', pdf_page=row['pdf_page_index']+1, printed_page=row['pdf_page_index'],
            positions=row['positions'], pin_type=row['pin_type'], sha256=sources['datasheet']['sha256'])
        assert route['package_pins'] == {p['name']: row['positions'][p['table_column']] for p in pinouts['packages']}
        assert route['oscillator_aliases'] == [s for s in row['signals'] if s.startswith('OSC')]
    manifest = read(ROOT / f'cw32-data/inputs/{profile.lower()}.yaml')
    assert manifest['analog_metadata'] == f'cw32-data/af/{profile.lower()}-analog.yaml'
    assert manifest['alias_pin_policy'] == 'common-package-intersection'
    return data['routes']


def analog_grid(pdf, profile):
    """Independently read analog names and physical positions in original PDF rows."""
    import pdfplumber
    sys.path.insert(0, str(ROOT / 'cw32-data/tools'))
    from extract_pinouts import SPECS as PIN_SPECS, extract_rows
    pages, columns, _ = PIN_SPECS[profile]
    pinouts = read(ROOT / f'cw32-data/pinouts/{profile.lower()}.yaml')
    assert extract_rows(pdf, profile) == pinouts['table_rows']
    result = {}
    with pdfplumber.open(pdf) as document:
        for index in pages:
            page = document.pages[index]
            tables = [t for t in page.find_tables() if any(marker in str(t.extract()[:2]) for marker in ('NFQ', 'PFQL', 'POSST'))]
            assert len(tables) == 1
            table = tables[0]
            words = page.extract_words()
            def cx(w): return (w['x0']+w['x1'])/2
            def cy(w): return (w['top']+w['bottom'])/2
            words = [w for w in words if table.bbox[1] < cy(w) < table.bbox[3]]
            names = [w for w in words if re.fullmatch(r'P[A-F]\d{2}/?', w['text']) and cx(w) < 300]
            name_x = statistics.median(cx(w) for w in names)
            type_x = statistics.median(cx(w) for w in words if w['text'] == 'I/O' and name_x < cx(w) < name_x+70)
            half_width = (type_x-name_x)/2
            for row in table.rows:
                if row.cells[0] is None: continue
                box = row.cells[0]
                selected = [w for w in words if box[1] < cy(w) < box[3]]
                channels = {int(m[1]) for w in selected for m in re.finditer(r'ADC_A?IN(\d+)', w['text'])}
                if not channels: continue
                pins = [norm(w['text'].rstrip('/')) for w in selected if name_x-half_width < cx(w) < name_x+half_width and re.fullmatch(r'P[A-F]\d{2}/?', w['text'])]
                assert len(channels) == len(pins) == 1, (profile, index, channels, pins)
                numbers = [w['text'] for w in sorted(selected, key=cx) if cx(w) < name_x-half_width and re.fullmatch(r'\d+|-', w['text'])]
                assert len(numbers) == len(columns)
                channel = channels.pop()
                assert channel not in result
                result[channel] = (pins[0], index+1, dict(zip(columns, [None if n == '-' else n for n in numbers])))
    return result


def verify_sources(directory, profile):
    data = read(ROOT / f'cw32-data/af/{profile.lower()}-analog.yaml')
    sources = data['sources']
    for key, source in sources.items():
        assert sha((ROOT if key == 'pinouts' else directory) / source['file']) == source['sha256'], (profile, key)
    page = SPECS[profile[4:]][1]
    manual = subprocess.check_output(['pdftotext', '-f', str(page), '-l', str(page), '-layout',
                                     str(directory / sources['reference_manual']['file']), '-'], text=True)
    muxes = {int(m[2]): (norm(m[3]), int(m[1], 2)) for m in re.finditer(r'([01]{4})\s+AIN(\d+)\s+(P[A-F]\d+)', manual)}
    assert muxes == {r['channel']: (r['pin'], r['mux']) for r in data['routes']}
    header = (directory / sources['adc_header']['file']).read_text().splitlines()
    grid = analog_grid(directory / sources['datasheet']['file'], profile)
    assert set(grid) == set(range(len(data['routes'])))
    for route in data['routes']:
        assert re.search(route['source_macro'] + r'\s+\(\(uint32_t\)0x' + f"{route['mux']:08X}" + r'\)', header[route['source_line']-1])
        comment_pin = route['sdk_comment_pin']
        assert f"通道{route['channel']}输入{comment_pin[:2]}{int(comment_pin[2:]):02d}" in header[route['sdk_pin_comment_line']-1]
        assert grid[route['channel']] == (route['pin'], route['pin_cell']['pdf_page'], route['pin_cell']['positions'])
    print(f'PASS {profile}: own RM/SDK muxes and independently parsed PDF analog/position rows')


def verify_generated(data_dir, profile):
    routes = qualified_routes(profile)
    pinouts = read(ROOT / f'cw32-data/pinouts/{profile.lower()}.yaml')
    sets = []
    counts = []
    for part in [p['name'] for p in pinouts['packages']] + [profile]:
        core = read(data_dir / 'chips' / (part+'.json'))['cores'][0]
        pins = {p['name'] for p in core['pins']}
        adc = next(p for p in core['peripherals'] if p['name'] == 'ADC')
        assert adc['registers']['version'] == SPECS[profile[4:]][4]
        got = {(r['pin'], r['signal'], r['adc_mux'], r.get('af')) for r in adc['pins']}
        expected = {(r['pin'], r['signal'], r['mux'], None) for r in routes if r['pin'] in pins}
        assert got == expected, part
        assert len(adc['pins']) == len(expected)
        for peripheral in core['peripherals']:
            if peripheral['name'] != 'ADC': assert all(r.get('adc_mux') is None for r in peripheral.get('pins', []))
        if part == profile: assert got == set.intersection(*sets)
        else: sets.append(got)
        counts.append(f'{part}={len(got)}')
    print('PASS package routes: ' + ', '.join(counts))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--sources', type=Path)
    parser.add_argument('--sidecars-only', action='store_true')
    parser.add_argument('--data-dir', type=Path, default=ROOT / 'cw32-data/data')
    args = parser.parse_args()
    for family in FAMILIES:
        profile = 'CW32'+family
        qualified_routes(profile)
        if args.sources: verify_sources(args.sources, profile)
        if not args.sidecars_only: verify_generated(args.data_dir, profile)
    print('PASS seven-family ADC qualification; R031 source labels remain distinct from hardware mux')

if __name__ == '__main__': main()
