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
    # Consumer backlinks grow independently of the locked text identity.
    assert {k: v for k, v in source['text'].items() if k != 'evidence'} == {
        k: v for k, v in pin['text'].items() if k != 'evidence'
    }
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

# Finite native four-channel own-source extension. This reads source facts and
# generated metadata/PAC only; it does not inspect or execute HAL behavior.
import io
import xml.etree.ElementTree as ET
import zipfile

four = load(ROOT / 'docs/four-channel-dma-owned-evidence.json')
assert four['schema_version'] == 1
assert len(four['sources']) == 54
family_names = {'CW32L031', 'CW32L052', 'CW32R031', 'CW32W031'}
assert {f['family'] for f in four['families']} == family_names
assert len(four['families']) == 4
exact_features = {
    'cw32l031c8t6', 'cw32l031c8u6', 'cw32l031f8p6', 'cw32l031f8u6',
    'cw32l031k8u6', 'cw32l031k8v6', 'cw32l052c8t6', 'cw32l052r8s6',
    'cw32l052r8t6', 'cw32r031c8u6', 'cw32w031r8u6',
}
assert {p['feature'] for p in four['exact_parts']} == exact_features
assert len(four['exact_parts']) == 11
assert {p['feature'] for p in four['generic_aliases']} == {f.lower() for f in family_names}
assert len(four['generic_aliases']) == 4


def four_index(pointer):
    value = lock
    for component in pointer.strip('/').split('/'):
        value = value[int(component)] if isinstance(value, list) else value[component]
    return value


def four_compact(value):
    return re.sub(r'\s+', '', value)


def four_tokens(value, tokens, where):
    value = four_compact(value)
    for token in tokens:
        assert four_compact(token) in value, (where, token)


four_bytes = {}
for source in four['sources']:
    pin = four_index(source['source_index_pointer'])
    artifact = lock['artifacts'][int(source['source_index_pointer'].split('/')[2])]
    identity = source['identity']
    # Bind the complete selected node, including provenance/licensing fields.
    # The archive's catalog is not itself a selected member: all used members
    # are bound separately below, without freezing unrelated catalog entries.
    observed_identity = {k: v for k, v in pin.items() if k != 'evidence'
                         and not (k == 'members' and source['source_ref'].startswith('vendor:'))}
    if source['source_ref'].startswith('vendor:'):
        assert source['source_ref'] == artifact['id']
    if 'text' in observed_identity:
        observed_identity['text'] = {k: v for k, v in observed_identity['text'].items() if k != 'evidence'}
    expected_identity = dict(identity)
    if 'text' in expected_identity:
        expected_identity['text'] = {k: v for k, v in expected_identity['text'].items() if k != 'evidence'}
    assert observed_identity == expected_identity, source['source_ref']
    assert source['official_url'] == artifact['url']
    raw = (SOURCES / identity['path']).read_bytes()
    assert len(raw) == identity['bytes'] and digest(raw) == identity['sha256'], identity['path']
    four_bytes[identity['path']] = raw
    if 'archive_source_ref' in source:
        assert source['archive_source_ref'] == artifact['id']
        payload = (SOURCES / artifact['path']).read_bytes()
        for member in identity['members']:
            with zipfile.ZipFile(io.BytesIO(payload)) as archive:
                payload = archive.read(member)
        assert payload == raw, identity['path']
assert len(four_bytes) == 54

native = load(ROOT / 'cw32-data/data/registers/dma_cw32l031_v1.json')
assert native == load(ROOT / 'cw32-data/registers/dma_cw32l031_v1.yaml')
reuse = next(g for g in load(ROOT / 'cw32-data/register-reuse.yaml')['groups']
             if g['canonical'] == 'dma_cw32l031_v1.yaml')
assert set(reuse['source_versions']) == {f'dma_{f.lower()}_v1.yaml' for f in family_names}
assert digest(json.dumps(native, sort_keys=True, separators=(',', ':')).encode()) == reuse['canonical_ir_sha256']
channel_ir = load(ROOT / 'cw32-data/data/registers/dmachannel_v1.json')
expected_offsets = {'ISR': 0, 'ICR': 4}
for channel in range(1, 5):
    expected_offsets.update({f'{reg}{channel}': 32 * channel + offset
                             for reg, offset in [('CSR', 0), ('CNT', 4), ('SRCADDR', 8), ('DSTADDR', 12), ('TRIG', 16)]})
assert {r['name']: r['byte_offset'] for r in native['block/DMA']['items']} == expected_offsets
expected_channels = [{'channel': i - 1, 'dma': 'DMA', 'name': f'DMA_CH{i}'} for i in range(1, 5)]
expected_vectors = {'DMACH1': 9, 'DMACH23': 10, 'DMACH4': 11, 'UART1': 27, 'UART2': 28, 'UART3': 29}
expected_channel_vectors = [('DMACH1', 9), ('DMACH23', 10), ('DMACH23', 10), ('DMACH4', 11)]
expected_flags = {f'{kind}{i}': (4 * (i - 1) + bit, 1)
                  for i in range(1, 5) for kind, bit in [('TC', 0), ('TE', 1)]}
assert {f['name']: (f['bit_offset'], f['bit_size']) for f in native['fieldset/ICR']['fields']} == expected_flags


def four_svd_field(field):
    encoding = {key for key in ('bitOffset', 'bitWidth', 'lsb', 'msb', 'bitRange')
                if field.find(key) is not None}
    if encoding == {'bitOffset', 'bitWidth'}:
        offset = int(field.findtext('bitOffset'), 0)
        width = int(field.findtext('bitWidth'), 0)
    elif encoding == {'lsb', 'msb'}:
        offset = int(field.findtext('lsb'), 0)
        width = int(field.findtext('msb'), 0) - offset + 1
    else:
        raise AssertionError(('unsupported or conflicting own-SVD field encoding', field.findtext('name'), encoding))
    assert 0 <= offset < 32 and 1 <= width <= 32 - offset
    return {'name': field.findtext('name'), 'bit_offset': offset,
            'bit_size': width, 'access': field.findtext('access')}


def four_svd_register(register):
    return {
        'name': register.findtext('name'),
        'offset': int(register.findtext('addressOffset'), 0),
        'fields': [four_svd_field(f) for f in register.findall('fields/field')],
    }


four_family_pages = {}
for family in four['families']:
    name = family['family']
    pages_by_kind = {}
    for kind in ('manual', 'datasheet'):
        source = next(s for s in four['sources'] if s['source_ref'] == family[kind + '_source_ref'])
        assert source['identity']['path'] == family[kind]
        assert source['identity']['text']['path'] == family[kind + '_text_path']
        pages = four_bytes[family[kind + '_text_path']].decode().split('\f')
        pages_by_kind[kind] = pages
        for page in family[kind + '_pages']:
            assert page['printed_page'] == page['pdf_page_index']
            assert page['pdf_viewer_page'] == page['pdf_page_index'] + 1
            assert digest(pages[page['pdf_page_index']].encode()) == page['extracted_page_utf8_sha256'], (name, kind, page)
    four_family_pages[name] = pages_by_kind
    manual = pages_by_kind['manual']
    claims = family['manual_page_claims']
    for claim, numbers in claims.items():
        assert numbers == [p['pdf_page_index'] for p in family['manual_pages'] if claim in p['claims']], (name, claim)
    claim_text = {claim: '\n'.join(manual[n] for n in numbers) for claim, numbers in claims.items()}
    four_tokens(claim_text['sram'], ['8KB', 'SRAM', '0x2000 0000'], (name, 'SRAM'))
    four_tokens(claim_text['flash'], ['64KB', '0x0000 0000', '0x0000 FFFF'], (name, 'Flash'))
    four_tokens(claim_text['dma_channels'], ['4 条独立 DMA 通道', '1 ~ 65535'], (name, 'channels'))
    four_tokens(claim_text['mmio_width_and_gates'], ['其它外设只支持', '32bit', '读数据的结果不可用'], (name, 'width/gates'))
    four_tokens(claim_text['software_block'], ['软件触发 BLOCK', '传输完成后 DMA_TRIGy.SOFTSRC 自动清零'], (name, 'software'))
    four_tokens(claim_text['width_repeat_count'], ['位宽完全一致', 'REPEAT', '建议写入 1', '1 ~ 65535'], (name, 'width/count'))
    four_tokens(claim_text['terminal'], ['数据传输全部正确完成', 'TEIE', 'TCIE', 'DMA_ICR.TEy 为 0', 'DMA_ICR.TCy 为 0'], (name, 'terminal'))
    four_tokens(claim_text['csr'], ['000：初始状态', '001：传输错误，传输地址超出寻址范围',
                '010：传输错误，传输停止请求引起中止', '011：传输错误，访问传输来源地址出错',
                '100：传输错误，访问传输目的地址出错', '101：传输完成',
                '00：8 比特', '01：16 比特', '10：32 比特', '0：批量传输（BULK）', '1：块传输（BLOCK）'], (name, 'CSR'))
    four_tokens(claim_text['trig'], ['HARDSRC', 'SOFTSRC', 'R0：传输已完成', 'W0：无功能',
                '0：软件触发模式', '1：硬件触发模式'], (name, 'TRIG'))
    if name == 'CW32L052':
        assert claims['trig'] == [138, 139]
        four_tokens(manual[138], ['HARDSRC', '000001：UART1 发送 Buf 空标志'], (name, 'request page'))
        four_tokens(manual[139], ['SOFTSRC', 'TYPE', 'R0：传输已完成'], (name, 'continuation'))
    icr = claim_text['icr']
    four_tokens(icr, ['DMA_ICR', 'Address offset: 0x04', 'Reset value: 0xFFFF FFFF',
                      '保留位，请保持默认值', 'W0：清除', 'W1：无功能'], (name, 'ICR'))
    for field, (bit, width) in expected_flags.items():
        assert re.search(rf'(?m)^\s*{bit}\s+{field}\s+R1W0\b', icr), (name, field)
    four_tokens(claim_text['dma_gate'], ['Address offset: 0x30', 'DMA 模块配置时钟及工作时钟使能控制', '0：关闭', '1：使能'], (name, 'gate'))
    four_tokens(claim_text['dma_reset'], ['Address offset: 0x40', 'DMA 模块复位控制', '0：模块处于复位状态', '1：模块正常工作'], (name, 'reset'))
    four_tokens(claim_text['uart_instances'], ['3 个通用异步收发器'], (name, 'UART scope'))
    four_tokens(claim_text['uart_dma'], ['UARTx_CR2.DMATX 为 1', 'UARTx_TDR', 'DMA_ISR.TCy', 'UARTx_ISR.TXBUSY'], (name, 'UART DMA'))
    four_tokens(claim_text['uart_layout'], ['UART1_BASE = 0x4001 3800', 'UART2_BASE = 0x4000 4400',
                'UART3_BASE = 0x4000 4800', 'UARTx_TDR', 'UARTx_BASE + 0x28'], (name, 'UART bases'))
    four_tokens(claim_text['uart_cr2'], ['7 DMATX RW', 'DMA 发送数据使能控制'], (name, 'DMATX'))
    four_tokens(claim_text['uart_tdr'], ['UARTx_TDR', 'Address offset: 0x28', '8:0 TDR WO'], (name, 'TDR'))
    tx = four_compact(claim_text['uart_byte_tx_example'])
    four_tokens(tx, ['SRAM', 'uint8_t SendBuff[100]', 'DMA_SRCADDR1 = ( uint32_t ) SendBuff',
                'DMA_DSTADDR1 = ( uint32_t ) &UART1_TDR', 'DMA_CSR1.SRCINC 为 1', 'DMA_CSR1.DSTINC 为 0',
                'DMA_CNT1 写入 0x10064', 'DMA_CSR1.SIZE 为 0', 'DMA_CSR1.TRANS 为 1', 'UART1_CR2.SOURCE 为 0'], (name, 'byte TX'))
    ordered = ['DMA_CSR1.EN 为 1', 'UART1_CR2.DMATX 为 1', 'DMA_ISR.TC1 变为 1',
               'UART1_CR2.DMATX 为 0', 'UART1_ISR.TXBUSY 变为 0']
    positions = [tx.index(four_compact(token)) for token in ordered]
    assert positions == sorted(positions), (name, 'EN/DMATX/completion order')
    irq = claim_text['irq']
    for symbol, number, manual_name in [('DMACH1', 9, 'DMA1'), ('DMACH23', 10, 'DMA23'), ('DMACH4', 11, 'DMA4')]:
        assert re.search(rf'(?m)^\s*{number + 16}\s+{number}\s+可配置\s+{manual_name}\b.*0x0000\s+00{4 * (number + 16):02X}', irq), (name, symbol)
    for symbol, number in [('UART1', 27), ('UART2', 28), ('UART3', 29)]:
        assert re.search(rf'(?m)^\s*{number + 16}\s+{number}\s+可配置\s+{symbol}\b', irq), (name, symbol)

    svd = ET.fromstring(four_bytes[family['svd_path']])
    peripherals = {p.findtext('name'): p for p in svd.findall('peripherals/peripheral')}
    for expected in family['svd_geometry']:
        peripheral = peripherals[expected['name']]
        assert int(peripheral.findtext('baseAddress'), 0) == expected['base']
        assert peripheral.get('derivedFrom') == expected['derived_from']
        assert [{'name': i.findtext('name'), 'number': int(i.findtext('value'), 0)}
                for i in peripheral.findall('interrupt')] == expected['interrupts']
        for key in ('registers', 'selected_registers'):
            if key in expected:
                registers = [four_svd_register(r) for r in peripheral.findall('registers/register')]
                if key == 'selected_registers':
                    selected = {r['name'] for r in expected[key]}
                    registers = [r for r in registers if r['name'] in selected]
                assert registers == expected[key], (name, expected['name'], key)
    dma = next(p for p in family['svd_geometry'] if p['name'] == 'DMA')
    assert dma['base'] == 0x40020000
    assert {r['name']: r['offset'] for r in dma['registers']} == expected_offsets
    for register in dma['registers']:
        fields = {f['name']: (f['bit_offset'], f['bit_size']) for f in register['fields']}
        assert fields == {f['name']: (f['bit_offset'], f['bit_size'])
                          for f in native['fieldset/' + register['name']]['fields']}, (name, register['name'])
        if register['name'] not in ('ISR', 'ICR'):
            channel_fieldset = re.sub(r'[1-4]$', '', register['name'])
            assert fields == {f['name']: (f['bit_offset'], f['bit_size'])
                              for f in channel_ir['fieldset/' + channel_fieldset]['fields']}
    for symbol in family['sdk_symbols']:
        lines = four_bytes[symbol['path']].decode().splitlines()
        assert lines[symbol['line'] - 1].strip() == symbol['text'].strip(), (name, symbol)
    sidecar = load(ROOT / family['dma_metadata_path'])
    assert sidecar['core_dma_channels'] == expected_channels
    assert sidecar['channel_details'] == family['physical_channels']
    assert len(family['physical_channels']) == 4
    for index, channel in enumerate(family['physical_channels']):
        assert channel['physical_channel'] == index + 1
        assert channel['name'] == f'DMA_CH{index + 1}'
        assert (channel['interrupt'], channel['interrupt_number']) == expected_channel_vectors[index]
        peripheral = peripherals[channel['register_peripheral']]
        assert int(peripheral.findtext('baseAddress'), 0) == 0x40020020 + 32 * index
        assert (peripheral.findtext('interrupt/name'), int(peripheral.findtext('interrupt/value'), 0)) == expected_channel_vectors[index]
    tx_requests = [r for r in sidecar['requests'] if r['peripheral'] in ('UART1', 'UART2', 'UART3') and r['signal'] == 'TX']
    assert tx_requests == family['qualified_tx_requests']
    assert [r for r in sidecar['requests'] if r not in tx_requests] == family['unqualified_inventory_only']
    assert [(r['peripheral'], r['request']) for r in tx_requests] == [('UART1', 1), ('UART2', 3), ('UART3', 5)]
    dma_header = next(path for path in four_bytes if path.endswith('/' + name.lower() + '_dma.h'))
    dma_lines = four_bytes[dma_header].decode().splitlines()
    for request in tx_requests:
        assert request['channels'] == [c['name'] for c in expected_channels]
        assert request['dma'] == 'DMA'
        assert request['sdk']['raw_value'] == request['request'] << 2
        four_tokens(manual[request['manual']['pdf_page_index']],
                    [request['manual']['binary'] + '：' + request['manual']['description']], (name, request['peripheral']))
        line = dma_lines[request['sdk']['line'] - 1]
        match = re.search(rf"#define\s+{request['sdk']['symbol']}\s+\(uint32_t\)\(0x([0-9a-f]+)UL\s*<<\s*2\)", line, re.I)
        assert match and int(match[1], 16) << 2 == request['sdk']['raw_value'], (name, line)
    clocks = load(ROOT / family['clock_metadata_path'])['peripherals']
    # The frozen ledger records clock facts, excluding only narrative notes and
    # consumer evidence links. Keep status, blockers and all scoped hardware
    # fields (including future fields) in the exact semantic comparison.
    clock_facts = [{k: v for k, v in c.items() if k not in ('notes', 'evidence')}
                   for c in clocks if c['name'] in ('DMA', 'GPIOA', 'UART1', 'UART2', 'UART3')]
    assert clock_facts == family['clocks']

