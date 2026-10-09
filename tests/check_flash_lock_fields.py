#!/usr/bin/env python3
"""Own-manual FLASH lock-field splits and typed PAC reserved-bit exclusions."""
import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import xml.etree.ElementTree as ET
import yaml

ROOT = Path(__file__).resolve().parents[1]

def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(',', ':')).encode()).hexdigest()


def historical_lock_ir(current, canonical):
    """Reverse only the independently reviewed Stage18 MODE enum addition."""
    current = copy.deepcopy(current)
    mode = current.pop('enum/Mode')
    assert mode['bit_size'] == 2
    expected = [('READ', 0), ('PROGRAM', 1), ('PAGE_ERASE', 2)]
    if canonical in {'flash_cw32f002_v1.yaml', 'flash_cw32f003_v1.yaml'}:
        expected.append(('CHIP_ERASE', 3))
    assert [(v['name'], v['value']) for v in mode['variants']] == expected
    field = next(f for f in current['fieldset/CR1']['fields'] if f['name'] == 'MODE')
    assert (field['bit_offset'], field['bit_size'], field.pop('enum')) == (0, 2, 'Mode')
    return current


def validate(sources):
    evidence = json.loads((ROOT / 'docs/flash-lock-access-corrections.json').read_text())
    for record in evidence['corrections']:
        family = record['family']
        manual = sources / record['manual']['file']
        assert hashlib.sha256(manual.read_bytes()).hexdigest() == record['manual']['sha256']
        peer = 'flash_cw32f003_v1.yaml' if family == 'CW32F002' else 'flash_v1.yaml'
        original = yaml.safe_load((ROOT / 'cw32-data/registers' / peer).read_text())
        original = historical_lock_ir(original, peer)
        assert digest(original) == record['original_ir_sha256']
        expected = copy.deepcopy(original)
        expected['fieldset/PAGELOCK']['fields'] = [f for f in expected['fieldset/PAGELOCK']['fields'] if f['name'] not in record['removed_fields']]
        current = yaml.safe_load((ROOT / 'cw32-data/registers' / record['canonical']).read_text())
        historical = historical_lock_ir(current, record['canonical'])
        assert historical == expected, family
        assert digest(historical) == record['corrected_ir_sha256']
        generated = yaml.safe_load((ROOT / 'cw32-data/data/registers' / record['canonical'].replace('.yaml', '.json')).read_text())
        assert generated == current
        profile = yaml.safe_load((ROOT / f'cw32-data/inputs/{family.lower()}.yaml').read_text())
        for name in record['removed_fields']:
            correction = next(x for x in profile['field_removals'] if x['fieldset'] == 'PAGELOCK' and x['field'] == name)
            assert correction['expected_bit_offset'] == int(name[4:]) and correction['expected_bit_size'] == 1
            assert record['manual']['url'] in correction['evidence'] and record['manual']['sha256'] in correction['evidence']
        svd = ET.parse(ROOT / f'sources/vendor/{family}.svd').getroot()
        peripheral = next(p for p in svd.findall('./peripherals/peripheral') if p.findtext('name') == 'FLASH')
        register = next(r for r in peripheral.findall('./registers/register') if r.findtext('name') == 'PAGELOCK')
        fields = {f.findtext('name'): f for f in register.findall('./fields/field')}
        for name in record['removed_fields']:
            assert name in fields, (family, 'vendor lineage', name)
        parts = [p for p in (ROOT / 'cw32-data/data/chips').glob(f'{family}*.json') if json.loads(p.read_text())['line'] == family]
        for part in parts:
            chip = json.loads(part.read_text())
            flash = next(p for p in chip['cores'][0]['peripherals'] if p['name'] == 'FLASH')
            assert flash['registers']['version'] == record['canonical'].removeprefix('flash_').removesuffix('.yaml')
        print(f'PASS {family}: own manual hash; exact field-only correction; vendor guard; {len(parts)} chip records')
    return evidence


def contracts(evidence):
    env = os.environ.copy()
    if (ROOT / '.cargo/bin/cargo').exists():
        env.update(CARGO_HOME=str(ROOT / '.cargo'), RUSTUP_HOME=str(ROOT / '.rustup'))
        env['PATH'] = str(ROOT / '.cargo/bin') + os.pathsep + env.get('PATH', '')
    env['CARGO_INCREMENTAL'] = '0'
    env.pop('RUSTFLAGS', None)
    targets = []
    for record in evidence['corrections']:
        family = record['family']
        targets += [(p.stem.lower(), record['removed_fields']) for p in sorted((ROOT / 'cw32-data/data/chips').glob(f'{family}*.json')) if json.loads(p.read_text())['line'] == family]
    targets += [(chip, []) for chip in ['cw32f003', 'cw32f030', 'cw32a030']]
    positives = negatives = 0
    with tempfile.TemporaryDirectory(prefix='cw32-flash-lock-contracts-') as temp:
        path = Path(temp)
        (path / 'src').mkdir()
        for chip, forbidden in targets:
            (path / 'Cargo.toml').write_text('[package]\nname="flash-lock-contracts"\nversion="0.0.0"\nedition="2024"\n[workspace]\n[dependencies]\ncw32-metapac={path=' + json.dumps(str(ROOT / 'cw32-metapac')) + ',default-features=false,features=["pac",' + json.dumps(chip) + ']}\n')
            def check(source):
                (path / 'src/lib.rs').write_text('#![no_std]\nuse cw32_metapac as pac;\n' + source)
                return subprocess.run(['cargo', 'check', '--offline', '--message-format=json', '--manifest-path', str(path / 'Cargo.toml'), '--target-dir', str(ROOT / 'target/flash-lock-contracts')], env=env, capture_output=True, text=True)
            upper = 9 if chip == 'cw32f003' else 15 if chip in ['cw32f030', 'cw32a030'] else 7
            source = f'pub fn allowed() {{ let mut r = pac::flash::regs::Pagelock::default(); r.set_lock{upper}(true); let _: bool = r.lock{upper}(); r.set_key(0x5a5a); }}'
            result = check(source)
            assert result.returncode == 0, result.stdout + result.stderr
            positives += 1
            for field in forbidden:
                for method in [field.lower(), 'set_' + field.lower()]:
                    argument = 'true' if method.startswith('set_') else ''
                    result = check(f'pub fn forbidden() {{ let mut r = pac::flash::regs::Pagelock::default(); r.{method}({argument}); }}')
                    diagnostics = [json.loads(line)['message'] for line in result.stdout.splitlines() if json.loads(line).get('reason') == 'compiler-message']
                    assert result.returncode != 0 and any(d.get('code') and d['code']['code'] == 'E0599' and method in d['message'] for d in diagnostics), result.stdout + result.stderr
                    negatives += 1
            print(f'PASS {chip}: valid LOCK{upper}/KEY control; {2 * len(forbidden)} exact E0599 reserved-field failures', flush=True)
    print(f'FLASH lock contracts passed: {positives} positives, {negatives} intended failures; no MMIO executed')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--sources', type=Path, default=Path(os.environ.get('CW32_SOURCES', '/workspace/shared/cw32-sources')))
    parser.add_argument('--source-only', action='store_true')
    args = parser.parse_args()
    evidence = validate(args.sources)
    if not args.source_only:
        contracts(evidence)
