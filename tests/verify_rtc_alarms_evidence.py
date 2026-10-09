#!/usr/bin/env python3
"""Validate RTC alarm source/data provenance only; never execute or mock the HAL."""
import argparse
import hashlib
import json
import re
import subprocess
import zipfile
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[1]
p = argparse.ArgumentParser()
p.add_argument('--sources', type=Path, default=Path('/workspace/shared/cw32-sources'))
a = p.parse_args()
data = yaml.safe_load((ROOT / 'cw32-data/rtc-alarms.yaml').read_text())
evidence = json.loads((ROOT / data['evidence']).read_text())
calendar = yaml.safe_load((ROOT / 'cw32-data/rtc-calendar.yaml').read_text())
assert data['profiles'].keys() == evidence['families'].keys() == calendar['profiles'].keys()
page_count = 0
sources = {}
for family, profile in data['profiles'].items():
    record = evidence['families'][family]
    assert profile == record['profile']
    sources.update(record['sources'])
    sdk = record['sdk_set_alarm']
    lines = (a.sources / sdk['source']).read_text(errors='replace').splitlines()
    body = '\n'.join(lines[sdk['start_line'] - 1:sdk['end_line']])
    assert re.search(r'void RTC_SetAlarm\(', body)
    assert re.search(r'RegTmp\s*=\s*RTC_AlarmStruct->RTC_AlarmMask\s*\|', body)
    assert 'CW_RTC->ALARMA = RegTmp;' in body and 'CW_RTC->ALARMB = RegTmp;' in body
    irq = record['sdk_irq']
    line = (a.sources / irq['source']).read_text(errors='replace').splitlines()[irq['line'] - 1]
    assert int(re.search(r'RTC_IRQn\s*=\s*(\d+)', line)[1]) == irq['number']

    assert profile['icr_preserve_seed'] == 0x7f
    assert profile['irq'] == 'RTC'
    assert profile['alarm_a_ignore_bit'] == 1
    assert profile['alarm_b_configuration_supported'] is False
    assert profile['alarm_b_mask_polarity'] == 'unresolved_manual_sdk_conflict'
    fields = yaml.safe_load((ROOT / f"cw32-data/registers/rtc_{profile['register_version']}.yaml").read_text())
    expected = {'SECOND': (0, 7), 'MINUTE': (8, 7), 'HOUR': (16, 6)}
    expected.update(zip(profile['alarm_mask_fields'], [(7, 1), (15, 1), (23, 1), (24, 7)]))
    for alarm in ('ALARMA', 'ALARMB'):
        assert {f['name']: (f['bit_offset'], f['bit_size']) for f in fields['fieldset/' + alarm]['fields']} == expected
    for register in ('IER', 'ISR', 'ICR'):
        assert {f['name']: (f['bit_offset'], f['bit_size']) for f in fields['fieldset/' + register]['fields']} == {
            'ALARMA': (0, 1), 'ALARMB': (1, 1), 'AWTIMER': (2, 1),
            'TAMP': (3, 1), 'TAMPOV': (4, 1), 'INTERVAL': (6, 1)}
    texts = {}
    for kind, ref in record['pages'].items():
        raw = subprocess.check_output(['pdftotext', '-layout', '-f', str(ref['pdf_page']), '-l', str(ref['pdf_page']), str(a.sources / record['manual']), '-'])
        assert hashlib.sha256(raw).hexdigest() == ref['extracted_page_sha256'], (family, kind)
        texts[kind] = re.sub(r'\s+', '', raw.decode())
        page_count += 1
    assert '0x0000007F' in texts['ICR'] and texts['ICR'].count('R1W0') == 6
    assert 'W0：清除闹钟A匹配标志' in texts['ICR'] and 'W1：无功能' in texts['ICR']
    for alarm in ('ALARMA', 'ALARMB'):
        assert 'bit0代表周日' in texts[alarm]
        for part in ('小时', '分钟', '秒钟'):
            compare, ignore = ('0', '1') if alarm == 'ALARMA' else ('1', '0')
            assert compare + '：该闹钟需检查' + part + '位计数值' in texts[alarm]
            assert ignore + '：该闹钟与' + part + '位计数值无关' in texts[alarm]
    assert '0x7F063000' in texts['alarm_behavior']
    assert '0x3E070000' in texts['alarm_behavior']
    direct = '均可直接读写' in texts['access']
    assert direct == profile['async_wait'] == (profile['alarm_register_access'] == 'direct')
    if direct:
        assert 'RTC_DATE、RTC_TIME、RTC_AWTARR' in texts['access']
    else:
        assert '10ms' in texts['access'] and '1000' in texts['access'] and '不可超出1s' in texts['access']

for path, source in sources.items():
    assert hashlib.sha256((a.sources / path).read_bytes()).hexdigest() == source['sha256'], path
    assert source['url'].startswith('https://www.whxy.com/'), path
    if 'archive' in source:
        archive = a.sources / source['archive']
        assert hashlib.sha256(archive.read_bytes()).hexdigest() == source['archive_sha256']
        assert len(source['member_chain']) == 1
        with zipfile.ZipFile(archive) as z:
            assert hashlib.sha256(z.read(source['member_chain'][0])).hexdigest() == source['sha256']


chips = 0
families = set()
for path in (ROOT / 'cw32-data/data/chips').glob('*.json'):
    chip = json.loads(path.read_text())
    for core in chip['cores']:
        rtc = next((p for p in core['peripherals'] if p['name'] == 'RTC'), None)
        if rtc is None:
            assert chip['line'] in ('CW32F002', 'CW32F003')
            continue
        family = chip['line']
        profile = data['profiles'][family]
        families.add(family)
        assert rtc['registers']['version'] == profile['register_version']
        assert rtc['interrupts'] == [{'interrupt': 'RTC', 'signal': 'GLOBAL'}]
        assert next(i['number'] for i in core['interrupts'] if i['name'] == 'RTC') == evidence['families'][family]['sdk_irq']['number']
        if profile['async_wait']:
            assert not any(p['name'] != 'RTC' and any(i['interrupt'] == 'RTC' for i in p.get('interrupts', [])) for p in core['peripherals'])
            assert rtc['rtc_calendar']['source'] == 'HSIOSC'
        chips += 1
assert families == data['profiles'].keys()
print(f'RTC alarm evidence verified: {len(families)} families, {chips} chip selections, {len(sources)} source hashes, {page_count} manual page hashes; no HAL code executed.')
