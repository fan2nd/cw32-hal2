#!/usr/bin/env python3
"""Validate qualified BTIM IP/instance/gate/flag facts against canonical data."""
import hashlib
import json
import os
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
e=json.loads((ROOT/'docs/btim-counter-evidence.json').read_text())
assert len(e['families'])==13
SOURCES=Path(os.environ.get('CW32_SOURCES', '/workspace/shared/cw32-sources'))
for name, source in e['sources'].items():
    path=SOURCES/name
    assert path.is_file() and hashlib.sha256(path.read_bytes()).hexdigest()==source['sha256'], f'Changed/missing own BTIM evidence: {path}'
print(f'PASS BTIM own-source fingerprints: {len(e["sources"])} PDF/text/SDK files')
count=0
for family,item in e['families'].items():
 path=ROOT/item['register_ir'];assert hashlib.sha256(path.read_bytes()).hexdigest()==item['register_sha256']
 registers=json.loads(path.read_text())
 # Canonical chiptool IR stores named blocks/fieldsets as dictionary keys.
 block=registers['block/BTIM'];items={r['name']:r for r in block['items']}
 assert items['ISR']['access']=='Read'
 status={f['name']:f['bit_offset'] for f in registers['fieldset/ISR']['fields']}
 flags={f['name']:f['bit_offset'] for f in registers['fieldset/ICR']['fields']}
 assert status==flags
 modern=item['command_mask_class']=='modern'
 assert flags==({'UIF':0,'TIF':6} if modern else {'OV':0,'TI':1,'TOP':2})
 if modern:assert items['EGR']['access']=='Write'
 sys=json.loads((ROOT/item['sysctrl_ir']).read_text())
 gate=sys['fieldset/APBEN2']['fields'];gate=next(f for f in gate if f['name'].startswith('BTIM'))
 assert gate['bit_offset']==item['shared_clock']['bit'] and gate['bit_size']==1
 key=next((f for f in sys['fieldset/APBEN2']['fields'] if f['name']=='KEY'),None)
 assert bool(key)==modern
 for feature in item['declared_features']:
  chip=json.loads((ROOT/'cw32-data/data/chips'/(feature.upper()+'.json')).read_text())
  timers={p['name']:p for p in chip['cores'][0]['peripherals'] if p['name'].startswith('BTIM')}
  assert set(timers)=={'BTIM1','BTIM2','BTIM3'}
  assert {name:p['address'] for name,p in timers.items()}==item['instances']
  assert all(p['registers']['version']==item['ip_version'] for p in timers.values())
  count+=1
print(f'PASS BTIM canonical evidence:13 families, {count} chip selections, three instances each; own IP versions/gates/keys/RO ISR/R1W0 flag fields')

# Current IRQ evidence extends the historical counter checks above. It binds
# own sources and semantic metadata, never HAL source strings or output hashes.
import re
import tomllib
import yaml

IRQ_EVIDENCE = 'docs/btim-overflow-irq-evidence.json'
irq_evidence = json.loads((ROOT / IRQ_EVIDENCE).read_text())
index = json.loads((ROOT / 'sources/evidence-sources.json').read_text())
registered = {}
for artifact in index['artifacts']:
    for source in [artifact, *artifact.get('members', []), *([artifact['text']] if 'text' in artifact else [])]:
        if 'sha256' in source:
            assert source['path'] not in registered, source['path']
            registered[source['path']] = source


def own_source(path, expected_hash, url=None):
    source = registered[path]
    assert source['sha256'] == expected_hash, path
    assert IRQ_EVIDENCE in source['evidence'], path
    if url is not None:
        assert source['url'] == url, path
    content = (SOURCES / path).read_bytes()
    assert hashlib.sha256(content).hexdigest() == expected_hash, path
    return content


manual_pages = {}
for name, manual in irq_evidence['external_manuals'].items():
    own_source(name, manual['pdf_sha256'], manual['url'])
    text = manual['derived_text']
    assert registered[name]['text']['path'] == text['path']
    manual_pages[name] = own_source(text['path'], text['sha256']).decode('utf-8').split('\f')

cmsis_irqs = {}
for family, sdk in irq_evidence['external_sdks'].items():
    archive = sdk['archive']
    own_source(archive['path'], archive['sha256'], archive['url'])
    archive_members = {member['path']: member for member in registered[archive['path']]['members']}
    assert set(sdk['members']) == {'cmsis', 'btim_header', 'btim_driver'}
    for role, member in sdk['members'].items():
        assert archive_members[member['path']]['sha256'] == member['sha256']
        content = own_source(member['path'], member['sha256']).decode('latin-1')
        if role == 'cmsis':
            entries = re.findall(r'\b(BTIM[123](?:_[A-Z0-9]+)?)_IRQn\s*=\s*(\d+)\b', content)
            assert len(entries) == 3 and len(dict(entries)) == 3, family
            cmsis_irqs[family] = {name: int(number) for name, number in entries}
