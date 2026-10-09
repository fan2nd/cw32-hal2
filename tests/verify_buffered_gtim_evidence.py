#!/usr/bin/env python3
"""Independently re-read current own PDFs/SDKs and check buffered GTIM register policy."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import zipfile
import yaml
ROOT=Path(__file__).resolve().parents[1]
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--sources',type=Path);args=parser.parse_args()
    proof=json.loads((ROOT/'docs/buffered-gtim-evidence.json').read_text())
    assert set(proof['families'])=={'CW32L010','CW32L011','CW32L012'}
    expected=dict(CR1=0,CR2=4,SMCR=8,IER=12,ISR=16,EGR=20,CCMR1CAP=24,CCMR1CMP=24,CCMR2CAP=28,CCMR2CMP=28,CCER=32,CNT=36,PSC=40,ARR=44,CCR1=52,CCR2=56,CCR3=60,CCR4=64,ECR=88,TISEL=92,AF1=96,AF2=100,ICR=112)
    expected_fields={'CR1':{'CEN':(0,1),'UDIS':(1,1),'URS':(2,1),'OPM':(3,1),'DIR':(4,1),'CMS':(5,2),'ARPE':(7,1)},'EGR':{'UG':(0,1)},'CCMR1CMP':{'CC1S':(0,2),'OC1PE':(3,1),'OC1M':(4,3),'OC1MH':(16,1),'CC2S':(8,2),'OC2PE':(11,1),'OC2M':(12,3),'OC2MH':(24,1)},'CCMR2CMP':{'CC3S':(0,2),'OC3PE':(3,1),'OC3M':(4,3),'OC3MH':(16,1),'CC4S':(8,2),'OC4PE':(11,1),'OC4M':(12,3),'OC4MH':(24,1)}}
    for i in range(1,5):expected_fields.setdefault('CCER',{}).update({f'CC{i}E':((i-1)*4,1),f'CC{i}P':((i-1)*4+1,1),f'CC{i}NP':((i-1)*4+3,1)})
    for name,family in proof['families'].items():
        assert sha(ROOT/family['register_file'])==family['register_sha256'];ir=yaml.safe_load((ROOT/family['register_file']).read_text());items={i['name']:i for i in ir['block/GTIM']['items']}
        assert {n:i['byte_offset'] for n,i in items.items()}==expected and items['ISR']['access']=='Read'
        for register, fields in expected_fields.items():
            actual={f['name']:(f['bit_offset'],f['bit_size']) for f in ir[f'fieldset/{register}']['fields']}
            assert all(actual[n]==value for n,value in fields.items()),(name,register)
        for register in ['PSC','ARR','CCR1','CCR2','CCR3','CCR4']:assert ir[f'fieldset/{register}']['fields'][0]['bit_size']==16
        flags={f['name']:f['bit_offset'] for f in ir['fieldset/ICR']['fields']};assert sum(1<<b for b in flags.values())==family['clearable_flags']==0xf01e5f
        irq={f['name'] for f in ir['fieldset/IER']['fields']};assert ('UDE' in irq)==family['dma_enable_fields']==(name=='CW32L012')
        assert family['minimum_period_ticks']==2 and family['maximum_period_ticks']==65536 and family['public_prescaler_subset']==[1<<n for n in range(16)]
        chip=json.loads((ROOT/f'cw32-data/data/chips/{name}.json').read_text());timers={p['name']:p for p in chip['cores'][0]['peripherals'] if p['name'].startswith('GTIM')}
        assert {n:p['address'] for n,p in timers.items()}==family['instances'];assert all(p['registers']['version']==family['register_version'] for p in timers.values())
        sysctrl=next(p for p in chip['cores'][0]['peripherals'] if p['name']=='SYSCTRL')['registers']['version'];sys=yaml.safe_load((ROOT/f'cw32-data/registers/sysctrl_{sysctrl}.yaml').read_text());regs={i['name']:i for i in sys['block/SYSCTRL']['items']}
        for reg,offset in [('APBEN1',56),('APBRST1',72)]:
            assert regs[reg]['byte_offset']==offset;fields={f['name']:(f['bit_offset'],f['bit_size']) for f in sys['fieldset/'+regs[reg]['fieldset']]['fields']}
            for timer,bit in family['clock_bits'].items():assert fields[timer]==(bit,1)
            if reg=='APBEN1':assert fields['KEY']==(16,16)
            else:assert 'KEY' not in fields
        if args.sources:
            text=subprocess.check_output(['pdftotext','-layout',str(args.sources/family['manual']),'-'],text=True);section=family['register_section'];chapter=family['chapter']
            def extract(suffix, next_suffix):
                matches=list(re.finditer(r'(?m)^\s*'+re.escape(section+'.'+str(suffix))+r'\s+GTIMx?_',text));assert matches,(name,suffix)
                start=matches[-1].start();m=re.search(r'(?m)^\s*'+re.escape(section+'.'+str(next_suffix))+r'\s+GTIMx?_',text[start+1:]);assert m
                return text[start:start+1+m.start()]
            for reg,offset in expected.items():
                manual_reg=family['request_register_manual'] if reg=='IER' else reg
                assert re.search(r'GTIMx?_'+manual_reg+r'\s+GTIMx?_BASE\s*\+\s*0x'+f'{offset:02X}'+r'\b',text),(name,reg)
            for timer,base in family['instances'].items():
                label='GTIM' if name=='CW32L010' else timer;m=re.search(label+r'_BASE\s*=\s*0x([0-9A-Fa-f]{4})\s*([0-9A-Fa-f]{4})',text);assert m and int(m[1]+m[2],16)==base
            cr1=extract(1,2);egr=extract(7,8);ccmr=extract(9,10);icr=extract(6,7);psc=extract(14,15);arr=extract(15,16);ccer=extract(12,13)
            for term in ['ARPE','URS','UDIS','CEN','UG','ARR','PSC']:assert term in cr1
            assert '0x00F0 1E5F' in icr
            actual_flags={m[2]:int(m[1]) for m in re.finditer(r'(?m)^\s*(\d+)\s+(UIF|CC[1-4]IF|TIF|CC[1-4]OF|IDXF|DIRF|IERRF|TERRF)\s+R1W0',icr)};assert actual_flags==flags,(name,actual_flags,flags)
            assert re.search(r'PSC\[15:0\]\s*\+\s*1',psc) and 'UG' in psc
            assert '空时，计数器不工作' in re.sub(r'\s+','',arr)
            for code in ['0100','0101','0110']:assert code in ccmr
            compact=re.sub(r'\s+','',ccmr);assert 'OC1PE' in compact and '冻结' in compact and 'PWM' in compact and '更新事件' in compact
            assert 'CC1NP' in ccer and 'CC1P' in ccer and 'CC1E' in ccer
            assert 'UG' in egr and '计数器' in egr and '清零' in egr
            reload_matches=list(re.finditer(r'(?m)^\s*'+re.escape(chapter+'.3.1.4')+r'\s+',text));assert reload_matches
            reload=text[reload_matches[-1].start():reload_matches[-1].start()+5000]
            assert 'ARPE' in reload and '停止状态' in reload and '立即' in reload
            for register,offset in [('APBEN1','38'),('APBRST1','48')]:
                matches=list(re.finditer(r'(?m)^\s*4\.7\.\d+\s+SYSCTRL_'+register,text));assert matches;start=matches[-1].start();block=text[start:start+9000]
                end=re.search(r'(?m)^\s*4\.7\.\d+\s+SYSCTRL_',block[10:]);block=block[:10+end.start()] if end else block
                assert re.search(r'Address offset:\s*0x'+offset,block)
                if register=='APBEN1':assert re.search(r'0x5[Aa]5[Aa]',block)
                for timer,bit in family['clock_bits'].items():
                    label='GTIM' if name=='CW32L010' else timer;assert re.search(r'(?m)^\s*'+str(bit)+r'\s+'+label+r'\s+RW',block),(name,register,timer)
            sdk=family['sdk_sources'];header=(args.sources/sdk['cmsis_header']['file']).read_text(errors='replace');driver_h=(args.sources/sdk['gtim_header']['file']).read_text(errors='replace');driver_c=(args.sources/sdk['gtim_source']['file']).read_text(errors='replace')
            for register,fields in expected_fields.items():
                for field,(bit,width) in fields.items():assert re.search(r'#define\s+GTIMx?_'+register+'_'+field+r'_Pos\s+\('+str(bit)+r'UL\)',header),(name,register,field)
            for macro,value in [('FORCE_LOW',4),('FORCE_HIGH',5),('PWM1',6)]:assert re.search(r'#define\s+GTIM_OC_MODE_'+macro+r'\s+'+str(value)+r'u\b',driver_h)
            for field in ['OC1PE','OC2PE','OC3PE','OC4PE','PSC','ARR']:assert field in driver_c
            # Read matching archive members rather than trusting extracted SDK filenames.
            with zipfile.ZipFile(args.sources/sdk['sdk_archive']['file']) as z:
                for key in ['cmsis_header','gtim_header','gtim_source','rcc_header','rcc_source']:
                    filename=Path(sdk[key]['file']).name;matches=[n for n in z.namelist() if n.endswith('/'+filename)];assert matches
                    assert any(hashlib.sha256(z.read(n)).hexdigest()==sdk[key]['sha256'] for n in matches),(name,key)
        print(f'PASS {name}: own-source buffered GTIM register/flag/clock/preload/PWM policy')
    if args.sources:
        for name,source in proof['sources'].items():assert sha(args.sources/name)==source['sha256'],name
        print(f'PASS {len(proof["sources"])} pinned current own-manual/datasheet/SDK sources')
if __name__=='__main__':main()
