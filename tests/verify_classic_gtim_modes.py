#!/usr/bin/env python3
"""Verify own-manual GTIM mode evidence against the canonical source authority.

Reads local-only vendor originals. Emits authored facts, locations and hashes;
never emits or redistributes vendor page text. Writes authored evidence only with --write.
"""
import argparse
import hashlib
import json
import re
import subprocess
import os
import yaml
from pathlib import Path

P = argparse.ArgumentParser(description=__doc__)
P.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[1])
P.add_argument('--source-cache', type=Path, default=Path(os.environ.get('CW32_SOURCES', str(Path(__file__).resolve().parents[2] / 'cw32-sources'))))
P.add_argument('--write', action='store_true', help='Regenerate reviewed authored evidence after source verification.')
args = P.parse_args()
args.output = args.root / "docs/classic-gtim-modes-evidence.json"
lock_path = args.root / 'sources/evidence-sources.json'
lock = json.loads(lock_path.read_text())
sha = lambda b: hashlib.sha256(b).hexdigest()
compact = lambda s: re.sub(r'\s+', '', s)

def require(condition, message):
    if not condition:
        raise ValueError(message)

def extracted(pdf, first=None, last=None):
    command = ['pdftotext']
    if first is not None:
        command += ['-f', str(first), '-l', str(last or first)]
    command += lock['text_extractor']['arguments'] + [str(pdf), '-']
    return subprocess.check_output(command)

