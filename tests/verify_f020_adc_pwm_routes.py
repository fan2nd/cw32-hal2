#!/usr/bin/env python3
"""Verify qualified F020 ADC/PWM routes and exact package/alias projections.

--sources rehashes official files, independently parses original PDF AF columns,
ADC mux rows and pin-table grid cells, and checks exact SDK header macros.
--sidecars-only skips generated data when reviewing before regeneration.
No files are changed, no silicon or broader peripheral support is inferred.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess

from verify_x030_af_tables import table
import yaml
from route_metadata import hardware_facts

ROOT = Path(__file__).resolve().parents[1]
PARTS = {'CW32F020F6U7': ('QFN20', 9, 17),
         'CW32F020K6U7': ('QFN32', 11, 32),
         'CW32F020C6U7': ('QFN48', 13, 46)}
ADC_PINS = [f'PA{i}' for i in range(8)] + ['PB0', 'PB1', 'PB2', 'PB10', 'PB11']
RESTRICTED = {'PA13', 'PA14', 'PF3', 'NRST'}


def read(path):
    return hardware_facts(yaml.safe_load(path.read_text()) if path.suffix == '.yaml' else json.loads(path.read_text()))


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def pdf_text(path, page=None):
    return subprocess.check_output(['pdftotext'] + ([] if page is None else ['-f', str(page), '-l', str(page)])
                                   + ['-layout', str(path), '-'], text=True)


def analog_grid(pdf):
    """Read actual PDF table row bounds, independent of stored pinout metadata."""
    import pdfplumber
    result = {}
    with pdfplumber.open(pdf) as doc:
        for index in (22, 23):
            page = doc.pages[index]
            tables = [t for t in page.find_tables() if any(marker in str(t.extract()[:2])
                      for marker in ('NFQ', 'PFQL', 'POSST'))]
            assert len(tables) == 1, 'Ambiguous F020 pin table'
            words = page.extract_words()
            for row in tables[0].rows:
                if row.cells[0] is None:
                    continue
                box = row.cells[0]
                selected = [w for w in words if box[1] < (w['top'] + w['bottom']) / 2 < box[3]]
                names = [w['text'] for w in selected if re.fullmatch(r'P[AB]\d{2}', w['text']) and w['x0'] < 230]
                channels = [re.search(r'ADC_IN(\d+)', w['text']) for w in selected]
                channels = [int(m[1]) for m in channels if m]
                if not channels:
                    continue
                assert len(channels) == len(names) == 1
                pin = names[0][:2] + str(int(names[0][2:]))
                name_x = min(w['x0'] for w in selected if w['text'] == names[0])
                numbers = [w['text'] for w in sorted(selected, key=lambda w: w['x0'])
                           if w['x0'] < name_x and re.fullmatch(r'\d+|-', w['text'])]
                assert len(numbers) == 3, (pin, numbers)
                assert channels[0] not in result
                result[channels[0]] = (pin, index + 1, dict(zip(
                    ['QFN48', 'QFN32', 'QFN20'], [None if n == '-' else n for n in numbers])))
    assert set(result) == set(range(13))
    return result


def verify_sidecars():
    adc = read(ROOT / 'cw32-data/af/cw32f020-analog.yaml')
    pwm = read(ROOT / 'cw32-data/af/cw32f020-pwm.yaml')
    proof = read(ROOT / 'docs/f020-adc-pwm-route-evidence.json')
    pinouts = read(ROOT / 'cw32-data/pinouts/cw32f020.yaml')
    candidate = read(ROOT / 'cw32-data/af/cw32f020.yaml')
    sources = proof['sources']
    assert candidate['status'] == 'candidate-sdk-and-register-verified'
    assert adc['sources'] == pwm['sources'] == sources
    for name in ['sdk_candidates', 'pinouts']:
        assert sha(ROOT / sources[name]['file']) == sources[name]['sha256']
    assert len(adc['routes']) == 13 and len(pwm['routes']) == 46
    assert proof['source_cells'] == {'gtim': 73, 'pwm': 46, 'other_gtim': 27, 'adc': 13}
    sdk_gtim = {(r['pin'], r['af']): r for r in candidate['routes'] if r.get('peripheral', '').startswith('GTIM')}
    assert len(sdk_gtim) == len(proof['verified_gtim_cells']) == 73
    for item in proof['verified_gtim_cells']:
        sdk = sdk_gtim[(item['pin'], item['af'])]
        assert {k: item[k] for k in sdk} == sdk
        assert item['datasheet_function'] == sdk['function']
    assert pinouts['source']['sha256'] == sources['datasheet']['sha256']
    for kind, sidecar in [('analog', adc), ('pwm', pwm)]:
        assert sidecar['profile'] == 'CW32F020' and sidecar['kind'] == kind
        assert sidecar['alias_pin_policy'] == 'common-package-intersection'
        identities = set()
        for route in sidecar['routes']:
            pin = route['pin']
            assert pin not in RESTRICTED
            row = next(r for r in pinouts['table_rows'] if pin in r['signals'])
            assert row['pin_type'] == 'I/O'
            assert not set(row['signals']) & {'SWCLK', 'SWDIO', 'NRST', 'BOOT'}
            assert route['pin_cell'] == dict(pin=pin, source_name=row['source_name'], table='5-2',
                pdf_page=row['pdf_page_index'] + 1, printed_page=row['pdf_page_index'],
                positions=row['positions'], pin_type='I/O', sha256=sources['datasheet']['sha256'])
            assert route['package_pins'] == {p['name']: row['positions'][p['table_column']] for p in pinouts['packages']}
            assert route['oscillator_aliases'] == [s for s in row['signals'] if s.startswith('OSC')]
            if kind == 'analog':
                mux = route['mux']
                assert pin == ADC_PINS[mux]
                assert route['peripheral'] == 'ADC' and route['af'] is None
                assert route['channel'] == mux and route['signal'] == f'IN{mux}'
                assert route['source_signal'] == f'ADC_IN{mux}'
                assert route['source_macro'] == f'ADC_ExInputCH{mux}'
                assert route['source_line'] == 206 + mux and route['sdk_pin_comment_line'] == 189 + mux
                assert route['source_sha256'] == sources['adc_header']['sha256']
                assert route['manual_cell'] == dict(table='21-5', pdf_page=378, printed_page=377,
                    source_signal=f'AIN{mux}', mux_bits=f'{mux:04b}', pin=pin,
                    sha256=sources['reference_manual']['sha256'])
                identity = (pin, mux)
            else:
                sdk = sdk_gtim[(pin, route['af'])]
                assert {k: route[k] for k in sdk} == sdk
                assert re.fullmatch(r'GTIM[1-4]', route['peripheral'])
                assert route['signal'] in ('CH1', 'CH2', 'CH3', 'CH4')
                assert route['channel'] == int(route['signal'][-1])
                assert route['source_signal'] == route['signal'] and route['source_kind'] == 'sdk-and-datasheet'
                assert route['source_sha256'] == sources['gpio_header']['sha256']
                page = 26 if pin[1] == 'A' else 27
                assert route['datasheet_cell'] == dict(pin=pin, af=route['af'],
                    table={'A': '5-3', 'B': '5-4', 'C': '5-5', 'F': '5-6'}[pin[1]],
                    pdf_page=page + 1, printed_page=page,
                    function=f"{route['peripheral']}_{route['signal']}", sha256=sources['datasheet']['sha256'])
                identity = (pin, route['peripheral'], route['signal'])
            assert identity not in identities
            identities.add(identity)
        for part, (package, analog_count, pwm_count) in PARTS.items():
            expected = analog_count if kind == 'analog' else pwm_count
            assert sum(r['package_pins'][part] is not None for r in sidecar['routes']) == expected
            assert proof['packages'][part] == dict(package=package, analog=analog_count, pwm=pwm_count)
    assert {r['mux'] for r in adc['routes']} == set(range(13))
    manifest = read(ROOT / 'cw32-data/inputs/cw32f020.yaml')
    assert manifest['af_metadata'] == 'cw32-data/af/cw32f020-serial.yaml'
    assert manifest['analog_metadata'] == 'cw32-data/af/cw32f020-analog.yaml'
    assert manifest['pwm_metadata'] == 'cw32-data/af/cw32f020-pwm.yaml'
    assert manifest['alias_pin_policy'] == 'common-package-intersection'
    print('PASS F020 qualified sidecars: exact provenance,13 ADC mux tuples,46 PWM channels,3 package joins')
    return adc, pwm, proof


def verify_sources(directory, adc, pwm, proof):
    sources = proof['sources']
    audit = read(ROOT / 'docs/adc-timer-next-batch-evidence.json')
    for file, source in audit['sources'].items():
        if 'f020' in file.lower():
            assert sha(directory / file) == source['sha256'], file
    for key, source in sources.items():
        if key not in ('sdk_candidates', 'pinouts'):
            assert sha(directory / source['file']) == source['sha256'], key
    pdf = directory / sources['datasheet']['file']
    cells = {key: value for key, value in table(pdf_text(pdf)).items() if value.startswith('GTIM')}
    assert cells == {(r['pin'], r['af']): r['datasheet_function'] for r in proof['verified_gtim_cells']}
    gpio = (directory / sources['gpio_header']['file']).read_text().splitlines()
    for route in proof['verified_gtim_cells']:
        line = gpio[route['source_line'] - 1]
        assert route['source_macro'] + '()' in line
        assert re.search(r'CW_GPIO' + route['pin'][1] + '->' + route['gpio_register'] + r'_f\.'
                         + route['gpio_field'] + r'\s*=\s*' + str(route['af']) + r'\)', line)
    manual = pdf_text(directory / sources['reference_manual']['file'], 378)
    mux = {int(m[2]): (m[3][:2] + str(int(m[3][2:])), int(m[1], 2))
           for m in re.finditer(r'([01]{4})\s+AIN(\d+)\s+(P[AB]\d+)', manual)}
    assert mux == {r['channel']: (r['pin'], r['mux']) for r in adc['routes']}
    header = (directory / sources['adc_header']['file']).read_text().splitlines()
    grid = analog_grid(pdf)
    for route in adc['routes']:
        n, pin = route['mux'], route['pin']
        assert f'通道{n}输入{pin[:2]}{int(pin[2:]):02d}' in header[route['sdk_pin_comment_line'] - 1]
        assert re.search(route['source_macro'] + r'\s+\(\(uint32_t\)0x' + f'{n:08X}' + r'\)',
                         header[route['source_line'] - 1])
        assert grid[n] == (pin, route['pin_cell']['pdf_page'], route['pin_cell']['positions'])
    print('PASS original sources: all15 snapshot hashes;73 GTIM PDF/SDK AF cells;13 ADC RM/SDK/PDF-grid rows')


def verify_generated(data, adc, pwm):
    route_sets = {}
    for part, (_, analog_count, pwm_count) in dict(PARTS, CW32F020=('alias', 9, 17)).items():
        chip = read(data / 'chips' / (part + '.json'))
        core = chip['cores'][0]
        bonded = {p['name'] for p in core['pins']}
        actual_adc, actual_pwm, actual_capture = set(), set(), set()
        assert not any(p['name'] == 'ATIM' for p in core['peripherals'])
        for peripheral in core['peripherals']:
            if peripheral['name'] == 'ADC':
                assert peripheral['registers']['version'] == 'cw32f020_v1'
                actual_adc = {(r['pin'], r['signal'], r['adc_mux'], r.get('af')) for r in peripheral.get('pins', [])}
            if peripheral['name'].startswith('GTIM'):
                assert peripheral['registers']['version'] == 'v1'
                for r in peripheral.get('pins', []):
                    target = actual_capture if r['signal'].startswith('CAP') else actual_pwm
                    target.add((peripheral['name'], r['pin'], r['signal'], r['af']))
        expected_adc = {(r['pin'], r['signal'], r['mux'], None) for r in adc['routes'] if r['pin'] in bonded}
        expected_pwm = {(r['peripheral'], r['pin'], r['signal'], r['af']) for r in pwm['routes'] if r['pin'] in bonded}
        assert actual_adc == expected_adc and len(actual_adc) == analog_count, part
        assert actual_pwm == expected_pwm and len(actual_pwm) == pwm_count, part
        assert actual_capture == {(p, pin, "CAP" + signal[2:], af) for p, pin, signal, af in expected_pwm}, part
        assert not any(r[0] in RESTRICTED for r in actual_adc)
        assert not any(r[1] in RESTRICTED for r in actual_pwm)
        route_sets[part] = (actual_adc, actual_pwm)
    for i in (0, 1):
        assert route_sets['CW32F020'][i] == set.intersection(*(route_sets[p][i] for p in PARTS))
    print('PASS generated F020 exact-part/alias routes: ADC9/11/13, PWM17/32/46; alias equals intersection')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--sources', type=Path)
    parser.add_argument('--data-dir', type=Path, default=ROOT / 'cw32-data/data')
    parser.add_argument('--sidecars-only', action='store_true')
    args = parser.parse_args()
    adc, pwm, proof = verify_sidecars()
    if args.sources:
        verify_sources(args.sources, adc, pwm, proof)
    if not args.sidecars_only:
        verify_generated(args.data_dir, adc, pwm)


if __name__ == '__main__':
    main()
