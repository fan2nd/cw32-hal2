#!/usr/bin/env python3
"""Verify the own-source PLL facts and authored projection; no HAL execution."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess

import yaml

ROOT = Path(__file__).resolve().parents[1]


def load(path):
    return yaml.safe_load(path.read_text())


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--sources', type=Path, required=True)
    args = parser.parse_args()
    policy = load(ROOT / 'cw32-data/pll-qualified.yaml')
    electrical = load(ROOT / 'cw32-data/electrical.yaml')
    receipt = load(ROOT / policy['source_evidence'])
    lock = {a['id']: a for a in load(ROOT / 'sources/evidence-sources.json')['artifacts']}
    assert sha(ROOT / policy['source_evidence']) == policy['source_evidence_sha256']
    assert sha(ROOT / 'cw32-data/pll-qualified.yaml') == electrical['policies']['cw32-data/pll-qualified.yaml']
    assert set(policy['families']) == {'CW32L083', 'CW32F020', 'CW32F030', 'CW32A030'}
    assert {name for name, p in electrical['profiles'].items() if p['clock_limits'].get('pll')} == set(policy['families'])
    for source in receipt['source_identities']:
        original = lock[source['id']]
        assert sha(args.sources / source['path']) == source['sha256'] == original['sha256']
        assert source['url'] == original['url'] and source['path'] == original['path']
        assert source['status'] == original['provenance']['status'] == 'selected'
        assert source['printed_revision'] == original['provenance']['printed_revision']
        assert source['chip_scope'] == original['provenance']['chip_scope']
    access = load(ROOT / 'cw32-data/field-access.yaml')['registers']
    groups = load(ROOT / 'cw32-data/register-reuse.yaml')['groups']
    for family, variant, page_number, pair_count in [
        ('CW32F020', 'sysctrl_cw32f020_v1', 75, 9),
        ('CW32F030', 'sysctrl_v1', 77, 12),
        ('CW32A030', 'sysctrl_v1', 77, 12),
    ]:
        facts = receipt['profiles'][family]
        own = policy['families'][family]
        projection = electrical['profiles'][family]
        pll = own['pll']
        assert projection['clock_limits']['pll'] == pll
        assert projection['pll_sources'] == own['sources']
        assert [s['source_ref'] for s in own['sources']] == [facts['manual_source'], facts['datasheet_source']]
        for source in own['sources']:
            assert source['sha256'] == lock[source['source_ref']]['sha256']
            assert family in lock[source['source_ref']]['provenance']['chip_scope']
            assert source['pdf_pages_1_based'] == [p + 1 for p in source['printed_pages']]
        assert pll['input_range_hz'] == [4000000, 24000000]
        assert pll['output_range_hz'] == facts['conservative_qualified_output_hz']
        assert own['datasheet_output_range_hz'] == facts['pll_datasheet_output_hz']
        assert pll['supply_mv'] == facts['supply_mv'] == [1650, 5500]
        assert pll['temperature_c'] == facts['temperature_c_conservative'] == [-40, 105]
        assert pll['multiplier_range'] == [2, 12]
        assert pll['reserved_debug_default'] == 5 and pll['startup_encoding'] == 7
        assert pll['startup_cycles'] == 16384 and pll['hsi_supported']
        assert pll['cycle_to_cycle_jitter_ps'] == 300
        expected_pairs = [{k: p['literal_multiplier'] if k == 'multiplier' else p[k]
                           for k in ('hsi_divisor', 'multiplier', 'nominal_hz', 'minimum_hz', 'maximum_hz',
                                     'input_bin_encoding', 'output_bin_encoding')}
                          for p in facts['hsi_full_envelope_pairs']]
        assert own['qualified_hsi_pairs'] == expected_pairs and len(expected_pairs) == pair_count
        manual = args.sources / lock[facts['manual_source']]['path']
        page = subprocess.check_output(['pdftotext', '-f', str(page_number), '-l', str(page_number),
                                        '-layout', str(manual), '-'], text=True)
        for pattern in [r'4\.7\.8', r'Address offset:\s*0x28', r'Reset value:\s*0x0005\s+3483',
                        r'19:16\s+RFU\s+RW\s+调试控制位，请保持默认值', r'15\s+STABLE\s+RO',
                        r'0x02\s*~\s*0x0C', r'111：16384', r'11：HSI 时钟 \(HSIOSC 分频后的时钟']:
            assert re.search(pattern, page), (family, pattern)
        assert receipt['common_pll_facts']['pll_register']['reset'] == '0x00053483'
        ir = load(ROOT / f'cw32-data/registers/{variant}.yaml')
        fields = {f['name']: f for f in ir['fieldset/PLL']['fields']}
        for field, offset, width, enum in [('SOURCE', 0, 2, 'PllSource'), ('FREQIN', 2, 2, 'PllInputRange'),
                                          ('MUL', 4, 5, 'PllMul'), ('FREQOUT', 9, 3, 'PllOutputRange'),
                                          ('WAITCYCLE', 12, 3, 'PllWait'), ('RESERVED_DEBUG', 16, 4, 'PllDebug')]:
            assert (fields[field]['bit_offset'], fields[field]['bit_size'], fields[field]['enum']) == (offset, width, enum)
        assert [v['value'] for v in ir['enum/PllMul']['variants']] == list(range(2, 13))
        assert [v['value'] for v in ir['enum/PllSource']['variants']] == [0, 1, 3]
        assert [(v['name'], v['value']) for v in ir['enum/PllDebug']['variants']] == [('Default', 5)]
        stable, = (f for f in access[variant] if f['register'] == 'PLL' and f['field'] == 'STABLE')
        assert (stable['bit_offset'], stable['bit_size']) == (15, 1)
        assert any(facts['manual_source'] in e for e in stable['evidence'])
        group, = (g for g in groups if g['canonical'] == variant + '.yaml')
        digest = hashlib.sha256(json.dumps(ir, sort_keys=True, separators=(',', ':')).encode()).hexdigest()
        assert digest == group['canonical_ir_sha256'] == group['canonical_ir_history'][-1]['after_sha256']
        assert group['canonical_ir_history'][-1]['evidence'] == policy['source_evidence']
    print('Verified F020/F030/A030 own originals, PLL facts, typed fields, reset authority, RO access and reuse ledger; no hardware execution.')


if __name__ == '__main__':
    main()