seed = four['native_icr_seed']
command = load(ROOT / 'cw32-data/register-writes.yaml')['registers']['dma_cw32l031_v1']
assert len(command) == 1
command = command[0]
for key in ('block', 'register', 'fieldset', 'reset_value', 'write_noop', 'zero_to_clear_fields'):
    assert command[key] == seed[key]
assert command['block'] == 'DMA' and command['register'] == command['fieldset'] == 'ICR'
assert command['reset_value'] == command['write_noop'] == 0xffffffff
assert command['zero_to_clear_fields'] == [f'{kind}{i}' for i in range(1, 5) for kind in ('TC', 'TE')]
assert load(ROOT / 'cw32-data/data/register-writes/dma_cw32l031_v1.json') == {
    'schema_version': 1, 'registers': {'dma_cw32l031_v1': [command]}}
assert [v['physical_channel'] for v in seed['per_channel_clear_commands']] == [1, 2, 3, 4]
for values in seed['per_channel_clear_commands']:
    i = values['physical_channel']
    for key, bits in [('tc_only', [4 * (i - 1)]), ('te_only', [4 * (i - 1) + 1]), ('both', [4 * (i - 1), 4 * (i - 1) + 1])]:
        assert int(values[key], 16) == 0xffffffff ^ sum(1 << bit for bit in bits)
pac = (ROOT / 'cw32-metapac/src/peripherals/dma_cw32l031_v1.rs').read_text()
assert pac.count('impl regs::Icr {') == 1
icr_helpers = pac.split('impl regs::Icr {', 1)[1]
for helper in ('reset_value', 'write_noop'):
    assert re.search(rf'pub const fn {helper}\(\) -> Self\s*\{{\s*Self\(4294967295\)\s*\}}', icr_helpers)

parts = load(ROOT / 'cw32-data/parts.yaml')['parts']
assert {p['feature'] for p in parts if p['family'] in family_names} == exact_features
selected_count = 0
for part in four['exact_parts'] + four['generic_aliases']:
    chip = load(ROOT / part['metadata_path'])
    assert chip['name'].lower() == part['feature']
    assert chip['memory'] == part['memory']
    core = chip['cores'][0]
    assert core['dma_channels'] == part['dma_channels'] == expected_channels
    assert [p for p in core['peripherals'] if p['name'] == 'DMA' or p['name'].startswith('DMACHANNEL')] == part['dma_instances']
    vectors = [v for v in core['interrupts'] if v['name'] in expected_vectors]
    assert vectors == part['dma_uart_vectors']
    assert {v['name']: v['number'] for v in vectors} == expected_vectors
    if part['feature'] not in exact_features:
        assert part['memory'] == [] and not chip['packages']
        continue
    assert part['flash_interval'] == [0, 0x10000]
    assert part['sram_interval'] == [0x20000000, 0x20002000]
    assert part['memory'] == [[{'address': 0, 'kind': 'flash', 'name': 'FLASH', 'size': 65536},
                              {'address': 0x20000000, 'kind': 'ram', 'name': 'RAM', 'size': 8192}]]
    authored_part = next(p for p in parts if p['feature'] == part['feature'])
    assert authored_part['memory'] == part['memory'][0]
    assert authored_part['package'] == part['package']
    package = chip['packages'][0]
    assert len(chip['packages']) == 1 and package['package'] == part['package']
    bonded = {signal: pin['position'] for pin in package['pins'] for signal in pin['signals']}
    safe = []
    for uart in [p for p in core['peripherals'] if p['name'] in ('UART1', 'UART2', 'UART3')]:
        for pin in uart['pins']:
            if pin['signal'] == 'TX' and pin['pin'] in bonded and pin['pin'] not in ('PA13', 'PA14', 'PF3'):
                safe.append((uart['name'], pin['pin'], pin['af'], bonded[pin['pin']]))
    assert set(safe) == {(p['peripheral'], p['pin'], p['af'], p['package_pad']) for p in part['safe_tx_pin_inventory']}
    assert part['safe_uart1_tx_available'] == any(p[0] == 'UART1' for p in safe)
    f8 = part['feature'] in ('cw32l031f8p6', 'cw32l031f8u6')
    assert part['safe_uart1_tx_available'] != f8
    routes = part['selected_example_tx_routes']
    assert len(routes) == (2 if f8 else 3)
    assert {r['peripheral'] for r in routes} == ({'UART2', 'UART3'} if f8 else {'UART1', 'UART2', 'UART3'})
    family = next(f for f in four['families'] if f['family'] == chip['line'])
    datasheet = four_family_pages[family['family']]['datasheet']
    product_table = datasheet[family['datasheet_pages'][0]['pdf_page_index']]
    product_row = next(line for line in product_table.splitlines() if '外设' in line and part['datasheet_product'] in line)
    products = re.findall(r'CW32[A-Z0-9]+', product_row)
    assert products[part['datasheet_memory_column']] == part['datasheet_product']
    for label, size in [('FLASH', 64), ('SRAM', 8)]:
        row = next(line for line in product_table.splitlines() if label + '（K' in line)
        values = [int(n) for n in re.findall(r'\d+', row)]
        assert values[part['datasheet_memory_column']] == size
    for route in routes:
        selected_count += 1
        assert (route['peripheral'], route['pin'], route['af'], route['package_pad']) in safe
        uart = next(p for p in core['peripherals'] if p['name'] == route['peripheral'])
        assert [r for r in uart['dma_channels'] if r['signal'] == 'TX'] == [route['request']]
        assert route['tdr_address'] == uart['address'] + 0x28
        assert route['uart_irq'] == uart['interrupts']
        expected_channel = {'UART1': 'DMA_CH4', 'UART2': 'DMA_CH2', 'UART3': 'DMA_CH3'}[route['peripheral']]
        assert route['dma_channel'] == expected_channel
        assert route['dma_irq'] == ('DMACH4' if expected_channel == 'DMA_CH4' else 'DMACH23')
        assert route['datasheet_af_pdf_page_index'] == family['gpioa_af_table_pdf_page_index']
        for key in ('datasheet_af_pdf_page_index', 'datasheet_pad_pdf_page_index'):
            assert route[key] in {p['pdf_page_index'] for p in family['datasheet_pages']}
            pin = route['pin'][:2] + route['pin'][2:].zfill(2)
            assert pin in datasheet[route[key]], (part['feature'], route['pin'], key)
        pad_page = datasheet[route['datasheet_pad_pdf_page_index']]
        pad_row = next(line for line in pad_page.splitlines() if re.search(rf'\b{pin}\b', line))
        pads = pad_row.split(pin, 1)[0].split()
        assert pads[part['datasheet_pad_column']] == route['package_pad'], (part['feature'], route)
        af_page = datasheet[route['datasheet_af_pdf_page_index']]
        af_row = next(line for line in af_page.splitlines() if re.match(rf'\s*{pin}\b', line))
        # The selected PA6/AF2, PA8/AF2, PA9/AF1 and PA10/AF6 rows
        # have no empty columns before the selected function.
        assert af_row.split()[route['af']] == route['peripheral'] + '_TXD', (part['feature'], route)
assert selected_count == 31
print('PASS: four own native DMA manuals/SDKs, 54 locked source identities, native ICR seed/PAC, shared CH2/3 and CH4 topology, 11 exact SRAM parts, four empty generics and 31 TX routes; source facts only')

