#!/usr/bin/env python3
"""Verify source-qualified FLASH data/PAC identity; no HAL execution or harness."""
import hashlib
import json
import os
from pathlib import Path
import yaml
ROOT = Path(__file__).resolve().parents[1]
SOURCES = Path(os.environ.get('CW32_SOURCES', str(ROOT.parent / 'cw32-sources')))
UPSTREAM = Path(os.environ.get('CW32_EMBASSY_UPSTREAM', str(ROOT.parent / 'cw32-upstream/embassy')))
if 'CW32_EMBASSY_UPSTREAM' in os.environ:
    assert UPSTREAM.is_dir(), f'Explicit upstream checkout missing: {UPSTREAM}'
if not UPSTREAM.is_dir():
    print('NOTE upstream FLASH API source rehash not run; set CW32_EMBASSY_UPSTREAM to the pinned checkout')
def load(path): return yaml.safe_load(path.read_text()) if path.suffix == '.yaml' else json.loads(path.read_text())
def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()
audit = load(ROOT / 'docs/flash-next-batch-audit.json')
proof = load(ROOT / 'docs/flash-remaining-evidence.json')
limits = load(ROOT / 'cw32-data/electrical.yaml')
reuse = load(ROOT / 'cw32-data/register-reuse.yaml')['groups']
FAMILIES = {x['family'] for x in audit['families']}
L01X = {'CW32L010', 'CW32L011', 'CW32L012'}
assert len(FAMILIES) == 13
verified = set()
for source in proof['sources'].values():
    if source['path_root'] == 'upstream checkout':
        # API-reference checkout is optional in source-only CI, as for RTC.
        # Explicitly requested or present checkouts retain exact pinned hashes.
        assert source['kind'] == 'upstream API reference'
        if not UPSTREAM.is_dir():
            continue
        relative = 'embassy-stm32/' + source['url'].split('/embassy-stm32/', 1)[1]
        path = UPSTREAM / relative
    else:
        assert source['path_root'] == 'CW32_SOURCES'
        path = SOURCES / source['path']
    assert sha(path) == source['sha256'], path
    verified.add(str(path))
    if 'text' in source:
        assert sha(SOURCES / source['text']['path']) == source['text']['sha256']
