#!/usr/bin/env python3
"""Validate timer field/command metadata against pinned own-family PDF pages.

This validates source and generated PAC contracts, not the HAL or hardware.
Optional --compile runs cargo check only: no MMIO, executable or firmware runs.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import yaml

ROOT = Path(__file__).resolve().parents[1]


def load(path):
    return yaml.safe_load(path.read_text()) if path.suffix == '.yaml' else json.loads(path.read_text())


def digest(data):
    return hashlib.sha256(data).hexdigest()


def normalized(data):
    return digest(json.dumps(data, sort_keys=True, separators=(',', ':')).encode())


def restore_classic_gtim_modes(ir, variant):
    """Remove only the exact, separately own-manual-qualified additive enums."""
    evidence = load(ROOT / 'docs/classic-gtim-modes-evidence.json')
    additions = evidence['register_additions'].get(variant)
    if additions is None:
        return ir
    assert set(additions) == {'enum/CcMode', 'enum/Mode'}
    restored = json.loads(json.dumps(ir))
    for key, expected in additions.items():
        assert restored.pop(key) == expected, (variant, key)
    fields = restored['fieldset/CMMR']['fields']
    assert [f['name'] for f in fields] == ['CC1M', 'CC2M', 'CC3M', 'CC4M']
    for field in fields:
        assert field.pop('enum') == 'CcMode'
    mode = next(f for f in restored['fieldset/CR0']['fields'] if f['name'] == 'MODE')
    assert mode.pop('enum') == 'Mode'
    return restored



def validate(manual_dir, source_only):
    evidence = load(ROOT / 'docs/timer-command-evidence.json')
    lock = load(ROOT / 'sources/evidence-sources.json')
    writes = load(ROOT / 'cw32-data/register-writes.yaml')
    accesses = load(ROOT / 'cw32-data/field-access.yaml')
    assert len(evidence['families']) == 13 and len(evidence['registers']) == 12
    covered = {key: set() for key in evidence['registers']}
    field_covered = {}
    for family, fact in evidence['families'].items():
        artifact = next(a for a in lock['artifacts'] if a['id'] == fact['source_ref'])
        assert artifact['url'] == fact['url'] and artifact['sha256'] == fact['sha256']
        assert 'CN V' + artifact['provenance']['printed_revision'] == fact['revision']
        pdf = manual_dir / fact['filename']
        assert digest(pdf.read_bytes()) == fact['sha256'], (family, 'PDF pin')
        for record in fact['sections']:
            page = str(record['pdf_page_1_based'])
            raw = subprocess.check_output(['pdftotext', '-f', page, '-l', page,
                                           '-layout', str(pdf), '-'])
            assert digest(raw) == record['extract_sha256'], (family, page)
            text = raw.decode()
            heading = re.search(r'^' + re.escape(record['section']) + r'\s', text, re.M)
            assert heading, (family, record['section'])
            text = text[heading.start():]
            end = re.search(r'\n\d+\.\d+\.\d+\s', text[1:])
            text = text[:end.start() + 1] if end else text
            variant = record['variant']
            profile = load(ROOT / f'cw32-data/inputs/{family.lower()}.yaml')
            kind = record['block'].lower()
            assert kind + '_' + profile['register_versions'][kind] == variant
            if record['register'] == 'CNT':
                assert re.search(r'31\s+UIFCPY\s+RO\b', text)
                assert re.search(r'15:0\s+CNT\s+RW\b', text)
                field_covered.setdefault(variant, set()).add(family)
                continue
            covered[variant].add(family)
            reset = re.search(r'Reset value:\s*0x([0-9A-Fa-f]{4})\s+([0-9A-Fa-f]{4})', text)
            assert int(''.join(reset.groups()), 16) == record['reset_value']
            fields = {name: int(bit) for bit, name in
                      re.findall(r'^\s*(\d+)\s+(\w+)\s+R1W0\b', text, re.M)}
            assert fields == {f['name']: f['bit_offset'] for f in record['fields']}
            assert '保留位，请保持默认值' in text and re.search(r'W?1：无功能', text)
            mask = sum(1 << bit for bit in fields.values())
            buffered = '_cw32l010_' in variant or '_cw32l012_' in variant
            expected = ((0x41, 0x41) if buffered else (7, 7)) if kind == 'btim' else (
                (0x00f01e5f, 0x00f01e5f) if buffered else (0x3ff, 0x27f))
            assert (record['write_noop'], mask) == expected
            assert record['reset_value'] == record['write_noop']
            assert record['defined_clear_mask'] == mask
            assert record['reserved_preserve_ones'] == record['write_noop'] & ~mask
        print(f'PASS {family}: pinned own manual, ICR W0C/reset/reserved policy and CNT field access')
    assert len(field_covered) == 4
    ledger = load(ROOT / 'cw32-data/register-reuse.yaml')
    for variant, fact in evidence['registers'].items():
        assert covered[variant] == set(fact['families'])
        command, = writes['registers'][variant]
        assert {k: v for k, v in command.items() if k != 'evidence'} == fact['icr']
        if 'read_only_fields' in fact:
            restriction, = accesses['registers'][variant]
            assert {k: v for k, v in restriction.items() if k != 'evidence'} == fact['read_only_fields'][0]
            assert field_covered[variant] == covered[variant]
            assert (restriction['field'], restriction['bit_offset'], restriction['bit_size']) == ('UIFCPY', 31, 1)
        current = load(ROOT / f'cw32-data/registers/{variant}.yaml')
        group = next(g for g in ledger['groups'] if g['canonical'] == variant + '.yaml')
        assert group['canonical_ir_sha256'] == normalized(current)
        if source_only:
            continue
        ir = load(ROOT / f'cw32-data/data/registers/{variant}.json')
        assert normalized(ir) == group['canonical_ir_sha256']
        fields = ir['fieldset/ICR']['fields']
        assert [f['name'] for f in fields] == command['zero_to_clear_fields']
        assert all(f['bit_size'] == 1 for f in fields)
        assert sum(1 << f['bit_offset'] for f in fields) == fact['defined_clear_mask']
        own_fields = next(record['fields'] for family in fact['families']
                          for record in evidence['families'][family]['sections']
                          if record['variant'] == variant and record['register'] == 'ICR')
        assert {f['name']: f['bit_offset'] for f in fields} == {
            f['name']: f['bit_offset'] for f in own_fields}
        projection = load(ROOT / f'cw32-data/data/register-writes/{variant}.json')
        assert projection == {'schema_version': 1, 'registers': {variant: [command]}}
        pac = (ROOT / f'cw32-metapac/src/peripherals/{variant}.rs').read_text()
        seed = pac.split('impl regs::Icr {', 1)[1]
        for method in ['reset_value', 'write_noop']:
            match = re.search(rf'pub const fn {method}\(\) -> Self\s*\{{\s*Self\((0x[\da-f]+|\d+)\)', seed)
            assert match and int(match.group(1), 0) == command[method]
        if 'read_only_fields' in fact:
            assert 'fn set_uifcpy(' not in pac
            assert pac.count('fn uifcpy(') == 1 and pac.count('fn set_cnt(') == 1
            projection = load(ROOT / f'cw32-data/data/field-access/{variant}.json')
            assert projection == {'schema_version': 1, 'registers': {variant: accesses['registers'][variant]}}
    # The new AWT source-semantic split carries the unchanged command protocol.
    split_command, = writes['registers']['awt_cw32f030_v1']
    old_command, = writes['registers']['awt_v1']
    assert {k: v for k, v in split_command.items() if k != 'evidence'} == {k: v for k, v in old_command.items() if k != 'evidence'}
    assert split_command['evidence'][:-1] == old_command['evidence']
    assert split_command['evidence'][-1].startswith('cw32-data/hse-qualified.yaml: own F020/x030 AWT semantic split')
    for variant in ('sysctrl_v1', 'sysctrl_cw32f020_v1'):
        restriction, = accesses['registers'][variant]
        assert {k: v for k, v in restriction.items() if k != 'evidence'} == {
            'block': 'SYSCTRL', 'register': 'HSE', 'fieldset': 'HSE', 'field': 'STABLE',
            'bit_offset': 19, 'bit_size': 1}
        assert len(restriction['evidence']) == 1 and 'cw32-data/hse-qualified.yaml' in restriction['evidence'][0]
    print('PASS 12 current command variants, four RO fields, source-qualified overlays and current reuse identities')
    return evidence


def compile_pac(evidence):
    # Actual generated register modules, no HAL or mocked implementation.
    with tempfile.TemporaryDirectory(prefix='cw32-timer-pac-') as directory:
        root = Path(directory)
        (root / 'src').mkdir()
        (root / 'Cargo.toml').write_text('[package]\nname="timer-pac-contract"\nversion="0.0.0"\nedition="2024"\n[workspace]\n[features]\ndefmt=[]\n')
        (root / 'Cargo.lock').write_text('version = 4\n[[package]]\nname = "timer-pac-contract"\nversion = "0.0.0"\n')
        preamble = '#![no_std]\n#![allow(dead_code, non_snake_case, non_camel_case_types, non_upper_case_globals)]\n'
        (root / 'src/common.rs').write_bytes((ROOT / 'cw32-metapac/src/common.rs').read_bytes())
        preamble += 'mod common;\n'
        positive = []
        for variant, fact in evidence['registers'].items():
            path = ROOT / f'cw32-metapac/src/peripherals/{variant}.rs'
            (root / 'src' / (variant + '.rs')).write_bytes(path.read_bytes())
            preamble += f'mod {variant};\n'
            positive.append(f'const _: () = assert!({variant}::regs::Icr::write_noop().0 == {fact["icr"]["write_noop"]});')
            positive.append(f'const _: () = assert!({variant}::regs::Icr::reset_value().0 == {fact["icr"]["reset_value"]});')
            body = f'let mut command = {variant}::regs::Icr::write_noop();'
            fields = load(ROOT / f'cw32-data/data/registers/{variant}.json')['fieldset/ICR']['fields']
            for name in fact['icr']['zero_to_clear_fields']:
                body += f'command.set_{name.lower()}(false);'
                bit = next(f['bit_offset'] for f in fields if f['name'] == name)
                word = fact['icr']['write_noop'] & ~(1 << bit)
                positive.append(f'const _: () = {{ let mut value = {variant}::regs::Icr::write_noop(); '
                                f'value.set_{name.lower()}(false); assert!(value.0 == {word}); }};')
            body += f'let mut count = {variant}::regs::Cnt::default(); count.set_cnt(65535); let _: u16 = count.cnt();'
            if 'read_only_fields' in fact:
                body += 'let _: bool = count.uifcpy();'
            positive.append(f'fn check_{variant}() {{ {body} }}')
        def check(source):
            (root / 'src/lib.rs').write_text(preamble + source)
            return subprocess.run(['cargo', 'check', '--offline', '--locked', '--message-format=json', '--manifest-path', str(root / 'Cargo.toml')], capture_output=True, text=True)
        result = check('\n'.join(positive))
        assert result.returncode == 0, result.stdout + result.stderr
        for variant, fact in evidence['registers'].items():
            if 'read_only_fields' not in fact:
                continue
            result = check(f'fn forbidden() {{ {variant}::regs::Cnt::default().set_uifcpy(false); }}')
            errors = [entry['message'] for line in result.stdout.splitlines()
                      if (entry := json.loads(line)).get('reason') == 'compiler-message'
                      and entry['message']['level'] == 'error']
            assert result.returncode != 0 and len(errors) == 1, result.stdout + result.stderr
            assert errors[0]['code']['code'] == 'E0599' and 'set_uifcpy' in errors[0]['message']
        print('PASS compile-only actual PAC modules: all 12 seeds, exact isolated clear words/reserved preservation, CNT writes and four precise forbidden UIFCPY setters')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--manual-dir', type=Path, default=Path(os.environ.get('CW32_SOURCES', ROOT.parent / 'cw32-sources')))
    parser.add_argument('--source-only', action='store_true')
    parser.add_argument('--compile', action='store_true')
    args = parser.parse_args()
    assert not (args.source_only and args.compile)
    evidence = validate(args.manual_dir, args.source_only)
    if args.compile:
        compile_pac(evidence)