assert len(manual_pages) == len(cmsis_irqs) == 12
print('PASS BTIM IRQ own-source identities: 12 PDF/text pairs, 12 SDK archives, 36 SDK members')


def named(entries):
    result = {entry['name']: entry for entry in entries}
    assert len(result) == len(entries), 'duplicate metadata name'
    return result


def register_contract(registers, block, contract):
    items = named(registers['block/' + block]['items'])
    for name, expected in contract.items():
        item = items[name]
        assert item['byte_offset'] == expected['offset'], (block, name)
        assert item.get('access', 'ReadWrite') == expected['access'], (block, name)
        fields = named(registers['fieldset/' + item['fieldset']]['fields'])
        assert {name: [field['bit_offset'], field['bit_size']] for name, field in fields.items()} == expected['fields'], (block, name)


versions = {'v1', 'cw32f002_v1', 'cw32l031_v1', 'cw32l052_v1', 'cw32l010_v1', 'cw32l012_v1'}
assert set(irq_evidence['register_contracts']) == versions
for version, contract in irq_evidence['register_contracts'].items():
    for key, loader in [('project_json', json.loads), ('project_yaml', yaml.safe_load)]:
        register_contract(loader((ROOT / contract[key]).read_text()), 'BTIM', contract['registers'])
    flags = contract['registers']['ICR']['fields']
    mask = sum(((1 << width) - 1) << bit for bit, width in flags.values())
    assert mask == contract['defined_flag_mask']
    assert contract['overflow_bit'] == 0
    assert flags[contract['overflow_status_field']] == [0, 1]
    assert contract['direct_overflow_clear_word'] == mask & ~1
    enable = 'DIER' if version == 'cw32l012_v1' else 'IER'
    assert contract['registers'][enable]['fields'][contract['overflow_enable_field']] == [0, 1]
print('PASS BTIM IRQ register semantics: six YAML/JSON variants, enables, RO status, direct R1W0 masks, native URS/UDIS/WO EGR and L012 DMA bits')

families = irq_evidence['families']
assert set(families) == {
    'CW32F030', 'CW32A030', 'CW32F020', 'CW32F002', 'CW32F003',
    'CW32L031', 'CW32R031', 'CW32W031', 'CW32L052', 'CW32L083',
    'CW32L010', 'CW32L011', 'CW32L012',
}
selections = [selection for facts in families.values() for selection in facts['normalized_selections']]
assert len(selections) == len(set(selections)) == 54
assert {path.stem for path in (ROOT / 'cw32-data/data/chips').glob('*.json')} == set(selections)
features = tomllib.loads((ROOT / 'embassy-cw32/Cargo.toml').read_text())['features']
assert {name.upper() for name in features if name.startswith('cw32')} == set(selections)
assert {facts['ip_version'] for facts in families.values()} == versions
assert families['CW32A030']['manual'] == families['CW32F030']['manual'] == 'CW32x030_UserManual_CN_V2.5.pdf'
assert families['CW32A030']['sdk'] == 'CW32F030'

shared_vectors = {facts['family']: facts for facts in irq_evidence['shared_vectors']}
assert len(shared_vectors) == len(irq_evidence['shared_vectors']) == 3
assert {(family, facts['irq'], facts['number'], facts['btim'], facts['partner']) for family, facts in shared_vectors.items()} == {
    ('CW32L052', 'BTIM2_LPTIM', 21, 'BTIM2', 'LPTIM'),
    ('CW32L083', 'BTIM2_LPTIM1', 21, 'BTIM2', 'LPTIM'),
    ('CW32L012', 'BTIM3_HALLTIM', 22, 'BTIM3', 'HALLTIM'),
}
for facts in shared_vectors.values():
    regs = facts['partner_registers']
    stem = f"{regs['kind']}_{regs['version']}"
    for directory, suffix, loader in [('data/registers', 'json', json.loads), ('registers', 'yaml', yaml.safe_load)]:
        registers = loader((ROOT / f'cw32-data/{directory}/{stem}.{suffix}').read_text())
        register_contract(registers, regs['block'], facts['partner_register_contract'])


