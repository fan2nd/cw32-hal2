#!/usr/bin/env python3
"""Qualify L010/L011/L012 GTIM CH1-4 routes from each family's own sources.

--sources independently parses the original datasheet and reference-manual AF
columns, SDK GPIO assignments, CMSIS instance addresses, and physical pin grids.
--write requires --sources and changes only the three PWM sidecars and evidence.
--sidecars-only does not need generated metadata. No silicon claim is made.
"""
import argparse
from copy import deepcopy
from functools import lru_cache
import hashlib
import json
from pathlib import Path
import re
import sys
import zipfile
import yaml
from route_metadata import hardware_facts, with_source_refs

ROOT = Path(__file__).resolve().parents[1]
FAMILIES = ('L010', 'L011', 'L012')
PWM = re.compile(r'(GTIM[1-4]?)_(CH[1-4])')
SDK_PWM = re.compile(r'(GTIM[1-4]?)(CH[1-4])')
COUNTS = {'L010': 9, 'L011': 19, 'L012': 41}
RETAINED = {'L010': 8, 'L011': 17, 'L012': 37}
VERSIONS = {'L010': 'cw32l010_v1', 'L011': 'cw32l010_v1', 'L012': 'cw32l012_v1'}
MAX_AF = {'L010': 7, 'L011': 7, 'L012': 9}
PAGE_OFFSET = {'L010': 1, 'L011': 3, 'L012': 3}
MANUAL_AF = {'L010': ('8-2', (123,), 1), 'L011': ('8-2', (122, 123), 1),
             'L012': ('9-2', (154, 155), 26)}
BASE_PAGES = {'L010': 256, 'L011': 256, 'L012': 309}
ADDRESSES = {'L010': {'GTIM1': 0x40001800},
             'L011': {'GTIM1': 0x40001800, 'GTIM2': 0x40001c00},
             'L012': {'GTIM1': 0x40001800, 'GTIM2': 0x40001c00,
                      'GTIM3': 0x40002400, 'GTIM4': 0x40002800}}
CMSIS_HASHES = {
    'L010': '28c3feca7a8930afd2452a598e1753d2782bdbb120fc8e66e64a6d807674730b',
    'L011': '2ae110a7a35e26be4816f6eeb615152eb52469da095d8d7a282df4aa80d946b8',
    'L012': '3758779c7b9e60fda1aa80dfd2848b6986074d91a6763076fa6777d55b1ad000',
}
PROOF = 'docs/buffered-pwm-route-evidence.json'
RAW_FIELDS = ('pin', 'af', 'function', 'source_macro', 'source_line',
              'gpio_register', 'gpio_field')
ROUTE_FIELDS = RAW_FIELDS + ('peripheral', 'signal')


def read(path): return hardware_facts(yaml.safe_load(path.read_text()) if path.suffix == '.yaml' else json.loads(path.read_text()))
def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()
def digest(value):
    return hashlib.sha256(json.dumps(hardware_facts(value), sort_keys=True, separators=(',', ':'),
                                    ensure_ascii=False).encode()).hexdigest()


def pdf_lines(page):
    return [(line['bbox'], ''.join(span['text'] for span in line['spans']))
            for block in page.get_text('dict')['blocks']
            for line in block.get('lines', [])]


def extract_cells(region, columns, table, page_index, offset, port=None):
    """Nearest explicit column/row centers preserve empty function columns."""
    pins = [(f'P{m[1]}{int(m[2])}', (w[1]+w[3])/2) for w in region
            if (m := re.fullmatch(r'P([A-F])(\d+)(?:/(?:SWDIO|SWCLK|BOOT)?)?', w[4]))
            and w[2] < min(columns.values())]
    result = {}
    for word in region:
        if not PWM.fullmatch(word[4]): continue
        x, y = (word[0]+word[2])/2, (word[1]+word[3])/2
        af = min(columns, key=lambda key: abs(x-columns[key]))
        pin, pin_y = min(pins, key=lambda pair: abs(y-pair[1]))
        assert abs(x-columns[af]) < 1 and abs(y-pin_y) < 10, (word, pin, af)
        assert (port is None or pin[1] == port) and (pin, af) not in result
        result[pin, af] = dict(pin=pin, af=af, function=word[4], table=table,
            pdf_page=page_index+1, printed_page=page_index+1-offset,
            bbox=[round(value, 4) for value in word[:4]])
    return result


