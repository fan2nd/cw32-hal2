#!/usr/bin/env python3
"""Verify UART enums/command seeds against pinned own manuals and SDK headers.

This checks authored/generated data and pure PAC generation, never HAL firmware.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import yaml

ROOT = Path(__file__).resolve().parents[1]

def section(text, heading):
    hits = list(re.finditer(r'^'+re.escape(heading)+r'\s', text, re.M))
    assert hits, heading
    tail = text[hits[-1].end():]
    end = re.search(r'^\d+\.\d+(?:\.\d+)*\s', tail, re.M)
    return tail[:end.start()] if end else tail

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--sources', type=Path, required=True)
    args = parser.parse_args()
    facts = json.loads((ROOT/'docs/uart-typed-register-evidence.json').read_text())
    writes = yaml.safe_load((ROOT/'cw32-data/register-writes.yaml').read_text())['registers']
    for family, fact in facts['families'].items():
        path = args.sources / fact['manual']['path']
        assert hashlib.sha256(path.read_bytes()).hexdigest() == fact['manual']['sha256']
        text = subprocess.run(['pdftotext','-layout',str(path),'-'],check=True,capture_output=True,text=True).stdout
        path = args.sources / fact['uart_header']['path']
        assert hashlib.sha256(path.read_bytes()).hexdigest() == fact['uart_header']['sha256']
        cr1 = section(text, fact['sections']['CR1'])
        cr2 = section(text, fact['sections']['CR2'])
        icr = section(text, fact['sections']['ICR'])
        flat = re.sub(r'\s+', '', cr1)
        # These values come from the manual, including SDK STOP contradictions.
        for value, desc in [('00','1位'),('01','1.5位'),('10','2位')]:
            assert value+'：'+desc in flat, (family,'STOP',value)
        for value, desc in [('00','16倍采样'),('01','8倍采样'),('10','4倍采样'),('11','专用采样')]:
            assert value+'：'+desc in flat, (family,'OVER',value)
        low = family in ['CW32L010','CW32L011','CW32L012']
        source = re.sub(r'\s+','',cr1 if low else cr2)
        source_values = [('00','PCLK'),('01','PCLK'),('11','LSI')]
        if family not in ['CW32F002','CW32F003']:
            source_values.append(('10','LSE'))
        for value, desc in source_values:
            assert value+'：UCLK来自'+desc in source,(family,'SOURCE',value)
        for pattern in (['0：偶校验','1：奇校验'] if low else ['00：无奇偶校验','01：自定义校验','10：偶校验','11：奇校验']):
            assert pattern in flat,(family,'PARITY',pattern)
        version = fact['register_version']
        ir = json.loads((ROOT/f'cw32-data/data/registers/{version}.json').read_text())
        enums = {name:{v['name']:v['value'] for v in ir['enum/'+name]['variants']} for name in ['Over','Stop','Source','Parity']}
        assert enums['Over']=={'OVER16':0,'OVER8':1,'OVER4':2,'SPECIAL':3}
        assert enums['Stop']=={'STOP1':0,'STOP1P5':1,'STOP2':2}
        assert enums['Source']==({'PCLK':0,'PCLK_ALT':1,'LSI':3} if family in ['CW32F002','CW32F003'] else {'PCLK':0,'PCLK_ALT':1,'LSE':2,'LSI':3})
        assert enums['Parity']==({'EVEN':0,'ODD':1} if low else {'NONE':0,'CUSTOM':1,'EVEN':2,'ODD':3})
        command, = writes[version]
        assert f"0x{command['reset_value']:08X}" in re.sub(r'\s+','',icr),family
        assert command['write_noop']==command['reset_value']
        fields = {f['name']:f for f in ir['fieldset/ICR']['fields'] if f['name']!='RFU'}
        assert set(fields)==set(command['zero_to_clear_fields'])
        for name in fields:
            assert re.search(r'\b'+name+r'\s+R1W0\b',icr),(family,name)
        mask = sum(1 << f['bit_offset'] for f in fields.values())
        assert command['write_noop'] & mask == mask
        if family in ['CW32L031','CW32R031','CW32W031','CW32L052']:
            assert mask & (1 << 8) == 0 and command['write_noop'] & (1 << 8)
        projected = json.loads((ROOT/f'cw32-data/data/register-writes/{version}.json').read_text())
        assert projected['registers'][version]==[command]
        pac = (ROOT/f'cw32-metapac/src/peripherals/{version}.rs').read_text()
        assert re.search(r'pub const fn write_noop\(\) -> Self\s*\{\s*Self\('+str(command['write_noop'])+r'\)',pac)
        print(f'PASS {family}: own-manual enum encodings, R1W0 fields, reset/no-op and reserved command domain')
    print('PASS UART typed source/data/PAC audit: 13 families, seven selected register versions; no hardware executed')

if __name__ == '__main__':
    main()
