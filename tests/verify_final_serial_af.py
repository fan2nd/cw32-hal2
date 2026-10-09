#!/usr/bin/env python3
"""Verify F002/F003/L010/L011/L012 serial AFs against their own official sources.

Default: deterministic review/normalization/package contracts from committed proof.
--sources DIR: re-read exact official PDFs and SDK macros and compare every cell.
--write requires --sources and rebuilds only the five sidecars and proof document.
It never promotes other AF candidates, changes a driver, or regenerates the PAC.
"""
import argparse
from copy import deepcopy
import hashlib
import json
from pathlib import Path
import re
import yaml
from route_metadata import hardware_facts, with_source_refs

ROOT = Path(__file__).resolve().parents[1]
FAMILIES = ('CW32F002', 'CW32F003', 'CW32L010', 'CW32L011', 'CW32L012')
COUNTS = dict(zip(FAMILIES, (42, 52, 43, 70, 133)))
MAX_AF = dict(zip(FAMILIES, (7, 7, 7, 7, 9)))
PAGE_OFFSET = dict(zip(FAMILIES, (1, 1, 1, 3, 3)))
DEBUG = dict(zip(FAMILIES, ({'PA2', 'PA5'}, {'PA2', 'PA5'}, {'PA7', 'PA8'},
                          {'PA13', 'PA14'}, {'PA13', 'PA14'})))
RESET = dict(zip(FAMILIES, ({'PC5'}, {'PC5'}, {'PB7'}, set(), set())))
SDK_ONLY_F002_PINS = {'PA3', 'PB7', 'PC3', 'PC4'}
SERIAL = re.compile(r'(UART\d+|SPI\d*|I2C\d*)_([A-Z]+)')
NORMALIZE = {'TXD': 'TX', 'RXD': 'RX', 'CS': 'NSS'}
# Datasheet instance -> SDK instance -> exact PAC instance. No global prefix fix.
INSTANCE_NAMES = {
    'CW32F002': {'SPI': ('SPI', 'SPI'), 'I2C': ('I2C', 'I2C')},
    'CW32F003': {'SPI': ('SPI', 'SPI'), 'I2C': ('I2C', 'I2C')},
    'CW32L010': {'SPI': ('SPI1', 'SPI'), 'I2C': ('I2C1', 'I2C1')},
    'CW32L011': {'SPI': ('SPI1', 'SPI'), 'I2C': ('I2C', 'I2C')},
    'CW32L012': {},
}
EVIDENCE_PATH = 'docs/final-serial-af-evidence.json'


def load(path):
    return hardware_facts(yaml.safe_load((ROOT / path).read_text()) if (ROOT / path).suffix == '.yaml' else json.loads((ROOT / path).read_text()))


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def names(family, function):
    match = SERIAL.fullmatch(function)
    assert match, function
    peripheral, signal = match.groups()
    sdk, pac = INSTANCE_NAMES[family].get(peripheral, (peripheral, peripheral))
    # F002/F003 use CS; the other three SDKs use NCS. The datasheets use CS.
    sdk_signal = 'NCS' if signal == 'CS' and family not in ('CW32F002', 'CW32F003') else signal
    return sdk + sdk_signal, pac, NORMALIZE.get(signal, signal)


