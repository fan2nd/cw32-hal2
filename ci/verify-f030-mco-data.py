#!/usr/bin/env python3
"""Verify exact four-part x030 MCO sources and all 52 active data projections; never execute HAL code."""
import argparse
import sys
import hashlib
import json
from pathlib import Path
import re
import zipfile
import yaml

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tests"))
from current_targets import retired_targets
PARTS = {
    'CW32F030C8T7': ('CW32F030', 'LQFP48', '29', 'docs/f030-mco-qualification.json', 364, 55),
    'CW32F030K8T7': ('CW32F030', 'LQFP32', '18', 'docs/f030k8t7-mco-qualification.json', 357, 56),
    'CW32F030K8U7': ('CW32F030', 'QFN32', '18', 'docs/f030k8u7-mco-qualification.json', 541, 37),
    'CW32A030C8T7': ('CW32A030', 'LQFP48', '29', 'docs/a030c8t7-mco-qualification.json', 364, 55),
}
ROUTE = {'pin': 'PA8', 'signal': 'MCO', 'af': 4}
DIVISORS = [[0, 1], [1, 2], [2, 8], [3, 64], [4, 128], [5, 256], [6, 512], [7, 1024]]
FACTS = {
    'disabled_source': 0, 'hclk_source': 1, 'pclk_source': 2, 'divisors': DIVISORS,
    'maximum_output_hz': 20_000_000, 'supply_mv': [2400, 5500], 'temperature_c': [-40, 105],
    'maximum_load_pf': 50, 'gpio_speed': 1, 'fmax_condition_duty_percent': [45, 55],
    'fmax_condition_edge_sum_period_ratio': [2, 3], 'electrical_values_measured': False,
}
RM = 'CW32x030_UserManual_CN_V2.5.pdf'
PDFS = {
    RM: ('1afd49261f0f0689af8cb8ebf1b0ac1c00e3209b20d3c722106707ff4a10bdd2',
         '23935a54e976c25acfacd689818f272db88b579fc8f15363c2732c12da8773fe', ['CW32F030', 'CW32A030']),
    'CW32F030_DataSheet_CN_V1.9.pdf': ('04ef91434320e5d05a3b6690fead0fedb31a7d9bad28c96b655a6e13a22e46b2',
         '7c4d70098481fb58e0924901dbbe24cb88cf445689efaf718410617b37f4e5bf', ['CW32F030']),
    'CW32A030_DataSheet_CN_V1.1.pdf': ('690433f36376352ee342ae3e2aa76786f2e7e59892a2519411f2754ac32c33a9',
         '87c782da759a6893bca9f773a474a7b0445f77cd51d86d9cb2d2d8b8de8d05b9', ['CW32A030']),
}
FAMILIES = {
    'CW32F030': ('CW32F030_DataSheet_CN_V1.9.pdf', 'cw32-data/pinouts/cw32f030.yaml',
                 'cw32-data/af/cw32f030.yaml', 26, 29, 38, 51, '7-27'),
    'CW32A030': ('CW32A030_DataSheet_CN_V1.1.pdf', 'cw32-data/pinouts/cw32a030.yaml',
                 'cw32-data/af/cw32a030.yaml', 23, 26, 35, 48, '7-26'),
}
EXTENDED_RM_PAGES = [61, 69, 82, 85, 94, 147, 152, 153, 154, 155]
EXTENDED_DS_PAGES = {
    'CW32F030': [24, 26, 29, 32, 38, 50, 51, 74, 75, 76],
    'CW32A030': [22, 23, 26, 29, 35, 47, 48, 62, 63],
}
SDK = ('CW32F030_StandardPeripheralLib_V2.2.zip', '7c431df43d7075b817a51d818ea4c9aba55f83b7c9c77bf0065976758954b780')
MEMBERS = [
    ('cw32f030/Libraries/inc/cw32f030_gpio.h', '3ffe458a615f2dd099c6a14283e0438cfece2260444f4167a4c05c62930abe2e', [837, 837]),
    ('cw32f030/Libraries/inc/cw32f030_rcc.h', 'de32996b8f852c456ada827bc5c13fe64a29fc12115489d98e2d3ddf6133d184', [402, 440]),
    ('cw32f030/Libraries/src/cw32f030_rcc.c', 'dbb3c80fa07af902f704c7e812d3b04d8819548736eb1163fe9feaae1d1b4370', [906, 914]),
]


def unique(pairs):
    result = {}
    for key, value in pairs:
        assert key not in result, ('duplicate key', key)
        result[key] = value
    return result


class StrictLoader(yaml.SafeLoader):
    pass


def mapping(loader, node):
    loader.flatten_mapping(node)
    return unique((loader.construct_object(k, deep=True), loader.construct_object(v, deep=True))
                  for k, v in node.value)


