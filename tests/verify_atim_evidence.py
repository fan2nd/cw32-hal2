#!/usr/bin/env python3
"""Original-source ATIM register/semantic/provenance validator; no HAL execution."""
import argparse,hashlib,json,re,subprocess
from pathlib import Path
import yaml
ROOT=Path(__file__).resolve().parents[1]
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--sources',required=True,type=Path);a=p.parse_args()
    proof=json.loads((ROOT/'docs/atim-evidence.json').read_text());pins=json.loads((ROOT/'build/provenance/reference-index.json').read_text());identities={s['id']:s for s in pins['sources']}
    for source in proof['classic']['sources']:
        for kind in ('pdf','text'):
            s=source[kind];assert identities[s['source_id']]['sha256']==s['sha256']==sha(a.sources/s['path'])
        text=subprocess.check_output(['pdftotext','-layout',str(a.sources/source['pdf']['path']),'-'],text=True)
        pages=text.split('\f')
        for name,cite in source['citations'].items():
            page=re.sub(r'\s+','',pages[cite['pdf_page_index']]);assert all(m in page for m in cite['whitespace_stripped_markers']),(source['families'],name)
        version=source['register_variant'];ir=yaml.safe_load((ROOT/f'cw32-data/registers/atim_{version}.yaml').read_text());items={i['name']:i for i in ir['block/ATIM']['items']}
        expected={'ARR':0,'CNT':4,'CR':12,'ISR':16,'ICR':20,'MSCR':24,'FLTR':28,'TRIG':32,'CH1CR':36,'CH2CR':40,'CH3CR':44,'DTR':48,'RCR':52,'CH1CCRA':60,'CH1CCRB':64,'CH2CCRA':68,'CH2CCRB':72,'CH3CCRA':76,'CH3CCRB':80,'CH4CCR':84,'CH4CR':88}
        assert {n:i['byte_offset'] for n,i in items.items()}==expected and items['ISR']['access']=='Read'
        for name,offset in expected.items():assert re.search(r'ATIM_'+name+r'\s+ATIM_BASE\s*\+\s*0x'+f'{offset:02X}'+r'\b',text),(source['families'],name)
        fields={f['name']:(f['bit_offset'],f['bit_size']) for f in ir['fieldset/CR']['fields']}
        for f,value in {'EN':(0,1),'COMP':(1,1),'CT':(2,1),'PWM2S':(3,1),'PRS':(4,3),'BUFPEN':(7,1),'MODE':(12,2),'URS':(17,1),'UG':(25,1),'DIR':(27,1)}.items():assert fields[f]==value
        for name in ('ARR','CNT','CH1CCRA','CH2CCRA','CH3CCRA'):assert ir['fieldset/'+name]['fields'][0]['bit_size']==16
        print('PASS '+','.join(source['families'])+': own register map, bounded semantics, source pages')
    for family,row in proof['buffered'].items():
        for s in row['sources'].values():assert identities[s['source_id']]['sha256']==s['sha256']==sha(a.sources/s['file'])
        text=subprocess.check_output(['pdftotext','-layout',str(a.sources/row['sources']['reference_manual']['file']),'-'],text=True)
        parts={}
        heads=list(re.finditer(r'(?m)^\s*(\d+(?:\.\d+)+)\s+ATIM_([A-Z0-9]+)\s',text));heads={m[2]:m for m in heads}
        for reg,cite in row['sections'].items():
            m=heads[reg];end=min([n.start() for n in heads.values() if n.start()>m.start()]+[len(text)])
            assert m[1]==cite['section'] and text[:m.start()].count('\f')==cite['pdf_page_index'];parts[reg]=text[m.start():end]
        ir=yaml.safe_load((ROOT/f'cw32-data/registers/atim_{row["register_variant"]}.yaml').read_text());items={i['name']:i for i in ir['block/ATIM']['items']}
        for reg,item in items.items():assert re.search(r'ATIM_'+reg+r'\s+ATIM_BASE\s*\+\s*0x'+f'{item["byte_offset"]:02X}'+r'\b',text),(family,reg)
        assert items['ISR']['access']=='Read'
        for reg in ['ARR','PSC','CCR1','CCR2','CCR3','CCR4']:assert ir['fieldset/'+reg]['fields'][0]['bit_size']==16
        expected={'CR1':{'CEN':(0,1),'URS':(2,1),'ARPE':(7,1)},'CR2':{'CCPC':(0,1)},'BDTR':{'DTG':(0,8),'LOCK':(8,2),'BKE':(12,1),'AOE':(14,1),'MOE':(15,1),'BK2E':(24,1)},'EGR':{'UG':(0,1)},'CCMR1CMP':{'CC1S':(0,2),'OC1PE':(3,1),'OC1M':(4,3),'OC1MH':(16,1),'OC2PE':(11,1),'OC2M':(12,3)},'CCER':{f'CC{i}{suffix}':((i-1)*4+bit,1) for i in range(1,7) for suffix,bit in [('E',0),('P',1),('NE',2),('NP',3)]}}
        for reg,fields in expected.items():
            actual={f['name']:(f['bit_offset'],f['bit_size']) for f in ir['fieldset/'+reg]['fields']}
            for name,(bit,width) in fields.items():
                assert actual[name]==(bit,width),(family,reg,name)
                span=str(bit) if width==1 else f'{bit+width-1}:{bit}'
                assert re.search(r'(?m)^\s*'+span+r'\s+'+name+r'\s+',parts[reg]),(family,reg,name,'own field table')
        assert re.search(r'PSC\[15:0\]\s*\+\s*1',parts['PSC']) and 'UG' in parts['PSC']
        assert '空时，计数器不工作' in re.sub(r'\s+','',parts['ARR'])
        assert all(code in parts['CCMR1CMP'] for code in ('0100','0101','0110','0111'))
        assert '冻结' in re.sub(r'\s+','',parts['CCMR1CMP']) and 'PWM' in parts['CCMR1CMP']
        flags={f['name']:f['bit_offset'] for f in ir['fieldset/ICR']['fields']}
        actual={m[2]:int(m[1]) for m in re.finditer(r'(?m)^\s*(\d+)\s+([A-Z0-9]+)\s+R1W0',parts['ICR'])};assert actual==flags and sum(1<<b for b in flags.values())==0xff3fff
        assert re.search(r'ATIM_BASE\s*=\s*0x4000\s*1400',text)
        for reg,offset in [('APBEN1','38'),('APBRST1','48')]:
            headings=list(re.finditer(r'(?m)^\s*\d+\.\d+\.\d+\s+SYSCTRL_'+reg+r'\b',text));assert headings
            start=headings[-1].start();block=text[start:];next_heading=re.search(r'(?m)^\s*\d+\.\d+\.\d+\s+SYSCTRL_',block[10:]);block=block[:10+next_heading.start()] if next_heading else block
            assert re.search(r'Address offset:\s*0x'+offset,block) and re.search(r'(?m)^\s*5\s+ATIM\s+RW',block),(family,reg)
            if reg=='APBEN1':assert re.search(r'0x5[Aa]5[Aa]',block)

        print('PASS '+family+': own ATIM preload, flags, MOE/break, six-channel register map; four main outputs only')
if __name__=='__main__':main()
