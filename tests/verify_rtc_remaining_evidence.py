#!/usr/bin/env python3
"""Source/data/PAC RTC breadth verification; never executes a HAL or MMIO harness."""
from pathlib import Path
import hashlib,json,os,re,subprocess,zipfile
import yaml
ROOT=Path(__file__).resolve().parents[1]
S=Path(os.environ.get('CW32_SOURCES',str(ROOT.parent/'cw32-sources')))
E=json.loads((ROOT/'docs/rtc-remaining-evidence.json').read_text())
F=yaml.safe_load((ROOT/'cw32-data/rtc-calendar.yaml').read_text())['profiles']
def sha(b):return hashlib.sha256(b).hexdigest()
cache={}
def pages(rel):
    if rel not in cache:cache[rel]=subprocess.check_output(['pdftotext','-layout',str(S/rel),'-'],text=True).split('\f')
    return cache[rel]
for rel,item in E['sources'].items():
    assert sha((S/rel).read_bytes())==item['sha256'],rel
    assert item['url'].startswith('https://www.whxy.com/'),rel
    if 'member_chain' in item:
        assert len(item['member_chain'])==1
        assert sha((S/item['archive']).read_bytes())==item['archive_sha256']
        with zipfile.ZipFile(S/item['archive']) as z:assert sha(z.read(item['member_chain'][0]))==item['sha256'],rel
assert len(F)==11 and not {'CW32F002','CW32F003'} & F.keys()
for family,f in F.items():
    e=E['families'][family];assert e['facts']==f
    for name,loc in e['clock_locators'].items():
        assert E['sources'][loc['source']]['sha256']==loc['pdf_sha256']
        assert sha(pages(loc['source'])[loc['pdf_page']-1].encode())==loc['extracted_page_sha256'],(family,name)
    sup=e['clock_locators']['supply'];text=pages(sup['source'])[sup['pdf_page']-1]
    rows=re.findall(r'VDD\s+标准工作电压\s+-\s+([0-9.]+)\s+([0-9.]+)',text)
    ranges=[[round(float(x)*1000) for x in row] for row in rows]
    if family=='CW32W031':
        assert ranges==[[1800,3600],[2000,3600]], family
        assert text.index('射频 LDO 模式') < text.index('射频 DCDC 模式')
        q=e['supply_qualification']
        assert q['source_modes']=={'RF_LDO':{'VDD_mv':[1800,3600]},'RF_DCDC':{'VDD_mv':[2000,3600]}}
        assert q['generic_rtc_supply_mv']==f['supply_mv']==[max(r[0] for r in ranges),min(r[1] for r in ranges)]==[2000,3600]
    else:
        assert ranges==[f['supply_mv']],family
    if f['source']=='LSI':
        loc=e['clock_locators']['accuracy'];text=pages(loc['source'])[loc['pdf_page']-1]
        text=text.split('低速内部（LSI）RC 振荡器')[1].split('超低速内部')[0]
        error=5 if family=='CW32F020' else 3
        assert re.search(r'fLSI\s+频率\s+-\s+-\s+32\.8\s+-\s+kHz',text),family
        lo,hi=f['temperature_c'];assert f'TA=-40℃~+{hi}℃' in re.sub(r'\s+','',text),family
        assert re.search(rf'℃\s+-{error}\s+-\s+\+{error}\s+%',text),family
        assert f['minimum_hz']==32800*(100-error)//100 and f['maximum_hz']==32800*(100+error)//100
        assert f['calendar_divisor']==32768 and f['nominal_hz']==32800
        assert f['prescaler_first']==f['prescaler_second']==0
        loc=e['clock_locators']['factory_trim'];text=re.sub(r'\s+','',pages(loc['source'])[loc['pdf_page']-1]).lower()
        assert f"0x{f['factory_trim_address']:08x}" in text,family
        loc=e['clock_locators']['startup'];text=pages(loc['source'])[loc['pdf_page']-1]
        assert '启动后禁止修改' in text and 'STABLE' in text
        header=(S/e['clock_sdk_sources'][0]).read_text();assert re.search(rf'LSI_TRIMCODEADDR\s+\(0x{f["factory_trim_address"]:08X}U\)',header),family
    else:
        assert f['source']=='HSIOSC' and f['source_encoding']==3
        assert f['nominal_hz']==2*f['prescaler_first']*f['prescaler_second']==f['calendar_divisor']
        assert f['maximum_hz']<=f['prescaler_first']*1_000_000
        assert f['minimum_hz']==f['nominal_hz']*98//100 and f['maximum_hz']==f['nominal_hz']*102//100
    r=json.loads((ROOT/f"cw32-data/data/registers/rtc_{e['register_version']}.json").read_text())
    fields=lambda name:{v['name']:(v['bit_offset'],v['bit_size']) for v in r['fieldset/'+name]['fields']}
    expected={'SOURCE':(8,3)}
    if family in ('CW32L010','CW32L011','CW32L012'):expected['WAIT']=(2,1)
    if family not in ('CW32L011','CW32L012'):expected.update(ACCESS=(0,1),WINDOW=(1,1))
    assert fields('CR1')==expected,family
    date=fields('DATE');assert date['YEAR']==(16,8) and date['WEEK']==(24,3)
    assert date['DAY']==(0,6 if family in ('CW32L010','CW32L011','CW32L012') else 8)
    assert date['MONTH']==(8,5 if family in ('CW32L010','CW32L011','CW32L012') else 8)
    assert fields('TIME')=={'SECOND':(0,7),'MINUTE':(8,7),'HOUR':(16,6)}
    print('PASS',family,'own clock envelope, calibration source and PAC protocol')
count=0
for p in (ROOT/'cw32-data/data/chips').glob('*.json'):
    chip=json.loads(p.read_text())
    for peri in chip['cores'][0]['peripherals']:
        expect=F.get(chip['line']) if peri['name']=='RTC' else None
        assert peri.get('rtc_calendar')==expect,(chip['name'],peri['name'])
        if expect:count+=1
up=Path(os.environ.get('CW32_EMBASSY_UPSTREAM',str(ROOT.parent/'cw32-upstream/embassy')))
if up.exists():
    for p,h in E['upstream']['files_sha256'].items():assert sha((up/p).read_bytes())==h,p
print('PASS',len(E['sources']),'source artifacts and archive member chains;',count,'calendar metadata profiles; no HAL tests')
