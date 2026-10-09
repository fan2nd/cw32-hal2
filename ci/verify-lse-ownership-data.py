#!/usr/bin/env python3
"""Audit own-source LSE facts and package projections; no HAL simulation or execution."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import yaml

ROOT = Path(__file__).resolve().parents[1]
def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--sources', type=Path, default=Path(os.environ.get('CW32_SOURCES', '/workspace/shared/cw32-sources')))
    parser.add_argument('--out', type=Path)
    args = parser.parse_args()
    facts_path = ROOT / 'cw32-data/lse-ownership.yaml'
    facts = yaml.safe_load(facts_path.read_text())
    evidence_path = ROOT / facts['source_evidence']
    assert sha(evidence_path) == facts['source_evidence_sha256']
    evidence = json.loads(evidence_path.read_text())
    electrical = yaml.safe_load((ROOT / 'cw32-data/electrical.yaml').read_text())
    assert electrical['policies']['cw32-data/lse-ownership.yaml'] == sha(facts_path)
    lock = json.loads((ROOT / 'sources/evidence-sources.json').read_text())
    families = {row['family']: row for row in evidence['families']}
    assert len(families) == 13 and set(facts['families']) == set(families)
    originals = []
    archives = {}
    for name, source in evidence['sources'].items():
        path = args.sources / source['path']
        assert sha(path) == source['sha256'], name
        originals.append({'path': source['path'], 'sha256': source['sha256']})
        if 'archive_path' in source:
            archives[source['archive_path']] = source['archive_sha256']
    for name, digest in archives.items():
        assert sha(args.sources / name) == digest, name
    for family, row in families.items():
        own = facts['families'][family]
        expected = None if family in {'CW32F002', 'CW32F003'} else {
            'pin_lock': family in {'CW32L010', 'CW32L011', 'CW32L012'},
            'pin_lock_requires_enable_lock': family in {'CW32L011', 'CW32L012'},
        }
        assert own['lse'] == expected
        assert electrical['profiles'][family]['clock_limits'].get('lse') == expected
        assert row['lse_present'] == (expected is not None)
        assert sha(ROOT / row['selected_ir']['path']) == row['selected_ir']['sha256']
        assert sha(ROOT / row['pinout_metadata']['path']) == row['pinout_metadata']['sha256']
        for kind in ['manual_source', 'datasheet_source']:
            original = next(a for a in lock['artifacts'] if a['path'] == row[kind])
            assert original['sha256'] == evidence['sources'][row[kind]]['sha256']
            assert family in original['provenance']['chip_scope']
            assert original['provenance']['status'] == 'selected'
            assert own[kind] == row[kind]
        for kind in ['manual_pdf_pages_1_based', 'datasheet_pad_pdf_pages_1_based']:
            assert own[kind] == row[kind]
    projections = []
    for path in sorted((ROOT / 'cw32-data/data/chips').glob('*.json')):
        chip = json.loads(path.read_text())
        family = chip['line']
        sysctrl = next(p for p in chip['cores'][0]['peripherals'] if p['name'] == 'SYSCTRL')
        assert sysctrl['clock_limits'].get('lse') == facts['families'][family]['lse']
        pins = yaml.safe_load((ROOT / families[family]['pinout_metadata']['path']).read_text())
        exact = [p for p in pins['packages'] if p['name'] == chip['name']]
        applicable = exact or [p for p in pins['packages'] if chip['name'] in {family, p['family_part']}]
        assert applicable
        routes = {}
        for alias, signal in [('OSC32_IN', 'LSE_IN'), ('OSC32_OUT', 'LSE_OUT')]:
            each = [{s for pin in package['pins'] if alias in pin['signals'] for s in pin['signals'] if re.fullmatch(r'P[A-F][0-9]{1,2}', s)} for package in applicable]
            common = set.intersection(*each)
            assert len(common) <= 1
            expected = next(iter(common)) if facts['families'][family]['lse'] is not None and common else None
            actual = [p for p in sysctrl.get('pins', []) if p['signal'] == signal]
            assert len(actual) == bool(expected), (chip['name'], signal, actual, expected)
            if expected:
                assert actual == [{'pin': expected, 'signal': signal}]
                expected_family_pin = ('PB1' if signal == 'LSE_IN' else 'PB0') if family == 'CW32L010' else ('PC14' if signal == 'LSE_IN' else 'PC15')
                assert expected == expected_family_pin
            routes[signal] = expected
        projections.append({'chip': chip['name'], 'lse': sysctrl['clock_limits'].get('lse'), 'pins': routes})
    assert len(projections) == 54
    report = {'scope': 'Own-source identity and source/data package projection only; no HAL tests or hardware execution.', 'originals': originals, 'sdk_archives': archives, 'families': 13, 'lse_families': 11, 'generated_selections': projections}
    if args.out:
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(json.dumps(report, indent=2) + '\n')
    print(f'PASS: 13 own family sources, 11 LSE families, {len(originals)} originals, {len(archives)} SDK archives and 54 package/alias projections')

if __name__ == '__main__':
    main()
