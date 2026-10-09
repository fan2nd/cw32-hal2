#!/usr/bin/env python3
"""Rebuild bounded ATIM main-output routes from own PDF grids and SDK macros.

This is a metadata/source validator, not a HAL test harness. --write requires
original sources and replaces only ATIM route sidecars/evidence. Guarded generator pins require separate review.
No raw document extracts or vendor source are bundled.
"""
import argparse, hashlib, json, re, sys, zipfile
from pathlib import Path
import yaml
from route_metadata import hardware_facts, with_source_refs
ROOT=Path(__file__).resolve().parents[1]
FAMILIES=('F030','A030','F003','L031','R031','W031','L052','L083','L010','L011','L012')
BUFFERED=('L010','L011','L012')
MANUAL_PAGES={'F030':[147,148],'A030':[147,148],'L031':[140,141],'R031':[142,143],'W031':[142],'L052':[145,146,147],'L083':[156,157,158,159],'L010':[123],'L011':[122,123],'L012':[154,155]}
PROOF='docs/atim-pwm-route-evidence.json'
def read(path):return hardware_facts(yaml.safe_load(path.read_text()) if path.suffix == '.yaml' else json.loads(path.read_text()))
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def digest(obj):return hashlib.sha256(json.dumps(hardware_facts(obj),sort_keys=True,separators=(',',':'),ensure_ascii=False).encode()).hexdigest()
def emit(path, obj):
    path.parent.mkdir(parents=True, exist_ok=True)
    text = (yaml.safe_dump(with_source_refs(obj), sort_keys=False, allow_unicode=True) if path.suffix == '.yaml'
            else json.dumps(obj, indent=2, ensure_ascii=False) + '\n')
    path.write_text(text)
def pattern(family):return re.compile(r'ATIM_(CH[1-4])' if family in BUFFERED else r'ATIM_(CH[1-3]A)')
def lines(page):return [(l['bbox'],''.join(s['text'] for s in l['spans'])) for b in page.get_text('dict')['blocks'] for l in b.get('lines',[])]
def cells(region,columns,regex,table,page,offset):
    pins=[(f'P{m[1]}{int(m[2])}',(w[1]+w[3])/2) for w in region if (m:=re.fullmatch(r'P([A-F])(\d+)(?:/(?:SWDIO|SWCLK|BOOT)?)?',w[4])) and w[2]<min(columns.values())]
    out={}
    for w in region:
        if not regex.fullmatch(w[4]):continue
        x,y=(w[0]+w[2])/2,(w[1]+w[3])/2
        af=min(columns,key=lambda a:abs(x-columns[a]));pin,py=min(pins,key=lambda p:abs(y-p[1]))
        assert abs(x-columns[af])<2 and abs(y-py)<10,(table,w,pin,af)
        assert (pin,af) not in out
        out[pin,af]=dict(pin=pin,af=af,function=w[4],table=table,pdf_page=page,printed_page=page-offset)
    return out

def pdf_cells(path,family,manual=False,route_pattern=None):
    import fitz
    out={};maxaf=9 if family=='L012' else 7;regex=route_pattern or pattern(family)
    offset=26 if manual and family=='L012' else (3 if not manual and family in ('L011','L012') else 1)
    with fitz.open(path) as doc:
        pages=[n-1 for n in MANUAL_PAGES[family]] if manual and family in MANUAL_PAGES else range(len(doc))
        for index in pages:
            page=doc[index];ls=lines(page);words=page.get_text('words')
            if manual:
                zero=[b for b,t in ls if t=='AF0']
                if not zero:continue
                assert len(zero)==1;top=zero[0][1]
                columns={int(m[1]):(b[0]+b[2])/2 for b,t in ls if abs(b[1]-top)<1 and (m:=re.fullmatch(r'AF([0-9])',t))}
                assert set(columns)==set(range(maxaf+1)),(path,index,columns)
                portions=[(top,780,columns,'own-manual GPIO AF allocation')]
            else:
                heads=sorted((b[1],m[1]) for b,t in ls if (m:=re.search(r'表\s*(5-\d+)\s*通过\s*GPIO[A-F]_AFR[Ly]',t)))
                portions=[]
                for n,(top,table) in enumerate(heads):
                    bottom=heads[n+1][0] if n+1<len(heads) else 780
                    columns={int(m[1]):(b[0]+b[2])/2 for b,t in ls if top<b[1]<bottom and (m:=re.fullmatch(r'功能\s*([1-9])',t))}
                    assert set(columns)==set(range(1,maxaf+1)),(path,table,columns)
                    portions.append((top,bottom,columns,table))
            for top,bottom,columns,table in portions:
                part=cells([w for w in words if top<w[1]<bottom],columns,regex,table,index+1,offset)
                assert not out.keys() & part.keys(),(path,index)
                out.update(part)
    assert out,(family,manual,'no main-output route evidence');return out

