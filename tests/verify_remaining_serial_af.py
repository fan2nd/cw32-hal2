#!/usr/bin/env python3
"""Audit L031/R031/W031/L052/L083 serial routing against each own datasheet.

Default mode checks the committed coordinate evidence, package joins, and narrow
SDK corrections. --sources additionally hashes official PDFs and SDK headers,
compares independent positioned-PDF and pdftotext extractors, and checks the
manual cells that resolve SDK disagreements. --write requires --sources and
rebuilds only the five reviewed serial sidecars and their evidence, not the PAC.
"""
import argparse
from copy import deepcopy
import hashlib
import json
from pathlib import Path
import re
import subprocess
import yaml
from route_metadata import hardware_facts, with_source_refs

ROOT = Path(__file__).resolve().parents[1]
FAMILIES = ('CW32L031', 'CW32R031', 'CW32W031', 'CW32L052', 'CW32L083')
COUNTS = dict(zip(FAMILIES, (108, 86, 90, 158, 233)))
SDK_ONLY = dict(zip(FAMILIES, (0, 21, 18, 0, 0)))
SERIAL = re.compile(r'(UART\d+|SPI\d*|I2C\d*)_([A-Z]+)')
NORMALIZE = {'TXD': 'TX', 'RXD': 'RX', 'CS': 'NSS'}
RESTRICTED = {'PA13', 'PA14', 'PF3'}
RF_PINS = {'CW32R031': {'PA0', 'PA1', 'PA2', 'PA3'},
           'CW32W031': {'PB3', 'PB4', 'PB5', 'PB6', 'PB13'}}
# Coordinate-specific decisions. No prefix-wide typo substitutions are allowed.
OVERRIDES = {
    ('CW32L031', 'PA10', 6): ('SUART3TXD', 'UART3_TXD', 'sdk-name-typo'),
    ('CW32R031', 'PA10', 6): ('SUART3TXD', 'UART3_TXD', 'sdk-name-typo'),
    ('CW32W031', 'PA10', 6): ('SUART3TXD', 'UART3_TXD', 'sdk-name-typo'),
    ('CW32R031', 'PA15', 4): ('UART2TXD', 'UART2_RXD', 'sdk-direction-correction'),
    ('CW32W031', 'PA2', 5): (None, 'UART3_RXD', 'sdk-omission'),
}
MANUALS = {
    'CW32L031': ('CW32L031_UserManual_CN_V1.6.pdf', 140),
    'CW32R031': ('CW32R031_UserManual_CN_V1.3.pdf', 142),
    'CW32W031': ('CW32W031_UserManual_CN_V1.4.pdf', 142),
}


def load(path):
    return hardware_facts(yaml.safe_load((ROOT / path).read_text()) if (ROOT / path).suffix == '.yaml' else json.loads((ROOT / path).read_text()))


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def normalized_function(function, family):
    value = function.replace('_', '').upper()
    if family in MANUALS:
        value = re.sub(r'^SPI(?=[A-Z])', 'SPI1', value)
        value = re.sub(r'^I2C(?=[A-Z])', 'I2C1', value)
    return value


