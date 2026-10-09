#!/usr/bin/env python3
"""Validate official source identity and input PAC data, never execute HAL code."""
import argparse
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--sources', type=Path, default=Path('/workspace/shared/cw32-sources'))
a = p.parse_args()
evidence = json.loads((ROOT / 'docs/timer-input-source-evidence.json').read_text())
checked = set()

def mapped(value):
    path = Path(value)
    if not path.is_absolute():
        assert path.parts and path.parts[0] == "cw32-data" and ".." not in path.parts
        return ROOT / path
    try:
        return a.sources / path.relative_to(evidence['source_root'])
    except ValueError:
        return ROOT / 'cw32-data' / path.relative_to(evidence['data_root'])

def verify(node):
    if isinstance(node, dict):
        for path_key, hash_key in [('path', 'sha256'), ('dataset_path', 'dataset_sha256'), ('pinout_path', 'pinout_sha256')]:
            if path_key in node and hash_key in node:
                path = mapped(node[path_key])
                digest = hashlib.sha256(path.read_bytes()).hexdigest()
                assert digest == node[hash_key], f'changed source/data: {path}'
                checked.add(str(path))
        for value in node.values():
            verify(value)
    elif isinstance(node, list):
        for value in node:
            verify(value)
verify(evidence['families'])
fields_checked = 0
for bank in ('gtim', 'atim'):
    for version in ('cw32l010_v1', 'cw32l012_v1'):
        path = ROOT / f'cw32-data/data/registers/{bank}_{version}.json'
        data = json.loads(path.read_text())
        def field(register, name, offset, width):
            global fields_checked
            fs = {f['name']: f for f in data[f'fieldset/{register}']['fields']}
            assert fs[name]['bit_offset'] == offset and fs[name]['bit_size'] == width, (path, register, name)
            fields_checked += 1
        items = {i['name']: i for i in data[f'block/{bank.upper()}']['items']}
        for register, offset in [('CR1', 0), ('SMCR', 8), ('ISR', 16), ('CCMR1CAP', 24), ('CCMR2CAP', 28), ('CCER', 32), ('CNT', 36), ('PSC', 40), ('ARR', 44)]:
            assert items[register]['byte_offset'] == offset, (path, register)
        field('CR1', 'DIR', 4, 1)
        field('CR1', 'CKD', 8, 2)
        field('SMCR', 'SMS', 0, 3)
        field('SMCR', 'SMSH', 16, 1)
        for channel in range(1, 5):
            reg = 'CCMR1CAP' if channel <= 2 else 'CCMR2CAP'
            shift = ((channel - 1) % 2) * 8
            field(reg, f'CC{channel}S', shift, 2)
            field(reg, f'IC{channel}PSC', shift + 2, 2)
            field(reg, f'IC{channel}F', shift + 4, 4)
            for suffix, offset in [('E', 0), ('P', 1), ('NP', 3)]:
                field('CCER', f'CC{channel}{suffix}', (channel - 1) * 4 + offset, 1)
            for register in ('ISR', 'ICR'):
                field(register, f'CC{channel}IF', channel, 1)
                field(register, f'CC{channel}OF', channel + 8, 1)
            field(f'CCR{channel}', f'CCR{channel}', 0, 16)
            assert items[f'CCR{channel}']['byte_offset'] == 48 + 4 * channel
print(json.dumps({'official_source_and_data_files_verified': len(checked), 'typed_input_fields_verified': fields_checked,
                  'pac_variants': 4, 'families': list(evidence['families']), 'hal_executed': False}, indent=2))