def check_control(peripheral, expected, sysctrl, enable_bit, reset_bit):
    assert peripheral['rcc_control'] == expected, peripheral['name']
    items = named(sysctrl['block/SYSCTRL']['items'])
    for kind, bit in [('enable', enable_bit), ('reset', reset_bit)]:
        selector = expected[kind]
        register = selector['register']
        assert items[register]['byte_offset'] == {'APBEN2': 0x34, 'APBEN1': 0x38, 'APBRST2': 0x44, 'APBRST1': 0x48}[register]
        field = named(sysctrl['fieldset/' + items[register]['fieldset']]['fields'])[selector['field']]
        assert (field['bit_offset'], field['bit_size']) == (bit, 1)
    if 'enable_write_key' in expected:
        key = expected['enable_write_key']
        field = named(sysctrl['fieldset/' + key['field']['register']]['fields'])[key['field']['field']]
        assert (field['bit_offset'], field['bit_size'], key['value']) == (16, 16, 0x5a5a)


route_count = 0
for family, facts in families.items():
    pages = manual_pages[facts['manual']]
    for role, location in facts['manual_pages'].items():
        page = pages[location['pdf_one_based'] - 1]
        assert 'BTIM' in page and str(location['printed']) in page, (family, role)
    assert 'R1W0' in pages[facts['manual_pages']['clear']['pdf_one_based'] - 1], family
    expected_instances = {instance['instance']: instance for instance in facts['instances']}
    assert len(expected_instances) == len(facts['instances']) == 3
    assert set(expected_instances) == {'BTIM1', 'BTIM2', 'BTIM3'}
    assert cmsis_irqs[facts['sdk']] == {instance['irq']: instance['number'] for instance in facts['instances']}, family
    for selection in facts['normalized_selections']:
        chip = json.loads((ROOT / f'cw32-data/data/chips/{selection}.json').read_text())
        assert chip['line'] == family and chip['name'] == selection
        assert len(chip['cores']) == 1
        core = chip['cores'][0]
        peripherals = named(core['peripherals'])
        timers = {name: peripheral for name, peripheral in peripherals.items() if peripheral.get('registers', {}).get('kind') == 'btim'}
        assert set(timers) == set(expected_instances)
        vectors = named(core['interrupts'])
        sysregs = peripherals['SYSCTRL']['registers']
        sysctrl = json.loads((ROOT / f"cw32-data/data/registers/sysctrl_{sysregs['version']}.json").read_text())
        for name, peripheral in timers.items():
            instance = expected_instances[name]
            assert peripheral['registers'] == {'kind': 'btim', 'version': facts['ip_version'], 'block': 'BTIM'}
            assert peripheral['address'] == int(instance['address'], 16)
            assert peripheral['interrupts'] == [{'signal': 'GLOBAL', 'interrupt': instance['irq']}]
            assert vectors[instance['irq']]['number'] == instance['number'] == {'BTIM1': 20, 'BTIM2': 21, 'BTIM3': 22}[name]
            assert sum(vector['number'] == instance['number'] for vector in core['interrupts']) == 1
            owners = sorted((p['name'], link['signal']) for p in core['peripherals'] for link in p.get('interrupts', []) if link['interrupt'] == instance['irq'])
            expected_owners = [name]
            shared = shared_vectors.get(family)
            if shared and name == shared['btim']:
                assert instance['irq'] == shared['irq']
                expected_owners.append(shared['partner'])
            else:
                assert instance['irq'] == name
            assert sorted(instance['owners']) == sorted(expected_owners)
            assert owners == sorted((owner, 'GLOBAL') for owner in expected_owners)
            check_control(peripheral, facts['rcc_control'], sysctrl, facts['clock_group']['bit'], facts['clock_group']['bit'])
            assert peripheral['rcc'] == {'bus_clock': 'PCLK', 'kernel_clock': 'PCLK', 'enable': facts['rcc_control']['enable'], 'reset': facts['rcc_control']['reset']}
            route_count += 1
        if family in shared_vectors:
            shared = shared_vectors[family]
            partner = peripherals[shared['partner']]
            assert partner['address'] == int(shared['partner_address'], 16)
            assert partner['registers'] == shared['partner_registers']
            assert partner['interrupts'] == [{'signal': 'GLOBAL', 'interrupt': shared['irq']}]
            check_control(partner, shared['partner_rcc_control'], sysctrl, shared['partner_enable'][1], shared['partner_reset'][1])
            assert partner['rcc_control']['enable'] != facts['rcc_control']['enable']
            assert partner['rcc_control']['reset'] != facts['rcc_control']['reset']
assert route_count == 162
assert irq_evidence['cohort'] == {
    'families': 13, 'normalized_selections': 54, 'chip_instance_associations': 162,
    'distinct_btim_versions': 6, 'external_manual_pdf_text_pairs': 12,
    'external_sdk_archives': 12, 'external_selected_sdk_members': 36,
}
print('PASS BTIM IRQ canonical evidence: exactly 13 families / 54 selections / 162 GLOBAL routes; exact shared owners, bank identities and independent RCC controls')