def pdf_cells(path):
    """Use actual PDF column/row centers, never sequential nonblank columns."""
    import fitz
    result = {}
    with fitz.open(path) as doc:
        for page_index, page in enumerate(doc):
            words = page.get_text('words')
            headings = sorted((w[1], m[1], m[2]) for w in words
                              if (m := re.search(r'表(5-\d+).*GPIO([A-F])_AFRy', w[4])))
            for index, (top, table, port) in enumerate(headings):
                bottom = headings[index + 1][0] if index + 1 < len(headings) else 775
                region = [w for w in words if top < w[1] < bottom]
                columns = {int(m[1]): (w[0] + w[2]) / 2 for w in region
                           if (m := re.fullmatch(r'功能([1-7])', w[4]))}
                assert set(columns) == set(range(1, 8)), 'Changed AF table columns'
                pins = [(f'P{m[1]}{int(m[2])}', (w[1] + w[3]) / 2) for w in region
                        if (m := re.fullmatch(r'P([A-F])(\d+)/?', w[4]))
                        and w[2] < min(columns.values())]
                for word in region:
                    if not SERIAL.fullmatch(word[4]):
                        continue
                    x, y = (word[0] + word[2]) / 2, (word[1] + word[3]) / 2
                    af = min(columns, key=lambda value: abs(x - columns[value]))
                    pin, pin_y = min(pins, key=lambda value: abs(y - value[1]))
                    assert abs(x - columns[af]) < 1 and abs(y - pin_y) < 10, 'Ambiguous AF cell'
                    assert pin[1] == port and (pin, af) not in result
                    result[pin, af] = dict(pin=pin, af=af, function=word[4], table=table,
                                           pdf_page=page_index + 1, printed_page=page_index,
                                           bbox=[round(value, 3) for value in word[:4]])
    return result


def text_cells(text):
    """Independent Poppler character-column extraction, preserving blank cells."""
    result, active, pin, centers = {}, False, None, None
    for line in text.splitlines():
        if re.search(r'通过 GPIO[A-F]_AFRy', line):
            active, pin, centers = True, None, None
            continue
        if re.match(r'\s*6\s+地址镜像', line):
            active = False
        if not active:
            continue
        headers = list(re.finditer(r'功能\s+([1-7])', line))
        if headers:
            assert len(headers) == 7
            centers = {int(m[1]): (m.start() + m.end()) / 2 for m in headers}
            continue
        if match := re.match(r'\s*(P[A-F])(\d+)', line):
            pin = match[1] + str(int(match[2]))
        for match in SERIAL.finditer(line):
            assert centers is not None and pin is not None
            center = (match.start() + match.end()) / 2
            af = min(centers, key=lambda value: abs(center - centers[value]))
            assert abs(center - centers[af]) < 4, 'Ambiguous text AF column'
            assert (pin, af) not in result
            result[pin, af] = match.group()
    return result


def manual_cell(path, page_number, pin, af):
    import fitz
    with fitz.open(path) as doc:
        words = doc[page_number - 1].get_text('words')
        columns = {int(m[1]): (w[0] + w[2]) / 2 for w in words
                   if (m := re.fullmatch(r'AF([0-7])', w[4]))}
        assert set(columns) == set(range(8))
        source_pin = f'{pin[:2]}{int(pin[2:]):02}'
        row = [w for w in words if w[4] == source_pin]
        assert len(row) == 1
        y = (row[0][1] + row[0][3]) / 2
        functions = [w for w in words if SERIAL.fullmatch(w[4])
                     and abs((w[0] + w[2]) / 2 - columns[af]) < 1
                     and abs((w[1] + w[3]) / 2 - y) < 1]
        assert len(functions) == 1
        return dict(pin=pin, af=af, function=functions[0][4], table='9-2',
                    pdf_page=page_number, printed_page=page_number - 1,
                    bbox=[round(value, 3) for value in functions[0][:4]])


def package_audit(family, routes):
    pinout = load(f'cw32-data/pinouts/{family.lower()}.yaml')
    result = []
    for package in pinout['packages']:
        bonded = {signal for pin in package['pins'] for signal in pin['signals']
                  if re.fullmatch(r'P[A-F](?:[0-9]|1[0-5])', signal)}
        safe = set(package['gpio_pins']) - RESTRICTED
        assert not safe.intersection(package['input_only_pins'])
        assert not bonded.intersection(RF_PINS.get(family, set()))
        available = [r for r in routes if r['pin'] in bonded]
        safe_routes = [r for r in available if r['pin'] in safe]
        result.append(dict(name=package['name'], package=package['package'],
                           bonded_routes=len(available), safe_routes=len(safe_routes),
                           hal_tx_rx_sck_mosi_miso_scl_sda_routes=sum(r['signal'] not in ('CTS', 'RTS', 'NSS') for r in safe_routes),
                           excluded_debug_input_pins=sorted(bonded & RESTRICTED),
                           safe_serial_pins=sorted({r['pin'] for r in safe_routes})))
    return result