# L012 has distinct native registers, current counts and paired IRQs. Bind its
# originals separately from the reviewed corrections and generated projections.
l012 = load(ROOT / 'docs/l012-dma-owned-evidence.json')
assert l012['schema_version'] == 1
assert len(l012['families']) == 1
family = l012['families'][0]
assert family['family'] == 'CW32L012'
l012_features = {'cw32l012c8t6', 'cw32l012c8u6'}
assert len(l012['exact_parts']) == 2
assert {p['feature'] for p in l012['exact_parts']} == l012_features
assert len(l012['generic_aliases']) == 1
assert l012['generic_aliases'][0]['feature'] == 'cw32l012'
l012_pointers = {'/artifacts/6'} | {
    f'/artifacts/6/members/{i}' for i in (17, 18, 19, 25, 28, 38, 39, 47, 60)
} | {f'/artifacts/{i}{suffix}' for i in (24, 25, 42) for suffix in ('', '/text')}
assert len(l012['sources']) == 16
assert {s['source_index_pointer'] for s in l012['sources']} == l012_pointers
l012_bytes = {}
for source in l012['sources']:
    pointer = source['source_index_pointer']
    pin = four_index(pointer)
    artifact = lock['artifacts'][int(pointer.split('/')[2])]
    identity = source['identity']
    if pointer == '/artifacts/42':
        # These retained superseded identities are not current source inputs.
        # Reverse only their explicit historical key labels before the same
        # full-node equality; no original pin-history field is omitted.
        identity = json.loads(json.dumps(identity))
        assert len(identity['pin_history']) == 2
        for historical in identity['pin_history']:
            assert set(historical) == {
                'historical_path', 'historical_sha256', 'bytes', 'changed_utc', 'reason'
            }
            historical['path'] = historical.pop('historical_path')
            historical['sha256'] = historical.pop('historical_sha256')
    observed = {k: v for k, v in pin.items() if k != 'evidence'
                and not (k == 'members' and pointer == '/artifacts/6')}
    expected = {k: v for k, v in identity.items() if k != 'evidence'}
    for value in (observed, expected):
        if 'text' in value:
            value['text'] = {k: v for k, v in value['text'].items() if k != 'evidence'}
    # Optional consumer backlinks are the only mutable selected-node property.
    # In particular the EN manual's pin_history and legal provenance stay bound.
    assert observed == expected, source['source_ref']
    assert source['official_url'] == artifact['url']
    if '/members/' in pointer:
        assert source['source_ref'] == 'member:' + identity['path']
        assert source['archive_source_ref'] == artifact['id']
    elif pointer.endswith('/text'):
        assert source['source_ref'] == 'text:' + identity['path']
    elif not pointer.endswith('/text'):
        assert source['source_ref'] == artifact['id']
    assert identity['path'] not in l012_bytes
    raw = (SOURCES / identity['path']).read_bytes()
    assert len(raw) == identity['bytes'] and digest(raw) == identity['sha256'], identity['path']
    l012_bytes[identity['path']] = raw
    if '/members/' in pointer:
        payload = (SOURCES / artifact['path']).read_bytes()
        for member in identity['members']:
            with zipfile.ZipFile(io.BytesIO(payload)) as archive:
                payload = archive.read(member)
        assert payload == raw, identity['path']
assert len(l012_bytes) == 16

l012_pages = {}
for kind, page_delta in [('manual', 25), ('datasheet', 2), ('english_manual', 25)]:
    source = next(s for s in l012['sources'] if s['source_ref'] == family[kind + '_source_ref'])
    assert source['identity']['path'] == family[kind]
    assert source['identity']['text']['path'] == family[kind + '_text_path']
    pages = l012_bytes[family[kind + '_text_path']].decode().split('\f')
    records = family[kind + '_pages']
    assert len({p['pdf_page_index'] for p in records}) == len(records)
    for page in records:
        assert page['pdf_page_index'] == page['printed_page'] + page_delta
        assert page['pdf_viewer_page'] == page['pdf_page_index'] + 1
        assert digest(pages[page['pdf_page_index']].encode()) == page['extracted_page_utf8_sha256'], (kind, page)
    l012_pages[kind] = pages

manual = l012_pages['manual']
for claim, numbers in family['manual_page_claims'].items():
    assert numbers == [p['pdf_page_index'] for p in family['manual_pages'] if claim in p['claims']], claim
# Exact coordinates differ from the older four-family manuals.
assert {p['pdf_page_index'] for p in family['manual_pages']} >= {
    81, 82, 85, 86, 95, 96, 142, 143, 144, 145, 146, 147, 148,
    494, 499, 505, 507, 510, 512, 513,
}
assert {p['pdf_page_index'] for p in family['datasheet_pages']} >= {7, 35, 36, 38}
assert {p['pdf_page_index'] for p in family['english_manual_pages']} >= set(range(149, 160)) | {540, 545}
four_tokens(manual[145], ['13 TE RO', '12 TC RO', '11 RESTART RW', '10:8 STATUS RW',
            '000：初始状态', '001：传输错误，传输地址超出寻址范围', '010：传输错误，传输停止请求引起中止',
            '011：传输错误，访问传输来源地址出错', '100：传输错误，访问传输目的地址出错', '101：传输完成',
            '00：位宽为 8 比特，自增量为 1', '01：位宽为 16 比特，自增量为 2', '10：位宽为 32 比特，自增量为 4',
            '位宽配置仅对存储器（FLASH、RAM）有效', '0：批量传输（BULK）', '1：块传输（BLOCK）'], 'L012 CSR')
four_tokens(manual[144], ['15 DSTLOAD RW', '14 SRCLOAD RW', 'Reset value: 0xFFFF FFFF',
            '保留位，请保持默认值', 'W0：清除', 'W1：无功能'], 'L012 ICR/reload')
four_tokens(manual[146], ['y = 1~4', '7:2 HARDSRC RW', 'R0：传输已完成', 'R1：传输正在进行中',
            'W0：无功能', 'W1：立即触发该通道启动传输', '0：软件触发模式', '1：硬件触发模式'], 'L012 TRIG')
four_tokens(manual[147], ['19:16 REPEAT RW', '15:0 CNT RW', 'REPEAT 只可设置为 1',
            'REPEAT 不可设置为 0', 'CNT×REPEAT'], 'L012 configured count')
four_tokens(manual[148], ['Address offset: 0x14 + 32×y', '19:16 CREPEAT RO', '15:0 CCNT RO',
            '31:0 CSRCADDR RO', '31:0 CDSTADDR RO'], 'L012 current state')
four_tokens(l012_pages['english_manual'][154], ['13 TE RO', '12 TC RO', '10:8 STATUS RW',
            '101: Transfer completed', 'Bit width configuration is only valid for memory'], 'L012 EN CSR')
four_tokens(l012_pages['english_manual'][159], ['19:16 CREPEAT RO', '15:0 CCNT RO',
            '31:0 CSRCADDR RO', '31:0 CDSTADDR RO'], 'L012 EN current state')

l012_svd = ET.fromstring(l012_bytes[family['svd_path']])
assert l012_svd.findtext('name') == 'CW32L012'
l012_peripherals = {p.findtext('name'): p for p in l012_svd.findall('peripherals/peripheral')}
geometry = {p['name']: p for p in family['svd_geometry']}
assert len(geometry) == len(family['svd_geometry']) == 9
assert set(geometry) == {'DMA', 'SYSCTRL', 'UART1', 'UART2', 'UART3'} | {f'DMACHANNEL{i}' for i in range(1, 5)}
l012_registers = {}
for name, expected in geometry.items():
    peripheral = l012_peripherals[name]
    parent = ('DMACHANNEL1' if name in ('DMACHANNEL2', 'DMACHANNEL3', 'DMACHANNEL4')
              else 'UART1' if name in ('UART2', 'UART3') else None)
    assert peripheral.get('derivedFrom') == expected['derived_from'] == parent
    assert int(peripheral.findtext('baseAddress'), 0) == expected['base']
    assert [{'name': i.findtext('name'), 'number': int(i.findtext('value'), 0)}
            for i in peripheral.findall('interrupt')] == expected['interrupts']
    owner = l012_peripherals[parent] if parent else peripheral
    if parent:
        assert peripheral.find('registers') is None and owner.get('derivedFrom') is None
    observed = []
    for register in owner.findall('registers/register'):
        if name == 'SYSCTRL' and register.findtext('name') not in ('AHBEN', 'APBEN1', 'AHBRST', 'APBRST1'):
            continue
        if name.startswith('UART') and register.findtext('name') not in ('CR1', 'CR2', 'TDR', 'IER', 'ISR'):
            continue
        assert register.get('derivedFrom') is None
        fields = register.findall('fields/field')
        for field in fields:
            assert field.get('derivedFrom') is None
            assert {k for k in ('lsb', 'msb', 'bitOffset', 'bitWidth', 'bitRange')
                    if field.find(k) is not None} == {'lsb', 'msb'}
            assert field.findtext('access') in ('read-only', 'read-write', 'write-only')
        record = four_svd_register(register)
        record['access'] = register.findtext('access')
        assert record['access'] in ('read-only', 'read-write', 'write-only')
        observed.append(record)
    key = 'selected_registers' if name == 'SYSCTRL' or name.startswith('UART') else 'registers'
    assert observed == expected[key], ('L012 original SVD', name)
    assert len({r['name'] for r in observed}) == len(observed)
    l012_registers[name] = {r['name']: r for r in observed}

l012_offsets = dict(zip(('CSR', 'CNT', 'SRCADDR', 'DSTADDR', 'TRIG', 'CCNT', 'CSRCADDR', 'CDSTADDR'), range(0, 32, 4)))
l012_flags = {f'{kind}{i}': (4 * (i - 1) + bit, 1)
              for i in range(1, 5) for kind, bit in [('TC', 0), ('TE', 1)]}
l012_vectors = {'DMACH12': 9, 'DMACH34': 10, 'UART1': 27, 'UART2': 28, 'UART3': 29}
l012_channel_vectors = [('DMACH12', 9), ('DMACH12', 9), ('DMACH34', 10), ('DMACH34', 10)]
assert geometry['DMA']['base'] == 0x40020000
assert {r['name']: r['offset'] for r in l012_registers['DMA'].values()} == {
    'ISR': 0, 'ICR': 4, **{f'{r}{i}': 32 * i + offset for i in range(1, 5) for r, offset in l012_offsets.items()}}
for i in range(1, 5):
    name = f'DMACHANNEL{i}'
    assert geometry[name]['base'] == 0x40020000 + 32 * i
    vector, number = l012_channel_vectors[i - 1]
    assert geometry[name]['interrupts'] == [{'name': vector, 'number': number}]
    assert {r['name']: r['offset'] for r in l012_registers[name].values()} == l012_offsets

l012_ir = {}
for version, block, peripheral in [('dmachannel_cw32l012_v1', 'DMACHANNEL', 'DMACHANNEL1'),
                                   ('dma_cw32l012_v1', 'DMA', 'DMA')]:
    authored = load(ROOT / f'cw32-data/registers/{version}.yaml')
    generated = load(ROOT / f'cw32-data/data/registers/{version}.json')
    assert authored == generated == load(ROOT / f'cw32-data/data/registers/{version}.yaml')
    l012_ir[version] = generated
    assert {k for k in generated if k.startswith('block/')} == {'block/' + block}
    reuse = next(g for g in load(ROOT / 'cw32-data/register-reuse.yaml')['groups'] if g['canonical'] == version + '.yaml')
    assert reuse['source_versions'] == [version + '.yaml']
    assert digest(json.dumps(generated, sort_keys=True, separators=(',', ':')).encode()) == reuse['canonical_ir_sha256']
    update = next(u for u in l012['canonical_ir_updates'] if u['canonical'] == version + '.yaml')
    assert update['after_sha256'] == reuse['canonical_ir_sha256']
    items = generated['block/' + block]['items']
    originals = l012_registers[peripheral]
    assert len(items) == len(originals) and {r['name'] for r in items} == set(originals)
    assert {k for k in generated if k.startswith('fieldset/')} == {'fieldset/' + r for r in originals}
    for item in items:
        original = originals[item['name']]
        assert item['byte_offset'] == original['offset']
        assert item['fieldset'] == item['name'] and 'array' not in item
        assert item.get('bit_size', 32) == 32
        fields = generated['fieldset/' + item['fieldset']]['fields']
        assert len(fields) == len(original['fields'])
        assert {f['name']: (f['bit_offset'], f['bit_size']) for f in fields} == {
            f['name']: (f['bit_offset'], f['bit_size']) for f in original['fields']}
        corrected = item['name'].startswith('CCNT')
        expected_access = 'Read' if corrected else {'read-only': 'Read', 'read-write': 'ReadWrite'}[original['access']]
        assert item.get('access', 'ReadWrite') == expected_access
        if corrected:
            assert original['access'] == 'read-write'
            assert {f['name']: f['access'] for f in original['fields']} == {'CCNT': 'read-write', 'CREPEAT': 'read-write'}
        if re.fullmatch(r'CSR[1-4]?', item['name']):
            access = {f['name']: f['access'] for f in original['fields']}
            assert access['TC'] == access['TE'] == 'read-only' and access['STATUS'] == 'read-write'