StrictLoader.add_constructor(yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG, mapping)


def load(path):
    if path.suffix == '.json':
        return json.loads(path.read_text(), object_pairs_hook=unique)
    return yaml.load(path.read_text(), Loader=StrictLoader)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def sha(path):
    return digest(path.read_bytes())


def canonical(value):
    return digest(json.dumps(value, sort_keys=True, separators=(',', ':')).encode())


def exact_keys(value, keys, label):
    assert isinstance(value, dict) and set(value) == set(keys.split()), (label, 'exact keys')


def one(rows, label):
    assert len(rows) == 1, (label, 'requires exactly one', len(rows))
    return rows[0]


def within(root, relative):
    path = Path(relative)
    assert not path.is_absolute() and '..' not in path.parts, ('relative source path', relative)
    return root / path


def check_facts(facts):
    assert canonical(facts) == canonical(FACTS), 'complete exact MCO source/electrical record'
    assert len(facts['divisors']) == 8 and {v[0] for v in facts['divisors']} == set(range(8))
    assert len({facts[k] for k in ('disabled_source', 'hclk_source', 'pclk_source')}) == 3
    assert facts['electrical_values_measured'] is False
    assert facts['maximum_output_hz'] == 20_000_000


def check_extended_pages(part, pages, ds):
    """Read each new part's own package, electrical and address evidence."""
    family, package, _, _, _, theta = PARTS[part]
    if family == 'CW32F030':
        bonds = pages[(ds, 24)]
        for pattern in [r'3\s+-\s+-\s+-\s+-\s+PC14', r'4\s+-\s+-\s+-\s+-\s+PC15',
                        r'5\s+2\s+2\s+2\s+19\s+PF00', r'6\s+3\s+3\s+3\s+20\s+PF01']:
            assert re.search(r'^\s*' + pattern + r'\s+I/O\s+TC', bonds, re.M)
        assert all(column in bonds for column in ['LQFP48', 'LQFP32', 'TSSOP20', 'QFN32', 'QFN20'])
        order_code = pages[(ds, 75)]
        for text in ['K=32 pins', '8=64K字节Flash', 'T=LQFP', 'U/V=QFN', '7=-40℃~105℃']:
            assert text in order_code
        order, address, dc, thermal = [pages[(ds, n)] for n in [76, 32, 50, 74]]
        assert '表 7-26' in dc and '表 8-6' in thermal
    else:
        bonds = pages[(ds, 22)]
        for position, pin in [('3', 'PC14'), ('4', 'PC15'), ('5', 'PF00'), ('6', 'PF01')]:
            assert re.search(r'^\s*' + position + r'\s+' + pin + r'\s+I/O\s+TC', bonds, re.M)
        assert 'LQFP48' in bonds and '表 5-2' in bonds
        order, address, dc, thermal = [pages[(ds, n)] for n in [63, 29, 47, 62]]
        assert '表 7-25' in dc and '表 8-2' in thermal
    assert re.search(r'^\s*' + part + r'\s+Tray\b', order, re.M), 'own exact orderable part'
    for pattern in [r'0x0000 0000\s*-\s*0x0000 FFFF\s+64KB',
                    r'0x2000 0000\s*-\s*0x2000 1FFF\s+8KB',
                    r'0x4001 0000\s*-\s*0x4001 03FF\s+1KB\s+SYSCTRL',
                    r'0x4800 0000\s*-\s*0x4800 03FF\s+1KB\s+GPIOA',
                    r'0x4800 0800\s*-\s*0x4800 0BFF\s+1KB\s+GPIOC']:
        assert re.search(pattern, address), (part, 'own address map', pattern)
    for text in ['±8mA', '±20mA', 'ΣIVDD', 'ΣIVSS', 'I OH(max)', 'I OL(max)', '100mA']:
        assert text in dc, (part, 'own DC/current qualification', text)
    assert 'TJ max = TA max +（PD max×ΘJA）' in thermal
    assert 'PDmax = PINTmax + PI/Omax' in thermal
    assert re.search(r'\b' + str(theta) + r'(?:\s+℃ /W)?\s+' + re.escape(package) + r'\s+–', thermal)

    registers = pages[(RM, 69)]
    assert 'SYSCTRL_BASE = 0x4001 0000' in registers
    for register, offset in [('LSE', '24'), ('MCO', '70'), ('AHBEN', '30'), ('AHBRST', '40')]:
        assert re.search(r'SYSCTRL_' + register + r'\s+SYSCTRL_BASE \+ 0x' + offset, registers)
    for page, register, offset, reset in [(82, 'AHBEN', '30', '0000'), (85, 'AHBRST', '40', '0277')]:
        text = pages[(RM, page)]
        assert 'SYSCTRL_' + register in text
        assert re.search(r'Address offset: 0x' + offset + r'\s+Reset value: 0x0000 ' + reset, text)
        assert re.search(r'4\s+GPIOA\s+RW', text) and re.search(r'6\s+GPIOC\s+RW', text)
    gpio = pages[(RM, 152)]
    assert 'GPIOA_BASE = 0x4800 0000' in gpio and 'GPIOC_BASE = 0x4800 0800' in gpio
    for register, offset in [('DIR', '00'), ('SPEED', '08'), ('PDR', '0C'), ('AFRH', '14'), ('ANALOG', '1C')]:
        assert re.search(r'GPIOx_' + register + r'\s+GPIOx_BASE \+ 0x' + offset, gpio)
    speed = pages[(RM, 153)].split('9.6.3   GPIOx_SPEED', 1)[1]
    assert re.search(r'Address offset: 0x08\s+Reset value: 0x0000 0000', speed)
    assert re.search(r'15:0\s+RW', speed) and '1：高输出速度' in speed and 'y=0 ~ 15' in speed
    afrh = pages[(RM, 154)].split('9.6.6   GPIOx_AFRH', 1)[1]
    assert re.search(r'Address offset: 0x14\s+Reset value: 0x0000 0000', afrh)
    assert 'y = 8 ~ 15' in afrh and '0100：AF4' in afrh and re.search(r'\n\s*3:0\s*\n', afrh)
    analog = pages[(RM, 155)].split('9.6.8   GPIOx_ANALOG', 1)[1]
    assert 'Address offset: 0x1C' in analog and '0：将端口配置为数字功能' in analog


