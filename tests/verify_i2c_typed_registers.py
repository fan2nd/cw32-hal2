#!/usr/bin/env python3
"""Audit own-manual I2C field/enumeration provenance and PAC command seeds.

This checks source documents and generated data/PAC only. It contains no HAL
register model, fake MMIO engine, compile fixture, or firmware execution.
"""
from functools import lru_cache
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
from urllib.parse import urlparse
import xml.etree.ElementTree as ET
import yaml

ROOT = Path(__file__).resolve().parents[1]
SOURCES = Path(os.environ.get('CW32_SOURCES', str(ROOT.parent / 'cw32-sources')))
EVIDENCE = json.loads((ROOT / 'docs/i2c-typed-register-evidence.json').read_text())


def digest(data):
    return hashlib.sha256(data).hexdigest()


def pages(fact, name):
    path = SOURCES / fact['manual']['filename']
    result = []
    for page in fact[name]:
        number = str(page['pdf_page_1_based'])
        data = subprocess.check_output(['pdftotext', '-f', number, '-l', number,
                                        '-layout', str(path), '-'])
        assert digest(data) == page['extract_sha256'], (path.name, number)
        result.append(data.decode())
    return '\n'.join(result)


def field(ir, register, name):
    return next(f for f in ir[f'fieldset/{register}']['fields'] if f['name'] == name)


def flat(text):
    return re.sub(r'\s+', '', text)


# This extension checks current selected vendor sources and authored/generated
# metadata. Review receipts and historical production snapshots are not inputs.
CLASSIC_FAMILIES = {'CW32F030', 'CW32A030', 'CW32F002', 'CW32F003', 'CW32F020',
                    'CW32L031', 'CW32R031', 'CW32W031', 'CW32L052', 'CW32L083',
                    'CW32L010', 'CW32L011'}
TYPED_EVIDENCE = 'docs/i2c-typed-register-evidence.json'
TYPED_CHECKER = 'tests/verify_i2c_typed_registers.py'


@lru_cache(maxsize=None)
def current_json(path):
    return json.loads((ROOT / path).read_text())


@lru_cache(maxsize=None)
def current_yaml(path):
    return yaml.safe_load((ROOT / path).read_text())


def at_pointer(value, pointer):
    for key in pointer.strip('/').split('/'):
        key = key.replace('~1', '/').replace('~0', '~')
        value = value[int(key)] if isinstance(value, list) else value[key]
    return value


def named(entries):
    result = {item['name']: item for item in entries}
    assert len(result) == len(entries), 'duplicate metadata name'
    return result


@lru_cache(maxsize=None)
def source_bytes(path, sha256, length):
    source = SOURCES / path
    with source.open('rb') as stream:
        actual = hashlib.file_digest(stream, 'sha256').hexdigest()
    assert source.stat().st_size == length and actual == sha256, path
    # Archive/PDF provenance needs identity checks, not retained binary copies.
    return source.read_bytes() if source.suffix.lower() in {'.txt', '.h', '.svd', '.pdsc', '.c'} else b''


def selected_source(binding):
    index = current_json('sources/evidence-sources.json')
    keys = binding['index_pointer'].strip('/').split('/')
    assert keys[:1] == ['artifacts'] and keys[1].isdigit(), binding
    node = at_pointer(index, binding['index_pointer'])
    parent = index['artifacts'][int(keys[1])]
    kind, path = binding['source_ref'].split(':', 1)
    expected_tail = ([] if kind == 'vendor' else ['text'] if kind == 'text'
                     else ['members', keys[-1]] if kind == 'member' else None)
    assert expected_tail is not None and keys[2:] == expected_tail, binding
    assert node['path'] == path == binding['path'], binding
    assert node['sha256'] == binding['sha256'] and node['bytes'] == binding['bytes'], binding
    assert parent['url'] == binding['url'], binding
    url = urlparse(parent['url'])
    assert url.scheme == 'https' and url.hostname == 'www.whxy.com', binding
    if kind == 'vendor':
        assert parent['id'] == binding['source_ref'], binding
    else:
        assert parent['id'] == binding['parent_source_ref'], binding
    if kind == 'member':
        assert node['members'] == binding['archive_member_chain'], binding
        assert keys[-1].isdigit() and binding['archive_member_chain'], binding
    # Each child resolves its own parent, not an unrelated parent carrying the
    # same digest or URL. The ordinary provenance gate verifies archive extraction.
    for consumer in [TYPED_EVIDENCE, TYPED_CHECKER]:
        assert node['evidence'].count(consumer) == 1, (binding['source_ref'], consumer)
    data = source_bytes(path, node['sha256'], node['bytes'])
    return node, data


@lru_cache(maxsize=None)
def pdf_page(path, number):
    return subprocess.check_output(['pdftotext', '-f', str(number), '-l', str(number),
                                    '-layout', str(SOURCES / path), '-']).decode()


def bounded_pages(text, locators, family):
    pages = text.split('\f')
    result = []
    for locator in locators:
        page = pages[locator['pdf_page_1_based'] - 1]
        assert family in page, (family, locator)
        assert re.search(r'\b' + str(locator['printed_page']) + r'\s*/', page), (family, locator)
        result.append(page)
    return '\n'.join(result)


def macro_value(header, name):
    match = re.search(r'^#define\s+' + re.escape(name)
                      + r'\s+\(?((?:0x)?[0-9A-Fa-f]+)UL\)?', header, re.M)
    assert match, name
    return int(match[1], 16 if match[1].startswith('0x') else 10)


def classic_protocol(family, block, text):
    locators = block['manual_locators']
    raw = {name: bounded_pages(text, places, family) for name, places in locators.items()}
    # x in multi-controller register prose is a documented placeholder.
    words = {name: flat(value).replace('I2Cx', 'I2C') for name, value in raw.items()}
    control, status = words['control'], words['status_codes']
    for clause in ['W0：清除I2C中断标志并使状态机执行下一个动作W1：无功能',
                   '在SI被清0之前，SCL低电平周期延长，传输暂停',
                   '在软件清除SI前，软件应该准备好合适的寄存器设置',
                   '在SI被清除后，I2C总线将会根据寄存器设置执行相应的操作',
                   '需要用户手动清除STA', 'W0：清零STA',
                   'W0：无功能W1：向总线发送STOP停止信号',
                   '硬件会在完成STOP信号发送后自动对STO清0',
                   '0：在应答阶段发送NACK', '1：在应答阶段发送ACK']:
        assert clause in control, (family, clause)
    for clause in ['0xF8除外', '同时产生中断请求', 'SI为0清除该标志位']:
        assert clause in words['interrupt_behavior'], (family, clause)
    for clause in ['SI还没有被置位，无中断产生',
                   '这种情况在其它状态和I2C模块还未开始执行串行传输之前出现',
                   '设备立即被切换到未寻址从机接收模式，释放SDA和SCL',
                   '将I2C_DR寄存器清零', '置位STO位并清除SI位',
                   '控制器并不会将STOP信号实际发送到总线上',
                   '依次设置I2C_CR.EN为0和1', '然后设置SI为0清除SI位']:
        assert clause in status, (family, clause)
    arbitration = words['arbitration_and_sync']
    assert '丢失仲裁的主机会立即切换到未被寻址的从机状态' in arbitration, family
    assert '丢失仲裁的主机会继续输出SCL串行时钟，直到当前字节传输完成' in arbitration, family
    for code in ['38', '68', '78', 'B0']:
        rows = [flat(line) for line in raw['status_codes'].splitlines() if re.search(r'\b'+code+r'H\b', line)]
        assert any('丢失仲裁' in line for line in rows), (family, code)
    assert all(term in words['master_state_diagram'] for term in
               ['STO=1，SI=0，发STOP', 'F8H', '短时的过渡状态，0xF8', '不会产生中断']), family
    for clause in ['fSCL=fPCLK/8/(BRR+1)', 'BRR有效范围为1~255', '主机时应设置EN为1']:
        assert clause in words['baud_clock'], (family, clause)
    for clause in ['BRR的值小于或等于9', 'I2C_CR.FLT为1', 'BRR的值大于9', 'I2C_CR.FLT为0']:
        assert clause in words['filter'], (family, clause)
    init = words['master_initialization']
    gate = block['instances'][0]['clock']['gate']['register']
    reset = block['instances'][0]['clock']['reset']['register']
    # The own initialization sequence is evidence for module reset. It is not
    # evidence that arbitrary EN=0 clears the FSM or releases physical lines.
    steps = [f'步骤2：设置SYSCTRL_{gate}.I2C为1',
             f'步骤3：向SYSCTRL_{reset}.I2C依次写入0、1',
             '步骤4：配置I2C_BRR', '步骤5：设置I2C_BRREN为1',
             '步骤6：设置I2C_CR.EN为1']
    positions = [init.find(step) for step in steps]
    assert all(pos >= 0 for pos in positions) and positions == sorted(positions), (family, steps)
    assert block['irq_contract'] == {
        'peripheral_local_enable_present': False, 'flag': 'CR.SI',
        'clear': 'write_zero_after_preparing_next_action',
        'stop_completion_interrupt': False, 'shared_i2c_vectors': []}
    return raw


