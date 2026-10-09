#!/usr/bin/env python3
"""Verify own L012 HSE source/data/PAC projection; never execute or model HAL."""
from pathlib import Path
from fractions import Fraction
import hashlib
import json
import os
import re
import sys
import xml.etree.ElementTree as ET
import yaml

ROOT = Path(__file__).resolve().parents[1]
SOURCES = Path(os.environ.get('CW32_SOURCES', '/workspace/shared/cw32-sources'))
SOURCE_ONLY = '--source-only' in sys.argv

def load(name):
    p = ROOT / name
    return yaml.safe_load(p.read_text()) if p.suffix == '.yaml' else json.loads(p.read_text())
def digest(value):
    return hashlib.sha256(value).hexdigest()
def compact(value):
    return re.sub(r'\s+', '', value)

receipt = load('docs/qualified-l012-hse-source-receipt.json')
lock = {a['id']: a for a in load('sources/evidence-sources.json')['artifacts']}
texts = {}
for source in receipt['source_identities']:
    original = lock[source['id']]
    assert source['sha256'] == original['sha256']
    assert source['url'] == original['url']
    assert digest((SOURCES / original['path']).read_bytes()) == original['sha256']
    if 'text' in original:
        body = (SOURCES / original['text']['path']).read_bytes()
        assert digest(body) == original['text']['sha256']
        texts[source['id']] = body.decode().split('\f')
rm = texts['vendor:CW32L012_UserManual_CN_V1.4.pdf']
ds = texts['vendor:CW32L012_DataSheet_CN_V1.0.pdf']
assert 'HSIOSC安全工作范围为90~100MHz' in compact(rm[58])
assert '自动将SysClk的时钟源切换为HSI4MHz' in compact(rm[70])
assert all(t in compact(rm[69]) for t in [
    '000：设置SysClk的时钟源为HSI', '001：设置SysClk的时钟源为HSE',
    '011：设置SysClk的时钟源为LSI', '100：设置SysClk的时钟源为LSE'])
assert all(t in compact(rm[75]) for t in [
    '23:20PDRIVERRW', '19STABLERO', '18:8DETCNTRW', '24DIGFLTRW', '0x0027FF22'])
assert 'fRTCCLKD=fRTCCLK/(PSC1+1)' in compact(rm[190])
assert '小于等于1MHz' in compact(rm[190])
assert all(t in compact(rm[201]) for t in [
    '000：外部低速时钟LSE', '001：外部高速时钟HSE',
    '010：内部低速时钟LSI', '011：内部高速时钟HSIOSC'])
assert all(t in compact(ds[34]) for t in ['55PF00', '66PF01', 'OSC_IN', 'OSC_OUT'])
assert '4~32MHz' in compact(ds[55])
assert all(t in compact(ds[53]) for t in ['用户外部时钟源频率1-32MHz', '0.7VDDIOx', '0.3VDDIOx'])
assert '31HALLTIMTRGORW' in compact(rm[655])
assert 'PCLK' in compact(rm[654]) and 'OPACLK来源配置' in compact(rm[676])
assert 'AZRUN' in compact(rm[676]) and '电平校准该标志不起作用' in compact(rm[676])
assert 'TENx位设置为0' in compact(rm[630]) and 'TENx位设置为1' in compact(rm[630])

review = load('cw32-data/hse-qualified.yaml')['families']
electrical = load('cw32-data/electrical.yaml')
profile = electrical['profiles']['CW32L012']
hse = review['CW32L012']['hse']
assert electrical['policies']['cw32-data/hse-qualified.yaml'] == digest((ROOT / 'cw32-data/hse-qualified.yaml').read_bytes())
assert profile['clock_limits']['hse'] == hse
assert profile['hse_sources'] == review['CW32L012']['sources']
assert profile['clock_limits']['hsi_operating_range_hz'] == review['CW32L012']['hsi_operating_range_hz'] == [90000000, 100000000]
assert profile['clock_limits']['default_hsi_divisor'] == 12
assert hse['fixed_ccs_hsi_divisor'] == 24
assert hse['ccs_lsi_maximum_hz'] == 36080 and hse['filter_maximum_hz'] == 8000000
assert 'frequency_ranges_hz' not in hse
for mode in ['crystal', 'bypass']:
    assert hse[mode] == {'minimum_hz': 4000000, 'maximum_hz': 32000000,
                         'supply_mv': [1700, 5500], 'temperature_c': [-40, 85]}
assert (Fraction(96000000 * 98, 100 * 24), Fraction(96000000 * 102, 100 * 24)) == (3920000, 4080000)
assert 4000000 * 2000 > 131072 * 36080
# Explicit older fixed-fallback facts remain tied to own source text. No missing
# optional fact implies use of the default or configured divider.
for family, divisor, manual, mhz in [
    ('CW32L010', 12, 'CW32L010_UserManual_CN_V1.2.pdf', 4),
    ('CW32L011', 24, 'CW32L011_UserManual_CN_V1.1.pdf', 4),
    ('CW32L052', 6, 'CW32L052_UserManual_CN_V1.5.pdf', 8),
]:
    assert review[family]['hse']['fixed_ccs_hsi_divisor'] == divisor
    assert electrical['profiles'][family]['clock_limits']['hse'] == review[family]['hse']
    source = lock['vendor:' + manual]
    assert digest((SOURCES / source['path']).read_bytes()) == source['sha256']
    body = (SOURCES / source['text']['path']).read_bytes()
    assert digest(body) == source['text']['sha256']
    assert f'自动将SysClk的时钟源切换为HSI{mhz}MHz' in compact(body.decode())