def check_sources(part, profile, proof, vendor, authority):
    family, package, position, _, power, theta = PARTS[part]
    ds, _, _, pin_page, af_page, envelope_page, io_page, io_table = FAMILIES[family]
    legacy = part == 'CW32F030C8T7'
    source_pages = [(RM, [61, 94, 147] if legacy else EXTENDED_RM_PAGES),
                    (ds, [26, 29, 38, 51] if legacy else EXTENDED_DS_PAGES[family])]
    sources = profile['sources']
    exact_keys(sources, 'pdfs sdk', 'MCO sources')
    assert len(sources['pdfs']) == 2
    pages = {}
    originals = []
    for source, (name, expected_pages) in zip(sources['pdfs'], source_pages):
        expected_hash, expected_text_hash, scope = PDFS[name]
        exact_keys(source, 'source_ref path sha256 text_path text_sha256 pages', name)
        assert source['path'] == name and source['source_ref'] == 'vendor:' + name
        assert source['sha256'] == expected_hash == sha(within(vendor, name))
        a = one([a for a in authority['artifacts'] if a.get('id') == source['source_ref']], name)
        assert (a['path'], a['sha256'], a['provenance']['status']) == (name, expected_hash, 'selected')
        assert a['provenance']['chip_scope'] == scope
        assert source['text_path'] == a['text']['path'] == name.removesuffix('.pdf') + '.txt'
        assert source['text_sha256'] == a['text']['sha256'] == expected_text_hash == sha(within(vendor, source['text_path']))
        text_pages = within(vendor, source['text_path']).read_text().split('\f')
        if name == RM:
            assert digest(text_pages[0].encode()) == '7f55b61c0e816050f3215ea6d1346ad65bf055308d65483bf233f16e519333c6'
            assert 'CW32F030/CW32A030' in text_pages[0], 'shared RM unnumbered cover must name both families'
        assert [p['pdf_page_1_based'] for p in source['pages']] == expected_pages
        for page in source['pages']:
            exact_keys(page, 'pdf_page_1_based printed_page text_sha256', name + ' page')
            number = page['pdf_page_1_based']
            assert type(number) is int and 0 < number <= a['provenance']['pdf_page_count']
            assert page['printed_page'] == number - 1
            page_text = text_pages[number - 1]
            assert digest(page_text.encode()) == page['text_sha256'], (name, number, 'extracted page')
            pages[(name, number)] = page_text
        originals.append({'path': name, 'sha256': expected_hash, 'chip_scope': scope,
                          'role': 'shared_x030_reference_manual' if name == RM else 'own_family_datasheet'})
    sdk = sources['sdk']
    exact_keys(sdk, 'source_ref path sha256 members', 'SDK')
    assert (sdk['path'], sdk['sha256']) == SDK and sdk['source_ref'] == 'vendor:' + SDK[0]
    assert sha(within(vendor, sdk['path'])) == sdk['sha256']
    a = one([a for a in authority['artifacts'] if a.get('id') == sdk['source_ref']], 'SDK source')
    assert a['path'] == SDK[0] and a['sha256'] == SDK[1]
    assert a['provenance']['status'] == 'selected' and a['provenance']['chip_scope'] == ['CW32F030']
    assert len(sdk['members']) == 3
    member_text = {}
    with zipfile.ZipFile(within(vendor, sdk['path'])) as archive:
        for member, (path, expected_hash, lines) in zip(sdk['members'], MEMBERS):
            exact_keys(member, 'source_ref path archive_member sha256 lines_1_based', path)
            assert member['source_ref'] == 'member:' + path and member['path'] == path
            assert member['archive_member'] == path.removeprefix('cw32f030/')
            assert member['lines_1_based'] == lines
            locked = one([m for m in a['members'] if m['path'] == path], path)
            assert locked['members'] == [member['archive_member']]
            raw = within(vendor, path).read_bytes()
            assert digest(raw) == member['sha256'] == expected_hash == locked['sha256']
            assert archive.namelist().count(member['archive_member']) == 1
            assert archive.read(member['archive_member']) == raw, (path, 'archive/member byte equality')
            all_lines = raw.decode('utf-8').splitlines()
            assert 1 <= lines[0] <= lines[1] <= len(all_lines)
            member_text[path] = '\n'.join(all_lines[lines[0] - 1:lines[1]])
    originals.append({'path': SDK[0], 'sha256': SDK[1], 'chip_scope': ['CW32F030'],
                      'role': 'supplemental_shared_ip_corroboration' if family == 'CW32A030' else 'own_family_sdk_corroboration'})
    rm = pages[(RM, 94)]
    assert re.search(r'Address offset:\s*0x70\s+Reset value:\s*0x0000 0000', rm)
    assert re.search(r'31:7\s+RFU', rm) and re.search(r'6:4\s+DIV\s+RW', rm)
    assert re.search(r'3:0\s+SOURCE\s+RW', rm)
    assert [[int(code, 2), int(div)] for code, div in re.findall(r'([01]{3})：([0-9]+) 分频', rm)] == DIVISORS
    for code, source in [(0, '无输出'), (1, 'HCLK'), (2, 'PCLK')]:
        assert f'{code:04b}：{source}' in rm
    rm_route = one([line for line in pages[(RM, 147)].splitlines() if re.match(r'\s*PA08\s', line)], 'RM PA8')
    assert rm_route.split() == ['PA08', 'GPIO', 'UART1_TXD', 'BTIM2_TOGN', 'MCO_OUT', 'LVD_OUT', 'GTIM3_ETR', 'ATIM_CH1A']
    ds_route = one([line for line in pages[(ds, af_page)].splitlines() if re.match(r'\s*PA08\s', line)], family + ' DS PA8')
    assert ds_route.split() == ['PA08', 'UART1_TXD', 'BTIM2_TOGN', 'MCO_OUT', 'LVD_OUT', 'GTIM3_ETR', 'ATIM_CH1A']
    pin_pattern = (r'^\s*29\s+18\s+-\s+18\s+-\s+PA08\s+I/O\s+TC' if family == 'CW32F030'
                   else r'^\s*29\s+PA08\s+I/O\s+TC')
    assert re.search(pin_pattern, pages[(ds, pin_page)], re.M)
    if not legacy:
        check_extended_pages(part, pages, ds)
    gpio = member_text[MEMBERS[0][0]]
    assert re.search(r'^#define\s+PA08_AFx_MCO\(\)\s+\(CW_GPIOA->AFRH_f.AFR8\s*=\s*4\)', gpio, re.M)
    rcc = member_text[MEMBERS[1][0]]
    macros = re.findall(r'^#define\s+RCC_MCO_DIV([0-9]+)\s+(.+)$', rcc, re.M)
    assert len(macros) == 8
    for (div, expression), (code, expected_div) in zip(macros, DIVISORS):
        assert int(div) == expected_div
        assert expression.strip() == ('(0x00U)' if code == 0 else f'((uint8_t)(0x{code:02X}U << SYSCTRL_MCO_DIV_Pos))')
    for label, code in [('NONE', 0), ('HCLK', 1), ('PCLK', 2)]:
        values = re.findall(r'^#define\s+RCC_MCO_SRC_' + label + r'\s+\(0x([0-9A-F]+)U\)', rcc, re.M)
        assert len(values) == 1 and int(values[0], 16) == code
    sdk_c = member_text[MEMBERS[2][0]]
    assert 'CW_SYSCTRL->MCO    = (uint32_t)(Source | Div);' in sdk_c
    assert 'GPIOx_AFRH_AFR8_Msk' in sdk_c and '(0x4U << GPIOx_AFRH_AFR8_Pos)' in sdk_c
    io = pages[(ds, io_page)]
    assert '表 ' + io_table in io and 'GPIOx_SPEED.PINy' in io
    # The selected 20 MHz policy is the minimum of all three High-speed rows.
    expected_rows = [
        ('30', r'VDDIOx ≥ 2\.7V', ['50', '5', '5']),
        ('50', r'VDDIOx ≥ 2\.7V', ['30', '8', '8']),
        ('50', r'2\.4V ≤ VDDIOx ＜ 2\.7V', ['20', '12', '12']),
    ]
    for load, voltage, values in expected_rows:
        assert re.findall(r'CL = ' + load + r'pF，' + voltage + r'\s+-\s+([0-9]+)', io) == values
    assert re.search(r'\n\s*1\s+tf\(IO\)out', io)
    assert '该表基于设计和仿真的数据，未实测' in io
    assert '( tr + tf ) ≤ 2/3T' in io and '(45-55%) when loaded by CL' in io
    envelope = pages[(ds, envelope_page)]
    assert re.search(r'VDD\s+标准工作电压\s+-\s+1\.65\s+5\.5', envelope)
    assert re.search(re.escape(package) + r'\s+-\s+' + str(power) + r'\b', envelope)
    assert re.search(r'最大功率耗散\s+-40\s+105', envelope)
    assert re.search(r'低功率耗散\s*2\s+-40\s+125', envelope)
    assert re.search(r'TJ\s+结温范围\s+温度标号 7\s+-40\s+125', envelope)
    assert 'TJ 不超过 TJmax' in envelope
    sf = proof['source_facts']
    exact_keys(sf, 'register pin high_speed_table_rows normal_operating_envelope fmax_conditions gpio', 'source facts')
    assert sf['register'] == {'owner': 'SYSCTRL', 'offset': 112, 'reset': 0,
        'source_bit_offset': 0, 'source_bit_size': 4, 'disabled': 0, 'hclk': 1, 'pclk': 2,
        'div_bit_offset': 4, 'div_bit_size': 3, 'divisors': DIVISORS,
        'reserved': 'bits31:7 retain incoming state; source/DIV-only RMW; inherited nonzero SOURCE rejected'}
    assert sf['pin'] == {'part': part, 'family': family, 'package': package,
        'owner': 'SYSCTRL', 'owner_address': 0x40010000,
        'owner_registers': {'block': 'SYSCTRL', 'kind': 'sysctrl', 'version': 'v1'},
        'pin_position': position, 'route': ROUTE, 'route_gpio_register': 'AFRH', 'route_gpio_field': 'AFR8'}
    assert sf['fmax_conditions']['data_measured'] is False
    assert sf['normal_operating_envelope']['low_power_125C_extension_admitted'] is False
    assert sf['fmax_conditions'] == {'loaded_duty_percent': [45, 55],
        'rise_plus_fall_maximum_period_ratio': [2, 3], 'data_measured': False}
    assert sf['normal_operating_envelope'] == {'supply_mv': [1650, 5500], 'temperature_c': [-40, 105],
        package + '_PD_mw_at_TA105': power, 'maximum_junction_c': 125, 'low_power_125C_extension_admitted': False}
    assert sf['high_speed_table_rows'] == [
        {'speed': 1, 'load_pf': 30, 'supply_min_mv': 2700, 'maximum_hz': 50000000, 'rise_ns_max': 5, 'fall_ns_max': 5},
        {'speed': 1, 'load_pf': 50, 'supply_min_mv': 2700, 'maximum_hz': 30000000, 'rise_ns_max': 8, 'fall_ns_max': 8},
        {'speed': 1, 'load_pf': 50, 'supply_min_mv': 2400, 'supply_max_exclusive_mv': 2700,
         'maximum_hz': 20000000, 'rise_ns_max': 12, 'fall_ns_max': 12},
    ]
    assert sf['gpio'] == {'owner': 'GPIOA', 'address': 0x48000000, 'registers': {'kind': 'gpio', 'version': 'v1', 'block': 'GPIO'},
        'af_register': 'AFRH', 'af_offset': 20, 'af_field': 'AFR8', 'af_bit_offset': 0, 'af_bit_size': 4,
        'speed_register': 'SPEED', 'speed_offset': 8, 'speed_field': 'PIN8', 'speed_bit_offset': 8, 'speed_bit_size': 1}
    conditions = proof['conditions']
    exact_keys(conditions, 'numerical_rate_check_is_sufficient_for_waveform_qualification load_policy voltage_policy temperature_policy board_declaration waveform_policy characterization', 'qualification conditions')
    assert conditions['numerical_rate_check_is_sufficient_for_waveform_qualification'] is False
    for key, terms in {
        'load_policy': ['conservative', '50 pF', 'trace, receiver and probe'],
        'voltage_policy': ['actual PA8 I/O supply', '2400..5500mV', 'chosen frozen source'],
        'temperature_policy': ['-40..105C', 'power/junction', 'no low-power 125C'],
        'board_declaration': ['not recovered from', 'earlier RCC'],
        'waveform_policy': ['45..55%', 'tr+tf <= 2T/3', 'not established', 'application qualification'],
        'characterization': ['design/simulation', 'unmeasured'],
    }.items():
        assert all(term in conditions[key] for term in terms), ('qualification condition', key)
    if not legacy:
        assert package in conditions['temperature_policy'] and f'thetaJA is {theta}C/W' in conditions['temperature_policy']
    if family == 'CW32A030':
        for term in ['own A030 DS', 'shared x030 RM', 'primary authority', 'F030 SDK', 'supplemental shared-IP',
                     'not an independent A030 SDK', 'unnumbered cover PDF page 1']:
            assert term.casefold() in conditions['characterization'].casefold(), ('A030 source role', term)
    return originals


