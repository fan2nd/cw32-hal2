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

def check_current_lse(row, ir, restrictions, label):
    """Check the complete inherited-pad contract, not unrelated SYSCTRL bytes.

    The pinned source review records the original manual's field access; current
    mixed-register RO access lives in field-access.yaml, not chiptool field IR.
    Keep the review's whole-file hashes as historical identities only.
    """
    family = row['family']
    present = family not in {'CW32F002', 'CW32F003'}
    pin_lock = family in {'CW32L010', 'CW32L011', 'CW32L012'}
    required = {
        'CR1.LSEEN': (4, 1, 'RW'),
        'CR1.LSELOCK': (5, 1, 'RW'),
        'LSE.MODE': (6, 1, 'RW'),
        'LSE.STABLE': (18 if pin_lock else 15, 1, 'RO'),
    } if present else {}
    if pin_lock:
        required['LSE.PINLOCK'] = (17, 1, 'RW')
    # Exact roster: deleting a reviewed required fact must not shrink the gate.
    assert set(row['fields']) == set(required), (label, 'review field roster')
    for name, expected in required.items():
        fact = row['fields'][name]
        assert (fact['bit_offset'], fact['bit_width'], fact['documented_access']) == expected, (label, name, 'own-source fact')
        assert fact['read_side_effect'] == 'none documented', (label, name, 'read side effect')
    offsets = {'CR1': 4, **({'LSE': 36} if present else {})}
    assert row['register_offsets'] == offsets, (label, 'own-source offsets')
    items = ir['block/SYSCTRL']['items']
    assert len({item['name'].upper() for item in items}) == len(items), (label, 'duplicate register')
    registers = {item['name']: item for item in items}
    assert ('LSE' in registers) == present, (label, 'LSE register presence')
    assert ('fieldset/LSE' in ir) == present, (label, 'LSE fieldset presence')
    fields = {}
    for name, offset in offsets.items():
        reg = registers[name]
        assert (reg['byte_offset'], reg.get('bit_size', 32), reg.get('access', 'ReadWrite'), reg.get('fieldset')) == (offset, 32, 'ReadWrite', name), (label, name, 'register layout/access')
        assert reg.get('array') is None and 'block' not in reg, (label, name, 'native scalar register')
        fieldset = ir['fieldset/' + name]
        assert fieldset.get('bit_size', 32) == 32 and fieldset.get('extends') is None, (label, name, 'native fieldset')
        members = fieldset['fields']
        assert len({field['name'].upper() for field in members}) == len(members), (label, name, 'duplicate field')
        occupied = set()
        for field in members:
            start, width = field['bit_offset'], field['bit_size']
            assert type(start) is int and type(width) is int and 0 <= start < start + width <= 32, (label, name, field['name'], 'field range')
            bits = set(range(start, start + width))
            assert not bits & occupied and field.get('array') is None, (label, name, field['name'], 'overlapping/array field')
            occupied |= bits
            fields[name + '.' + field['name']] = field
    if not present:
        assert not any(name.startswith('CR1.LSE') for name in fields), (label, 'unexpected LSE control')
    assert ('LSE.PINLOCK' in fields) == pin_lock, (label, 'PINLOCK presence')
    relevant_ro = [entry for entry in restrictions if entry['register'] in {'CR1', 'LSE'} or entry['fieldset'] in {'CR1', 'LSE'}]
    expected_ro = []
    if present:
        expected_ro = [{'block': 'SYSCTRL', 'register': 'LSE', 'fieldset': 'LSE', 'field': 'STABLE', 'bit_offset': required['LSE.STABLE'][0], 'bit_size': 1}]
    assert [{k: v for k, v in entry.items() if k != 'evidence'} for entry in relevant_ro] == expected_ro, (label, 'exact read-only restrictions')
    for name, expected in required.items():
        field = fields[name]  # Required lookup, never an intersection/subset test.
        assert field.get('enum') is None and 'access' not in field, (label, name, 'native boolean/access representation')
        actual_access = 'RO' if any(entry['register'] + '.' + entry['field'] == name for entry in relevant_ro) else 'RW'
        assert (field['bit_offset'], field['bit_size'], actual_access) == expected, (label, name, 'field layout/access')
    return {'register_offsets': offsets, 'register_access': 'ReadWrite', 'register_bit_size': 32, 'fields': {name: {'bit_offset': value[0], 'bit_width': value[1], 'access': value[2]} for name, value in required.items()}}

