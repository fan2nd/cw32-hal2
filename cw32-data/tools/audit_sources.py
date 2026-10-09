#!/usr/bin/env python3
"""Independent read-only audit of pinned raw SVDs and import configuration."""
import hashlib, json, xml.etree.ElementTree as ET
from pathlib import Path
import yaml
ROOT = Path(__file__).resolve().parents[2]
checks = 0
for manifest in sorted((ROOT / 'cw32-data/inputs').glob('*.yaml')):
    spec = yaml.safe_load(manifest.read_text())
    data = (ROOT / spec['source']['path']).read_bytes()
    assert hashlib.sha256(data).hexdigest() == spec['source']['sha256'], manifest
    root = ET.fromstring(data)
    assert root.findtext('name') == spec['expected_svd_name']
    assert not root.findall('.//dim') and not root.findall('.//cluster'), 'array/cluster audit not implemented'
    names = {p.findtext('name') for p in root.findall('./peripherals/peripheral')}
    assert len(names) == len(root.findall('./peripherals/peripheral'))
    aliases = {frozenset((a['block'],a['register'],a['alias'])) for a in spec['register_aliases']}
    irqs = {}
    for p in root.findall('./peripherals/peripheral'):
        if p.get('derivedFrom'):
            assert p.get('derivedFrom') in names
        block = p.findtext('headerStructName',p.findtext('name'))
        regs = []
        for r in p.findall('./registers/register'):
            name = r.findtext('name')
            offset = int(r.findtext('addressOffset'),0)
            width = int(r.findtext('size',p.findtext('size',root.findtext('size','32'))),0)
            assert width in (8,16,32,64), (manifest,block,name,width)
            for old,off,w in regs:
                if offset < off+w//8 and off < offset+width//8:
                    assert frozenset((block,name,old)) in aliases, (manifest,block,name,old)
            regs.append((name,offset,width))
            field_bits = set()
            for f in r.findall('./fields/field'):
                if f.find('lsb') is not None:
                    low,high = int(f.findtext('lsb'),0),int(f.findtext('msb'),0)
                else:
                    low = int(f.findtext('bitOffset'),0)
                    high = low+int(f.findtext('bitWidth'),0)-1
                assert 0 <= low <= high < width, (manifest,block,name,f.findtext('name'),low,high,width)
                bits = set(range(low,high+1))
                assert not bits & field_bits, (manifest,block,name,f.findtext('name'))
                field_bits |= bits
                checks += 1
        for irq in p.findall('./interrupt'):
            name, number = irq.findtext('name').strip().upper(),int(irq.findtext('value'),0)
            assert name not in irqs or irqs[name] == number
            irqs[name] = number
    for irq in spec.get('supplemental_interrupts',[]):
        assert irq['evidence']
        assert irq['name'] not in irqs or irqs[irq['name']] == irq['number']
        irqs[irq['name']] = irq['number']
    assert len(set(irqs.values())) == len(irqs), ('IRQ number aliases',manifest)
    print(spec['line'], 'quarantined' if spec.get('quarantine') else 'eligible', len(names), 'peripherals;',len(irqs),'IRQs')
print(f'Checked hashes, references, register widths/spans, {checks} field ranges and interrupt uniqueness')
