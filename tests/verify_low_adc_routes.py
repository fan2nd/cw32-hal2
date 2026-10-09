#!/usr/bin/env python3
"""Independently qualify L010/L011 ADC routes against their own PDF/SDK sources.

--sources rehashes original files, reads ADC labels from the analog-column PDF
cells, joins independent physical-package rows, and parses own-manual mux tables
and the selected own-family SDK comment column. --sidecars-only requires no
regeneration; normal mode also verifies all seven generated feature selections.
No driver behavior or electrical/silicon correctness is asserted.
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
FAMILIES = ('CW32L010', 'CW32L011')
# Manual PDF page, datasheet front-matter offset, SDK macro/comment first line,
# own-family SDK comment column. In L011 adc.h the left column is explicitly L010.
SPECS = {'CW32L010': (507, 1, 186, 169, 0), 'CW32L011': (509, 3, 206, 189, 1)}
PINS = {'CW32L010': [f'PA{i}' for i in range(7)] + [f'PB{i}' for i in range(7)],
        'CW32L011': [f'PA{i}' for i in range(8)] + ['PB0', 'PB1'] + [f'PA{i}' for i in range(8, 12)]}
COUNTS = {'CW32L010': 10, 'CW32L010Y8M6': 10, 'CW32L010F8P6': 14, 'CW32L010F8U6': 14,
          'CW32L011': 14, 'CW32L011K8T6': 14, 'CW32L011K8U6': 14}
FORBIDDEN = {'CW32L010': {'PA7', 'PA8', 'PB7'}, 'CW32L011': {'PA13', 'PA14'}}
SOURCE_FILES = {
    'CW32L010': {'datasheet': 'CW32L010_DataSheet_CN_V1.3.pdf',
                 'reference_manual': 'CW32L010_UserManual_CN_V1.2.pdf',
                 'sdk_archive': 'CW32L010_StandardPeripheralLib_V1.0.9.zip',
                 'adc_header': 'cw32l010/CW32L010_StandardPeripheralLib_V1.0.9/Libraries/inc/cw32l010_adc.h'},
    'CW32L011': {'datasheet': 'CW32L011_DataSheet_CN_V1.1.pdf',
                 'reference_manual': 'CW32L011_UserManual_CN_V1.1.pdf',
                 'sdk_archive': 'CW32L011_StandardPeripheralLib_V1.0.3.zip',
                 'adc_header': 'cw32l011/Libraries/inc/cw32l011_adc.h'}}


def read(path): return hardware_facts(yaml.safe_load(path.read_text()) if path.suffix == '.yaml' else json.loads(path.read_text()))
def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()
def norm(pin): return pin[:2] + str(int(pin[2:]))


@lru_cache(maxsize=None)
def qualified_routes(profile):
    assert profile in FAMILIES
    data = read(ROOT / f'cw32-data/af/{profile.lower()}-analog.yaml')
    pinouts = read(ROOT / f'cw32-data/pinouts/{profile.lower()}.yaml')
    audit = read(ROOT / 'docs/adc-timer-next-batch-evidence.json')
    sources = data['sources']
    page, offset, macro_start, comment_start, column = SPECS[profile]
    assert (data['schema_version'], data['profile'], data['kind'], data['register_version']) == (1, profile, 'analog', profile.lower()+'_v1')
    assert data['status'] == 'verified-sdk-datasheet-reference-manual'
    assert data['alias_pin_policy'] == 'common-package-intersection'
    assert set(sources) == {'pinouts'} | set(SOURCE_FILES[profile])
    for key, source in sources.items():
        if key == 'pinouts':
            assert source['file'] == f'cw32-data/pinouts/{profile.lower()}.yaml'
            assert sha(ROOT / source['file']) == source['sha256']
        else:
            assert source['file'] == SOURCE_FILES[profile][key]
            assert source['sha256'] == audit['sources'][source['file']]['sha256']
    assert pinouts['family'] == profile and pinouts['source']['sha256'] == sources['datasheet']['sha256']
    assert len(data['routes']) == 14
    for channel, (route, pin) in enumerate(zip(data['routes'], PINS[profile])):
        assert (route['peripheral'], route['pin'], route['signal'], route['source_signal'], route['channel'], route['mux'], route['af']) == ('ADC', pin, f'IN{channel}', f'ADC_IN{channel}', channel, channel, None)
        assert route['source_macro'] == f'ADC_InputCH{channel}'
        assert route['source_line'] == macro_start + channel
        assert route['sdk_pin_comment_line'] == comment_start + channel
        assert route['sdk_pin_comment_column'] == column
        assert route['sdk_comment_pin'] == pin
        assert route['source_kind'] == 'sdk-mux-and-datasheet-reference-manual'
        assert route['source_sha256'] == sources['adc_header']['sha256']
        assert route['manual_cell'] == dict(table='20-4', pdf_page=page, printed_page=page-1,
            source_signal=f'ADC_IN{channel}', mux_bits=f'{channel:04b}', pin=pin, sha256=sources['reference_manual']['sha256'])
        row = next(r for r in pinouts['table_rows'] if pin in r['signals'])
        assert row['pin_type'] == 'I/O' and not set(row['signals']) & {'SWCLK', 'SWDIO', 'NRST', 'BOOT'}
        assert pin not in FORBIDDEN[profile]
        assert route['pin_cell'] == dict(pin=pin, source_name=row['source_name'], source_signal=f'ADC_IN{channel}',
            table='5-2', pdf_page=row['pdf_page_index']+1, printed_page=row['pdf_page_index']+1-offset,
            positions=row['positions'], pin_type=row['pin_type'], sha256=sources['datasheet']['sha256'])
        assert route['package_pins'] == {p['name']: row['positions'][p['table_column']] for p in pinouts['packages']}
        for package in pinouts['packages']:
            bonded = route['package_pins'][package['name']] is not None
            assert (pin in package['gpio_pins_preserving_swd']) == bonded
        assert route['oscillator_aliases'] == [s for s in row['signals'] if s.startswith('OSC')]
    assert len(data['internal_sources']) == 2
    for index, (signal, macro, source_name) in enumerate([
            ('TEMPERATURE', 'ADC_InputTs', 'TS 内置温度传感器'),
            ('BGR1P2', 'ADC_InputVref1P2', '1.2V 内核电压基准源')]):
        mux = index + 14
        assert data['internal_sources'][index] == dict(signal=signal, mux=mux, source_macro=macro,
            source_line=macro_start+mux, source_sha256=sources['adc_header']['sha256'],
            manual_cell=dict(table='20-4', pdf_page=page, printed_page=page-1, mux_bits=f'{mux:04b}',
                             source_signal=source_name, gpio='-', sha256=sources['reference_manual']['sha256']))
    return data['routes']


def analog_grid(pdf, profile):
    """Read the original PDF's analog column, then pin and package coordinates.

    The source pinout extractor independently cross-checks physical positions
    with Poppler text. It does not provide these ADC names: they are extracted
    here only from the rightmost analog cells, not from SDK comments or sidecars.
    """
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
            assert len(tables) == 1, (profile, index)
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
                box, analog = row.cells[0], row.cells[-1]
                if box is None or analog is None: continue
                analog_words = [w for w in words if analog[0] < cx(w) < analog[2] and analog[1] < cy(w) < analog[3]]
                channels = {int(m[1]) for w in analog_words for m in re.finditer(r'ADC_IN(\d+)\b', w['text'])}
                if not channels: continue
                selected = [w for w in words if box[1] < cy(w) < box[3]]
                pins = [norm(w['text'].rstrip('/')) for w in selected if name_x-half_width < cx(w) < name_x+half_width and re.fullmatch(r'P[A-F]\d{2}/?', w['text'])]
                assert len(channels) == len(pins) == 1, (profile, index, channels, pins)
                numbers = [w['text'] for w in sorted(selected, key=cx) if cx(w) < name_x-half_width and re.fullmatch(r'\d+|-', w['text'])]
                assert len(numbers) == len(columns), (profile, index, numbers)
                channel = channels.pop()
                assert channel not in result
                result[channel] = (pins[0], index+1, dict(zip(columns, [None if n == '-' else n for n in numbers])))
    return result


def verify_sources(directory, profile):
    data = read(ROOT / f'cw32-data/af/{profile.lower()}-analog.yaml')
    sources = data['sources']
    for key, source in sources.items():
        assert sha((ROOT if key == 'pinouts' else directory) / source['file']) == source['sha256'], (profile, key)
    page, _, macro_start, comment_start, column = SPECS[profile]
    manual = subprocess.check_output(['pdftotext', '-f', str(page), '-l', str(page), '-layout',
                                     str(directory / sources['reference_manual']['file']), '-'], text=True)
    assert re.search(r'表\s*20-4', manual)
    muxes = {int(m[2]): (norm(m[3]), int(m[1], 2)) for m in re.finditer(r'([01]{4})\s+ADC_IN(\d+)\s+(P[A-F]\d+)', manual)}
    assert muxes == {r['channel']: (r['pin'], r['mux']) for r in data['routes']}
    assert re.search(r'1110\s+TS\s*内置温度传感器\s+-', manual)
    assert re.search(r'1111\s+1\.2V\s*内核电压基准源\s+-', manual)
    header = (directory / sources['adc_header']['file']).read_text().splitlines()
    if profile == 'CW32L011': assert re.search(r'L010\s+L011', header[187])
    grid = analog_grid(directory / sources['datasheet']['file'], profile)
    assert set(grid) == set(range(14))
    for route in data['routes']:
        assert re.fullmatch(r'#define\s+' + route['source_macro'] + r'\s+\(\(uint32_t\)0x' + f"{route['mux']:08X}" + r'\)\s*', header[route['source_line']-1])
        comments = re.findall(r'通道(\d+)输入(P[A-F]\d+)', header[route['sdk_pin_comment_line']-1])
        assert len(comments) == column+1
        number, pin = comments[column]
        assert (int(number), norm(pin)) == (route['channel'], route['pin'])
        assert grid[route['channel']] == (route['pin'], route['pin_cell']['pdf_page'], route['pin_cell']['positions'])
    for internal in data['internal_sources']:
        assert re.fullmatch(r'#define\s+' + internal['source_macro'] + r'\s+\(\(uint32_t\)0x' + f"{internal['mux']:08X}" + r'\)\s*', header[internal['source_line']-1])
    print(f'PASS {profile}: own RM muxes, own-family SDK column, original PDF analog-cell/package coordinates, TS14/BGR15')


def verify_generated(data_dir, profile):
    routes = qualified_routes(profile)
    manifest = read(ROOT / f'cw32-data/inputs/{profile.lower()}.yaml')
    assert manifest['analog_metadata'] == f'cw32-data/af/{profile.lower()}-analog.yaml'
    assert manifest['alias_pin_policy'] == 'common-package-intersection'
    pinouts = read(ROOT / f'cw32-data/pinouts/{profile.lower()}.yaml')
    sets = []
    counts = []
    for part in [p['name'] for p in pinouts['packages']] + [profile]:
        core = read(data_dir / 'chips' / (part+'.json'))['cores'][0]
        pins = {p['name'] for p in core['pins']}
        adc = next(p for p in core['peripherals'] if p['name'] == 'ADC')
        assert adc['registers']['version'] == profile.lower()+'_v1'
        got = {(r['pin'], r['signal'], r['adc_mux'], r.get('af')) for r in adc['pins']}
        expected = {(r['pin'], r['signal'], r['mux'], None) for r in routes if r['pin'] in pins}
        assert got == expected, part
        assert len(adc['pins']) == len(expected) == COUNTS[part]
        assert not {r['pin'] for r in adc['pins']} & FORBIDDEN[profile]
        assert all(r['adc_mux'] < 14 for r in adc['pins'])
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
    for profile in FAMILIES:
        qualified_routes(profile)
        if args.sources: verify_sources(args.sources, profile)
        if not args.sidecars_only: verify_generated(args.data_dir, profile)
    print('PASS two-family low ADC route qualification; logical signals and hardware muxes remain separate')


if __name__ == '__main__': main()
