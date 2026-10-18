#!/usr/bin/env python3
"""Compile real HAL APIs, checking both allowed borrows and rejected aliases."""
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
HEADER = '#![no_std]\n#![allow(dead_code)]\nuse embassy_cw32::usart::BufferedUartRx;\n'
CASES = [
    ('reclaim', "fn probe<'a>(rx: BufferedUartRx<'a>) -> &'a mut [u8] { rx.into_buffer() }", None),
    ('no_static_promotion', "fn probe<'a>(rx: BufferedUartRx<'a>) -> &'static mut [u8] { rx.into_buffer() }", 'lifetime may not live long enough'),
    ('reclaim_after_waiter', "fn probe(mut rx: BufferedUartRx<'_>) { let wait = rx.fill_buf(); drop(wait); let _ = rx.into_buffer(); }", None),
    ('reclaim_live_waiter', "fn probe(mut rx: BufferedUartRx<'_>) { let wait = rx.fill_buf(); let _ = rx.into_buffer(); drop(wait); }", 'E0505'),
    ('replace_after_waiter', "fn probe(mut rx: BufferedUartRx<'_>, storage: &'static mut [u8]) { let wait = rx.fill_buf(); drop(wait); let _ = rx.try_replace_buffer(storage); }", None),
    ('replace_live_waiter', "fn probe(mut rx: BufferedUartRx<'_>, storage: &'static mut [u8]) { let wait = rx.fill_buf(); let _ = rx.try_replace_buffer(storage); drop(wait); }", 'E0499'),
    ('replace_live_slice', "async fn probe(mut rx: BufferedUartRx<'_>, storage: &'static mut [u8]) { let data = rx.fill_buf().await.unwrap(); let _ = rx.try_replace_buffer(storage); core::hint::black_box(data); }", 'E0499'),
    ('replacement_must_be_static', "fn probe(mut rx: BufferedUartRx<'_>, storage: &mut [u8]) { let _ = rx.try_replace_buffer(storage); }", 'E0521'),
]
DAC = [
    ('dac_split', 'fn probe(dac: &mut embassy_cw32::dac::Dac<embassy_cw32::peripherals::DAC>) { let (a, b) = dac.split(); let _ = (a.unwrap().read(), b.unwrap().read()); }', None),
    ('dac_parent_exclusion', 'fn probe(dac: &mut embassy_cw32::dac::Dac<embassy_cw32::peripherals::DAC>) { let (a, _b) = dac.split(); let _ = dac.read(embassy_cw32::dac::Channel::Ch1); let _ = a.unwrap().read(); }', 'E0502'),
]


I2C = [
    ('i2c_cancel_then_reuse', "fn probe(i2c: &mut embassy_cw32::i2c::I2c<'_, embassy_cw32::mode::Async>) { let wait = i2c.write(42, &[1]); drop(wait); let _ = i2c.write(42, &[2]); }", None),
    ('i2c_live_transfer', "fn probe(i2c: &mut embassy_cw32::i2c::I2c<'_, embassy_cw32::mode::Async>) { let wait = i2c.write(42, &[1]); let _ = i2c.write(42, &[2]); drop(wait); }", 'E0499'),
    ('i2c_read_buffer', "fn probe(i2c: &mut embassy_cw32::i2c::I2c<'_, embassy_cw32::mode::Async>, data: &mut [u8]) { let wait = i2c.read(42, data); data[0] = 0; drop(wait); }", 'E0503'),
]
WWDT = [
    ('wwdt_cancel_then_reuse', "fn probe(w: &mut embassy_cw32::wdg::WindowWatchdog<'_, embassy_cw32::peripherals::WWDT>) { let wait = w.wait_early_warning(); drop(wait); let _ = w.try_pet(); }", None),
    ('wwdt_live_wait', "fn probe(w: &mut embassy_cw32::wdg::WindowWatchdog<'_, embassy_cw32::peripherals::WWDT>) { let wait = w.wait_early_warning(); let _ = w.wait_early_warning(); drop(wait); }", 'E0499'),
    ('wwdt_drop_live_wait', "fn probe(mut w: embassy_cw32::wdg::WindowWatchdog<'_, embassy_cw32::peripherals::WWDT>) { let wait = w.wait_early_warning(); drop(w); drop(wait); }", 'E0505'),
]