def classic_registers(family, fact, block, raw):
    version = fact['register_version']
    authored = current_yaml(f'cw32-data/registers/{version}.yaml')
    generated = current_json(f'cw32-data/data/registers/{version}.json')
    offsets = {'BRREN': 0, 'BRR': 4, 'CR': 8, 'DR': 12, 'ADDR0': 16,
               'STAT': 20, 'ADDR1': 32, 'ADDR2': 36, 'MATCH': 40}
    listed = re.findall(r'I2Cx?_(\w+)\s+I2Cx?_BASE\s*\+\s*(0x[0-9A-Fa-f]+)', raw['register_list'])
    assert len(listed) == 9 and {name: int(value, 16) for name, value in listed} == offsets, family
    # Complete register list and exact CR fieldset exclude fictional local IER,
    # FIFO, DMA, or STOP-completion registers without reading HAL source text.
    expected = {'BRREN': {'EN': (0, 1)}, 'BRR': {'BRR': (0, 8)},
                'CR': {'FLT': (0, 1), 'AA': (2, 1), 'SI': (3, 1),
                       'STO': (4, 1), 'STA': (5, 1), 'EN': (6, 1)},
                'DR': {'DR': (0, 8)}, 'ADDR0': {'GC': (0, 1), 'ADDR0': (1, 7)},
                'STAT': {'STAT': (0, 8)}, 'ADDR1': {'ADDR1': (1, 7)},
                'ADDR2': {'ADDR2': (1, 7)},
                'MATCH': {'ADDR0': (0, 1), 'ADDR1': (1, 1), 'ADDR2': (2, 1)}}
    mux = family in {'CW32L010', 'CW32L011'}
    assert block['gpio_input_mux']['present'] == fact['gpio_input_selectors'] == mux
    if mux:
        expected['CR'].update({'SCLINSRC': (8, 3), 'SDAINSRC': (11, 3)})
        assert block['gpio_input_mux']['GPIO_encoding'] == 0
    page_numbers = sorted({p['pdf_page_1_based'] for role in
                           ['baud_registers', 'control', 'status_register', 'address_match_registers']
                           for p in block['manual_locators'][role]})
    text = selected_source(block['sources']['manual_text'])[1].decode()
    register_text = '\n'.join(text.split('\f')[n - 1] for n in page_numbers)
    headings = list(re.finditer(r'^\s*\d+\.\d+\.\d+\s+I2Cx?_(\w+)\b', register_text, re.M))
    sections = {m[1]: register_text[m.end():headings[i + 1].start() if i + 1 < len(headings) else None]
                for i, m in enumerate(headings)}
    assert set(sections) == set(offsets), (family, set(sections))
    baseline = {'BRREN': 0, 'BRR': 0, 'CR': 0, 'DR': 0, 'STAT': 0xf8}
    assert {name: block['reset_baseline'][name] for name in baseline} == baseline
    pac = (ROOT / f'cw32-metapac/src/peripherals/{version}.rs').read_text()
    for register, offset in offsets.items():
        access = 'Read' if register in {'STAT', 'MATCH'} else 'ReadWrite'
        section = sections[register]
        actual_fields = {name: (int(lo or hi), int(hi) - int(lo or hi) + 1)
                         for hi, lo, name, permission in re.findall(
                             r'^\s*(\d+)(?::(\d+))?\s+([A-Z][A-Z0-9]+)\s+(RW|RO)\b', section, re.M)
                         if name != 'RFU'}
        assert actual_fields == expected[register], (family, register, actual_fields)
        permissions = re.findall(r'^\s*\d+(?::\d+)?\s+[A-Z][A-Z0-9]+\s+(RW|RO)\b', section, re.M)
        assert set(permissions) == ({'RO'} if access == 'Read' else {'RW'}), (family, register)
        assert re.search(r'Address\s+offset:\s*0x0*' + format(offset, 'x') + r'\b', section, re.I)
        if register in baseline:
            assert f'Resetvalue:0x{baseline[register]:08X}' in flat(section), (family, register)
        for ir in [authored, generated]:
            items = named(ir['block/I2C']['items'])
            assert set(items) == set(offsets), (family, version)
            item = items[register]
            assert (item['byte_offset'], item.get('access', 'ReadWrite'), item.get('bit_size', 32),
                    item['fieldset']) == (offset, access, 32, register), (family, register)
            fields = ir['fieldset/' + register]['fields']
            assert {f['name']: (f['bit_offset'], f['bit_size']) for f in fields} == expected[register]
        method = register.lower() + ('_' if register == 'MATCH' else '')
        rust_type = register.title()
        match = re.search(r'pub const fn ' + method + r'\(self\).*?Reg<regs::' + rust_type
                          + r', crate::common::(R|RW)>\s*\{.*?wrapping_add\((0x[0-9a-f]+)usize\)', pac, re.S)
        assert match and match[1] == ('R' if access == 'Read' else 'RW') and int(match[2], 16) == offset
    variants = EVIDENCE['registers'][version]['variants']
    for ir in [authored, generated]:
        assert {v['name']: v['value'] for v in ir['enum/Status']['variants']} == variants
        assert ir['enum/Status']['bit_size'] == 8 and field(ir, 'STAT', 'STAT')['enum'] == 'Status'
    assert 'pub struct Status(u8);' in pac
    assert re.search(r'fn from_bits\(val: u8\) -> Status\s*\{\s*Self\(val & 0xff\)', pac)
    assert re.search(r'fn to_bits\(self\) -> u8\s*\{\s*self\.0', pac)