def datasheet_cells(path, family):
    import fitz
    result = {}
    with fitz.open(path) as document:
        for page_index, page in enumerate(document):
            lines, words = pdf_lines(page), page.get_text('words')
            headings = sorted((box[1], m[1], m[2]) for box, text in lines
                if (m := re.search(r'表\s*(5-\d+)\s*通过\s*GPIO([A-F])_AFR[Ly]', text)))
            for index, (top, table, port) in enumerate(headings):
                bottom = headings[index+1][0] if index+1 < len(headings) else 780
                columns = {int(m[1]): (box[0]+box[2])/2 for box, text in lines
                    if top < box[1] < bottom and (m := re.fullmatch(r'功能\s*([1-9])', text))}
                assert set(columns) == set(range(1, MAX_AF[family]+1)), (path, table)
                cells = extract_cells([w for w in words if top < w[1] < bottom],
                                      columns, table, page_index, PAGE_OFFSET[family], port)
                assert not result.keys() & cells.keys()
                result.update(cells)
    assert len(result) == COUNTS[family]
    return result


def manual_cells(path, family):
    """Independent own-manual AF0..AF9 allocation grids, not generic width prose."""
    import fitz
    table, pages, offset = MANUAL_AF[family]
    result = {}
    with fitz.open(path) as document:
        for number in pages:
            page = document[number-1]
            lines = pdf_lines(page)
            # AF0 uniquely identifies the allocation grid below the generic
            # selector description; continuation pages repeat this header.
            zeroes = [box for box, text in lines if text == 'AF0']
            assert len(zeroes) == 1
            top = zeroes[0][1]
            columns = {int(m[1]): (box[0]+box[2])/2 for box, text in lines
                       if abs(box[1]-top) < 1 and (m := re.fullmatch(r'AF([0-9])', text))}
            assert set(columns) == set(range(MAX_AF[family]+1)), (path, number)
            cells = extract_cells([w for w in page.get_text('words') if top < w[1] < 780],
                                  columns, table, number-1, offset)
            assert not result.keys() & cells.keys()
            result.update(cells)
    assert len(result) == COUNTS[family]
    return result


def sdk_cells(path):
    result = {}
    pattern = re.compile(r'^#define\s+(P([A-F])(\d+)_AFx_((GTIM[1-4]?)(CH[1-4])))'
        r'\(\)\s+\(CW_GPIO([A-F])->(AFR[HL])_f\.((?:AFR|PIN)(\d+))\s*=\s*(\d+)\)')
    for number, line in enumerate(path.read_text().splitlines(), 1):
        match = pattern.match(line)
        if not match: continue
        macro, port, n, function, peripheral, signal, bank, register, field, bit, af = match.groups()
        assert port == bank and int(n) == int(bit)
        assert register == ('AFRL' if int(n) < 8 else 'AFRH')
        pin, af = f'P{port}{int(n)}', int(af)
        assert (pin, af) not in result
        result[pin, af] = dict(pin=pin, af=af, function=function, source_macro=macro,
            source_line=number, gpio_register=register, gpio_field=field,
            peripheral=peripheral, signal=signal)
    return result


def sdk_candidate_cells(candidate):
    result = {}
    for section in ('routes', 'unresolved'):
        for original in candidate[section]:
            match = SDK_PWM.fullmatch(original['function'])
            if not match: continue
            row = {k: original[k] for k in RAW_FIELDS}
            row.update(peripheral=match[1], signal=match[2])
            for key in ('peripheral', 'signal'):
                if key in original: assert original[key] == row[key]
            assert (row['pin'], row['af']) not in result
            result[row['pin'], row['af']] = row
    return result


