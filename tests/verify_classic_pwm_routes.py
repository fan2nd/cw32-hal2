#!/usr/bin/env python3
"""Qualify seven classic GTIM PWM sidecars from own-family primary sources.

--sources rehashes original PDFs/SDKs, independently parses numbered PDF AF
columns, SDK register assignments, and physical package pin grids. --write
requires those original sources and writes only reviewed PWM sidecars/evidence.
--sidecars-only does not require generated metadata and never builds a PAC.
"""
import argparse
from copy import deepcopy
from functools import lru_cache
import hashlib
import json
from pathlib import Path
import re
import sys
import yaml
from route_metadata import hardware_facts, with_source_refs

ROOT = Path(__file__).resolve().parents[1]
FAMILIES = ('F002', 'F003', 'L031', 'R031', 'W031', 'L052', 'L083')
PWM = re.compile(r'(GTIM[1-4]?)_(CH[1-4])')
SDK_PWM = re.compile(r'(GTIM[1-4]?)(CH[1-4])')
COUNTS = dict(zip(FAMILIES, (15, 18, 20, 13, 16, 56, 97)))
RETAINED = dict(zip(FAMILIES, (13, 16, 20, 13, 16, 48, 89)))
SDK_ONLY = dict(zip(FAMILIES, (3, 0, 0, 7, 4, 0, 0)))
VERSIONS = dict(zip(FAMILIES, ('cw32f002_v1', 'cw32f002_v1', 'cw32l031_v1',
    'cw32l031_v1', 'cw32l031_v1', 'cw32l052_v1', 'cw32l031_v1')))
RF_PINS = {'R031': {'PA0', 'PA1', 'PA2', 'PA3'},
           'W031': {'PB3', 'PB4', 'PB5', 'PB6', 'PB13'}}
PROOF = 'docs/classic-pwm-route-evidence.json'
ROUTE_FIELDS = ('pin', 'af', 'function', 'source_macro', 'source_line',
                'gpio_register', 'gpio_field', 'peripheral', 'signal')


def read(path): return hardware_facts(yaml.safe_load(path.read_text()) if path.suffix == '.yaml' else json.loads(path.read_text()))
def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()
def digest(value):
    return hashlib.sha256(json.dumps(hardware_facts(value), sort_keys=True, separators=(',', ':'),
                                    ensure_ascii=False).encode()).hexdigest()


def pdf_cells(path):
    """Read actual row/column centers, including blank AF columns, from PDF."""
    import fitz
    result = {}
    with fitz.open(path) as document:
        for page_index, page in enumerate(document):
            lines = [(line['bbox'], ''.join(span['text'] for span in line['spans']))
                     for block in page.get_text('dict')['blocks']
                     for line in block.get('lines', [])]
            headings = sorted((box[1], match[1], match[2]) for box, text in lines
                if (match := re.search(r'表\s*(5-\d+)\s*通过\s*GPIO([A-F])_AFR[Ly]', text)))
            words = page.get_text('words')
            for index, (top, table, port) in enumerate(headings):
                bottom = headings[index + 1][0] if index + 1 < len(headings) else 780
                columns = {int(match[1]): (box[0]+box[2])/2 for box, text in lines
                    if top < box[1] < bottom and (match := re.fullmatch(r'功能\s*([1-7])', text))}
                assert set(columns) == set(range(1, 8)), (path, table, 'AF columns changed')
                region = [w for w in words if top < w[1] < bottom]
                pins = [(f'P{match[1]}{int(match[2])}', (w[1]+w[3])/2) for w in region
                    if (match := re.fullmatch(r'P([A-F])(\d+)(?:/(?:SWDIO|SWCLK)?)?', w[4]))
                    and w[2] < min(columns.values())]
                for word in region:
                    if not PWM.fullmatch(word[4]): continue
                    x, y = (word[0]+word[2])/2, (word[1]+word[3])/2
                    af = min(columns, key=lambda key: abs(x-columns[key]))
                    pin, pin_y = min(pins, key=lambda pair: abs(y-pair[1]))
                    assert abs(x-columns[af]) < 1 and abs(y-pin_y) < 10, (path, word, pin, af)
                    assert pin[1] == port and (pin, af) not in result
                    result[pin, af] = dict(pin=pin, af=af, function=word[4], table=table,
                        pdf_page=page_index+1, printed_page=page_index,
                        bbox=[round(value, 4) for value in word[:4]])
    return result


def sdk_cells(path):
    """Independent raw SDK macro/function/selector/register/field extraction."""
    result = {}
    pattern = re.compile(r'^#define\s+(P([A-F])(\d+)_AFx_((GTIM[1-4]?)(CH[1-4])))'
        r'\(\)\s+\(CW_GPIO([A-F])->(AFR[HL])_f\.(AFR(\d+))\s*=\s*(\d+)\)')
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


