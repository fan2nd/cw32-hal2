#!/usr/bin/env python3
"""Verify bounded own-manual RTC field/access corrections and typed PAC contracts.

Source-only checks need no regeneration. PAC checks only compile MMIO expressions;
width-mask host tests exercise register values in RAM, never any device/address.
The optional baseline proof checks non-RTC IR at this mutation batch boundary.
"""
import argparse
import copy
import hashlib
import json
import os
import re
from pathlib import Path
import subprocess
import tempfile
import sys
import xml.etree.ElementTree as ET
import yaml
from check_gpio_isr_access import cargo_environment
from audit_generated_parity import field_span

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'cw32-data/tools'))
from source_provenance import enrich_source_refs
FREQ_FAMILIES = {'CW32F020', 'CW32L031', 'CW32L052', 'CW32L083', 'CW32R031', 'CW32W031'}
WIDTH_FAMILIES = {'CW32L010', 'CW32L011'}

def load(p):
    return yaml.safe_load(p.read_text()) if p.suffix == '.yaml' else json.loads(p.read_text())

def digest(v):
    return hashlib.sha256(json.dumps(v, sort_keys=True, separators=(',', ':')).encode()).hexdigest()

def sha(p):
    return hashlib.sha256(p.read_bytes()).hexdigest()

def verify_sources(e, source_root):
    assert {r['family'] for r in e['corrections']} == FREQ_FAMILIES | WIDTH_FAMILIES
    ledger = load(ROOT / 'cw32-data/register-reuse.yaml')
    for c in e['canonical_changes']:
        p = ROOT / 'cw32-data/registers' / c['canonical']
        current = yaml.safe_load(p.read_text())
        # Current hardware contract; dated before/after identities remain in docs.
        if c['removed_field']:
            assert not any(f['name'] == c['removed_field']['name']
                           for f in current['fieldset/COMPEN']['fields'])
        for reg in c['access_changes']:
            item = next(r for r in current['block/RTC']['items'] if r['name'] == reg)
            assert item['access'] == 'Read'
        for width in c.get('width_changes', []):
            field = next(f for f in current['fieldset/DATE']['fields'] if f['name'] == width['field'])
            assert (field['bit_offset'], field['bit_size']) == (width['expected_bit_offset'], width['bit_size'])
        group = next(g for g in ledger['groups'] if g['canonical'] == c['canonical'])
        assert group['canonical_ir_sha256'] == digest(current)
    assert ledger['canonical_template_count'] == len(ledger['groups'])
    assert ledger['source_template_count'] == sum(len(g['source_versions']) for g in ledger['groups'])
    for r in e['corrections']:
        family = r['family']; m = r['manual']
        assert sha(source_root / m['file']) == m['sha256']
        page = (source_root / m['file']).with_suffix('.txt').read_text().split('\f')[m['pdf_page'] - 1]
        assert m['section'] in page
        if family in FREQ_FAMILIES:
            assert '31:16' in page and 'RFU' in page
        else:
            assert '12:8' in page and '5:0' in page
        profile = load(ROOT / f'cw32-data/inputs/{family.lower()}.yaml')
        expected_source = enrich_source_refs(copy.deepcopy(r['source_svd']),
                                            load(ROOT / 'sources/evidence-sources.json'))
        assert profile['source'] == expected_source
        assert 'rtc_' + profile['register_versions']['rtc'] + '.yaml' == r['canonical']
        rules = []
        if family in FREQ_FAMILIES:
            hits = [x for x in profile['field_removals'] if (x['fieldset'], x['field']) == ('COMPEN', 'FREQ')]
            assert len(hits) == 1
            rule = hits[0]; assert (rule['expected_bit_offset'], rule['expected_bit_size']) == (16, 4)
            rules.append(rule)
        for width in r.get('width_changes', []):
            hits = [x for x in profile['field_width_overrides'] if (x['block'], x['fieldset'], x['field']) == ('RTC', 'DATE', width['field'])]
            assert len(hits) == 1
            rule = hits[0]
            assert all(rule[k] == width[k] for k in width)
            rules.append(rule)
        for a in r['access_changes']:
            hits = [x for x in profile['register_overrides'] if (x['block'], x['register']) == ('RTC', a['register'])]
            assert len(hits) == 1
            rule = hits[0]; assert (rule['expected_access'], rule['access']) == ('ReadWrite', 'Read')
            rules.append(rule)
            apage = (source_root / m['file']).with_suffix('.txt').read_text().split('\f')[a['pdf_page'] - 1]
            assert a['section'] in apage and 'RTC_' + a['register'] in apage and 'RO' in apage
        for rule in rules:
            assert all(x in rule['evidence'] for x in (family, m['url'], m['sha256']))
        svd_path = ROOT / profile['source']['path']; assert sha(svd_path) == profile['source']['sha256']
        xml = ET.parse(svd_path).getroot()
        rtc = next(p for p in xml.findall('./peripherals/peripheral') if p.findtext('name') == 'RTC')
        registers = {r.findtext('name'): r for r in rtc.findall('./registers/register')}
        if family in FREQ_FAMILIES:
            freq = next(f for f in registers['COMPEN'].findall('./fields/field') if f.findtext('name') == 'FREQ')
            assert field_span(freq) == (16, 4)
        for a in r['access_changes']:
            reg = registers[a['register']]
            assert int(reg.findtext('addressOffset'), 0) == a['byte_offset']
            assert (reg.findtext('access') or rtc.findtext('access') or xml.findtext('access')) == 'read-write'
        for w in r.get('width_changes', []):
            f = next(f for f in registers['DATE'].findall('./fields/field') if f.findtext('name') == w['field'])
            assert field_span(f) == (w['expected_bit_offset'], w['expected_bit_size'])
        print(f'PASS {family}: own manual hash/page; source SVD guard; exact scoped input correction')
    print('PASS current RTC correction fields, access rules and canonical reuse identities')

