#!/usr/bin/env python3
"""Deterministic safety/exclusion contracts for source-qualified classic PWM."""
from verify_classic_pwm_routes import FAMILIES, ROOT, qualified_routes, read

EXPECTED = {
    'F002': (13, 3, 2), 'F003': (16, 0, 2), 'L031': (20, 0, 0),
    'R031': (13, 7, 0), 'W031': (16, 4, 0), 'L052': (48, 0, 8), 'L083': (89, 0, 8),
}
for family in FAMILIES:
    profile = 'CW32'+family
    routes = qualified_routes(profile)
    sidecar = read(ROOT / f'cw32-data/af/{profile.lower()}-pwm.yaml')
    count, sdk_only, safety = EXPECTED[family]
    assert (len(routes), len(sidecar['excluded_sdk_routes']), len(sidecar['excluded_safety_routes'])) == (count, sdk_only, safety)
    assert all(r['signal'] in ('CH1', 'CH2', 'CH3', 'CH4') and 1 <= r['af'] <= 7 for r in routes)
    assert all(not r['oscillator_aliases'] for r in routes)
    forbidden = {'PC14', 'PC15', 'PF0', 'PF1'}
    if family in ('F002', 'F003'): forbidden |= {'PA2', 'PA5', 'PC5'}
    else: forbidden |= {'PA13', 'PA14', 'PF3'}
    if family == 'R031': forbidden |= {'PA0', 'PA1', 'PA2', 'PA3', 'PB8', 'PB9'}
    if family == 'W031': forbidden |= {'PA15', 'PB3', 'PB4', 'PB5', 'PB6', 'PB13'}
    assert not {r['pin'] for r in routes} & forbidden
    candidate = read(ROOT / f'cw32-data/af/{profile.lower()}.yaml')
    assert candidate['status'] == 'candidate-sdk-and-register-verified'
    manifest = read(ROOT / f'cw32-data/inputs/{profile.lower()}.yaml')
    assert manifest['af_metadata'].endswith('-serial.yaml')
    assert manifest['pwm_metadata'].endswith('-pwm.yaml')

f003 = qualified_routes('CW32F003')
pb7 = next(r for r in f003 if r['pin'] == 'PB7')
assert pb7['package_pins'] == {'CW32F003F4U7': None, 'CW32F003F4P7': None, 'CW32F003E4P7': '12'}
l052 = qualified_routes('CW32L052')
l083 = qualified_routes('CW32L083')
assert any((r['pin'], r['af'], r['peripheral'], r['signal']) == ('PA7', 1, 'GTIM2', 'CH1') for r in l052)
assert any((r['pin'], r['af'], r['peripheral'], r['signal']) == ('PA7', 1, 'GTIM4', 'CH1') for r in l083)
assert not any(r['peripheral'] == 'GTIM4' for r in l052)
assert all(r['peripheral'] == 'GTIM' for r in qualified_routes('CW32F002')+f003)
assert all(not r['peripheral'] == 'GTIM' for r in l052+l083)
print('PASS classic PWM safety, source-scope, real-instance and exact-package negative contracts')
