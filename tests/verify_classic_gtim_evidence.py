#!/usr/bin/env python3
"""Verify classic GTIM own-family register/clock evidence, optionally original PDFs."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import yaml
from verify_timer_commands import restore_classic_gtim_modes
ROOT=Path(__file__).resolve().parents[1]
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--sources',type=Path);args=parser.parse_args()
    proof=json.loads((ROOT/'docs/classic-gtim-evidence.json').read_text())
    assert set(proof['families'])=={'CW32F002','CW32F003','CW32L031','CW32R031','CW32W031','CW32L052','CW32L083'}
    for name,family in proof['families'].items():
        source=ROOT/family['register_file'];ir=yaml.safe_load(source.read_text())
        # Preserve the original byte proof across separately qualified enum additions.
        restored=restore_classic_gtim_modes(ir,source.stem)
        historical=(yaml.safe_dump(restored,sort_keys=False,allow_unicode=True,width=100).encode()
                    if restored!=ir else source.read_bytes())
        assert hashlib.sha256(historical).hexdigest()==family['register_sha256']
        items={i['name']:i for i in ir['block/GTIM']['items']}
        expected=dict(ARR=0x300,CNT=0x304,CMMR=0x308,ETR=0x30c,CR0=0x310,IER=0x314,ISR=0x318,ICR=0x31c,CCR1=0x320,CCR2=0x324,CCR3=0x328,CCR4=0x32c,CR1=0x330)
        if family['has_dma']:expected.update(PSC=0x334,DMA=0x340)
        assert {n:i['byte_offset'] for n,i in items.items()}==expected
        assert items['ISR']['access']=='Read'
        fields={f['name']:(f['bit_offset'],f['bit_size']) for f in ir['fieldset/CR0']['fields']}
        assert fields['EN']==(0,1) and fields['ONESHOT']==(5,1)
        if family['has_dma']:
            assert 'PRS' not in fields and ir['fieldset/PSC']['fields'][0]['bit_size']==16
        else:assert fields['PRS']==(7,4)
        for register in ['ARR','CNT','CCR1','CCR2','CCR3','CCR4']:
            assert ir[f'fieldset/{register}']['fields'][0]['bit_size']==16
        flags=ir['fieldset/ICR']['fields'];assert sum(((1<<f['bit_size'])-1)<<f['bit_offset'] for f in flags)==0x27f
        chip=json.loads((ROOT/f'cw32-data/data/chips/{name}.json').read_text())
        actual={p['name']:p for p in chip['cores'][0]['peripherals'] if p['name'].startswith('GTIM')}
        assert {n:p['address'] for n,p in actual.items()}==family['instances']
        assert all(p['registers']['version']==family['register_version'] for p in actual.values())
        sysctrl=next(p for p in chip['cores'][0]['peripherals'] if p['name']=='SYSCTRL')['registers']['version']
        sys=yaml.safe_load((ROOT/f'cw32-data/registers/sysctrl_{sysctrl}.yaml').read_text()); regs={i['name']:i for i in sys['block/SYSCTRL']['items']}
        bits=[]
        for gate in family['clock_gates']:
            assert gate['peripheral'] in actual and gate['kernel_clock']=={'clock':'PCLK'} and gate['reset_asserted_value']==0
            for role in ['enable','reset']:
                entry=gate[role];reg=regs[entry['register']];assert reg['byte_offset']==entry['byte_offset']
                field=next(f for f in sys['fieldset/'+reg['fieldset']]['fields'] if f['name']==entry['field']);assert field['bit_offset']==entry['bit'] and field['bit_size']==1
            bits.append((gate['enable']['register'],gate['enable']['bit']))
        assert len(bits)==len(set(bits))==len(actual), 'No shared GTIM gates may be reset/gated by an individual owner'
        assert family['public_prescaler_subset']==[1<<n for n in range(16)]
        if args.sources:
            # Independently extract actual section text from the original own manual.
            text=subprocess.check_output(['pdftotext','-layout',str(args.sources/family['manual']),'-'],text=True)
            section=family['section']
            def extract(suffix):
                matches=list(re.finditer(r'(?m)^\s*'+re.escape(section+'.'+suffix)+r'\s+',text));assert matches
                return text[matches[-1].start():matches[-1].start()+1700]
            prescaler=extract('3.1.1');force=extract('3.4.2');pwm=extract('3.4.3')
            assert ('PSC+1' in prescaler) if family['has_dma'] else ('32768' in prescaler and 'PRS' in prescaler)
            assert '溢出' in prescaler and re.search(r'EN\s*由\s*0\s*变为\s*1',prescaler)
            assert '立即生效' in prescaler and 'ARR' in prescaler
            assert '0x8' in force and '0x9' in force and '低电平' in force and '高电平' in force
            assert '0xE' in pwm and '0xF' in pwm and '>=' in pwm and '<' in pwm
            icr_matches=list(re.finditer(r'(?m)^\s*'+re.escape(section)+r'\.\d+\.\d+\s+GTIMx?_ICR',text));assert icr_matches
            icr=text[icr_matches[-1].start():icr_matches[-1].start()+3300]
            # The 031 own manuals anchor the register block0x300 later than
            # the normalized PAC's reserved-prefix structure. Check absolute addresses.
            anchor_shift = 0x300 if name in ('CW32L031','CW32R031','CW32W031') else 0
            detail_offset = '0x1C' if anchor_shift else '0x31C'
            assert detail_offset in icr and '0x0000 03FF' in icr and re.search(r'8:7\s+RFU',icr)
            assert re.search(r'GTIMx?_ICR\s+GTIMx?_BASE\s*\+\s*'+detail_offset,text)
            for instance, base in family['instances'].items():
                address=re.search(re.escape(instance)+r'_BASE\s*=\s*0x([0-9A-Fa-f]{4})\s+([0-9A-Fa-f]{4})',text)
                assert address and int(address[1]+address[2],16)==base+anchor_shift
            assert set((int(m[1]),m[2]) for m in re.finditer(r'(?m)^\s*(\d+)\s+(DIRCHANGE|CC[1-4]|UD|TI|OV)\s+R1W0',icr))=={(9,'DIRCHANGE'),(6,'CC4'),(5,'CC3'),(4,'CC2'),(3,'CC1'),(2,'UD'),(1,'TI'),(0,'OV')}
            electrical=family['pad_ac_characteristics']
            data=subprocess.check_output(['pdftotext','-layout',str(args.sources/electrical['datasheet']),'-'],text=True)
            tables=list(re.finditer(r'表\s+'+re.escape(electrical['table'])+r'\s+输入输出交流特性',data));assert tables
            table=data[tables[-1].start():tables[-1].start()+3500]
            assert 'fmax(IO)out' in table and 'tr(IO)out' in table and 'tf(IO)out' in table and '未实测' in table
            for row in electrical['conditions']:
                vdd='2.4V ≤ VDDIOx ＜ 2.7V' if row['minimum_vddio_mv']==2400 else 'VDDIOx ≥ 2.7V'
                pattern=r'CL\s*=\s*'+str(row['load_pf'])+r'pF，'+re.escape(vdd)+r'\s+-\s+'
                assert re.search(pattern+str(row['max_frequency_mhz'])+r'\b',table)
                assert len(re.findall(pattern+str(row['max_rise_fall_ns'])+r'\b',table))==2
        print(f'PASS {name}: own GTIM version, four16-bit channels, PCLK prescaler, independent gates, RO ISR/R1W0 ICR')
    if args.sources:
        for name,source in proof['sources'].items():assert sha(args.sources/name)==source['sha256'],name
        print(f'PASS {len(proof["sources"])} pinned own-manual/datasheet/GTIM/RCC source hashes')
if __name__=='__main__':main()
