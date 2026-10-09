#!/usr/bin/env python3
"""Reviewed DMA sidecars: source, topology, request and generated-reference checks.

These validate facts against the generated register/peripheral/IRQ inventory.
They require the generated Chip routing to match the exact reviewed upstream
projection as well as resolving the physical topology and source evidence.
"""
from copy import deepcopy
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import unittest
import yaml

ROOT = Path(__file__).resolve().parents[1]
PROFILES = ROOT / 'cw32-data/dma'
GENERATED = ROOT / 'cw32-data/data'
COUNTS = {'CW32A030': (5, 43), 'CW32F002': (0, 0), 'CW32F003': (0, 0),
          'CW32F020': (2, 41), 'CW32F030': (5, 43), 'CW32L010': (0, 0),
          'CW32L011': (0, 0), 'CW32L012': (4, 64), 'CW32L031': (4, 30),
          'CW32L052': (4, 39), 'CW32L083': (5, 51), 'CW32R031': (4, 30), 'CW32W031': (4, 30)}


def load(path):
    return yaml.safe_load(path.read_text()) if path.suffix == '.yaml' else json.loads(path.read_text())


class DmaContractError(ValueError):
    pass


def require(condition, message):
    if not condition:
        raise DmaContractError(message)


def validate_dma_profile(profile, core):
    family = profile['family']
    count, request_count = COUNTS[family]
    require(profile['schema_version'] == 1, 'Unknown DMA schema')
    require(profile['hardware_validated'] is False, 'No hardware validation has been performed')
    require(profile['presence'] == ('present' if count else 'absent'), 'Incorrect presence claim')
    require(all(profile['coverage'][x] == 'complete' for x in ('channels', 'interrupts', 'requests')), 'Unexpected coverage')
    require(len(profile['core_dma_channels']) == count, 'Wrong physical DMA channel count')
    require(len(profile['requests']) == request_count, 'Wrong hardware request count')
    require(len(profile['channel_details']) == count, 'Missing channel evidence')
    per = {p['name']: p for p in core['peripherals']}
    irq = {x['name']: x['number'] for x in core['interrupts']}
    require(len(irq) == len(core['interrupts']), 'Duplicate IRQ name')
    source_ids = set(profile['sources'])
    for source in profile['sources'].values():
        require(bool(re.fullmatch(r'[0-9a-f]{64}', source['sha256'])), 'Missing source hash')
        require(not Path(source['path']).is_absolute() and '..' not in Path(source['path']).parts,
                'Non-portable source path')
        require(source.get('url', source.get('archive_url', '')).startswith('https://www.whxy.com/'), 'Non-vendor source')
    for evidence in profile['evidence'].values():
        require(evidence['source'] in source_ids, 'Unresolved evidence source')
        require(all(type(p) is int and p >= 0 for p in evidence.get('pdf_page_indices', [])), 'Invalid page reference')
    expected_names = [f'DMA_CH{n}' for n in range(1, count + 1)]
    require([c['name'] for c in profile['core_dma_channels']] == expected_names, 'Wrong channel naming')
    require(set(p for p in per if re.fullmatch(r'DMACHANNEL\d+', p)) == {f'DMACHANNEL{n}' for n in range(1, count + 1)},
            'Generated physical-channel inventory mismatch')
    for index, (channel, detail) in enumerate(zip(profile['core_dma_channels'], profile['channel_details'])):
        require(channel == {'name': expected_names[index], 'dma': 'DMA', 'channel': index}, 'Wrong zero-based index or unsupported core keys')
        physical = index + 1
        require(detail['name'] == channel['name'] and detail['physical_channel'] == physical, 'Wrong physical ordinal')
        require(detail['register_peripheral'] == f'DMACHANNEL{physical}', 'Wrong register instance')
        require(detail['interrupt'] in irq and detail['interrupt_number'] == irq[detail['interrupt']], 'Unresolved or incorrect IRQ')
        cp = per[detail['register_peripheral']]
        require({'signal': 'GLOBAL', 'interrupt': detail['interrupt']} in cp.get('interrupts', []), 'Wrong channel IRQ association')
        regs = cp['registers']
        ir = load(GENERATED / 'registers' / f"{regs['kind']}_{regs['version']}.json")
        require(regs['kind'] == 'dmachannel', 'Wrong register kind')
        block = ir['block/' + regs['block']]
        trig = next((x for x in block['items'] if x['name'] == 'TRIG'), None)
        require(trig is not None, 'Missing request selector register')
        hardsrc = next((x for x in ir['fieldset/' + trig['fieldset']]['fields'] if x['name'] == 'HARDSRC'), None)
        require(hardsrc is not None and hardsrc['bit_offset'] == 2 and hardsrc['bit_size'] == 6, 'Incorrect selector field')
        # Cross-check global TC/TE flags for every physical channel as well.
        dr = per['DMA']['registers']
        dma_ir = load(GENERATED / 'registers' / f"{dr['kind']}_{dr['version']}.json")
        for register in ('ISR', 'ICR'):
            fields = {f['name']: f for f in dma_ir['fieldset/' + register]['fields']}
            for prefix, bit in [('TC', 4 * index), ('TE', 4 * index + 1)]:
                require(prefix + str(physical) in fields, 'Missing global channel interrupt flag')
                require(fields[prefix + str(physical)]['bit_offset'] == bit, 'Incorrect interrupt flag layout')
    expected_projection = {}
    values = []
    for route in profile['requests']:
        require(route['peripheral'] in per, 'Unresolved request peripheral')
        require(route['dma'] == 'DMA' and 'DMA' in per, 'Unresolved DMA controller')
        require(type(route['request']) is int and 0 <= route['request'] < 64, 'Request exceeds HARDSRC field')
        require(bool(re.fullmatch(r'[A-Z][A-Z0-9_]*', route['signal'])), 'Invalid normalized signal')
        require(route['channels'] == expected_names, 'Unverified channel restriction or expansion')
        require(int(route['manual']['binary'], 2) == route['request'], 'Manual request code mismatch')
        require(route['manual']['pdf_page_index'] in profile['evidence']['request_table']['pdf_page_indices'], 'Wrong request evidence page')
        sdk = route['sdk']
        require(sdk['line'] > 0, 'Missing SDK definition line')
        if sdk['encoding'] == 'shifted_left_2_define':
            require(sdk['raw_value'] == route['request'] << 2 and sdk['symbol'].startswith('DMA_HardTrig_'), 'Wrong shifted SDK encoding')
        else:
            require(sdk['encoding'] == 'unshifted_enum' and sdk['raw_value'] == route['request'] and sdk['symbol'].startswith('DMA_TRIGGER_SRC_'), 'Wrong enum SDK encoding')
        require(set(route['evidence']) <= set(profile['evidence']), 'Unresolved request evidence')
        values.append(route['request'])
        expected_projection.setdefault(route['peripheral'], []).append({k: route[k] for k in ('signal', 'dma', 'request')})
    require(len(values) == len(set(values)), 'Duplicate hardware request number')
    require(expected_projection == profile['peripheral_dma_channels'], 'Projection is not the exact reviewed routing')
    for routes in expected_projection.values():
        require(len({r['signal'] for r in routes}) == len(routes), 'Duplicate peripheral signal')
    if count:
        require(profile['request_register'] == {'peripheral_kind': 'dmachannel', 'register': 'TRIG', 'field': 'HARDSRC', 'bit_offset': 2, 'bit_size': 6}, 'Unknown request selector')
    else:
        require(not any('DMA' in name for name in per), 'Unexpected DMA hardware on no-DMA family')
    if family == 'CW32L052':
        require(set(values) == set(range(37)) | {49, 50}, 'L052 copied absent-peripheral SDK requests')
        require({x['request'] for x in profile['excluded_sdk_requests']} == set(range(37, 49)), 'Incomplete L052 exclusion audit')
    elif family == 'CW32F020':
        require(set(values) == set(range(43)) - {17, 18}, 'F020 must not acquire ATIM requests')
    elif family in ('CW32L031', 'CW32R031', 'CW32W031'):
        require(set(values) == set(range(8)) | set(range(10, 31)) | {50}, 'Wrong reduced-family request set')
    else:
        require(set(values) == set(range(request_count)), 'Missing/extra requests')