def classic_instances(family, fact, block, material, raw, chips):
    svd = ET.fromstring(material['selected_svd'])
    peripherals = {p.findtext('name'): p for p in svd.findall('./peripherals/peripheral')}
    all_irqs = [(p.findtext('name'), i.findtext('name'), int(i.findtext('value'), 0))
                for p in peripherals.values() for i in p.findall('interrupt')]
    instances = named(block['instances'])
    assert {name for name in peripherals if name.startswith('I2C')} == set(instances), family
    own_clock = current_yaml(next(iter(instances.values()))['clock']['authored_metadata'])
    authored = named(own_clock['peripherals'])
    header = material['device_header']
    irq_header = {name: int(number) for name, number in re.findall(r'\b(I2C\d*)_IRQn\s*=\s*(\d+)', header)}
    assert set(irq_header) == set(instances), family
    for name, instance in instances.items():
        irq, = instance['interrupts']
        assert instance['register_version'] == fact['register_version']
        assert int(peripherals[name].findtext('baseAddress'), 0) == int(instance['base_address'], 16)
        assert macro_value(header, name + '_BASE') == int(instance['base_address'], 16)
        assert irq_header[name] == irq['number'] and irq['name'] == name and irq['signal'] == 'GLOBAL'
        owners = [{'peripheral': p, 'interrupt': i} for p, i, n in all_irqs if n == irq['number']]
        assert owners == irq['source_svd_all_owners'] == [{'peripheral': name, 'interrupt': name}]
        assert irq['shared'] is False
        manual_name = instance['manual_name']
        assert manual_name == (name if len(instances) == 2 else 'I2C')
        assert re.search(r'\b' + str(irq['number'] + 16) + r'\s+' + str(irq['number'])
                         + r'\s+可配置\s+' + manual_name + r'\s', raw['nvic']), (family, name)
        assert f'{manual_name}_BASE={instance["base_address"]}' in flat(raw['register_list']), (family, name)
        clock, authored_clock = instance['clock'], authored[name]
        assert clock['bus'] == clock['kernel'] == authored_clock['bus_clock'] == 'PCLK'
        assert authored_clock['kernel_clock'] == {'clock': 'PCLK'}
        for kind, source_key, polarity in [('enable', 'gate', 'active_value'), ('reset', 'reset', 'asserted_value')]:
            source = clock[source_key]
            expected_field = {key: source[key] for key in ['peripheral', 'register', 'field']}
            assert authored_clock[kind] == expected_field
            assert authored_clock[kind + '_' + polarity] == source[polarity] == (1 if kind == 'enable' else 0)
            assert authored_clock.get(kind + '_write_key') == source['write_key']
            assert authored_clock.get('shared_' + kind + '_group') is None
            physical = instance['rcc_ownership'][kind]
            assert physical['shared_group'] is None and physical['all_physical_owners'] == [name]
            assert (physical['bit_offset'], physical['bit_size']) == (source['bit'], 1)
            own_field = instance['header_rcc_field']
            assert own_field == ('I2C' if family == 'CW32L011' else source['field'])
            macro = 'SYSCTRL_' + source['register'] + '_' + own_field
            assert macro_value(header, macro + '_Pos') == source['bit']
            assert macro_value(header, macro + '_Msk') == 1 << source['bit']
            manual = raw[source_key]
            assert re.search(r'\b' + str(source['bit']) + r'\s+' + manual_name + r'\s+RW\b', manual)
            assert re.search(r'Address\s+offset:\s*0x0*' + format(physical['sysctrl_byte_offset'], 'x') + r'\b', manual, re.I)
            if kind == 'reset':
                assert '0：模块处于复位状态' in flat(manual) and '1：模块正常工作' in flat(manual)
                assert source['write_key'] is None and not re.search(r'\bKEY\s+WO\b', manual)
            else:
                assert '0：关闭' in flat(manual) and '1：使能' in flat(manual)
                keyed = family in {'CW32L010', 'CW32L011'}
                assert (source['write_key'] is not None) == keyed
                assert ('0x5A5A' in manual) == keyed
                if keyed:
                    assert source['write_key'] == {'field': {'peripheral': 'SYSCTRL', 'register': source['register'], 'field': 'KEY'}, 'value': 0x5a5a}
                    assert re.search(r'31:16\s+KEY\s+WO', manual)
    if len(instances) == 1:
        assert not any(number == 24 for _, _, number in all_irqs), family
    # Inspect every current exact-chip and family-profile artifact in this family.
    assert chips, family
    for path, chip in chips:
        profile = current_yaml(f'cw32-data/inputs/{family.lower()}.yaml')
        assert chip['line'] == family == profile['line']
        assert chip['family'] == profile['family'] and chip['die'] == profile['die']
        for core in chip['cores']:
            per = named(core['peripherals'])
            current = {name: p for name, p in per.items() if p.get('registers', {}).get('kind') == 'i2c'}
            assert set(current) == set(instances), path
            irq_table = named(core['interrupts'])
            sysctrl = per['SYSCTRL']['registers']
            sys_ir = current_json(f'cw32-data/data/registers/sysctrl_{sysctrl["version"]}.json')
            sys_items = named(sys_ir['block/' + sysctrl['block']]['items'])
            def physical_field(control, kind):
                fld = control.get(kind)
                if not fld or control['controller'] != 'SYSCTRL':
                    return None
                item = sys_items[fld['register']]
                bits = field(sys_ir, item['fieldset'], fld['field'])
                return item['byte_offset'], bits['bit_offset'], bits['bit_size']
            for name, instance in instances.items():
                p = current[name]
                assert p['registers'] == {'kind': 'i2c', 'version': fact['register_version'][4:], 'block': 'I2C'}
                assert p['address'] == int(instance['base_address'], 16)
                assert p['interrupts'] == [{'signal': 'GLOBAL', 'interrupt': name}]
                assert irq_table[name]['number'] == instance['interrupts'][0]['number']
                assert [q['name'] for q in per.values() for i in q.get('interrupts', [])
                        if irq_table[i['interrupt']]['number'] == irq_table[name]['number']] == [name]
                control = p['rcc_control']
                assert control['controller'] == 'SYSCTRL' and control['bus_clock'] == 'PCLK'
                assert p['rcc']['bus_clock'] == p['rcc']['kernel_clock'] == 'PCLK'
                for kind, source_key, polarity in [('enable', 'gate', 'active_value'), ('reset', 'reset', 'asserted_value')]:
                    source = instance['clock'][source_key]
                    fld = {k: source[k] for k in ['register', 'field']}
                    assert control[kind] == p['rcc'][kind] == fld
                    assert control[kind + '_' + polarity] == bool(source[polarity])
                    key = source['write_key']
                    expected_key = None if key is None else {'field': {k: key['field'][k] for k in ['register', 'field']}, 'value': key['value']}
                    assert control.get(kind + '_write_key') == expected_key
                    if key is not None:
                        assert field(sys_ir, source['register'], 'KEY')['bit_offset'] == 16
                        assert field(sys_ir, source['register'], 'KEY')['bit_size'] == 16
                    assert control.get('shared_' + kind + '_group') is None
                    expected = instance['rcc_ownership'][kind]
                    physical = physical_field(control, kind)
                    assert physical == (expected['sysctrl_byte_offset'], expected['bit_offset'], expected['bit_size'])
                    owners = sorted(q['name'] for q in per.values()
                                    if 'rcc_control' in q and physical_field(q['rcc_control'], kind) == physical)
                    assert owners == expected['all_physical_owners'] == [name], (path, name, kind, owners)
                assert p['i2c_limits']['maximum_frequency_hz'] == block['timing_evidence']['maximum_scl_hz'] == 1000000


