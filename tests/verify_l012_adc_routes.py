#!/usr/bin/env python3
"""Independently check CW32L012 dual-ADC routing; no silicon is executed.

--sources rehashes the official artifacts, reads original PDF pin-table grids
and manual mux rows, and verifies SDK macro/comment coordinates.
--sidecars-only permits running before generated metadata has been replaced.
"""
import argparse
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
SIDECAR = 'cw32-data/af/cw32l012-analog.yaml'
PINS = {
    'ADC1': [f'PA{i}' for i in range(8)] + ['PB0', 'PB1', 'PB10', 'PB2'],
    'ADC2': ['PA5', 'PA6', 'PA7', 'PB0', 'PB1', 'PA8', 'PA9', 'PA10',
             'PA11', 'PA12', 'PB10', 'PB2'],
}
SOURCE_IDS = {
    'datasheet': ('CW32L012_DataSheet_CN_V1.0.pdf', '08a605fe8843d1eefa2b1115474b5a74df2833068f0ad6f35225e4f39377da76'),
    'reference_manual': ('CW32L012_UserManual_CN_V1.4.pdf', 'a9e54694a26f03c1f3e3041f40844900168ca2b8e6142f3328671b92e6b7a340'),
    'sdk_archive': ('CW32L012_StandardPeripheralLib_V1.0.5.zip', '8b0a4c0ee865642d08f353aec98906c900a8b10f672120e649e6b1bf5e29c18f'),
    'adc_header': ('cw32l012/Libraries/inc/cw32l012_adc.h', '13a189fef19ca32b64c6fa289c2e144069e8961ed4cf72bdbe62cd190266b045'),
    'pinouts': ('cw32-data/pinouts/cw32l012.yaml', '70ad225317acd9b1bd51a19fff15f6834f3efb4f58563b33344ab5d69eab07fb'),
}


def read(path): return hardware_facts(yaml.safe_load(path.read_text()) if path.suffix == '.yaml' else json.loads(path.read_text()))
def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()
def norm(pin): return pin[:2] + str(int(pin[2:]))


def qualified_routes():
    data = read(ROOT / SIDECAR)
    pinouts = read(ROOT / SOURCE_IDS['pinouts'][0])
    assert (data['schema_version'], data['profile'], data['kind'], data['register_version']) == (1, 'CW32L012', 'analog', 'cw32l012_v1')
    assert data['status'] == 'verified-sdk-datasheet-reference-manual'
    assert data['alias_pin_policy'] == 'common-package-intersection'
    for key, (path, digest) in SOURCE_IDS.items():
        assert (data['sources'][key]['file'], data['sources'][key]['sha256']) == (path, digest)
    assert sha(ROOT / SOURCE_IDS['pinouts'][0]) == SOURCE_IDS['pinouts'][1]
    assert pinouts['family'] == 'CW32L012'
    assert pinouts['source']['sha256'] == SOURCE_IDS['datasheet'][1]
    assert len(data['routes']) == 24
    assert {p['name'] for p in pinouts['packages']} == {'CW32L012C8T6', 'CW32L012C8U6'}
    expected = [(adc, channel, pin) for adc, pins in PINS.items() for channel, pin in enumerate(pins)]
    for route, (adc, channel, pin) in zip(data['routes'], expected):
        source_signal = f'{adc}_IN{channel}'
        assert (route['peripheral'], route['pin'], route['signal'], route['source_signal'], route['channel'], route['mux'], route['af']) == (adc, pin, f'IN{channel}', source_signal, channel, channel, None)
        assert route['source_macro'] == f'ADC_InputCH{channel}'
        assert route['source_line'] == 222 + channel
        assert route['sdk_pin_comment_line'] == {'ADC1': 187, 'ADC2': 205}[adc] + channel
        assert route['sdk_comment_pin'] == pin
        assert route['source_kind'] == 'sdk-mux-and-datasheet-reference-manual'
        assert route['source_sha256'] == SOURCE_IDS['adc_header'][1]
        assert route['manual_cell'] == dict(table='25-4', pdf_page=604, printed_page=578,
            instance=adc, source_signal=f'ADCx_IN{channel}', mux_bits=f'{channel:04b}',
            pin=pin, sha256=SOURCE_IDS['reference_manual'][1])
        row = next(r for r in pinouts['table_rows'] if pin in r['signals'])
        assert row['pin_type'] == 'I/O' and not set(row['signals']) & {'SWCLK', 'SWDIO', 'NRST', 'BOOT'}
        assert route['pin_cell'] == dict(pin=pin, source_name=row['source_name'], source_signal=source_signal,
            table='5-2', pdf_page=row['pdf_page_index']+1, printed_page=row['pdf_page_index']-2,
            positions=row['positions'], pin_type=row['pin_type'], sha256=SOURCE_IDS['datasheet'][1])
        assert route['package_pins'] == {p['name']: row['positions'][p['table_column']] for p in pinouts['packages']}
        assert route['oscillator_aliases'] == []
    manifest = read(ROOT / 'cw32-data/inputs/cw32l012.yaml')
    assert manifest['analog_metadata'] == SIDECAR
    assert manifest['alias_pin_policy'] == 'common-package-intersection'
    return data['routes']