def check_ir(ir, block, register, offset, expected_fields):
    item = one([r for r in ir['block/' + block]['items'] if r['name'] == register], register)
    assert item['byte_offset'] == offset and item.get('array') is None
    assert item.get('bit_size', 32) == 32 and item.get('access', 'ReadWrite') == 'ReadWrite'
    fields = ir['fieldset/' + item['fieldset']]['fields']
    assert len({f['name'] for f in fields}) == len(fields)
    if register == 'MCO':
        assert {f['name'] for f in fields} == {'SOURCE', 'DIV'}
    for name, start, size in expected_fields:
        field = one([f for f in fields if f['name'] == name], register + '.' + name)
        assert (field['bit_offset'], field['bit_size']) == (start, size)
        assert field.get('enum') is None and field.get('array') is None


def selection_universe(proofs):
    """Bind every proof's universe to current authoritative input/part selections."""
    by_part = {}
    for path in sorted((ROOT / 'cw32-data/inputs').glob('*.yaml')):
        manifest = load(path)
        if manifest.get('quarantine'):
            continue
        names = [chip['name'] for chip in manifest['chips']]
        if manifest.get('parts_catalog'):
            catalog = load(within(ROOT, manifest['parts_catalog']))
            assert catalog['schema_version'] == 1
            names.extend(part['name'] for part in catalog['parts'] if part['family'] == manifest['line'])
        for name in names:
            assert name not in by_part, ('duplicate current selection', name)
            by_part[name] = manifest
    assert len(by_part) == 54 and set(PARTS) <= set(by_part)
    assert {'CW32F030', 'CW32F030C8', 'CW32F030K8', 'CW32A030',
            'CW32F030F8V7', 'CW32F030F6P7'} <= set(by_part) - set(PARTS)
    for part, proof in proofs.items():
        scope = proof['projection_scope']
        exact_keys(scope, 'selection_count parts', part + ' projection scope')
        assert scope['selection_count'] == 54 and len(scope['parts']) == len(set(scope['parts'])) == 54
        assert scope['parts'] == sorted(by_part), (part, 'current complete 54-selection universe')
    # Historical evidence keeps all 54 identities; only current outputs retire these exact two.
    assert retired_targets() <= set(by_part)
    return {name: manifest for name, manifest in by_part.items() if name not in retired_targets()}