def normalize(family, pin, af, cell, sdk):
    peripheral, signal = PWM.fullmatch(cell['function']).groups()
    if family == 'L010':
        # The only GTIM is called GTIM in both manuals and GTIM1 by CMSIS/SVD.
        # The PA3 SDK macro alone omits the 1. Do not generalize the exception.
        assert peripheral == 'GTIM'
        expected = 'GTIMCH4' if (pin, af) == ('PA3', 6) else 'GTIM1'+signal
        assert sdk['function'] == expected
        peripheral = 'GTIM1'
    else:
        assert sdk['function'] == peripheral+signal
    assert peripheral in ADDRESSES[family]
    return peripheral, signal


def reason(pin, row):
    if row is None: return 'absent-from-own-package-pin-grid'
    if row['pin_type'] != 'I/O': return 'not-output-capable'
    if set(row['signals']) & {'SWDIO', 'SWCLK', 'NRST', 'BOOT'}:
        return 'debug-reset-or-boot-pad'
    if any(signal.startswith('OSC') for signal in row['signals']):
        return 'oscillator-ownership-not-provided'
    return None


def package_counts(pinouts, routes):
    result, sets = {}, []
    for package in pinouts['packages']:
        pins = set(package['gpio_pins_preserving_swd'])
        selected = {(r['peripheral'], r['pin'], r['signal'], r['af']) for r in routes if r['pin'] in pins}
        result[package['name']] = len(selected)
        sets.append(selected)
    result[pinouts['family']] = len(set.intersection(*sets))
    return result


def source_manifest(profile):
    family = profile[4:]
    analog = read(ROOT / f'cw32-data/af/{profile.lower()}-analog.yaml')
    candidate_path = f'cw32-data/af/{profile.lower()}.yaml'
    candidate = read(ROOT / candidate_path)
    sources = {k: deepcopy(v) for k, v in analog['sources'].items() if k != 'adc_header'}
    sources['sdk_archive']['url'] = candidate['source']['sdk_url']
    inc = str(Path(analog['sources']['adc_header']['file']).parent)
    sources['gpio_header'] = dict(file=f'{inc}/{profile.lower()}_gpio.h',
        sha256=candidate['source']['header_sha256'], url=candidate['source']['sdk_url'])
    sources['cmsis_header'] = dict(file=f'{inc}/{profile.lower()}.h',
        sha256=CMSIS_HASHES[family], url=candidate['source']['sdk_url'])
    sources['sdk_candidates'] = dict(file=candidate_path, sha256=sha(ROOT / candidate_path))
    return sources


def electrical_limits(family):
    output_page = {'L010': 46, 'L011': 54, 'L012': 61}[family]
    ac_page = 47 if family == 'L010' else output_page
    offset = PAGE_OFFSET[family]
    return dict(
        section='7.3.11',
        output_table='7-25' if family == 'L012' else '7-24',
        output_pdf_page=output_page, output_printed_page=output_page-offset,
        output_conditions='VDD = 3.3 V; own-family Table 7-4 general operating conditions apply.',
        voltage_at_10ma=dict(voh_min_v=2.95, vol_max_v=0.28, combined_output_limit_ma=40),
        voltage_at_20ma=dict(voh_min_v=2.7 if family == 'L010' else 2.55,
                            vol_max_v=0.60, combined_output_limit_ma=100),
        ac_table='7-26' if family == 'L012' else '7-25',
        ac_pdf_page=ac_page, ac_printed_page=ac_page-offset,
        ac_rows=[dict(load_pf=30, vddio_min_v=2.7, max_frequency_mhz=50, rise_fall_max_ns=5),
                 dict(load_pf=50, vddio_min_v=2.7, max_frequency_mhz=30, rise_fall_max_ns=8),
                 dict(load_pf=50, vddio_min_v=2.4, vddio_max_exclusive_v=2.7,
                      max_frequency_mhz=20, rise_fall_max_ns=12)],
        ac_basis='Vendor design/simulation data, not measured; maximum-frequency definition requires '
                 '(tr + tf) <= 2T/3 and 45-55% duty at CL. These are conditional pad limits, not a HAL PWM-rate guarantee.',
        restrictions=['Respect total VDD/VSS current including MCU consumption and absolute-maximum ratings.',
                      'Use digital output mode and select the exact AF; no oscillator/debug/reset ownership is changed.',
                      'Board load, voltage, signal integrity and PWM behavior require hardware validation.'])