def check_sdk_fields(row, sources):
    """Recheck reviewed positions and masks against the hashed own SDK member."""
    header = (sources / row['sdk_header_source']).read_text()
    for name, fact in row['fields'].items():
        macro = 'SYSCTRL_' + name.replace('.', '_')
        for suffix, value in [('Pos', fact['bit_offset']), ('Msk', ((1 << fact['bit_width']) - 1) << fact['bit_offset'])]:
            values = re.findall(r'^#define\s+' + re.escape(macro + '_' + suffix) + r'\s+\((0x[0-9a-fA-F]+|[0-9]+)UL\)', header, re.M)
            assert len(values) == 1 and int(values[0], 0) == value, (row['family'], macro, suffix, 'own SDK')

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
    field_access = yaml.safe_load((ROOT / 'cw32-data/field-access.yaml').read_text())
    assert field_access['schema_version'] == 1
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
    current_fields = []
    for family, row in families.items():
        own = facts['families'][family]
        expected = None if family in {'CW32F002', 'CW32F003'} else {
            'pin_lock': family in {'CW32L010', 'CW32L011', 'CW32L012'},
            'pin_lock_requires_enable_lock': family in {'CW32L011', 'CW32L012'},
        }
        assert own['lse'] == expected
        assert electrical['profiles'][family]['clock_limits'].get('lse') == expected
        assert row['lse_present'] == (expected is not None)
        # selected_ir.sha256 identifies the reviewed historical file. Current
        # LSE semantics must survive independently reviewed HSE/PLL edits.
        version = 'sysctrl_' + row['sysctrl_version']
        assert row['selected_ir']['path'] == f'cw32-data/registers/{version}.yaml'
        restrictions = field_access['registers'].get(version, [])
        for entry in restrictions:
            if entry['register'] == 'LSE':
                assert any(row['manual_source'] in citation and evidence['sources'][row['manual_source']]['sha256'] in citation for citation in entry['evidence']), (family, 'RO own-source identity')
        ir = yaml.safe_load((ROOT / row['selected_ir']['path']).read_text())
        current = check_current_lse(row, ir, restrictions, family + ' authored')
        projected_access_path = ROOT / f'cw32-data/data/field-access/{version}.json'
        if restrictions:
            projected_access = json.loads(projected_access_path.read_text())
            assert projected_access['schema_version'] == 1
            assert set(projected_access['registers']) == {version}
            projected_restrictions = projected_access['registers'][version]
        else:
            projected_restrictions = []
            assert not projected_access_path.exists(), (family, 'unexpected access projection')
        relevant = lambda entries: [entry for entry in entries if entry['register'] in {'CR1', 'LSE'} or entry['fieldset'] in {'CR1', 'LSE'}]
        assert relevant(projected_restrictions) == relevant(restrictions), (family, 'current RO source projection')
        for extension in ['json', 'yaml']:
            path = ROOT / f'cw32-data/data/registers/{version}.{extension}'
            generated = yaml.safe_load(path.read_text())
            assert check_current_lse(row, generated, projected_restrictions, family + ' generated ' + extension) == current
        check_sdk_fields(row, args.sources)
        current_fields.append({'family': family, 'ir': row['selected_ir']['path'], **current})
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
        assert sysctrl['address'] == int(families[family]['sysctrl_address'], 0), chip['name']
        assert sysctrl['registers'] == {'block': 'SYSCTRL', 'kind': 'sysctrl', 'version': families[family]['sysctrl_version']}, chip['name']
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
    report = {'scope': 'Own-source identity, current native LSE field/access semantics and source/data package projection only; no HAL tests or hardware execution. Historical whole-SYSCTRL review hashes are retained, not replayed.', 'originals': originals, 'sdk_archives': archives, 'families': 13, 'lse_families': 11, 'current_lse_fields': current_fields, 'generated_selections': projections}
    if args.out:
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(json.dumps(report, indent=2) + '\n')
    print(f'PASS: 13 own family sources, 11 current LSE field/access contracts, {len(originals)} originals, {len(archives)} SDK archives and 54 package/alias projections')

if __name__ == '__main__':
    main()
