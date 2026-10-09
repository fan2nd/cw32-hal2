#!/usr/bin/env python3
"""Verify selected UART PAC/metadata against pinned official register evidence.
--sources additionally checks original manual/SDK hashes, R1W0 tables, SDK flags,
parity, instance addresses, IRQ vectors and gate bits. Not a silicon test.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
ROOT=Path(__file__).resolve().parents[1]

def section(text, heading):
    hits=list(re.finditer(r'^'+re.escape(heading)+r'\s',text,re.M))
    assert hits, heading
    tail=text[hits[-1].end():]
    end=re.search(r'^\d+\.\d+(?:\.\d+)*\s',tail,re.M)
    return tail[:end.start()] if end else tail

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--sources',type=Path)
    args=parser.parse_args()
    evidence=json.loads((ROOT/'docs/remaining-uart-evidence.json').read_text())
    feature_count=0
    for family,e in evidence['families'].items():
        low=family in ['CW32L010','CW32L011','CW32L012']
        reg=json.loads((ROOT/f"cw32-data/data/registers/uart_{e['register_version']}.json").read_text())
        items={i['name']:i for i in reg['block/UART']['items']}
        for name,offset in {'CR1':0,'CR2':4,'IER':8,'BRRI':12,'BRRF':16,'ISR':28,'ICR':32,'RDR':36,'TDR':40}.items():
            assert items[name]['byte_offset']==offset,(family,name)
        assert items['ISR']['access']==items['RDR']['access']=='Read'
        assert items['TDR']['access']=='Write'
        field=lambda name:{i['name']:(i['bit_offset'],i['bit_size']) for i in reg['fieldset/'+name]['fields']}
        flags={'TXE':0,'TC':1,'RC':2,'FE':8 if low else 3,'PE':9 if low else 4,'TXBUSY':14 if low else 8}
        if low:flags.update(NE=10,ORE=11,RXIDLE=3,RXBRK=4,BAUD=5,TIMOV=6,CTS=7,RXMATCH=12)
        elif family=='CW32L052':flags.update(TIMOV=9,BAUD=10,RXBRK=11)
        for name,bit in flags.items(): assert field('ISR')[name]==(bit,1),(family,name)
        assert field('CR1')['TXEN']==(0,1) and field('CR1')['RXEN']==(1,1)
        assert field('CR1')['PARITY']==(2,1 if low else 2)
        if low:
            assert field('CR1')['PARITYEN']==(3,1) and field('CR1')['CHLEN']==(6,1)
            assert field('CR1')['SOURCE']==(12,2) and items['CR3']['byte_offset']==0x38
        assert sum(1<<pos for name,(pos,size) in field('ICR').items() if name!='RFU')==e['icr_clearable']
        material={}
        if args.sources:
            for name,source in e['sources'].items():
                path=args.sources/source['artifact']
                assert hashlib.sha256(path.read_bytes()).hexdigest()==source['sha256'],str(path)
                material[name]=subprocess.run(['pdftotext','-layout',str(path),'-'],text=True,capture_output=True,check=True).stdout if path.suffix=='.pdf' else path.read_text()
            for name,bit in flags.items():
                match=re.search(r'#define\s+(?:USART|UART)_FLAG_'+name+r'\s+\(\(uint16_t\)(0x[\da-fA-F]+)\)',material['uart_header'])
                assert match and int(match[1],16)==1<<bit,(family,name)
            if 'manual' in material:
                icr=section(material['manual'],e['icr_section'])
                assert f"0x0000{e['icr_reset']:04X}" in re.sub(r'\s+','',icr)
                for name in field('ICR'):
                    if name!='RFU':assert re.search(r'\b'+name+r'\s+R1W0\b',icr),(family,name)
            if low:
                assert 'PARITYEN' in material['header'] and 'CHLEN' in material['header']
                assert re.search(r'CR1_f\.CHLEN\s*=\s*1',material['uart_driver'])
                assert '5a5a0000' in material['uart_driver'].lower()
        sysversion='cw32f002_v1' if family=='CW32F002' else family.lower()+'_v1'
        sys=json.loads((ROOT/f'cw32-data/data/registers/sysctrl_{sysversion}.json').read_text())
        for path in sorted((ROOT/'cw32-data/data/chips').glob(f'{family}*.json')):
            core=json.loads(path.read_text())['cores'][0]
            irqs={i['name']:i['number'] for i in core['interrupts']}
            uarts=[p for p in core['peripherals'] if p['name'].startswith('UART')]
            count=6 if family=='CW32L083' else 2 if family in ['CW32F002','CW32F003','CW32L010'] else 3
            assert len(uarts)==count
            for p in uarts:
                n=int(p['name'][4:]);name=p['name']
                assert p['registers']['version']==e['register_version']
                irq=f'UART{(n-1)%3+1}_UART{(n-1)%3+4}' if family=='CW32L083' else name
                assert p['interrupts']==[{'signal':'GLOBAL','interrupt':irq}] and irqs[irq]==27+(n-1)%3
                bank,bit=((1,{1:3,2:4,3:8}[n]) if low else {1:(2,9),2:(1,7),3:(1,8),4:(1,9),5:(1,10),6:(2,1)}[n])
                for regname in [f'APBEN{bank}',f'APBRST{bank}']:
                    f=next(i for i in sys['fieldset/'+regname]['fields'] if i['name']==name)
                    assert (f['bit_offset'],f['bit_size'])==(bit,1),(family,regname,name)
                if material:
                    assert re.search(r'\b'+irq+r'_IRQn\s*=\s*'+str(irqs[irq])+r'\b',material['header'])
                    base=re.search(r'#define\s+'+name+r'_BASE\s+(0x[\da-fA-F]+)',material['header'])
                    assert base and int(base[1],16)==p['address']
            feature_count+=1
        print(f'PASS {family}: UART register policy, offsets, keyed/unkeyed gate routes, instance/IRQ metadata'+(('; pinned own manual/SDK checked' if 'manual' in material else '; pinned own SDK checked, reference manual absent') if args.sources else ''))
    print(f'PASS remaining UART source contract: {feature_count} chip features; L011 own-manual evidence included')
if __name__=='__main__':main()
