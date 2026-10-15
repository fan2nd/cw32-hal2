#!/usr/bin/env python3
"""Own-source route/command/data audit for F030/A030 complementary PWM; no HAL tests."""
import argparse
import hashlib
import json
import re
import sys
from pathlib import Path
import fitz
import yaml
ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tests'))
import verify_atim_pwm_routes as common
PATTERN = re.compile(r'ATIM_(CH[1-3]B)')
PROOF = ROOT / 'docs/classic-atim-complementary-route-evidence.json'


def sdk_cells(path):
    regex = re.compile(r'^#define\s+(P([A-F])(\d+)_AFx_(ATIM(CH[1-3]B)))\(\)\s+\(CW_GPIO([A-F])->(AFR[HL])_f\.((?:AFR|PIN)(\d+))\s*=\s*(\d+)\)')
    cells = {}
    for line, text in enumerate(path.read_text().splitlines(), 1):
        m = regex.match(text)
        if not m:
            continue
        macro, port, n, function, signal, bank, register, field, bit, af = m.groups()
        assert port == bank and int(n) == int(bit)
        assert register == ('AFRL' if int(n) < 8 else 'AFRH')
        pin, af = f'P{port}{int(n)}', int(af)
        cells[pin, af] = dict(pin=pin, af=af, function=function, source_macro=macro,
            source_line=line, gpio_register=register, gpio_field=field, peripheral='ATIM', signal=signal)
    return cells


def routes(sources, write, source_only):
    proof = dict(schema_version=1, families={})
    for family in ('F030', 'A030'):
        profile = 'CW32' + family
        src = common.sources(family)
        for key, source in src.items():
            base = ROOT if key in ('pinouts', 'sdk_candidates') else sources
            assert common.sha(base / source['file']) == source['sha256']
        ds = common.pdf_cells(sources / src['datasheet']['file'], family, route_pattern=PATTERN)
        rm = common.pdf_cells(sources / src['reference_manual']['file'], family, True, PATTERN)
        sdk = sdk_cells(sources / src['gpio_header']['file'])
        candidate = common.read(ROOT / src['sdk_candidates']['file'])
        candidates = {(r['pin'], r['af']): r for r in candidate.get('routes', []) + candidate.get('unresolved', []) if re.fullmatch(r'ATIMCH[1-3]B', r['function'])}
        for key, cell in sdk.items():
            assert all(cell[k] == candidates[key][k] for k in ('pin','af','function','source_macro','source_line','gpio_register','gpio_field'))
        sidecar, evidence = common.construct(family, src, ds, rm, sdk)
        sidecar['kind'] = 'complementary-pwm'
        sidecar['datasheet']['af_evidence'] = 'Own PDF AF cells, package grid and exact SDK macros; docs/classic-atim-complementary-route-evidence.json.'
        sidecar['scope'] = 'Classic CH1B-3B routes. Main A routes are independently qualified. Complete pairs are optional; no BK or per-pair gate API.'
        sidecar['capability'] = dict(channels=3, dead_time_max_ticks=1010, break_inputs=0)
        sidecar['pin_role_mapping'] = {f'CH{i}B': f'TimerComplementaryPin<ATIM, Ch{i}>' for i in range(1,4)}
        assert len(sidecar['routes']) == 9 and not sidecar['excluded_routes']
        path = ROOT / f'cw32-data/af/{profile.lower()}-atim-complementary.yaml'
        if write:
            common.emit(path, sidecar)
        else:
            assert common.read(path) == sidecar, profile
        proof['families'][profile] = evidence
    if write:
        common.emit(PROOF, proof)
    else:
        assert common.read(PROOF) == proof
    if source_only or write:
        return
    checked = 0
    for path in sorted((ROOT / 'cw32-data/data/chips').glob('*.json')):
        chip = common.read(path)
        if chip['line'] not in proof['families']:
            continue
        timer = next(p for p in chip['cores'][0]['peripherals'] if p['name'] == 'ATIM')
        sidecar = common.read(ROOT / f"cw32-data/af/{chip['line'].lower()}-atim-complementary.yaml")
        assert timer['atim_complementary'] == sidecar['capability']
        bonded = {p['name'] for p in chip['cores'][0]['pins']}
        expected = {(r['pin'],r['signal'],r['af']) for r in sidecar['routes'] if r['pin'] in bonded}
        selected = [r for r in timer['pins'] if r['signal'] in ('CH1B','CH2B','CH3B','CH1N','CH2N','CH3N','CH4N')]
        actual = {(r['pin'],r['signal'],r['af']) for r in selected}
        assert len(selected) == len(actual) and expected == actual, path.name
        if chip['name'] in ('CW32F030', 'CW32F030F6P7', 'CW32F030F8V7'):
            assert {r[1] for r in actual} == {'CH1B', 'CH3B'}, path.name
        checked += 1
    assert checked >= 8
    print(f'PASS {checked} classic exact-package/alias projections; no fabricated CHxN or unbonded CH2B')