def construct(profile, cells, manual, sdk, sources):
    family = profile[4:]
    assert cells.keys() == manual.keys() == sdk.keys()
    assert all(cells[k]['function'] == manual[k]['function'] for k in cells)
    pinouts = read(ROOT / sources['pinouts']['file'])
    rows = {signal: row for row in pinouts['table_rows'] for signal in row['signals']
            if re.fullmatch(r'P[A-F]\d+', signal)}
    routes, exclusions = [], []
    for key, cell in sorted(cells.items()):
        pin, af = key
        original = sdk[key]
        peripheral, signal = normalize(family, pin, af, cell, original)
        exclusion = reason(pin, rows.get(pin))
        if exclusion:
            exclusions.append(dict(pin=pin, af=af, function=cell['function'],
                reason=exclusion, datasheet_cell=cell, pin_signals=rows[pin]['signals']))
            continue
        row = rows[pin]
        route = dict(original)
        route.update(peripheral=peripheral, signal=signal,
            source_peripheral=original['peripheral'], source_signal=original['signal'],
            channel=int(signal[-1]), source_kind='sdk-and-datasheet',
            source_sha256=sources['gpio_header']['sha256'],
            datasheet_cell=dict({k: v for k, v in cell.items() if k != 'bbox'},
                               sha256=sources['datasheet']['sha256']),
            manual_cell=dict({k: v for k, v in manual[key].items() if k != 'bbox'},
                             sha256=sources['reference_manual']['sha256']),
            pin_cell=dict(pin=pin, source_name=row['source_name'], table='5-2',
                pdf_page=row['pdf_page_index']+1,
                printed_page=row['pdf_page_index']+1-PAGE_OFFSET[family],
                positions=row['positions'], pin_type=row['pin_type'], io_structure=row['io_structure'],
                sha256=sources['datasheet']['sha256']),
            package_pins={p['name']: row['positions'][p['table_column']] for p in pinouts['packages']},
            oscillator_aliases=[])
        assert any(value is not None for value in route['package_pins'].values())
        for package in pinouts['packages']:
            if route['package_pins'][package['name']] is not None:
                assert pin in package['gpio_pins_preserving_swd']
        routes.append(route)
    assert len(cells) == COUNTS[family] and len(routes) == RETAINED[family]
    assert len({(r['peripheral'], r['pin'], r['signal']) for r in routes}) == len(routes)
    datasheet = dict(sources['datasheet'], af_evidence=f'Own-family numbered AF columns in Tables '
        f"{', '.join(sorted({c['table'] for c in cells.values()}))}; independently cross-checked with "
        f'own reference-manual Table {MANUAL_AF[family][0]} and raw SDK. Per-cell proof: {PROOF}.')
    normalization = dict(
        instances={'GTIM': 'GTIM1'} if family == 'L010' else {},
        sdk_exception=dict(pin='PA3', af=6, function='GTIMCH4', peripheral='GTIM1', signal='CH4')
            if family == 'L010' else None,
        basis='Own manual register-list base addresses and exact own CMSIS GTIM instances; no sibling inheritance.',
        manual_base_pdf_page=BASE_PAGES[family],
        manual_base_printed_page=BASE_PAGES[family]-MANUAL_AF[family][2],
        instance_addresses={k: f'0x{v:08x}' for k, v in ADDRESSES[family].items()})
    sidecar = dict(schema_version=1, profile=profile, kind='pwm',
        status='verified-sdk-and-datasheet', register_version=VERSIONS[family], sources=sources,
        datasheet=datasheet, alias_pin_policy='common-package-intersection',
        selector_capability=dict(min=1, max=MAX_AF[family], pdf_printed_page_offset=PAGE_OFFSET[family],
            evidence='Actual numbered own-family datasheet AND manual allocation columns; no register-width inference.'),
        normalization=normalization, electrical_limits=electrical_limits(family),
        scope='GTIM CH1-4 outputs only; general AF candidates and ETR/TRGO/cascade routes remain unpromoted.',
        limitations=['Source-level route qualification only; no silicon or electrical validation.',
            'Exact-package bonding is required; family aliases use the common-package intersection.',
            'Debug, reset, boot, input-only and oscillator-alias pads are excluded.',
            'No ATIM, ETR, TRGO, capture, DMA, cascade or remapping authorization is implied.'],
        routes=routes, excluded_sdk_routes=[], excluded_safety_routes=exclusions)
    proof = dict(profile=profile, sources=sources, pdf_cells=list(cells.values()),
        manual_cells=list(manual.values()), normalization=normalization,
        electrical_limits=electrical_limits(family), sdk_only_excluded=[], safety_excluded=exclusions,
        counts=dict(pdf_pwm=len(cells), manual_pwm=len(manual), sdk_pwm=len(sdk),
                    retained=len(routes), sdk_only=0, safety=len(exclusions)),
        package_counts=package_counts(pinouts, routes), routes_sha256=digest(routes))
    return sidecar, proof


