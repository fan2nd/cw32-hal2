#!/usr/bin/env python3
"""Verify source-backed I2C limits and clock-envelope plumbing, without MMIO.

Uses hash-pinned original PDFs, not derived register YAML or copied tables as
its sole authority. Factory-HSI evidence is shared with the RCC bounds policy.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import yaml

ROOT = Path(__file__).resolve().parents[1]
SOURCES = Path(os.environ.get('CW32_SOURCES', str(ROOT.parent / 'cw32-sources')))
POLICY = json.loads((ROOT / 'docs/i2c-clock-bounds-sources.json').read_text())
CLOCKS = json.loads((ROOT / POLICY['clock_envelope_policy']).read_text())
ARTIFACTS = {a['path']: a for a in json.loads((ROOT / 'sources/evidence-sources.json').read_text())['artifacts']}
verified = set()


def page_text(citation):
    name = citation['source_ref'].removeprefix('vendor:')
    assert name in ARTIFACTS, f'Unregistered source: {name}'
    source = ARTIFACTS[name]
    path = SOURCES / name
    if name not in verified:
        assert hashlib.sha256(path.read_bytes()).hexdigest() == source['sha256'], name
        verified.add(name)
    assert citation['printed_pages'] and citation['pdf_pages_1_based']
    return '\n'.join(subprocess.check_output(['pdftotext', '-f', str(page), '-l', str(page), '-layout', str(path), '-'], text=True) for page in citation['pdf_pages_1_based'])


def flat(text):
    return re.sub(r'\s+', '', text)


def row_numbers(text, symbol):
    row = next(line.strip() for line in text.splitlines() if line.strip().startswith(symbol + ' '))
    return [float(n) for n in re.findall(r'(?<![A-Za-z])\d+(?:\.\d+)?', row[len(symbol):])]


def main():
    assert set(POLICY['families']) == set(CLOCKS['families']) and len(POLICY['families']) == 13
    for family, rules in POLICY['families'].items():
        ds = flat(page_text(rules['datasheet']))
        assert all(f'最高比特率{rate}' in ds for rate in ['100kbit/s', '400kbit/s', '1Mbit/s']), family
        assert rules['maximum_scl_hz'] == 1_000_000
        manual = flat(page_text(rules['manual']))
        if family != 'CW32L012':
            assert 'fSCL=fPCLK/8/(BRR+1)' in manual, family
            assert 'BRR有效范围为1~255' in manual, family
            filtering = flat(page_text(rules['filter_manual']))
            assert 'BRR的值小于或等于9' in filtering and 'FLT为1' in filtering
            assert 'BRR的值大于9' in filtering and 'FLT为0' in filtering
        else:
            for formula in ['(CLKHI+CLKLO+2+SCL_LATENCY)×2PRESCALE', '(CLKLO+1)×2PRESCALE', '(CLKHI+1+SCL_LATENCY)×2PRESCALE', '(SETHOLD+1)×2PRESCALE', '(SETHOLD+1+SCL_LATENCY)×2PRESCALE', '(DATAVD+1)×2PRESCALE', '(SDA_LATENCY+1)×2PRESCALE', 'ROUNDDOWN((2+FLTSCL+SCL_RISETIME)/(2PRESCALE))', 'ROUNDDOWN((2+FLTSDA+SDA_RISETIME)/(2PRESCALE))', 'CLKLO×2PRESCALE>SCL_LATENCY', 'SETHOLD×2PRESCALE>SDA_LATENCY', 'CLKLO-SDA_LATENCY-1', '(CLKLO+SETHOLD+2)×2']:
                assert formula in manual, formula
        # Independently re-open each own-family HSI page through the common
        # citation; never introduce a separate I2C oscillator-percentage table.
        hsi = CLOCKS['families'][family]['hsi']
        citation = hsi['evidence'].get('hsi_table_source', hsi['evidence'].get('accuracy'))
        hsi_text = flat(page_text(citation)).split('HSIOSC', 1)[1].split('低速内部', 1)[0]
        percent = hsi['factory_error_bound_ppm'] // 10_000
        temp = hsi['ambient_temperature_max_c']
        assert f'TA=-40℃~+{temp}℃-{percent}.0-+{percent}.0%' in hsi_text, family
        print(f'PASS {family}: own max rate, divider/filter or waveform equations, shared HSI qualification')

    own = page_text(POLICY['families']['CW32L012']['datasheet'])
    rows = {'low': ('tw(SCLL)', 1000), 'high': ('tw(SCLH)', 1000), 'start_hold': ('th(STA)', 1000), 'start_setup': ('tsu(STA)', 1000), 'stop_setup': ('tsu(STO)', 1000), 'data_setup': ('tsu(SDA)', 1), 'data_hold': ('th(SDA)', 1)}
    for key, (symbol, multiplier) in rows.items():
        assert [round(n * multiplier) for n in row_numbers(own, symbol)] == POLICY['l012_limits']['own_datasheet_ns'][key], key
    external = POLICY['l012_limits']['external_bus_spec']
    nxp = page_text(external)
    assert 'Rev. 7.0' in nxp and '1 October 2021' in nxp and 'Table 11.' in nxp
    assert row_numbers(nxp, 'tVD;DAT') == [3.45, 0.9, 0.45]
    assert row_numbers(nxp, 'tVD;ACK') == [3.45, 0.9, 0.45]
    assert row_numbers(nxp, 'tr') == [1000, 20, 300, 120]
    assert 'must only be met if the device does not stretch' in nxp
    l012 = (ROOT / 'embassy-cw32/src/i2c/lpi2c/timing.rs').read_text()
    catalog = yaml.safe_load((ROOT / 'cw32-data/electrical.yaml').read_text())
    waveform = catalog['profiles']['CW32L012']['i2c_limits']['waveform']
    for key, values in POLICY['l012_limits']['solver_ns'].items():
        assert waveform[key] == values, (key, waveform[key], values)
        assert 'crate::I2C_TIMING_' + key.upper() + '[index]' in l012
    for family, profile in catalog['profiles'].items():
        chip = json.loads((ROOT / f'cw32-data/data/chips/{family}.json').read_text())
        values = profile['i2c_limits']
        assert values['maximum_frequency_hz'] == POLICY['families'][family]['maximum_scl_hz']
        for peripheral in chip['cores'][0]['peripherals']:
            if peripheral.get('registers', {}).get('kind') == 'i2c':
                assert peripheral['i2c_limits'] == values
    assert 'u64::from(bounds.maximum().0)' in l012 and 'u64::from(bounds.minimum().0)' in l012
    assert 'u64::from(config.sda_rise_time_ns) * slow_clock' in l012 and '> spec.data_valid * slow_clock' in l012
    assert 'bounds.divided_by(period as u32).nominal()' in l012
    classic = (ROOT / 'embassy-cw32/src/i2c/classic.rs').read_text()
    assert 'u64::from(clock.maximum().0)' in classic and '.div_ceil(8 * u64::from(config.frequency.0))' in classic
    assert 'clock.divided_by(8 * divisor as u32).nominal()' in classic
    assert 'self.brr <= 9' in classic and 'divisor > 256' in classic
    for backend in ['classic', 'lpi2c']:
        relative = 'classic.rs' if backend == 'classic' else 'lpi2c/mod.rs'
        driver = (ROOT / 'embassy-cw32/src/i2c' / relative).read_text()
        assert 'crate::rcc::bus_clock_bounds::<T>()' in driver and 'kernel_clock: ClockBounds' in driver
        assert 'select_timing(self.kernel_clock, config)?' in driver
        assert 'pub fn get_current_frequency_bounds(&self) -> (Hertz, Hertz)' in driver
    print(f'PASS {len(verified)} original PDF hashes; own L012 minima, external data-valid/rise limits, runtime extrema and nominal-reporting plumbing')


if __name__ == '__main__':
    main()
