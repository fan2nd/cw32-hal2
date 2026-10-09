#!/usr/bin/env python3
"""Production ARM builds and real Embassy Timer links only; never run firmware.

The caller supplies CARGO_HOME/RUSTUP_HOME and a bounded private CARGO_TARGET_DIR.
Logs, ELF hashes/vector checks and ownership receipts go under verification-logs.
No HAL test crate, synthetic executor, mock registers or test adapter is built.
"""
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import sys
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[1]
OUT = Path(os.environ.get('CW32_TIME_VERIFICATION_DIR', str(ROOT / 'docs/verification-logs/time-driver/production'))).resolve()
assert OUT.is_relative_to(ROOT / 'docs/verification-logs')
TARGET = Path(os.environ['CARGO_TARGET_DIR']).resolve()
assert TARGET != (ROOT / 'target').resolve(), 'use a separate bounded target'
OUT.mkdir(parents=True, exist_ok=True)
env = os.environ.copy()
env.update(CARGO_INCREMENTAL='0', CARGO_PROFILE_DEV_DEBUG='0', CARGO_PROFILE_RELEASE_DEBUG='0')
records = []
BASE = ['cargo', 'build', '--offline', '--locked', '--release', '--target', 'thumbv6m-none-eabi']


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(label, args, expected_error=None):
    print(label, flush=True)
    result = subprocess.run(args, cwd=ROOT, env=env, capture_output=True)
    log = OUT / (label + '.log')
    log.write_bytes(result.stdout + result.stderr)
    text = log.read_text()
    valid = result.returncode == 0 if expected_error is None else result.returncode != 0 and expected_error in text
    records.append({'label': label, 'command': args, 'exit_code': result.returncode,
                    'expected_error': expected_error, 'passed': valid,
                    'log_sha256': sha(log), 'warning_lines': sum(x.startswith('warning:') and not (expected_error is not None and x == 'warning: build failed, waiting for other jobs to finish...') for x in text.splitlines())})
    (OUT / 'commands.json').write_text(json.dumps(records, indent=2) + '\n')
    if not valid:
        raise SystemExit(text)
    return text


def trim_target():
    # Dependencies/toolchains are reused in place. Only this private build output
    # can be cleaned, and only if it exceeds the declared 650 MiB working bound.
    total = sum(p.stat().st_size for p in TARGET.rglob('*') if p.is_file())
    if total > 650 * 1024 * 1024:
        run('trim-' + str(len(records)), ['cargo', 'clean', '--manifest-path', str(Path(__file__).resolve().parents[1] / 'firmware/Cargo.toml'), '--release', '--target', 'thumbv6m-none-eabi',
                                        '-p', 'embassy-cw32', '-p', 'cw32-metapac'])


def generated_ownership(chip, selected):
    # Choose the exact output selected by the build's chip metadata marker.
    matches = []
    for f in (TARGET / 'thumbv6m-none-eabi/release/build').glob('embassy-cw32-*/out/_generated.rs'):
        text = f.read_text()
        if f'pub(crate) use crate::pac::{selected} as TIME_DRIVER_REGS;' not in text:
            continue
        matches.append(f)
    assert matches
    # All generated time-driver outputs, regardless of chip, must reserve their
    # selected peripheral across tokens, instance, pin and DMA request paths.
    for f in matches:
        text = f.read_text()
        tokens = text.split('embassy_hal_internal::peripherals! {', 1)[1].split('}', 1)[0]
        assert f'    {selected},' not in tokens
        assert f'crate::timer::impl_gtim!({selected},' not in text
        assert f'crate::timer::impl_timer_pin!({selected},' not in text
        assert f'crate::dma::impl_request!({selected}_' not in text
        assert 'impl crate::rcc::SealedRccPeripheral for TimeDriverPeripheral' in text
    return len(matches)


def elf_receipt(chip, variant):
    elf = TARGET / 'thumbv6m-none-eabi/release/cw32-embassy-time-example'
    data = elf.read_bytes()
    assert data[:7] == b'\x7fELF\x01\x01\x01'
    assert struct.unpack_from('<H', data, 18)[0] == 40  # EM_ARM
    chipdata = json.loads((ROOT / 'cw32-data/data/chips' / (chip.upper() + '.json')).read_text())
    core = chipdata['cores'][0]
    name = 'GTIM' if chip.startswith(('cw32f002', 'cw32f003')) else 'GTIM1'
    peripheral = next(p for p in core['peripherals'] if p['name'] == name)
    irq = next(i['interrupt'] for i in peripheral['interrupts'] if i['signal'] == 'GLOBAL')
    number = next(i['number'] for i in core['interrupts'] if i['name'] == irq)
    symbols = subprocess.check_output(['nm', str(elf)], text=True)
    values = {line.split()[-1]: int(line.split()[0], 16) for line in symbols.splitlines() if len(line.split()) == 3}
    assert values[irq] != values.get('DefaultHandler', -1)
    shoff = struct.unpack_from('<I', data, 32)[0]
    entsize, count, strings = struct.unpack_from('<HHH', data, 46)
    headers = [struct.unpack_from('<10I', data, shoff + i * entsize) for i in range(count)]
    names = data[headers[strings][4]:headers[strings][4]+headers[strings][5]]
    vector = next(h for h in headers if names[h[0]:].split(b'\0', 1)[0] == b'.vector_table')
    word = struct.unpack_from('<I', data, vector[4] + (16 + number) * 4)[0]
    assert word == values[irq] and word & 1
    dest = OUT / f'{chip}-{variant}.elf'
    shutil.copyfile(elf, dest)
    (OUT / f'{chip}-{variant}.symbols.txt').write_text(symbols)
    return {'chip': chip, 'queue': variant, 'elf_sha256': sha(dest), 'elf_bytes': len(data),
            'irq': irq, 'irq_number': number, 'vector_word': word, 'handler_address': values[irq],
            'size': subprocess.check_output(['size', str(elf)], text=True).strip(),
            'executed': False}