def build_sources(directory, profile):
    import fitz
    family = profile[4:]
    sources = source_manifest(profile)
    for key, source in sources.items():
        assert sha((ROOT if key in ('pinouts', 'sdk_candidates') else directory) / source['file']) == source['sha256'], (profile, key)
    # Prove that the locked extracted headers are members of this exact
    # own-family SDK archive, rather than similarly named standalone files.
    with zipfile.ZipFile(directory / sources['sdk_archive']['file']) as archive:
        for key in ('gpio_header', 'cmsis_header'):
            source = sources[key]
            suffix = 'Libraries/inc/' + Path(source['file']).name
            members = [name for name in archive.namelist() if name.endswith(suffix)]
            assert len(members) == 1, (profile, key, 'ambiguous SDK member')
            assert hashlib.sha256(archive.read(members[0])).hexdigest() == source['sha256']
    pdf = directory / sources['datasheet']['file']
    manual_pdf = directory / sources['reference_manual']['file']
    cells = datasheet_cells(pdf, family)
    manual = manual_cells(manual_pdf, family)
    sdk = sdk_cells(directory / sources['gpio_header']['file'])
    candidate = read(ROOT / sources['sdk_candidates']['file'])
    assert sdk == sdk_candidate_cells(candidate), (profile, 'SDK candidates differ')
    header = (directory / sources['cmsis_header']['file']).read_text()
    addresses = {m[1]: int(m[2], 16) for m in re.finditer(
        r'^#define\s+(GTIM[1-4]?)_BASE\s+(0x[\dA-Fa-f]+)UL', header, re.M)}
    assert addresses == ADDRESSES[family], (profile, 'CMSIS instance/base mismatch')
    with fitz.open(manual_pdf) as document:
        text = re.sub(r'\s+', '', document[BASE_PAGES[family]-1].get_text())
        for instance, address in addresses.items():
            name = 'GTIM' if family == 'L010' else instance
            assert f'{name}_BASE=0x{address:08X}' in text, (profile, instance)
        programming_page = 157 if family == 'L012' else 125
        text = re.sub(r'\s+', '', document[programming_page-1].get_text())
        assert all(token in text for token in ('数字输出', 'GPIOx_ANALOG', 'GPIOx_DIR', 'GPIOx_OPENDRAIN', 'GPIOx_AFR'))
    limits = electrical_limits(family)
    with fitz.open(pdf) as document:
        text = re.sub(r'\s+', '', document[limits['output_pdf_page']-1].get_text())
        assert all(token in text for token in ('Sourcing10mA', 'Sourcing20mA', 'Sinking10mA', 'Sinking20mA',
                                             '2.95', str(limits['voltage_at_20ma']['voh_min_v']),
                                             '0.28', '0.60', '40mA', '100mA'))
        text = re.sub(r'\s+', '', document[limits['ac_pdf_page']-1].get_text())
        assert all(token in text for token in ('30pF', '50pF', '2.7V', '2.4V', '50', '30', '20',
                                             '设计和仿真', '未实测'))
    sys.path.insert(0, str(ROOT / 'cw32-data/tools'))
    from extract_pinouts import extract_rows
    assert extract_rows(pdf, profile) == read(ROOT / sources['pinouts']['file'])['table_rows']
    print(f'PASS {profile}: own PDF/manual/SDK {len(cells)} matching cells, CMSIS bases, package grid, electrical evidence')
    return construct(profile, cells, manual, sdk, sources)