def verify_manual_maps(source_root):
    audit = load(ROOT / 'docs/rtc-next-batch-audit.json')
    seen = set(); registers = fields = 0
    for family in audit['families']:
        if not family['rtc_present'] or family['manual_source'] in seen: continue
        seen.add(family['manual_source'])
        source = audit['sources'][family['manual_source']]
        assert sha(source_root / source['artifact']) == source['sha256']
        pages = family['manual_pdf_pages']
        end = pages['ICR'] if family['controller_group'] == 'classic' else pages['compensation_register']
        text = '\f'.join((source_root / source['artifact']).with_suffix('.txt').read_text().split('\f')[pages['register_list']-1:end])
        ir = yaml.safe_load((ROOT / f"cw32-data/registers/rtc_{family['canonical_version']}.yaml").read_text())
        headings = list(re.finditer(r'^\s*(?:10|12|13)\.5\.\d+\s+(RTC_\w+)\s', text, re.M))
        assert len(headings) == len(ir['block/RTC']['items'])
        for i, heading in enumerate(headings):
            name = heading[1][4:]
            part = text[heading.end():headings[i+1].start() if i+1 < len(headings) else len(text)]
            offset = int(re.search(r'Address offset:\s*(0x[0-9A-Fa-f]+)', part)[1], 16)
            item = next(x for x in ir['block/RTC']['items'] if x['name'] == name)
            assert item['byte_offset'] == offset
            expected = {}
            for m in re.finditer(r'^\s*(\d+(?::\d+)?)\s+(\w+)\s+(RO|RW|WO|R1W0)\s', part, re.M):
                span = list(map(int, m[1].split(':'))); field = m[2]
                if family['family'] == 'CW32F020' and name in ['ALARMA', 'ALARMB']:
                    field = {'WEEKMASK':'WEEK','HOUREN':'HOURMASK','MINUTEEN':'MINUTEMASK','SECONDEN':'SECONDMASK'}.get(field, field)
                expected[field] = (span[-1], span[0]-span[-1]+1)
            actual = {x['name']:(x['bit_offset'], x['bit_size']) for x in ir['fieldset/'+name]['fields']}
            assert expected == actual, (family['family'], name)
            registers += 1; fields += len(expected)
    assert (len(seen), registers, fields) == (10, 159, 667)
    print('PASS all 10 RTC manuals: 159 table offsets and 667 named field spans')

def verify_generated(e):
    count = 0
    records = {r['family']: r for r in e['corrections']}
    for p in (ROOT / 'cw32-data/data/chips').glob('*.json'):
        chip = load(p)
        if chip['line'] not in records: continue
        r = records[chip['line']]
        rtc = next(x for x in chip['cores'][0]['peripherals'] if x['name'] == 'RTC')
        assert 'rtc_' + rtc['registers']['version'] + '.yaml' == r['canonical']
        current = yaml.safe_load((ROOT / 'cw32-data/registers' / r['canonical']).read_text())
        assert current == load(ROOT / 'cw32-data/data/registers' / r['canonical'].replace('.yaml', '.json'))
        count += 1
    print(f'PASS all {count} affected generic/exact generated chip profiles')
    return count

