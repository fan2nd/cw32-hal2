#!/usr/bin/env python3
"""Independent F030 source audit; reads vendor files without redistributing them.

Usage: python3 tests/audit_f030_sources.py --sdk /path/to/sdk-v2.2
This is a source-consistency check, not execution of generated Rust or hardware.
"""
import argparse
import hashlib
import json
import re
from pathlib import Path
import xml.etree.ElementTree as ET


def audit(sdk: Path) -> dict:
    header_path = sdk / 'Libraries/inc/cw32f030.h'
    svd_path = sdk / 'IdeSupport/EWARM/arm/config/debugger/CW/CW32F030.svd'
    header = header_path.read_text()
    xml = ET.parse(svd_path).getroot()
    peripherals = xml.findall('peripherals/peripheral')
    bases = {name: int(value, 16) for name, value in re.findall(
        r'#define\s+(\w+)_BASE\s+(0x[0-9a-fA-F]+)', header)}
    for peripheral in peripherals:
        name = peripheral.findtext('name')
        assert int(peripheral.findtext('baseAddress'), 0) == bases[name], name
    header_irqs = {name: int(value) for name, value in re.findall(
        r'(\w+)_IRQn\s*=\s*(-?\d+)', header) if int(value) >= 0}
    svd_irqs = {}
    for node in xml.findall('.//interrupt'):
        name, value = node.findtext('name'), int(node.findtext('value'), 0)
        assert name not in svd_irqs or svd_irqs[name] == value
        svd_irqs[name] = value
        assert header_irqs[name] == value, name
    assert header_irqs.keys() - svd_irqs.keys() == {'FAULT'}
    assert header_irqs['FAULT'] == 31
    assert set(header_irqs.values()) == set(range(32))
    header_priority = int(re.search(r'#define\s+__NVIC_PRIO_BITS\s+(\d+)', header)[1])
    assert header_priority == 2
    assert int(xml.findtext('cpu/nvicPrioBits')) == 3
    crc = next(p for p in peripherals if p.findtext('name') == 'CRC')
    registers = {r.findtext('name'): r for r in crc.findall('registers/register')}
    expected = {
        'CR': (0, 32), 'DR8': (8, 8), 'DR16': (8, 16), 'DR32': (8, 32),
        'RESULT16': (12, 16), 'RESULT32': (12, 32),
    }
    for name, (offset, width) in expected.items():
        r = registers[name]
        assert int(r.findtext('addressOffset'), 0) == offset, name
        assert int(r.findtext('size'), 0) == width, name
    for name in ['RESULT16', 'RESULT32']:
        assert registers[name].findtext('access') == 'read-only'
    return {
        'check': 'CW32F030 SDK V2.2 source consistency',
        'peripheral_bases_matched': len(peripherals),
        'svd_irqs_matched': len(svd_irqs),
        'cmsis_irqs': len(header_irqs),
        'required_corrections': [
            'SVD nvicPrioBits=3 -> 2 (CMSIS plus manual section5.5.3 p99)',
            'Add CMSIS FAULT IRQ31 omitted from SVD',
        ],
        'crc_access_width_checks': len(expected),
        'svd_sha256': hashlib.sha256(svd_path.read_bytes()).hexdigest(),
        'header_sha256': hashlib.sha256(header_path.read_bytes()).hexdigest(),
        'hardware_tested': False,
        'rust_compiled': False,
    }

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--sdk', type=Path, required=True)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    result = json.dumps(audit(args.sdk), indent=2) + '\n'
    if args.output:
        args.output.write_text(result)
    print(result, end='')
