#!/usr/bin/env python3
"""Cross-check SDK AF assignments against both x030 datasheets, with narrow errata.

Uses already downloaded official PDF/text sources. Does not guess blank cells,
analog functions or extra shared routes. --write promotes reviewed x030 sidecars.
"""
import argparse
import hashlib
import json
import re
from pathlib import Path
import yaml
ROOT = Path(__file__).resolve().parents[1]


def table(text):
    routes = {}
    port = pin = centers = None
    for line in text.splitlines():
        section = re.search(r'通过 GPIO([ABCF])_AFRy', line)
        if section:
            port, pin, centers = section[1], None, None
            continue
        if port is None:
            continue
        if re.match(r'\s*6\s+地址镜像', line):
            break
        match = re.match(r'\s*(P[ABCF])(\d+)', line)
        if match:
            pin = match[1] + str(int(match[2]))
        functions = list(re.finditer(r'[A-Z][A-Za-z0-9]*_[A-Za-z0-9_]+', line))
        if not functions or pin is None:
            continue
        if centers is None:
            assert len(functions) == 7, 'unexpected table column layout'
            centers = [(f.start()+f.end())/2 for f in functions]
        for function in functions:
            center = (function.start()+function.end())/2
            column = min(range(7), key=lambda i: abs(center-centers[i]))
            assert abs(center-centers[column]) < 5, 'ambiguous table cell'
            key = (pin, column+1)
            assert key not in routes, 'duplicate table coordinate'
            routes[key] = function.group().replace('_','').upper()
    assert len(routes) == 265, 'review required: datasheet AF table changed'
    return routes


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--sources', type=Path, required=True)
    parser.add_argument('--write', action='store_true')
    args = parser.parse_args()
    inputs = yaml.safe_load((ROOT/'cw32-data/parts.yaml').read_text())['sources']
    base = yaml.safe_load((ROOT/'cw32-data/af/cw32f030.yaml').read_text())
    # Preserve the source-only rejected entry when rerunning after promotion.
    candidates = base['routes'] + base['unresolved'] + base.get('excluded_routes', [])
    sdk = {(r['pin'],r['af']): r for r in candidates}
    assert len(sdk) == 266
    for profile in ['CW32F030','CW32A030']:
        evidence = inputs[profile+'_datasheet']
        pdf = args.sources / evidence['document_filename']
        assert hashlib.sha256(pdf.read_bytes()).hexdigest() == evidence['sha256']
        entries = table(pdf.with_suffix('.txt').read_text())
        assert set(sdk)-set(entries) == {('PA11',7)}
        assert sdk[('PA11',7)]['function'] == 'ATIMGATE'
        for key, function in entries.items():
            actual = sdk[key]['function'].upper()
            # The SDK calls PA8 AF4 MCO; both datasheets spell MCO_OUT.
            assert actual == function or (key==('PA8',4) and actual=='MCO' and function=='MCOOUT'), (profile,key,actual,function)
        result = dict(base)
        result.update(profile=profile, status='verified-sdk-and-datasheet', generated_metadata_merged=False)
        result['datasheet'] = evidence
        result['datasheet']['af_evidence'] = 'Tables5-3 through5-6; all265 nonblank pin/AF/function cells independently compared with SDK macros'
        result['routes'] = []
        result['excluded_routes'] = [dict(sdk[('PA11',7)], reason='SDK-only ATIMGATE; PA11 AF7 blank in both current official datasheets. Withheld, not inferred.')]
        for record in base['routes']:
            if (record['pin'],record['af']) == ('PA11',7):
                continue
            r = dict(record)
            r['source_signal'] = r.get('source_signal',r['signal'])
            r['signal'] = {'TXD':'TX','RXD':'RX','CS':'NSS'}.get(r['source_signal'],r['source_signal']).upper()
            result['routes'].append(r)
        result['limitations'] = [
            'Digital AF table coverage only; analog routes and special clock/shared functions remain separate.',
            'Exact package bonding must filter these die-level routes.',
            'No silicon validation; SWD/BOOT/oscillator restrictions still apply.',
        ]
        result['normalization'] = {'TXD':'TX','RXD':'RX','CS':'NSS','basis':'Standard signal spelling; vendor function names and macro evidence retained.'}
        if args.write:
            (ROOT/'cw32-data/af'/f'{profile.lower()}.yaml').write_text(json.dumps(result,indent=2)+'\n')
        print(f'{profile}: 265 datasheet AF cells matched, {len(result["routes"])} peripheral routes, one SDK-only route withheld')

if __name__ == '__main__': main()