def pdf_cells(path, family):
    """Positioned text extraction, retaining actual blank column coordinates."""
    import fitz
    result = {}
    with fitz.open(path) as doc:
        for page_index, page in enumerate(doc):
            lines = [(line['bbox'], ''.join(span['text'] for span in line['spans']))
                     for block in page.get_text('dict')['blocks']
                     for line in block.get('lines', [])]
            headings = sorted((box[1], m[1], m[2]) for box, text in lines
                              if (m := re.search(r'表\s*(5-\d+)\s*通过\s*GPIO([A-F])_AFR[Ly]', text)))
            words = page.get_text('words')
            for index, (top, table, port) in enumerate(headings):
                bottom = headings[index + 1][0] if index + 1 < len(headings) else 780
                region = [w for w in words if top < w[1] < bottom]
                columns = {int(m[1]): (box[0] + box[2]) / 2 for box, text in lines
                           if top < box[1] < bottom
                           and (m := re.fullmatch(r'功能\s*([1-9])', text))}
                assert set(columns) == set(range(1, MAX_AF[family] + 1)), 'Changed AF columns'
                pins = [(f'P{m[1]}{int(m[2])}', (w[1] + w[3]) / 2) for w in region
                        if (m := re.fullmatch(r'P([A-F])(\d+)(?:/(?:SWDIO|SWCLK)?)?', w[4]))
                        and w[2] < min(columns.values())]
                for word in region:
                    if not SERIAL.fullmatch(word[4]):
                        continue
                    x, y = (word[0] + word[2]) / 2, (word[1] + word[3]) / 2
                    af = min(columns, key=lambda k: abs(x - columns[k]))
                    pin, pin_y = min(pins, key=lambda item: abs(y - item[1]))
                    assert abs(x - columns[af]) < 1 and abs(y - pin_y) < 10, (family, word, pin, af)
                    assert pin[1] == port and (pin, af) not in result
                    result[pin, af] = dict(pin=pin, af=af, function=word[4], table=table,
                                          pdf_page=page_index + 1,
                                          printed_page=page_index + 1 - PAGE_OFFSET[family],
                                          bbox=[round(v, 4) for v in word[:4]])
    return result


def package_audit(family, routes):
    pinout = load(f'cw32-data/pinouts/{family.lower()}.yaml')
    result = []
    for package in pinout['packages']:
        bonded = {s for p in package['pins'] for s in p['signals']
                  if re.fullmatch(r'P[A-F](?:[0-9]|1[0-5])', s)}
        safe = set(package['gpio_pins']) - DEBUG[family] - RESET[family]
        assert not safe.intersection(package['input_only_pins'])
        available = [r for r in routes if r['pin'] in bonded]
        safe_routes = [r for r in available if r['pin'] in safe]
        result.append(dict(name=package['name'], package=package['package'],
                           bonded_routes=len(available), safe_routes=len(safe_routes),
                           hal_tx_rx_sck_mosi_miso_scl_sda_routes=sum(r['signal'] not in ('CTS', 'RTS', 'NSS') for r in safe_routes),
                           excluded_debug_reset_pins=sorted(bonded & (DEBUG[family] | RESET[family])),
                           safe_serial_pins=sorted({r['pin'] for r in safe_routes})))
    return result


