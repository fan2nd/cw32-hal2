#!/usr/bin/env python3
"""Independent data/source comparison; no HAL execution, fixtures or mocks."""
import argparse
import hashlib
import json
import io
import re
import zipfile
from pathlib import Path
import yaml
import xml.etree.ElementTree as ET

parser = argparse.ArgumentParser()
parser.add_argument('--sources', required=True, type=Path)
parser.add_argument('--out', required=True, type=Path)
args = parser.parse_args()
root = Path(__file__).resolve().parents[1]
def load(path):
    return yaml.safe_load(path.read_text()) if path.suffix == '.yaml' else json.loads(path.read_text())
sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
proof = load(root / 'docs/adc-scan-source-evidence.json')['families']['CW32L012']
lock = load(root / 'sources/evidence-sources.json')['artifacts']
source_receipts = []
for source in [proof['manual'], proof['datasheet'], proof['sdk']['archive']]:
    expected = next(s for s in lock if s['path'] == source['path'])
    assert source['sha256'] == expected['sha256'] == sha(args.sources / source['path'])
    assert source['url'] == expected['url']
    source_receipts.append({k: source[k] for k in ('path', 'url', 'sha256')})
for source in proof['sdk']['members']:
    assert sha(args.sources / source['path']) == source['sha256']
    source_receipts.append(source)
manifest = load(root / 'cw32-data/inputs/cw32l012.yaml')
svd_path = root / manifest['source']['path']
assert sha(svd_path) == manifest['source']['sha256']
svd = ET.parse(svd_path).getroot()
peripherals = {p.findtext('name'): p for p in svd.find('peripherals')}
assert peripherals['ADC2'].get('derivedFrom') == 'ADC1'
accesses = {'read-only': 'Read', 'read-write': 'ReadWrite', 'write-only': 'Write'}
source_regs = {}
for reg in peripherals['ADC1'].find('registers'):
    name = reg.findtext('name')
    fields = {}
    for field in reg.find('fields'):
        lo = int(field.findtext('lsb'), 0)
        fields[field.findtext('name')] = [lo, int(field.findtext('msb'), 0) - lo + 1]
    access = accesses[reg.findtext('access')]
    # Sole access correction predates scans and is backed by own CN1.4 p597.
    if name == 'ISR':
        assert access == 'ReadWrite'
        access = 'Read'
    source_regs[name] = {'offset': int(reg.findtext('addressOffset'), 0),
                         'width': int(reg.findtext('size')), 'access': access, 'fields': fields}
ir = load(root / 'cw32-data/data/registers/adc_cw32l012_v1.json')
generated_regs = {}
for reg in ir['block/ADC']['items']:
    array = reg.get('array')
    for index in range(array['len'] if array else 1):
        name = reg['name'] + (str(index) if array else '')
        fields = {}
        for field in ir['fieldset/' + reg['fieldset']]['fields']:
            fa = field.get('array')
            for fi in range(fa['len'] if fa else 1):
                fields[field['name'] + (str(fi) if fa else '')] = [
                    field['bit_offset'] + (fi * fa['stride'] if fa else 0), field['bit_size']]
        generated_regs[name] = {'offset': reg['byte_offset'] + (index * array['stride'] if array else 0),
                                'width': reg.get('bit_size', 32),
                                'access': reg.get('access', 'ReadWrite'), 'fields': fields}
assert source_regs == generated_regs, 'Expanded ADC registers differ from own SVD'
digest = hashlib.sha256(json.dumps(ir, sort_keys=True, separators=(',', ':')).encode()).hexdigest()
group = next(g for g in load(root / 'cw32-data/register-reuse.yaml')['groups']
             if g['canonical'] == 'adc_cw32l012_v1.yaml')