for family in audit['families']:
    name = family['family']
    version = family['observed_pac_version']
    assert family['main_flash_base'] == 0
    assert family['main_array']['erase_bytes'] == 512
    assert family['main_array']['program_access_bytes'] == [1, 2, 4]
    assert family['main_array']['each_programmed_access_must_be_fully_erased']
    assert not family['main_array']['MultiwriteNorFlash']
    assert not family['execution']['completion_interrupt']
    assert family['electrical']['guaranteed_max_program_or_erase_time'] is None
    for role in ['manual', 'datasheet', 'sdk']:
        key = family['sources'][role]
        if key is None:
            assert name == 'CW32A030' and role == 'sdk'
            continue
        source = audit['sources'][key]
        assert sha(SOURCES / source['path']) == source['sha256'], source['path']
        verified.add(source['path'])
    ir = load(ROOT / 'cw32-data/data/registers' / f'flash_{version}.json')
    items = {v['name']: v for v in ir['block/FLASH']['items']}
    lock_names = ['PAGELOCK1', 'PAGELOCK2', 'PAGELOCK3', 'PAGELOCK4'] if name == 'CW32L083' else ['PAGELOCK1'] if version == 'cw32l031_v1' else ['PAGELOCK']
    offsets = {'CR1': 0, 'CR2': 4, 'IER': 32, 'ISR': 36, 'ICR': 40,
               **{n: 8 + i * 4 for i, n in enumerate(lock_names)}}
    if name in L01X: offsets['SDKCFR'] = 112
    assert {n: v['byte_offset'] for n, v in items.items()} == offsets
    assert items['ISR']['access'] == 'Read'
    if name in L01X: assert items['SDKCFR']['access'] == 'Read'
    def fields(register):
        return {v['name']: (v['bit_offset'], v['bit_size']) for v in ir[f'fieldset/{register}']['fields']}
    l01x = name in L01X
    assert fields('CR1') == {'MODE': (0, 2), 'KEY': (16, 16), **({'SECURITY': (5, 2)} if l01x else {'STANDBY': (4, 1), 'BUSY': (5, 1), 'SECURITY': (6, 2)})}
    errors = {'PC': (0, 1), 'PAGELOCK': (1, 1), 'PROG': (4, 1)}
    if name in {'CW32L011', 'CW32L012'}: errors['SDKERR'] = (2, 1)
    if name == 'CW32L012': errors['CACHEON'] = (3, 1)
    assert fields('ICR') == errors
    assert fields('IER') == errors
    assert fields('ISR') == {**errors, **({'BUSY': (5, 1)} if l01x else {})}
    count = family['protection']['lock_bits_documented']
    for i, register in enumerate(lock_names):
        width = min(16, count)
        assert fields(register) == {**{f'LOCK{x+i*16}': (x, 1) for x in range(width)}, 'KEY': (16, 16)}
    cache = not family['registers']['CR2_CACHE_bit'] is None
    assert fields('CR2') == {'WAIT': (0, 3), **({'FETCH': (3, 1), 'CACHE': (4, 1)} if cache else {}), **({'CACHEINVALID': (5, 1)} if name == 'CW32L012' else {}), 'KEY': (16, 16)}
    mode = next(f for f in ir['fieldset/CR1']['fields'] if f['name'] == 'MODE')
    assert mode['enum'] == 'Mode'
    variants = [('READ', 0), ('PROGRAM', 1), ('PAGE_ERASE', 2)]
    if version not in {'v1', 'cw32f020_v1'}: variants.append(('CHIP_ERASE', 3))
    assert [(v['name'], v['value']) for v in ir['enum/Mode']['variants']] == variants
    group = next(g for g in reuse if g['canonical'] == f'flash_{version}.yaml')
    assert group['canonical_ir_sha256'] == hashlib.sha256(json.dumps(ir, sort_keys=True, separators=(',', ':')).encode()).hexdigest()
    f = limits['profiles'][name]['flash_limits']
    c = limits['profiles'][name]['clock_limits']
    assert f['lock_mask'] == 2**count - 1
    assert f['lock_group_bytes'] == family['protection']['lock_group_bytes']
    assert f['has_cache_control'] == cache
    assert f['maximum_hclk_hz'] == family['electrical']['max_device_HCLK_MHz'] * 1000000
    volts = family['electrical']['program_voltage_range_V'] or family['electrical']['general_VDD_range_V']
    assert f['supply_mv'][0] >= round(volts[0] * 1000)
    assert f['supply_mv'][1] <= round(volts[1] * 1000)
    assert f['maximum_wait_states'] == c['initial_flash_wait']
    assert [(i + 1) * f['wait_step_hz'] // 1000000 for i in range(f['maximum_wait_states'] + 1)] == family['clock']['read_wait_thresholds_mhz']
    assert load(ROOT / 'cw32-data/data/chips' / f'{name}.json')['memory'] == []
exact = audit['exact_ordering_codes']
assert len(exact) == 37
for part in exact:
    chip = load(ROOT / 'cw32-data/data/chips' / f'{part["part"]}.json')
    flash = next(m for m in chip['memory'][0] if m['name'] == 'FLASH')
    assert (flash['address'], flash['size']) == (0, part['flash_bytes'])
    instance = next(p for p in chip['cores'][0]['peripherals'] if p['name'] == 'FLASH')
    assert instance['address'] == 0x40022000
    assert instance['flash_limits'] == limits['profiles'][part['family']]['flash_limits']
    assert flash['size'] // 512 in {32, 40, 64, 128, 256, 512}
for alias in ['CW32F030C8', 'CW32F030K8', 'CW32F030F8', 'CW32F030F6']:
    assert alias not in {p['part'] for p in exact}
    assert load(ROOT / 'cw32-data/data/chips' / f'{alias}.json')['memory']
assert proof['upstream_embassy_commit'] == 'f16efeffe37581092ec184718e6fdb1620393214'
print(f'PASS FLASH evidence: {len(verified)} source fingerprints, 13 own-family maps/electrical policies, 37 exact capacities, typed MODE enums, u64 locks, unknown generic and unqualified aliases')