def classic_routes(family, block, material):
    package = block['representative_package']
    chip = current_json(package['chip_metadata'])
    part_path, pointer = package['part_identity'].split('#', 1)
    part = at_pointer(current_yaml(part_path), pointer)
    assert (part['name'], part['feature'], part['family'], part['package']) == (
        package['part'], package['feature'], family, package['package'])
    assert chip['name'] == package['part'] and chip['line'] == family
    profile = current_yaml(f'cw32-data/inputs/{family.lower()}.yaml')
    assert chip['family'] == profile['family'] and chip['die'] == profile['die']
    pack, = chip['packages']
    assert pack['name'] == package['part'] and pack['package'] == package['package']
    peripherals = named(chip['cores'][0]['peripherals'])
    timing_ds = block['sources']['datasheet']
    assert block['timing_evidence']['datasheet']['source_ref'] == timing_ds['source_ref']
    # F020's already-locked timing PDF differs from its current pinout PDF.
    # Resolve each purpose by exact identity; neither version is substituted.
    ds, = [binding for binding in block['sources'].values()
           if binding['source_ref'] == package['pinout_source_ref']]
    for number in block['timing_evidence']['datasheet']['pdf_pages_1_based']:
        timing = flat(pdf_page(timing_ds['path'], number))
        assert family in timing and 'Fast-modePlus' in timing and ('1Mbit/s' in timing or '1Mbits/s' in timing), family
    seen = set()
    for route in package['routes']:
        key = (route['peripheral'], route['signal'])
        assert key not in seen
        seen.add(key)
        target = {k: route[k] for k in ['pin', 'af', 'signal']}
        assert target in peripherals[route['peripheral']]['pins']
        physical = next(p for p in pack['pins'] if p['position'] == route['package_position'])
        assert route['pin'] in physical['signals']
        path, pointer = route['pinout_pointer'].split('#', 1)
        pinout = current_yaml(path)
        row = at_pointer(pinout, pointer)
        assert pinout['source']['source_ref'] == ds['source_ref'] and pinout['source']['sha256'] == ds['sha256']
        authored_package, = [p for p in pinout['packages'] if p['name'] == package['part']]
        assert authored_package['package'] == package['package']
        column = authored_package['table_column']
        assert column in pinout['table_columns']
        assert column == package.get('pinout_table_column', package['package'])
        assert row['positions'][column] == route['package_position']
        assert route['pin'] in row['signals'] and row['pin_type'] == route['pin_type'] == 'I/O'
        assert row['pdf_page_index'] + 1 == route['pinout_pdf_page_1_based']
        source_page = pdf_page(ds['path'], route['pinout_pdf_page_1_based'])
        assert family in source_page and row['source_name'] in flat(source_page)
        for evidence in route['af_evidence']:
            authored = current_yaml(evidence['path'])
            af = at_pointer(authored, evidence['pointer'])
            assert all(af[k] == route[k] for k in ['peripheral', 'signal', 'pin', 'af'])
            assert af['source_macro'] == evidence['source_macro'] and af['source_line'] == evidence['source_line']
            source = authored['source']
            assert source['header_sha256'] == block['sources']['gpio_header']['sha256']
            assert source['sdk_url'] == block['sources']['sdk']['url'] and source['sdk_sha256'] == block['sources']['sdk']['sha256']
            line = material['gpio_header'].splitlines()[af['source_line'] - 1]
            assert re.search(r'^#define\s+' + re.escape(af['source_macro']) + r'\(\)', line)
            assert re.search(r'\.' + af['gpio_field'] + r'\s*=\s*' + str(route['af']) + r'\)', line)
            assert '->' + af['gpio_register'] + '_f.' in line
    assert seen == {(i['name'], signal) for i in block['instances'] for signal in ['SCL', 'SDA']}
    return len(seen)


def classic_async_sources():
    families = {name: fact for name, fact in EVIDENCE['families'].items() if 'async_master' in fact}
    assert set(families) == CLASSIC_FAMILIES and 'async_master' not in EVIDENCE['families']['CW32L012']
    chips = {family: [] for family in families}
    for path in sorted((ROOT / 'cw32-data/data/chips').glob('*.json')):
        chip = current_json(str(path.relative_to(ROOT)))
        if chip['line'] in chips:
            chips[chip['line']].append((str(path.relative_to(ROOT)), chip))
    # Broad family (for example CW32F0) is distinct from the exact source line.
    # Derive the complete current inventory from authored profiles and parts.
    for family, entries in chips.items():
        profile = current_yaml(f'cw32-data/inputs/{family.lower()}.yaml')
        assert profile['line'] == family
        catalog = current_yaml(profile['parts_catalog'])['parts']
        expected = {p['name'] for p in profile['chips']} | {
            p['name'] for p in catalog if p['family'] == family}
        assert {chip['name'] for _, chip in entries} == expected, family
        assert all(Path(path).stem == chip['name'] for path, chip in entries), family
    slots, routes, versions = 0, 0, set()
    for family, fact in families.items():
        block = fact['async_master']
        assert block['identity_authority'] == 'sources/evidence-sources.json'
        material = {}
        for role, binding in block['sources'].items():
            _, data = selected_source(binding)
            if not binding['source_ref'].startswith('vendor:'):
                material[role] = data.decode(errors='replace')
            authority = ('cw32f030' if family == 'CW32A030' and role in
                         {'sdk', 'device_header', 'i2c_sdk_header', 'gpio_header', 'selected_svd'} else
                         'cw32x030' if family in {'CW32F030', 'CW32A030'} and role in
                         {'manual', 'manual_text'} else family.lower())
            assert authority in binding['path'].lower(), (family, role)
            if binding['source_ref'].startswith('member:'):
                assert binding['parent_source_ref'] == block['sources']['sdk']['source_ref'], (family, role)
        assert block['sources']['manual_text']['parent_source_ref'] == block['sources']['manual']['source_ref']
        for existing, role in [('manual', 'manual'), ('sdk_header', 'i2c_sdk_header')]:
            assert fact[existing]['filename'] == block['sources'][role]['path']
            assert fact[existing]['sha256'] == block['sources'][role]['sha256']
            assert fact[existing]['url'] == block['sources'][role]['url']
        raw = classic_protocol(family, block, material['manual_text'])
        classic_registers(family, fact, block, raw)
        classic_instances(family, fact, block, material, raw, chips[family])
        routes += classic_routes(family, block, material)
        slots += len(block['instances'])
        versions.add(fact['register_version'])
        print(f'PASS {family}: own classic async protocol, dedicated IRQ, exclusive reset and exact-package routes')
    assert slots == 17 and len(versions) == 5 and routes == 34
    print('PASS classic async sources: 12 families, 17 controllers, 5 IP versions and 34 exact-package routes')


def command_pages(block):
    """Reopen exact locked PDF pages; receipts and HAL text are not inputs."""
    sources = {b['source_ref']: b for b in block['sources'].values()}
    identities = {(p['source_ref'], p['pdf_page_1_based']): p for p in block['page_identities']}
    assert len(identities) == len(block['page_identities']) == 32
    expected = {'manual': {59, 63, 71, 524, 530, 531, 533, 536, 537, 541, 542,
                           546, 547, 548, 549, 550, 551, 552, 553, 554, 555, 556, 557, 562, 563, 564},
                'datasheet': {5, 32, 33, 36, 37, 38}}
    for role, printed in expected.items():
        source = block['sources'][role]
        assert {p['printed_page'] for p in identities.values() if p['source_ref'] == source['source_ref']} == printed
    content = {}
    for key, page in identities.items():
        source = sources[page['source_ref']]
        text = pdf_page(source['path'], page['pdf_page_1_based'])
        assert digest(text.encode()) == page['layout_extract_sha256'], key
        offset = 26 if source == block['sources']['manual'] else 3
        assert page['pdf_page_1_based'] == page['printed_page'] + offset
        assert 'CW32L012' in text and re.search(r'^\s*' + str(page['printed_page'])
                                               + r'\s+(?:https://www\.whxy\.com)?\s*$', text, re.M)
        content[key] = text
    result = {}
    for role, places in block['manual_locators'].items():
        assert places and len({(p['source_ref'], p['pdf_page_1_based']) for p in places}) == len(places)
        for page in places:
            assert page == identities[page['source_ref'], page['pdf_page_1_based']]
        result[role] = '\n'.join(content[p['source_ref'], p['pdf_page_1_based']] for p in places)
    return result