assert group['canonical_ir_sha256'] == digest
for chip in ['CW32L012', 'CW32L012C8T6', 'CW32L012C8U6']:
    data = load(root / ('cw32-data/data/chips/' + chip + '.json'))
    for name in ['ADC1', 'ADC2']:
        adc = next(p for p in data['cores'][0]['peripherals'] if p['name'] == name)
        assert adc['adc_limits']['sequence'] == proof['facts']
        assert adc['address'] == int(peripherals[name].findtext('baseAddress'), 0)
        assert adc['registers']['version'] == proof['register_version']
        for result in [f'RESULT{i}' for i in range(8)]:
            assert generated_regs[result]['access'] == 'Read'
writes = load(root / 'cw32-data/data/register-writes/adc_cw32l012_v1.json')['registers']['adc_cw32l012_v1']
assert len(writes) == 1
assert writes[0]['write_noop'] == 31 and writes[0]['reset_value'] == 15
# The additional contract is source/data comparison only. No HAL source text,
# model, peripheral execution, build subprocess or historical output hash gate.
def verify_adc2_shared_irq():
    evidence_path = root / 'docs/adc-l012-adc2-async-evidence.json'
    evidence = load(evidence_path)
    checks = []

    def check(name, condition):
        checks.append({'check': name, 'status': 'passed' if condition else 'failed'})
        if not condition:
            args.out.parent.mkdir(parents=True, exist_ok=True)
            args.out.write_text(json.dumps({'adc2_shared_irq': {
                'status': 'failed', 'checks': checks,
                'scope': 'Own-source/data comparison only; no HAL or silicon execution.'}}, indent=2) + '\n')
            raise AssertionError(name)

    nodes = {}
    for artifact in lock:
        entries = [(artifact['id'], artifact, None)]
        entries += [('member:' + m['path'], m, artifact) for m in artifact.get('members', [])]
        if 'text' in artifact:
            entries.append(('text:' + artifact['text']['path'], artifact['text'], artifact))
        for ref, node, parent in entries:
            assert ref not in nodes, 'duplicate current source ID: ' + ref
            nodes[ref] = (node, parent)

    sdk_ref = 'vendor:CW32L012_StandardPeripheralLib_V1.0.5.zip'
    cn = 'CW32L012_UserManual_CN_V1.4'
    en = 'CW32L012_UserManual_EN_V1.0'
    ds = 'CW32L012_DataSheet_CN_V1.0'
    svd_ref = 'member:cw32l012/IDEsupport/MDK/WHXY.CW32L012_DFP.1.0.2/SVD/CW32L012.svd'
    member_paths = ['inc/cw32l012.h', 'inc/cw32l012_adc.h', 'src/cw32l012_adc.c',
                    'inc/cw32l012_dac.h', 'src/cw32l012_dac.c',
                    'inc/cw32l012_sysctrl.h', 'src/cw32l012_sysctrl.c']
    required_refs = {sdk_ref, svd_ref}
    required_refs.update('member:cw32l012/Libraries/' + p for p in member_paths)
    required_refs.update(prefix + stem + suffix for stem in [cn, en, ds]
                         for prefix, suffix in [('vendor:', '.pdf'), ('text:', '.txt')])
    selected_refs = [s['source_ref'] for s in evidence['sources']]
    check('exact15 own-source identities', len(selected_refs) == 15 and set(selected_refs) == required_refs)
    check('current EN identity', nodes['vendor:' + en + '.pdf'][0]['sha256'] ==
          'f56d5ed899dd090b469fac6a09084aab9f5068ae3b56cf7562b83997bd2f1088')
    selected_bytes = {}
    selected_sources = []
    for source in evidence['sources']:
        ref = source['source_ref']
        expected, parent = nodes[ref]
        check('locked identity: ' + ref,
              all(source[k] == expected[k] for k in ['path', 'sha256', 'bytes']))
        contents = (args.sources / expected['path']).read_bytes()
        check('source bytes: ' + ref, len(contents) == expected['bytes'] and
              hashlib.sha256(contents).hexdigest() == expected['sha256'])
        if ref.startswith('vendor:'):
            check('original URL: ' + ref, source['url'] == expected['url'])
        else:
            check('source parent: ' + ref, source['original_source_ref'] == parent['id'])
        if ref.startswith('member:'):
            check('archive chain: ' + ref, source['archive_member_chain'] == expected['members'])
            content = (args.sources / parent['path']).read_bytes()
            for member_name in expected['members']:
                with zipfile.ZipFile(io.BytesIO(content)) as archive:
                    check('unique archive member: ' + member_name,
                          archive.namelist().count(member_name) == 1)
                    content = archive.read(member_name)
            check('archive/member correspondence: ' + ref, content == contents)
        selected_bytes[ref] = contents
        selected_sources.append({'source_ref': ref, 'path': expected['path'],
                                 'sha256': expected['sha256'], 'bytes': expected['bytes']})

    def references(value):
        if isinstance(value, dict):
            for key, item in value.items():
                if key == 'source_ref' or key.endswith('_source_ref'):
                    check('resolved evidence reference: ' + item, item in required_refs and item in nodes)
                else:
                    references(item)
        elif isinstance(value, list):
            for item in value:
                references(item)
    references(evidence)
    own_svd = ET.fromstring(selected_bytes[svd_ref])
    own_peripherals = {p.findtext('name'): p for p in own_svd.find('peripherals')}
    check('generator uses current own SVD', sha(svd_path) == nodes[svd_ref][0]['sha256'])
    check('ADC2 own derived layout', own_peripherals['ADC2'].get('derivedFrom') == 'ADC1')

    def source_geometry(name):
        result = {}
        for reg in own_peripherals[name].find('registers'):
            fields = {f.findtext('name'): [int(f.findtext('lsb'), 0),
                      int(f.findtext('msb'), 0) - int(f.findtext('lsb'), 0) + 1]
                      for f in reg.find('fields')}
            result[reg.findtext('name')] = {'offset': int(reg.findtext('addressOffset'), 0),
                'width': int(reg.findtext('size')), 'access': accesses[reg.findtext('access')],
                'fields': fields}
        return result

    def ir_geometry(ir_data, block):
        result = {}
        for reg in ir_data['block/' + block]['items']:
            array = reg.get('array')
            for index in range(array['len'] if array else 1):
                name = reg['name'] + (str(index) if array else '')
                fields = {}
                for field in ir_data['fieldset/' + reg['fieldset']]['fields']:
                    fa = field.get('array')
                    for fi in range(fa['len'] if fa else 1):
                        key = field['name'] + (str(fi) if fa else '')
                        assert key not in fields, 'duplicate expanded IR field: ' + block + '.' + name + '.' + key
                        fields[key] = [field['bit_offset'] + (fi * fa['stride'] if fa else 0), field['bit_size']]
                assert name not in result, 'duplicate expanded IR register: ' + block + '.' + name
                result[name] = {'offset': reg['byte_offset'] + (index * array['stride'] if array else 0),
                               'width': reg.get('bit_size', 32), 'access': reg.get('access', 'ReadWrite'),
                               'fields': fields}
        return result

    facts = evidence['hardware_facts']
    dac_regs = source_geometry('DAC')
    dac_ir = load(root / 'cw32-data/data/registers/dac_cw32l012_v1.json')
    check('complete DAC own SVD/canonical/generated parity', dac_regs ==
          ir_geometry(load(root / 'cw32-data/registers/dac_cw32l012_v1.yaml'), 'DAC') ==
          ir_geometry(dac_ir, 'DAC') == facts['dac_register_geometry'])
    check('DAC14 registers34 fields', len(dac_regs) == 14 and
          sum(len(r['fields']) for r in dac_regs.values()) == 34)
    cr0_fields = {'EN1': [0, 1], 'TEN1': [1, 1], 'TSEL1': [2, 4], 'WAVE1': [6, 2],
                  'MAMP1': [8, 4], 'DMAEN1': [12, 1], 'DMAUDRIE1': [13, 1],
                  'EN2': [16, 1], 'TEN2': [17, 1], 'TSEL2': [18, 4], 'WAVE2': [22, 2],
                  'MAMP2': [24, 4], 'DMAEN2': [28, 1], 'DMAUDRIE2': [29, 1]}
    check('complete DAC CR0 geometry', dac_regs['CR0'] ==
          {'offset': 0, 'width': 32, 'access': 'ReadWrite', 'fields': cr0_fields})
    check('DAC output switch geometry', dac_regs['CR1'] ==
          {'offset': 52, 'width': 32, 'access': 'ReadWrite', 'fields': {'C1OUT': [0, 1], 'C2OUT': [1, 1]}})
    for name, offset in [('DOR1', 44), ('DOR2', 48)]:
        check('DAC mixed status geometry: ' + name, dac_regs[name] ==
              {'offset': offset, 'width': 32, 'access': 'ReadWrite',
               'fields': {'DATA': [0, 12], 'DMAUDR': [15, 1]}})
    check('ADC canonical/generated parity', generated_regs ==
          ir_geometry(load(root / 'cw32-data/registers/adc_cw32l012_v1.yaml'), 'ADC'))
    adc_fields = {'EOC': [0, 1], 'EOS': [1, 1], 'AWDL': [3, 1], 'AWDH': [4, 1]}
    for name, offset, access in [('IER', 112, 'ReadWrite'), ('ISR', 120, 'Read'), ('ICR', 116, 'ReadWrite')]:
        fields = dict(adc_fields)
        if name == 'IER':
            fields.update(DMAEOC=[5, 1], DMAEOS=[6, 1])
        check('ADC interrupt geometry: ' + name, generated_regs[name] == facts['adc_interrupt_registers'][name] ==
              {'offset': offset, 'width': 32, 'access': access, 'fields': fields})

    # Each check is confined to the exact reviewed printed/PDF pages of the
    # locked text. Hashes alone and a self-declared evidence verdict do not
    # establish the interrupt, access, key or polarity facts below.
    section_pages = {
        'vectors': ([70, 71], [76]), 'dac_irq': ([607], [663]),
        'dac_cr0': ([609, 610, 611], [665, 666, 667, 668]),
        'dac_dor': ([615], [672]), 'dac_output': ([616], [673]),
        'adc_irq': ([596, 597], [651, 652]), 'adc_finite': ([581], [635]),
        'adc_abort': ([590], [644]), 'adc_gate': ([57, 58], [62, 63]),
        'adc_reset': ([61, 62], [66, 67]), 'dac_gate': ([59], [64]),
        'dac_reset': ([63], [68]), 'dac_analog_limit': ([62],),
    }
    sections = {s['id']: s for s in evidence['manual_sections']}
    check('exact qualified manual sections', len(sections) == len(evidence['manual_sections']) and
          set(sections) == set(section_pages))
    page_receipts = []
    text_sections = {}
    for name, page_groups in section_pages.items():
        stems = [ds] if name == 'dac_analog_limit' else [cn, en]
        expected_locators = [{'source_ref': 'vendor:' + stem + '.pdf',
            'text_source_ref': 'text:' + stem + '.txt', 'printed_pages': pages,
            'pdf_pages_1_based': [p + (3 if stem == ds else 26) for p in pages]}
            for stem, pages in zip(stems, page_groups)]
        check('exact section locators: ' + name, sections[name]['locators'] == expected_locators)
        for stem, locator in zip(stems, expected_locators):
            pages = selected_bytes[locator['text_source_ref']].decode('utf-8').split('\f')
            selected = [pages[p - 1] for p in locator['pdf_pages_1_based']]
            text_sections[name, stem] = re.sub(r'\s+', ' ', ' '.join(selected))
            page_receipts.append({'section': name, **locator,
                'page_text_sha256': [hashlib.sha256(p.encode()).hexdigest() for p in selected]})

    def matches(name, stem, *patterns):
        section = text_sections[name, stem]
        check('own manual facts: ' + name + '/' + stem,
              all(re.search(pattern, section) is not None for pattern in patterns))

    for stem in [cn, en]:
        matches('vectors', stem, r'28\s+12\s+\S+\s+ADC1', r'46\s+30\s+\S+\s+ADC2/DAC')
        matches('dac_irq', stem, r'DOR1\.DMAUDR1', r'CR0\.\s*DMAUDRIE1',
                r'DOR2\.DMAUDR2', r'CR0\.DMAUDRIE2')
        matches('dac_cr0', stem, r'Address offset:\s*0x00', *[
            r'\b' + (str(bit + width - 1) + ':' + str(bit) if width > 1 else str(bit)) +
            r'\s+' + name + r'\s+RW\b' for name, (bit, width) in cr0_fields.items()])
        matches('dac_dor', stem, r'Address offset:\s*0x2C', r'Address offset:\s*0x30',
                r'15\s+DMAUDR1\s+RW0', r'15\s+DMAUDR2\s+RW0',
                r'11:0\s+DACC1DOR\s+RO', r'11:0\s+DACC2DOR\s+RO')
        matches('dac_output', stem, r'0\s+DAC_OUT1\s+RW', r'1\s+DAC_OUT2\s+RW')
        matches('adc_irq', stem, r'Address offset:\s*0x70', r'Address offset:\s*0x78',
                r'Address offset:\s*0x74\s+Reset value:\s*0x0000 000F', r'2\s+RFU',
                *[str(bit) + r'\s+' + name + r'\s+' + access
                  for name, (bit, _) in adc_fields.items() for access in ['RW', 'RO', 'R1W0']],
                r'5\s+DMAEOC\s+RW', r'6\s+DMAEOS\s+RW')
        matches('adc_finite', stem, r'ADCx_ISR\.EOC', r'ADCx_ISR\.EOS', r'START', r'SQRCH0')
        matches('adc_abort', stem, r'Address offset:\s*0x04', r'0\s+START\s+RW')
        for name, register, offset, field, bit in [
                ('adc_gate', 'APBEN1', '38', 'ADC', 0), ('dac_gate', 'APBEN2', '34', 'DAC', 10),
                ('adc_reset', 'APBRST1', '48', 'ADC', 0), ('dac_reset', 'APBRST2', '44', 'DAC', 10)]:
            matches(name, stem, 'SYSCTRL_' + register, 'Address offset: 0x' + offset,
                    str(bit) + r'\s+' + field + r'\s+RW')
            if name.endswith('gate'):
                matches(name, stem, r'31:16\s+KEY\s+WO', r'0x5A5A',
                        str(bit) + r'\s+' + field + r'\s+RW\s+0[:：]\s*' +
                        ('禁止' if stem == cn else 'Disabled') + r'\s+1[:：]\s*' +
                        ('使能' if stem == cn else 'Enabled'))
            else:
                matches(name, stem, str(bit) + r'\s+' + field + r'\s+RW\s+0[:：]\s*' +
                        ('模块处于复位状态' if stem == cn else r'Module is in reset state\.') +
                        r'\s+1[:：]\s*' + ('模块正常工作' if stem == cn else r'Module works fine\.'))
    matches('dac_irq', cn, r'DAC 支持 2 个中断源', r'如果设置了对应的中断使能控制\s*位，将产生中断请求')
    matches('dac_irq', en, r'DAC supports 2 interrupt sources', r'If the corresponding interrupt enable control bit is set, an interrupt request will be generated')
    matches('dac_output', cn, r'内部互联不受影响')
    matches('dac_output', en, r'internal interconnect is not affected')
    matches('adc_finite', cn, r'START 位自动清 0')
    matches('adc_finite', en, r'START bit is automatically cleared to 0')
    matches('adc_abort', cn, r'软件写 0 可停止转换并复位序列的转换通道')
    matches('adc_abort', en, r'Software write 0 to stop conversion and reset the conversion channel of sequence')
    matches('dac_cr0', cn, r'DAC\s*时钟使能后，DHRx\s*的值就会更新到\s*DOR')
    matches('dac_cr0', en, r'After the DAC clock is enabled, the value of DHRx is updated to DOR')
    check('DAC clock note does not identify configuration gate',
          all('APBEN2' not in text_sections['dac_cr0', stem] for stem in [cn, en]))
    matches('dac_analog_limit', ds, r'7-30', r'tSTART\s*\[3\]\s*建立时间\s*-\s*3\s*-\s*μs')

    def member_text(path):
        return selected_bytes['member:cw32l012/Libraries/' + path].decode('utf-8-sig')
    header = member_text('inc/cw32l012.h')
    for name, number in [('ADC1', 12), ('ADC2_DAC', 30)]:
        check('CMSIS IRQ: ' + name, re.search(r'\b' + name + r'_IRQn\s*=\s*' + str(number) + r'\b', header) is not None)
    positions = {'ADC_IER_' + name: bit for name, (bit, _) in generated_regs['IER']['fields'].items()}
    positions.update({'DAC_CR0_' + name: bit for name, (bit, _) in cr0_fields.items()})
    positions.update({'SYSCTRL_APBEN1_ADC': 0, 'SYSCTRL_APBEN1_KEY': 16, 'SYSCTRL_APBRST1_ADC': 0,
                      'SYSCTRL_APBEN2_DAC': 10, 'SYSCTRL_APBEN2_KEY': 16, 'SYSCTRL_APBRST2_DAC': 10})
    for name, bit in positions.items():
        check('CMSIS field: ' + name, re.search(r'#define\s+' + name + r'_Pos\s+\(' + str(bit) + r'UL\)', header) is not None)
    sdk_checks = {
        'inc/cw32l012_adc.h': [r'ADC_IT_EOS\s+\(\(uint16_t\)0x0002\)', r'ADC_IT_EOC\s+\(\(uint16_t\)0x0001\)',
                              r'ADC_IT_AWDH\s+\(\(uint16_t\)0x0008\)', r'ADC_IT_AWDL\s+\(\(uint16_t\)0x0004\)'],
        'src/cw32l012_adc.c': [r'ADCx->ICR = 0x0000;', r'REGBITS_CLR\(ADCx->ICR, ADC_IT\)'],
        'inc/cw32l012_dac.h': [r'DAC_IT_DMA_UNDER\s+\(0x8000\)', r'void DAC_ITConfig\(', r'void DAC_ClearFlag\('],
        'src/cw32l012_dac.c': [r'CR0_f\.DMAUDRIE1 = 1', r'CR0_f\.DMAUDRIE2 = 1',
                              r'DOR1_f\.DMAUDR = 0', r'DOR2_f\.DMAUDR = 0'],
        'inc/cw32l012_sysctrl.h': [r'SYSCTRL_KEY\s+\(0x5A5A0000U\)', r'SYSCTRL_APB1_PERIPH_ADC\s+bv0',
                                  r'SYSCTRL_APB2_PERIPH_DAC\s+bv10',
                                  r'__SYSCTRL_ADC_RST_ENABLE\(\)\s+REGBITS_CLR\(CW_SYSCTRL->APBRST1',
                                  r'__SYSCTRL_DAC_RST_ENABLE\(\)\s+REGBITS_CLR\(CW_SYSCTRL->APBRST2'],
        'src/cw32l012_sysctrl.c': [r'CW_SYSCTRL->APBEN1 \|= SYSCTRL_KEY \| Periph;',
                                  r'CW_SYSCTRL->APBEN2 \|= SYSCTRL_KEY \| Periph;'],
    }
    for path, patterns in sdk_checks.items():
        check('own SDK corroboration: ' + path, all(re.search(p, member_text(path)) for p in patterns))

    expected_instances = {
        'ADC1': (0x40000000, 'adc', 'ADC', 'ADC1', 12),
        'ADC2': (0x40000100, 'adc', 'ADC', 'ADC2_DAC', 30),
        'DAC': (0x400000c0, 'dac', 'DAC', 'ADC2_DAC', 30),
    }
    for name, (address, kind, block, irq, number) in expected_instances.items():
        source = own_peripherals[name]
        irqs = source.findall('interrupt')
        check('own SVD identity: ' + name, int(source.findtext('baseAddress'), 0) == address and
              len(irqs) == 1 and irqs[0].findtext('name') == irq and int(irqs[0].findtext('value')) == number)
        check('evidence instance identity: ' + name, facts['instances'][name] ==
              {'address': address, 'kind': kind, 'version': 'cw32l012_v1', 'block': block,
               'interrupt': irq, 'number': number})
    own_owners = sorted(p.findtext('name') for p in own_svd.find('peripherals')
                        for irq in p.findall('interrupt') if int(irq.findtext('value')) == 30)
    check('complete own SVD IRQ30 owners', own_owners == facts['shared_irq_owners'] == ['ADC2', 'DAC'])

    domains = {}
    for name in expected_instances:
        adc = name != 'DAC'
        enable, reset, field = ('APBEN1', 'APBRST1', 'ADC') if adc else ('APBEN2', 'APBRST2', 'DAC')
        domains[name] = {'controller': 'SYSCTRL', 'bus_clock': 'PCLK',
            'enable': {'register': enable, 'field': field}, 'enable_active_value': True,
            'enable_write_key': {'field': {'register': enable, 'field': 'KEY'}, 'value': 23130},
            'reset': {'register': reset, 'field': field}, 'reset_asserted_value': False}
        if adc:
            domains[name].update(shared_enable_group='ADC1-ADC2', shared_reset_group='ADC1-ADC2')
    check('evidence exact domains', facts['domains'] == domains)
    expected_sysctrl = {'APBEN1': {'offset': 56, 'ADC': [0, 1], 'KEY': [16, 16]},
                        'APBEN2': {'offset': 52, 'DAC': [10, 1], 'KEY': [16, 16]},
                        'APBRST1': {'offset': 72, 'ADC': [0, 1]},
                        'APBRST2': {'offset': 68, 'DAC': [10, 1]}}
    check('evidence SYSCTRL geometry', facts['sysctrl_geometry'] == expected_sysctrl)
    sys_source = source_geometry('SYSCTRL')
    sys_ir = ir_geometry(load(root / 'cw32-data/data/registers/sysctrl_cw32l012_v1.json'), 'SYSCTRL')
    sys_canonical = ir_geometry(load(root / 'cw32-data/registers/sysctrl_cw32l012_v1.yaml'), 'SYSCTRL')
    for name, expected in expected_sysctrl.items():
        for label, registers in [('own SVD', sys_source), ('canonical', sys_canonical), ('generated', sys_ir)]:
            reg = registers[name]
            check(label + ' SYSCTRL domain: ' + name, reg['offset'] == expected['offset'] and
                  reg['width'] == 32 and reg['access'] == 'ReadWrite' and
                  all(reg['fields'][field] == value for field, value in expected.items() if field != 'offset'))
    selections = ['CW32L012', 'CW32L012C8T6', 'CW32L012C8U6']
    check('three qualified selections', facts['selections'] == selections)
    for chip in selections:
        core = load(root / ('cw32-data/data/chips/' + chip + '.json'))['cores'][0]
        by_name = {}
        for p in core['peripherals']:
            check(chip + ' unique peripheral: ' + p['name'], p['name'] not in by_name)
            by_name[p['name']] = p
        irq_entries = core['interrupts']
        for irq, number in [('ADC1', 12), ('ADC2_DAC', 30)]:
            selected = [i for i in irq_entries if i['name'] == irq or i['number'] == number]
            check(chip + ' unique vector: ' + irq, selected == [{'name': irq, 'number': number}])
        owners = {12: [], 30: []}
        for p in core['peripherals']:
            for link in p.get('interrupts', []):
                resolved = [i for i in irq_entries if i['name'] == link['interrupt']]
                check(chip + ' resolved IRQ link: ' + p['name'] + '/' + link['signal'], len(resolved) == 1)
                if resolved[0]['number'] in owners:
                    owners[resolved[0]['number']].append((p['name'], link['signal'], link['interrupt']))
        check(chip + ' dedicated ADC1 IRQ12', owners[12] == [('ADC1', 'GLOBAL', 'ADC1')])
        check(chip + ' complete IRQ30 owner/link set', sorted(owners[30]) ==
              [('ADC2', 'GLOBAL', 'ADC2_DAC'), ('DAC', 'GLOBAL', 'ADC2_DAC')])
        for name, (address, kind, block, irq, _) in expected_instances.items():
            p = by_name[name]
            check(chip + ' peripheral identity: ' + name, p['address'] == address and
                  p['registers'] == {'kind': kind, 'version': 'cw32l012_v1', 'block': block} and
                  p['interrupts'] == [{'interrupt': irq, 'signal': 'GLOBAL'}])
            check(chip + ' exact RCC_CONTROL: ' + name, p['rcc_control'] == domains[name])
        check(chip + ' SYSCTRL identity', by_name['SYSCTRL']['address'] == 0x40004000 and
              by_name['SYSCTRL']['registers'] == {'kind': 'sysctrl', 'version': 'cw32l012_v1', 'block': 'SYSCTRL'})
        check(chip + ' DAC kernel qualification unchanged', 'rcc' not in by_name['DAC'])
    dac_writes = load(root / 'cw32-data/data/register-writes/dac_cw32l012_v1.json')['registers']['dac_cw32l012_v1']
    check('two DAC existing W0C seeds', len(dac_writes) == 2 and
          {w['register'] for w in dac_writes} == {'DOR1', 'DOR2'} and
          all(w['write_noop'] == 32768 and w['reset_value'] == 0 and w['zero_to_clear_fields'] == ['DMAUDR']
              for w in dac_writes))
    check('ADC reserved-bit seed retained', writes[0]['zero_to_clear_fields'] == ['EOC', 'EOS', 'AWDL', 'AWDH'] and
          facts['adc_icr'] == {'write_noop': 31, 'reset_value': 15,
              'zero_to_clear_fields': ['EOC', 'EOS', 'AWDL', 'AWDH'], 'reserved_bit_2_preserved': True})
    check('DAC mixed-access evidence', facts['dac_dor'] == {'hardware_data_access': 'Read', 'flag_bit': 15,
          'flag_semantics': 'write_zero_to_clear', 'write_noop': 32768, 'reset_value': 0})
    return {'status': 'passed', 'evidence_sha256': sha(evidence_path), 'sources': selected_sources,
            'selections_checked': len(selections), 'adc_instances_checked': 6,
            'shared_irq_number': 30, 'shared_irq_owners': own_owners,
            'dac_register_geometry': dac_regs, 'domains': domains, 'sysctrl_geometry': expected_sysctrl,
            'manual_sections_checked': page_receipts, 'checks': checks,
            'scope': 'Own-source/data comparison only. No HAL source assertions, model, build or silicon execution.'}


shared_irq_receipt = verify_adc2_shared_irq()

receipt = {'source_verification': 'passed', 'sources': source_receipts,
           'svd_sha256': sha(svd_path), 'expanded_register_count': len(source_regs),
           'expanded_field_count': sum(len(r['fields']) for r in source_regs.values()),
           'expanded_svd_parity': 'passed, with previously reviewed ISR RO correction',
           'canonical_ir_sha256': digest, 'chip_instances_checked': 6,
           'icr_noop': 31, 'icr_reset': 15, 'adc2_shared_irq': shared_irq_receipt,
           'scope': 'Data comparison only. No HAL tests, firmware execution or silicon validation.'}
args.out.parent.mkdir(parents=True, exist_ok=True)
args.out.write_text(json.dumps(receipt, indent=2) + '\n')
summary = {k: v for k, v in receipt.items() if k not in ['sources', 'adc2_shared_irq']}
summary['adc2_shared_irq'] = {k: shared_irq_receipt[k] for k in
    ['status', 'evidence_sha256', 'selections_checked', 'adc_instances_checked',
     'shared_irq_number', 'shared_irq_owners']}
summary['adc2_shared_irq']['checks_passed'] = len(shared_irq_receipt['checks'])
print(json.dumps(summary, indent=2))