# Reclassify/verify persisted receipts without repeating completed builds. This
# mode allows documentation/provenance-only finalization; every actual Rust,
# Cargo, selected metadata and firmware source from the build remains identical.
if '--verify-existing' in sys.argv:
    records = json.loads((OUT/'commands.json').read_text())
    for row in records:
        log = OUT/(row['label']+'.log')
        assert sha(log) == row['log_sha256']
        content = log.read_text()
        expected = row['expected_error']
        assert row['passed']
        assert (row['exit_code'] == 0) if expected is None else (row['exit_code'] != 0 and expected in content)
        warnings = [line for line in content.splitlines() if line.startswith('warning:')
                    and not (expected is not None and line == 'warning: build failed, waiting for other jobs to finish...')]
        assert not warnings, warnings
    snapshot = json.loads((ROOT/'docs/verification-logs/time-driver/final/source-after.json').read_text())
    noncompile = {'build/provenance/reference-index.json', 'build/provenance/SOURCE-CATALOG.md', 'build/provenance/source-lock.json'}
    checked = []
    for row in snapshot:
        rel = row['path']
        if rel.startswith('ci/') or rel in noncompile:
            continue
        assert sha(ROOT/rel) == row['sha256'], rel
        checked.append(row)
    links = json.loads((OUT/'firmware-links.json').read_text())
    for row in links:
        path = OUT/(row['chip']+'-'+row['queue']+'.elf')
        assert sha(path) == row['elf_sha256']
        assert row['vector_word'] == row['handler_address'] and row['vector_word'] & 1
        assert not row['executed']
    summary = {'status': 'passed', 'mode': 'verify immutable compilation/link receipts',
        'time_driver_builds': sum(row['label'].endswith(('-time', '-time-defmt')) and not row['label'].endswith('-without-time') for row in records),
        'without_driver_builds': sum(row['label'].endswith('-without-time') for row in records),
        'expected_feature_rejections': sum(row['expected_error'] is not None for row in records),
        'firmware_links': len(links), 'compiler_warning_lines': 0,
        'compiled_source_files_unchanged': len(checked), 'hardware_execution': False,
        'classification_correction': 'Original aggregate counted Cargo progress text from an expected feature rejection as a compiler warning. All command outcomes and logs were preserved and verified; no failed compile or link is relabeled as successful.',
        'finalization_exclusions': ['ci scripts', *sorted(noncompile)]}
    (OUT/'validated-compiled-inputs.json').write_text(json.dumps(checked, indent=2)+'\n')
    (OUT/'validated-summary.json').write_text(json.dumps(summary, indent=2)+'\n')
    print(json.dumps(summary, indent=2))
    raise SystemExit(0)

features = tomllib.loads((ROOT/'embassy-cw32/Cargo.toml').read_text())['features']
chips = sorted(x for x in features if x.startswith('cw32'))
for chip in chips:
    selected = 'GTIM' if chip.startswith(('cw32f002', 'cw32f003')) else 'GTIM1'
    for extra in ['', ',defmt']:
        label = chip + ('-time-defmt' if extra else '-time')
        run(label, BASE + ['--manifest-path', str(Path(__file__).resolve().parents[1] / 'firmware/Cargo.toml'), '-p', 'embassy-cw32', '--no-default-features', '--features',
                          chip + ',time-driver-' + selected.lower() + extra])
        generated_ownership(chip, selected)
    trim_target()

families = sorted(json.loads((ROOT/'docs/time-driver-evidence.json').read_text())['families'])
for family in families:
    chip = family.lower()
    run(chip + '-without-time', BASE + ['--manifest-path', str(Path(__file__).resolve().parents[1] / 'firmware/Cargo.toml'), '-p', 'embassy-cw32', '--no-default-features', '--features', chip + ',rt,defmt'])

invalid = [
    ('both-selectors', 'cw32f030,time-driver-gtim,time-driver-gtim1', 'select exactly one time-driver'),
    ('missing-gtim', 'cw32f030,time-driver-gtim', 'selected time driver GTIM is absent'),
    ('missing-gtim1', 'cw32f002,time-driver-gtim1', 'selected time driver GTIM1 is absent'),
    ('internal-without-selector', 'cw32f030,_time-driver', '_time-driver requires an explicit timer selector'),
]
for label, choice, diagnostic in invalid:
    run(label, BASE + ['--manifest-path', str(Path(__file__).resolve().parents[1] / 'firmware/Cargo.toml'), '-p', 'embassy-cw32', '--no-default-features', '--features', choice], diagnostic)

example_features = tomllib.loads((ROOT/'examples/embassy-time/Cargo.toml').read_text())['features']
links = []
for chip in sorted(x for x in example_features if x.startswith('cw32')):
    for variant in ['integrated', 'generic']:
        run(chip + '-link-' + variant, BASE + ['--manifest-path', 'examples/embassy-time/Cargo.toml',
            '--no-default-features', '--features', chip + (',generic-queue' if variant == 'generic' else '')])
        links.append(elf_receipt(chip, variant))
        (OUT/'firmware-links.json').write_text(json.dumps(links, indent=2)+'\n')
    trim_target()

assert not any(r['warning_lines'] for r in records)
print(json.dumps({'hal_chip_features': len(chips), 'time_driver_builds': len(chips)*2,
                  'without_driver_builds': len(families), 'expected_feature_rejections': len(invalid),
                  'firmware_links': len(links), 'warning_lines': 0, 'hardware_execution': False}), flush=True)