def check_package(part, chip, core, owner):
    family, package_name, position, _, _, _ = PARTS[part]
    ds, pinout_path, _, pin_page, _, _, _, _ = FAMILIES[family]
    pinout = load(ROOT / pinout_path)
    assert pinout['family'] == family and pinout['source']['sha256'] == PDFS[ds][0]
    reviewed = one([p for p in pinout['packages'] if p['name'] == part], part + ' reviewed package')
    assert reviewed['package'] == reviewed['table_column'] == package_name
    assert (reviewed['flash_bytes'], reviewed['ram_bytes']) == (65536, 8192)
    assert part in reviewed['documented_orderable_parts']
    package = one(chip['packages'], part + ' package')
    assert package == {'name': part, 'package': package_name, 'pins': reviewed['pins']}, 'complete ordered physical pin records'
    assert chip['memory'] == [[{'name': 'FLASH', 'kind': 'flash', 'address': 0, 'size': 65536},
                               {'name': 'RAM', 'kind': 'ram', 'address': 0x20000000, 'size': 8192}]]
    pin_names = sorted({signal for pin in reviewed['pins'] for signal in pin['signals']
                        if re.fullmatch(r'P[A-Z](?:[0-9]|1[0-5])', signal)})
    assert core['pins'] == [{'name': name} for name in pin_names], 'complete ordered core pin records'
    row = one([r for r in pinout['table_rows'] if 'PA8' in r['signals']], part + ' PA8 table row')
    assert (row['source_name'], row['pin_type'], row['io_structure'], row['pdf_page_index']) == ('PA08', 'I/O', 'TC', pin_page - 1)
    assert row['positions'][package_name] == position
    if family == 'CW32F030':
        assert row['positions'] == {'LQFP48': '29', 'LQFP32': '18', 'TSSOP20': None, 'QFN32': '18', 'QFN20': None}
    pa8 = one([p for p in package['pins'] if 'PA8' in p['signals']], part + ' PA8 bond')
    assert pa8['position'] == position and pin_names.count('PA8') == 1
    if package_name == 'QFN32':
        assert one([p for p in package['pins'] if p['position'] == '0'], 'QFN32 exposed pad')['signals'] == ['VSS']
    expected_oscillators = []
    for pin, signal, alias, pin_position in [
            ('PF0', 'HSE_IN', 'OSC_IN', '5' if package_name == 'LQFP48' else '2'),
            ('PF1', 'HSE_OUT', 'OSC_OUT', '6' if package_name == 'LQFP48' else '3'),
            ('PC14', 'LSE_IN', 'OSC32_IN', '3' if package_name == 'LQFP48' else None),
            ('PC15', 'LSE_OUT', 'OSC32_OUT', '4' if package_name == 'LQFP48' else None)]:
        row = one([r for r in pinout['table_rows'] if pin in r['signals']], part + ' ' + pin)
        assert alias in row['signals'] and row['positions'][package_name] == pin_position
        bonds = [p for p in package['pins'] if pin in p['signals']]
        if pin_position is None:
            assert pin not in pin_names and bonds == []
            assert not any(p['pin'] == pin for peripheral in core['peripherals'] for p in peripheral.get('pins', []))
        else:
            assert one(bonds, pin + ' bond')['position'] == pin_position and pin_names.count(pin) == 1
            expected_oscillators.append(('SYSCTRL', {'pin': pin, 'signal': signal}))
    actual_oscillators = [(p['name'], pin) for p in core['peripherals'] for pin in p.get('pins', [])
                          if pin['signal'] in {'HSE_IN', 'HSE_OUT', 'LSE_IN', 'LSE_OUT'}]
    order = lambda item: (item[0], item[1]['pin'], item[1]['signal'])
    assert sorted(actual_oscillators, key=order) == sorted(expected_oscillators, key=order), 'exact global oscillator owner/routes'
    assert owner['pins'] == sorted(owner['pins'], key=lambda pin: (pin['pin'], pin['signal'], pin.get('af', -1)))
    electrical = load(ROOT / 'cw32-data/electrical.yaml')['profiles'][family]['clock_limits']
    assert owner['clock_limits']['lse'] == electrical['lse'] == {'pin_lock': False, 'pin_lock_requires_enable_lock': False}
    gpioc = one([p for p in core['peripherals'] if p['name'] == 'GPIOC'], part + ' die GPIOC')
    assert gpioc['address'] == 0x48000800
    assert gpioc['registers'] == {'kind': 'gpioc', 'version': 'v1', 'block': 'GPIOC'}
    assert gpioc['gpio'] == {'output_mask': 0xE000, 'pull_down_mask': 0xE000}


