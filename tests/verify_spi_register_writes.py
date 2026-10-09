#!/usr/bin/env python3
"""Verify SPI PAC command semantics against own-family manuals and register IR.

This is a source/PAC validator, not a HAL model or firmware test. It performs no
MMIO and does not execute firmware. Original manuals are restored separately.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import yaml

ROOT = Path(__file__).resolve().parents[1]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    evidence = json.loads((ROOT / 'docs/spi-register-write-evidence.json').read_text())
    authored = yaml.safe_load((ROOT / 'cw32-data/register-writes.yaml').read_text())['registers']
    assert len(evidence['families']) == 13
    assert len(evidence['registers']) == 5
    fields = ['FLUSH', 'RXNE', 'SSF', 'SSR', 'UD', 'OV', 'SSERR', 'MODF']
    source_root = Path(os.environ.get('CW32_SOURCES', str(ROOT.parent / 'cw32-sources')))
    for family, fact in evidence['families'].items():
        original = source_root / fact['filename']
        assert digest(original) == fact['sha256'], (family, 'manual hash')
        number = str(fact['pdf_page_1_based'])
        page = subprocess.check_output(['pdftotext', '-f', number, '-l', number,
                                        '-layout', str(original), '-'])
        assert hashlib.sha256(page).hexdigest() == fact['extract_sha256'], family
        text = page.decode()
        assert re.search(r'Reset value:\s+0x0000 00FF', text), family
        assert re.search(r'31:8\s+RFU\s+-\s+保留位，请保持默认值', text), family
        assert text.count('W1：无功能') == 8, family
        for bit, field in enumerate(fields):
            assert re.search(rf'\b{bit}\s+{field}\s+R1W0', text), (family, field)
            semantics = fact['field_semantics'][field]
            assert semantics['bit_offset'] == bit and semantics['bit_size'] == 1
        assert fact['reset_value'] == fact['write_noop'] == 255
        print(f'PASS {family}: own manual section {fact["section"]}, '
              f'printed p{fact["printed_page"]}, no-op/reserved/clear semantics')
    for version, fact in evidence['registers'].items():
        commands = [entry for entry in authored[version] if entry['register'] == 'ICR']
        assert len(commands) == 1, version
        command = commands[0]
        assert command['block'] == 'SPI' and command['fieldset'] == 'ICR'
        assert command['reset_value'] == command['write_noop'] == 255, version
        assert command['zero_to_clear_fields'] == fields, version
        generated = json.loads((ROOT / f'cw32-data/data/registers/{version}.json').read_text())
        icr = generated['fieldset/ICR']['fields']
        assert len(icr) == 8, version
        for bit, name in enumerate(fields):
            field = next(item for item in icr if item['name'] == name)
            assert field['bit_offset'] == bit and field['bit_size'] == 1, (version, name)
        pac = (ROOT / f'cw32-metapac/src/peripherals/{version}.rs').read_text()
        icr_code = pac.split('impl regs::Icr {', 1)[1]
        for method in ['write_noop', 'reset_value']:
            assert re.search(rf'pub const fn {method}\(\) -> Self\s*\{{\s*Self\((?:0xff|255)\)',
                             icr_code), (version, method)
        print(f'PASS {version}: authored commands, generated register IR and typed PAC seeds')
    print('PASS SPI source/PAC command provenance for all 13 supported families')


if __name__ == '__main__':
    main()