def command_protocol(block, raw, material):
    words = {name: flat(text) for name, text in raw.items()}
    clauses = {
        'fifo_depth': ['1字节命令/发送FIFO', '1字节接收FIFO', '独立的I2C从机寄存器'],
        'master_reset': ['I2Cx_MCR0.RESET将所有主机逻辑和寄存器重置为默认状态，I2Cx_MCR0寄存器自身除外',
                         'R0W1', '00：PCLK'],
        'men_disable_drain': ['I2C继续清空发送FIFO', '直到发送了STOP停止信号',
                              'I2C将不再暂停I2C总线等待发送或接收FIFO', 'I2C自动生成STOP停止信号'],
        'receive_commands': ['在主机接收传输的最后一个字节上发送NACK',
                             '除非发送FIFO中的下一个命令也是接收数据命令', '（MTDR[7:0]+1）'],
        'errors': ['I2C主机释放I2C总线', '主机在发送数据时检测到未预料的NACK',
                   '在NACK标志被清除之前，主机不会发出新的START', '只要引脚低电平超时持续，PINLOW标志就无法清除'],
        'arbitration': ['丢失仲裁的主机会继续输出SCL串行时钟，直到当前字节传输完成'],
        'master_registers': ['先通过MCR0.TXFIFORST复位TXFIFO后，才能用MICR.FIFO清除',
                             'Resetvalue:0x00007F04', 'R1W0', 'RepeatedSTART信号或STOP信号'],
        'slave_admission': ['I2C主机逻辑独立于从机逻辑运行', 'I2C从机逻辑也独立于主机逻辑运行',
                            'I2Cx_SCR0', 'I2Cx_SIER', 'I2Cx_SDER', 'SLVBUSY'],
    }
    for role, required in clauses.items():
        for clause in required:
            assert clause in words[role], (role, clause)
    irq_fields = ['TXE', 'RXNE', 'PACKET', 'STOP', 'NACK', 'ARBI', 'FIFO', 'PINLOW', 'MATCH']
    enables = re.findall(r'^\s*\d+\s+([A-Z]+)\s+RW\b', pdf_page(block['sources']['manual']['path'], 580), re.M)
    assert set(enables) == set(irq_fields) and len(enables) == len(irq_fields)
    p = block['protocol']
    assert p['address_bits'] == 7 and p['receive_command_bytes'] == [1, 256]
    assert p['transmit_fifo_entries'] == p['receive_fifo_entries'] == 1
    assert p['receive_command_encoding'] == 'count_minus_one'
    assert p['watermarks'] == {'TXWATER': 0, 'RXWATER': 0}
    assert p['irq_enable_fields'] == irq_fields and p['error_fields'] == ['NACK', 'ARBI', 'FIFO', 'PINLOW']
    assert p['idle_interrupt_enable_present'] is False and p['micr_write_noop'] == 0x7f04
    assert p['gpio_input_selector'] == p['pin_configuration'] == 0 and p['scr2_alias_byte_offset'] == 0x24
    assert re.search(r'^#define\s+FSL_FEATURE_I2C_FIFO_SIZEn\(x\)\s+\(1\)', material['i2c_sdk_header'], re.M)
    sdk = flat(material['i2c_sdk_source'])
    assert 'I2Cx->MTDR=I2C_Cmd_Rx_DataToMRDR|(rxSize-1);' in sdk
    assert 'kRxDataCmd=I2C_MTDR_CMD(0X1U)' in flat(material['i2c_sdk_header'])
    assert re.search(r'^#define\s+SYSCTRL_KEY\s+\(0x5A5A0000U\)', material['sysctrl_header'], re.M)


def command_registers(fact, material):
    version = fact['register_version']
    source = ET.fromstring(material['selected_svd'])
    per = {p.findtext('name'): p for p in source.findall('./peripherals/peripheral')}
    assert per['I2C2'].get('derivedFrom') == 'I2C1'
    registers = {r.findtext('name'): r for r in per['I2C1'].findall('./registers/register')}
    assert len(registers) == 29
    pac = (ROOT / f'cw32-metapac/src/peripherals/{version}.rs').read_text()
    for ir in [current_yaml(f'cw32-data/registers/{version}.yaml'), current_json(f'cw32-data/data/registers/{version}.json')]:
        items = named(ir['block/I2C']['items'])
        assert set(items) == set(registers)
        for name, reg in registers.items():
            item = items[name]
            offset = int(reg.findtext('addressOffset'), 0)
            access = {'read-only': 'Read', 'write-only': 'Write', 'read-write': 'ReadWrite'}[reg.findtext('access')]
            assert (item['byte_offset'], item.get('access', 'ReadWrite'), item.get('bit_size', 32)) == (offset, access, 32)
            assert offset % 4 == 0 and item.get('array') is None
            expected = {f.findtext('name'): (int(f.findtext('lsb')), int(f.findtext('msb')) - int(f.findtext('lsb')) + 1)
                        for f in reg.findall('./fields/field')}
            fields = ir['fieldset/' + item['fieldset']]['fields']
            assert len(named(fields)) == len(expected)
            assert ir['fieldset/' + item['fieldset']].get('bit_size', 32) == 32
            assert all(f.get('array') is None and isinstance(f['bit_offset'], int)
                       and 0 < f['bit_size'] <= 32 and 0 <= f['bit_offset'] <= 32 - f['bit_size'] for f in fields)
            assert {f['name']: (f['bit_offset'], f['bit_size']) for f in fields} == expected, name
            for fld, (bit, width) in expected.items():
                macro = f'I2C_{name}_{fld}'
                assert macro_value(material['device_header'], macro + '_Pos') == bit
                assert macro_value(material['device_header'], macro + '_Msk') == ((1 << width) - 1) << bit
            mode = {'Read': 'R', 'Write': 'W', 'ReadWrite': 'RW'}[access]
            match = re.search(r'pub const fn ' + name.lower() + r'\(self\).*?Reg<regs::' + name.title()
                              + r', crate::common::(R|W|RW)>\s*\{.*?wrapping_add\((0x[0-9a-f]+)usize\)', pac, re.S)
            assert match and match[1] == mode and int(match[2], 16) == offset, name
        assert items['SCR2']['byte_offset'] == items['MCR2']['byte_offset'] == 0x24
        assert {f['name'] for f in ir['fieldset/MIER']['fields']} == set(fact['command_async_master']['protocol']['irq_enable_fields'])
        assert field(ir, 'MRDR', 'EMPTY')['bit_offset'] == 14
    return per


def command_rcc_field(ir, block, reference):
    item = named(ir['block/' + block]['items'])[reference['register']]
    assert item.get('array') is None and item['byte_offset'] % 4 == 0
    assert item.get('access', 'ReadWrite') == 'ReadWrite' and item.get('bit_size', 32) == 32
    fields = ir['fieldset/' + item['fieldset']]
    assert fields.get('bit_size', 32) == 32
    fld = named(fields['fields'])[reference['field']]
    assert fld.get('array') is None and isinstance(fld['bit_offset'], int)
    assert 0 < fld['bit_size'] <= 32 and 0 <= fld['bit_offset'] <= 32 - fld['bit_size']
    return item['byte_offset'], fld['bit_offset'], fld['bit_size']


