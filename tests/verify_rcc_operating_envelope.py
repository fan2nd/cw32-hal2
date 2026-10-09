#!/usr/bin/env python3
"""Own-family source/hash/page and runtime RCC operating-envelope verification."""
import hashlib,json,os,re,subprocess
from pathlib import Path
import yaml
ROOT=Path(__file__).resolve().parents[1]
SOURCES=Path(os.environ.get('CW32_SOURCES',str(ROOT.parent/'cw32-sources')))
P=json.loads((ROOT/'docs/rcc-operating-envelope.json').read_text())
ARTIFACTS={a['path']:a for a in json.loads((ROOT/'sources/evidence-sources.json').read_text())['artifacts']}
seen=set()
def verify_sources(x):
 if isinstance(x,dict):
  if 'path' in x and 'sha256' in x:
   name=x['path'];assert name in ARTIFACTS
   assert x['sha256']==ARTIFACTS[name]['sha256'],name
   if name not in seen:
    assert hashlib.sha256((SOURCES/name).read_bytes()).hexdigest()==x['sha256'],name;seen.add(name)
   assert x['printed_pages'] and x['pdf_pages_1_based']
  for v in x.values():verify_sources(v)
 elif isinstance(x,list):
  for v in x:verify_sources(v)
verify_sources(P)
def page(x):
 return '\n'.join(subprocess.check_output(['pdftotext','-f',str(n),'-l',str(n),'-layout',str(SOURCES/x['path']),'-'],text=True) for n in x['pdf_pages_1_based'])
assert len(P['families'])==13
for f in P['families']:
 name=f['family'];high=64_000_000 if name in {'CW32F030','CW32A030','CW32L083'} else 96_000_000 if name in {'CW32L011','CW32L012'} else 48_000_000
 assert f['absolute_rated_bus_max_hz']==high
 text=page(f['bus_source']);flat=re.sub(r'\s+','',text)
 assert 'fHCLK' in flat and 'fPCLK' in flat and str(high//1_000_000) in flat,name
 for symbol in ['fHCLK','fPCLK']:
  start=flat.index(symbol);piece=flat[start:start+220]
  assert str(high//1_000_000) in piece,(name,symbol,piece)
  if f['factory_hsi']['supply_mv'][0]<1800:assert '24' in piece and '1.8' in piece,(name,piece)
 flash=f['flash'];max_wait=1 if name=='CW32L010' else 3 if name in {'CW32L011','CW32L012'} else 2
 assert flash['agreed_wait_values']==list(range(max_wait+1))
 assert flash['wait_clock_ceilings_hz']==[24_000_000*(n+1) for n in range(max_wait+1)]
 assert flash['candidate_initial_wait']==max_wait
 assert flash['initial_wait_supported_ceiling_hz']>=high
 assert flash['initial_wait_covers_all_legal_incoming_hclk']
 text=page(flash['table_source']);flat=re.sub(r'\s+','',text)
 assert 'WAIT' in flat and all(str(n//1_000_000) in flat for n in flash['wait_clock_ceilings_hz']),name
 register=re.sub(r'\s+','',page(flash['register_source']))
 assert 'WAIT' in register,name
 print(f'PASS {name}: own rated bus ceiling {high}, declared-source ranges, agreed WAIT0..{max_wait}, legal-entry initial latency')
catalog=yaml.safe_load((ROOT/'cw32-data/electrical.yaml').read_text())
for f in P['families']:
 values=catalog['profiles'][f['family']]['clock_limits']
 assert values['high_voltage_bus_max_hz']==f['absolute_rated_bus_max_hz']
 assert values['low_voltage_threshold_mv']==1800 and values['low_voltage_bus_max_hz']==24_000_000
 assert values['initial_flash_wait']==f['flash']['candidate_initial_wait']
 chip=json.loads((ROOT/f"cw32-data/data/chips/{f['family']}.json").read_text())
 assert next(p for p in chip['cores'][0]['peripherals'] if p['name']=='SYSCTRL')['clock_limits']==values
print(f'PASS {len(seen)} own source hashes; all 13 voltage/Flash tables and authored/generated metadata; no HAL runtime claim')