def commands(sources, source_only):
    evidence = common.read(ROOT / 'docs/classic-atim-complementary-evidence.json')
    writes = common.read(ROOT / 'cw32-data/register-writes.yaml')['registers']
    reuse = common.read(ROOT / 'cw32-data/register-reuse.yaml')['groups']
    covered = set()
    for source in evidence['classic_command_sources']:
        path = sources / source['file']
        assert common.sha(path) == source['sha256']
        with fitz.open(path) as pdf:
            text = pdf[source['icr_pdf_page_index']].get_text()
        assert re.search(r'Reset value:\s*0x0007\s*FFFF', text)
        assert '保留位，请保持默认值' in text
        fields = {name: int(bit) for bit, name in re.findall(r'(?m)^\s*(\d+)\s+([A-Z0-9]+)\s+R1W0\b', text)}
        variant = source['variant']
        ir = common.read(ROOT / f'cw32-data/registers/{variant}.yaml')
        assert fields == {f['name']: f['bit_offset'] for f in ir['fieldset/ICR']['fields']}
        command, = writes[variant]
        assert command['reset_value'] == command['write_noop'] == 524287
        assert command['zero_to_clear_fields'] == [f['name'] for f in ir['fieldset/ICR']['fields']]
        assert sum(1 << b for b in fields.values()) == 524285
        assert 524287 & ~sum(1 << b for b in fields.values()) == 2
        group = next(g for g in reuse if g['canonical'] == variant + '.yaml')
        digest = hashlib.sha256(json.dumps(ir, sort_keys=True, separators=(',', ':')).encode()).hexdigest()
        assert digest == group['canonical_ir_sha256'] == evidence['unchanged_register_identities'][variant]
        if not source_only:
            assert common.read(ROOT / f'cw32-data/data/registers/{variant}.json') == ir
            assert common.read(ROOT / f'cw32-data/data/register-writes/{variant}.json') == {'schema_version':1, 'registers':{variant:[command]}}
            pac = (ROOT / f'cw32-metapac/src/peripherals/{variant}.rs').read_text()
            seed = pac.split('impl regs::Icr {', 1)[1]
            for name in ('reset_value', 'write_noop'):
                m = re.search(rf'pub const fn {name}\(\) -> Self\s*\{{\s*Self\((0x[\da-f]+|\d+)\)', seed)
                assert m and int(m[1], 0) == 524287
        covered.add(variant)
    assert covered == {'atim_v1', 'atim_cw32f003_v1', 'atim_cw32l052_v1'}
    source = evidence['pwm_source']
    with fitz.open(sources / source['file']) as pdf:
        for page, anchors in source['anchors'].items():
            text = re.sub(r'\s+', '', pdf[int(page)].get_text())
            assert all(anchor in text for anchor in anchors), (page, anchors)
    print('PASS own-family ICR/no-op sources, reserved bit1, unchanged current reuse/IR and exact generated command projections')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--sources', type=Path, required=True)
    parser.add_argument('--write', action='store_true', help='Rebuild authored route sidecars/evidence from own source PDFs')
    parser.add_argument('--source-only', action='store_true')
    args = parser.parse_args()
    routes(args.sources, args.write, args.source_only)
    if not args.write:
        commands(args.sources, args.source_only)
    print('PASS F030/A030 classic complementary source qualification')

if __name__ == '__main__':
    main()
