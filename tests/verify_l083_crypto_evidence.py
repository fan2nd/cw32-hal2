#!/usr/bin/env python3
"""Own-source/PAC data validation only; no HAL execution, mocks or test harness."""
from pathlib import Path
import hashlib
import json
import os
import re
import zipfile
import yaml

ROOT = Path(__file__).resolve().parents[1]
SOURCES = Path(os.environ.get('CW32_SOURCES', '/workspace/shared/cw32-sources'))
def load(path): return yaml.safe_load(path.read_text()) if path.suffix == '.yaml' else json.loads(path.read_text())
def digest(data): return hashlib.sha256(data).hexdigest()
e = load(ROOT / 'docs/l083-aes-trng-evidence.json')
lock = load(ROOT / 'sources/evidence-sources.json')
for s in e['sources'].values():
    canonical = next(a for a in lock['artifacts'] if a['id'] == s['source_ref'])
    assert all(s[k] == canonical[k] for k in ('path', 'url', 'sha256', 'bytes'))
    assert digest((SOURCES / s['path']).read_bytes()) == s['sha256']
sdk = next(a for a in lock['artifacts'] if a['id'] == e['sources']['sdk']['source_ref'])
with zipfile.ZipFile(SOURCES / sdk['path']) as archive:
    for m in e['sdk_members']:
        assert len(m['members']) == 1
        raw = archive.read(m['members'][0])
        assert raw == (SOURCES / m['path']).read_bytes()
        assert digest(raw) == m['sha256']
        assert next(x for x in sdk['members'] if x['path'] == m['path'])['sha256'] == m['sha256']
manual = (SOURCES / 'CW32L083_UserManual_CN_V2.0.txt').read_text()
# Check the actual own-manual chapter/register tables, not unrelated occurrences.
trng = manual[manual.index('\n26     真随机数发生器'):manual.index('\n27     液晶控制器')]
aes = manual[manual.index('\n28     高级加密标准模块'):manual.index('\n29     调试接口')]
for chapter, name in [(aes, 'AES'), (trng, 'TRNG')]:
    assert re.search(r'W1：启动\s*'+name+r'\s*运算', chapter)
    assert re.search(r'R0：'+name+r'\s*运算完成', chapter)
    assert re.search(r'R1：'+name+r'\s*正在进行运算', chapter)
for encoding, count in [('001', 8), ('010', 16), ('011', 32), ('100', 64), ('101', 128), ('110', 256)]:
    assert re.search(encoding+r'：移位\s*'+str(count)+r'\s*次', trng)
for encoding, count in [('00',128),('01',192),('10',256),('11',128)]:
    assert re.search(encoding+r'：密钥长度为\s*'+str(count)+r'\s*比特', aes)
assert 'AES 外设不支持 DMA 访问' in aes
assert 'TRNG 寄存器、AES 寄存器' in manual
assert e['facts'] == load(ROOT / 'cw32-data/crypto.yaml')
for kind in ('aes','trng'):
    name=kind+'_cw32l083_v1'
    authored=yaml.safe_load((ROOT / 'cw32-data/registers' / (name+'.yaml')).read_text())
    generated=load(ROOT / 'cw32-data/data/registers' / (name+'.json'))
    for selector, values in e['enums'].items():
        prefix, enum=selector.split('.')
        if prefix == kind:
            assert {v['name']:v['value'] for v in authored['enum/'+enum]['variants']} == values
            assert generated['enum/'+enum] == authored['enum/'+enum]
    assert {v['name']:v['value'] for v in generated['enum/Operation']['variants']} == {'IDLE':0,'ACTIVE':1}
    group=next(g for g in load(ROOT/'cw32-data/register-reuse.yaml')['groups'] if g['canonical']==name+'.yaml')
    assert group['canonical_ir_sha256'] == digest(json.dumps(generated,sort_keys=True,separators=(',',':')).encode())
    for fieldset in ('CR',) if kind=='aes' else ('CR1','CR2'):
        for f in generated['fieldset/'+fieldset]['fields']:
            if f['name'] in ('START','MODE','KEYSIZE','SOURCE','SHIFT'): assert f.get('enum')
chips = [load(p) for p in (ROOT/'cw32-data/data/chips').glob('CW32L083*.json')]
assert len(chips)==6
for chip in chips:
    for name,address in [('AES',0x40026000),('TRNG',0x40025000)]:
        p=next(p for p in chip['cores'][0]['peripherals'] if p['name']==name)
        assert p['address']==address
        assert not p.get('interrupts') and not p.get('dma_channels')
        c=p['rcc_control']; assert c['enable']=={'field':name,'register':'AHBEN'}
        assert c['reset']=={'field':name,'register':'AHBRST'} and c['reset_asserted_value'] is False
        assert c['bus_clock']=='HCLK' and c['enable_active_value'] is True
print('PASS: own-L083 source hashes, ZIP member bytes, START semantics, enum encodings, six chip RCC/IRQ/DMA metadata selections and whole-IR fingerprints; no HAL execution')
