#!/usr/bin/env python3
"""Validate the pinned 031 UART audit against generated register/chip data.

--sources DIR additionally verifies original official-source hashes, extracts
all three manual PDFs, and checks SDK encodings and operational equivalence.
This proves neither electrical behavior nor silicon operation.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import yaml

ROOT = Path(__file__).resolve().parents[1]


def load(path):
    return yaml.safe_load((ROOT / path).read_text()) if (ROOT / path).suffix == '.yaml' else json.loads((ROOT / path).read_text())


def sha(data):
    return hashlib.sha256(data).hexdigest()


def section(text, heading):
    starts = list(re.finditer(r'^' + re.escape(heading) + r'\s', text, re.M))
    assert starts, heading
    tail = text[starts[-1].end():]
    end = re.search(r'^\d+\.\d+(?:\.\d+)*\s', tail, re.M)
    return tail[:end.start()] if end else tail


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--sources', type=Path)
    args = parser.parse_args()
    audit = load('docs/l031-r031-w031-uart-evidence.json')
    assert set(audit['families']) == {'CW32L031', 'CW32R031', 'CW32W031'}
    registers = load('cw32-data/data/registers/uart_cw32l031_v1.json')
    assert {item['name']: item['byte_offset'] for item in registers['block/UART']['items']} == audit['offsets']
    for name, fields in audit['fields'].items():
        assert {item['name']: [item['bit_offset'], item['bit_size']] for item in registers[f'fieldset/{name}']['fields']} == fields, name
    timcnt = next(item for item in registers['block/UART']['items'] if item['name'] == 'TIMCNT')
    assert timcnt['access'] == audit['semantics']['timcnt_access'] == 'Read'
    sysctrl = load('cw32-data/data/registers/sysctrl_cw32l031_v1.json')
    offsets = {item['name']: item['byte_offset'] for item in sysctrl['block/SYSCTRL']['items']}
    assert {name: offsets[name] for name in ['APBEN1', 'APBEN2', 'APBRST1', 'APBRST2']} == {'APBEN1': 0x38, 'APBEN2': 0x34, 'APBRST1': 0x48, 'APBRST2': 0x44}
    checked = 0
    for family, evidence in audit['families'].items():
        clock = load(f'cw32-data/clock/{family.lower()}.yaml')
        for source in evidence['sources'].values():
            known = next((item for item in clock['sources'].values() if item['artifact'] == source['artifact']), None)
            if known is not None:
                assert known['sha256'] == source['sha256'] and known['url'] == source['url']
        for path in sorted((ROOT / 'cw32-data/data/chips').glob(f'{family}*.json')):
            core = json.loads(path.read_text())['cores'][0]
            peripherals = {p['name']: p for p in core['peripherals']}
            irqs = {i['name']: i['number'] for i in core['interrupts']}
            for instance, fields in audit['instances'].items():
                assert peripherals[instance]['address'] == fields['address']
                assert peripherals[instance]['registers'] == {'block': 'UART', 'kind': 'uart', 'version': 'cw32l031_v1'}
                assert peripherals[instance]['interrupts'] == [{'signal': 'GLOBAL', 'interrupt': instance}]
                assert irqs[instance] == fields['interrupt']
                for name in [fields['gate'], fields['reset']]:
                    field = next(f for f in sysctrl[f'fieldset/{name}']['fields'] if f['name'] == instance)
                    assert (field['bit_offset'], field['bit_size']) == (fields['bit'], 1)
            checked += 1
        if not args.sources:
            continue
        material = {}
        for name, source in evidence['sources'].items():
            path = args.sources / source['artifact']
            assert sha(path.read_bytes()) == source['sha256'], str(path)
            if path.suffix == '.pdf':
                material['manual'] = subprocess.run(['pdftotext', '-layout', str(path), '-'], text=True, capture_output=True, check=True).stdout
            else:
                material[name] = path.read_text()
        manual = material['manual']
        count = section(manual, '18.9.6')
        assert re.search(r'23:0\s+TIMCNT\s+RO\b', count) and '0x18' in count
        icr = section(manual, '18.9.11')
        assert re.search(r'Reset value:\s*0x0000\s*0FFF', icr)
        for flag in ['TC', 'RC', 'FE', 'PE', 'CTS', 'TIMOV', 'BAUD', 'RXBRK']:
            assert re.search(r'\b' + flag + r'\s+R1W0\b', icr), (family, flag)
        for heading in ['4.7.15', '4.7.16']:
            reset = re.sub(r'\s+', '', section(manual, heading))
            assert '0：模块处于复位状态' in reset and '1：模块正常工作' in reset
            assert 'KEY' not in reset
        for heading in ['4.7.12', '4.7.13']:
            assert 'KEY' not in section(manual, heading)
        baud = re.sub(r'\s+', '', section(manual, '18.3.3.2'))
        for formula in ['UCLK/(16×BRRI+BRRF)', 'UCLK/(8×BRRI)', 'UCLK/(4×BRRI)']:
            assert formula in baud, (family, formula)
        assert '1~65535' in baud and '0~15' in baud
        assert 'PCLK' in section(manual, '18.9.2')
        assert '数据丢失' in section(manual, '18.3.3.4')
        header = material['uart_h']
        for flag, offset in [('TXE', 0), ('TC', 1), ('RC', 2), ('FE', 3), ('PE', 4), ('CTS', 6), ('TXBUSY', 8), ('TIMOV', 9), ('BAUD', 10), ('RXBRK', 11)]:
            found = re.search(r'#define\s+USART_FLAG_' + flag + r'\s+\(\(uint16_t\)(0x[0-9A-Fa-f]+)\)', header)
            assert found and int(found[1], 16) == 1 << offset, (family, flag)
        for instance, fields in audit['instances'].items():
            assert re.search(r'\b' + instance + r'_IRQn\s*=\s*' + str(fields['interrupt']) + r'\b', material['h'])
            bank = fields['gate'][-1]
            assert re.search(r'#define\s+RCC_APB' + bank + '_PERIPH_' + instance + r'\s+bv' + str(fields['bit']) + r'\b', material['rcc_h'])
        source = re.sub(r'\\\n', '', material['uart_c'])
        source = re.sub(r'CW32[LRWlrw]031|cw32[lrw]031', 'CW32x031', source)
        source = re.sub(r'/\*.*?\*/|//[^\n]*', '', source, flags=re.S)
        source = re.sub(r'\s+', '', source)
        assert sha(source.encode()) == audit['normalized_uart_c_sha256']
        print(f'PASS {family}: official manual/SDK hashes, R1W0 flags, active-low unkeyed reset, baud equations, IRQs and gate bits; normalized UART operations agree')
    assert checked == 11
    assert audit['semantics']['icr_clearable'] == sum(1 << f[0] for f in audit['fields']['ICR'].values())
    assert audit['semantics']['icr_reserved_reset'] == audit['semantics']['icr_reset'] & ~audit['semantics']['icr_clearable']
    print('PASS 031 UART source audit: 11 chip features, three instances each, verified register fields/offsets, gates and interrupts')


if __name__ == '__main__':
    main()