channel_ir = l012_ir['dmachannel_cw32l012_v1']
native = l012_ir['dma_cw32l012_v1']
for fieldset in ('ISR', 'ICR'):
    assert {f['name']: (f['bit_offset'], f['bit_size']) for f in native['fieldset/' + fieldset]['fields']} == l012_flags
for field, (bit, width) in l012_flags.items():
    assert re.search(rf'(?m)^\s*{bit}\s+{field}\s+R1W0\b', manual[144]), field
assert {f['name']: (f['bit_offset'], f['bit_size']) for f in channel_ir['fieldset/CSR']['fields']} == {
    'EN': (0, 1), 'TCIE': (1, 1), 'TEIE': (2, 1), 'TRANS': (3, 1), 'SRCINC': (4, 1), 'DSTINC': (5, 1),
    'SIZE': (6, 2), 'STATUS': (8, 3), 'RESTART': (11, 1), 'TC': (12, 1), 'TE': (13, 1), 'SRCLOAD': (14, 1), 'DSTLOAD': (15, 1)}
expected_enums = {
    'TransferMode': {'bit_size': 1, 'variants': {'BULK': 0, 'BLOCK': 1}},
    'Width': {'bit_size': 2, 'variants': {'BYTE': 0, 'HALF_WORD': 1, 'WORD': 2}},
    'Status': {'bit_size': 3, 'variants': {'INITIAL': 0, 'ADDRESS_OUT_OF_RANGE': 1, 'STOPPED': 2,
                                       'SOURCE_ERROR': 3, 'DESTINATION_ERROR': 4, 'COMPLETE': 5}},
    'Trigger': {'bit_size': 1, 'variants': {'SOFTWARE': 0, 'HARDWARE': 1}},
}
assert l012['native_enums'] == expected_enums
assert {k for k in channel_ir if k.startswith('enum/')} == {'enum/' + n for n in expected_enums}
assert not any(k.startswith('enum/') for k in native)
for name, expected in expected_enums.items():
    enum = channel_ir['enum/' + name]
    assert enum['bit_size'] == expected['bit_size']
    assert len(enum['variants']) == len(expected['variants'])
    assert {v['name']: v['value'] for v in enum['variants']} == expected['variants']
associations = {('CSR', 'TRANS'): 'TransferMode', ('CSR', 'SIZE'): 'Width',
                ('CSR', 'STATUS'): 'Status', ('TRIG', 'TYPE'): 'Trigger'}
assert {(key.split('/')[1], f['name']): f['enum'] for key, value in channel_ir.items()
        if key.startswith('fieldset/') for f in value['fields'] if 'enum' in f} == associations
assert {tuple(a) for a in l012['field_associations']} == {(f'fieldset/{s}', f, e) for (s, f), e in associations.items()}
overrides = [o for o in load(ROOT / 'cw32-data/inputs/cw32l012.yaml')['register_overrides']
             if o['block'] in ('DMA', 'DMACHANNEL')]
assert overrides == l012['register_access_corrections'] and len(overrides) == 5
assert {(o['block'], o['register'], o['expected_access'], o['access']) for o in overrides} == {
    ('DMACHANNEL', 'CCNT', 'ReadWrite', 'Read'),
    *{('DMA', f'CCNT{i}', 'ReadWrite', 'Read') for i in range(1, 5)},
}
manual_source = next(s for s in l012['sources'] if s['source_ref'] == family['manual_source_ref'])
for override in overrides:
    four_tokens(override['evidence'], ['8.8.8', '123', '149', manual_source['identity']['sha256'],
                'SVD', 'CMSIS', 'RW', 'RO'], ('L012 access correction', override['register']))

assert set(l012['field_access_restrictions']) == set(l012_ir)
for version, expected in l012['field_access_restrictions'].items():
    restrictions = load(ROOT / 'cw32-data/field-access.yaml')['registers'][version]
    assert restrictions == expected
    block, suffixes = ('DMA', ('1', '2', '3', '4')) if version == 'dma_cw32l012_v1' else ('DMACHANNEL', ('',))
    assert len(restrictions) == 2 * len(suffixes)
    assert {(r['block'], r['register'], r['fieldset'], r['field'], r['bit_offset'], r['bit_size'])
            for r in restrictions} == {(block, 'CSR' + s, 'CSR' + s, f, bit, 1)
                                      for s in suffixes for f, bit in [('TC', 12), ('TE', 13)]}
    assert load(ROOT / f'cw32-data/data/field-access/{version}.json') == {
        'schema_version': 1, 'registers': {version: restrictions}}
    pac = (ROOT / f'cw32-metapac/src/peripherals/{version}.rs').read_text()
    for suffix in suffixes:
        csr = re.search(rf'\bimpl Csr{suffix} \{{(.*?)\n    \}}', pac, re.S)
        assert csr, (version, suffix)
        for field in ('tc', 'te'):
            assert len(re.findall(rf'pub const fn {field}\(&self\) -> bool', csr[1])) == 1
            assert not re.search(rf'\bfn set_{field}\(', csr[1])
        assert len(re.findall(r'pub const fn set_status\(', csr[1])) == 1
        register = 'ccnt' + suffix
        assert len(re.findall(rf'pub const fn {register}\(self\) -> crate::common::Reg<regs::Ccnt{suffix}, crate::common::R>', pac)) == 1
    if version == 'dmachannel_cw32l012_v1':
        for (fieldset, field), enum in associations.items():
            field = 'type_' if field == 'TYPE' else field.lower()
            assert len(re.findall(rf'pub const fn {field}\(&self\) -> super::vals::{enum}\b', pac)) == 1
            assert len(re.findall(rf'pub const fn set_{field}\(&mut self, val: super::vals::{enum}\)', pac)) == 1

seed = l012['native_icr_seed']
commands = load(ROOT / 'cw32-data/register-writes.yaml')['registers']['dma_cw32l012_v1']
assert len(commands) == 1
command = commands[0]
assert command == {k: v for k, v in seed.items() if k not in ('register_version', 'per_channel_clear_commands')}
assert seed['register_version'] == 'dma_cw32l012_v1'
assert command['block'] == 'DMA' and command['register'] == command['fieldset'] == 'ICR'
assert command['reset_value'] == command['write_noop'] == 0xffffffff
assert command['zero_to_clear_fields'] == [f'{kind}{i}' for i in range(1, 5) for kind in ('TC', 'TE')]
assert load(ROOT / 'cw32-data/data/register-writes/dma_cw32l012_v1.json') == {
    'schema_version': 1, 'registers': {'dma_cw32l012_v1': commands}}
assert [v['physical_channel'] for v in seed['per_channel_clear_commands']] == [1, 2, 3, 4]
for values in seed['per_channel_clear_commands']:
    i = values['physical_channel']
    for key, bits in [('tc_only', [4 * (i - 1)]), ('te_only', [4 * (i - 1) + 1]), ('both', [4 * (i - 1), 4 * (i - 1) + 1])]:
        assert int(values[key], 16) == 0xffffffff ^ sum(1 << bit for bit in bits)
pac = (ROOT / 'cw32-metapac/src/peripherals/dma_cw32l012_v1.rs').read_text()
assert pac.count('impl regs::Icr {') == 1
icr_helpers = pac.split('impl regs::Icr {', 1)[1]
for helper in ('reset_value', 'write_noop'):
    assert re.search(rf'pub const fn {helper}\(\) -> Self\s*\{{\s*Self\(4294967295\)\s*\}}', icr_helpers)
assert re.search(r'impl Default for Icr \{.*?fn default\(\) -> Icr\s*\{\s*Icr\(0\)', pac, re.S)

dma_header_path = 'cw32l012/Libraries/inc/cw32l012_dma.h'
dma_header = l012_bytes[dma_header_path].decode()
# The own SDK uses literal first values followed by implicit successors. This
# finite extraction never treats the Stage85 shifted request macros as L012.
l012_sdk_enums = {}
for body, name in re.findall(r'typedef\s+enum\s*\{([^{}]*)\}\s*(DMA_\w+TypeDef)\s*;', dma_header, re.S):
    value = -1
    values = {}
    body = re.sub(r'//[^\n]*|/\*.*?\*/', '', body, flags=re.S)
    for entry in body.split(','):
        if not entry.strip():
            continue
        match = re.fullmatch(r'\s*(DMA_\w+)(?:\s*=\s*(0x[0-9a-fA-F]+|[0-9]+))?\s*', entry)
        assert match, (name, entry)
        value = int(match[2], 0) if match[2] else value + 1
        assert match[1] not in values
        values[match[1]] = value
    assert name not in l012_sdk_enums
    l012_sdk_enums[name] = values
for name, values in {
    'DMA_ChannelTypeDef': {f'DMA_CHANNEL_{i}': i - 1 for i in range(1, 5)},
    'DMA_DataSizeTypeDef': {'DMA_DATA_SIZE_8BITS': 0, 'DMA_DATA_SIZE_16BITS': 1, 'DMA_DATA_SIZE_32BITS': 2},
    'DMA_ModeTypeDef': {'DMA_MODE_BULK': 0, 'DMA_MODE_BLOCK': 1},
    'DMA_TriggerTypeDef': {'DMA_TRIGGER_SOFTWARE': 0, 'DMA_TRIGGER_HARDWARE': 1},
    'DMA_StatusTypeDef': {'DMA_STATUS_IDLE': 0, 'DMA_STATUS_ERROR_ADDR': 1, 'DMA_STATUS_ERROR_STOP': 2,
                          'DMA_STATUS_ERROR_SRC': 3, 'DMA_STATUS_ERROR_DST': 4, 'DMA_STATUS_COMPLETE': 5},
}.items():
    assert l012_sdk_enums[name] == values

for symbol in family['sdk_symbols']:
    lines = l012_bytes[symbol['path']].decode().splitlines()
    assert lines[symbol['line'] - 1].strip() == symbol['text'].strip(), symbol
cmsis = l012_bytes['cw32l012/Libraries/inc/cw32l012.h'].decode()
for register in ['CCNT', 'CCNT1', 'CCNT2', 'CCNT3', 'CCNT4']:
    assert len(re.findall(rf'__IOM\s+uint32_t\s+{register}\s*;', cmsis)) == 1, register
assert len(re.findall(r'__IOM\s+uint32_t\s+CREPEAT\s*:\s*4\s*;', cmsis)) == 5
assert len(re.findall(r'__IOM\s+uint32_t\s+CCNT\s*:\s*16\s*;', cmsis)) == 5
for name, bit in [('TCIE', 1), ('TEIE', 2)]:
    assert re.search(rf'#define\s+DMA_CSR1_{name}_Pos\s+\({bit}UL\)', cmsis)
for field, (bit, width) in l012_flags.items():
    assert re.search(rf'#define\s+DMA_ICR_{field}_Pos\s+\({bit}UL\)', cmsis)
four_tokens(dma_header, ['#define DMA_INTERRUPT_TC DMA_CSR1_TCIE_Msk',
            '#define DMA_INTERRUPT_TE DMA_CSR1_TEIE_Msk'], 'L012 SDK interrupt mask domain')
dma_source = l012_bytes['cw32l012/Libraries/src/cw32l012_dma.c'].decode()
four_tokens(dma_source, ['*DMA_ICR &= ~(interrupt_type << channel);',
            '*DMA_CNTy(channel) = (transfer_size & 0xFFFF);'], 'L012 rejected SDK helpers')
assert len(l012['conflicts']) == 4
assert {c['id'] for c in l012['conflicts']} == {
    'current-count-access', 'mixed-csr-access-loss', 'sdk-clear-helper', 'sdk-config-channel-repeat'}

sidecar = load(ROOT / family['dma_metadata_path'])
assert sidecar['family'] == 'CW32L012'
assert sidecar['core_dma_channels'] == expected_channels
assert sidecar['channel_details'] == family['physical_channels']
assert len(family['physical_channels']) == 4
for index, channel in enumerate(family['physical_channels']):
    assert channel['physical_channel'] == index + 1
    assert channel['name'] == f'DMA_CH{index + 1}'
    assert channel['register_peripheral'] == f'DMACHANNEL{index + 1}'
    assert (channel['interrupt'], channel['interrupt_number']) == l012_channel_vectors[index]
