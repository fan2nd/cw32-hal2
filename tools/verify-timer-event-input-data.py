#!/usr/bin/env python3
"""Verify current own timer sources, native-input metadata and package projection.

Reads the current catalog, locked external originals and generated data only.
No HAL execution, historical stage dependency, mutation mode or build harness.
"""
import argparse
import collections
import hashlib
import json
from pathlib import Path
import re
import sys

import fitz
import yaml

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'cw32-data/tools'))
from extract_pinouts import extract_rows
from source_provenance import source_nodes


def require(ok, label):
    if not ok:
        raise ValueError(label)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read(path):
    return yaml.safe_load(path.read_text()) if path.suffix == '.yaml' else json.loads(path.read_text())


def field(ir, register, name, bit, width=1):
    values = [f for f in ir['fieldset/' + register]['fields'] if f['name'] == name]
    require(len(values) == 1 and values[0]['bit_offset'] == bit
            and values[0]['bit_size'] == width and 'array' not in values[0],
            f'Canonical field differs: {register}.{name}')


def page_receipts(node, pages):
    if isinstance(node, dict):
        if 'derived_page_text_sha256' in node:
            index = node.get('pdf_page_index_zero_based', node.get('pdf_page_1_based', 0) - 1)
            require(index >= 0 and hashlib.sha256(pages[index].encode()).hexdigest()
                    == node['derived_page_text_sha256'], 'Own semantic page identity differs')
        for value in node.values():
            page_receipts(value, pages)
    elif isinstance(node, list):
        for value in node:
            page_receipts(value, pages)