# Tuple contents: manual stem, chips explicitly covered, canonical template,
# CR0 section/start page, MODE row page, CMMR section/page, narrative first page.
manuals = [
 ('CW32x030_UserManual_CN_V2.5', ['CW32F030','CW32A030'], 'gtim_v1', '14.8.1',244,245,'14.8.4',247,223),
 ('CW32F020_UserManual_CN_V1.4', ['CW32F020'], 'gtim_v1', '14.8.1',241,242,'14.8.4',244,220),
 ('CW32F002_UserManual_CN_V1.4', ['CW32F002'], 'gtim_cw32f002_v1', '12.7.1',177,178,'12.7.4',180,157),
 ('CW32F003_UserManual_CN_V2.3', ['CW32F003'], 'gtim_cw32f002_v1', '12.7.1',179,180,'12.7.4',182,159),
 ('CW32L031_UserManual_CN_V1.6', ['CW32L031'], 'gtim_cw32l031_v1', '14.7.1',234,235,'14.7.5',238,215),
 ('CW32R031_UserManual_CN_V1.3', ['CW32R031'], 'gtim_cw32l031_v1', '14.7.1',236,237,'14.7.5',240,217),
 ('CW32W031_UserManual_CN_V1.4', ['CW32W031'], 'gtim_cw32l031_v1', '14.7.1',236,237,'14.7.5',240,217),
 ('CW32L052_UserManual_CN_V1.5', ['CW32L052'], 'gtim_cw32l052_v1', '15.8.1',271,272,'15.8.5',275,249),
 ('CW32L083_UserManual_CN_V2.0', ['CW32L083'], 'gtim_cw32l031_v1', '15.8.1',283,284,'15.8.5',287,261),
]
cc = [
 (0,'DISABLED','No capture/compare channel function.',r'0000[：:]无功能'),
 (1,'CAPTURE_RISING','Capture on a rising edge.',r'0001[：:]上升沿捕获'),
 (2,'CAPTURE_FALLING','Capture on a falling edge.',r'0010[：:]下降沿捕获'),
 (3,'CAPTURE_BOTH','Capture on both rising and falling edges.',r'0011[：:]上下沿同时捕获'),
 (8,'FORCE_LOW','Force the channel output low.',r'1000[：:]强制输出低电平'),
 (9,'FORCE_HIGH','Force the channel output high.',r'1001[：:]强制输出高电平'),
 (10,'COMPARE_LOW','Set the channel output low on a compare match.',r'1010[：:]在比较匹配时置0'),
 (11,'COMPARE_HIGH','Set the channel output high on a compare match.',r'1011[：:]在比较匹配时置1'),
 (14,'PWM_HIGH_AT_OR_ABOVE','PWM output is high when CNT >= CCR.',r'1110[：:]PWM正向输出[（(]CNT>=CCR输出高电平[）)]'),
 (15,'PWM_HIGH_BELOW','PWM output is high when CNT < CCR.',r'1111[：:]PWM反向输出[（(]CNT<CCR输出高电平[）)]'),
]
mode = [
 (0,'TIMER','Timer mode: PCLK is the input clock source; the prescaler supplies the counter clock.',r'00[：:]定时器模式，计数时钟源为PCLK'),
 (1,'COUNTER','Counter mode: the TRS-selected ETR or ITR signal is the input clock source; the prescaler supplies the counter clock.',r'01[：:]计数器模式，计数时钟源为TRS信号'),
 (2,'TRIGGER_START','Trigger-start mode: PCLK supplies the prescaler. Software EN=1 or a valid TRS trigger starts counting; a valid trigger sets EN and TI.',r'10[：:]触发启动模式，计数时钟源为PCLK，TRS信号触发计数器启动'),
 (3,'GATED','Gated mode: PCLK supplies the prescaler; counting requires EN=1 and an active ETR gate level.',r'11[：:]门控模式，计数时钟源为PCLK，ETR输入信号作为门控'),
]
report = {
 'schema_version':1,
 'scope':'Own-manual qualification of classic GTIM CR0.MODE and CMMR.CC1M..CC4M. Includes all nine selected CN manuals covering ten families; excludes low-end L010/L011/L012 variants.',
 'source_authority':'sources/evidence-sources.json',
 'redistribution':'Only authored facts, canonical source references, page locations and extraction hashes. No vendor PDFs or derived text are included.',
 'verification_method':{
  'tool':subprocess.check_output(['pdftotext','-v'],stderr=subprocess.STDOUT).decode().splitlines()[0],
  'full_text_arguments':lock['text_extractor']['arguments'],
  'page_text_arguments':['-f','<pdf_page_1_based>','-l','<same>'] + lock['text_extractor']['arguments'],
  'page_hash_bytes':'Exact stdout bytes, including trailing form feed, of the single-page pdftotext command. Each extraction is also byte-equal to the corresponding form-feed-delimited page in the pinned full derived text.',
  'pdf_and_text_check':'Every PDF and cached full text is checked against its canonical authority SHA-256 and byte count. Full text is freshly re-extracted and checked against the same pinned text hash.',
  'template_mapping_reference':'cw32-data/register-reuse.yaml',
 },
 'enums':{
  'CcMode':{'width':4,'fields':{'CC1M':[3,0],'CC2M':[7,4],'CC3M':[11,8],'CC4M':[15,12]},'values':[{'value':v,'name':n,'meaning':d} for v,n,d,_ in cc], 'unqualified_values':[4,5,6,7,12,13], 'unqualified_policy':'The selected own-manual CMMR tables do not define these encodings. Leave unnamed; do not infer a toggle mode from SDK constants.'},
  'Mode':{'width':2,'field_bits':[2,1],'values':[{'value':v,'name':n,'meaning':d} for v,n,d,_ in mode]},
 },
 'common_qualifications':[
  'CR0.MODE governs normal operation when CR0.ENCMODE=0; nonzero ENCMODE selects encoder counting instead.',
  'CR0.TRS=0 selects ETR; CR0.TRS=1 selects the internal ITR signal.',
  'For ETR, CR0.POL=0 selects the rising edge in trigger/counter modes and the high level in gated mode; POL=1 selects falling edge/low level.',
  'Trigger-start does not mean trigger-only: software can set EN=1. A valid trigger sets EN and the TI interrupt flag.',
  'Gated mode uses ETR as its gate; TRS selecting ITR does not change the documented ETR gate source.',
  'CC2M, CC3M and CC4M explicitly refer to the same manual CC1M definition; no channel semantics are inferred across families.',
 ],
 'manuals':[],
}
for stem,families,variant,crsec,crpage,modepage,ccsec,ccpage,narrpage in manuals:
    matches = [(i,a) for i,a in enumerate(lock['artifacts']) if a['path']==stem+'.pdf']
    require(len(matches)==1, f'{stem}: non-unique source authority')
    index,a = matches[0]
    pdf = args.source_cache/a['path']
    text_path=args.source_cache/a['text']['path']
    pdf_bytes=pdf.read_bytes(); text_bytes=text_path.read_bytes()
    require(sha(pdf_bytes)==a['sha256'] and len(pdf_bytes)==a['bytes'],f'{stem}: PDF pin mismatch')
    require(sha(text_bytes)==a['text']['sha256'] and len(text_bytes)==a['text']['bytes'],f'{stem}: text pin mismatch')
    require(extracted(pdf)==text_bytes,f'{stem}: fresh full extraction differs from pinned text')
    raw_pages=text_bytes.split(b'\f')
    pages=[p.decode() for p in raw_pages]
    require(re.search(re.escape(crsec)+r'\s+GTIMx?_CR0',pages[crpage-1]),f'{stem}: CR0 section mismatch')
    require(re.search(re.escape(ccsec)+r'\s+GTIMx?_CMMR',pages[ccpage-1]),f'{stem}: CMMR section mismatch')
    cctext=pages[ccpage-1].split('GTIMx_CMMR' if 'GTIMx_CMMR' in pages[ccpage-1] else 'GTIM_CMMR',1)[1]
    cctext=re.split(r'\n\d+\.\d+\.\d+\s+GTIM',cctext)[0]
    ccn=compact(cctext)
    require({int(s,2) for s in re.findall(r'([01]{4})[：:]',ccn)}=={x[0] for x in cc},f'{stem}: CMMR code set mismatch')
    for value,name,description,pattern in cc:
        require(re.search(pattern,ccn), f'{stem}: missing {name} semantics')
    for field,bits in report['enums']['CcMode']['fields'].items():
        require(re.search(rf'{bits[0]}:{bits[1]}{field}RW',ccn), f'{stem}: {field} width/access mismatch')
    require(ccn.count('功能描述详见CC1M')==3,f'{stem}: channel aliases mismatch')
    crn=compact(pages[crpage-1]+pages[modepage-1])
    mn=compact(pages[modepage-1]).replace('2:1MODERW','')
    require('2:1MODERW' in crn,f'{stem}: MODE bit/access mismatch')
    for value,name,description,pattern in mode:
        require(re.search(pattern,mn), f'{stem}: missing {name} semantics')
    require('00：定时器功能由MODE位配置' in crn,f'{stem}: encoder override mismatch')
    require('0：ETR输入信号' in crn and '1：ITR' in crn,f'{stem}: TRS selection mismatch')
    require('触发模式上升沿有效，门控模式高电平有效' in crn and '触发模式下降沿有效，门控模式低电平有效' in crn,f'{stem}: POL mismatch')
    need_psc=variant in ['gtim_cw32l031_v1','gtim_cw32l052_v1']
    require(('计数器模式时，GTIMx_PSC寄存器值必须大于0' in crn)==need_psc,f'{stem}: counter PSC condition mismatch')
    chapter=crsec.split('.')[0]
    narrative_pages=[narrpage,narrpage+2,narrpage+3,narrpage+4]
    for i,page in enumerate(narrative_pages,1):
        require(re.search(rf'{chapter}\.3\.2\.{i}\s',pages[page-1]),f'{stem}: narrative {i} location mismatch')
    counter=compact(pages[narrpage+1])
    require('TRS信号' in counter and 'ITR' in counter and 'ETR' in counter and '上升沿或下降沿进行计数' in counter,f'{stem}: counter source/polarity mismatch')
    require('预分频器' in compact(pages[narrpage-1]),f'{stem}: timer prescaler mismatch')
    trigger=compact(pages[narrpage+2]); gated=compact(pages[narrpage+3])
    require('EN为1或触发信号有效' in trigger and 'EN被硬件置位' in trigger and 'TI被硬件置位' in trigger, f'{stem}: trigger start semantics mismatch')
    require('EN为1且门控信号有效' in gated and '门控信号的来源为ETR输入信号' in gated,f'{stem}: gate semantics mismatch')
    if stem.startswith('CW32x030'):
        require('CW32F030/CW32A030系列' in compact(pages[ccpage-1]),f'{stem}: explicit two-family scope missing')
    relevant_pages=sorted(set([crpage,modepage,ccpage]+narrative_pages))
    locations=[]
    for page in relevant_pages:
        raw=extracted(pdf,page)
        require(raw==raw_pages[page-1]+b'\f',f'{stem} page {page}: per-page extraction mismatch')
        footer=re.findall(r'(\d+)\s*/\s*(\d+)',pages[page-1])
        # Footer is the final fraction-like page count in each inspected page.
        require(footer and int(footer[-1][0])==page-1,f'{stem} page {page}: printed page mismatch')
        roles=[]
        if page==crpage: roles.append('CR0 field table: ENCMODE, POL, prescaler type and/or TRS')
        if page==modepage: roles.append('CR0.MODE values and any counter PSC constraint')
        if page==ccpage: roles.append('CMMR.CC1M..CC4M values and bit ranges')
        if page in narrative_pages: roles.append(f'{chapter}.3.2.{narrative_pages.index(page)+1}: mode narrative')
        locations.append({'pdf_page_1_based':page,'printed_page':int(footer[-1][0]),'page_text_sha256':sha(raw),'page_text_bytes':len(raw),'supports':roles})
    report['manuals'].append({
      'families':families,'canonical_register_variant':variant,
      'source_ref':a['id'], 'source_authority_pointer':f'sources/evidence-sources.json#/artifacts/{index}',
      'pdf_pinned_sha256_verified':True,'full_text_pinned_sha256_verified':True,'fresh_full_extraction_matches_pin':True,
      'cr0_section':crsec,'cmmr_section':ccsec,'mode_narrative_sections':[f'{chapter}.3.2.{i}' for i in range(1,5)],
      'qualified_ccmode_values':[x[0] for x in cc], 'qualified_mode_values':[x[0] for x in mode],
      'psc_must_be_greater_than_zero_in_counter_mode':need_psc,
      'counter_constraint_note':('GTIMx_PSC must be greater than zero in COUNTER mode.' if need_psc else 'This manual uses CR0.PRS; its MODE table does not state the separate-PSC counter-mode constraint.'),
      'pages':locations,
    })
