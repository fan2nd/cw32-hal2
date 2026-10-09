#!/usr/bin/env python3
"""Audit and extract direct vendor GPIO alternate-function macros.

Reads official SDK files already acquired by the source-verification workflow.
This does not infer package bonding, analog routes, or grouped/shared functions.
Candidate records are not integrated into generated PAC metadata automatically.
"""
import argparse
import hashlib
import json
import re
from pathlib import Path
import yaml
from route_metadata import with_source_refs

ROOT = Path(__file__).resolve().parents[1]
PATTERN = re.compile(r'#define\s+(P([A-Z])(\d+)_AFx_(\w+))\(\)\s*\(CW_GPIO([A-Z])->(AFR[HL])_f\.(?:AFR|PIN)(\d+)\s*=\s*(\d+)\s*\)')


def extract(source, header: Path):
    family = source['family']
    chip = json.loads((ROOT / 'cw32-data/data/chips' / (family + '.json')).read_text())
    peripherals = {p['name']: p for p in chip['cores'][0]['peripherals']}
    irs = {}
    for name, p in peripherals.items():
        if name.startswith('GPIO'):
            r = p['registers']
            irs[name] = json.loads((ROOT / 'cw32-data/data/registers' / f'{r["kind"]}_{r["version"]}.json').read_text())
    routes, unresolved, gpio, seen = [], [], [], set()
    for line_number, line in enumerate(header.read_text(errors='strict').splitlines(), 1):
        if not re.match(r'#define\s+P[A-Z]\d+_AF', line):
            continue
        match = PATTERN.fullmatch(line.strip())
        if not match:
            raise ValueError(f'{family}:{line_number}: unsupported AF macro syntax')
        macro, port, number, function, actual_port, register, field, af = match.groups()
        number, field, af = int(number), int(field), int(af)
        assert macro not in seen, (family, macro, 'duplicate macro')
        seen.add(macro)
        assert port == actual_port and number == field
        assert register == ('AFRL' if number < 8 else 'AFRH')
        peripheral = peripherals['GPIO' + port]
        regs = irs['GPIO' + port]
        block = regs['block/' + peripheral['registers']['block']]
        item = next(i for i in block['items'] if i['name'] == register)
        fields = regs['fieldset/' + item['fieldset']]['fields']
        field_def = next(f for f in fields if f['bit_offset'] == (number % 8) * 4)
        assert 0 <= af < (1 << field_def['bit_size'])
        record = {'pin': f'P{port}{number}', 'af': af, 'function': function,
                  'source_macro': macro, 'source_line': line_number,
                  'gpio_register': register, 'gpio_field': field_def['name']}
        if function == 'GPIO':
            assert af == 0
            gpio.append(record)
            continue
        candidates = [p for p in peripherals if function.startswith(p) and function != p]
        candidates.sort(key=len, reverse=True)
        if not candidates:
            record['reason'] = 'No exact peripheral-prefix mapping; shared or special function needs manual review'
            unresolved.append(record)
            continue
        name = candidates[0]
        record['peripheral'] = name
        record['signal'] = function[len(name):]
        routes.append(record)
    return {
        'schema_version': 1, 'profile': family,
        'status': 'candidate-sdk-and-register-verified',
        'generated_metadata_merged': False,
        'source': {'sdk_url': source['source_url'], 'sdk_sha256': source['archive_sha256'],
                   'header': 'Libraries/inc/' + header.name,
                   'header_sha256': hashlib.sha256(header.read_bytes()).hexdigest()},
        'limitations': ['Function-to-AF values are vendor SDK declarations, not independently validated silicon.',
                       'Physical package bonding and per-pin capability require the package data.',
                       'Analog and grouped/shared functions are not inferred.',
                       'Signals retain vendor suffix spelling; no unverified STM32 signal renaming.'],
        'routes': routes, 'gpio_selection': gpio, 'unresolved': unresolved,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source-manifest', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    total = {'profiles': 0, 'macros': 0, 'routes': 0, 'unresolved': 0}
    for source in json.loads(args.source_manifest.read_text()):
        header = Path(source['header']['path']).with_name(source['family'].lower() + '_gpio.h')
        result = extract(source, header)
        (args.output / (source['family'].lower()+'.yaml')).write_text(yaml.safe_dump(with_source_refs(result), sort_keys=False, allow_unicode=True))
        total['profiles'] += 1
        total['routes'] += len(result['routes'])
        total['unresolved'] += len(result['unresolved'])
        total['macros'] += len(result['routes']) + len(result['unresolved']) + len(result['gpio_selection'])
    print(json.dumps(total, indent=2))

if __name__ == '__main__':
    main()