def review(family, candidate, ds, cells, excluded_confirmation):
    sdk = {(r['pin'], r['af']): r for key in ('routes', 'unresolved') for r in candidate[key]}
    sdk_serial = {key: r for key, r in sdk.items() if r['function'].startswith(('UART', 'SPI', 'I2C'))}
    routes = []
    for key, cell in sorted(cells.items()):
        original = sdk[key]
        expected, peripheral, signal = names(family, cell['function'])
        assert original['function'] == expected, (family, key, original, cell)
        route = deepcopy(original)
        route.update(peripheral=peripheral, signal=signal,
                     source_signal=SERIAL.fullmatch(cell['function'])[2],
                     source_kind='sdk-and-datasheet',
                     datasheet_cell={k: v for k, v in cell.items() if k != 'bbox'})
        routes.append(route)
    excluded = [dict(r, reason='Absent from own-family datasheet AF tables and physical packages; own current reference-manual Table 8-2 also omits this pad.')
                for key, r in sorted(sdk_serial.items()) if key not in cells]
    assert len(excluded) == (10 if family == 'CW32F002' else 0)
    assert not excluded or {r['pin'] for r in excluded} == SDK_ONLY_F002_PINS
    ds = deepcopy(ds)
    pages = sorted({c['pdf_page'] for c in cells.values()})
    ds['af_evidence'] = f"Own-family Tables {', '.join(sorted({c['table'] for c in cells.values()}))}; {len(cells)} serial cells checked by positioned-PDF extraction against original SDK macros and prior rendered audit. PDF pages {pages}. Per-cell proof: {EVIDENCE_PATH}."
    selector = dict(min=1, max=MAX_AF[family], pdf_printed_page_offset=PAGE_OFFSET[family],
                    evidence='Numbered function columns in the cited own-family datasheet AF tables; register width alone never authorizes selectors.')
    reviewed = dict(schema_version=1, profile=family, status='verified-sdk-and-datasheet',
                    generated_metadata_merged=True, source=candidate['source'], datasheet=ds,
                    scope='UART/SPI/I2C serial routes only; all unrelated AF candidates remain unpromoted.',
                    selector_capability=selector,
                    normalization=dict(signals=NORMALIZE, instances={k: dict(sdk=v[0], pac=v[1]) for k, v in INSTANCE_NAMES[family].items()},
                                       basis='Exact family datasheet and PAC names; original SDK function, macro and line retained.'),
                    limitations=[
                        'Physical package bonding filters generated metadata, including family-alias common-package intersections.',
                        'HAL additionally withholds debug/reset-restricted pads; no constructor remaps debug, reset or oscillator functions.',
                        'CTS/RTS/NSS remain metadata unless explicitly exposed by a separately reviewed driver.',
                        'No silicon, electrical-performance, DMA or backend-compatibility validation is implied.'
                    ], routes=routes, unresolved=[], excluded_routes=excluded, corrections=[])
    proof = dict(profile=family, sources=dict(
                     candidate=dict(path=f'cw32-data/af/{family.lower()}.yaml', sha256=sha(ROOT / f'cw32-data/af/{family.lower()}.yaml')),
                     sdk=candidate['source'], datasheet=ds,
                     pinout=dict(path=f'cw32-data/pinouts/{family.lower()}.yaml', sha256=sha(ROOT / f'cw32-data/pinouts/{family.lower()}.yaml'))),
                 counts=dict(reviewed=len(routes), sdk_serial=len(sdk_serial), sdk_only_excluded=len(excluded), datasheet_only=0, corrected_sdk=0),
                 selector_capability=selector, cells=list(cells.values()), excluded_confirmation=excluded_confirmation,
                 packages=package_audit(family, routes))
    return reviewed, proof


def build(family, sources):
    candidate = load(f'cw32-data/af/{family.lower()}.yaml')
    ds = load('cw32-data/parts.yaml')['sources'][family + '_datasheet']
    pdf = sources / ds['document_filename']
    assert sha(pdf) == ds['sha256']
    cells = pdf_cells(pdf, family)
    assert len(cells) == COUNTS[family]
    headers = list((sources / family.lower()).rglob(Path(candidate['source']['header']).name))
    assert len(headers) == 1
    assert sha(headers[0]) == candidate['source']['header_sha256']
    lines = headers[0].read_text().splitlines()
    for section in ('routes', 'unresolved'):
        for route in candidate[section]:
            if not route['function'].startswith(('UART', 'SPI', 'I2C')):
                continue
            line = lines[route['source_line'] - 1]
            assert route['source_macro'] + '()' in line
            assert f"CW_GPIO{route['pin'][1]}->{route['gpio_register']}_f.{route['gpio_field']}" in line
            assert re.search(r'=\s*' + str(route['af']) + r'\s*\)', line)
    confirmation = None
    if family == 'CW32F002':
        import fitz
        path = sources / 'CW32F002_UserManual_CN_V1.4.pdf'
        assert sha(path) == 'e453b3f82d9d180a86cd5f7cb18d858d7f228afc8101d87c700ca9d5c02d5add'
        with fitz.open(path) as doc:
            text = doc[104].get_text()
        assert '表8-2' in text
        pads = {f'P{m[1]}{int(m[2])}' for m in re.finditer(r'\bP([ABC])(\d\d)\b', text)}
        assert not pads & SDK_ONLY_F002_PINS
        assert pads == {pin for pin, af in cells}
        confirmation = dict(document_filename=path.name, sha256=sha(path),
                            url='https://www.whxy.com/uploads/files/20240920/' + path.name,
                            table='8-2', pdf_page=105, printed_page=104,
                            documented_pads=sorted(pads), omitted_sdk_pads=sorted(SDK_ONLY_F002_PINS))
    if family.startswith('CW32L'):
        prior = next(x for x in load('docs/read-only-low-power-serial-af-audit.json')['families'] if x['family'] == family)
        assert prior['datasheet_sha256'] == ds['sha256']
        assert prior['sdk_header_sha256'] == candidate['source']['header_sha256']
        assert prior['candidate_sha256'] == sha(ROOT / prior['candidate_path'])
        assert prior['pinout_sha256'] == sha(ROOT / prior['pinout_path'])
        prior_cells = {(c['pin'], c['af']): c for c in prior['cells']}
        assert prior_cells.keys() == cells.keys()
        for key, cell in cells.items():
            p = prior_cells[key]
            assert all(cell[k] == p[k] for k in ('pin', 'af', 'table', 'pdf_page', 'printed_page'))
            assert cell['function'] == p['datasheet_function']
            assert cell['bbox'] == p['function_bbox']
    return review(family, candidate, ds, cells, confirmation)


