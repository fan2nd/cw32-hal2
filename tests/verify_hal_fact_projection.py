#!/usr/bin/env python3
"""Source/data/PAC projection audit; no HAL tests, models, mocks or execution."""
from fractions import Fraction
from pathlib import Path
import hashlib
import json
import os
import re
import yaml

ROOT = Path(__file__).resolve().parents[1]
SOURCES = Path(os.environ.get('CW32_SOURCES', '/workspace/shared/cw32-sources'))
def load(path): return yaml.safe_load((ROOT / path).read_text()) if (ROOT / path).suffix == '.yaml' else json.loads((ROOT / path).read_text())
def digest(path): return hashlib.sha256(path.read_bytes()).hexdigest()

def main():
    accelerators = load('cw32-data/accelerators.yaml')
    math = load('docs/l012-math-evidence.json')
    assert accelerators['domains'] == math['q31_domains']
    source = next(s for s in load('sources/evidence-sources.json')['artifacts']
                  if s['id'] == accelerators['source_ref'])
    assert digest(SOURCES / source['path']) == source['sha256'] == math['manual']['sha256']
    assert digest(SOURCES / source['text']['path']) == source['text']['sha256']
    pages = (SOURCES / source['text']['path']).read_text().split('\f')
    table = re.sub(r'\s+', '', ''.join(pages[p-1] for p in accelerators['pdf_pages_1_based']))
    # The independent decimal table expressions retain each endpoint's openness.
    intervals = {
        'HYPERBOLIC': '[-0.559,0.559]', 'ATANH': '[-0.403,0.403]',
        'LN1': '[0.0535,0.5)', 'LN2': '[0.25,0.75)', 'LN3': '[0.375,0.875)',
        'LN4': '[0.4375,0.584)', 'SQRT0': '[0.027,0.75)',
        'SQRT1': '[0.375,0.875)', 'SQRT2': '[0.4375,0.585]',
    }
    domains = []
    for name, expr in sorted(intervals.items()):
        assert expr in table
        lower, upper = expr[1:-1].split(',')
        row = accelerators['domains'][name]
        assert Fraction(*row['minimum']) == Fraction(lower)
        assert Fraction(*row['maximum']) == Fraction(upper)
        assert row['maximum_inclusive'] == expr.endswith(']')
        lo = Fraction(lower) * (1 << 31)
        hi = Fraction(upper) * (1 << 31)
        minimum = -(-lo.numerator // lo.denominator)
        maximum = hi.numerator // hi.denominator if expr.endswith(']') else -(-hi.numerator // hi.denominator)-1
        assert -(1 << 31) <= minimum <= maximum < (1 << 31)
        domains.append({'name': name, 'minimum': minimum, 'maximum': maximum})
    crypto = load('cw32-data/crypto.yaml')
    assert crypto == load(crypto['evidence'])['facts']
    ram = load('cw32-data/ram-parity.yaml')
    ram_evidence = load(ram['evidence'])
    assert ram['common'] == ram_evidence['common']
    gpio_review = {f['family']: f for f in load('docs/gpio-shared-inventory.json')['families']}
    count = 0
    for path in sorted((ROOT/'cw32-data/data/chips').glob('*.json')):
        chip = json.loads(path.read_text()); family = chip['line']; core = chip['cores'][0]
        pinouts = load('cw32-data/pinouts/'+family.lower()+'.yaml')
        masks = {}
        for package in pinouts['packages']:
            for pin in package['gpio_pins']:
                bank = 'GPIO'+pin[1]; masks[bank] = masks.get(bank,0) | (1 << int(pin[2:]))
        expected_banks = {p['name']: p for p in gpio_review[family]['ports']}
        for p in core['peripherals']:
            if p['name'].startswith('GPIO'):
                row = expected_banks[p['name']]
                assert p['gpio']['output_mask'] == masks.get(p['name'],0) == int(row['family_pin_mask'],16)
                assert p['gpio']['pull_down_mask'] == int(row['pull_down_mask'],16)
                assert p['gpio']['pull_down_mask'] & ~p['gpio']['output_mask'] == 0
                assert set(p['gpio']) == {'output_mask','pull_down_mask'}
                # A separate record holds serviced IRQ/command domains, unchanged here.
                assert set(p['gpio_interrupt']) == {'serviced_mask','clear_noop_mask','level_trigger'}
            else:
                assert 'gpio' not in p
            if p['name'] == 'CORDIC': assert p['cordic'] == {'domains': domains}
            else: assert 'cordic' not in p
            if p['name'] == 'AES':
                assert p['aes'] == {k: crypto['aes'][k] for k in ['block_words','key_words_128','key_words_192','key_words_256']}
            else: assert 'aes' not in p
            if p['name'] == 'TRNG': assert p['trng'] == {'output_words': crypto['trng']['output_words']}
            else: assert 'trng' not in p
            if p['name'] == 'RAM':
                profile = ram['profiles'][family]
                assert profile == ram_evidence['families'][family]['profile']
                assert p['ram_parity'] == {'enable_status': profile['enable_status']}
                assert p['registers']['version'] == profile['register_version'] and p['address'] == profile['base_address']
                assert 'rcc' not in p and 'rcc_control' not in p
            else: assert 'ram_parity' not in p
        count += 1
    assert count == 54
    print(f'PASS: {count} chip selections; GPIO physical/pull domains, nine exact own-table Q1.31 domains, AES/TRNG geometry and RAM diagnostic qualification; no HAL execution')

if __name__ == '__main__': main()