requests = sidecar['requests']
tx_requests = [r for r in requests if r['peripheral'] in ('UART1', 'UART2', 'UART3') and r['signal'] == 'TX']
assert tx_requests == family['qualified_tx_requests']
assert [r for r in requests if r not in tx_requests] == family['unqualified_inventory_only']
assert [(r['peripheral'], r['request']) for r in tx_requests] == [('UART1', 1), ('UART2', 3), ('UART3', 5)]
assert len(requests) == 64 and [r['request'] for r in requests] == list(range(64))
assert l012_sdk_enums['DMA_TriggerSourceTypeDef'] == {r['sdk']['symbol']: r['request'] for r in requests}
for request in requests:
    assert request['channels'] == [c['name'] for c in expected_channels] and request['dma'] == 'DMA'
    assert request['sdk']['encoding'] == 'unshifted_enum'
    assert request['sdk']['raw_value'] == request['request']
    assert request['manual']['pdf_page_index'] == 146 and request['manual']['printed_page'] == 121
    assert int(request['manual']['binary'], 2) == request['request']
    four_tokens(manual[146], [request['manual']['binary'] + '：' + request['manual']['description']], request['sdk']['symbol'])
    assert re.match(r'\s*' + re.escape(request['sdk']['symbol']) + r'\b', dma_header.splitlines()[request['sdk']['line'] - 1])
assert sidecar['request_register'] == {'peripheral_kind': 'dmachannel', 'register': 'TRIG', 'field': 'HARDSRC', 'bit_offset': 2, 'bit_size': 6}
assert sidecar['excluded_sdk_requests'] == []
assert sidecar['peripheral_dma_channels'] == {
    name: [{'signal': r['signal'], 'dma': r['dma'], 'request': r['request']} for r in requests if r['peripheral'] == name]
    for name in {r['peripheral'] for r in requests}}

clock_names = {'DMA', 'GPIOA', 'UART1', 'UART2', 'UART3'}
clocks = load(ROOT / family['clock_metadata_path'])['peripherals']
clock_facts = [{k: v for k, v in c.items() if k not in ('notes', 'evidence')}
               for c in clocks if c['name'] in clock_names]
assert clock_facts == family['clocks'] and len(clock_facts) == 5
assert {c['name'] for c in clock_facts} == clock_names
sysctrl_ir = load(ROOT / 'cw32-data/data/registers/sysctrl_cw32l012_v1.json')
for register, offset, page in [('AHBEN', 0x30, 81), ('APBEN1', 0x38, 82), ('AHBRST', 0x40, 85), ('APBRST1', 0x48, 86)]:
    original = l012_registers['SYSCTRL'][register]
    assert original['offset'] == offset and original['access'] == 'read-write'
    item = next(r for r in sysctrl_ir['block/SYSCTRL']['items'] if r['name'] == register)
    assert item['byte_offset'] == offset and item.get('access', 'ReadWrite') == 'ReadWrite'
    assert {f['name']: (f['bit_offset'], f['bit_size']) for f in sysctrl_ir['fieldset/' + register]['fields']} == {
        f['name']: (f['bit_offset'], f['bit_size']) for f in original['fields']}
    fields = {f['name']: f for f in original['fields']}
    if register in ('AHBEN', 'APBEN1'):
        assert (fields['KEY']['bit_offset'], fields['KEY']['bit_size'], fields['KEY']['access']) == (16, 16, 'write-only')
        four_tokens(manual[page], ['31:16 KEY WO', 'KEY 为 0x5A5A', '0：禁止', '1：使能'], register)
    else:
        assert 'KEY' not in fields
        four_tokens(manual[page], ['0：模块处于复位状态', '1：模块正常工作'], register)
    for name, bit in ({'DMA': 0, 'GPIOA': 4} if register.startswith('AHB') else {'UART1': 3, 'UART2': 4, 'UART3': 8}).items():
        assert (fields[name]['bit_offset'], fields[name]['bit_size'], fields[name]['access']) == (bit, 1, 'read-write')
        four_tokens(manual[page], [f'{bit} {name} RW'], (register, name))
for clock in clock_facts:
    name = clock['name']
    uart = name.startswith('UART')
    enable, reset = ('APBEN1', 'APBRST1') if uart else ('AHBEN', 'AHBRST')
    assert clock['enable'] == {'peripheral': 'SYSCTRL', 'register': enable, 'field': name}
    assert clock['reset'] == {'peripheral': 'SYSCTRL', 'register': reset, 'field': name}
    assert clock['enable_active_value'] == 1 and clock['reset_asserted_value'] == 0
    assert clock['enable_write_key'] == {'field': {'peripheral': 'SYSCTRL', 'register': enable, 'field': 'KEY'}, 'value': 0x5a5a}
    assert clock['bus_clock'] == ('PCLK' if uart else 'HCLK')
    if uart:
        assert clock['status'] == 'partial'
        assert clock['kernel_clock'] == {'mux': {'peripheral': name, 'register': 'CR1', 'field': 'SOURCE'}}
        assert clock['kernel_sources'] == ['PCLK', 'LSE', 'LSI']
    if name == 'DMA':
        assert clock['shared_enable_group'] == clock['shared_reset_group'] == 'DMA-DMACHANNEL1-DMACHANNEL2-DMACHANNEL3-DMACHANNEL4'

uart_ir = load(ROOT / 'cw32-data/data/registers/uart_cw32l012_v1.json')
for i, base in [(1, 0x40000c00), (2, 0x40001000), (3, 0x40002000)]:
    name = f'UART{i}'
    assert geometry[name]['base'] == base
    assert geometry[name]['interrupts'] == [{'name': name, 'number': 26 + i}]
    assert set(l012_registers[name]) == {'CR1', 'CR2', 'TDR', 'IER', 'ISR'}
    for register, original in l012_registers[name].items():
        item = next(r for r in uart_ir['block/UART']['items'] if r['name'] == register)
        assert item['byte_offset'] == original['offset']
        assert item.get('access', 'ReadWrite') == {'read-only': 'Read', 'write-only': 'Write', 'read-write': 'ReadWrite'}[original['access']]
        fields = uart_ir['fieldset/' + item['fieldset']]['fields']
        assert len(fields) == len(original['fields'])
        assert {f['name']: (f['bit_offset'], f['bit_size']) for f in fields} == {
            f['name']: (f['bit_offset'], f['bit_size']) for f in original['fields']}
tdr = l012_registers['UART1']['TDR']
assert tdr['offset'] == 0x28 and tdr['access'] == 'write-only'
assert {f['name']: (f['bit_offset'], f['bit_size'], f['access']) for f in tdr['fields']} == {
    'TDR': (0, 9, 'write-only'), 'IDLE': (9, 1, 'write-only'), 'BREAK': (10, 1, 'write-only')}
four_tokens(manual[507], ['Address offset: 0x04', '7 DMATX RW'], 'L012 UART CR2')
four_tokens(manual[510], ['UARTx_TDR', 'Address offset: 0x28', '8:0 TDR WO'], 'L012 UART TDR')
four_tokens(manual[512], ['Address offset: 0x1C', '14 TXBUSY RO'], 'L012 wire completion')
four_tokens(manual[505], ['13:12 SOURCE RW', '00：UCLK 来自 PCLK', '01：UCLK 来自 PCLK',
            '10：UCLK 来自 LSE', '11：UCLK 来自 LSI'], 'L012 local UART clock')
assert {v['name']: v['value'] for v in uart_ir['enum/Source']['variants']} == {'PCLK': 0, 'PCLK_ALT': 1, 'LSE': 2, 'LSI': 3}
four_tokens(manual[494], ['UARTx_CR2.DMATX 为 1', 'UARTx_TDR', 'DMA_ISR.TCy', 'UARTx_ISR.TXBUSY'], 'L012 UART DMA')
tx = four_compact(manual[499])
four_tokens(tx, ['PA09', 'SRAM', 'uint8_t SendBuff[100]', 'DMA_SRCADDR1 = (uint32_t) SendBuff',
            'DMA_DSTADDR1 = (uint32_t) &UART1_TDR', 'DMA_CSR1.SRCINC 为 1', 'DMA_CSR1.DSTINC 为 0',
            'DMA_CNT1 写入 0x10064', 'DMA_CSR1.SIZE 为 0', 'DMA_CSR1.TRANS 为 1',
            'DMA_TRIG1. HARDSRC 为 0x01'], 'L012 SRAM-byte TX example')
ordered = ['DMA_CSR1.EN 为 1', 'UART1_CR2.DMATX 为 1', 'DMA_ISR.TC1 变为 1',
           'UART1_CR2.DMATX 为 0', 'UART1_ISR.TXBUSY 变为 0']
positions = [tx.index(four_compact(token)) for token in ordered]
assert positions == sorted(positions)
for number, name in [(9, 'DMA1/2'), (10, 'DMA3/4')]:
    assert re.search(rf'(?m)^\s*{number + 16}\s+{number}\s+可配置\s+{name}\b', manual[95]), name
for number, name in [(27, 'UART1'), (28, 'UART2'), (29, 'UART3')]:
    assert re.search(rf'(?m)^\s*{number + 16}\s+{number}\s+可配置\s+{name}\b', manual[96]), name

pdsc_path = next(path for path in l012_bytes if path.endswith('.pdsc'))
pdsc = ET.fromstring(l012_bytes[pdsc_path])
devices = pdsc.findall(".//device[@Dname='CW32L012C8']")
assert len(devices) == 1
assert {m.get('name'): (int(m.get('start'), 0), int(m.get('size'), 0), m.get('access'))
        for m in devices[0].findall('memory')} == {
    'IROM1': (0, 0x10000, 'rx'), 'IRAM1': (0x20000000, 0x2000, 'rw')}