def build(family, sources):
    candidate_path = f'cw32-data/af/{family.lower()}.yaml'
    candidate = load(candidate_path)
    ds = deepcopy(load('cw32-data/parts.yaml')['sources'][family + '_datasheet'])
    pdf = sources / ds['document_filename']
    assert sha(pdf) == ds['sha256']
    cells = pdf_cells(pdf)
    text = subprocess.run(['pdftotext', '-layout', str(pdf), '-'], check=True, capture_output=True, text=True).stdout
    assert text_cells(text) == {key: cell['function'] for key, cell in cells.items()}
    assert len(cells) == COUNTS[family]
    all_sdk = {(r['pin'], r['af']): r for section in ('routes', 'unresolved') for r in candidate[section]}
    sdk_serial = {key: item for key, item in all_sdk.items()
                  if item['function'].startswith(('UART', 'SPI', 'I2C', 'SUART'))}
    headers = list((sources / family.lower()).rglob(Path(candidate['source']['header']).name))
    assert len(headers) == 1, 'SDK header path is ambiguous'
    header = headers[0]
    assert sha(header) == candidate['source']['header_sha256']
    lines = header.read_text().splitlines()
    for item in sdk_serial.values():
        line = lines[item['source_line'] - 1]
        assert item['source_macro'] + '()' in line
        assert f"CW_GPIO{item['pin'][1]}->{item['gpio_register']}_f.{item['gpio_field']}" in line
        assert re.search(r'=\s*' + str(item['af']) + r'\s*\)', line)
    routes, corrections = [], []
    for key, cell in sorted(cells.items()):
        original = all_sdk.get(key)
        expected = normalized_function(cell['function'], family)
        override = OVERRIDES.get((family, *key))
        if override:
            assert (original['function'] if original else None, cell['function']) == override[:2]
        else:
            assert original and original['function'] == expected, (family, key, original, cell)
        peripheral, signal = SERIAL.fullmatch(cell['function']).groups()
        if peripheral in ('SPI', 'I2C'):
            assert family in MANUALS
            peripheral += '1'
        route = dict(pin=key[0], af=key[1], function=expected, peripheral=peripheral,
                     signal=NORMALIZE.get(signal, signal), source_signal=signal,
                     source_macro=original['source_macro'] if original else None,
                     source_line=original['source_line'] if original else None,
                     gpio_register='AFRL' if int(key[0][2:]) < 8 else 'AFRH',
                     gpio_field=f'AFR{int(key[0][2:])}',
                     source_kind='datasheet' if not original else ('datasheet-corrected-sdk' if override else 'sdk-and-datasheet'),
                     datasheet_cell={k: v for k, v in cell.items() if k != 'bbox'})
        if override:
            filename, page_number = MANUALS[family]
            manual_path = sources / filename
            proof = manual_cell(manual_path, page_number, *key)
            assert proof['function'] == cell['function']
            correction = dict(pin=key[0], af=key[1], kind=override[2], sdk_function=override[0],
                              authoritative_function=cell['function'],
                              resolution='Official family datasheet and reference manual agree; explicit coordinate-specific correction, never a family-wide substitution.',
                              reference_manual=dict(document_filename=filename,
                                  url=f'https://www.whxy.com/uploads/files/20240920/{filename}',
                                  sha256=sha(manual_path), cell=proof))
            route['sdk_function'] = override[0]
            corrections.append(correction)
        routes.append(route)
    excluded = []
    for key, item in sorted(sdk_serial.items()):
        if key not in cells:
            excluded.append(dict(item, reason='Pin absent from this family\'s datasheet AF tables and physical package map; not inherited from the SDK or sibling family.',
                                 rf_reserved=key[0] in RF_PINS.get(family, set())))
    assert len(excluded) == SDK_ONLY[family]
    pages = sorted({cell['pdf_page'] for cell in cells.values()})
    ds['af_evidence'] = f"Tables 5-3 through {max(cell['table'] for cell in cells.values())}; {len(cells)} serial cells checked by positioned-PDF and independent pdftotext extraction; rendered PDF pages {pages} reviewed. See docs/remaining-serial-af-evidence.json."
    reviewed = dict(schema_version=1, profile=family, status='verified-sdk-and-datasheet',
                    generated_metadata_merged=False, source=candidate['source'], datasheet=ds,
                    scope='UART/SPI/I2C serial digital routes only; no unrelated SDK AF candidate promoted.',
                    normalization={'TXD': 'TX', 'RXD': 'RX', 'CS': 'NSS',
                                   'singleton_instances': {'SPI': 'SPI1', 'I2C': 'I2C1'} if family in MANUALS else {},
                                   'basis': 'Exact family datasheet functions; SDK/PAC singleton instance names retained, all family-specific differences explicit.'},
                    limitations=[
                        'Datasheet pin/AF/function cells are the hardware authority; SDK corroboration is optional and disagreements are recorded explicitly.',
                        'Exact physical package bonding filters metadata. HAL additionally withholds PA13/PA14 SWD and input-only PF3; radio-internal pads are not projected.',
                        'AF0 is GPIO. Only documented selectors 1-7 are admitted; codes 8-15 and blank cells are unsupported.',
                        'CTS/RTS/NSS remain metadata only unless a reviewed HAL API explicitly exposes those signals.',
                        'Oscillator-pad serial use requires external circuitry and RCC configuration compatible with GPIO use.',
                        'No silicon validation or additional DMA compatibility claim.'
                    ], routes=routes, unresolved=[], excluded_routes=excluded, corrections=corrections)
    if family == 'CW32W031':
        reviewed['limitations'].append('Datasheet section 4.4.4: external SPI is unavailable while the RF subsystem is in use. No simultaneous RF/SPI operation is supported; the RF driver is not implemented.')
    pinout_path = f'cw32-data/pinouts/{family.lower()}.yaml'
    audit = dict(profile=family, sources=dict(candidate=dict(path=candidate_path, sha256=sha(ROOT / candidate_path)),
                  sdk=candidate['source'], datasheet=ds, pinout=dict(path=pinout_path, sha256=sha(ROOT / pinout_path))),
                 counts=dict(datasheet_serial=len(cells), reviewed=len(routes), sdk_only_excluded=len(excluded),
                             corrected_sdk=sum(c['kind'] != 'sdk-omission' for c in corrections),
                             datasheet_only=sum(c['kind'] == 'sdk-omission' for c in corrections), unresolved=0),
                 visually_reviewed_pdf_pages=pages, cells=list(cells.values()), corrections=corrections,
                 excluded_routes=excluded, packages=package_audit(family, routes))
    return reviewed, audit