def analog_grid(pdf):
    """Read each ADC instance label and package column from original PDF rows."""
    import pdfplumber
    sys.path.insert(0, str(ROOT / 'cw32-data/tools'))
    from extract_pinouts import SPECS, extract_rows
    pages, columns, _ = SPECS['CW32L012']
    pinouts = read(ROOT / SOURCE_IDS['pinouts'][0])
    assert extract_rows(pdf, 'CW32L012') == pinouts['table_rows']
    result = {}
    with pdfplumber.open(pdf) as document:
        for index in pages:
            page = document.pages[index]
            tables = [t for t in page.find_tables() if any(marker in str(t.extract()[:2]) for marker in ('NFQ', 'PFQL'))]
            assert len(tables) == 1
            table = tables[0]
            def cx(w): return (w['x0'] + w['x1']) / 2
            def cy(w): return (w['top'] + w['bottom']) / 2
            words = [w for w in page.extract_words() if table.bbox[1] < cy(w) < table.bbox[3]]
            names = [w for w in words if re.fullmatch(r'P[A-F]\d{2}/?', w['text']) and cx(w) < 300]
            name_x = statistics.median(cx(w) for w in names)
            type_x = statistics.median(cx(w) for w in words if w['text'] == 'I/O' and name_x < cx(w) < name_x+70)
            half_width = (type_x-name_x) / 2
            for row in table.rows:
                if row.cells[0] is None:
                    continue
                box = row.cells[0]
                selected = [w for w in words if box[1] < cy(w) < box[3]]
                channels = {(m[1], int(m[2])) for w in selected for m in re.finditer(r'(ADC[12])_IN(\d+)', w['text'])}
                if not channels:
                    continue
                pins = [norm(w['text'].rstrip('/')) for w in selected if name_x-half_width < cx(w) < name_x+half_width and re.fullmatch(r'P[A-F]\d{2}/?', w['text'])]
                assert len(pins) == 1, (index, pins)
                numbers = [w['text'] for w in sorted(selected, key=cx) if cx(w) < name_x-half_width and re.fullmatch(r'\d+|-', w['text'])]
                assert len(numbers) == len(columns)
                for channel in channels:
                    assert channel not in result
                    result[channel] = (pins[0], index+1, dict(zip(columns, [None if n == '-' else n for n in numbers])))
    return result


def verify_sources(directory):
    routes = qualified_routes()
    for key, (path, digest) in SOURCE_IDS.items():
        assert sha((ROOT if key == 'pinouts' else directory) / path) == digest, key
    manual = subprocess.check_output(['pdftotext', '-f', '604', '-l', '604', '-layout',
                                     str(directory / SOURCE_IDS['reference_manual'][0]), '-'], text=True)
    assert '25-4' in manual and '578' in manual
    muxes = {}
    for line in manual.splitlines():
        match = re.match(r'\s*([01]{4})\s+ADCx_IN(\d+)\s+(.*)', line)
        if not match:
            continue
        mux, channel = int(match[1], 2), int(match[2])
        pins = re.findall(r'P[A-F]\d{2}', match[3])
        if channel >= 12:
            assert not pins and 'DAC' in match[3]
            continue
        assert mux == channel and len(pins) == (2 if channel < 10 else 1)
        for adc, pin in zip(PINS, pins if len(pins) == 2 else pins * 2):
            muxes[(adc, channel)] = (norm(pin), mux)
    assert muxes == {(r['peripheral'], r['channel']): (r['pin'], r['mux']) for r in routes}
    header = (directory / SOURCE_IDS['adc_header'][0]).read_text().splitlines()
    grid = analog_grid(directory / SOURCE_IDS['datasheet'][0])
    assert set(grid) == {(adc, channel) for adc in PINS for channel in range(12)}
    for route in routes:
        assert re.search(route['source_macro'] + r'\s+\(\(uint32_t\)0x' + f"{route['mux']:08X}" + r'\)', header[route['source_line']-1])
        pin = route['sdk_comment_pin']
        assert f"通道{route['channel']}输入{pin[:2]}{int(pin[2:]):02d}" in header[route['sdk_pin_comment_line']-1]
        assert grid[(route['peripheral'], route['channel'])] == (route['pin'], route['pin_cell']['pdf_page'], route['pin_cell']['positions'])
    print('PASS own CN1.4 manual, CN1.0 PDF pin grids and V1.0.5 SDK: 24 external dual-ADC routes')


def verify_generated(data_dir):
    routes = qualified_routes()
    pinouts = read(ROOT / SOURCE_IDS['pinouts'][0])
    sets = []
    for part in [p['name'] for p in pinouts['packages']] + ['CW32L012']:
        core = read(data_dir / 'chips' / (part + '.json'))['cores'][0]
        pins = {p['name'] for p in core['pins']}
        got = set()
        for adc in core['peripherals']:
            if adc['name'] not in PINS:
                assert all(r.get('adc_mux') is None for r in adc.get('pins', []))
                continue
            assert adc['registers'] == dict(block='ADC', kind='adc', version='cw32l012_v1')
            assert len(adc['pins']) == 12
            for route in adc['pins']:
                assert route['adc_mux'] < 12 and route.get('af') is None
                got.add((adc['name'], route['pin'], route['signal'], route['adc_mux']))
        expected = {(r['peripheral'], r['pin'], r['signal'], r['mux']) for r in routes if r['pin'] in pins}
        assert got == expected and len(got) == 24, part
        if part == 'CW32L012':
            assert got == set.intersection(*sets)
        else:
            sets.append(got)
        print(f'PASS {part}: ADC1=12, ADC2=12 external routes; explicit per-instance muxes')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--sources', type=Path)
    parser.add_argument('--sidecars-only', action='store_true')
    parser.add_argument('--data-dir', type=Path, default=ROOT / 'cw32-data/data')
    args = parser.parse_args()
    qualified_routes()
    if args.sources:
        verify_sources(args.sources)
    if not args.sidecars_only:
        verify_generated(args.data_dir)
    print('PASS CW32L012 dual-ADC route qualification; internal DAC mux12/13 excluded')


if __name__ == '__main__':
    main()
