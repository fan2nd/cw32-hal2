#!/usr/bin/env python3
"""Own-source HALLTIM data/PAC qualification; never executes HAL or MMIO."""
import argparse, hashlib, io, json, re, zipfile
from pathlib import Path
import yaml
ROOT = Path(__file__).resolve().parents[1]
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
def load(p): return yaml.safe_load(p.read_text()) if p.suffix == '.yaml' else json.loads(p.read_text())
def digest(v): return hashlib.sha256(json.dumps(v, sort_keys=True, separators=(',', ':')).encode()).hexdigest()
def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--sources', required=True, type=Path)
    args = parser.parse_args()
    proof = load(ROOT/'docs/halltim-evidence.json')
    lock = load(ROOT/'sources/evidence-sources.json')
    nodes = {}
    for artifact in lock['artifacts']:
        nodes['vendor:'+artifact['path']] = artifact
        for member in artifact.get('members', []): nodes['member:'+member['path']] = member
    for s in proof['sources']:
        assert s['sha256'] == nodes[s['source_ref']]['sha256'] == sha(args.sources/s['path']), s['path']
        if 'members' in s:
            data=(args.sources/'CW32L012_StandardPeripheralLib_V1.0.5.zip').read_bytes()
            for member in s['members']:
                with zipfile.ZipFile(io.BytesIO(data)) as archive: data=archive.read(member)
            assert hashlib.sha256(data).hexdigest() == s['sha256']
    for name in ['CW32L012_UserManual_CN_V1.4.pdf', 'CW32L012_DataSheet_CN_V1.0.pdf']:
        entry=nodes['vendor:'+name]['text']
        assert sha(args.sources/entry['path']) == entry['sha256']
    pages=(args.sources/'CW32L012_UserManual_CN_V1.4.txt').read_text().split('\f')
    for cite in proof['manual_citations'].values():
        if 'pdf_page' in cite and cite['section'] != 'revision history':
            assert cite['section'] in pages[cite['pdf_page']-1]
    ir=yaml.safe_load((ROOT/'cw32-data/registers/halltim_cw32l012_v1.yaml').read_text())
    generated=load(ROOT/'cw32-data/data/registers/halltim_cw32l012_v1.json')
    assert ir == generated
    group=next(g for g in load(ROOT/'cw32-data/register-reuse.yaml')['groups'] if g['canonical']=='halltim_cw32l012_v1.yaml')
    assert group['canonical_ir_sha256'] == digest(generated)
    assert group['source_versions'] == ['halltim_cw32l012_v1.yaml']
    items={i['name']:i for i in ir['block/HALLTIM']['items']}
    assert {n:v['byte_offset'] for n,v in items.items()} == {'CR':0,'DIER':4,'CNT':8,'ARR':12,'WIDTH':16,'CCR':20,'ISR':24,'ICR':28,'STATE':32}
    assert all(items[n]['access']=='Read' for n in ['ISR','WIDTH','STATE'])
    for name in ['CNT','ARR','WIDTH','CCR']: assert ir['fieldset/'+name]['fields'][0]['bit_size']==24
    header=(args.sources/'cw32l012/Libraries/inc/cw32l012.h').read_text()
    for field,offset,width in [('DIV',2,2),('MMS',4,3),('FLT2LEN',8,15),('FLT1EN',1,1),('EN',0,1),('SOFTCAP',23,1)]:
        f=next(f for f in ir['fieldset/CR']['fields'] if f['name']==field)
        assert (f['bit_offset'],f['bit_size'])==(offset,width)
        assert re.search(r'#define\s+HALLTIM_CR_'+field+r'_Pos\s+\('+str(offset)+r'UL\)',header)
        span=str(offset) if width==1 else f'{offset+width-1}:{offset}'
        assert re.search(r'(?m)^\s*'+span+r'\s+'+field+r'\s+',pages[446]),field
    assert re.search(r'BTIM3_HALLTIM_IRQn\s*=\s*22',header)
    for name in ['CNT','ARR','WIDTH','CCR','ISR','ICR','STATE']:
        assert re.search(r'HALLTIM_'+name+r'\s+HALLTIM_BASE\s*\+0x'+f'{items[name]["byte_offset"]:02X}',pages[445])
    assert 'RW0' in pages[447]
    assert 'R1W0' in pages[449]
    assert [v['name'] for v in ir['enum/Prescaler']['variants']]==['DIV1','DIV2','DIV4','DIV8']
    assert [v['value'] for v in ir['enum/MasterMode']['variants']]==list(range(8))
    assert [v['name'] for v in ir['enum/MasterMode']['variants']]==['OVERFLOW','PWM','MATCH','CAPTURE','CH3','CH2','CH1','XOR']
    capture=re.sub(r'\s+','',pages[441]); assert all(s in capture for s in ['WIDTH','硬件清零CNT','任意一路有电平变化','CAPF'])
    for page,register in [(84,'APBEN2'),(88,'APBRST2')]:
        assert re.search(r'(?m)^\s*12\s+HALLTIM\s+RW',pages[page])
        if register=='APBEN2': assert '0x5A5A' in pages[page]
    seed=load(ROOT/'cw32-data/register-writes.yaml')['registers']['halltim_cw32l012_v1'][0]
    assert (seed['reset_value'],seed['write_noop'])==(65535,65535)
    assert seed['zero_to_clear_fields']==['CAPF','OVF','MATCHF']
    routes=load(ROOT/'cw32-data/af/cw32l012-halltim.yaml')['routes']
    ds=(args.sources/'CW32L012_DataSheet_CN_V1.0.txt').read_text().split('\f')[38]
    gpio=(args.sources/'cw32l012/Libraries/inc/cw32l012_gpio.h').read_text().splitlines()
    pinout=load(ROOT/'cw32-data/pinouts/cw32l012.yaml')
    assert len(routes)==12
    for r in routes:
        pin=r['pin']; n=int(pin[2:]); spelling=pin[:2]+f'{n:02}'
        assert re.search(r'#define\s+'+r['source_macro']+r'\(\)\s+\(CW_GPIO'+pin[1]+r'->'+r['gpio_register']+r'_f.PIN'+str(n)+r'\s*=\s*9\)',gpio[r['source_line']-1])
        row=next(line for line in ds.splitlines() if re.match(r'\s*'+spelling+r'\s+',line))
        assert row.split()[-1]=='HALLTIM_'+r['signal']
        source=next(row for row in pinout['table_rows'] if pin in row['signals'])
        assert r['package_positions']==source['positions']
    for path in (ROOT/'cw32-data/data/chips').glob('*.json'):
        chip=load(path); core=chip['cores'][0]; halls=[p for p in core['peripherals'] if p['name']=='HALLTIM']
        if chip['line']!='CW32L012': assert not halls;continue
        assert len(halls)==1
        hall=halls[0]; bonded={p['name'] for p in core['pins']}
        assert hall['pins']==sorted([{'pin':r['pin'],'signal':r['signal'],'af':9} for r in routes if r['pin'] in bonded],key=lambda r:(r['pin'],r['signal'],r['af']))
        assert len(hall['pins'])==12,chip['name']
        assert hall['rcc']['kernel_clock']=='PCLK'
        assert hall['interrupts']==[{'interrupt':'BTIM3_HALLTIM','signal':'GLOBAL'}]
        assert hall['rcc_control']['reset_asserted_value'] is False
        print(f'PASS {chip["name"]}: 12 bonded own-source AF9 capture routes, PCLK/RCC/IRQ identity')
    print('PASS HALLTIM original archive/member hashes, manual fields/semantics, 24-bit access, enums, R1W0 seeds, normalized reuse fingerprint, all non-L012 absence')
if __name__=='__main__': main()
