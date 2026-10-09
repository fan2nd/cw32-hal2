#!/usr/bin/env python3
"""Match production SPI/RCC policies to each family's separately pinned sources.

Source bounds remain in canonical RCC/ADC provenance. Verifies the
SPI page-extraction hashes directly from fixed-version original PDFs. Raw page
text is not redistributed; use ./d fetch-evidence to restore the originals.
No device instructions, MMIO, or electrical measurements occur.
"""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import yaml

ROOT = Path(__file__).resolve().parents[1]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    spi = json.loads((ROOT / 'docs/spi-clock-source-policy.json').read_text())['families']
    hsi = json.loads((ROOT / 'docs/adc-clock-source-bounds.json').read_text())['families']
    assert len(spi) == 13 and set(spi) == set(hsi)
    catalog = yaml.safe_load((ROOT / 'cw32-data/spi.yaml').read_text())
    assert set(catalog['profiles']) == set(spi)
    assert catalog['policy'] == 'docs/spi-clock-source-policy.json'
    chips = 0
    instances = 0
    for path in sorted((ROOT / 'cw32-data/data/chips').glob('*.json')):
        chip = json.loads(path.read_text())
        profile = catalog['profiles'][chip['line']]
        policy = spi[chip['line']]
        assert profile['policy_ref'] == '#/families/' + chip['line']
        expected = {'maximum_frequency': policy['maximum_sck_hz'],
                    'minimum_divisor': policy['minimum_divisor']}
        actual = {}
        for peripheral in chip['cores'][0]['peripherals']:
            if peripheral.get('registers', {}).get('kind') == 'spi':
                assert peripheral['spi'] == expected, (chip['name'], peripheral['name'])
                actual[peripheral['name']] = peripheral['spi']
                instances += 1
            else:
                assert 'spi' not in peripheral, (chip['name'], peripheral['name'])
        assert actual == profile['peripherals'], chip['name']
        chips += 1
    assert chips >= 54
    print(f'PASS {chips} chip metadata selections and {instances} SPI instance limits match own-family evidence')
    env = os.environ
    source_root = Path(env.get('CW32_SPI_SOURCE_ROOT', env.get('CW32_SOURCES', str(ROOT.parent/'cw32-sources' if (ROOT.parent/'cw32-sources').is_dir() else ROOT/'sources/evidence'))))
    assert source_root.is_dir(), 'Run ./d fetch-evidence or set CW32_SPI_SOURCE_ROOT'
    for family, policy in spi.items():
        for source in policy['sources'].values():
            originals = source_root.rglob(source['filename'])
            original = next((path for path in originals if digest(path) == source['sha256']), None)
            assert original is not None, (family, source['filename'], 'missing pinned original PDF')
            for page in source['pages']:
                number = str(page['pdf_page_1_based'])
                extracted = subprocess.check_output(['pdftotext', '-f', number, '-l', number, '-layout', str(original), '-'])
                assert hashlib.sha256(extracted).hexdigest() == page['extract_sha256'], (family, page)
        print(f'PASS {family}: pinned SPI datasheet/manual identities and page extractions')
    print('PASS all 26 own-family datasheet/manual PDF identities and exact page extractions matched')


if __name__ == '__main__':
    main()