ANALOG_BINDINGS = "embassy_cw32::bind_interrupts!(struct Irqs { LVD => embassy_cw32::lvd::InterruptHandler; VC13 => embassy_cw32::comparator::InterruptHandler<embassy_cw32::peripherals::VC1>, embassy_cw32::comparator::InterruptHandler<embassy_cw32::peripherals::VC3>; });"
ANALOG = []
for label, module, owner in [
    ('lvd', 'lvd', "embassy_cw32::lvd::LowVoltageMonitor<'_>"),
    ('comparator', 'comparator', "embassy_cw32::comparator::Comparator<'_, embassy_cw32::peripherals::VC1>"),
]:
    wait = f"v.wait_for_edge(embassy_cw32::{module}::Edge::Either, Irqs)"
    ANALOG.extend([
        (label + '_cancel_then_reuse', ANALOG_BINDINGS + f"fn probe(v: &mut {owner}) {{ let wait = {wait}; drop(wait); let _ = {wait}; }}", None),
        (label + '_live_wait', ANALOG_BINDINGS + f"fn probe(v: &mut {owner}) {{ let wait = {wait}; let _ = {wait}; drop(wait); }}", 'E0499'),
        (label + '_drop_live_wait', ANALOG_BINDINGS + f"fn probe(mut v: {owner}) {{ let wait = {wait}; drop(v); drop(wait); }}", 'E0505'),
    ])

def main():
    env = os.environ.copy()
    env['CARGO_INCREMENTAL'] = '0'
    rows = []
    output = ROOT / 'build/acceptance/ownership'
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='cw32-ownership-') as directory:
        path = Path(directory)
        (path / 'src').mkdir()
        for chip in ['cw32f030c8t7', 'cw32l010f8p6', 'cw32l011k8t6', 'cw32l012c8t6']:
            (path / 'Cargo.toml').write_text('[package]\nname="cw32-ownership"\nversion="0.0.0"\nedition="2024"\n[workspace]\n[dependencies]\nembassy-cw32={path=' + json.dumps(str(ROOT / 'embassy-cw32')) + ',default-features=false,features=[' + json.dumps(chip) + ']}\n')
            adc = 'ADC1' if chip == 'cw32l012c8t6' else 'ADC'
            adc_type = f"embassy_cw32::adc::Adc<'_, embassy_cw32::peripherals::{adc}, embassy_cw32::mode::Async>"
            adc_cases = [
                ('adc_cancel_then_reuse', f"fn probe(adc: &mut {adc_type}) {{ let wait = adc.read_sequence(&[], &mut []); drop(wait); let _ = adc.read_sequence(&[], &mut []); }}", None),
                ('adc_live_transfer', f"fn probe(adc: &mut {adc_type}) {{ let wait = adc.read_sequence(&[], &mut []); let _ = adc.read_sequence(&[], &mut []); drop(wait); }}", 'E0499'),
                ('adc_live_output', f"fn probe(adc: &mut {adc_type}, data: &mut [u16]) {{ let wait = adc.read_sequence(&[], data); data[0] = 0; drop(wait); }}", 'E0503'),
            ]
            for name, source, expected in CASES + I2C + adc_cases + (DAC + WWDT + ANALOG if chip == 'cw32l012c8t6' else []):
                (path / 'src/lib.rs').write_text(HEADER + source)
                command = ['cargo', 'check', '--offline', '--manifest-path', str(path / 'Cargo.toml'), '--target', 'thumbv6m-none-eabi', '--target-dir', str(ROOT / 'target/ownership'), '--message-format=json']
                result = subprocess.run(command, env=env, text=True, capture_output=True)
                messages = [json.loads(line)['message'] for line in result.stdout.splitlines() if json.loads(line).get('reason') == 'compiler-message']
                errors = [m for m in messages if m['level'] == 'error']
                passed = result.returncode == 0 if expected is None else result.returncode != 0 and any(m.get('code', {}).get('code') == expected if m.get('code') else expected in m['message'] for m in errors)
                (output / f'{chip}-{name}.log').write_text(result.stdout + result.stderr)
                rows.append({'chip': chip, 'case': name, 'expected': expected, 'passed': passed})
                print(chip, name, 'PASS' if passed else 'FAIL', flush=True)
                if not passed:
                    print(result.stdout + result.stderr)
                    raise SystemExit(1)
    (output / 'results.json').write_text(json.dumps(rows, indent=2))
    print(f'PASS {len(rows)} positive/negative ownership compilations; no MMIO execution')

if __name__ == '__main__':
    main()
