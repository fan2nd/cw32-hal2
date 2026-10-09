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
