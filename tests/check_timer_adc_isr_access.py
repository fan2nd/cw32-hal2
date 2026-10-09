#!/usr/bin/env python3
"""Verify own-manual ISR-only corrections, exact dedup and forbidden PAC writes.

Fixtures are type-checked only; no MMIO or firmware is executed. --source-only
works before regeneration. --manual-dir rechecks the pinned source PDF bytes.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import tempfile
import subprocess
import xml.etree.ElementTree as ET
import yaml
from check_gpio_isr_access import cargo_environment
from verify_timer_commands import restore_classic_gtim_modes

ROOT = Path(__file__).resolve().parents[1]
EXPECTED = {
    'CW32F002': ('GTIM', 'gtim_cw32f002_v1', ['GTIM'], 0x318),
    'CW32F003': ('GTIM', 'gtim_cw32f002_v1', ['GTIM'], 0x318),
    'CW32F020': ('GTIM', 'gtim_v1', ['GTIM1', 'GTIM2', 'GTIM3', 'GTIM4'], 0x318),
    'CW32L052': ('GTIM', 'gtim_cw32l052_v1', ['GTIM1', 'GTIM2', 'GTIM3'], 0x318),
    'CW32L083': ('GTIM', 'gtim_cw32l031_v1', ['GTIM1', 'GTIM2', 'GTIM3', 'GTIM4'], 0x318),
    'CW32L012': ('ADC', 'adc_cw32l012_v1', ['ADC1', 'ADC2'], 0x78),
}

def load(p):
    return yaml.safe_load(p.read_text()) if p.suffix == '.yaml' else json.loads(p.read_text())

def sha(p):
    return hashlib.sha256(p.read_bytes()).hexdigest()

def sources(evidence, manual_dir):
    records = {r['family']: r for r in evidence['corrections']}
    assert records.keys() == EXPECTED.keys() and len(records) == len(evidence['corrections'])
    for family, (block, canonical, instances, offset) in EXPECTED.items():
        r = records[family]
        assert (r['block'], r['canonical'], r['instances'], r['byte_offset']) == (block, canonical, instances, offset)
        assert (r['expected_access'], r['access']) == ('ReadWrite', 'Read')
        m = r['manual']
        assert m['filename'].startswith(family + '_UserManual_')
        if manual_dir:
            assert sha(manual_dir / m['filename']) == m['sha256']
            text = (manual_dir / m['filename']).with_suffix('.txt').read_text()
            page = text.split('\f')[m['pdf_page'] - 1]
            assert re.search(r'^' + re.escape(m['section']) + r'\s', page, re.M)
            assert 'RO' in page and f'{offset:02X}' in page.upper()
        profile = load(ROOT / f'cw32-data/inputs/{family.lower()}.yaml')
        assert r['source_svd'] == {k: profile['source'][k] for k in ('path', 'sha256')}
        assert f"{block.lower()}_{profile['register_versions'][block.lower()]}" == canonical
        overrides = [o for o in profile['register_overrides'] if (o['block'], o['register']) == (block, 'ISR')]
        assert len(overrides) == 1
        o = overrides[0]
        assert (o['expected_access'], o['access']) == ('ReadWrite', 'Read')
        for item in (family, m['revision'], m['section'], m['sha256'], m['url'],
                     f"printed page {m['printed_page']}/PDF page {m['pdf_page']}"):
            assert item in o['evidence'], (family, item)
        svd = ROOT / r['source_svd']['path']
        assert sha(svd) == r['source_svd']['sha256']
        root = ET.parse(svd).getroot()
        peripherals = {p.findtext('name'): p for p in root.findall('./peripherals/peripheral')}
        for instance in instances:
            p = peripherals[instance]
            base = peripherals[p.get('derivedFrom')] if p.get('derivedFrom') else p
            assert not base.get('derivedFrom')
            assert (base.findtext('headerStructName') or base.findtext('name')) == block
            reg = next(x for x in base.findall('./registers/register') if x.findtext('name') == 'ISR')
            assert int(reg.findtext('addressOffset'), 0) == offset
            access = reg.findtext('access') or base.findtext('access') or root.findtext('access')
            assert access == 'read-write', (family, instance, access)
        doc = yaml.safe_load((ROOT / f'cw32-data/registers/{canonical}.yaml').read_text())
        reg = next(r for r in doc['block/' + block]['items'] if r['name'] == 'ISR')
        assert (reg['byte_offset'], reg['access']) == (offset, 'Read')
        print(f'PASS {family}: own-manual proof, every original SVD instance, exact override')
    for c in evidence['canonical_changes']:
        path = ROOT / c['path']
        revision = c.get('subsequent_revision')
        if revision:
            assert c['path'] == 'cw32-data/registers/adc_cw32l012_v1.yaml'
            assert sha(path) == revision['current_sha256']
            assert (ROOT / revision['evidence']).is_file()
            historical = ROOT / revision['historical_snapshot']
            assert sha(historical) == c['after_sha256']
            current_ir = yaml.safe_load(path.read_text())
            previous_ir = yaml.safe_load(historical.read_text())
            for ir in (current_ir, previous_ir):
                reg = next(r for r in ir['block/ADC']['items'] if r['name'] == 'ISR')
                assert (reg['byte_offset'], reg['access']) == (c['byte_offset'], 'Read')
            assert current_ir['fieldset/ISR'] == previous_ir['fieldset/ISR']
            text = historical.read_text()
        else:
            current = load(path)
            restored = restore_classic_gtim_modes(current, path.stem)
            text = (yaml.safe_dump(restored, sort_keys=False, allow_unicode=True, width=100)
                    if restored != current else path.read_text())
            assert hashlib.sha256(text.encode()).hexdigest() == c['after_sha256']
        pattern = r'(  - name: ISR\n    description: [^\n]*\n    byte_offset: ' + str(c['byte_offset']) + r'\n)    access: Read\n'
        old, n = re.subn(pattern, r'\1', text)
        assert n == 1 and hashlib.sha256(old.encode()).hexdigest() == c['before_sha256']
    ledger = load(ROOT / 'cw32-data/register-reuse.yaml')
    # Other independently reviewed IP splits may change the global count.
    # This proof owns only the two exact GTIM merges below.
    assert ledger['canonical_template_count'] == len(ledger['groups'])
    assert sum(len(g['source_versions']) for g in ledger['groups']) == ledger['source_template_count']
    for merge in evidence['exact_merges']:
        assert not (ROOT / 'cw32-data/registers' / merge['removed']).exists()
        groups = [g for g in ledger['groups'] if merge['removed'] in g['source_versions']]
        assert len(groups) == 1 and groups[0]['canonical'] == merge['canonical']
    print('PASS five ISR-only snapshots and two exact canonical merges')

def generated():
    count = 0
    for path in (ROOT / 'cw32-data/data/chips').glob('*.json'):
        chip = load(path)
        if chip['line'] not in EXPECTED:
            continue
        block, canonical, instances, offset = EXPECTED[chip['line']]
        ps = {p['name']: p for p in chip['cores'][0]['peripherals']}
        for instance in instances:
            r = ps[instance]['registers']
            assert (r['block'], f"{r['kind']}_{r['version']}") == (block, canonical)
        doc = load(ROOT / f'cw32-data/data/registers/{canonical}.json')
        reg = next(r for r in doc['block/' + block]['items'] if r['name'] == 'ISR')
        assert (reg['byte_offset'], reg['access']) == (offset, 'Read')
        count += 1
    assert count == 24, count
    print(f'PASS all {count} generated chip records and exact canonical selection')

def pac():
    negatives = 0
    with tempfile.TemporaryDirectory(prefix='cw32-timer-adc-isr-') as temp:
        p = Path(temp); (p / 'src').mkdir()
        for family, (block, _, instances, _) in EXPECTED.items():
            (p / 'Cargo.toml').write_text('[package]\nname="cw32-timer-adc-isr-tests"\nversion="0.0.0"\nedition="2024"\n[workspace]\n[dependencies]\ncw32-metapac={path=' + json.dumps(str(ROOT / 'cw32-metapac')) + ',default-features=false,features=["pac","' + family.lower() + '"]}\n')
            def check(code):
                (p / 'src/main.rs').write_text('use cw32_metapac as pac;\nfn main(){\n' + code + '\n}\n')
                return subprocess.run(['cargo','check','--offline','--message-format=json','--manifest-path',str(p/'Cargo.toml'),'--target-dir',str(ROOT/'target/timer-adc-isr-access-tests')],env=cargo_environment(),capture_output=True,text=True)
            control = 'cr0' if block == 'GTIM' else 'cr'
            good = check('\n'.join(f'let _ = pac::{i}.isr().read();' for i in instances) + f'\npac::{instances[0]}.{control}().write(|_| {{}});\npac::{instances[0]}.{control}().modify(|_| {{}});')
            assert good.returncode == 0, good.stdout + good.stderr
            for instance in instances:
                for method in ['write','modify']:
                    bad = check(f'pac::{instance}.isr().{method}(|_| {{}});')
                    errors = [j['message'] for line in bad.stdout.splitlines() if (j := json.loads(line)).get('reason')=='compiler-message' and j['message']['level']=='error']
                    assert bad.returncode != 0 and len(errors)==1 and errors[0].get('code',{}).get('code')=='E0599', bad.stdout+bad.stderr
                    assert f'`{method}`' in errors[0]['message']
                    assert any(f'pac::{instance}.isr().{method}' in line['text'] for span in errors[0]['spans'] for line in span['text'])
                    negatives += 1
            print(f'PASS {family}: ISR reads, control writes and {len(instances)*2} precise compile-negative cases')
    assert negatives == 30
    print('PASS six positive controls and 30 forbidden ISR accesses; no hardware executed')

if __name__ == '__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--source-only',action='store_true');parser.add_argument('--manual-dir',type=Path);args=parser.parse_args()
    sources(load(ROOT/'docs/timer-adc-isr-access-corrections.json'),args.manual_dir)
    if not args.source_only:
        generated();pac()