def validate_dma_projection(profile, core):
    require(core['dma_channels'] == profile['core_dma_channels'], 'Core DMA projection mismatch')
    expected = profile['peripheral_dma_channels']
    for peripheral in core['peripherals']:
        require(peripheral.get('dma_channels', []) == expected.get(peripheral['name'], []), 'Peripheral DMA projection mismatch')


class DmaMetadataTests(unittest.TestCase):
    def profile(self, family):
        return load(PROFILES / (family.lower() + '.yaml'))

    def core(self, family):
        return load(GENERATED / 'chips' / (family + '.json'))['cores'][0]

    def test_exact_profile_coverage(self):
        self.assertEqual({p.stem.upper() for p in PROFILES.glob('cw32*.yaml')}, set(COUNTS))
        self.assertEqual({p.stem.upper() for p in (ROOT / 'cw32-data/inputs').glob('cw32*.yaml')}, set(COUNTS))

    def test_every_generated_chip_reference(self):
        profiles = {n: self.profile(n) for n in COUNTS}
        for path in sorted((GENERATED / 'chips').glob('*.json')):
            chip = load(path)
            with self.subTest(chip=chip['name']):
                validate_dma_profile(profiles[chip['line']], chip['cores'][0])
                validate_dma_projection(profiles[chip['line']], chip['cores'][0])

    def test_request_numbers_do_not_cross_families(self):
        def lookup(family, peripheral, signal):
            return next(x['request'] for x in self.profile(family)['requests'] if x['peripheral'] == peripheral and x['signal'] == signal)
        self.assertEqual(lookup('CW32F030', 'ADC', 'COMPLETE'), 10)
        self.assertEqual(lookup('CW32L052', 'ADC', 'SEQ'), 10)
        self.assertEqual(lookup('CW32L012', 'SPI3', 'RX'), 10)
        self.assertEqual(lookup('CW32L012', 'ADC1', 'SEQ'), 12)
        self.assertEqual(lookup('CW32L012', 'I2C2', 'RX'), 63)
        self.assertEqual(lookup('CW32L012', 'DAC', 'CH1_DHR_UNDERRUN'), 16)
        self.assertEqual(lookup('CW32L012', 'DAC', 'CH2_DHR_UNDERRUN'), 17)
        self.assertEqual(lookup('CW32L031', 'ATIM', 'CH1A2A3A4_UP'), 17)
        self.assertEqual(lookup('CW32L012', 'ATIM', 'CH1'), 50)
        self.assertEqual(lookup('CW32L031', 'ADC', 'SINGLE'), 50)

    def test_irq_sharing(self):
        def irqs(f):
            return [c['interrupt'] for c in self.profile(f)['channel_details']]
        self.assertEqual(irqs('CW32F020'), ['DMACH1', 'DMACH2'])
        self.assertEqual(irqs('CW32L012'), ['DMACH12', 'DMACH12', 'DMACH34', 'DMACH34'])
        self.assertEqual(irqs('CW32F030'), ['DMACH1', 'DMACH23', 'DMACH23', 'DMACH45', 'DMACH45'])
        self.assertEqual(irqs('CW32L052'), ['DMACH1', 'DMACH23', 'DMACH23', 'DMACH4'])

    def test_projection_is_valid_upstream_shape(self):
        for family in COUNTS:
            p, c = self.profile(family), deepcopy(self.core(family))
            c['dma_channels'] = deepcopy(p['core_dma_channels'])
            for peripheral in c['peripherals']:
                if peripheral['name'] in p['peripheral_dma_channels']:
                    peripheral['dma_channels'] = deepcopy(p['peripheral_dma_channels'][peripheral['name']])
            validate_dma_projection(p, c)

    def test_negative_fixtures(self):
        p, c = self.profile('CW32F020'), self.core('CW32F020')
        mutations = [
            lambda p: p['core_dma_channels'][0].update(channel=1),
            lambda p: p['core_dma_channels'].append({'name': 'DMA_CH3', 'dma': 'DMA', 'channel': 2}),
            lambda p: p['channel_details'][1].update(interrupt='DMACH23'),
            lambda p: p['channel_details'][0].update(register_peripheral='DMACHANNEL2'),
            lambda p: p['requests'][1].update(request=4),
            lambda p: p['requests'][0].update(peripheral='ABSENT'),
            lambda p: p['requests'][0].update(channels=['DMA_CH3']),
            lambda p: p['peripheral_dma_channels']['UART1'][0].update(request=63),
            lambda p: p['requests'][0]['sdk'].update(raw_value=4),
            lambda p: p['requests'][0]['manual'].update(binary='111111'),
        ]
        for i, mutation in enumerate(mutations):
            with self.subTest(fixture=i):
                bad = deepcopy(p)
                mutation(bad)
                with self.assertRaises(DmaContractError):
                    validate_dma_profile(bad, c)

    def test_reextract_pinned_vendor_sources_when_available(self):
        source_root = Path(os.environ.get('CW32_SOURCE_ROOT', ROOT.parent / 'cw32-sources'))
        if not (source_root / 'CW32L012_UserManual_CN_V1.4.pdf').is_file():
            self.skipTest('Vendor source cache not present; standalone source re-extraction is not claimed')
        subprocess.run([sys.executable, str(PROFILES / 'build_verified.py'), '--check', '--source-root', str(source_root)], check=True)


if __name__ == '__main__':
    unittest.main()