datasheet = l012_pages['datasheet']
four_tokens(datasheet[7], ['CW32L012C8T6', 'CW32L012C8U6', 'FLASH 64KB', 'SRAM 8KB', 'LQFP48', 'QFN48'], 'L012 exact parts')
parts = load(ROOT / 'cw32-data/parts.yaml')['parts']
assert {p['feature'] for p in parts if p['family'] == 'CW32L012'} == l012_features
pinout = load(ROOT / 'cw32-data/pinouts/cw32l012.yaml')
af_routes = load(ROOT / 'cw32-data/af/cw32l012-serial.yaml')['routes']
selected_count = 0
for part in l012['exact_parts'] + l012['generic_aliases']:
    chip = load(ROOT / part['metadata_path'])
    assert chip['name'].lower() == part['feature'] and chip['line'] == 'CW32L012'
    assert chip['memory'] == part['memory']
    assert len(chip['cores']) == 1
    core = chip['cores'][0]
    assert core['dma_channels'] == part['dma_channels'] == expected_channels
    instances = [p for p in core['peripherals'] if p['name'] == 'DMA' or p['name'].startswith('DMACHANNEL')]
    assert instances == part['dma_instances'] and len(instances) == 5
    for instance in instances:
        name = instance['name']
        assert name in geometry and instance['address'] == geometry[name]['base']
        assert instance['registers'] == {'block': 'DMA' if name == 'DMA' else 'DMACHANNEL',
                                         'kind': 'dma' if name == 'DMA' else 'dmachannel', 'version': 'cw32l012_v1'}
        assert instance['interrupts'] == [{'interrupt': i['name'], 'signal': 'GLOBAL'} for i in geometry[name]['interrupts']]
        assert instance['rcc'] == instances[0]['rcc'] and instance['rcc_control'] == instances[0]['rcc_control']
    vectors = [v for v in core['interrupts'] if v['name'].startswith('DMA') or v['name'].startswith('UART')]
    assert vectors == part['dma_uart_vectors']
    assert len(vectors) == 5 and {v['name']: v['number'] for v in vectors} == l012_vectors
    assert [v for v in core['interrupts'] if v['number'] == 11] == [{'name': 'CORDIC', 'number': 11}]
    for name in clock_names:
        peripheral = next(p for p in core['peripherals'] if p['name'] == name)
        clock = next(c for c in clock_facts if c['name'] == name)
        control = peripheral['rcc_control']
        assert control['controller'] == 'SYSCTRL' and control['bus_clock'] == clock['bus_clock']
        for key in ('enable', 'reset'):
            assert control[key] == {k: v for k, v in clock[key].items() if k != 'peripheral'}
        assert control['enable_active_value'] is True and control['reset_asserted_value'] is False
        assert control['enable_write_key'] == {'field': {'register': clock['enable']['register'], 'field': 'KEY'}, 'value': 0x5a5a}
        assert 'reset_write_key' not in control
    if part['feature'] == 'cw32l012':
        assert part['memory'] == [] and chip['packages'] == []
        continue
    assert part['flash_interval'] == [0, 0x10000]
    assert part['sram_interval'] == [0x20000000, 0x20002000]
    assert part['memory'] == [[{'address': 0, 'kind': 'flash', 'name': 'FLASH', 'size': 65536},
                              {'address': 0x20000000, 'kind': 'ram', 'name': 'RAM', 'size': 8192}]]
    authored_part = next(p for p in parts if p['feature'] == part['feature'])
    assert authored_part['memory'] == part['memory'][0] and authored_part['package'] == part['package']
    assert part['package'] == ('LQFP48' if part['feature'] == 'cw32l012c8t6' else 'QFN48')
    assert len(chip['packages']) == 1
    package = chip['packages'][0]
    assert package['package'] == part['package']
    bonded = {signal: pin['position'] for pin in package['pins'] for signal in pin['signals']}
    assert part['datasheet_product'] == chip['name']
    assert part['datasheet_product_pdf_page_index'] == 7
    assert part['datasheet_pad_column'] == (0 if part['package'] == 'LQFP48' else 1)
    assert part['safe_uart1_tx_available'] is True
    routes = part['selected_example_tx_routes']
    assert len(routes) == 3
    assert {(r['peripheral'], r['pin'], r['af'], r['package_pad']) for r in routes} == {
        ('UART1', 'PA9', 1, '30'), ('UART2', 'PA7', 1, '17'), ('UART3', 'PA8', 1, '29')}
    for route in routes:
        selected_count += 1
        uart = next(p for p in core['peripherals'] if p['name'] == route['peripheral'])
        assert uart['registers'] == {'block': 'UART', 'kind': 'uart', 'version': 'cw32l012_v1'}
        assert uart['address'] == geometry[uart['name']]['base']
        assert {'signal': 'TX', 'pin': route['pin'], 'af': 1} in uart['pins']
        assert bonded[route['pin']] == route['package_pad']
        assert [r for r in uart['dma_channels'] if r['signal'] == 'TX'] == [route['request']]
        assert route['request'] == {'dma': 'DMA', 'request': {'UART1': 1, 'UART2': 3, 'UART3': 5}[uart['name']], 'signal': 'TX'}
        assert route['tdr_address'] == uart['address'] + 0x28
        assert route['uart_irq'] == uart['interrupts'] == [{'interrupt': uart['name'], 'signal': 'GLOBAL'}]
        assert (route['dma_channel'], route['dma_irq']) == {
            'UART1': ('DMA_CH1', 'DMACH12'), 'UART2': ('DMA_CH2', 'DMACH12'), 'UART3': ('DMA_CH4', 'DMACH34')}[uart['name']]
        pin = route['pin'][:2] + route['pin'][2:].zfill(2)
        assert route['datasheet_af_pdf_page_index'] == family['gpioa_af_table_pdf_page_index'] == 38
        af_row = next(line for line in datasheet[38].splitlines() if re.match(rf'\s*{pin}\b', line))
        assert af_row.split()[1] == uart['name'] + '_TXD'
        page = route['datasheet_pad_pdf_page_index']
        assert page == (35 if pin == 'PA07' else 36)
        pad_row = next(line for line in datasheet[page].splitlines() if re.search(rf'\b{pin}\b', line))
        assert pad_row.split()[:6] == [route['package_pad'], route['package_pad'], pin, 'I/O', 'TTa', '-']
        rows = [r for r in pinout['table_rows'] if route['pin'] in r['signals']]
        assert len(rows) == 1 and rows[0]['positions'][part['package']] == route['package_pad']
        assert rows[0]['source_name'] == pin and rows[0]['pin_type'] == 'I/O' and rows[0]['pdf_page_index'] == page
        authored_routes = [r for r in af_routes if (r['peripheral'], r['signal'], r['pin'], r['af']) == (uart['name'], 'TX', route['pin'], 1)]
        assert len(authored_routes) == 1
        authored_route = authored_routes[0]
        gpio_line = l012_bytes['cw32l012/Libraries/inc/cw32l012_gpio.h'].decode().splitlines()[authored_route['source_line'] - 1]
        assert authored_route['source_macro'] == f'{pin}_AFx_{uart["name"]}TXD'
        assert re.search(r'#define\s+' + re.escape(authored_route['source_macro']) + r'\b', gpio_line)
        four_tokens(gpio_line, [f'CW_GPIOA->{authored_route["gpio_register"]}_f.PIN{int(pin[2:])} = 1'], (part['feature'], pin))
assert selected_count == 6
print('PASS: L012 16 locked own identities and nested bytes, native enums/CCNT corrections/ten RO fields/ICR PAC, paired IRQs, 64 source request facts, two exact 8 KiB parts and six selected TX routes; no HAL execution')

# Five independently qualified classic UART RX endpoints. These are finite
# current source/data/PAC checks, not a runtime simulation or acceptance receipt.
import tomllib

classic_path = 'docs/classic-uart-dma-rx-evidence.json'
classic_checker = 'tests/verify_dma_owned_evidence.py'
classic = load(ROOT / classic_path)
classic_families = {'CW32F020', 'CW32L031', 'CW32L052', 'CW32R031', 'CW32W031'}
classic_features = {
    'cw32f020c6u7', 'cw32f020k6u7', 'cw32f020f6u7',
    'cw32l031c8t6', 'cw32l031c8u6', 'cw32l031f8p6', 'cw32l031f8u6',
    'cw32l031k8u6', 'cw32l031k8v6', 'cw32l052c8t6', 'cw32l052r8s6',
    'cw32l052r8t6', 'cw32r031c8u6', 'cw32w031r8u6',
}
assert classic['schema_version'] == 1
assert set(classic['families']) == set(classic['scope']['families']) == classic_families
assert classic['scope']['exact_part_count'] == len(classic['exact_parts']) == 14
assert {p['feature'] for p in classic['exact_parts']} == classic_features
assert classic['excluded_safe_pins'] == ['PA13', 'PA14', 'PF3']
assert 'L012 DMA RX' in classic['scope']['excluded']
assert classic['shared_facts']['uart_fields'] == {'DMARX': [6, 1], 'DMATX': [7, 1],
        'RC': [2, 1], 'FE': [3, 1], 'PE': [4, 1], 'RDR': [0, 9]}
assert classic['shared_facts']['dma_fields'] == {'STATUS': [8, 3], 'REPEAT': [16, 4],
        'CNT': [0, 16], 'HARDSRC': [2, 6]}
assert classic['shared_facts']['example_cnt'] == (1 << 16) | 100
assert classic['shared_facts']['dma_icr_noop'] == 0xffffffff
assert classic['shared_facts']['error_statuses'] == [1, 2, 3, 4]
assert classic['shared_facts']['cpu_mmio_bits'] == 32
assert classic['shared_facts']['silent_overwrite_possible'] is True
# Each family consumes exactly one SDK archive, two PDFs, their texts and six
# selected SDK members. The existing source index gives child nodes no IDs.
classic_pointers = set()
for sdk, manual, datasheet, members in [
    (2, 18, 40, [2, 17, 31, 9, 7, 0]),
    (7, 27, 26, [2, 17, 35, 9, 7, 0]),
    (8, 29, 28, [2, 19, 39, 9, 7, 0]),
    (10, 33, 32, [2, 17, 32, 9, 7, 0]),
    (11, 35, 34, [2, 17, 32, 9, 7, 0]),
]:
    classic_pointers.add(f'/artifacts/{sdk}')
    classic_pointers.update(f'/artifacts/{sdk}/members/{i}' for i in members)
    classic_pointers.update(f'/artifacts/{i}{suffix}' for i in (manual, datasheet) for suffix in ('', '/text'))
assert len(classic['sources']) == len(classic_pointers) == 55
assert {s['index_pointer'] for s in classic['sources'].values()} == classic_pointers
contract = classic['source_index_append_plan']
assert contract['new_consumer'] == classic_path and contract['checker_extension'] == classic_checker
assert len(contract['bound_source_refs']) == 55
assert set(contract['bound_source_refs']) == set(classic['sources'])
assert {ref for family in classic['families'].values() for ref in family['sources'].values()} == set(classic['sources'])
classic_bytes = {}
for ref, source in classic['sources'].items():
    pointer = source['index_pointer']
    pin = four_index(pointer)
    artifact = lock['artifacts'][int(pointer.split('/')[2])]
    observed = {k: v for k, v in pin.items() if k != 'evidence'
                and not (k == 'members' and ref.startswith('vendor:'))}
    if 'text' in observed:
        observed['text'] = {k: v for k, v in observed['text'].items() if k != 'evidence'}
    assert observed == source['identity'], ref
    assert {k: source[k] for k in ('path', 'sha256', 'bytes')} == {
        k: pin[k] for k in ('path', 'sha256', 'bytes')}
    assert source['official_url'] == artifact['url']
    assert pin['evidence'].count(classic_path) == pin['evidence'].count(classic_checker) == 1
    if '/members/' in pointer:
        assert ref == 'member:' + pin['path'] and 'id' not in pin
        assert source['archive_member_chain'] == pin['members']
    elif pointer.endswith('/text'):
        assert ref == 'text:' + pin['path'] and 'id' not in pin
    else:
        assert ref == artifact['id']
    raw = (SOURCES / pin['path']).read_bytes()
    assert len(raw) == source['bytes'] and digest(raw) == source['sha256'], ref
    assert pin['path'] not in classic_bytes
    classic_bytes[pin['path']] = raw
    if '/members/' in pointer:
        payload = (SOURCES / artifact['path']).read_bytes()
        for member in source['archive_member_chain']:
            with zipfile.ZipFile(io.BytesIO(payload)) as archive:
                payload = archive.read(member)
        assert payload == raw, ref
classic_reverse = set()
for index, artifact in enumerate(lock['artifacts']):
    nodes = [(f'/artifacts/{index}', artifact)]
    nodes += [(f'/artifacts/{index}/members/{i}', member) for i, member in enumerate(artifact.get('members', []))]
    if 'text' in artifact:
        nodes.append((f'/artifacts/{index}/text', artifact['text']))
    for pointer, node in nodes:
        if classic_path in node.get('evidence', []):
            assert node['evidence'].count(classic_path) == 1
            classic_reverse.add(pointer)
        assert node.get('evidence', []).count(classic_checker) <= 1
assert classic_reverse == classic_pointers

classic_pages = {}
classic_channels = {}
classic_uart_ir = {}
page_count = 0
status_access_views = 0
assert len(classic['source_discrepancies']) == 1
status_discrepancy = classic['source_discrepancies'][0]
assert status_discrepancy['id'] == 'dma-status-access'
assert set(status_discrepancy['families']) == classic_families
assert (status_discrepancy['own_svd_access'], status_discrepancy['own_manual_access'],
        status_discrepancy['current_ir_access'], status_discrepancy['field'],
        status_discrepancy['selected_register_views']) == ('read-only', 'RW', 'ReadWrite', 'STATUS', 23)