def reason(family, pin, row):
    if pin in RF_PINS.get(family, set()): return 'radio-reserved-pad'
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
        selected = {(r['peripheral'], r['pin'], r['signal'], r['af'])
                    for r in routes if r['pin'] in pins}
        result[package['name']] = len(selected)
        sets.append(selected)
    result[pinouts['family']] = len(set.intersection(*sets))
    return result


def construct(profile, cells, sdk, sources):
    family = profile[4:]
    pinouts = read(ROOT / sources['pinouts']['file'])
    rows = {signal: row for row in pinouts['table_rows'] for signal in row['signals']
            if re.fullmatch(r'P[A-F]\d+', signal)}
    routes, excluded_safety = [], []
    for key, cell in sorted(cells.items()):
        pin, af = key
        original = sdk[key]
        assert original['function'] == cell['function'].replace('_', ''), (profile, key)
        exclusion = reason(family, pin, rows.get(pin))
        if exclusion:
            excluded_safety.append(dict(pin=pin, af=af, function=cell['function'],
                reason=exclusion, datasheet_cell=cell, pin_signals=rows[pin]['signals']))
            continue
        row = rows[pin]
        route = dict(original)
        route.update(source_signal=original['signal'], channel=int(original['signal'][-1]),
            source_kind='sdk-and-datasheet', source_sha256=sources['gpio_header']['sha256'],
            datasheet_cell=dict({k: v for k, v in cell.items() if k != 'bbox'},
                               sha256=sources['datasheet']['sha256']),
            pin_cell=dict(pin=pin, source_name=row['source_name'], table='5-2',
                pdf_page=row['pdf_page_index']+1, printed_page=row['pdf_page_index'],
                positions=row['positions'], pin_type=row['pin_type'],
                sha256=sources['datasheet']['sha256']),
            package_pins={p['name']: row['positions'][p['table_column']] for p in pinouts['packages']},
            oscillator_aliases=[])
        assert any(value is not None for value in route['package_pins'].values())
        for package in pinouts['packages']:
            if route['package_pins'][package['name']] is not None:
                assert pin in package['gpio_pins_preserving_swd']
        routes.append(route)
    excluded_sdk = [dict(item, reason='absent-from-own-datasheet-PWM-AF-tables')
                    for key, item in sorted(sdk.items()) if key not in cells]
    assert len(cells) == COUNTS[family] and len(routes) == RETAINED[family]
    assert len(excluded_sdk) == SDK_ONLY[family]
    assert len({(r['peripheral'], r['pin'], r['signal']) for r in routes}) == len(routes)
    datasheet = dict(sources['datasheet'], af_evidence=f'Own-family numbered AF columns in Tables '
        f"{', '.join(sorted({c['table'] for c in cells.values()}))}; independently parsed original PDF and SDK. "
        f'Per-cell proof and conservative exclusions: {PROOF}.')
    sidecar = dict(schema_version=1, profile=profile, kind='pwm',
        status='verified-sdk-and-datasheet', register_version=VERSIONS[family], sources=sources,
        datasheet=datasheet, alias_pin_policy='common-package-intersection',
        selector_capability=dict(min=1, max=7, pdf_printed_page_offset=1,
            evidence='Actual own-family numbered function columns; no register-width inference.'),
        scope='GTIM CH1–4 outputs only; original general AF candidates remain unpromoted.',
        limitations=['Source-level route qualification only; no silicon or electrical validation.',
            'Exact package pin bonding is required; family aliases use the common-package intersection.',
            'Debug, reset, input-only, radio-reserved and oscillator-alias pads are excluded.',
            'No ATIM, ETR, TOG, capture, DMA or remapping authorization is implied.'],
        routes=routes, excluded_sdk_routes=excluded_sdk, excluded_safety_routes=excluded_safety)
    proof = dict(profile=profile, sources=sources, pdf_cells=list(cells.values()),
        sdk_only_excluded=excluded_sdk, safety_excluded=excluded_safety,
        counts=dict(pdf_pwm=len(cells), sdk_pwm=len(sdk), retained=len(routes),
                    sdk_only=len(excluded_sdk), safety=len(excluded_safety)),
        package_counts=package_counts(pinouts, routes), routes_sha256=digest(routes))
    return sidecar, proof


def source_manifest(profile):
    analog = read(ROOT / f'cw32-data/af/{profile.lower()}-analog.yaml')
    candidate_path = f'cw32-data/af/{profile.lower()}.yaml'
    candidate = read(ROOT / candidate_path)
    audit = read(ROOT / 'docs/adc-timer-next-batch-evidence.json')
    sources = {key: deepcopy(value) for key, value in analog['sources'].items() if key != 'adc_header'}
    sources['sdk_archive']['url'] = candidate['source']['sdk_url']
    header = next(file for file in audit['sources'] if file.startswith(profile.lower()+'/')
                  and file.endswith('/'+profile.lower()+'_gpio.h'))
    sources['gpio_header'] = dict(file=header, sha256=audit['sources'][header]['sha256'],
                                  url=candidate['source']['sdk_url'])
    sources['sdk_candidates'] = dict(file=candidate_path, sha256=sha(ROOT / candidate_path))
    assert sources['gpio_header']['sha256'] == candidate['source']['header_sha256']
    return sources


