#!/usr/bin/env python3
"""Validate authored RTC alarm projection and generated command seeds; no HAL execution."""
from pathlib import Path
import json
import re
import yaml

ROOT = Path(__file__).resolve().parents[1]
profiles = yaml.safe_load((ROOT / 'cw32-data/rtc-alarms.yaml').read_text())['profiles']
policy = yaml.safe_load((ROOT / 'cw32-data/register-writes.yaml').read_text())
versions = {p['register_version'] for p in profiles.values()}
for version in versions:
    key = 'rtc_' + version
    entries = policy['registers'][key]
    assert len(entries) == 1
    entry = entries[0]
    assert entry['block'] == 'RTC' and entry['register'] == entry['fieldset'] == 'ICR'
    assert entry['reset_value'] == entry['write_noop'] == 127
    assert entry['zero_to_clear_fields'] == ['ALARMA', 'ALARMB', 'AWTIMER', 'TAMP', 'TAMPOV', 'INTERVAL']
    assert json.loads((ROOT / f'cw32-data/data/register-writes/{key}.json').read_text()) == {
        'schema_version': 1, 'registers': {key: entries}}
    pac = (ROOT / f'cw32-metapac/src/peripherals/{key}.rs').read_text()
    compact = re.sub(r'\s+', '', pac)
    assert 'pubconstfnwrite_noop()->Self{Self(127)}' in compact
    assert 'pubconstfnreset_value()->Self{Self(127)}' in compact
    assert 'implDefaultforIcr{' in compact and 'fndefault()->Icr{Icr(0)}' in compact

count = 0
absent = 0
for path in (ROOT / 'cw32-data/data/chips').glob('*.json'):
    chip = json.loads(path.read_text())
    for core in chip['cores']:
        for peripheral in core['peripherals']:
            if peripheral['name'] != 'RTC':
                assert 'rtc_alarms' not in peripheral
                continue
            profile = profiles[chip['line']]
            assert peripheral['rtc_alarms'] == {
                'direct_register_access': profile['alarm_register_access'] == 'direct',
                'async_wait': profile['async_wait'],
                'alarm_a_ignore_bit': profile['alarm_a_ignore_bit'],
                'alarm_b_configuration_supported': profile['alarm_b_configuration_supported']}
            count += 1
        if chip['line'] in ('CW32F002', 'CW32F003'):
            assert not any(p['name'] == 'RTC' for p in core['peripherals'])
            absent += 1
assert count == 47 and absent > 0
hal = (ROOT / 'embassy-cw32/src/rtc/alarm.rs').read_text()
assert 'Icr::write_noop()' in hal and not re.search(r'\bIcr\s*\(', hal)
build = (ROOT / 'embassy-cw32/build.rs').read_text()
assert 'rtc-alarms.json' not in build and '.rtc_alarms' in build
schema = (ROOT / 'cw32-data-serde/src/lib.rs').read_text()
assert re.search(r'#\[serde\(default, skip_serializing_if = "Option::is_none"\)\]\s*pub rtc_alarms: Option<peripheral::RtcAlarms>', schema)
print(f'RTC alarm projection verified: {count} metadata profiles, {absent} absent-RTC selections, {len(versions)} generated ICR command seeds; Default remains zero; no HAL executed.')