registers = load('cw32-data/registers/sysctrl_cw32l012_v1.yaml')
expected_enums = {
    'HseDrive': {f'LEVEL{i}': i for i in range(8)},
    'HseWait': {'CYCLES8192': 0, 'CYCLES32768': 1, 'CYCLES131072': 2, 'CYCLES262144': 3},
    'Sysclk': {'HSI': 0, 'HSE': 1, 'LSI': 3, 'LSE': 4},
}
for name, expected in expected_enums.items():
    assert {v['name']: v['value'] for v in registers['enum/' + name]['variants']} == expected
fields = {f['name']: f for f in registers['fieldset/HSE']['fields']}
assert not {'FREQ', 'FREQRANGE', 'PFREQRANGE'} & fields.keys()
for field, offset, size, enum in [('DRIVER', 0, 4, 'HseDrive'), ('PDRIVER', 20, 4, 'HseDrive'), ('WAITCYCLE', 4, 2, 'HseWait')]:
    assert (fields[field]['bit_offset'], fields[field]['bit_size'], fields[field]['enum']) == (offset, size, enum)
assert not any(i['name'] == 'PLL' for i in registers['block/SYSCTRL']['items'])
vc = load('cw32-data/registers/vc_cw32l012_v1.yaml')
assert {f['bit_offset'] for f in vc['fieldset/CR2']['fields']} == set(range(32))
assert all(f['bit_size'] == 1 for f in vc['fieldset/CR2']['fields'])
svd_path = 'cw32l012/IDEsupport/MDK/WHXY.CW32L012_DFP.1.0.2/SVD/CW32L012.svd'
assert digest((SOURCES / svd_path).read_bytes()) == receipt['facts']['sources_sha256'][svd_path]
svd = ET.parse(SOURCES / svd_path)
sysctrl = next(p for p in svd.findall('.//peripheral') if p.findtext('name') == 'SYSCTRL')
for name in ['HSI', 'LSI', 'HSE', 'LSE']:
    register = next(p for p in sysctrl.findall('./registers/register') if p.findtext('name') == name)
    field = next(f for f in register.findall('./fields/field') if f.findtext('name') == 'STABLE')
    assert register.findtext('access') == 'read-write' and field.findtext('access') == 'read-only'
ledger = load('cw32-data/register-reuse.yaml')
group = next(g for g in ledger['groups'] if g['canonical'] == 'sysctrl_cw32l012_v1.yaml')
sha = digest(json.dumps(registers, sort_keys=True, separators=(',', ':')).encode())
assert group['canonical_ir_sha256'] == group['canonical_ir_history'][-1]['after_sha256'] == sha
assert group['source_versions'] == ['sysctrl_cw32l012_v1.yaml']

if not SOURCE_ONLY:
    ir = load('cw32-data/data/registers/sysctrl_cw32l012_v1.json')
    assert ir == registers
    pac = (ROOT / 'cw32-metapac/src/peripherals/sysctrl_cw32l012_v1.rs').read_text()
    assert 'fn set_stable(' not in pac
    assert all('fn '+setter+'(' in pac for setter in ['set_mode', 'set_driver', 'set_pdriver', 'set_waitcycle', 'set_detcnt', 'set_digflt'])
    chips = sorted((ROOT / 'cw32-data/data/chips').glob('CW32L012*.json'))
    assert len(chips) == 3
    for path in chips:
        chip = json.loads(path.read_text())
        peripherals = {p['name']: p for p in chip['cores'][0]['peripherals']}
        sysctrl = peripherals['SYSCTRL']
        assert sysctrl['clock_limits'] == profile['clock_limits']
        assert {p['signal']: p['pin'] for p in sysctrl['pins'] if p['signal'] in ['HSE_IN', 'HSE_OUT']} == {'HSE_IN': 'PF0', 'HSE_OUT': 'PF1'}
        assert all(name in peripherals for name in ['ADC1','ADC2','VC1','VC2','VC3','VC4','OPA1','OPA2','DAC','LVD'])
        assert 'rcc_control' not in peripherals['LVD']
        for package in chip['packages']:
            assert {s: p['position'] for p in package['pins'] for s in p['signals'] if s in ['OSC_IN', 'OSC_OUT']} == {'OSC_IN': '5', 'OSC_OUT': '6'}
print('PASS L012 own source identities/tables, explicit fixed fallback and legal entry range, typed selector-free HSE, original RO status and reuse history' + (' (source only; generated checks pending)' if SOURCE_ONLY else ', generated PAC/data and exact package routes') + '; no HAL or hardware execution')