for name, family in classic['families'].items():
    refs = family['sources']
    assert len(refs) == 11
    inputs = {kind: classic['sources'][ref] for kind, ref in refs.items()}
    assert inputs['sdk']['identity']['family'] == name
    for kind in ('cmsis_header', 'uart_header', 'uart_driver', 'gpio_header', 'dma_header', 'svd'):
        assert name.lower() in inputs[kind]['path'].lower()
    for kind in ('manual', 'datasheet'):
        assert inputs[kind]['identity']['text']['path'] == inputs[kind + '_text']['path']
        pages = classic_bytes[inputs[kind + '_text']['path']].decode().split('\f')
        classic_pages[name, kind] = pages
        hashes = family[kind + '_page_utf8_sha256']
        for number, sha in hashes.items():
            assert digest(pages[int(number)].encode()) == sha, (name, kind, number)
            page_count += 1
    claims = family['manual_claim_pages']
    assert {int(p) for p in family['manual_page_utf8_sha256']} == {p for pages in claims.values() for p in pages}
    manual = classic_pages[name, 'manual']
    claim_text = {claim: '\n'.join(manual[p] for p in pages) for claim, pages in claims.items()}
    assert claims['byte_rx_example'] == [{'CW32F020': 288, 'CW32L031': 345, 'CW32L052': 382,
                                         'CW32R031': 348, 'CW32W031': 348}[name]]
    rx = four_compact(claim_text['byte_rx_example'])
    four_tokens(rx, ['UART1_RXD', 'PA09', 'SRAM', 'uint8_t RecvBuff[100]',
                'DMA_SRCADDR2 = (uint32_t) &UART1_RDR', 'DMA_DSTADDR2 = (uint32_t) RecvBuff',
                'DMA_CSR2.SRCINC 为 0', 'DMA_CSR2.DSTINC 为 1', 'DMA_CNT2 写入 0x10064',
                'DMA_CSR2.SIZE 为 0', 'DMA_CSR2.TRANS 为 1', 'DMA_TRIG2.Type 为 1',
                'DMA_TRIG2.HardSrc 为 0x00'], (name, 'own byte RX'))
    ordered = ['DMA_CSR2.EN 为 1', 'UART1_CR2.DMARX 为 1', 'DMA_ISR.TC2 变为 1', 'UART1_CR2.DMARX 为 0']
    positions = [rx.index(four_compact(token)) for token in ordered]
    assert positions == sorted(positions), (name, 'RX launch and terminal order')
    four_tokens(claim_text['receive_errors'], ['UARTx_ISR.RC', 'UARTx_ISR.PE', 'UARTx_ISR.FE',
                '新接收的数据会覆盖 UARTx_RDR 寄存器', '造成数据丢失'], (name, 'receive errors and overwrite'))
    four_tokens(claim_text['terminal'], ['数据传输全部正确完成', 'TEIE', 'TCIE'], (name, 'terminal'))
    csr = claim_text['dma_csr' if name == 'CW32F020' else 'csr']
    four_tokens(csr, ['10:8 STATUS RW', '000：初始状态', '001：传输错误，传输地址超出寻址范围',
                '010：传输错误，传输停止请求引起中止', '011：传输错误，访问传输来源地址出错',
                '100：传输错误，访问传输目的地址出错', '101：传输完成',
                '00：8 比特', '0：批量传输（BULK）', '1：块传输（BLOCK）'], (name, 'STATUS'))
    trig = claim_text['request_and_trigger' if name == 'CW32F020' else 'trig']
    four_tokens(trig, ['7:2 HARDSRC RW', 'R0：传输已完成', 'W0：无功能',
                '0：软件触发模式', '1：硬件触发模式'], (name, 'TRIG'))
    count = claim_text['repeat_count' if name == 'CW32F020' else 'count_and_addresses']
    four_tokens(count, ['19:16 REPEAT RW', '15:0 CNT RW'], (name, 'count geometry'))
    four_tokens(claim_text['sram'], ['8KB', 'SRAM', '0x2000 0000'], (name, 'SRAM'))
    mmio = claim_text['cpu_mmio_width' if name == 'CW32F020' else 'mmio_width_and_gates']
    four_tokens(mmio, ['32bit'], (name, 'ordinary CPU register width'))
    four_tokens(claim_text['uart_layout'], ['UART1_BASE = 0x4001 3800', 'UART2_BASE = 0x4000 4400',
                'UART3_BASE = 0x4000 4800'], (name, 'own UART instances'))
    four_tokens(claim_text['uart_cr2'], ['6 DMARX RW', '7 DMATX RW'], (name, 'request gates'))
    four_tokens(claim_text['uart_rdr'], ['Address offset: 0x24', '8:0 RDR RO'], (name, 'RDR'))
    for register, access in [('ier', 'RW'), ('isr', 'RO'), ('icr', 'R1W0')]:
        four_tokens(claim_text['uart_' + register], [f'{bit} {field} {access}' for field, bit in [('RC', 2), ('FE', 3), ('PE', 4)]], (name, register))
    seed = 255 if name == 'CW32F020' else 4095
    assert family['uart_icr_noop'] == seed
    four_tokens(claim_text['uart_icr'], ['Address offset: 0x20', f'Reset value: 0x{seed:08X}',
                '保留位，请保持默认值', 'W0：清除', 'W1：无功能'], (name, 'UART clear seed'))
    driver_lines = classic_bytes[inputs['uart_driver']['path']].decode().splitlines()
    assert len(family['sdk_rc_no_read_clear_lines']) == len(family['sdk_rc_no_read_clear_observed']) == 2
    for line, observed in zip(family['sdk_rc_no_read_clear_lines'], family['sdk_rc_no_read_clear_observed']):
        assert driver_lines[line - 1].strip() == observed
        four_tokens(observed, ['只能软件清除', '不能通过读UARTx_RDR寄存器来清除'], (name, 'RC no-read-clear'))
    cmsis = classic_bytes[inputs['cmsis_header']['path']].decode()
    channels = 2 if name == 'CW32F020' else 4
    expected_channels = [{'channel': i - 1, 'dma': 'DMA', 'name': f'DMA_CH{i}'} for i in range(1, channels + 1)]
    classic_channels[name] = expected_channels
    vectors = [('DMACH1', 9), ('DMACH2', 10)] if channels == 2 else [('DMACH1', 9), ('DMACH23', 10), ('DMACH23', 10), ('DMACH4', 11)]
    irq_map = dict(vectors + [('UART1', 27), ('UART2', 28), ('UART3', 29)])
    assert family['cmsis_irqs'] == {irq + '_IRQn': number for irq, number in irq_map.items()}
    for irq, number in family['cmsis_irqs'].items():
        assert re.search(rf'\b{irq}\s*=\s*{number}\b', cmsis), (name, irq)
    for irq, number in irq_map.items():
        manual_name = irq.replace('DMACH', 'DMA')
        assert re.search(rf'(?m)^\s*{number + 16}\s+{number}\s+可配置\s+{manual_name}\b', claim_text['irq']), (name, irq)
    svd = ET.fromstring(classic_bytes[inputs['svd']['path']])
    peripherals = {p.findtext('name'): p for p in svd.findall('peripherals/peripheral')}
    assert {p for p in peripherals if p.startswith('DMACHANNEL')} == {f'DMACHANNEL{i}' for i in range(1, channels + 1)}
    for peripheral_name, expected in family['svd_geometry'].items():
        peripheral = peripherals[peripheral_name]
        assert int(peripheral.findtext('baseAddress'), 0) == int(expected['base'], 0)
        assert peripheral.get('derivedFrom') == expected.get('derived_from')
        owner = peripherals[expected['derived_from']] if 'derived_from' in expected else peripheral
        if 'derived_from' in expected:
            assert peripheral.find('registers') is None and owner.get('derivedFrom') is None
        registers = {r.findtext('name'): r for r in owner.findall('registers/register')}
        for register, offset in expected.get('register_offsets', {}).items():
            assert int(registers[register].findtext('addressOffset'), 0) == offset
        if peripheral_name.startswith('UART'):
            assert [(i.findtext('name'), int(i.findtext('value'), 0)) for i in peripheral.findall('interrupt')] == [(peripheral_name, 26 + int(peripheral_name[-1]))]
        elif peripheral_name.startswith('DMACHANNEL'):
            assert [(i.findtext('name'), int(i.findtext('value'), 0)) for i in peripheral.findall('interrupt')] == [vectors[int(peripheral_name[-1]) - 1]]
    uart_version = 'uart_' + family['uart_register_version']
    dma_version = 'dma_cw32f020_v1' if channels == 2 else 'dma_cw32l031_v1'
    for version, block, peripheral_name in [(uart_version, 'UART', 'UART1'), (dma_version, 'DMA', 'DMA'), ('dmachannel_v1', 'DMACHANNEL', 'DMACHANNEL1')]:
        ir = load(ROOT / f'cw32-data/data/registers/{version}.json')
        assert ir == load(ROOT / f'cw32-data/registers/{version}.yaml')
        original_registers = {r.findtext('name'): r for r in peripherals[peripheral_name].findall('registers/register')}
        own = {register: four_svd_register(r) for register, r in original_registers.items()}
        items = {r['name']: r for r in ir['block/' + block]['items']}
        selected = {'CR1', 'CR2', 'IER', 'ISR', 'ICR', 'RDR', 'TDR'} if block == 'UART' else set(own)
        if block != 'UART':
            assert set(items) == set(own)
        pac = (ROOT / f'cw32-metapac/src/peripherals/{version}.rs').read_text()
        for register in selected:
            item = items[register]
            assert item['byte_offset'] == own[register]['offset']
            access_names = {'read-only': 'Read', 'write-only': 'Write', 'read-write': 'ReadWrite'}
            register_access = original_registers[register].findtext('access')
            assert item.get('access', 'ReadWrite') == access_names[register_access], (name, register, 'access')
            fields = {f['name']: (f['bit_offset'], f['bit_size']) for f in ir['fieldset/' + item['fieldset']]['fields']}
            assert fields == {f['name']: (f['bit_offset'], f['bit_size']) for f in own[register]['fields']}, (name, version, register)
            for field in ir['fieldset/' + item['fieldset']]['fields']:
                original = next(f for f in own[register]['fields'] if f['name'] == field['name'])
                current_access = field.get('access', item.get('access', 'ReadWrite'))
                if block in ('DMA', 'DMACHANNEL') and re.fullmatch(r'CSR[1-4]?', register) and field['name'] == 'STATUS':
                    # All five own manuals say STATUS RW; their SVDs say RO.
                    # Preserve the existing manual-qualified metadata, while
                    # binding exactly this discrepancy in all 23 selected views.
                    assert original['access'] == status_discrepancy['own_svd_access']
                    assert current_access == status_discrepancy['current_ir_access']
                    four_tokens(csr, ['10:8 STATUS ' + status_discrepancy['own_manual_access']], (name, register, 'STATUS access'))
                    status_access_views += 1
                else:
                    assert current_access == access_names[original['access'] or register_access], (name, register, field['name'], 'access')
            # PAC address accessors are generated facts, not HAL behavior.
            accessor = re.search(rf'pub const fn {register.lower()}\(self\) -> ([^\n]+)\s*\{{\s*unsafe \{{ crate::common::Reg::from_ptr\(self.ptr.wrapping_add\((0x[0-9a-f]+)usize\)', pac)
            assert accessor and int(accessor[2], 16) == item['byte_offset'], (version, register)
            pac_access = {'read-only': 'R', 'write-only': 'W', 'read-write': 'RW'}[register_access]
            handle = re.search(r'crate::common::Reg<regs::(\w+), crate::common::(\w+)>', accessor[1])
            assert handle and handle[2] == pac_access, (version, register, 'PAC access')
            assert re.search(rf'pub struct {handle[1]}\(pub u32\);', pac), (version, register, '32-bit MMIO')
        if block == 'UART':
            classic_uart_ir[name] = ir
            assert [f['name'] for f in own['ISR']['fields']] == family['own_isr_fields']
            assert set(family['own_isr_fields']) == {'TXE', 'TC', 'RC', 'FE', 'PE', 'MATCH', 'CTS', 'CTSLV', 'TXBUSY'} | (set() if channels == 2 else {'TIMOV', 'BAUD', 'RXBRK'})
            for field in family['own_isr_fields']:
                bit = next(f['bit_offset'] for f in own['ISR']['fields'] if f['name'] == field)
                four_tokens(claim_text['uart_isr'], [f'{bit} {field} RO'], (name, 'full own ISR'))
            assert not {'ORE', 'NE'} & set(family['own_isr_fields'])
            assert family['error_fields'] == {'PE': [4, 1], 'FE': [3, 1]}
            assert 'crate::common::R>' in re.search(r'pub const fn rdr\(self\) -> ([^\n]+)', pac)[1]
        if block in ('UART', 'DMA'):
            commands = load(ROOT / 'cw32-data/register-writes.yaml')['registers'][version]
            assert len(commands) == 1
            command = commands[0]
            no_op = seed if block == 'UART' else 0xffffffff
            assert command['reset_value'] == command['write_noop'] == no_op
            flags = {f['name'] for f in own['ICR']['fields']} - {'RFU'}
            assert set(command['zero_to_clear_fields']) == flags
            assert load(ROOT / f'cw32-data/data/register-writes/{version}.json') == {'schema_version': 1, 'registers': {version: commands}}
            helpers = pac.split('impl regs::Icr {', 1)[1]
            for helper in ('reset_value', 'write_noop'):
                assert re.search(rf'pub const fn {helper}\(\) -> Self\s*\{{\s*Self\({no_op}\)\s*\}}', helpers)
            if block == 'DMA':
                expected_flags = {f'{kind}{i}': (4 * (i - 1) + bit, 1) for i in range(1, channels + 1) for kind, bit in [('TC', 0), ('TE', 1)]}
                assert {f['name']: (f['bit_offset'], f['bit_size']) for f in own['ICR']['fields']} == expected_flags
                icr = claim_text['dma_icr' if name == 'CW32F020' else 'icr']
                four_tokens(icr, ['Reset value: 0xFFFF FFFF', 'W0：清除', 'W1：无功能'], (name, 'DMA ICR'))
                for field, (bit, _) in expected_flags.items():
                    four_tokens(icr, [f'{bit} {field} R1W0'], (name, field))
    sidecar = load(ROOT / f'cw32-data/dma/{name.lower()}.yaml')
    assert sidecar['core_dma_channels'] == expected_channels
    assert len(sidecar['channel_details']) == channels
    for i, channel in enumerate(sidecar['channel_details'], 1):
        assert channel['physical_channel'] == i and channel['name'] == f'DMA_CH{i}'
        assert channel['register_peripheral'] == f'DMACHANNEL{i}'
        assert (channel['interrupt'], channel['interrupt_number']) == vectors[i - 1]
    requests = [r for r in sidecar['requests'] if r['peripheral'] in ('UART1', 'UART2', 'UART3') and r['signal'] in ('RX', 'TX')]
    assert [{k: v for k, v in r.items() if k not in ('dma', 'evidence')} for r in requests] == family['uart_requests']
    assert [(r['peripheral'], r['signal'], r['request']) for r in requests] == [(f'UART{i}', signal, 2 * (i - 1) + offset) for i in range(1, 4) for signal, offset in [('RX', 0), ('TX', 1)]]
    dma_lines = classic_bytes[inputs['dma_header']['path']].decode().splitlines()
    for request in requests:
        assert request['dma'] == 'DMA' and request['channels'] == [c['name'] for c in expected_channels]
        assert request['sdk']['encoding'] == 'shifted_left_2_define'
        assert request['sdk']['raw_value'] == request['request'] << 2
        assert int(request['manual']['binary'], 2) == request['request']
        assert request['manual']['printed_page'] == request['manual']['pdf_page_index']
        four_tokens(manual[request['manual']['pdf_page_index']], [request['manual']['binary'] + '：' + request['manual']['description']], (name, request['sdk']['symbol']))
        line = dma_lines[request['sdk']['line'] - 1]
        match = re.search(rf"#define\s+{request['sdk']['symbol']}\s+\(uint32_t\)\(0x([0-9a-f]+)UL\s*<<\s*2\)", line, re.I)
        assert match and int(match[1], 16) == request['request'], (name, line)
    af = load(ROOT / family['af_path'])
    pinout = load(ROOT / family['pinout_path'])
    assert af['source']['sdk_url'] == inputs['sdk']['official_url']
    assert af['source']['sdk_sha256'] == inputs['sdk']['sha256']
    assert af['source']['header_sha256'] == inputs['gpio_header']['sha256']
    for source in (af['datasheet'], pinout['source']):
        assert source['source_ref'] == refs['datasheet']
        assert source['url'] == inputs['datasheet']['official_url'] and source['sha256'] == inputs['datasheet']['sha256']