def command_instances(block, raw, material, svd):
    instances = named(block['instances'])
    assert set(instances) == {name for name in svd if name.startswith('I2C')} == {'I2C1', 'I2C2'}
    all_irqs = [(p.findtext('name'), i.findtext('name'), int(i.findtext('value'), 0))
                for p in svd.values() for i in p.findall('interrupt')]
    header_irqs = {name: int(number) for name, number in re.findall(r'\b(I2C\d+)_IRQn\s*=\s*(\d+)', material['device_header'])}
    assert header_irqs == {'I2C1': 23, 'I2C2': 24}
    clock = named(current_yaml('cw32-data/clock/cw32l012.yaml')['peripherals'])
    sysregs = {r.findtext('name'): r for r in svd['SYSCTRL'].findall('./registers/register')}
    for name, address, number, bit in [('I2C1', 0x40005800, 23, 6), ('I2C2', 0x40005c00, 24, 11)]:
        i = instances[name]
        assert (i['register_kind'], i['register_version'], i['register_block']) == ('i2c', 'cw32l012_v1', 'I2C')
        assert int(i['base_address'], 16) == int(svd[name].findtext('baseAddress'), 0) == macro_value(material['device_header'], name + '_BASE') == address
        assert i['interrupt'] == {'name': name, 'number': number, 'signal': 'GLOBAL', 'shared': False, 'all_source_owners': [name]}
        assert [(p, irq) for p, irq, n in all_irqs if n == number] == [(name, name)]
        assert re.search(r'\b' + str(number + 16) + r'\s+' + str(number) + r'\s+可配置\s+' + name + r'\s', raw['nvic'])
        assert i['clock']['bus'] == clock[name]['bus_clock'] == 'PCLK' and i['clock']['master_source_encoding'] == 0
        assert clock[name]['kernel_clock'] == {'mux': {'peripheral': name, 'register': 'MCR0', 'field': 'CLKSRC'}}
        for kind, role, register, offset, polarity, active in [('enable', 'gate', 'APBEN2', 0x34, 'active_value', True),
                                                             ('reset', 'reset', 'APBRST2', 0x44, 'asserted_value', False)]:
            source = i['clock'][role]
            assert {k: source[k] for k in ['peripheral', 'register', 'field']} == clock[name][kind] == {'peripheral': 'SYSCTRL', 'register': register, 'field': name}
            assert (source['byte_offset'], source['bit_offset'], source['bit_size'], source[polarity]) == (offset, bit, 1, active)
            assert source['all_physical_owners'] == [name] and source['shared_group'] is None
            assert clock[name][kind + '_' + polarity] == int(active) and clock[name].get('shared_' + kind + '_group') is None
            reg = sysregs[register]
            assert int(reg.findtext('addressOffset'), 0) == offset and reg.findtext('access') == 'read-write'
            source_field, = [f for f in reg.findall('./fields/field') if f.findtext('name') == name]
            assert int(source_field.findtext('lsb')) == int(source_field.findtext('msb')) == bit
            assert macro_value(material['device_header'], f'SYSCTRL_{register}_{name}_Pos') == bit
            assert macro_value(material['device_header'], f'SYSCTRL_{register}_{name}_Msk') == 1 << bit
            for ir in [current_yaml('cw32-data/registers/sysctrl_cw32l012_v1.yaml'),
                       current_json('cw32-data/data/registers/sysctrl_cw32l012_v1.json')]:
                assert command_rcc_field(ir, 'SYSCTRL', clock[name][kind]) == (offset, bit, 1)
                if kind == 'enable':
                    assert command_rcc_field(ir, 'SYSCTRL', clock[name]['enable_write_key']['field']) == (offset, 16, 16)
            assert re.search(r'\b' + str(bit) + r'\s+' + name + r'\s+RW\b', raw[role])
            assert f'Addressoffset:0x{offset:X}' in flat(raw[role])
            if kind == 'enable':
                assert source['write_key'] == clock[name]['enable_write_key'] == {'field': {'peripheral': 'SYSCTRL', 'register': register, 'field': 'KEY'}, 'value': 0x5a5a}
                key, = [f for f in reg.findall('./fields/field') if f.findtext('name') == 'KEY']
                assert (int(key.findtext('lsb')), int(key.findtext('msb')), key.findtext('access')) == (16, 31, 'write-only')
                assert macro_value(material['device_header'], 'SYSCTRL_APBEN2_KEY_Pos') == 16
                assert macro_value(material['device_header'], 'SYSCTRL_APBEN2_KEY_Msk') == 0xffff0000
                assert '31:16KEYWO' in flat(raw[role]) and '0x5A5A' in raw[role]
                assert '0：禁止' in flat(raw[role]) and '1：使能' in flat(raw[role])
                line, = [line for line in material['sysctrl_header'].splitlines() if line.startswith(f'#define __SYSCTRL_{name}_CLK_ENABLE()')]
                assert 'REGBITS_SET(CW_SYSCTRL->APBEN2, (SYSCTRL_KEY | SYSCTRL_APB2_PERIPH_' + name + '))' in line
            else:
                assert source['write_key'] is None and clock[name].get('reset_write_key') is None
                assert '0：模块处于复位状态' in flat(raw[role]) and '1：模块正常工作' in flat(raw[role])
                assert 'Resetvalue:0x000000FF' in flat(raw[role]) and i['clock']['central_reset_register_value'] == 0xff
    profile = current_yaml('cw32-data/inputs/cw32l012.yaml')
    expected = {p['name'] for p in profile['chips']} | {p['name'] for p in current_yaml(profile['parts_catalog'])['parts'] if p['family'] == 'CW32L012'}
    assert expected == set(block['chip_inventory']) == {'CW32L012', 'CW32L012C8T6', 'CW32L012C8U6'}
    chips = [current_json(str(p.relative_to(ROOT))) for p in sorted((ROOT / 'cw32-data/data/chips').glob('*.json'))]
    chips = [c for c in chips if c['line'] == 'CW32L012']
    assert {c['name'] for c in chips} == expected and len(chips) == 3
    assert profile['source']['source_ref'] == block['sources']['selected_svd']['source_ref']
    assert profile['source']['sha256'] == block['sources']['selected_svd']['sha256']
    for chip in chips:
        assert (chip['family'], chip['line'], chip['die']) == ('CW32L0', 'CW32L012', 'CW32L012')
        assert len(chip['cores']) == 1
        core = chip['cores'][0]
        per, irqs = named(core['peripherals']), named(core['interrupts'])
        assert {p['name'] for p in per.values() if p['name'].startswith('I2C') or p.get('registers', {}).get('kind') == 'i2c'} == set(instances)
        def physical(control, kind):
            if not control.get(kind):
                return None
            bank = per[control['controller']]
            regs = bank['registers']
            ir = current_json(f'cw32-data/data/registers/{regs["kind"]}_{regs["version"]}.json')
            offset, bit, width = command_rcc_field(ir, regs['block'], control[kind])
            return bank['address'] + offset, bit, width
        for name, i in instances.items():
            p, irq = per[name], i['interrupt']
            assert p['address'] == int(i['base_address'], 16) and p['registers'] == {'kind': 'i2c', 'version': 'cw32l012_v1', 'block': 'I2C'}
            assert p['interrupts'] == [{'signal': 'GLOBAL', 'interrupt': name}]
            assert irqs[name]['number'] == irq['number'] and sum(q['number'] == irq['number'] for q in irqs.values()) == 1
            assert [(q['name'], link['signal']) for q in per.values() for link in q.get('interrupts', []) if irqs[link['interrupt']]['number'] == irq['number']] == [(name, 'GLOBAL')]
            c = p['rcc_control']
            assert c['controller'] == 'SYSCTRL' and c['bus_clock'] == 'PCLK' and per['SYSCTRL']['address'] == 0x40004000
            assert per['SYSCTRL']['registers'] == {'kind': 'sysctrl', 'version': 'cw32l012_v1', 'block': 'SYSCTRL'}
            for kind, role, polarity, active in [('enable', 'gate', 'active_value', True), ('reset', 'reset', 'asserted_value', False)]:
                source = i['clock'][role]
                assert c[kind] == {k: source[k] for k in ['register', 'field']}
                assert c[kind + '_' + polarity] is active and c.get('shared_' + kind + '_group') is None
                own = physical(c, kind)
                assert own == (0x40004000 + source['byte_offset'], source['bit_offset'], 1)
                owners = []
                for q in per.values():
                    if 'rcc_control' not in q:
                        continue
                    for operation in ['enable', 'reset']:
                        other = physical(q['rcc_control'], operation)
                        if other and other[0] == own[0] and other[1] <= own[1] < other[1] + other[2]:
                            owners.append((q['name'], operation))
                assert owners == [(name, kind)], (chip['name'], name, kind, owners)
            assert c['enable_write_key'] == {'field': {'register': 'APBEN2', 'field': 'KEY'}, 'value': 0x5a5a}
            sys_ir = current_json('cw32-data/data/registers/sysctrl_cw32l012_v1.json')
            assert command_rcc_field(sys_ir, 'SYSCTRL', c['enable_write_key']['field']) == (0x34, 16, 16)
            assert p['i2c_limits']['maximum_frequency_hz'] == 1000000
    return chips