def native_registers(ir, version, policy):
    items = {i['name']: i for i in ir['block/ATIM']['items']}
    for name, offset in [('ARR', 0), ('CNT', 4), ('CR', 12), ('ISR', 16), ('ICR', 20),
                         ('MSCR', 24), ('FLTR', 28), ('CH1CR', 36), ('CH2CR', 40),
                         ('CH3CR', 44), ('RCR', 52), ('CH1CCRA', 60), ('CH1CCRB', 64),
                         ('CH2CCRA', 68), ('CH2CCRB', 72), ('CH3CCRA', 76), ('CH3CCRB', 80)]:
        item = items[name]
        require(item['byte_offset'] == offset and item['fieldset'] == name
                and 'array' not in item and item.get('bit_size', 32) == 32,
                'Canonical native register layout differs')
    require(items['ISR']['access'] == 'Read', 'Canonical native ISR access differs')
    for register, name, bit, width in [
        ('CR', 'EN', 0, 1), ('CR', 'CT', 2, 1), ('CR', 'PRS', 4, 3),
        ('CR', 'BUFPEN', 7, 1), ('CR', 'UIE', 10, 1), ('CR', 'MODE', 12, 2),
        ('CR', 'ONESHOT', 14, 1), ('CR', 'URS', 17, 1), ('CR', 'UG', 25, 1),
        ('CR', 'DIR', 27, 1), ('MSCR', 'SMS', 8, 3), ('MSCR', 'IA1S', 11, 1),
        ('MSCR', 'IB1S', 12, 1), ('ARR', 'ARR', 0, 16), ('CNT', 'CNT', 0, 16),
        ('RCR', 'RCR', 0, 8), ('RCR', 'OV', 8, 1), ('RCR', 'UD', 9, 1),
    ]:
        field(ir, register, name, bit, width)
    for channel in range(1, 4):
        control = f'CH{channel}CR'
        for name, bit, width in [('BKSA', 0, 2), ('BKSB', 2, 2), ('CSA', 4, 1),
                                 ('CSB', 5, 1), ('BUFEA', 6, 1), ('BUFEB', 7, 1),
                                 ('CIEA', 8, 1), ('CIEB', 9, 1), ('CISB', 12, 2),
                                 ('CCGA', 14, 1), ('CCGB', 15, 1)]:
            field(ir, control, name, bit, width)
        for name, bit in [('CDEA', 10), ('CDEB', 11)]:
            if version == 'cw32f003_v1':
                require(all(f['name'] != name for f in ir['fieldset/' + control]['fields']),
                        'F003 must retain its actual missing DMA fields')
            else:
                field(ir, control, name, bit)
        for half, flag, offset in [('A', channel + 1, (channel - 1) * 8),
                                   ('B', channel + 4, (channel - 1) * 8 + 4)]:
            field(ir, 'FLTR', f'OCM{channel}{half}FLT{channel}{half}', offset, 3)
            field(ir, 'FLTR', f'CCP{channel}{half}', offset + 3)
            field(ir, f'CH{channel}CCR{half}', f'CCR{channel}{half}', 0, 16)
            for register in ['ISR', 'ICR']:
                field(ir, register, f'C{channel}{half}F', flag)
                field(ir, register, f'C{channel}{half}E', flag + 6)
    command = next(p for p in policy['atim_' + version] if p['register'] == 'ICR')
    require(command['block'] == 'ATIM' and command['fieldset'] == 'ICR'
            and command['write_noop'] == command['reset_value'] == 0x7ffff,
            'Native ATIM command seed differs')
    require(set(command['zero_to_clear_fields']) == {f['name'] for f in ir['fieldset/ICR']['fields']},
            'Native ATIM clear-field domain differs')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--sources', type=Path, required=True)
    args = parser.parse_args()
    catalog = read(ROOT / 'cw32-data/classic-atim-input.yaml')
    require(catalog['schema_version'] == 1, 'Unknown input catalog schema')
    require(digest(ROOT / catalog['evidence']) == catalog['evidence_sha256'], 'Input evidence hash differs')
    proof = read(ROOT / catalog['evidence'])
    require(proof['schema_version'] == 1, 'Unknown source-evidence schema')
    lock = read(ROOT / proof['source_authority'])
    nodes = {n['id']: n for n in source_nodes(lock)}
    records = {s['source_ref']: s for s in proof['source_records']}
    require(len(records) == len(proof['source_records']), 'Duplicate source identity')
    for ref, source in records.items():
        node = nodes[ref]
        for key in ['path', 'sha256', 'bytes']:
            require(source[key] == node[key], f'Locked source {key} differs: {ref}')
        original = nodes[node['original_id']]
        require(source['original_source_ref'] == original['id'] and source['url'] == original['url']
                and source['revision'] == original['provenance']['printed_revision']
                and source['chip_scope'] == original['provenance']['chip_scope'],
                'Original revision, URL or admitted sharing scope differs: ' + ref)
        path = args.sources / source['path']
        require(path.stat().st_size == source['bytes'] and digest(path) == source['sha256'],
                'External source bytes differ: ' + ref)
        if node['kind'] == 'sdk-member':
            require(source['archive_member_chain'] == node['archive_member_chain']
                    and source['archive_path'] == original['path']
                    and source['archive_sha256'] == original['sha256'], 'Archive member origin differs: ' + ref)
        if node['kind'] == 'pdf-text':
            require(source['extractor'] == lock['text_extractor'], 'Derived-text extractor differs')
    policy = read(ROOT / 'cw32-data/register-writes.yaml')['registers']
    event_families = proof['counter_events']['families']
    association_counts = collections.Counter()
    own_vectors = {}
    for family, own in event_families.items():
        pages = (args.sources / own['derived_text']).read_text().split('\f')
        page_receipts(own, pages)
        lines = (args.sources / own['sdk_header']).read_text(errors='replace').splitlines()
        vectors = {}
        for vector in own['sdk_vector_lines']:
            line = lines[vector['line_1_based'] - 1]
            require(vector['name'] in line and re.search(r'=\s*' + str(vector['external_irq']) + r'\b', line),
                    'Own SDK vector differs: ' + family)
            vectors[vector['name'].removesuffix('_IRQn')] = vector['external_irq']
        own_vectors[family] = vectors
        for ref in own['gate_reset_own_manual_fields']:
            if 'sdk_field_receipt' in ref:
                receipt = ref['sdk_field_receipt']
                require(receipt['path'] == own['sdk_header']
                        and lines[receipt['line_1_based'] - 1].strip() == receipt['text'],
                        'Own gate/reset naming alias differs')
        for kind, bank in own['banks'].items():
            require(digest(ROOT / bank['register_file']) == bank['register_file_sha256'],
                    'Own selected register identity differs: ' + family + kind)
            ir = read(ROOT / bank['register_file'])
            for name in ['request_field', 'event_field']:
                f = bank[name]
                field(ir, f['register'], f['field'], f['bit'])
            reference = next(v for k, v in bank['references'].items() if k.lower() == 'icr')
            start = reference['pdf_page_1_based'] - 1
            text = '\n'.join(pages[start:start + reference.get('continued_page_count', 1)])
            reset = re.search(r'Reset value:\s*(0x[0-9A-Fa-f]{4}\s*[0-9A-Fa-f]{4})', text)
            require(reset is not None, 'Own ICR reset anchor absent: ' + family + kind)
            mask = int(re.sub(r'\s+', '', reset[1]), 16)
            require(mask == int(bank['icr_noop'], 16) and mask & ~1 == int(bank['overflow_clear'], 16),
                    'Own counter command mask differs: ' + family + kind)
            expected_masks = {'classic_gtim': 0x3ff, 'classic_atim': 0x7ffff,
                              'buffered_gtim': 0x00f01e5f, 'buffered_atim': 0x00ff3fff}
            require(mask == expected_masks[bank['cohort']], 'Counter cohort differs')
            key = kind.lower() + '_' + bank['register_variant']
            if key in policy:
                command = next(p for p in policy[key] if p['register'] == 'ICR')
                require(command['write_noop'] == mask, 'Canonical counter command seed differs')
            if mask == 0x3ff:
                require(re.search(r'8:7\s+RFU', text), 'Reserved classic GTIM reset-one bits absent')
            if mask == 0x7ffff:
                require(re.search(r'\b1\s+RFU', text), 'Reserved classic ATIM reset-one bit absent')
            association_counts[kind] += len(bank['instances'])

    input_families = proof['classic_atim_input']['families']
    require(set(catalog['profiles']) == set(input_families), 'Own input profile set differs')
    pinouts_by_family = {}
    die_routes = {}
    pdfs = {}
    cell_count = 0
    physical_rows = 0
    for family, profile in catalog['profiles'].items():
        own = input_families[family]
        require(profile['protocol'] == own['protocol'] == 'ClassicPairedV1'
                and profile['register_version'] == own['register_variant'], 'Native protocol differs')
        require(profile['register_version'] in ('v1', 'cw32f003_v1', 'cw32l052_v1'), 'Native IP differs')
        require(profile['pinout'] == own['pinout']['path']
                and digest(ROOT / profile['pinout']) == profile['pinout_sha256'] == own['pinout']['sha256'],
                'Own pinout identity differs')
        pinout = read(ROOT / profile['pinout'])
        pinouts_by_family[family] = pinout
        for source in own['original_sources'].values():
            require(source['source_id'] in records and source['sha256'] == records[source['source_id']]['sha256'],
                    'Route source lacks current canonical identity')
        rows = extract_rows(args.sources / own['original_sources']['datasheet']['file'], family)
        require(rows == pinout['table_rows'], 'Original physical pin grid differs: ' + family)
        physical_rows += len(rows)
        macros = (args.sources / own['original_sources']['gpio_header']['file']).read_text(errors='replace').splitlines()
        page_text = (args.sources / own['derived_text']).read_text().split('\f')
        page_receipts(own['own_manual_anchors'], page_text)
        page_receipts(own['own_input_role_qualification'], page_text)
        capture_member = next(s for s in records.values()
                              if s.get('archive_path') == own['sdk_archive']
                              and s['sha256'] == own['sdk_capture_source_member_sha256'])
        require(capture_member['archive_member_chain'] == [own['sdk_capture_source_member']],
                'Capture driver member chain differs')
        capture_lines = (args.sources / capture_member['path']).read_text(
            encoding='utf-8-sig', errors='replace').splitlines()
        for ref in own['sdk_capture_function_lines']:
            require(capture_lines[ref['line_1_based'] - 1].strip() == ref['signature'],
                    'Own six-half capture function differs')
        qualified = set()
        for route in own['routes']:
            macro = route.get('own_sdk_macro')
            if macro:
                text = macros[macro['line_1_based'] - 1]
                require(macro['macro'] in text and re.search(r'=\s*' + str(route['af']) + r'\s*\)', text),
                        'GPIO SDK assignment differs')
            if not route.get('input_source_qualified', False):
                require(route['withheld_reasons'], 'Withheld route lacks its reason')
                continue
            key = route['pin'], route['af'], route['signal']
            require(key not in qualified and re.fullmatch(r'CH[1-3][AB]', route['signal']), 'Invalid native route')
            qualified.add(key)
            require(not route['withheld_reasons'], 'Qualified route also withheld')
            for name, role in [('own_datasheet_cell', 'datasheet'), ('own_manual_cell', 'reference_manual')]:
                cell = route[name]
                require((cell['pin'], cell['af'], cell['signal']) == key, 'Own AF cell identity differs')
                path = own['original_sources'][role]['file']
                if path not in pdfs:
                    pdfs[path] = fitz.open(args.sources / path)
                page = pdfs[path][cell['pdf_page_1_based'] - 1]
                box = cell['cell_bbox']
                text = re.sub(r'\s+', '', page.get_text(clip=fitz.Rect(box)))
                require('ATIM_' + route['signal'] in text or 'ATIM' + route['signal'] in text,
                        'Original own AF cell differs')
                text = page.get_text(clip=fitz.Rect(0, box[1], box[0], box[3]))
                pins = {m[1] + str(int(m[2])) for m in re.finditer(r'\b(P[A-F])(\d+)\b', text)}
                require(route['pin'] in pins, 'Original own AF row differs')
                cell_count += 1
            row = next(r for r in rows if route['pin'] in r['signals'])
            require(row['pin_type'] in ('I/O', 'I') and row['positions'] == route['pin_definition']['positions']
                    and row['source_name'] == route['pin_definition']['source_name'], 'Own input pin definition differs')
            for package in pinout['packages']:
                positions = {signal: p['position'] for p in package['pins'] for signal in p['signals']}
                require(positions.get(route['pin']) == route['package_pins'][package['name']], 'Own package bond differs')
        actual = set()
        for route in profile['routes']:
            key = route['pin'], route['af'], route['signal']
            require(key not in actual, 'Duplicate catalog tuple')
            actual.add(key)
            prefix = f'/classic_atim_input/families/{family}/routes/'
            require(route['evidence_pointer'].startswith(prefix), 'Wrong-family route evidence')
            original = own['routes'][int(route['evidence_pointer'].removeprefix(prefix))]
            require(original.get('input_source_qualified', False)
                    and (original['pin'], original['af'], original['signal']) == key, 'Catalog source pointer differs')
        require(actual == qualified, 'Catalog does not contain exactly the qualified own routes')
        die_routes[family] = qualified
        native_registers(read(ROOT / f"cw32-data/registers/atim_{profile['register_version']}.yaml"),
                         profile['register_version'], policy)

    clock_pages = 0
    sdk_references = 0
    for own in proof['classic_atim_input']['clock']['own_family_sources']:
        pages = (args.sources / own['derived_text']['path']).read_text().split('\f')
        page_receipts(own['pages'], pages)
        clock_pages += len(own['pages'])
        for member in own['sdk']['members']:
            require(member['source_ref'] in records and member['sha256'] == records[member['source_ref']]['sha256'],
                    'Own SDK semantic member identity differs')
            lines = (args.sources / member['path']).read_text(encoding='utf-8-sig', errors='replace').splitlines()
            for name, ref in member['references'].items():
                if 'line' in ref:
                    require(lines[ref['line'] - 1].strip() == ref['text'].strip(), 'Own SDK semantic line differs')
                elif name != 'IS_ATIM_PRESCALER':
                    require(name in '\n'.join(lines[ref['line_start'] - 1:ref['line_end']]), 'Own SDK function differs')
                sdk_references += 1

    counts = collections.Counter()
    selected_families = set()
    bodies = {}
    for path in sorted((ROOT / 'cw32-data/data/chips').glob('*.json')):
        chip = read(path)
        family = chip['line']
        selected_families.add(family)
        own = event_families[family]
        profile = catalog['profiles'].get(family)
        core = chip['cores'][0]
        pins = {p['name'] for p in core['pins']}
        irq_numbers = {i['name']: i['number'] for i in core['interrupts']}
        require({p['name'] for p in core['peripherals'] if p.get('registers', {}).get('kind') in ('gtim', 'atim')}
                == {i for b in own['banks'].values() for i in b['instances']}, 'Selected timer instance set differs')
        if profile:
            pinout = pinouts_by_family[family]
            exact = [p for p in pinout['packages'] if p['name'] == chip['name']]
            packages = exact or [p for p in pinout['packages'] if chip['name'] in (family, p['family_part'])]
            require(packages and bool(exact) == bool(chip['packages']), 'Current package/alias policy differs')
            expected = {(pin, af, 'CAP' + signal[2:]) for pin, af, signal in die_routes[family]
                        if pin in pins and all(any(pin in pad['signals'] for pad in p['pins']) for p in packages)}
        else:
            expected = set()
        for peripheral in core['peripherals']:
            identity = peripheral.get('registers', {})
            kind = identity.get('kind')
            if kind in ('gtim', 'atim'):
                bank = own['banks'][kind.upper()]
                require(identity == {'kind': kind, 'version': bank['register_variant'], 'block': kind.upper()},
                        'Selected raw register identity differs')
                vectors = [i['interrupt'] for i in peripheral.get('interrupts', []) if i['signal'] == 'GLOBAL']
                require(len(vectors) == 1 and irq_numbers[vectors[0]] == own_vectors[family][vectors[0]],
                        'Selected GLOBAL association differs')
                control = peripheral['rcc_control']
                require(control['controller'] == 'SYSCTRL' and control['bus_clock'] == 'PCLK'
                        and control['enable_active_value'] is True and control['reset_asserted_value'] is False,
                        'Selected timer clock/reset policy differs')
                for action, prefix in [('enable', 'SYSCTRL_APBEN'), ('reset', 'SYSCTRL_APBRST')]:
                    ref = next(r for r in own['gate_reset_own_manual_fields']
                               if r.get('canonical_field', r['field']) == peripheral['name'] and r['register'].startswith(prefix))
                    require(control[action] == {'register': ref['register'].removeprefix('SYSCTRL_'),
                                                'field': ref.get('canonical_field', ref['field'])}, 'Own gate/reset association differs')
                    sysctrl = next(p for p in core['peripherals'] if p['name'] == control['controller'])
                    sysreg = sysctrl['registers']
                    sysir = read(ROOT / f"cw32-data/registers/{sysreg['kind']}_{sysreg['version']}.yaml")
                    field(sysir, control[action]['register'], control[action]['field'], ref['bit'])
                counts[kind + '_selected_associations'] += 1
            routes = peripheral.get('pins', [])
            if peripheral['name'] == 'ATIM':
                cap = [p for p in routes if p['signal'].startswith('CAP')]
                observed = {(p['pin'], p.get('af'), p['signal']) for p in cap}
                # Buffered ATIM retains its independent CAP1..4 protocol.
                native = {(pin, af, signal) for pin, af, signal in observed if re.fullmatch(r'CAP\d+[AB]', signal)}
                if profile:
                    require(observed == expected and len(cap) == len(expected), 'Native package capture routes differ: ' + chip['name'])
                    require(peripheral.get('classic_atim_input') == 'ClassicPairedV1', 'Native fact absent')
                    require(all(p == {'pin': p['pin'], 'signal': p['signal'], 'af': p['af']} for p in cap),
                            'Native pin has an invented analog mux or new shape')
                    require(routes == sorted(routes, key=lambda p: (p['pin'], p['signal'], p.get('af', -1))),
                            'Native route sort order differs')
                    counts['qualified_selections'] += 1
                    counts['selected_capture_routes'] += len(observed)
                else:
                    require(not native and 'classic_atim_input' not in peripheral, 'Unqualified native ATIM fact/routes')
            else:
                require('classic_atim_input' not in peripheral, 'Native ATIM fact on another peripheral')
                require(not any(re.fullmatch(r'CAP\d+[AB]', p['signal']) for p in routes), 'Native route on another peripheral')
        wrapper = ROOT / 'cw32-metapac/src/chips' / chip['name'].lower() / 'metadata.rs'
        bodyname = re.search(r'\.\./(metadata_\d+\.rs)', wrapper.read_text())[1]
        bodypath = ROOT / 'cw32-metapac/src/chips' / bodyname
        body = bodypath.read_text()
        literals = re.findall(r'^    Peripheral \{\n(.*?)^    \},$', body, re.M | re.S)
        require(len(literals) == len(core['peripherals']), 'Static peripheral literal count differs')
        for peripheral, literal in zip(core['peripherals'], literals):
            require(re.search(r'^        name: "([^"]+)"', literal, re.M)[1] == peripheral['name'],
                    'Static selected peripheral differs')
            variant = 'Some(PeripheralClassicAtimInput::ClassicPairedV1)' if peripheral.get('classic_atim_input') else 'None'
            require(re.search(r'^        classic_atim_input: (.*),$', literal, re.M)[1] == variant,
                    'Static enum expression differs')
            if peripheral.get('classic_atim_input'):
                require('block: "Atim"' in literal, 'Static ATIM block sanitation differs')
                projected = {(m[1], int(m[3]), m[2]) for m in re.finditer(
                    r'PeripheralPin \{\s*pin: "([^"]+)",\s*signal: "(CAP[1-3][AB])",\s*af: Some\((\d+)\)', literal)}
                require(projected == expected, 'Static native routes differ')
        bodies[bodyname] = len(literals)
        counts['selections'] += 1
    require(selected_families == set(event_families), 'Current event-family coverage differs')
    require((ROOT / 'cw32-metapac/src/metadata.rs').read_bytes()
            == (ROOT / 'cw32-metapac-gen/res/src/metadata.rs').read_bytes(), 'Public static model differs from template')
    for pdf in pdfs.values():
        pdf.close()
    print(json.dumps({'source_files_verified': len(records), 'own_event_associations': dict(association_counts),
                      'native_profiles': len(input_families), 'qualified_die_routes': sum(map(len, die_routes.values())),
                      'own_af_cells': cell_count, 'physical_pin_rows': physical_rows,
                      'clock_page_identities': clock_pages, 'sdk_semantic_references': sdk_references,
                      **counts, 'unqualified_selections': counts['selections'] - counts['qualified_selections'],
                      'metadata_bodies': len(bodies), 'deduplicated_peripheral_literals': sum(bodies.values()),
                      'hal_execution': 'none; current-source and data qualification only'}, indent=2))


if __name__ == '__main__':
    main()