def validate(family, reviewed, proof):
    assert proof['profile'] == family
    sources = proof['sources']
    for key in ('candidate', 'pinout'):
        assert sha(ROOT / sources[key]['path']) == sources[key]['sha256']
    cells = {(c['pin'], c['af']): c for c in proof['cells']}
    assert len(cells) == len(proof['cells']) == COUNTS[family]
    for key, cell in cells.items():
        assert 1 <= cell['af'] <= MAX_AF[family]
        assert cell['pdf_page'] == cell['printed_page'] + PAGE_OFFSET[family]
        assert re.fullmatch(r'5-[3-6]', cell['table']) and len(cell['bbox']) == 4
    expected, expected_proof = review(family, load(sources['candidate']['path']),
                                      load('cw32-data/parts.yaml')['sources'][family + '_datasheet'],
                                      cells, proof['excluded_confirmation'])
    assert reviewed == expected, 'Reviewed profile differs from exact source/normalization/package proof'
    assert proof == expected_proof, 'Committed evidence changed without source re-review'
    assert load(f'cw32-data/inputs/{family.lower()}.yaml')['af_metadata'] == f'cw32-data/af/{family.lower()}-serial.yaml'
    assert not reviewed['unresolved']
    assert not {r['pin'] for r in reviewed['routes']} & (SDK_ONLY_F002_PINS if family == 'CW32F002' else set())
    return {(r['pin'], r['af']): r for r in reviewed['routes']}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--sources', type=Path)
    parser.add_argument('--write', action='store_true')
    args = parser.parse_args()
    assert not args.write or args.sources, '--write requires official sources'
    audit = dict(schema_version=1, scope=list(FAMILIES), profiles=[]) if args.write else load(EVIDENCE_PATH)
    for family in FAMILIES:
        path = f'cw32-data/af/{family.lower()}-serial.yaml'
        if args.sources:
            rebuilt, rebuilt_proof = build(family, args.sources)
        if args.write:
            (ROOT / path).write_text(yaml.safe_dump(with_source_refs(rebuilt), sort_keys=False, allow_unicode=True))
            reviewed, proof = rebuilt, rebuilt_proof
            audit['profiles'].append(proof)
        else:
            reviewed = load(path)
            proof = next(p for p in audit['profiles'] if p['profile'] == family)
            if args.sources:
                assert (reviewed, proof) == (rebuilt, rebuilt_proof), 'Source parity failure'
        validate(family, reviewed, proof)
        print(f"PASS {family}: {len(reviewed['routes'])} source-verified serial routes; selectors 1..{MAX_AF[family]}; {len(proof['packages'])} exact package joins")
    if args.write:
        (ROOT / EVIDENCE_PATH).write_text(json.dumps(audit, indent=2) + '\n')


if __name__ == '__main__':
    main()