@lru_cache(maxsize=None)
def qualified_routes(profile):
    assert profile[4:] in FAMILIES
    sidecar = read(ROOT / f'cw32-data/af/{profile.lower()}-pwm.yaml')
    proof = read(ROOT / PROOF)['families'][profile]
    candidate = read(ROOT / f'cw32-data/af/{profile.lower()}.yaml')
    assert candidate['status'] == 'candidate-sdk-and-register-verified'
    cells = {(c['pin'], c['af']): c for c in proof['pdf_cells']}
    manual = {(c['pin'], c['af']): c for c in proof['manual_cells']}
    expected, expected_proof = construct(profile, cells, manual, sdk_candidate_cells(candidate), source_manifest(profile))
    assert sidecar == expected, (profile, 'curated PWM sidecar mismatch')
    assert proof == expected_proof, (profile, 'PWM evidence mismatch')
    manifest = read(ROOT / f'cw32-data/inputs/{profile.lower()}.yaml')
    assert manifest['pwm_metadata'] == f'cw32-data/af/{profile.lower()}-pwm.yaml'
    assert manifest['alias_pin_policy'] == 'common-package-intersection'
    return sidecar['routes']


def verify_generated(directory, profile):
    routes = qualified_routes(profile)
    proof = read(ROOT / PROOF)['families'][profile]
    results = []
    for part, count in proof['package_counts'].items():
        core = read(directory / 'chips' / (part+'.json'))['cores'][0]
        bonded = {p['name'] for p in core['pins']}
        actual, emitted = set(), 0
        timers = [p for p in core['peripherals'] if p['name'].startswith('GTIM')]
        assert {p['name'] for p in timers} == set(ADDRESSES[profile[4:]])
        for peripheral in timers:
            assert peripheral['registers'] == dict(kind='gtim', version=VERSIONS[profile[4:]], block='GTIM')
            emitted += len(peripheral['pins'])
            actual.update((peripheral['name'], p['pin'], p['signal'], p['af']) for p in peripheral['pins'])
            assert all(p.get('adc_mux') is None for p in peripheral['pins'])
        expected = {(r['peripheral'], r['pin'], r['signal'], r['af']) for r in routes if r['pin'] in bonded}
        assert actual == expected and len(actual) == emitted == count, (part, actual ^ expected)
        if part == profile: assert actual == set.intersection(*results)
        else: results.append(actual)
    print(f'PASS {profile}: {len(results)} exact parts and common-package alias, matching full route tuples')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--sources', type=Path)
    parser.add_argument('--write', action='store_true')
    parser.add_argument('--sidecars-only', action='store_true')
    parser.add_argument('--data-dir', type=Path, default=ROOT / 'cw32-data/data')
    args = parser.parse_args()
    assert not args.write or args.sources, '--write requires original source verification'
    proofs = {}
    for family in FAMILIES:
        profile = 'CW32'+family
        if args.sources:
            sidecar, proof = build_sources(args.sources, profile)
            if args.write:
                (ROOT / f'cw32-data/af/{profile.lower()}-pwm.yaml').write_text(yaml.safe_dump(with_source_refs(sidecar), sort_keys=False, allow_unicode=True))
            else:
                assert sidecar == read(ROOT / f'cw32-data/af/{profile.lower()}-pwm.yaml')
                assert proof == read(ROOT / PROOF)['families'][profile]
            proofs[profile] = proof
    if args.write:
        (ROOT / PROOF).write_text(json.dumps(dict(schema_version=1, date='2026-10-08',
            scope='Source-qualified buffered GTIM PWM routes; no hardware validation.',
            families=proofs), indent=2, ensure_ascii=False)+'\n')
    for family in FAMILIES:
        profile = 'CW32'+family
        qualified_routes(profile)
        if not args.sidecars_only: verify_generated(args.data_dir, profile)
    print('PASS buffered PWM route qualification: 62 retained routes, 7 oscillator exclusions; no ETR/TRGO/cascade promotion')


if __name__ == '__main__': main()
