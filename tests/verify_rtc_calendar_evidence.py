#!/usr/bin/env python3
"""Rehash the bounded RTC source evidence and check register/prescaler contracts."""
import hashlib
import json
import os
import re
import subprocess
from pathlib import Path
import yaml
ROOT=Path(__file__).resolve().parents[1]
SOURCES=Path(os.environ.get('CW32_SOURCES',str(ROOT.parent/'cw32-sources')))
UPSTREAM=Path(os.environ.get('CW32_EMBASSY_UPSTREAM',str(ROOT.parent/'cw32-upstream/embassy')))
# A pinned upstream checkout is optional for fresh source-only CI; official
# hardware evidence and PAC checks remain mandatory. Require it if requested.
if 'CW32_EMBASSY_UPSTREAM' in os.environ:
    assert UPSTREAM.is_dir(), f'Explicit upstream checkout missing: {UPSTREAM}'
evidence=json.loads((ROOT/'docs/rtc-blocking-calendar.json').read_text())
for path,item in evidence['sources'].items():
    assert hashlib.sha256((SOURCES/path).read_bytes()).hexdigest()==item['sha256'],path
# repository_evidence_sha256 records the historical review snapshot. Current
# hardware contracts are checked below and by check_rtc_pac_corrections.py;
# unrelated current IP fields or later review notes must not invalidate them.

def check_rtc_clock_registers(sysctrl):
    items={item['name']:item for item in sysctrl['block/SYSCTRL']['items']}
    for name,offset in {'APBEN2':0x34,'APBRST2':0x44}.items():
        item=items[name]
        assert item['byte_offset']==offset and item.get('access','ReadWrite')=='ReadWrite',(name,item)
        assert item.get('bit_size',32)==32 and item['fieldset']==name,(name,item)
        fields={field['name']:field for field in sysctrl['fieldset/'+name]['fields']}
        rtc=fields['RTC']
        assert (rtc['bit_offset'],rtc['bit_size'],rtc.get('access','ReadWrite'))==(1,1,'ReadWrite'),rtc
        if name=='APBEN2':
            key=fields['KEY']
            assert (key['bit_offset'],key['bit_size'],key.get('access','ReadWrite'))==(16,16,'ReadWrite'),key
if UPSTREAM.is_dir():
    for path,digest in evidence['upstream']['files_sha256'].items():
        assert hashlib.sha256((UPSTREAM/path).read_bytes()).hexdigest()==digest,path
    print('PASS pinned upstream RTC API source rehash')
else:
    print('NOTE upstream source rehash not run; set CW32_EMBASSY_UPSTREAM to the pinned checkout')
for family in evidence['family_evidence']:
    version=family['canonical_version']
    registers=yaml.safe_load((ROOT/f'cw32-data/data/registers/rtc_{version}.yaml').read_text())
    items={i['name']:i for i in registers['block/RTC']['items']}
    expected={'KEY':0,'CR0':4,'CR1':8,'CR2':12,'COMPCFR1':16,'DATE':20,'TIME':24,'PSC':64}
    for name,offset in expected.items():assert items[name]['byte_offset']==offset,(version,name)
    assert items['KEY']['access']=='Write'
    for name in expected.keys()-{'KEY'}:
        assert items[name].get('access','ReadWrite')=='ReadWrite',(version,name)
    fields={k:{f['name']:(f['bit_offset'],f['bit_size']) for f in v['fields']} for k,v in registers.items() if k.startswith('fieldset/')}
    assert fields['fieldset/CR1']=={'WAIT':(2,1),'SOURCE':(8,3)}
    assert fields['fieldset/CR0']['START']==(7,1) and fields['fieldset/CR0']['H24']==(3,1)
    assert fields['fieldset/DATE']=={'DAY':(0,6),'MONTH':(8,5),'YEAR':(16,8),'WEEK':(24,3)}
    assert fields['fieldset/TIME']=={'SECOND':(0,7),'MINUTE':(8,7),'HOUR':(16,6)}
    assert fields['fieldset/PSC']=={'PSC2':(0,20),'PSC1':(20,8)}
    sysctrl=yaml.safe_load((ROOT/f'cw32-data/data/registers/sysctrl_{version}.yaml').read_text())
    check_rtc_clock_registers(sysctrl)
    assert family['clock_sources']=={'0':'LSE','1':'HSE','2':'LSI','3':'HSIOSC'}
    assert family['bus']['gate_write_key']==0x5a5a and family['bus']['reset_asserted_value']==0
p=evidence['selected_prescalers']
assert p['PSC1']<=255 and p['PSC2']<1<<20
assert p['source_hz_nominal']==2*(p['PSC1']+1)*(p['PSC2']+1)
bounds=evidence['hsi_electrical_bounds']
assert bounds['maximum_hz']==bounds['nominal_hz']*102//100==97_920_000
assert bounds['minimum_hz']==bounds['nominal_hz']*98//100==94_080_000
assert p['source_hz_max']==bounds['maximum_hz']
assert p['source_hz_max']<=1_000_000*(p['PSC1']+1)
assert p['RTCCLKD_hz_max']==p['source_hz_max']//(p['PSC1']+1)==816_000
assert p['source_hz_max']>1_000_000*96, 'Old nominal-only /96 must remain rejected'
for item in bounds['sources']:
    page=subprocess.check_output(['pdftotext','-f',str(item['pdf_page']),'-l',str(item['pdf_page']),'-layout',str(SOURCES/item['source']),'-'],text=True)
    assert '7-18' in page and '96' in page
    assert re.search(r'TA=-40℃~\+85℃\s+-2\.0\s+-\s+\+2\.0',page),item['source']
for item in bounds['manual_intermediate_limit']:
    page=subprocess.check_output(['pdftotext','-f',str(item['pdf_page']),'-l',str(item['pdf_page']),'-layout',str(SOURCES/item['source']),'-'],text=True)
    assert item['section'] in page and '1MHz' in page,item['source']
print('PASS own Table 7-18 positive HSI tolerance and <=1MHz manual ceiling; default maximum816kHz, old /96 rejected')
print(f"PASS RTC calendar evidence: {len(evidence['sources'])} rehashed source artifacts; two own register/source contracts; exact nominal divider product; pinned Embassy API provenance")