def validate(family, reviewed, audit):
    candidate = load(audit['sources']['candidate']['path'])
    assert sha(ROOT / audit['sources']['candidate']['path']) == audit['sources']['candidate']['sha256']
    assert candidate['status'] == 'candidate-sdk-and-register-verified'
    sdk = {(r['pin'], r['af']): r for section in ('routes', 'unresolved') for r in candidate[section]}
    assert reviewed['status'] == 'verified-sdk-and-datasheet' and reviewed['profile'] == family
    assert reviewed['source'] == audit['sources']['sdk'] == candidate['source']
    assert reviewed['datasheet'] == audit['sources']['datasheet']
    assert sha(ROOT / audit['sources']['pinout']['path']) == audit['sources']['pinout']['sha256']
    cells = {(c['pin'], c['af']): c for c in audit['cells']}
    routes = {(r['pin'], r['af']): r for r in reviewed['routes']}
    assert len(routes) == len(reviewed['routes']) == len(cells) == COUNTS[family]
    assert routes.keys() == cells.keys()
    assert reviewed['corrections'] == audit['corrections']
    assert reviewed['excluded_routes'] == audit['excluded_routes']
    assert len(reviewed['excluded_routes']) == SDK_ONLY[family]
    assert not reviewed['unresolved']
    identities = set()
    for key, route in routes.items():
        cell = cells[key]
        assert route['function'] == normalized_function(cell['function'], family)
        peripheral, signal = SERIAL.fullmatch(cell['function']).groups()
        if peripheral in ('SPI', 'I2C'):
            peripheral += '1'
        assert (route['peripheral'], route['signal']) == (peripheral, NORMALIZE.get(signal, signal))
        assert route['source_signal'] == signal and 1 <= route['af'] <= 7
        assert route['datasheet_cell'] == {k: v for k, v in cell.items() if k != 'bbox'}
        assert cell['pdf_page'] == cell['printed_page'] + 1
        assert re.fullmatch(r'5-[3-8]', cell['table']) and len(cell['bbox']) == 4
        assert route['gpio_register'] == ('AFRL' if int(route['pin'][2:]) < 8 else 'AFRH')
        assert route['gpio_field'] == f"AFR{int(route['pin'][2:])}"
        identity = (route['peripheral'], route['pin'], route['signal'])
        assert identity not in identities
        identities.add(identity)
        override = OVERRIDES.get((family, *key))
        source = sdk.get(key)
        assert route['source_macro'] == (source['source_macro'] if source else None)
        assert route['source_line'] == (source['source_line'] if source else None)
        if not route['source_macro']:
            assert (family, *key) == ('CW32W031', 'PA2', 5)
            assert route['source_kind'] == 'datasheet' and route['source_line'] is None
        else:
            assert route['source_line'] > 0
            assert route['source_kind'] == ('datasheet-corrected-sdk' if override else 'sdk-and-datasheet')
        if override:
            correction = next(c for c in audit['corrections'] if (c['pin'], c['af']) == key)
            assert (correction['sdk_function'], correction['authoritative_function'], correction['kind']) == override
            assert correction['reference_manual']['cell']['function'] == cell['function']
            assert re.fullmatch('[0-9a-f]{64}', correction['reference_manual']['sha256'])
    assert audit['packages'] == package_audit(family, reviewed['routes'])
    assert not {r['pin'] for r in reviewed['routes']}.intersection(RF_PINS.get(family, set()))
    return routes


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--sources', type=Path)
    parser.add_argument('--write', action='store_true')
    args = parser.parse_args()
    assert not args.write or args.sources, '--write requires the original official sources'
    evidence_path = 'docs/remaining-serial-af-evidence.json'
    audit = dict(schema_version=1, scope=list(FAMILIES), profiles=[])
    if not args.write:
        audit = load(evidence_path)
    for family in FAMILIES:
        path = f'cw32-data/af/{family.lower()}-serial.yaml'
        if args.sources:
            rebuilt, rebuilt_proof = build(family, args.sources)
        if args.write:
            (ROOT / path).write_text(yaml.safe_dump(with_source_refs(rebuilt), sort_keys=False, allow_unicode=True))
            proof = rebuilt_proof
            audit['profiles'].append(proof)
            reviewed = rebuilt
        else:
            reviewed = load(path)
            proof = next(p for p in audit['profiles'] if p['profile'] == family)
            if args.sources:
                assert rebuilt == reviewed and proof == rebuilt_proof, 'Source audit differs from committed review'
        validate(family, reviewed, proof)
        print(f"PASS {family}: {COUNTS[family]} reviewed serial routes; {SDK_ONLY[family]} SDK-only routes excluded; {len(proof['packages'])} exact package joins")
    if args.write:
        (ROOT / evidence_path).write_text(json.dumps(audit, indent=2) + '\n')


if __name__ == '__main__':
    main()
