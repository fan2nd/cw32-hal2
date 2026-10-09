#!/usr/bin/env python3
"""Own-source/data/PAC analog-output audit. No HAL code or firmware is executed."""
import hashlib,json,re,os,zipfile
from pathlib import Path
import yaml
ROOT=Path(__file__).resolve().parents[1]
S=Path(os.environ.get('CW32_SOURCES','/workspace/shared/cw32-sources'))
def load(p):return yaml.safe_load(p.read_text()) if p.suffix == '.yaml' else json.loads(p.read_text())
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
c=load(ROOT/'cw32-data/dac-opa.yaml');e=load(ROOT/'docs/dac-opa-evidence.json');lock=load(ROOT/'sources/evidence-sources.json')
for artifact in e['artifacts']:
 assert sha(S/artifact['path'])==artifact['sha256']
 if 'text' in artifact:assert sha(S/artifact['text']['path'])==artifact['text']['sha256']
sdk=next(a for a in lock['artifacts'] if a['path']=='CW32L012_StandardPeripheralLib_V1.0.5.zip')
with zipfile.ZipFile(S/sdk['path']) as z:
 for source in e['additional_verified_sdk_members']:
  member=next(m for m in sdk['members'] if m['path']==source['path'])
  assert member['sha256']==source['sha256']==sha(S/source['path'])
  assert hashlib.sha256(z.read(member['members'][0])).hexdigest()==source['sha256']
manual=(S/'CW32L012_UserManual_CN_V1.4.txt').read_text(); datasheet=(S/'CW32L012_DataSheet_CN_V1.0.txt').read_text()
# Exact paired Table29-1 cells, not route records used to generate the result.
expected=[]
for signal,p1,p2 in [('INP1','PA03','PA04'),('INP2','PA06','PA06'),('INP3','PB01','PB00'),('INN1','PA04','PA05'),('INN2','PA07','PA07'),('OUT','PB00','PB01')]:
 assert re.search(rf'\b{signal}\s+{p1}\s+{signal}\s+{p2}\b',manual),(signal,p1,p2)
 for instance,pin in [('OPA1',p1),('OPA2',p2)]:expected.append((instance,pin[:2]+str(int(pin[2:])),signal))
expected += [('DAC','PB0','OUT1'),('DAC','PB1','OUT2')]
assert sorted((x['peripheral'],x['pin'],x['signal']) for x in c['routes'])==sorted(expected)
assert 'DAC_OUT1, OPA1_OUT' in datasheet and 'OPA2_OUT' in datasheet
assert 'Reset value: 0x0000 E000' in manual
assert c['limits']=={'dac':{'supply_mv':[1700,5500],'resolution_bits':12},'opa':{'supply_mv':[1700,5500],'output_headroom_mv':100}}
# Numeric facts tied to the exact own-family tables and checked page image (TSTART column).
assert re.search(r'VDDA\s+DAC开启时的供电电压\s+-\s+1.7\s+-\s+5.5',datasheet)
assert re.search(r'VOUT\s+输出电压\s+-\s+0.1\s+-\s+VDDA -0.1',datasheet)
assert re.search(r'TSTART\s+初始化时间\s+-\s+2.5\s+-\s+-',datasheet)
for feature in ['CW32L012','CW32L012C8T6','CW32L012C8U6']:
 chip=load(ROOT/f'cw32-data/data/chips/{feature}.json');ps=chip['cores'][0]['peripherals'];pins={p['name'] for p in chip['cores'][0]['pins']}
 for name in ['DAC','OPA1','OPA2']:
  p=next(p for p in ps if p['name']==name);want=sorted((pin,sig) for n,pin,sig in expected if n==name and pin in pins)
  assert sorted((p['pin'],p['signal']) for p in p['pins'])==want
  assert p['rcc_control']['enable']['field']==('DAC' if name=='DAC' else 'OPA')
  assert p['rcc_control']['reset_asserted_value'] is False
  assert p['dac_limits' if name=='DAC' else 'opa_limits']==c['limits']['dac' if name=='DAC' else 'opa']
for reg in ['dac','opa']:
 ir=load(ROOT/f'cw32-data/data/registers/{reg}_cw32l012_v1.json')
 groups=load(ROOT/'cw32-data/register-reuse.yaml')['groups'];g=next(g for g in groups if g['canonical']==f'{reg}_cw32l012_v1.yaml')
 assert hashlib.sha256(json.dumps(ir,sort_keys=True,separators=(',',':')).encode()).hexdigest()==g['canonical_ir_sha256']
 if reg=='opa':
  assert {v['value'] for v in ir['enum/Gain']['variants']}==set(range(5))
  assert [v['value'] for v in ir['enum/Mode']['variants']]==[0,1,2,3]
  assert len(ir['enum/Bias']['variants'])==8
 else:
  assert len(ir['enum/Trigger']['variants'])==16
  for i in [1,2]:
   w=next(w for w in load(ROOT/'cw32-data/data/register-writes/dac_cw32l012_v1.json')['registers']['dac_cw32l012_v1'] if w['register']==f'DOR{i}')
   assert w['write_noop']==32768 and w['reset_value']==0 and w['zero_to_clear_fields']==['DMAUDR']
print('PASS: L012 own-source identities, SDK members, 14 analog routes, 3 selectors, supply/headroom facts, semantic enums, RCC controls and DAC W0C seeds; no HAL tests or hardware execution')