def check_projection(proofs, data):
    by_part = selection_universe(proofs)
    entries = list((data / 'chips').iterdir())
    assert all(p.is_file() for p in entries), 'unexpected generated chip directory entry'
    assert sorted(p.name for p in entries) == sorted(name + '.json' for name in by_part), 'complete exact 52 active selections required'
    projected = []
    for name, manifest in sorted(by_part.items()):
        path = data / 'chips' / (name + '.json')
        chip = load(path)
        assert chip['name'] == name and chip['line'] == manifest['line'] and chip['family'] == manifest['family']
        assert len(chip['cores']) == 1
        core = chip['cores'][0]
        assert len({p['name'] for p in core['peripherals']}) == len(core['peripherals'])
        owners = [p for p in core['peripherals'] if p.get('clock_limits') is not None]
        owner = one(owners, name + ' clock owner')
        assert owner['name'] == 'SYSCTRL'
        facts = [(p, p['clock_limits']['mco']) for p in core['peripherals']
                 if 'mco' in p.get('clock_limits', {})]
        routes = [(p, pin) for p in core['peripherals'] for pin in p.get('pins', []) if pin['signal'] == 'MCO']
        if name in PARTS:
            assert chip['line'] == PARTS[name][0] and len(facts) == len(routes) == 1
            assert facts[0][0] is owner and routes[0][0] is owner and routes[0][1] == ROUTE
            check_facts(facts[0][1])
            assert owner['address'] == 0x40010000 and owner['registers'] == {'kind': 'sysctrl', 'version': 'v1', 'block': 'SYSCTRL'}
            check_package(name, chip, core, owner)
            gpio = one([p for p in core['peripherals'] if p['name'] == 'GPIOA'], 'GPIOA')
            assert gpio['address'] == 0x48000000 and gpio['gpio']['output_mask'] & 256
            assert gpio['registers'] == {'kind': 'gpio', 'version': 'v1', 'block': 'GPIO'}
        else:
            assert facts == routes == [], (name, 'unqualified fact/route must be omitted')
        projected.append({'part': name, 'sha256': sha(path), 'mco': name in PARTS})
    assert sum(p['mco'] for p in projected) == 4 and sum(not p['mco'] for p in projected) == 48
    for suffix in ['yaml', 'json']:
        check_ir(load(data / ('registers/sysctrl_v1.' + suffix)), 'SYSCTRL', 'MCO', 112, [('SOURCE', 0, 4), ('DIV', 4, 3)])
        gpio = load(data / ('registers/gpio_v1.' + suffix))
        check_ir(gpio, 'GPIO', 'AFRH', 20, [('AFR8', 0, 4)])
        check_ir(gpio, 'GPIO', 'SPEED', 8, [('PIN8', 8, 1)])
    return projected


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--sources', type=Path, required=True, help='Directory of locked vendor PDFs, extracted text and SDK files')
    parser.add_argument('--data', type=Path, required=True, help='Complete generated cw32-data/data directory')
    parser.add_argument('--out', type=Path, required=True, help='Write the source/data audit receipt here')
    args = parser.parse_args()
    path = ROOT / 'cw32-data/mco-qualified.yaml'
    catalog = load(path)
    exact_keys(catalog, 'schema_version source_authority policies parts', 'catalog')
    assert catalog['schema_version'] == 1 and catalog['source_authority'] == 'sources/evidence-sources.json'
    assert set(catalog['parts']) == set(PARTS) and set(catalog['policies']) == {row[3] for row in PARTS.values()}
    proofs, qualifications = {}, []
    authority = load(ROOT / catalog['source_authority'])
    for part, (family, package, position, proof_relative, _, _) in PARTS.items():
        proof_path = ROOT / proof_relative
        assert catalog['policies'][proof_relative] == sha(proof_path)
        proof = load(proof_path)
        assert proof['schema_version'] == 1 and proof['part'] == part
        profile = catalog['parts'][part]
        exact_keys(profile, 'family package pin_position route facts sources', part + ' profile')
        exact_keys(profile['route'], 'pin signal af', part + ' route')
        assert canonical(profile) == canonical(proof['profile']) and profile['route'] == ROUTE
        assert (profile['family'], profile['package'], profile['pin_position']) == (family, package, position)
        check_facts(profile['facts'])
        originals = check_sources(part, profile, proof, args.sources, authority)
        proofs[part] = proof
        qualifications.append({'part': part, 'path': proof_relative, 'sha256': sha(proof_path),
                               'originals': originals})
    electrical = load(ROOT / 'cw32-data/electrical.yaml')
    assert all('mco' not in p['clock_limits'] for p in electrical['profiles'].values())
    for family, (_, _, af_path, _, _, _, _, _) in FAMILIES.items():
        candidate = load(ROOT / af_path)
        assert candidate['profile'] == family
        raw = one([p for p in candidate['unresolved'] if p['pin'] == 'PA8' and p['af'] == 4], family + ' raw AF evidence')
        assert raw['function'] == 'MCO' and raw['source_macro'] == 'PA08_AFx_MCO' and raw['source_line'] == 837
        assert (raw['gpio_register'], raw['gpio_field']) == ('AFRH', 'AFR8')
        assert all(p['signal'] != 'MCO' for p in candidate['routes'])
    check_ir(load(ROOT / 'cw32-data/registers/sysctrl_v1.yaml'), 'SYSCTRL', 'MCO', 112, [('SOURCE', 0, 4), ('DIV', 4, 3)])
    gpio = load(ROOT / 'cw32-data/registers/gpio_v1.yaml')
    check_ir(gpio, 'GPIO', 'AFRH', 20, [('AFR8', 0, 4)])
    check_ir(gpio, 'GPIO', 'SPEED', 8, [('PIN8', 8, 1)])
    projected = check_projection(proofs, args.data)
    receipt = {'schema_version': 1, 'status': 'passed_source_and_data_audit', 'scope': 'Exact four-part x030 MCO',
        'catalog': {'path': 'cw32-data/mco-qualified.yaml', 'sha256': sha(path)},
        'qualifications': qualifications,
        'validator': {'path': 'ci/verify-f030-mco-data.py', 'sha256': sha(Path(__file__))},
        'selections': projected, 'selection_count': len(projected),
        'qualified_count': sum(p['mco'] for p in projected), 'unqualified_count': sum(not p['mco'] for p in projected),
        'source_codes': [0, 1, 2], 'divisors': DIVISORS, 'conditional_unmeasured_policy_preserved': True,
        'hal_executed': False, 'hardware_waveform_validated': False,
        'acceptance_boundary': 'Source/data validation does not constitute final schema/runtime review, link validation or hardware acceptance.'}
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(receipt, indent=2) + '\n')
    print('x030 MCO own sources and all 52 active capability projections passed; 48 excluded, four exact capabilities.')


if __name__ == '__main__':
    main()