assert page_count == 140
assert status_access_views == status_discrepancy['selected_register_views'] == 23

parts = load(ROOT / 'cw32-data/parts.yaml')['parts']
assert {p['feature'] for p in parts if p['family'] in classic_families} == classic_features
route_count = pair_count = 0
for part in classic['exact_parts']:
    feature = part['feature']
    authored = next(p for p in parts if p['feature'] == feature)
    name = authored['family']
    family = classic['families'][name]
    chip = load(ROOT / f'cw32-data/data/chips/{feature.upper()}.json')
    assert chip['line'] == name and chip['name'] == feature.upper()
    memory = [{'address': 0, 'kind': 'flash', 'name': 'FLASH', 'size': 32768 if name == 'CW32F020' else 65536},
              {'address': 0x20000000, 'kind': 'ram', 'name': 'RAM', 'size': 8192}]
    assert chip['memory'] == [part['memory']] == [authored['memory']] == [memory]
    assert authored['package'] == part['package']
    assert len(chip['cores']) == len(chip['packages']) == 1
    core = chip['cores'][0]
    package = chip['packages'][0]
    assert package['package'] == part['package']
    assert core['dma_channels'] == classic_channels[name]
    dma_instances = [p for p in core['peripherals'] if p['name'].startswith('DMACHANNEL')]
    assert len(dma_instances) == len(classic_channels[name])
    for instance in dma_instances:
        number = int(instance['name'][10:])
        assert instance['address'] == 0x40020000 + 32 * number
        assert instance['registers'] == {'block': 'DMACHANNEL', 'kind': 'dmachannel', 'version': 'v1'}
        irq = 'DMACH' + str(number) if name == 'CW32F020' or number in (1, 4) else 'DMACH23'
        assert instance['interrupts'] == [{'interrupt': irq, 'signal': 'GLOBAL'}]
    af = load(ROOT / family['af_path'])
    pinout = load(ROOT / family['pinout_path'])
    own_package = next(p for p in pinout['packages'] if feature.upper() in p['documented_orderable_parts'])
    assert own_package['package'] == part['package']
    assert package['pins'] == own_package['pins']
    assert own_package['flash_bytes'] == memory[0]['size'] and own_package['ram_bytes'] == 8192
    bonded = {signal: p['position'] for p in package['pins'] for signal in p['signals']}
    datasheet = classic_pages[name, 'datasheet']
    product_page = datasheet[min(map(int, family['datasheet_page_utf8_sha256']))]
    product_row = next(line for line in product_page.splitlines() if '外设' in line and name in line)
    products = re.findall(r'CW32[A-Z0-9]+', product_row)
    product = next(p for p in products if feature.upper().startswith(p))
    for label, size in [('FLASH', memory[0]['size'] // 1024), ('SRAM', 8)]:
        row = next(line for line in product_page.splitlines() if label + '（K' in line)
        assert [int(n) for n in re.findall(r'\d+', row)][products.index(product)] == size
    assert set(part['uarts']) == {'UART1', 'UART2', 'UART3'}
    for uart_name, routes in part['uarts'].items():
        uart = next(p for p in core['peripherals'] if p['name'] == uart_name)
        assert uart['registers'] == {'block': 'UART', 'kind': 'uart', 'version': family['uart_register_version']}
        assert uart['address'] == {'UART1': 0x40013800, 'UART2': 0x40004400, 'UART3': 0x40004800}[uart_name]
        assert uart['interrupts'] == [{'interrupt': uart_name, 'signal': 'GLOBAL'}]
        assert {'name': uart_name, 'number': 26 + int(uart_name[-1])} in core['interrupts']
        for signal in ('RX', 'TX'):
            expected_request = {'dma': 'DMA', 'request': 2 * (int(uart_name[-1]) - 1) + (signal == 'TX'), 'signal': signal}
            assert [r for r in uart['dma_channels'] if r['signal'] == signal] == [expected_request]
            current = [p for p in uart['pins'] if p['signal'] == signal and p['pin'] not in classic['excluded_safe_pins']]
            projected = sorted(f"{p['pin']}/AF{p['af']}@{bonded[p['pin']]}" for p in current)
            assert routes[signal] == projected and len(set(projected)) == len(projected), (feature, uart_name, signal)
            authored_routes = [p for p in af['routes'] if p['peripheral'] == uart_name and p['signal'] == signal
                               and p['pin'] in bonded and p['pin'] not in classic['excluded_safe_pins']]
            assert projected == sorted(f"{p['pin']}/AF{p['af']}@{bonded[p['pin']]}" for p in authored_routes)
            route_count += len(projected)
            gpio_lines = classic_bytes[classic['sources'][family['sources']['gpio_header']]['path']].decode().splitlines()
            for route in authored_routes:
                pin = route['pin']; af_number = route['af']
                assert 1 <= af_number <= 7
                pin_rows = [row for row in pinout['table_rows'] if pin in row['signals']]
                assert len(pin_rows) == 1
                row = pin_rows[0]
                page = routes['bonded_pad_source_pages'][pin]
                assert page == row['pdf_page_index'] and str(page) in family['datasheet_page_utf8_sha256']
                assert row['positions'][own_package['table_column']] == bonded[pin]
                source_pin = pin[:2] + pin[2:].zfill(2)
                pad_row = next(line for line in datasheet[page].splitlines() if re.search(rf'\b{source_pin}\b', line))
                pads = pad_row.split(source_pin, 1)[0].split()
                assert pads[pinout['table_columns'].index(own_package['table_column'])] == bonded[pin], (feature, pin)
                if route['source_macro'] is not None:
                    line = gpio_lines[route['source_line'] - 1]
                    assert re.search(r'#define\s+' + re.escape(route['source_macro']) + r'\b', line)
                    four_tokens(line, [f"CW_GPIO{pin[1]}->{route['gpio_register']}_f.AFR{int(pin[2:])} = {af_number}"], (name, pin, signal))
                else:
                    assert (name, uart_name, signal, pin, af_number) == ('CW32W031', 'UART3', 'RX', 'PA2', 5)
                    assert route['source_kind'] == 'datasheet' and route['source_line'] is None
                    cell = route['datasheet_cell']
                    assert cell == {'pin': 'PA2', 'af': 5, 'function': 'UART3_RXD', 'table': '5-3', 'pdf_page': 31, 'printed_page': 30}
                    page_text = datasheet[30]
                    header = next(line for line in page_text.splitlines() if '功能 5' in line)
                    af_row = next(line for line in page_text.splitlines() if re.match(r'\s*PA02\b', line))
                    columns = [(int(m[1]), m.start() + len(m[0]) / 2) for m in re.finditer(r'功能\s+(\d)', header)]
                    match = re.search(r'UART3_RXD', af_row)
                    assert min(columns, key=lambda col: abs(col[1] - (match.start() + match.end()) / 2))[0] == 5
        assert set(routes['bonded_pad_source_pages']) == {r.split('/')[0] for signal in ('TX', 'RX') for r in routes[signal]}
        pairs = {(tx, rx) for tx in routes['TX'] for rx in routes['RX'] if tx.split('/')[0] != rx.split('/')[0]}
        assert len(pairs) == routes['distinct_pin_pair_count']
        pair_count += len(pairs)
assert (route_count, pair_count) == (459, 1449)
edge = next(p for p in classic['exact_parts'] if p['feature'] == 'cw32l031f8u6')['uarts']['UART1']
assert edge['TX'] == [] and [r.split('@')[0] for r in edge['RX']] == ['PA9/AF2', 'PC15/AF6']
for feature in ('cw32f020f6u7', 'cw32l031f8p6'):
    uart = next(p for p in classic['exact_parts'] if p['feature'] == feature)['uarts']['UART1']
    assert uart['TX'] == uart['RX'] == [] and uart['distinct_pin_pair_count'] == 0
for name in classic_families:
    generic = load(ROOT / f'cw32-data/data/chips/{name}.json')
    assert generic['memory'] == generic['packages'] == []
    assert generic['cores'][0]['dma_channels'] == classic_channels[name]

# Manifest/capability declarations describe the advertised surface only. Actual
# ARM links and implementation ownership review remain separate acceptance work.
manifest = tomllib.loads((ROOT / 'examples/uart-dma/Cargo.toml').read_text())
assert manifest['package']['name'] == 'cw32-uart-dma-example'
old_features = {'cw32f030c8t7', 'cw32a030c8t7', 'cw32l083rbt6', 'cw32l083rcs6', 'cw32l083rct6', 'cw32l083mct6', 'cw32l083vct6'}
assert {f for f in manifest['features'] if f.startswith('cw32')} == classic_features | old_features
for feature in classic_features | old_features:
    assert {f'embassy-cw32/{feature}', f'cw32-metapac/{feature}'} <= set(manifest['features'][feature])
assert (ROOT / 'examples/uart-dma/src/main.rs').is_file()
assert (ROOT / 'examples/uart-dma/src/bin/receive_only.rs').is_file()
assert manifest['package'].get('autobins', True)
assert manifest['bin'] == [{'name': 'shared_vector', 'path': 'src/bin/shared_vector.rs', 'required-features': ['l083']}]
for dependency in ('embassy-executor', 'embassy-futures'):
    assert manifest['dependencies'][dependency]['rev'] == 'f16efeffe37581092ec184718e6fdb1620393214'
capabilities = load(ROOT / 'ci/hal-capabilities.yaml')
for kind in ('uart', 'dma'):
    mode = next(m for m in capabilities['functions'][kind]['modes'] if m['id'] == 'safe_peripheral_dma')
    assert mode['status'] == 'experimental_partial'
    assert set(mode['implemented_families']) == classic_families | {'CW32F030', 'CW32A030', 'CW32L083', 'CW32L012'}
    for name in classic_families:
        override = next(o for o in mode['overrides'] if name in o['families'])
        assert 'RX' in override['scope'] and 'combined' in override['scope']
        assert 'MissingMemoryMetadata' in ' '.join(override['limitations'])
    l012_override = next(o for o in mode['overrides'] if o['families'] == ['CW32L012'])
    assert 'TX only' in l012_override['scope'] and 'No L012 DMA RX/combined UART' in ' '.join(l012_override['limitations'])
print('PASS: classic UART RX: 55 exact own-source nodes/member chains, 140 pages, full ISR/typed ICR PAC and native two/four-channel requests, fourteen exact SRAM parts, 459 safe routes/1449 distinct-pin pairs and empty generics; source/declaration facts only')