def compare_baseline(e):
    for key, directory, pattern, parser in [('non_rtc_baseline', 'cw32-data/registers', '*.yaml', lambda p: yaml.safe_load(p.read_text())), ('generated_non_rtc_baseline', 'cw32-data/data/registers', '*.json', load)]:
        actual = {p.name: digest(parser(p)) for p in sorted((ROOT / directory).glob(pattern)) if not p.name.startswith('rtc_')}
        assert actual == e[key], (key, [n for n in actual if actual[n] != e[key].get(n)])
        print(f'PASS {len(actual)} {key} exact normalized IR hashes unchanged')

def pac_contracts():
    env = cargo_environment(); env['CARGO_INCREMENTAL'] = '0'; env.pop('RUSTFLAGS', None)
    positive = negative = masks = 0
    with tempfile.TemporaryDirectory(prefix='cw32-rtc-contracts-') as temp:
        p = Path(temp); (p / 'src').mkdir()
        for family in sorted(FREQ_FAMILIES | WIDTH_FAMILIES | {'CW32F030', 'CW32A030'}):
            (p / 'Cargo.toml').write_text('[package]\nname="cw32-rtc-contracts"\nversion="0.0.0"\nedition="2024"\n[workspace]\n[dependencies]\ncw32-metapac={path=' + json.dumps(str(ROOT / 'cw32-metapac')) + ',default-features=false,features=["pac","' + family.lower() + '"]}\n')
            def run(code, command='check'):
                (p / 'src/lib.rs').write_text('use cw32_metapac as pac;\n' + code)
                return subprocess.run(['cargo',command,'--offline','--message-format=json','--manifest-path',str(p/'Cargo.toml'),'--target-dir',str(ROOT/'target/rtc-pac-contracts')], env=env, capture_output=True, text=True)
            comp = 'compcfr1' if family in WIDTH_FAMILIES else 'compen'
            good = run(f'pub fn control() {{ let _ = pac::RTC.tampdate().read(); let _ = pac::RTC.tamptime().read(); pac::RTC.date().write(|w| {{w.set_day(1); w.set_month(1);}}); pac::RTC.{comp}().modify(|w| {{w.set_comp(1); w.set_en(true);}}); }}')
            assert good.returncode == 0, good.stdout + good.stderr
            positive += 1
            bad_cases = []
            if family not in WIDTH_FAMILIES:
                bad_cases += [('freq', 'pub fn bad() { let _ = pac::RTC.compen().read().freq(); }'), ('set_freq', 'pub fn bad() { pac::RTC.compen().modify(|w| w.set_freq(1)); }')]
            if family == 'CW32F020':
                bad_cases += [(method, f'pub fn bad() {{ pac::RTC.{register}().{method}(|_| {{}}); }}') for register in ['tampdate','tamptime'] for method in ['write','modify']]
            for method, code in bad_cases:
                result = run(code)
                errors = [j['message'] for line in result.stdout.splitlines() if (j := json.loads(line)).get('reason') == 'compiler-message' and j['message']['level'] == 'error']
                assert result.returncode != 0 and len(errors) == 1 and errors[0].get('code', {}).get('code') == 'E0599' and f'`{method}`' in errors[0]['message'], result.stdout + result.stderr
                negative += 1
            if family in WIDTH_FAMILIES:
                result = run('''#[test] fn pure_date_masks_preserve_reserved_bits() {
                    let reserved = 0x0000_e0c0;
                    let mut r = pac::rtc::regs::Date(reserved);
                    r.set_day(0xff); r.set_month(0xff);
                    assert_eq!(r.day(), 0x3f); assert_eq!(r.month(), 0x1f);
                    assert_eq!(r.0, reserved | 0x0000_1f3f);
                    r.set_day(0); r.set_month(0); assert_eq!(r.0, reserved);
                }''', 'test')
                assert result.returncode == 0, result.stdout + result.stderr
                masks += 1
            print(f'PASS {family}: allowed PAC control; {len(bad_cases)} exact compile-negative cases; no MMIO executed', flush=True)
    assert (positive, negative, masks) == (10, 20, 2)
    print('PASS 10 positive builds; 20 precise E0599 failures; 2 pure host mask tests')

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--sources', type=Path, default=Path(os.environ.get('CW32_SOURCES', '/workspace/shared/cw32-sources')))
    parser.add_argument('--source-only', action='store_true')
    parser.add_argument('--compare-baseline', action='store_true')
    args = parser.parse_args(); evidence = load(ROOT / 'docs/rtc-pac-corrections.json')
    verify_sources(evidence, args.sources)
    verify_manual_maps(args.sources)
    if not args.source_only: verify_generated(evidence); pac_contracts()
    if args.compare_baseline: compare_baseline(evidence)
