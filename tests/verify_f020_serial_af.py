#!/usr/bin/env python3
"""Verify only the reviewed F020 UART/SPI/I2C AF routes, without generating files.

The checked-in audit is an independent transcription of the rendered datasheet.
Pass --sources DIR to also hash and parse the original official PDF and compare
SDK macros. No peripheral backend or silicon compatibility is inferred here.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import yaml
from route_metadata import hardware_facts

ROOT = Path(__file__).resolve().parents[1]
SERIAL = ('UART', 'SPI', 'I2C')
NORMALIZE = {'TXD': 'TX', 'RXD': 'RX', 'CS': 'NSS'}


def load(path):
    return hardware_facts(yaml.safe_load((ROOT / path).read_text()) if (ROOT / path).suffix == '.yaml' else json.loads((ROOT / path).read_text()))


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def table(text):
    """Read numbered cells from pdftotext -layout, preserving blank columns."""
    result = {}
    pin = centers = None
    active = False
    for line in text.splitlines():
        if re.search(r'通过 GPIO[ABCF]_AFRy', line):
            active, pin, centers = True, None, None
            continue
        if not active:
            continue
        if re.match(r'\s*6\s+地址镜像', line):
            break
        match = re.match(r'\s*(P[ABCF])(\d+)', line)
        if match:
            pin = match[1] + str(int(match[2]))
        functions = list(re.finditer(r'[A-Z][A-Za-z0-9]*_[A-Za-z0-9_]+', line))
        if not functions or pin is None:
            continue
        if centers is None:
            assert len(functions) == 7, 'Unexpected table layout; manual review required'
            centers = [(f.start() + f.end()) / 2 for f in functions]
        for function in functions:
            center = (function.start() + function.end()) / 2
            column = min(range(7), key=lambda i: abs(center - centers[i]))
            assert abs(center - centers[column]) < 5, 'Ambiguous table cell'
            key = (pin, column + 1)
            assert key not in result, 'Duplicate table coordinate'
            result[key] = function.group().replace('_', '').upper()
    assert len(result) == 265, 'Datasheet table changed; manual review required'
    return {key: value for key, value in result.items() if value.startswith(SERIAL)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--sources', type=Path)
    args = parser.parse_args()
    candidate_path = 'cw32-data/af/cw32f020.yaml'
    candidate = load(candidate_path)
    evidence = load('docs/f020-serial-af-evidence.json')
    verified = load('cw32-data/af/cw32f020-serial.yaml')
    assert sha(ROOT / candidate_path) == evidence['sources']['candidate']['sha256']
    assert verified['profile'] == evidence['profile'] == 'CW32F020'
    assert verified['status'] == 'verified-sdk-and-datasheet'
    assert candidate['status'] == 'candidate-sdk-and-register-verified', 'Do not implicitly promote unrelated routes'
    assert verified['datasheet']['sha256'] == evidence['sources']['datasheet']['sha256']
    assert verified['datasheet']['af_evidence']
    assert verified['source'] == candidate['source']
    assert verified['source']['header_sha256'] == evidence['sources']['sdk']['header_sha256']
    routes = candidate['routes']
    expected = {(r['pin'], r['af']): r for r in evidence['routes']}
    sdk = {(r['pin'], r['af']): r for r in routes if r.get('peripheral', '').startswith(SERIAL)}
    checked = {(r['pin'], r['af']): r for r in verified['routes']}
    assert len(evidence['routes']) == len(expected) == len(sdk) == len(checked) == len(verified['routes']) == 116
    assert expected.keys() == sdk.keys() == checked.keys()
    assert evidence['counts'] == {'matched': 116, 'conflicts': 0, 'datasheet_only': 0, 'sdk_only': 0, 'existing_hal_signal': 82}
    assert not evidence['conflicts']
    for key, audit in expected.items():
        source, item = sdk[key], checked[key]
        assert audit['status'] == 'sdk-datasheet-match'
        assert audit['function'] == source['function'] == item['function']
        assert audit['peripheral'] == source['peripheral'] == item['peripheral']
        assert audit['signal'] == source['signal'] == item['source_signal']
        assert item['signal'] == NORMALIZE.get(source['signal'], source['signal'])
        assert audit['sdk_macro'] == source['source_macro'] == item['source_macro']
        assert audit['sdk_header_line'] == source['source_line'] == item['source_line']
        assert audit['existing_hal_signal'] == (source['signal'] not in ['CTS', 'RTS', 'CS'])
        n = int(item['pin'][2:])
        assert item['gpio_register'] == source['gpio_register'] == ('AFRL' if n < 8 else 'AFRH')
        assert item['gpio_field'] == source['gpio_field'] == f'AFR{n}'
        assert 1 <= item['af'] <= 7
        assert audit['datasheet_table'] == {'A': '5-3', 'B': '5-4', 'C': '5-5', 'F': '5-6'}[item['pin'][1]]
        assert audit['datasheet_printed_page'] == (26 if item['pin'][1] == 'A' else 27)
        assert audit['datasheet_pdf_page'] == audit['datasheet_printed_page'] + 1
    if args.sources:
        pdf = args.sources / evidence['sources']['datasheet']['file']
        assert sha(pdf) == verified['datasheet']['sha256']
        text = subprocess.run(['pdftotext', '-layout', str(pdf), '-'], check=True, capture_output=True, text=True).stdout
        cells = table(text)
        assert len(cells) == 116
        assert cells == {key: item['function'] for key, item in expected.items()}
        header = args.sources / 'cw32f020' / verified['source']['header']
        assert sha(header) == verified['source']['header_sha256']
        lines = header.read_text().splitlines()
        for item in checked.values():
            line = lines[item['source_line'] - 1]
            assert item['source_macro'] + '()' in line
            assert f"CW_GPIO{item['pin'][1]}->{item['gpio_register']}_f.{item['gpio_field']}" in line
            assert re.search(r'=\s*' + str(item['af']) + r'\s*\)', line)
        print('PASS official PDF hash/layout cells and SDK header hash/macros')
    print('PASS F020: 116 exact SDK/datasheet matches; 82 current-HAL signal routes before package filtering; no conflicts')


if __name__ == '__main__':
    main()
