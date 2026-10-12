#!/usr/bin/env python3
"""Check current own-source facts for native finite UART observed-idle receive.

This reads source identities and semantic metadata only. It does not execute the
HAL, model interrupt timing, freeze HAL text, or compare historical output hashes.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import tomllib

import yaml

ROOT = Path(__file__).resolve().parents[1]
EVIDENCE = 'docs/uart-idle-evidence.json'
CHECKER = 'tests/verify_native_uart_idle_sources.py'
FAMILIES = {'CW32L010', 'CW32L011', 'CW32L012'}
VERSIONS = {'cw32l010_v1', 'cw32l012_v1'}


def read_json(path):
    return json.loads((ROOT / path).read_text())


def read_yaml(path):
    return yaml.safe_load((ROOT / path).read_text())


def named(entries):
    result = {entry['name']: entry for entry in entries}
    assert len(result) == len(entries), 'duplicate metadata name'
    return result


def fields(registers, name):
    return {field['name']: [field['bit_offset'], field['bit_size']]
            for field in registers['fieldset/' + name]['fields']}


def compact(text):
    return re.sub(r'\s+', '', text)


def macro(header, name):
    match = re.search(r'^#define\s+' + re.escape(name) + r'\s+\(?((?:0x)?[0-9A-Fa-f]+)UL\)?',
                      header, re.M)
    assert match, name
    return int(match[1], 16 if match[1].startswith('0x') else 10)


def page(pages, pair, family):
    printed, viewer = pair
    text = pages[viewer - 1]
    assert family in text and str(printed) in text, (family, pair)
    return text


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--sources', type=Path, required=True,
                        help='Absolute directory containing the already locked external evidence')
    args = parser.parse_args()
    assert args.sources.is_absolute() and args.sources.is_dir(), args.sources
    evidence = read_json(EVIDENCE)
    index = read_json(evidence['identity_authority'])
    source_bytes = {}
    nodes = {}
    for binding in evidence['sources']:
        ref = binding['source_ref']
        assert ref not in nodes, ref
        node = index
        for key in binding['index_pointer'].strip('/').split('/'):
            node = node[int(key)] if isinstance(node, list) else node[key]
        kind, path = ref.split(':', 1)
        assert kind in {'vendor', 'member', 'text'} and node['path'] == path, ref
        assert EVIDENCE in node['evidence'] and CHECKER in node['evidence'], ref
        data = (args.sources / path).read_bytes()
        assert len(data) == node['bytes'], ref
        assert hashlib.sha256(data).hexdigest() == node['sha256'], ref
        nodes[ref], source_bytes[ref] = node, data
    assert len(nodes) == 27
    families = evidence['families']
    assert set(families) == FAMILIES
    assert set(evidence['register_contracts']) == VERSIONS
    assert set(evidence['cfg_register_versions']) == {'uart_' + v for v in VERSIONS}
    commands = read_yaml('cw32-data/register-writes.yaml')['registers']
    for version, contract in evidence['register_contracts'].items():
        for path, loader in [(contract['project_yaml'], read_yaml),
                             (contract['project_json'], read_json)]:
            registers = loader(path)
            items = named(registers['block/UART']['items'])
            for name, expected in contract['registers'].items():
                assert items[name]['byte_offset'] == expected['offset'], (path, name)
                assert items[name].get('access', 'ReadWrite') == expected['access'], (path, name)
            for name, expected in contract['fields'].items():
                assert fields(registers, name) == expected, (path, name)
        icr = contract['fields']['ICR']
        mask = sum(((1 << width) - 1) << bit for bit, width in icr.values())
        assert mask == contract['icr_clearable_mask'] == 0x1ffe
        command, = commands['uart_' + version]
        assert (command['block'], command['register'], command['fieldset']) == ('UART', 'ICR', 'ICR')
        assert command['write_noop'] == command['reset_value'] == contract['icr_write_noop'] == 0x1fff
        assert set(command['zero_to_clear_fields']) == set(icr)
        # The generated PAC command seed is semantic output, not a whole-file hash.
        generated = (ROOT / f'cw32-metapac/src/peripherals/uart_{version}.rs').read_text()
        icr_impl = re.search(r'impl regs::Icr\s*\{(.*?)\n\}', generated, re.S)
        assert icr_impl, version
        for method in ['reset_value', 'write_noop']:
            value = re.search(r'fn ' + method + r'\(\) -> Self\s*\{\s*Self\((\d+)\)', icr_impl[1])
            assert value and int(value[1]) == command[method], (version, method)
    capabilities = read_yaml('ci/hal-capabilities.yaml')
    features = tomllib.loads((ROOT / 'embassy-cw32/Cargo.toml').read_text())['features']
    selections, instance_count, route_count = set(), 0, 0
    for family, facts in families.items():
        sources = facts['sources']
        assert set(sources) == {'sdk', 'cmsis', 'uart_header', 'uart_driver', 'gpio_header',
                                'manual', 'manual_text', 'datasheet', 'datasheet_text'}
        assert len(set(sources.values())) == 9 and set(sources.values()) <= nodes.keys()
        assert all(family.lower() in ref.lower() for ref in sources.values()), family
        for role in ['manual', 'datasheet']:
            assert nodes[sources[role]]['text']['path'] == nodes[sources[role + '_text']]['path']
        sdk_members = {member['path'] for member in nodes[sources['sdk']]['members']}
        assert all(nodes[sources[role]]['path'] in sdk_members
                   for role in ['cmsis', 'uart_header', 'uart_driver', 'gpio_header'])
        text = {role: source_bytes[ref].decode('utf-8', errors='replace')
                for role, ref in sources.items() if role not in {'sdk', 'manual', 'datasheet'}}
        pages = text['manual_text'].split('\f')
        locations = facts['own_manual_pages_printed_then_pdf_viewer_1_based']
        idle = compact(page(pages, locations['idle_frame_definition'], family))
        assert '空闲帧是完全由‘1’组成的一个完整的数据帧' in idle, family
        receive = compact(''.join(page(pages, pair, family)
                                  for pair in locations['receive_and_overrun_idle']))
        assert all(word in receive for word in ['UARTx_RDR', 'UARTx_ISR.RC', '覆盖', 'ORE']), family
        timer = compact(page(pages, locations['programmable_timer'], family))
        assert all(word in timer for word in ['TIMCR', '0x3', '(TIMARR+1)/fBaudRate', 'TIMOV', '停止位']), family
        control = ''.join(page(pages, pair, family) for pair in locations['IER_ISR_ICR'])
        assert '每当接收完成一个字符时，才会启动空闲字符检测' in compact(control), family
        for access in ['RW', 'RO', 'R1W0']:
            for flag, bit in [('RC', 2), ('RXIDLE', 3), ('TIMOV', 6), ('ORE', 11)]:
                assert re.search(r'\b' + str(bit) + r'\s+' + flag + r'\s+' + access + r'\b', control), (family, flag, access)
        driver_lines = text['uart_driver'].splitlines()
        rdr_spelling = 'UART_RDR' if family == 'CW32L012' else 'UARTx_RDR'
        for line in locations['rdr_no_clear_sdk_lines']:
            anchor = compact(driver_lines[line - 1])
            assert all(word in anchor for word in ['RC', '不能', rdr_spelling, '清除']), (family, line)
        for name, value in [('RXIDLE', 8), ('TIMOV', 64)]:
            for prefix in ['UART_IT_', 'UART_FLAG_']:
                match = re.search(r'^#define\s+' + prefix + name + r'\s+.*?(0x[0-9a-fA-F]+)', text['uart_header'], re.M)
                assert match and int(match[1], 16) == value, (family, prefix, name)
        cmsis = text['cmsis']
        prefix = 'UART' if family == 'CW32L012' else 'UARTx'
        version = 'cw32l012_v1' if family == 'CW32L012' else 'cw32l010_v1'
        contract = evidence['register_contracts'][version]
        for register in ['IER', 'ISR', 'ICR', 'RDR', 'CR1', 'CR2']:
            for field, (bit, width) in contract['fields'][register].items():
                name = f'{prefix}_{register}_{field}'
                assert macro(cmsis, name + '_Pos') == bit, (family, name)
                assert macro(cmsis, name + '_Msk') == ((1 << width) - 1) << bit, (family, name)
        instances = named(facts['instances'])
        assert set(instances) == ({'UART1', 'UART2'} if family == 'CW32L010' else {'UART1', 'UART2', 'UART3'})
        instance_count += len(instances)
        irqs = {name: int(number) for name, number in re.findall(r'\b(UART[123])_IRQn\s*=\s*(\d+)\b', cmsis)}
        assert irqs == {name: item['irq_number'] for name, item in instances.items()}, family
        for name, instance in instances.items():
            assert macro(cmsis, name + '_BASE') == int(instance['base'], 16)
            assert instance['vector'] == name and instance['register_version'] == version
            for kind in ['gate', 'reset']:
                field = instance[kind]
                assert macro(cmsis, f"SYSCTRL_{field['register']}_{field['field']}_Pos") == field['bit']
        inputs = read_yaml(f'cw32-data/inputs/{family.lower()}.yaml')
        assert inputs['register_versions']['uart'] == version
        assert inputs['alias_pin_policy'] == facts['alias_policy'] == 'common-package-intersection'
        assert inputs['af_metadata'] == facts['af_path'] and inputs['pinout_metadata'] == facts['pinout_path']
        af = read_yaml(facts['af_path'])
        pinout = read_yaml(facts['pinout_path'])
        assert af['datasheet']['source_ref'] == pinout['source']['source_ref'] == sources['datasheet']
        assert af['source']['header_sha256'] == nodes[sources['gpio_header']]['sha256']
        routes = [row for row in af['routes'] if row['peripheral'] in instances]
        assert len(routes) == facts['uart_route_cells'] == {'CW32L010': 19, 'CW32L011': 48, 'CW32L012': 69}[family]
        datasheet = text['datasheet_text'].split('\f')
        gpio_lines = text['gpio_header'].splitlines()
        for row in routes:
            line = gpio_lines[row['source_line'] - 1]
            assert row['source_macro'] + '()' in line
            assert re.search(r'=' + r'\s*' + str(row['af']) + r'\s*\)', line), row
            assert row['gpio_register'] + '_f.' + row['gpio_field'] in line, row
            cell = row['datasheet_cell']
            assert (cell['pin'], cell['af']) == (row['pin'], row['af'])
            sheet = page(datasheet, [cell['printed_page'], cell['pdf_page']], family)
            # A preceding physical-pin table may contain the same pin name.
            table = re.search(r'表\s+' + re.escape(cell['table']) + r'\s+[^\n]*', sheet)
            assert table, row
            table_text = re.split(r'表\s+\d+-\d+\s+', sheet[table.end():], maxsplit=1)[0]
            pin = row['pin'][:2] + row['pin'][2:].zfill(2)
            start = re.search(r'(?m)^[ \t]*' + pin + r'(?:/|\s)', table_text)
            assert start, row
            row_text = re.split(r'(?m)^[ \t]*P[A-F][0-9]+(?:/|\s)',
                                table_text[start.end():], maxsplit=1)[0]
            assert cell['function'] in row_text, row
        packages = named(pinout['packages'])
        expected_selections = {family.lower(), *(name.lower() for name in packages)}
        assert set(facts['selections']) == expected_selections
        assert {path.stem.lower() for path in (ROOT / 'cw32-data/data/chips').glob(family + '*.json')} == expected_selections
        assert {name for name in features if name.startswith(family.lower())} == expected_selections
        assert facts['hal_excluded_pads'] == capabilities['pin_policy']['excluded_by_family'][family]
        exclusions = set(facts['hal_excluded_pads'])
        clock = named(read_yaml(f'cw32-data/clock/{family.lower()}.yaml')['peripherals'])
        for selection in facts['selections']:
            assert selection not in selections
            selections.add(selection)
            chip = read_json(f'cw32-data/data/chips/{selection.upper()}.json')
            assert chip['line'] == family and chip['name'] == selection.upper()
            if selection == family.lower():
                assert chip['packages'] == [] and chip['memory'] == []
                pads = set.intersection(*(set(item['gpio_pins']) for item in packages.values()))
            else:
                package = packages[selection.upper()]
                assert chip['packages'] == [{key: package[key] for key in ['name', 'package', 'pins']}]
                pads = set(package['gpio_pins'])
                assert chip['memory'] == [[
                    {'name': 'FLASH', 'kind': 'flash', 'address': 0, 'size': package['flash_bytes']},
                    {'name': 'RAM', 'kind': 'ram', 'address': 0x20000000, 'size': package['ram_bytes']}]]
            core, = chip['cores']
            peripherals = named(core['peripherals'])
            assert {name for name, p in peripherals.items() if p.get('registers', {}).get('kind') == 'uart'} == set(instances)
            vectors = named(core['interrupts'])
            sys_version = inputs['register_versions']['sysctrl']
            assert peripherals['SYSCTRL']['registers']['version'] == sys_version
            sysctrl = read_json(f'cw32-data/data/registers/sysctrl_{sys_version}.json')
            for name, instance in instances.items():
                uart = peripherals[name]
                assert uart['registers'] == {'block': 'UART', 'kind': 'uart', 'version': version}
                assert uart['address'] == int(instance['base'], 16)
                assert uart['interrupts'] == [{'signal': 'GLOBAL', 'interrupt': name}]
                assert vectors[name]['number'] == instance['irq_number']
                assert [(p['name'], link['signal']) for p in core['peripherals']
                        for link in p.get('interrupts', []) if link['interrupt'] == name] == [(name, 'GLOBAL')]
                control = uart['rcc_control']
                assert control['controller'] == 'SYSCTRL' and control['bus_clock'] == instance['bus_clock'] == 'PCLK'
                assert control['enable_active_value'] == clock[name]['enable_active_value'] == instance['gate']['enable_value'] == 1
                assert control['reset_asserted_value'] == clock[name]['reset_asserted_value'] == instance['reset']['asserted_value'] == 0
                for target, kind in [('enable', 'gate'), ('reset', 'reset')]:
                    item = instance[kind]
                    assert control[target] == {key: item[key] for key in ['register', 'field']}
                    assert fields(sysctrl, item['register'])[item['field']] == [item['bit'], 1]
                assert control['enable_write_key'] == {'field': {'register': 'APBEN1', 'field': 'KEY'}, 'value': 0x5a5a}
                assert fields(sysctrl, 'APBEN1')['KEY'] == [16, 16]
                expected = {(row['pin'], row['signal'], row['af']) for row in routes
                            if row['peripheral'] == name and row['pin'] in pads}
                actual = {(row['pin'], row['signal'], row['af']) for row in uart['pins']}
                assert len(actual) == len(uart['pins']) and actual == expected, (selection, name)
                safe_rx = {pin for pin, signal, _ in actual if signal == 'RX'} - exclusions
                assert safe_rx, (selection, name)
                route_count += len(actual)
    assert len(selections) == 10 and instance_count == 8
    assert evidence['cohort'] == {'families': 3, 'selections': 10, 'family_instances': 8,
                                  'register_versions': 2, 'external_source_identities': 27}
    modes = {mode['id']: mode for mode in capabilities['functions']['uart']['modes']}
    assert 'buffered_ring_idle' not in modes
    idle = modes['finite_idle_receive']
    assert idle['status'] == 'partial' and set(idle['implemented_families']) == FAMILIES
    assert idle['pins_all'] == ['RX']
    assert {item['path'] for item in idle['evidence']} >= {'embassy-cw32/src/usart.rs', 'docs/uart-idle.md'}
    assert modes['buffered_ring_receive']['status'] == 'unimplemented'
    print('PASS native UART idle: 27 own locked identities; 3 families, 10 selections, 8 instances, 2 register versions')
    print(f'PASS native RXIDLE/TIMOV, RDR/RC, R1W0, RCC and {route_count} package-filtered UART routes; split coverage declarations')
    print('Source/data verification only; runtime lifecycle and hardware behavior require separate review/evidence.')


if __name__ == '__main__':
    main()