def sources(family):
    profile='CW32'+family;candidate=read(ROOT/f'cw32-data/af/{profile.lower()}.yaml')
    pins=f'cw32-data/pinouts/{profile.lower()}.yaml';pinouts=read(ROOT/pins)
    lock=read(ROOT/'sources/evidence-sources.json')
    def source(path):
        for a in lock['artifacts']:
            if a['path']==path:return dict(file=path,sha256=a['sha256'],source_id=a['id'],url=a['url'])
            for m in a.get('members',[]):
                if m['path']==path:return dict(file=path,sha256=m['sha256'],source_id='member:'+path,url=a['url'])
        raise AssertionError(('unregistered source',path))
    ds=source(pinouts['source']['document_filename'])
    if family in ('F030','A030'):
        rm=source('CW32x030_UserManual_CN_V2.5.pdf');header=source('cw32f030/Libraries/inc/cw32f030_gpio.h')
    else:
        old=read(ROOT/f'cw32-data/af/{profile.lower()}-pwm.yaml')['sources']
        rm=source(old['reference_manual']['file']);header=source(old['gpio_header']['file'])
    archive=next(a for a in lock['artifacts'] if a['sha256']==candidate['source']['sdk_sha256'])
    return dict(datasheet=ds,reference_manual=rm,gpio_header=header,sdk_archive=source(archive['path']),pinouts=dict(file=pins,sha256=sha(ROOT/pins)),sdk_candidates=dict(file=f'cw32-data/af/{profile.lower()}.yaml',sha256=sha(ROOT/f'cw32-data/af/{profile.lower()}.yaml')))

def sdk_cells(path,family):
    regex=re.compile(r'^#define\s+(P([A-F])(\d+)_AFx_(ATIM(CH[1-4](?:A)?)))\(\)\s+\(CW_GPIO([A-F])->(AFR[HL])_f\.((?:AFR|PIN)(\d+))\s*=\s*(\d+)\)')
    out={}
    for number,line in enumerate(path.read_text().splitlines(),1):
        m=regex.match(line)
        if not m:continue
        macro,port,n,function,signal,bank,reg,field,bit,af=m.groups()
        if not pattern(family).fullmatch('ATIM_'+signal):continue
        assert port==bank and int(n)==int(bit) and reg==('AFRL' if int(n)<8 else 'AFRH')
        pin,af=f'P{port}{int(n)}',int(af);assert (pin,af) not in out
        out[pin,af]=dict(pin=pin,af=af,function=function,source_macro=macro,source_line=number,gpio_register=reg,gpio_field=field,peripheral='ATIM',signal=signal)
    return out

def exclusion(family,pin,row):
    reserved={'R031':{'PA0','PA1','PA2','PA3'},'W031':{'PB3','PB4','PB5','PB6','PB13'}}
    if pin in reserved.get(family,set()):return 'radio-reserved-pad'
    if row is None:return 'absent-from-own-package-pin-grid'
    if row['pin_type']!='I/O':return 'not-output-capable'
    if set(row['signals']) & {'SWDIO','SWCLK','NRST','BOOT'}:return 'debug-reset-or-boot-pad'
    if any(s.startswith('OSC') for s in row['signals']):return 'oscillator-ownership-not-provided'

