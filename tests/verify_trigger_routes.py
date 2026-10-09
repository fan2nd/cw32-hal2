#!/usr/bin/env python3
"""Replay bounded L010/L011 route facts against each family's own locked sources."""
import argparse
import hashlib
import json
import os
import re
import subprocess
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[1]
PROFILES = {
    'cw32l010': {'adc_trigger': 521, 'adc_single': 509, 'adc_control': 517,
                'adc_stop': 518, 'adc_clear': 524, 'register_section': '20.12', 'bindings': 8},
    'cw32l011': {'adc_trigger': 523, 'adc_single': 511, 'adc_control': 518,
                'adc_stop': 519, 'adc_clear': 526, 'register_section': '20.11', 'bindings': 6},
}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--sources', type=Path, default=Path(os.environ.get(
        'CW32_SOURCES', str(Path('/workspace/shared/cw32-sources')
                           if Path('/workspace/shared/cw32-sources').is_dir()
                           else ROOT / 'sources/evidence'))))
    args = parser.parse_args()
    lock = json.loads((ROOT / 'sources/evidence-sources.json').read_text())
    locked = {}
    for artifact in lock['artifacts']:
        for source in [artifact, *artifact.get('members', [])]:
            locked[source['path']] = source['sha256']
    expectations = {}
    for family, pages in PROFILES.items():
        authored = yaml.safe_load((ROOT / f'cw32-data/triggers/{family}.yaml').read_text())
        assert authored['family'] == family.upper()
        for source in authored['sources']:
            path = args.sources / source['path']
            assert path.is_file(), path
            assert hashlib.sha256(path.read_bytes()).hexdigest() == source['sha256'], path
            assert locked.get(source['path']) == source['sha256'], f'Unpinned source: {path}'

        def page(number):
            result = subprocess.run(
                ['pdftotext', '-layout', '-f', str(number), '-l', str(number),
                 str(args.sources / authored['sources'][0]['path']), '-'],
                check=True, text=True, capture_output=True)
            return re.sub(r'\s+', ' ', result.stdout)

        # Own manual PDFs, not sibling-family or inventory-derived strings.
        text = page(181)
        assert '6:4 MMS RW' in text and re.search('010：更新信号.*更新事件作为触发输出', text)
        text = page(182)
        assert '1000：BTIM1_Trgo' in text and '10:7 TRGISRC RW' in text
        assert '切勿选择 BTIMx 自己输出的 Trgo' in text
        text = page(183)
        assert '011：外部计数模式' in text and '2:0 SMS RW' in text and '0000：无复位信号' in text
        text = page(163)
        assert '一个时钟周期后' in text and 'BTIMx_CR1.EN 被硬件自动清零' in text
        text = page(pages['adc_trigger'])
        assert 'BTIM1 TRGO 信号触发 ADC 启动 13 BTIM1TRGO RW 0：禁止 1：使能' in text
        text = page(pages['adc_single'])
        assert '20.5.2' in text and 'ADC_ISR.EOC' in text and 'ADC_ISR.EOS' in text
        assert 'ADC_START.START 位自动清 0' in text
        text = page(pages['adc_control'])
        assert '8:6 ENS RW' in text and '3 CONT RW' in text and '000：转换 SQRCH0' in text
        text = page(pages['adc_stop'])
        assert pages['register_section'] + '.2' in text
        assert '通过软件写 0 可停止转换并复位序列的转换通道' in text
        text = page(pages['adc_clear'])
        assert pages['register_section'] + '.10' in text
        assert '1 EOS R1W0' in text and '0 EOC R1W0' in text and 'W1：无功能' in text
        for reference in authored['runtime_references']:
            assert reference['source_sha256'] == authored['sources'][0]['sha256']
            assert reference['printed_page'] == reference['pdf_page_1based'] - 1
            assert reference['section'] in page(reference.get(
                'section_continued_from_pdf_page', reference['pdf_page_1based']))
        headers = {Path(s['path']).name: (args.sources / s['path']).read_text(errors='replace')
                   for s in authored['sources'][1:]}
        btim, adc, cmsis = (headers[family + suffix] for suffix in ['_btim.h', '_adc.h', '.h'])
        for pattern in [r'#define\s+BTIM_MASTER_OUTPUT_TRIGGER_UPDATE\s+\(2U\s*<<\s*4\)',
                        r'#define\s+BTIM_TS_BTIM1_TRGO\s+\(8U\s*<<\s*7\)',
                        r'#define\s+BTIM_MODE_COUNTER\s+\(\(uint16_t\)0x0003\)']:
            assert re.search(pattern, btim), pattern
        assert re.search(r'#define\s+ADC_TRIG_BTIM1TRGO\s+bv13', adc)
        for macro, position, mask in [
            ('BTIMx_CR2_MMS', 4, 0x70), ('BTIMx_SMCR_TRGISRC', 7, 0x780),
            ('BTIMx_SMCR_SMS', 0, 7), ('BTIMx_CR1_ONESHOT', 3, 8),
            ('ADC_TRIGGER_BTIM1TRGO', 13, 0x2000), ('ADC_CR_ENS', 6, 0x1c0),
            ('ADC_CR_CONT', 3, 8), ('ADC_START_START', 0, 1),
            ('ADC_ISR_EOC', 0, 1), ('ADC_ISR_EOS', 1, 2),
            ('ADC_ICR_EOC', 0, 1), ('ADC_ICR_EOS', 1, 2),
        ]:
            def value(suffix):
                return int(re.search(r'#define\s+' + macro + '_' + suffix
                                     + r'\s+\((0x[0-9a-fA-F]+|\d+)UL\)', cmsis)[1], 0)
            assert value('Pos') == position and value('Msk') == mask
        expected = {r['destination']: {k: r[k] for k in ['signal', 'source', 'registers']}
                    for r in authored['routes']}
        for route in authored['routes']:
            assert route['status'] == 'documented' and not route['conflict_ids']
            assert route['source_event'] == 'UPDATE'
        assert len(expected) == 2 and set(expected) == {'ADC', 'BTIM2'}
        expectations[family.upper()] = expected
    counts = dict.fromkeys(expectations, 0)
    for path in (ROOT / 'cw32-data/data/chips').glob('*.json'):
        chip = json.loads(path.read_text())
        expected = expectations.get(chip['line'], {})
        for peripheral in chip['cores'][0]['peripherals']:
            if peripheral['name'] in expected:
                assert peripheral.get('triggers') == [expected[peripheral['name']]], path
                counts[chip['line']] += 1
            else:
                assert not peripheral.get('triggers'), f'Unreviewed binding: {path}:{peripheral["name"]}'
    assert counts == {family.upper(): pages['bindings'] for family, pages in PROFILES.items()}, counts
    print('PASS L010/L011: own-family manual lifecycle/routes, eight locked PDF/SDK/CMSIS inputs, '
          '14 exact/alias bindings; other families withheld')


if __name__ == '__main__':
    main()