def command_routes(block, material, chips):
    from importlib.util import module_from_spec, spec_from_file_location
    import sys
    from verify_final_serial_af import pdf_cells
    routes = current_yaml(block['route_inventory']['authored'])
    sources = block['sources']
    assert routes['source']['header_sha256'] == sources['gpio_header']['sha256']
    assert routes['source']['sdk_sha256'] == sources['sdk']['sha256'] and routes['source']['sdk_url'] == sources['sdk']['url']
    cells = pdf_cells(SOURCES / sources['datasheet']['path'], 'CW32L012')
    selected = [r for r in routes['routes'] if r['peripheral'].startswith('I2C')]
    assert len(selected) == 24 and block['route_inventory']['counts'] == {'I2C1': 18, 'I2C2': 6}
    inventory = {(r['peripheral'], r['signal'], r['pin'], r['af']) for r in selected}
    assert len(inventory) == 24
    source_i2c = {key: cell for key, cell in cells.items() if cell['function'].startswith('I2C')}
    assert {(r['pin'], r['af']) for r in selected} == set(source_i2c)
    for r in selected:
        cell = source_i2c[r['pin'], r['af']]
        assert r['datasheet_cell'] == {k: v for k, v in cell.items() if k != 'bbox'}
        assert cell['function'] == r['peripheral'] + '_' + r['signal']
        line = material['gpio_header'].splitlines()[r['source_line'] - 1]
        assert re.search(r'^#define\s+' + re.escape(r['source_macro']) + r'\(\)', line)
        assert re.search(r'\.' + r['gpio_field'] + r'\s*=\s*' + str(r['af']) + r'\)', line)
        assert '->' + r['gpio_register'] + '_f.' in line
    pinout = current_yaml('cw32-data/pinouts/cw32l012.yaml')
    assert pinout['source']['source_ref'] == sources['datasheet']['source_ref'] and pinout['source']['sha256'] == sources['datasheet']['sha256']
    # Reuse the existing own-PDF table reader; do not infer package bonds from
    # matching generated and authored metadata alone.
    spec = spec_from_file_location('i2c_pinout_source_reader', ROOT / 'cw32-data/tools/extract_pinouts.py')
    reader = module_from_spec(spec)
    search_path = sys.path[:]
    try:
        sys.path.insert(0, str(ROOT / 'cw32-data/tools'))
        spec.loader.exec_module(reader)
    finally:
        sys.path[:] = search_path
    assert reader.extract_rows(SOURCES / sources['datasheet']['path'], 'CW32L012') == pinout['table_rows']
    devices = ET.fromstring(material['device_pack']).findall('.//device')
    device, = [d for d in devices if d.get('Dname') == 'CW32L012C8']
    memories = {m.get('name'): (int(m.get('start'), 0), int(m.get('size'), 0)) for m in device.findall('memory')}
    assert memories == {'IROM1': (0, 65536), 'IRAM1': (0x20000000, 8192)}
    assert {p['part'] for p in block['representative_packages']} == {'CW32L012C8T6', 'CW32L012C8U6'}
    for chip in chips:
        core = chip['cores'][0]
        actual = {(p['name'], route['signal'], route['pin'], route['af']) for p in core['peripherals'] if p['name'].startswith('I2C') for route in p['pins']}
        pins = {p['name'] for p in core['pins']}
        assert actual == {r for r in inventory if r[2] in pins}
        if chip['name'] == 'CW32L012':
            assert chip['memory'] == chip['packages'] == [] and actual == inventory
            continue
        pack, = chip['packages']
        authored_pack, = [p for p in pinout['packages'] if p['name'] == chip['name']]
        assert pack['package'] == authored_pack['package'] == authored_pack['table_column']
        column = authored_pack['table_column']
        source_pins = sorted([{'position': row['positions'][column], 'signals': row['signals']}
                              for row in pinout['table_rows'] if row['positions'][column] is not None],
                             key=lambda p: int(p['position']))
        assert [p['position'] for p in source_pins] == [str(n) for n in range(1, 49)]
        assert pack['pins'] == authored_pack['pins'] == source_pins
        bonded = {s for p in pack['pins'] for s in p['signals'] if re.fullmatch(r'P[A-F]\d+', s)}
        assert pins == bonded
        sample, = [p for p in block['representative_packages'] if p['part'] == chip['name']]
        path, pointer = sample['part_identity'].split('#')
        part = at_pointer(current_yaml(path), pointer)
        assert (part['name'], part['feature'], part['package'], part['family']) == (sample['part'], sample['feature'], sample['package'], 'CW32L012')
        memory = [{'name': 'FLASH', 'kind': 'flash', 'address': 0, 'size': 65536}, {'name': 'RAM', 'kind': 'ram', 'address': 0x20000000, 'size': 8192}]
        assert chip['memory'] == [memory] and part['memory'] == sample['memory'] == memory
        assert sample['chip_metadata'] == f'cw32-data/data/chips/{chip["name"]}.json'
        assert part['dfp_device'] == sample['device_pack_device'] == 'CW32L012C8'
        assert {(r['peripheral'], r['pin'], r['signal'], r['af'], r['package_position']) for r in sample['routes']} == {
            ('I2C1', 'PA4', 'SCL', 2, '14'), ('I2C1', 'PA5', 'SDA', 2, '15'),
            ('I2C2', 'PB0', 'SCL', 8, '18'), ('I2C2', 'PB1', 'SDA', 8, '19')}
        for r in sample['routes']:
            assert (r['peripheral'], r['signal'], r['pin'], r['af']) in actual
            assert r['pin'] in next(p for p in pack['pins'] if p['position'] == r['package_position'])['signals']
            path, pointer = r['pinout_pointer'].split('#')
            row = at_pointer(current_yaml(path), pointer)
            assert r['pin'] in row['signals'] and row['pin_type'] == r['pin_type'] == 'I/O'
            assert row['positions'][sample['package']] == r['package_position']
            assert row['pdf_page_index'] + 1 == r['pinout_pdf_page_1_based']
            page = pdf_page(sources['datasheet']['path'], r['pinout_pdf_page_1_based'])
            assert row['source_name'] in flat(page)
            path, pointer = r['af_pointer'].split('#')
            af = at_pointer(current_yaml(path), pointer)
            assert all(af[k] == r[k] for k in ['peripheral', 'pin', 'signal', 'af', 'source_macro', 'source_line'])


def command_async_sources():
    fact = EVIDENCE['families']['CW32L012']
    assert 'async_master' not in fact
    assert {f for f, p in EVIDENCE['families'].items() if 'command_async_master' in p} == {'CW32L012'}
    block = fact['command_async_master']
    assert block['identity_authority'] == 'sources/evidence-sources.json'
    assert set(block['sources']) == {'sdk', 'selected_svd', 'device_pack', 'device_header', 'gpio_header',
                                     'i2c_sdk_header', 'sysctrl_header', 'i2c_sdk_source', 'datasheet', 'manual'}
    material = {}
    for role, binding in block['sources'].items():
        _, data = selected_source(binding)
        assert 'cw32l012' in binding['path'].lower()
        if binding['source_ref'].startswith('member:'):
            assert binding['parent_source_ref'] == block['sources']['sdk']['source_ref']
            material[role] = data.decode(errors='replace')
    for existing, role in [('manual', 'manual'), ('sdk_header', 'i2c_sdk_header')]:
        assert fact[existing] == {k: block['sources'][role][v] for k, v in [('filename', 'path'), ('sha256', 'sha256'), ('url', 'url')]}
    raw = command_pages(block)
    command_protocol(block, raw, material)
    svd = command_registers(fact, material)
    chips = command_instances(block, raw, material, svd)
    command_routes(block, material, chips)
    print('PASS CW32L012 command async sources: 10 locked originals, 32 page identities, 29 register layouts, 3 selections, 2 dedicated IRQs, exclusive gates/resets, 24 AF routes and 8 exact-package example routes')


