#!/usr/bin/env python3
"""Validate own-source DMA/PAC facts. No HAL execution, harness, mock or fixture."""
from pathlib import Path
import hashlib
import json
import os
import re
import yaml

ROOT = Path(__file__).resolve().parents[1]
SOURCES = Path(os.environ.get('CW32_SOURCES', '/workspace/shared/cw32-sources'))
def load(path): return yaml.safe_load(path.read_text()) if path.suffix == '.yaml' else json.loads(path.read_text())
def digest(data): return hashlib.sha256(data).hexdigest()
e = load(ROOT / 'docs/dma-owned-copy-evidence.json')
lock = load(ROOT / 'sources/evidence-sources.json')
for key, source in e['sources'].items():
    pin = next(a for a in lock['artifacts'] if a['id'] == source['source_ref'])
    assert all(source[k] == pin[k] for k in ('path', 'url', 'sha256', 'bytes'))
    assert digest((SOURCES / source['path']).read_bytes()) == source['sha256']
    assert source['text'] == pin['text']
    raw = (SOURCES / source['text']['path']).read_bytes()
    assert digest(raw) == source['text']['sha256']
    if '8.8.3' not in source['sections']:
        continue
    pages = raw.decode().split('\f')
    csr = pages[source['sections']['8.8.3']['pdf_page_index']]
    trig = pages[source['sections']['8.8.4']['pdf_page_index']]
    assert 'DMA_CSRy' in csr and 'STATUS' in csr and 'SIZE' in csr and 'TRANS' in csr
    assert 'SOFTSRC' in trig and 'TYPE' in trig
    if key == 'x030_en':
        for token in ['000: Initial', '101: Transfer completed', '00: 8bit', '01: 16bit', '10: 32bit',
                      '0: Bulk transfer (BULK)', '1: Block transfer (BLOCK)']:
            assert token in csr, (key, token)
        for token in ['0: Software trigger mode', '1: In hardware trigger mode', 'R0: Transfer completed', 'W0: No function']:
            assert token in trig, (key, token)
    else:
        for token in ['000：初始状态', '001：传输错误，传输地址超出寻址范围', '010：传输错误，传输停止请求引起中止',
                      '011：传输错误，访问传输来源地址出错', '100：传输错误，访问传输目的地址出错',
                      '101：传输完成', '00：8 比特', '01：16 比特', '10：32 比特',
                      '0：批量传输（BULK）', '1：块传输（BLOCK）']:
            assert token in csr, (key, token)
        for token in ['0：软件触发模式', '1：硬件触发模式', 'R0：传输已完成', 'W0：无功能']:
            assert token in trig, (key, token)
    if '8.8.2' in source['sections']:
        icr = pages[source['sections']['8.8.2']['pdf_page_index']]
        assert 'DMA_ICR' in icr and 'Reset value: 0xFFFF FFFF' in icr and 'R1W0' in icr
        assert re.search(r'W0[:：].*(?:Clear|clear|清除)', icr)
        assert re.search(r'W1[:：].*(?:No function|无功能)', icr)

reg = 'dmachannel_v1'
authored = yaml.safe_load((ROOT / 'cw32-data/registers' / (reg + '.yaml')).read_text())
generated = load(ROOT / 'cw32-data/data/registers' / (reg + '.json'))
for name, values in e['enums'].items():
    assert {v['name']: v['value'] for v in authored['enum/' + name]['variants']} == values
    assert authored['enum/' + name] == generated['enum/' + name]
for fieldset, field, name in [('CSR','TRANS','TransferMode'), ('CSR','SIZE','Width'), ('CSR','STATUS','Status'), ('TRIG','TYPE','Trigger')]:
    assert next(f for f in generated['fieldset/' + fieldset]['fields'] if f['name'] == field)['enum'] == name
for reg in ('dmachannel_v1', 'dma_v1'):
    ir = load(ROOT / 'cw32-data/data/registers' / (reg + '.json'))
    group = next(g for g in load(ROOT / 'cw32-data/register-reuse.yaml')['groups'] if g['canonical'] == reg + '.yaml')
    assert digest(json.dumps(ir, sort_keys=True, separators=(',', ':')).encode()) == group['canonical_ir_sha256']
command = load(ROOT / 'cw32-data/register-writes.yaml')['registers']['dma_v1'][0]
assert command['reset_value'] == command['write_noop'] == 0xffffffff
assert set(command['zero_to_clear_fields']) == {f'{kind}{index}' for index in range(1,6) for kind in ('TC','TE')}
for name, size in e['sram'].items():
    if name == 'base': continue
    chip = load(ROOT / 'cw32-data/data/chips' / (name + '.json'))
    memory = chip['memory'][0]
    ram = next(region for region in memory if region['name'] == 'RAM')
    assert ram['address'] == e['sram']['base'] and ram['size'] == size
for name in ('CW32F030','CW32A030'):
    assert not load(ROOT / 'cw32-data/data/chips' / (name + '.json')).get('memory')
print('PASS: ten pinned own documents/texts, eight own DMA enum tables, three ICR seeds, generated enums/reuse hashes and x030 exact-part SRAM bounds; no HAL execution')
