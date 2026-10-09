#!/usr/bin/env python3
"""Source/data/PAC LCD audit. Does not execute or simulate HAL firmware."""
import hashlib,json,os,re,sys
from pathlib import Path
import yaml,pdfplumber
ROOT=Path(__file__).resolve().parents[1]
S=Path(os.environ.get('CW32_SOURCES','/workspace/shared/cw32-sources'))
def load(p):return yaml.safe_load((ROOT/p).read_text()) if (ROOT/p).suffix == '.yaml' else json.loads((ROOT/p).read_text())
def digest(x):return hashlib.sha256(json.dumps(x,sort_keys=True,separators=(',',':')).encode()).hexdigest()
data=load('cw32-data/lcd.yaml');proof=load('docs/lcd-evidence.json');lock=load('sources/evidence-sources.json')
assert data['schema_version']==proof['schema_version']==1
for fam,p in data['profiles'].items():
 e=proof['families'][fam];assert p['facts']==e['facts'];sources=e['sources']
 for name,s in sources.items():
  path=S/s['path'];assert hashlib.sha256(path.read_bytes()).hexdigest()==s['sha256'],path
  if 'member' in s:
   import zipfile
   a=next(a for a in lock['artifacts'] if a['path']==s['archive']);assert a['sha256']==s['archive_sha256'];assert a['url']==s['url']
   m=next(m for m in a['members'] if m['path']==s['path']);assert m['sha256']==s['sha256'] and m['members']==[s['member']]
   assert zipfile.ZipFile(S/s['archive']).read(s['member'])==path.read_bytes()
 pinout=load('cw32-data/pinouts/'+fam.lower()+'.yaml');expected=[]
 with pdfplumber.open(S/sources['datasheet']['path']) as pdf:
  for page in pinout['source']['table_pdf_page_indices']:
   for table in pdf.pages[page].extract_tables():
    for row in table:
     names=[c for c in row if c and re.fullmatch(r'P[A-F]\d{1,2}(?:/\n.*)?',c)]
     if not names:continue
     n=re.match(r'P([A-F])0?(\d+)',names[0]);pin='P'+n[1]+str(int(n[2]));cell=row[-1] or ''
     for signal in re.findall(r'\b(?:SEG\d+|COM[0-7]|VLCD[1-4])\b',cell):
      r=next(r for r in pinout['table_rows'] if pin in r['signals'])
      expected.append(dict(pin=pin,signal=signal,table='5-2',pdf_page=page+1,analog_cell=cell,positions=r['positions']))
 assert p['routes']==sorted(expected,key=lambda r:(r['pin'],r['signal'])),fam
 segments={int(r['signal'][3:]) for r in p['routes'] if r['signal'].startswith('SEG')}
 assert segments==set(p['facts']['segments'])
 assert p['facts']['ram_registers']==sorted({s//4 for s in segments})
 ir=yaml.safe_load((ROOT/f'cw32-data/registers/lcd_{fam.lower()}_v1.yaml').read_text());gen=load(f'cw32-data/data/registers/lcd_{fam.lower()}_v1.json')
 assert ir==gen
 assert next(g for g in load('cw32-data/register-reuse.yaml')['groups'] if g['canonical']==f'lcd_{fam.lower()}_v1.yaml')['canonical_ir_sha256']==digest(ir)
 ram={r['name']:r['byte_offset'] for r in ir['block/LCD']['items'] if r['name'].startswith('RAM')}
 assert ram=={f'RAM{n}':64+4*n for n in p['facts']['ram_registers']}
 assert [(v['name'],v['value']) for v in ir['enum/Duty']['variants']]==[('Static',0),('Half',1),('Third',2),('Quarter',3),('Sixth',5),('Eighth',7)]
 assert [(v['name'],v['value']) for v in ir['enum/BiasSource']['variants']]==[('External',0),('InternalMedium',1),('InternalLow',2),('InternalLowest',4),('InternalHighest',7)]
 for n in p['facts']['ram_registers']:
  fs=ir[f'fieldset/RAM{n}']['fields'];assert {(f['bit_offset'],f['bit_size']) for f in fs}=={(b,1) for b in range(32)}
 # Every exact package and generic intersection gets precisely its bonded roles.
 count=0
 for file in (ROOT/'cw32-data/data/chips').glob(fam+'*.json'):
  chip=json.loads(file.read_text());core=chip['cores'][0];lcd=next(x for x in core['peripherals'] if x['name']=='LCD');pins={x['name'] for x in core['pins']}
  assert lcd['lcd']==p['facts']
  assert lcd['pins']==[{k:r[k] for k in ('pin','signal')} for r in p['routes'] if r['pin'] in pins]
  count+=1
 print(f'PASS {fam}: {len(p["routes"])} own-datasheet routes, {len(ram)} exact RAM words, {count} package/alias projections, typed selectors and source locks')
# L052 vendor SVD/SDK exported undocumented holes. Review requires absent accessors.
manifest=load('cw32-data/inputs/cw32l052.yaml')
assert [(r['register'],r['expected_byte_offset']) for r in manifest['register_removals']]==[(f'RAM{n}',64+4*n) for n in range(9,13)]
pac=(ROOT/'cw32-metapac/src/peripherals/lcd_cw32l052_v1.rs').read_text()
assert not any(f'fn ram{n}(' in pac for n in range(9,13))
assert all(f'fn ram{n}(' in (ROOT/'cw32-metapac/src/peripherals/lcd_cw32l083_v1.rs').read_text() for n in range(14))
print('PASS L052-only RAM9–12 interface removal; L083 RAM0–13 preserved. No hardware validation claimed.')