def main():
    assert len(EVIDENCE['families']) == 13 and len(EVIDENCE['registers']) == 6
    classic_fields = {'FLT': 0, 'AA': 2, 'SI': 3, 'STO': 4, 'STA': 5, 'EN': 6}
    # Distinct own-manual status rows, including slave transitions after arbitration.
    status_meaning = {
        0x00: '总线错误', 0x08: '已发送起始信号', 0x10: '已发送重复起始信号',
        0x18: '已发送SLA+W，已接收ACK', 0x20: '已发送SLA+W，已接收NACK',
        0x28: '已接收ACK', 0x30: '已接收NACK', 0x38: '丢失仲裁',
        0x40: '已发送SLA+R，已接收ACK', 0x48: '已发送SLA+R，已接收NACK',
        0x50: '已接收数据字节，ACK已返回', 0x58: '已接收数据字节，NACK已返回',
        0x60: '已接收自身的SLA+W', 0x68: '丢失仲裁',
        0x70: '已接收广播地址', 0x78: '丢失仲裁',
        0x80: '前一次寻址使用自身从地址，已接收数据字节，已返回ACK',
        0x88: '前一次寻址使用自身从地址，已接收数据字节，已返回NACK',
        0x90: '前一次寻址使用广播地址，已接收数据字节，已返回ACK',
        0x98: '前一次寻址使用广播地址，已接收数据字节，已返回NACK',
        0xA0: '接收到停止条件或重复起始条件', 0xA8: '已接收自身的SLA+R',
        0xB0: '丢失仲裁', 0xB8: '已发送数据字节，已接收ACK',
        0xC0: '已发送数据字节，已接收NACK', 0xC8: '从机最后一个数据字节已被发送',
        0xF8: '无可用的相关状态信息',
    }
    for family, fact in EVIDENCE['families'].items():
        for source in ['manual', 'sdk_header']:
            assert digest((SOURCES / fact[source]['filename']).read_bytes()) == fact[source]['sha256'], (family, source)
        ir = json.loads((ROOT / f'cw32-data/data/registers/{fact["register_version"]}.json').read_text())
        control = pages(fact, 'control_pages')
        status = pages(fact, 'status_pages')
        if family != 'CW32L012':
            for value, meaning in status_meaning.items():
                rows = [flat(line) for line in status.splitlines() if re.search(rf'\b{value:02X}H\b', line)]
                assert any(meaning in row for row in rows), (family, hex(value), meaning)
            assert set(EVIDENCE['registers'][fact['register_version']]['variants'].values()) == set(status_meaning)
            assert 'Reset value: 0x0000 0000' in control
            for name, bit in classic_fields.items():
                f = field(ir, 'CR', name)
                assert f['bit_offset'] == bit and f['bit_size'] == 1, (family, name)
                assert re.search(rf'\b{bit}\s+{name}\s+', control), (family, name)
            assert '在SI被清除后' in flat(control), family
            for name, offset in [('SCLINSRC', 8), ('SDAINSRC', 11)]:
                if fact['gpio_input_selectors']:
                    f = field(ir, 'CR', name)
                    assert f['bit_offset'] == offset and f['bit_size'] == 3
                    assert f'000：I2C_{"SCL" if offset == 8 else "SDA"}引脚' in flat(control)
                else:
                    assert name not in control
            assert field(ir, 'STAT', 'STAT')['enum'] == 'Status'
            print(f'PASS {family}: own STAT table, CR fields, SI release and GPIO-source differences')
        else:
            command = flat(pages(fact, 'command_pages'))
            for encoding, text in [('000', '向总线发送一个字节数据'), ('001', '从总线接收'),
                                   ('010', '在总线上产生STOP信号'), ('011', '从总线接收'),
                                   ('100', '在总线上生成（Repeated）START信号'),
                                   ('101', '在总线上生成（Repeated）START信号')]:
                assert encoding + '：' + text in command, encoding
            assert '不存入MRDR寄存器' in command and '期待从总线收到NACK' in command
            assert field(ir, 'MTDR', 'CMD')['enum'] == 'Command'
            micr = pages(fact, 'micr_pages')
            assert 'Reset value: 0x0000 7F04' in micr
            assert re.search(r'7:0\s+RFU\s+-\s+保留位，请保持默认值', micr)
            assert micr.count('W1：无功能') == 7
            assert '先通过MCR0.TXFIFORST复位TXFIFO后' in flat(micr)
            for bit, name in enumerate(fact['clearable_fields'], start=8):
                assert re.search(rf'\b{bit}\s+{name}\s+R1W0', micr), name
                assert field(ir, 'MICR', name)['bit_offset'] == bit
            for name, bit in [('MEN', 0), ('RESET', 1), ('TXFIFORST', 8), ('RXFIFORST', 9)]:
                assert re.search(rf'\b{bit}\s+{name}\s+', control), name
                assert field(ir, 'MCR0', name)['bit_offset'] == bit
            assert '00：PCLK' in flat(control)
            for name, bit in [('TXE', 0), ('RXNE', 1), ('MSTBUSY', 24), ('BUSBUSY', 25)]:
                assert re.search(rf'\b{bit}\s+{name}\s+RO', status), name
                assert field(ir, 'MISR', name)['bit_offset'] == bit
            receive = pages(fact, 'receive_pages')
            assert re.search(r'14\s+EMPTY\s+RO', receive)
            assert field(ir, 'MRDR', 'EMPTY')['bit_offset'] == 14
            print('PASS CW32L012: own command, control/status, atomic receive and MICR R1W0 semantics')
    for version, fact in EVIDENCE['registers'].items():
        ir = json.loads((ROOT / f'cw32-data/data/registers/{version}.json').read_text())
        variants = ir['enum/' + fact['enum']]['variants']
        assert {v['name']: v['value'] for v in variants} == fact['variants'], version
        pac = (ROOT / f'cw32-metapac/src/peripherals/{version}.rs').read_text()
        assert re.search(rf'pub (?:enum|struct) {fact["enum"]}\b', pac), version
        for name in fact['variants']:
            rust = ''.join(word.title() for word in name.split('_'))
            assert re.search(rf'\b{rust}(?:\s*:\s*Self)?\s*=', pac), (version, rust)
    authored = yaml.safe_load((ROOT / 'cw32-data/register-writes.yaml').read_text())
    micr = authored['registers']['i2c_cw32l012_v1'][0]
    assert micr['register'] == micr['fieldset'] == 'MICR' and micr['block'] == 'I2C'
    assert micr['reset_value'] == micr['write_noop'] == 0x7f04
    assert micr['zero_to_clear_fields'] == EVIDENCE['families']['CW32L012']['clearable_fields']
    projected = json.loads((ROOT / 'cw32-data/data/register-writes/i2c_cw32l012_v1.json').read_text())
    assert projected['registers']['i2c_cw32l012_v1'][0] == micr
    pac = (ROOT / 'cw32-metapac/src/peripherals/i2c_cw32l012_v1.rs').read_text()
    code = pac.split('impl regs::Micr {', 1)[1]
    for method in ['reset_value', 'write_noop']:
        assert re.search(rf'pub const fn {method}\(\) -> Self\s*\{{\s*Self\(32516\)', code), method
    assert re.search(r'impl Default for Micr.*?(?:Self|Micr)\(0\)', pac, re.S)
    print('PASS all 6 typed PAC variants, 13 source families, explicit MICR seed and unchanged zero Default')
    classic_async_sources()
    command_async_sources()


if __name__ == '__main__':
    main()