# Exact additions are recorded only after the own-manual values qualify. The
# command-metadata audit uses them to reconstruct its historical byte proofs.
report['register_additions'] = {}
for variant in sorted(set(m['canonical_register_variant'] for m in report['manuals'])):
    ir = yaml.safe_load((args.root / f'cw32-data/registers/{variant}.yaml').read_text())
    additions = {}
    for name, values in [('CcMode', cc), ('Mode', mode)]:
        enum = ir[f'enum/{name}']
        require(set(enum) == {'description', 'bit_size', 'variants'}, f'{variant}: enum shape')
        require(enum['bit_size'] == report['enums'][name]['width'], f'{variant}: enum width')
        require([(v['value'], v['name']) for v in enum['variants']] == [(v, n) for v, n, _, _ in values], f'{variant}: enum encodings')
        require(all(set(v) == {'name', 'description', 'value'} for v in enum['variants']), f'{variant}: variant shape')
        additions[f'enum/{name}'] = enum
    for field in ir['fieldset/CMMR']['fields']:
        require(field['name'] in report['enums']['CcMode']['fields'] and field['enum'] == 'CcMode', f'{variant}: CMMR field')
    require(next(f for f in ir['fieldset/CR0']['fields'] if f['name'] == 'MODE')['enum'] == 'Mode', f'{variant}: CR0 MODE')
    report['register_additions'][variant] = additions
if args.write:
    args.output.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
else:
    require(json.loads(args.output.read_text()) == report, 'Authored mode evidence differs from freshly verified sources/register additions')
print(json.dumps({'result':'pass','manuals':len(report['manuals']),'families':sum(len(m['families']) for m in report['manuals']),'page_extractions':sum(len(m['pages']) for m in report['manuals']),'output':str(args.output)}))
