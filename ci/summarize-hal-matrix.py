#!/usr/bin/env python3
"""Summarize a successful input-frozen production ARM build matrix."""
import argparse
import json
from pathlib import Path
import re
import tomllib

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('directory', type=Path)
args = parser.parse_args()
run = json.loads((args.directory / 'summary.json').read_text())
if run['status'] != 'passed' or not run['source_unchanged']:
    raise SystemExit('The build run did not pass on unchanged inputs.')
log = (args.directory / 'run.log').read_text()
features = tomllib.loads((ROOT / 'embassy-cw32/Cargo.toml').read_text())['features']
expected = sorted(c for c in features if c.startswith('cw32'))
variants = ['and runtime', 'with runtime and defmt']
arm = re.findall(r'^=== (cw32\w+): Cortex-M0\+ release library (and runtime|with runtime and defmt) ===$', log, re.M)
if arm != [(chip, variant) for chip in expected for variant in variants] or 'HAL ARM production matrix passed.' not in log:
    raise SystemExit('The log does not contain the complete declared ARM matrix.')
families = sorted({json.loads((ROOT / 'cw32-data/data/chips' / (chip.upper() + '.json')).read_text())['line'] for chip in expected})
examples = 'HAL firmware example builds passed.' in log
example_binaries = ['cw32f030/blocking', 'cw32f030/async_gpio_edge'] if examples else []
if examples and '=== Link ATIM counter/main-PWM firmware for 11 exact packages ===' in log:
    atim_features = tomllib.loads((ROOT / 'examples/atim/Cargo.toml').read_text())['features']
    atim_expected = sorted(c for c in atim_features if c.startswith('cw32'))
    atim_built = re.findall(r'^=== Link ATIM counter/main-PWM for (cw32\w+) ===$', log, re.M)
    if sorted(atim_built) != atim_expected:
        raise SystemExit('ATIM example package builds are incomplete.')
    example_binaries += [f'atim/{chip}/{binary}' for chip in atim_built for binary in ['polling_counter', 'main_pwm']]
if examples and '=== Link reserved-FLASH storage firmware for 37 exact packages ===' in log:
    flash_features = tomllib.loads((ROOT / 'examples/flash-storage/Cargo.toml').read_text())['features']
    flash_expected = sorted(c for c in flash_features if c.startswith('cw32'))
    flash_built = re.findall(r'^=== Link reserved-FLASH storage for (cw32\w+) ===$', log, re.M)
    if sorted(flash_built) != flash_expected:
        raise SystemExit('Flash storage example package builds are incomplete.')
    example_binaries += [f'flash-storage/{chip}' for chip in flash_built]
if examples:
    example_binaries += ['cw32f030/lvd_ir_bursts', 'cw32f030/dma_owned_copy']
    for directory, marker, binaries in [
        ('comparator', '=== Link external comparator polling firmware for 13 exact packages ===', ['cw32-comparator-example']),
        ('low-power-timers', '=== Link AWT/LPTIM polling and IRQ firmware for 13 exact packages ===', ['polling', 'interrupt']),
        ('l012-analog', '=== Link DAC/OPA firmware for both exact L012 packages ===', ['dac_steps', 'opa_buffer', 'opa_pga', 'opa_standalone']),
        ('l083-crypto', '=== Link hardware-word AES and raw TRNG firmware for five exact L083 packages ===', ['aes-blocks', 'trng-samples']),
        ('lcd', '=== Link LCD, HALLTIM and passive RAM diagnostic firmware ===', ['cw32-lcd-example']),
        ('halltim', '=== Link LCD, HALLTIM and passive RAM diagnostic firmware ===', ['capture']),
        ('ram-parity', '=== Link LCD, HALLTIM and passive RAM diagnostic firmware ===', ['cw32-ram-parity-example']),
        ('timer-input', '=== Link qualified timer-input and UART RTS/CTS firmware ===', ['polling_capture', 'quadrature_encoder']),
        ('uart-flow-control', '=== Link qualified timer-input and UART RTS/CTS firmware ===', ['blocking_echo', 'async_echo', 'blocking_sender', 'async_sender', 'blocking_receiver', 'async_receiver']),
        ('rtc-calendar', '=== Link RTC calendar firmware for36 explicit package/legacy selections across eleven families ===', ['initialize_calendar', 'preserve_calendar', 'set_calendar']),
        ('autotrim-counter', '=== Link AUTOTRIM counter firmware for eight exact packages ===', ['cw32-autotrim-counter-example']),
        ('l012-math', '=== Link EAU/CORDIC firmware for both exact L012 packages ===', ['eau_integer', 'cordic_fixed_point']),
    ]:
        if marker not in log:
            raise SystemExit(f'Missing current {directory} example batch.')
        selections = tomllib.loads((ROOT / 'examples' / directory / 'Cargo.toml').read_text())['features']
        example_binaries += [f'{directory}/{chip}/{binary}' for chip in sorted(selections) if chip.startswith('cw32') for binary in binaries]
if examples:
    if '=== Link all declared RTC alarm firmware selections ===' not in log:raise SystemExit('Missing RTC alarm example batch')
    alarm_features=tomllib.loads((ROOT/'examples/rtc-alarms/Cargo.toml').read_text())['features']
    for chip,flags in alarm_features.items():
        if chip.startswith('cw32'):
            example_binaries.append(f'rtc-alarms/{chip}/poll_alarm')
            if 'hsi-source' in flags:example_binaries.append(f'rtc-alarms/{chip}/interrupt_alarm')
summary = {**run, 'chip_features': len(expected), 'families': len(families),
           'completed_arm_release_builds': len(arm), 'feature_selections': expected,
           'build_variants': ['rt', 'rt,defmt'],
           'real_firmware_examples_built': example_binaries,
           'warning_lines': sum(line.startswith('warning:') for line in log.splitlines()),
           'scope': 'Production ARM library builds and, when requested, existing firmware examples. HAL tests were deleted. Source audits and physical hardware validation are separate.'}
(args.directory / 'matrix-summary.json').write_text(json.dumps(summary, indent=2) + '\n')
print(json.dumps({k: summary[k] for k in ['chip_features', 'families', 'completed_arm_release_builds', 'real_firmware_examples_built', 'source_unchanged', 'warning_lines']}, indent=2))
