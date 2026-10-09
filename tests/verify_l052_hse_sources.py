#!/usr/bin/env python3
"""Verify own L052 source facts and native data/PAC projection; no HAL execution."""
from pathlib import Path
from fractions import Fraction
import hashlib
import json
import os
import re
import yaml

ROOT = Path(__file__).resolve().parents[1]
SOURCES = Path(os.environ.get('CW32_SOURCES', '/workspace/shared/cw32-sources'))
def load(path):
    path = ROOT / path
    return yaml.safe_load(path.read_text()) if path.suffix == '.yaml' else json.loads(path.read_text())
def digest(data): return hashlib.sha256(data).hexdigest()

receipt = load('docs/qualified-l052-hse-source-receipt.json')
lock = {a['id']: a for a in load('sources/evidence-sources.json')['artifacts']}
texts = {}
for source in receipt['source_identities']:
    original = lock[source['id']]
    assert source['sha256'] == original['sha256']
    assert source['url'] == original['url']
    assert digest((SOURCES / original['path']).read_bytes()) == original['sha256']
    if source['kind'] == 'pdf':
        body = (SOURCES / original['text']['path']).read_bytes()
        assert digest(body) == original['text']['sha256']
        texts[source['id']] = body.decode().split('\f')
    for member in source.get('selected_members', []):
        assert digest((SOURCES / member['path']).read_bytes()) == member['sha256']
rm = texts['vendor:CW32L052_UserManual_CN_V1.5.pdf']
ds = texts['vendor:CW32L052_DataSheet_CN_V1.3.pdf']
compact = lambda value: re.sub(r'\s+', '', value)
# Assertions refer to exact own table rows, not another family's source text.
assert all(x in compact(rm[69]) for x in ['000：设置SysClk的时钟源为HSI', '001：设置SysClk的时钟源为HSE', '011：设置SysClk的时钟源为LSI', '100：设置SysClk的时钟源为LSE'])
assert '自动将SysClk的时钟源切换为HSI8MHz' in compact(rm[70])
assert all(x in compact(rm[74]) for x in ['23:22PFREQRANGERW', '21:20PDRIVERRW', '00：4M~8M', '11：24M~32M'])
assert all(x in compact(rm[192]) for x in ['000：外部低频时钟LSE', '010：内置低频时钟LSI', '100：外部高速时钟HSE，128分频', '111：外部高速时钟HSE，1024分频'])
assert all(x in compact(ds[25]) for x in ['OSC_IN', 'OSC_OUT', 'PF00', 'PF01'])

expected = {
    'sysctrl_cw32l052_v1': {
        'HseDrive': {'LEVEL0': 0, 'LEVEL1': 1, 'LEVEL2': 2, 'LEVEL3': 3},
        'HseRange': {'MHZ4_TO_8': 0, 'MHZ8_TO_16': 1, 'MHZ16_TO_24': 2, 'MHZ24_TO_32': 3},
        'HseWait': {'CYCLES8192': 0, 'CYCLES32768': 1, 'CYCLES131072': 2, 'CYCLES262144': 3},
        'Sysclk': {'HSI': 0, 'HSE': 1, 'LSI': 3, 'LSE': 4},
    },
    'rtc_cw32l052_v1': {'Source': {'LSE': 0, 'LSI': 2, 'HSE_DIV128': 4, 'HSE_DIV256': 5, 'HSE_DIV512': 6, 'HSE_DIV1024': 7}},
}
ledger = load('cw32-data/register-reuse.yaml')
for name, enums in expected.items():
    authored = load('cw32-data/registers/' + name + '.yaml')
    ir = load('cw32-data/data/registers/' + name + '.json')
    for enum, variants in enums.items():
        assert {v['name']: v['value'] for v in authored['enum/' + enum]['variants']} == variants
        assert {v['name']: v['value'] for v in ir['enum/' + enum]['variants']} == variants
    group = next(g for g in ledger['groups'] if g['canonical'] == name + '.yaml')
    assert group['source_versions'] == [name + '.yaml']
    assert digest(json.dumps(ir, sort_keys=True, separators=(',', ':')).encode()) == group['canonical_ir_sha256']
    assert group['canonical_ir_history'][-1]['after_sha256'] == group['canonical_ir_sha256']
sysctrl = load('cw32-data/registers/sysctrl_cw32l052_v1.yaml')
fields = {f['name']: f for f in sysctrl['fieldset/HSE']['fields']}
for name, offset, enum in [('DRIVER', 0, 'HseDrive'), ('FREQRANGE', 2, 'HseRange'), ('WAITCYCLE', 4, 'HseWait'), ('PDRIVER', 20, 'HseDrive'), ('PFREQRANGE', 22, 'HseRange')]:
    assert (fields[name]['bit_offset'], fields[name]['bit_size'], fields[name]['enum']) == (offset, 2, enum)
assert not any(i['name'] == 'PLL' for i in sysctrl['block/SYSCTRL']['items'])
assert not any(f['name'] == 'PLLEN' for f in sysctrl['fieldset/CR1']['fields'])
pac = (ROOT / 'cw32-metapac/src/peripherals/sysctrl_cw32l052_v1.rs').read_text()
assert 'pub fn set_stable(' not in pac
for setter in ['set_pdriver', 'set_pfreqrange', 'set_driver', 'set_freqrange']:
    assert setter in pac

review = load('cw32-data/hse-qualified.yaml')['families']['CW32L052']
electrical = load('cw32-data/electrical.yaml')['profiles']['CW32L052']
assert electrical['clock_limits']['hse'] == review['hse']
assert electrical['hse_sources'] == review['sources']
for mode in ['crystal', 'bypass']:
    assert review['hse'][mode] == {'minimum_hz': 4000000, 'maximum_hz': 32000000, 'supply_mv': [1650, 5500], 'temperature_c': [-40, 85]}
assert (Fraction(48000000 * 98, 100 * 6), Fraction(48000000 * 102, 100 * 6)) == (7840000, 8160000)
assert 4000000 * 2000 > 131072 * 36080
parts = 0
for path in (ROOT / 'cw32-data/data/chips').glob('CW32L052*.json'):
    chip = json.loads(path.read_text())
    sysctrl = next(p for p in chip['cores'][0]['peripherals'] if p['name'] == 'SYSCTRL')
    assert sysctrl['clock_limits'] == electrical['clock_limits']
    assert {p['signal']: p['pin'] for p in sysctrl['pins'] if p['signal'] in ['HSE_IN', 'HSE_OUT']} == {'HSE_IN': 'PF0', 'HSE_OUT': 'PF1'}
    for package in chip['packages']:
        assert {s: p['position'] for p in package['pins'] for s in p['signals'] if s in ['OSC_IN', 'OSC_OUT']} == {'OSC_IN': '5', 'OSC_OUT': '6'}
        parts += 1
assert parts == 3
print('PASS own L052 source identities, source tables, enum-only canonical groups, native pre-start PAC fields, electrical facts and all three exact package HSE routes; no HAL or hardware execution')
