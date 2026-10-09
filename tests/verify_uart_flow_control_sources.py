#!/usr/bin/env python3
"""Validate UART flow-control source facts and authored/package AF membership.

This is a source/data validator. It does not execute, simulate or test the HAL.
--sources rechecks the actual official manual/SDK bytes and control descriptions.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import yaml

ROOT = Path(__file__).resolve().parents[1]


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--sources', type=Path)
    args = parser.parse_args()
    evidence = json.loads((ROOT / 'docs/uart-flow-control-sources.json').read_text())
    selections = 0
    total_routes = 0
    for family, e in evidence['families'].items():
        af = ROOT / e['af']['path']
        assert sha(af) == e['af']['sha256'], af
        routes = {
            (p['peripheral'], p['pin'], p['signal'], p['af'])
            for p in yaml.safe_load(af.read_text())['routes']
            if p.get('signal') in ('RTS', 'CTS')
        }
        assert len(routes) == e['af']['qualified_flow_routes_before_package_filter']
        ir = json.loads((ROOT / f"cw32-data/data/registers/uart_{e['register_version']}.json").read_text())
        cr2 = {f['name']: f for f in ir['fieldset/CR2']['fields']}
        for name, bit in e['fields'].items():
            field = cr2[name.split('.')[1]]
            assert (field['bit_offset'], field['bit_size']) == (bit, 1)
        family_routes = 0
        for path in (ROOT / 'cw32-data/data/chips').glob(family + '*.json'):
            chip = json.loads(path.read_text())
            assert chip['line'] == family
            selections += 1
            for p in chip['cores'][0]['peripherals']:
                if not p['name'].startswith('UART'):
                    continue
                assert p['registers']['version'] == e['register_version']
                for pin in p.get('pins', []):
                    if pin['signal'] in ('CTS', 'RTS'):
                        assert (p['name'], pin['pin'], pin['signal'], pin['af']) in routes
                        family_routes += 1
        if args.sources:
            manual = e['manual']
            path = args.sources / manual['artifact']
            assert sha(path) == manual['sha256'], path
            text = subprocess.run(['pdftotext', '-layout', str(path), '-'],
                                  check=True, capture_output=True, text=True).stdout
            pages = text.split('\f')
            rts = pages[manual['rts_pdf_page'] - 1]
            cts = pages[manual['cts_pdf_page'] - 1]
            cr = pages[manual['cr2_pdf_page'] - 1]
            assert 'UARTx_CR2.RTSEN' in rts and '低电平' in rts and '高电平' in rts
            assert re.search(r'RC\s*=\s*0', rts) and re.search(r'RC\s*=\s*1', rts)
            assert 'CTSEN' in cts and 'TXE' in cts and '低电平' in cts and '高电平' in cts
            assert re.search(r'3\s+RTSEN\s+RW', cr) and re.search(r'2\s+CTSEN\s+RW', cr)
            sdk = e['sdk']
            assert sha(args.sources / sdk['archive']) == sdk['sha256']
            for member in sdk['members'].values():
                assert sha(args.sources / member['artifact']) == member['sha256']
            header = (args.sources / sdk['members']['cmsis_header']['artifact']).read_text(errors='replace')
            for field, position in [('CTSEN', 2), ('RTSEN', 3)]:
                assert re.search(r'UARTx?_CR2_' + field + r'_Pos\s+\(' + str(position) + r'UL\)', header)
        total_routes += family_routes
        print(f'PASS {family}: CR2 CTSEN/RTSEN, {len(routes)} authored routes, {family_routes} package/alias routes')
    print(f'PASS {selections} chip selections, {total_routes} metadata CTS/RTS routes; no HAL execution')


if __name__ == '__main__':
    main()
