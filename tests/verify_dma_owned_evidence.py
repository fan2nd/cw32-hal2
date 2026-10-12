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