def construct(family,src,ds,rm,sdk,normalize_sdk=lambda function: function):
    profile='CW32'+family;pinouts=read(ROOT/src['pinouts']['file'])
    rows={s:r for r in pinouts['table_rows'] for s in r['signals'] if re.fullmatch(r'P[A-F]\d+',s)}
    routes=[];excluded=[]
    for key in sorted(ds.keys()|rm.keys()|sdk.keys()):
        pin,af=key
        if key not in ds or key not in rm or key not in sdk:
            excluded.append(dict(pin=pin,af=af,reason='missing-own-source-corroboration',datasheet=ds.get(key),manual=rm.get(key),sdk=sdk.get(key)));continue
        if ds[key]['function']!=rm[key]['function'] or ds[key]['function'].replace('_','')!=normalize_sdk(sdk[key]['function']):
            excluded.append(dict(pin=pin,af=af,reason='own-source-function-contradiction',datasheet=ds[key],manual=rm[key],sdk=sdk[key]));continue
        row=rows.get(pin);why=exclusion(family,pin,row)
        if why:excluded.append(dict(pin=pin,af=af,reason=why));continue
        route=dict(sdk[key]);route.update(source_signal=route['signal'],source_kind='sdk-and-datasheet',source_sha256=src['gpio_header']['sha256'],datasheet_cell=dict(ds[key],sha256=src['datasheet']['sha256']),manual_cell=dict(rm[key],sha256=src['reference_manual']['sha256']),pin_cell=dict(pin=pin,pdf_page=row['pdf_page_index']+1,positions=row['positions'],pin_type=row['pin_type'],sha256=src['datasheet']['sha256']),package_pins={p['name']:row['positions'][p['table_column']] for p in pinouts['packages']},oscillator_aliases=[])
        routes.append(route)
    version={'F003':'cw32f003_v1','L052':'cw32l052_v1','L010':'cw32l010_v1','L011':'cw32l010_v1','L012':'cw32l012_v1'}.get(family,'v1')
    sidecar=dict(schema_version=1,profile=profile,kind='pwm',status='verified-sdk-and-datasheet',register_version=version,alias_pin_policy='common-package-intersection',sources=src,datasheet=dict(src['datasheet'],af_evidence='Own-family numbered datasheet/manual AF columns and exact SDK assignments; reproducible evidence: docs/atim-pwm-route-evidence.json'),selector_capability=dict(min=1,max=9 if family=='L012' else 7,pdf_printed_page_offset=3 if family in ('L011','L012') else 1,evidence='Own numbered datasheet and reference-manual AF grids, corroborated against exact SDK macro assignments.'),routes=routes,excluded_routes=excluded,scope='ATIM main CH1A–3A on classic or CH1–4 on buffered IP; complementary and advanced modes withheld.')
    counts={p['name']:sum(r['package_pins'][p['name']] is not None for r in routes) for p in pinouts['packages']}
    return sidecar,dict(profile=profile,sources=src,datasheet_cells=list(ds.values()),manual_cells=list(rm.values()),sdk_cells=list(sdk.values()),route_count=len(routes),routes_sha256=digest(routes),package_counts=counts,excluded_routes=excluded)

def main():
    ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('--sources',type=Path);ap.add_argument('--write',action='store_true');a=ap.parse_args();assert not a.write or a.sources
    old=read(ROOT/PROOF) if not a.write else None;proof=dict(schema_version=1,families={})
    for family in FAMILIES:
        profile='CW32'+family;src=sources(family)
        if a.sources:
            for key,s in src.items():assert sha((ROOT if key in ('pinouts','sdk_candidates') else a.sources)/s['file'])==s['sha256'],(family,key)
            ds=pdf_cells(a.sources/src['datasheet']['file'],family);rm=pdf_cells(a.sources/src['reference_manual']['file'],family,True);sdk=sdk_cells(a.sources/src['gpio_header']['file'],family)
            with zipfile.ZipFile(a.sources/src['sdk_archive']['file']) as z:
                name=Path(src['gpio_header']['file']).name
                assert any(hashlib.sha256(z.read(m)).hexdigest()==src['gpio_header']['sha256'] for m in z.namelist() if m.endswith('/'+name))
            sys.path.insert(0,str(ROOT/'cw32-data/tools'));from extract_pinouts import extract_rows
            assert extract_rows(a.sources/src['datasheet']['file'],profile)==read(ROOT/src['pinouts']['file'])['table_rows']
        else:
            saved=old['families'][profile]
            ds,rm,sdk=({(r['pin'],r['af']):r for r in saved[k]} for k in ('datasheet_cells','manual_cells','sdk_cells'))
        candidate=read(ROOT/src['sdk_candidates']['file'])
        candidates={(r['pin'],r['af']):r for r in candidate.get('routes',[])+candidate.get('unresolved',[]) if re.fullmatch(r'ATIMCH[1-4](?:A)?',r['function']) and pattern(family).fullmatch(r['function'].replace('ATIM','ATIM_'))}
        for key,row in sdk.items():
            assert key in candidates
            for k in ('pin','af','function','source_macro','source_line','gpio_register','gpio_field'):assert row[k]==candidates[key][k],(family,key,k)
        sidecar,evidence=construct(family,src,ds,rm,sdk);proof['families'][profile]=evidence
        path=ROOT/f'cw32-data/af/{profile.lower()}-atim-pwm.yaml'
        if a.write:emit(path,sidecar)
        else:assert read(path)==sidecar
        print(f"PASS {profile}: {len(ds)} datasheet/{len(rm)} manual/{len(sdk)} SDK; {len(sidecar['routes'])} routes; {len(sidecar['excluded_routes'])} excluded",flush=True)
    if a.write:emit(ROOT/PROOF,proof)
    else:assert proof==old
if __name__=='__main__':main()