def build_sources(directory, profile):
    family = profile[4:]
    sources = source_manifest(profile)
    for key, source in sources.items():
        assert sha((ROOT if key in ('pinouts', 'sdk_candidates') else directory) / source['file']) == source['sha256'], (profile, key)
    pdf = directory / sources['datasheet']['file']
    cells = pdf_cells(pdf)
    sdk = sdk_cells(directory / sources['gpio_header']['file'])
    candidate = read(ROOT / sources['sdk_candidates']['file'])
    prior = {(r['pin'], r['af']): {k: r[k] for k in ROUTE_FIELDS}
             for section in ('routes', 'unresolved') for r in candidate[section]
             if SDK_PWM.fullmatch(r['function'])}
    assert sdk == prior, (profile, 'original SDK differs from candidate')
    audit = read(ROOT / 'docs/adc-timer-next-batch-evidence.json')['pwm_af_read_only_comparison'][family]
    assert {key: {k: c[k] for k in ('pin','af','function','table','pdf_page','printed_page')}
            for key, c in cells.items()} == {(c['pin'], c['af']): {k: c[k] for k in
            ('pin','af','function','table','pdf_page','printed_page')} for c in audit['pwm_cells']}
    assert {(r['pin'], r['af']) for r in audit['sdk_only']} == set(sdk)-set(cells)
    sys.path.insert(0, str(ROOT / 'cw32-data/tools'))
    from extract_pinouts import extract_rows
    pinouts = read(ROOT / sources['pinouts']['file'])
    assert extract_rows(pdf, profile) == pinouts['table_rows'], (profile, 'original pin grid differs')
    print(f'PASS {profile}: own PDF {len(cells)} PWM cells, SDK {len(sdk)} exact macros, original package grid')
    return construct(profile, cells, sdk, sources)


@lru_cache(maxsize=None)
def qualified_routes(profile):
    assert profile[4:] in FAMILIES
    sidecar = read(ROOT / f'cw32-data/af/{profile.lower()}-pwm.yaml')
    proof = read(ROOT / PROOF)['families'][profile]
    candidate = read(ROOT / f'cw32-data/af/{profile.lower()}.yaml')
    assert candidate['status'] == 'candidate-sdk-and-register-verified'
    sdk = {(r['pin'], r['af']): {k:r[k] for k in ROUTE_FIELDS}
           for section in ('routes', 'unresolved') for r in candidate[section]
           if SDK_PWM.fullmatch(r['function'])}
    cells = {(c['pin'], c['af']): c for c in proof['pdf_cells']}
    expected, expected_proof = construct(profile, cells, sdk, source_manifest(profile))
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
        actual = set()
        capture = set()
        emitted = 0
        for peripheral in core['peripherals']:
            if not peripheral['name'].startswith('GTIM'): continue
            assert peripheral['registers']['version'] == VERSIONS[profile[4:]]
            assert peripheral['registers']['kind'] == 'gtim' and peripheral['registers']['block'] == 'GTIM'
            for p in peripheral['pins']:
                target = capture if p['signal'].startswith('CAP') else actual
                target.add((peripheral['name'], p['pin'], p['signal'], p['af']))
                emitted += not p['signal'].startswith('CAP')
            assert all(p.get('adc_mux') is None for p in peripheral['pins'])
        expected = {(r['peripheral'], r['pin'], r['signal'], r['af']) for r in routes if r['pin'] in bonded}
        assert actual == expected and len(actual) == emitted == count, (part, actual ^ expected)
        assert capture == {(p, pin, "CAP" + signal[2:], af) for p, pin, signal, af in expected}, part
        if part == profile: assert actual == set.intersection(*results)
        else: results.append(actual)
    print(f'PASS {profile}: exact package projections and common-intersection alias ({len(results)} packages)')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--sources', type=Path)
    parser.add_argument('--write', action='store_true')
    parser.add_argument('--sidecars-only', action='store_true')
    parser.add_argument('--data-dir', type=Path, default=ROOT / 'cw32-data/data')
    args = parser.parse_args()
    if args.write: assert args.sources, '--write requires independent original-source verification'
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
            scope='Source-qualified classic GTIM PWM routes; no hardware validation.',
            families=proofs), indent=2, ensure_ascii=False)+'\n')
    for family in FAMILIES:
        profile = 'CW32'+family
        qualified_routes(profile)
        if not args.sidecars_only: verify_generated(args.data_dir, profile)
    print('PASS seven-family PWM route qualification: 215 retained routes, 14 SDK-only and 20 safety exclusions')


if __name__ == '__main__': main()
