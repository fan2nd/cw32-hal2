#!/usr/bin/env python3
"""Source/data audit for the bounded complementary PWM projection, not a HAL test."""
import argparse, hashlib, json, re, sys, zipfile
from pathlib import Path
ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tests"))
import verify_atim_pwm_routes as common
PATTERN = re.compile(r"ATIM_(CH[1-4]N|BK)")

def sdk_cells(path):
    regex = re.compile(r'^#define\s+(P([A-F])(\d+)_AFx_(ATIM(CH[1-4]N|BK(?:IN)?)))\(\)\s+\(CW_GPIO([A-F])->(AFR[HL])_f\.((?:AFR|PIN)(\d+))\s*=\s*(\d+)\)')
    out = {}
    for number, line in enumerate(path.read_text().splitlines(), 1):
        match = regex.match(line)
        if not match: continue
        macro, port, n, function, signal, bank, reg, field, bit, af = match.groups()
        assert port == bank and int(n) == int(bit)
        assert reg == ('AFRL' if int(n) < 8 else 'AFRH')
        pin, af = f'P{port}{int(n)}', int(af)
        assert (pin, af) not in out
        out[pin, af] = dict(pin=pin, af=af, function=function, source_macro=macro,
            source_line=number, gpio_register=reg, gpio_field=field, peripheral='ATIM', signal='BK' if signal == 'BKIN' else signal)
    return out

def audit_semantics(sources):
    import fitz
    proof = common.read(ROOT / 'docs/atim-complementary-evidence.json')
    for family, facts in proof['families'].items():
        source = facts['sources']['reference_manual']
        assert common.sha(sources / source['file']) == source['sha256']
        with fitz.open(sources / source['file']) as pdf:
            clean = lambda text: re.sub(r'\s+', '', text)
            behavior = clean(''.join(pdf[p].get_text() for p in facts['behavior_pdf_page_indices']))
            for anchor in ('6对互补PWM信号', 'DTG[7:5]=0xx', 'DTG[7:5]=10x',
                'DTG[7:5]=110', 'DTG[7:5]=111', 'MOE位异步清零', 'BIF和B2IF也不能被清零'):
                assert anchor in behavior, (family, anchor)
            page = facts['register_sections']['CR1']['pdf_page_index']
            assert '00：fDTS=fPCLK/1' in clean(pdf[page].get_text())
            page = facts['register_sections']['BDTR']['pdf_page_index']
            bdtr = clean(''.join(pdf[p].get_text() for p in range(page, page+4)))
            for anchor in ('LOCK', '复位后只能对LOCK位执行一次写操作', 'MOE', 'AOE', 'BKP', 'BKE'):
                assert anchor in bdtr, (family, anchor)
            page = facts['register_sections']['ICR']['pdf_page_index']
            icr = clean(''.join(pdf[p].get_text() for p in range(page, page+2)))
            assert 'BIF' in icr and 'R1W0' in icr
        for source in facts['sdk_register_sources']:
            assert common.sha(sources / source['file']) == source['sha256']
            with zipfile.ZipFile(sources / source['archive']) as archive:
                assert hashlib.sha256(archive.read(source['archive_member'])).hexdigest() == source['sha256']
        print(f"PASS {family}: own dead-time/clock/break/lock/W0C source anchors and SDK archive members")
    for path in sorted((ROOT / 'cw32-data/data/chips').glob('*.json')):
        chip = common.read(path)
        line = chip['line']
        if line not in proof['families']: continue
        timer = next(p for p in chip['cores'][0]['peripherals'] if p['name'] == 'ATIM')
        sidecar = common.read(ROOT / f'cw32-data/af/{line.lower()}-atim-complementary.yaml')
        assert timer['atim_complementary'] == sidecar['capability']
        bonded = {p['name'] for p in chip['cores'][0]['pins']}
        expected = {(r['pin'],r['signal'],r['af']) for r in sidecar['routes'] if r['pin'] in bonded}
        actual = {(r['pin'],r['signal'],r['af']) for r in timer['pins'] if r['signal'] in ('CH1N','CH2N','CH3N','CH4N','BK')}
        assert expected == actual, path.name
    print('PASS generated exact-package and family-alias complementary projections')

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--sources', type=Path, required=True)
    parser.add_argument('--write', action='store_true')
    args = parser.parse_args()
    proof = dict(schema_version=1, families={})
    for family in ('L010', 'L011', 'L012'):
        profile = 'CW32' + family
        sources = common.sources(family)
        for key, value in sources.items():
            root = ROOT if key in ('pinouts', 'sdk_candidates') else args.sources
            assert common.sha(root / value['file']) == value['sha256'], (family, key)
        ds = common.pdf_cells(args.sources / sources['datasheet']['file'], family, route_pattern=PATTERN)
        rm = common.pdf_cells(args.sources / sources['reference_manual']['file'], family, True, PATTERN)
        sdk = sdk_cells(args.sources / sources['gpio_header']['file'])
        with zipfile.ZipFile(args.sources / sources['sdk_archive']['file']) as archive:
            name = Path(sources['gpio_header']['file']).name
            assert any(hashlib.sha256(archive.read(m)).hexdigest() == sources['gpio_header']['sha256']
                for m in archive.namelist() if m.endswith('/' + name))
        sys.path.insert(0, str(ROOT / 'cw32-data/tools'))
        from extract_pinouts import extract_rows
        assert extract_rows(args.sources / sources['datasheet']['file'], profile) == common.read(ROOT / sources['pinouts']['file'])['table_rows']
        candidate = common.read(ROOT / sources['sdk_candidates']['file'])
        candidates = {(r['pin'], r['af']): r for r in candidate.get('routes', []) + candidate.get('unresolved', [])
            if PATTERN.fullmatch(r['function'].replace('ATIMBKIN', 'ATIMBK').replace('ATIM', 'ATIM_'))}
        for key, row in sdk.items():
            for field in ('pin','af','function','source_macro','source_line','gpio_register','gpio_field'):
                assert row[field] == candidates[key][field]
        sidecar, evidence = common.construct(family, sources, ds, rm, sdk, lambda function: 'ATIMBK' if function == 'ATIMBKIN' else function)
        sidecar['kind'] = 'complementary-pwm'
        sidecar['datasheet']['af_evidence'] = 'Own PDF AF cells, package grid and SDK macros; docs/atim-complementary-route-evidence.json.'
        sidecar['scope'] = 'Buffered ATIM CH1N–4N and external BK1. Main pins use independently qualified main-output routes. BK2, CH5/6, comparator and system-source configuration withheld.'
        sidecar['capability'] = dict(channels=4, dead_time_max_ticks=1008, break_inputs=1)
        path = ROOT / f'cw32-data/af/{profile.lower()}-atim-complementary.yaml'
        if args.write: common.emit(path, sidecar)
        else: assert common.read(path) == sidecar
        proof['families'][profile] = evidence
        print(f"PASS {profile}: {len(sidecar['routes'])} qualified routes, {len(sidecar['excluded_routes'])} exclusions")
    path = ROOT / 'docs/atim-complementary-route-evidence.json'
    if args.write: common.emit(path, proof)
    else:
        assert common.read(path) == proof
        audit_semantics(args.sources)
if __name__ == '__main__': main()
