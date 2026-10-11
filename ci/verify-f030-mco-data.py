#!/usr/bin/env python3
"""Verify own-source F030 MCO facts and the complete data projection; never execute HAL code."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import zipfile
import yaml

ROOT = Path(__file__).resolve().parents[1]
PART = 'CW32F030C8T7'
ROUTE = {'pin': 'PA8', 'signal': 'MCO', 'af': 4}
DIVISORS = [[0, 1], [1, 2], [2, 8], [3, 64], [4, 128], [5, 256], [6, 512], [7, 1024]]
FACTS = {
    'disabled_source': 0, 'hclk_source': 1, 'pclk_source': 2, 'divisors': DIVISORS,
    'maximum_output_hz': 20_000_000, 'supply_mv': [2400, 5500], 'temperature_c': [-40, 105],
    'maximum_load_pf': 50, 'gpio_speed': 1, 'fmax_condition_duty_percent': [45, 55],
    'fmax_condition_edge_sum_period_ratio': [2, 3], 'electrical_values_measured': False,
}
PDFS = [
    ('CW32x030_UserManual_CN_V2.5.pdf', '1afd49261f0f0689af8cb8ebf1b0ac1c00e3209b20d3c722106707ff4a10bdd2', [61, 94, 147]),
    ('CW32F030_DataSheet_CN_V1.9.pdf', '04ef91434320e5d05a3b6690fead0fedb31a7d9bad28c96b655a6e13a22e46b2', [26, 29, 38, 51]),
]
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


def check_sources(profile, proof, vendor, authority):
    sources = profile['sources']
    exact_keys(sources, 'pdfs sdk', 'MCO sources')
    assert len(sources['pdfs']) == 2
    pages = {}
    originals = []
    for source, (name, expected_hash, expected_pages) in zip(sources['pdfs'], PDFS):
        exact_keys(source, 'source_ref path sha256 text_path text_sha256 pages', name)
        assert source['path'] == name and source['source_ref'] == 'vendor:' + name
        assert source['sha256'] == expected_hash == sha(within(vendor, name))
        a = one([a for a in authority['artifacts'] if a.get('id') == source['source_ref']], name)
        assert (a['path'], a['sha256'], a['provenance']['status']) == (name, expected_hash, 'selected')
        assert 'CW32F030' in a['provenance']['chip_scope']
        assert source['text_path'] == a['text']['path'] == name.removesuffix('.pdf') + '.txt'
        assert source['text_sha256'] == a['text']['sha256'] == sha(within(vendor, source['text_path']))
        text_pages = within(vendor, source['text_path']).read_text().split('\f')
        assert [p['pdf_page_1_based'] for p in source['pages']] == expected_pages
        for page in source['pages']:
            exact_keys(page, 'pdf_page_1_based printed_page text_sha256', name + ' page')
            number = page['pdf_page_1_based']
            assert type(number) is int and 0 < number <= a['provenance']['pdf_page_count']
            assert page['printed_page'] == number - 1
            page_text = text_pages[number - 1]
            assert digest(page_text.encode()) == page['text_sha256'], (name, number, 'extracted page')
            pages[(name, number)] = page_text
        originals.append({'path': name, 'sha256': expected_hash})
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
            member_text[path] = raw.decode('utf-8')
    originals.append({'path': SDK[0], 'sha256': SDK[1]})
    rm = pages[(PDFS[0][0], 94)]
    assert re.search(r'Address offset:\s*0x70\s+Reset value:\s*0x0000 0000', rm)
    assert re.search(r'31:7\s+RFU', rm) and re.search(r'6:4\s+DIV\s+RW', rm)
    assert re.search(r'3:0\s+SOURCE\s+RW', rm)
    assert [[int(code, 2), int(div)] for code, div in re.findall(r'([01]{3})：([0-9]+) 分频', rm)] == DIVISORS
    for code, source in [(0, '无输出'), (1, 'HCLK'), (2, 'PCLK')]:
        assert f'{code:04b}：{source}' in rm
    rm_route = one([line for line in pages[(PDFS[0][0], 147)].splitlines() if re.match(r'\s*PA08\s', line)], 'RM PA8')
    assert rm_route.split() == ['PA08', 'GPIO', 'UART1_TXD', 'BTIM2_TOGN', 'MCO_OUT', 'LVD_OUT', 'GTIM3_ETR', 'ATIM_CH1A']
    ds_route = one([line for line in pages[(PDFS[1][0], 29)].splitlines() if re.match(r'\s*PA08\s', line)], 'DS PA8')
    assert ds_route.split() == ['PA08', 'UART1_TXD', 'BTIM2_TOGN', 'MCO_OUT', 'LVD_OUT', 'GTIM3_ETR', 'ATIM_CH1A']
    assert re.search(r'^\s*29\s+18\s+-\s+18\s+-\s+PA08\s+I/O\s+TC', pages[(PDFS[1][0], 26)], re.M)
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
    io = pages[(PDFS[1][0], 51)]
    assert '表 7-27' in io and 'GPIOx_SPEED.PINy' in io
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
    envelope = pages[(PDFS[1][0], 38)]
    assert re.search(r'VDD\s+标准工作电压\s+-\s+1\.65\s+5\.5', envelope)
    assert re.search(r'LQFP48\s+-\s+364', envelope)
    assert re.search(r'最大功率耗散\s+-40\s+105', envelope)
    assert re.search(r'低功率耗散\s*2\s+-40\s+125', envelope)
    assert re.search(r'TJ\s+结温范围\s+温度标号 7\s+-40\s+125', envelope)
    assert 'TJ 不超过 TJmax' in envelope
    sf = proof['source_facts']
    assert sf['register'] == {'owner': 'SYSCTRL', 'offset': 112, 'reset': 0,
        'source_bit_offset': 0, 'source_bit_size': 4, 'disabled': 0, 'hclk': 1, 'pclk': 2,
        'div_bit_offset': 4, 'div_bit_size': 3, 'divisors': DIVISORS,
        'reserved': 'bits31:7 retain incoming state; source/DIV-only RMW; inherited nonzero SOURCE rejected'}
    assert sf['pin'] == {'part': PART, 'family': 'CW32F030', 'package': 'LQFP48',
        'owner': 'SYSCTRL', 'owner_address': 0x40010000,
        'owner_registers': {'block': 'SYSCTRL', 'kind': 'sysctrl', 'version': 'v1'},
        'pin_position': '29', 'route': ROUTE, 'route_gpio_register': 'AFRH', 'route_gpio_field': 'AFR8'}
    assert sf['fmax_conditions']['data_measured'] is False
    assert sf['normal_operating_envelope']['low_power_125C_extension_admitted'] is False
    assert sf['fmax_conditions'] == {'loaded_duty_percent': [45, 55],
        'rise_plus_fall_maximum_period_ratio': [2, 3], 'data_measured': False}
    assert sf['normal_operating_envelope'] == {'supply_mv': [1650, 5500], 'temperature_c': [-40, 105],
        'LQFP48_PD_mw_at_TA105': 364, 'maximum_junction_c': 125, 'low_power_125C_extension_admitted': False}
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


def check_projection(proof, data):
    scope = proof['projection_scope']
    assert scope['selection_count'] == 54 and len(scope['parts']) == len(set(scope['parts'])) == 54
    by_part = scope['parts']
    actual = sorted(p.name for p in (data / 'chips').iterdir() if p.is_file())
    assert actual == sorted(name + '.json' for name in by_part), 'complete exact 54 selections required'
    assert PART in by_part and 'CW32F030' in by_part and 'CW32F030C8' in by_part
    projected = []
    for name in by_part:
        path = data / 'chips' / (name + '.json')
        chip = load(path)
        assert chip['name'] == name and len(chip['cores']) == 1
        core = chip['cores'][0]
        owners = [p for p in core['peripherals'] if p.get('clock_limits') is not None]
        owner = one(owners, name + ' clock owner')
        assert owner['name'] == 'SYSCTRL'
        facts = [(p, p['clock_limits']['mco']) for p in core['peripherals']
                 if 'mco' in p.get('clock_limits', {})]
        routes = [(p, pin) for p in core['peripherals'] for pin in p.get('pins', []) if pin['signal'] == 'MCO']
        if name == PART:
            assert chip['line'] == 'CW32F030' and len(facts) == len(routes) == 1
            assert facts[0][0] is owner and routes[0][0] is owner and routes[0][1] == ROUTE
            check_facts(facts[0][1])
            assert owner['address'] == 0x40010000 and owner['registers'] == {'kind': 'sysctrl', 'version': 'v1', 'block': 'SYSCTRL'}
            package = one(chip['packages'], name + ' package')
            assert package['name'] == name and package['package'] == 'LQFP48'
            assert len([p for p in package['pins'] if p['position'] == '29' and 'PA8' in p['signals']]) == 1
            assert len([p for p in core['pins'] if p['name'] == 'PA8']) == 1
            gpio = one([p for p in core['peripherals'] if p['name'] == 'GPIOA'], 'GPIOA')
            assert gpio['address'] == 0x48000000 and gpio['gpio']['output_mask'] & 256
            assert gpio['registers'] == {'kind': 'gpio', 'version': 'v1', 'block': 'GPIO'}
        else:
            assert facts == routes == [], (name, 'unqualified fact/route must be omitted')
        projected.append({'part': name, 'sha256': sha(path), 'mco': name == PART})
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
    assert set(catalog['parts']) == {PART} and set(catalog['policies']) == {'docs/f030-mco-qualification.json'}
    proof_path = ROOT / 'docs/f030-mco-qualification.json'
    assert catalog['policies']['docs/f030-mco-qualification.json'] == sha(proof_path)
    proof = load(proof_path)
    assert proof['schema_version'] == 1 and proof['part'] == PART
    profile = catalog['parts'][PART]
    exact_keys(profile, 'family package pin_position route facts sources', 'profile')
    exact_keys(profile['route'], 'pin signal af', 'route')
    assert profile == proof['profile'] and profile['route'] == ROUTE
    assert (profile['family'], profile['package'], profile['pin_position']) == ('CW32F030', 'LQFP48', '29')
    check_facts(profile['facts'])
    electrical = load(ROOT / 'cw32-data/electrical.yaml')
    assert all('mco' not in p['clock_limits'] for p in electrical['profiles'].values())
    candidate = load(ROOT / 'cw32-data/af/cw32f030.yaml')
    raw = one([p for p in candidate['unresolved'] if p['pin'] == 'PA8' and p['af'] == 4], 'raw AF evidence')
    assert raw['function'] == 'MCO' and raw['source_macro'] == 'PA08_AFx_MCO' and raw['source_line'] == 837
    assert (raw['gpio_register'], raw['gpio_field']) == ('AFRH', 'AFR8')
    assert all(p['signal'] != 'MCO' for p in candidate['routes'])
    check_ir(load(ROOT / 'cw32-data/registers/sysctrl_v1.yaml'), 'SYSCTRL', 'MCO', 112, [('SOURCE', 0, 4), ('DIV', 4, 3)])
    gpio = load(ROOT / 'cw32-data/registers/gpio_v1.yaml')
    check_ir(gpio, 'GPIO', 'AFRH', 20, [('AFR8', 0, 4)])
    check_ir(gpio, 'GPIO', 'SPEED', 8, [('PIN8', 8, 1)])
    originals = check_sources(profile, proof, args.sources, load(ROOT / catalog['source_authority']))
    projected = check_projection(proof, args.data)
    receipt = {'schema_version': 1, 'status': 'passed_source_and_data_audit', 'scope': 'Exact CW32F030C8T7 MCO',
        'catalog': {'path': 'cw32-data/mco-qualified.yaml', 'sha256': sha(path)},
        'qualification': {'path': 'docs/f030-mco-qualification.json', 'sha256': sha(proof_path)},
        'validator': {'path': 'ci/verify-f030-mco-data.py', 'sha256': sha(Path(__file__))},
        'originals': originals, 'selections': projected, 'selection_count': len(projected),
        'qualified_count': 1, 'unqualified_count': 53,
        'source_codes': [0, 1, 2], 'divisors': DIVISORS, 'conditional_unmeasured_policy_preserved': True,
        'hal_executed': False, 'hardware_waveform_validated': False,
        'acceptance_boundary': 'Source/data validation does not constitute final schema/runtime review, link validation or hardware acceptance.'}
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(receipt, indent=2) + '\n')
    print('F030 MCO own sources and all 54 capability projections passed; 53 excluded, one exact capability.')


if __name__ == '__main__':
    main()
