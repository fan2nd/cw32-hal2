#!/usr/bin/env python3
"""Independent data/source comparison; no HAL execution, fixtures or mocks."""
import argparse
import hashlib
import json
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
receipt = {'source_verification': 'passed', 'sources': source_receipts,
           'svd_sha256': sha(svd_path), 'expanded_register_count': len(source_regs),
           'expanded_field_count': sum(len(r['fields']) for r in source_regs.values()),
           'expanded_svd_parity': 'passed, with previously reviewed ISR RO correction',
           'canonical_ir_sha256': digest, 'chip_instances_checked': 6,
           'icr_noop': 31, 'icr_reset': 15,
           'scope': 'Data comparison only. No HAL tests, firmware execution or silicon validation.'}
args.out.parent.mkdir(parents=True, exist_ok=True)
args.out.write_text(json.dumps(receipt, indent=2) + '\n')
print(json.dumps({k: v for k, v in receipt.items() if k != 'sources'}, indent=2))
