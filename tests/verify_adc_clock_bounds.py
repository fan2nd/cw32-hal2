#!/usr/bin/env python3
"""Verify own-family factory-HSI bounds from hashed official pages and current data projections."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import yaml
ROOT=Path(__file__).resolve().parents[1]
SOURCES=Path(os.environ.get('CW32_SOURCES',str(ROOT.parent/'cw32-sources')))
policy=json.loads((ROOT/'docs/adc-clock-source-bounds.json').read_text())
artifacts={a['path']:a for a in json.loads((ROOT/'sources/evidence-sources.json').read_text())['artifacts']}
expected={'CW32F030','CW32A030','CW32F002','CW32F003','CW32F020','CW32L031','CW32R031','CW32W031','CW32L052','CW32L083','CW32L010','CW32L011','CW32L012'}
assert set(policy['families'])==expected
sources=set()
def citations(value):
 if isinstance(value,dict):
  if 'source_ref' in value:yield value
  for child in value.values():yield from citations(child)
 elif isinstance(value,list):
  for child in value:yield from citations(child)
for citation in citations(policy):
 ref=citation['source_ref'];assert ref.startswith('vendor:');name=ref.removeprefix('vendor:')
 assert name in artifacts,ref
 source=artifacts[name]
 assert source['url'].startswith('https://www.whxy.com/'),source['url']
 if name not in sources:
  assert hashlib.sha256((SOURCES/name).read_bytes()).hexdigest()==source['sha256'],name
  sources.add(name)
 assert citation['printed_pages'] and citation['pdf_pages_1_based'],citation
 assert all(isinstance(n,int) and n>0 for n in citation['pdf_pages_1_based'])
for family,f in policy['families'].items():
 h=f['hsi'];nom=h['nominal_oscillator_hz'];ppm=h['factory_error_bound_ppm']
 assert nom==(96_000_000 if family in {'CW32L011','CW32L012'} else 48_000_000)
 assert ppm==(50_000 if family in {'CW32F002','CW32F020'} else 20_000)
 assert h['actual_oscillator_min_hz']==nom*(1_000_000-ppm)//1_000_000
 assert h['actual_oscillator_max_hz']==nom*(1_000_000+ppm)//1_000_000
 assert h['ambient_temperature_min_c']==-40
 assert h['ambient_temperature_max_c']==(105 if family[4] in 'FA' else 85)
 assert h['ahb_divisors']==[1,2,4,8,16,32,64,128] and h['apb_divisors']==[1,2,4,8]
 expected_divisors=([1,2,4,6,8,10,12,14,16] if f['backend']=='classic' else list(range(1,17)) if family=='CW32L010' else [1,2,3,4,5,6,7,8,9,10,12,16,20,24,28,32])
 assert h['supported_divisors']==expected_divisors
 citation=h['evidence'].get('hsi_table_source',h['evidence'].get('accuracy'))
 page=str(citation['pdf_pages_1_based'][0]); name=citation['source_ref'].removeprefix('vendor:')
 text=subprocess.check_output(['pdftotext','-f',page,'-l',page,'-layout',str(SOURCES/name),'-'],text=True)
 # Isolate HSI so equal LSI tolerance values cannot accidentally qualify HSI.
 assert 'HSIOSC' in text and 'ACCHSI' in text
 text=text.split('HSIOSC',1)[1].split('低速内部',1)[0]
 flat=re.sub(r'\s+','',text)
 pct=ppm//10_000;maximum=h['ambient_temperature_max_c']
 assert f'TA=-40℃~+{maximum}℃-{pct}.0-+{pct}.0%' in flat,(family,flat)
 assert re.search(r'fHSI.*?'+str(nom//1_000_000)+r'.*?MHz',flat),family
 print(f'PASS {family}: own HSI PDF hash/page, {nom} Hz +/-{pct}% over TA[-40,{maximum}]C; {len(expected_divisors)} HSI divisors')
catalog=yaml.safe_load((ROOT/'cw32-data/electrical.yaml').read_text())
for family,f in policy['families'].items():
 values=catalog['profiles'][family]['clock_limits'];h=f['hsi']
 assert values['hsi_error_percent']==h['factory_error_bound_ppm']//10_000
 assert values['hsi_temperature_c']==[h['ambient_temperature_min_c'],h['ambient_temperature_max_c']]
 assert values['hsi_supply_mv']==[h['supply_min_mv'],h['supply_max_mv']]
 chip=json.loads((ROOT/f'cw32-data/data/chips/{family}.json').read_text())
 peripherals=chip['cores'][0]['peripherals']
 assert next(p for p in peripherals if p['name']=='SYSCTRL')['clock_limits']==values
 adc=dict(catalog['profiles'][family]['adc_limits'])
 sequence=yaml.safe_load((ROOT/'cw32-data/adc-sequences.yaml').read_text())['profiles'].get(family)
 if sequence is not None: adc['sequence']=sequence['facts']
 classic=yaml.safe_load((ROOT/'cw32-data/classic-adc-scans.yaml').read_text())['profiles'].get(family)
 if classic is not None:
  adc['sequence']=classic['sequence']
  adc['classic_scan']=classic['facts']
 assert all(p['adc_limits']==adc for p in peripherals if p.get('registers',{}).get('kind')=='adc')
 assert adc['minimum_clock_hz']==(4_000_000 if family in {'CW32L011','CW32L012'} else 0)
print(f'PASS {len(sources)} original source hashes; own HSI bounds and source-qualified authored/generated ADC metadata; no HAL runtime claim')
