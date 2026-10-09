#!/usr/bin/env python3
"""Build normal ARM libraries and exact-package ADC firmware; never run them."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time
import tomllib

root = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--profiles', nargs='*', help='Limit to selected normal feature profiles')
args = parser.parse_args()
receipts = root / 'docs/verification-logs/classic-adc-scan'
receipts.mkdir(parents=True, exist_ok=True)
target = Path(os.environ.get('CARGO_TARGET_DIR', str(root / 'target/classic-adc-scan'))).resolve()
env = os.environ | {'CARGO_TARGET_DIR': str(target), 'CARGO_INCREMENTAL': '0',
                    'CARGO_PROFILE_DEV_DEBUG': '0', 'CARGO_PROFILE_RELEASE_DEBUG': '0'}

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

source_files = [p for base in ('embassy-cw32', 'cw32-metapac', 'firmware', 'examples/classic-adc-scan')
                for p in (root / base).rglob('*')
                if p.is_file() and 'target' not in p.relative_to(root).parts
                and (p.suffix == '.rs' or p.name in ('Cargo.toml', 'Cargo.lock'))]
source_fingerprint = hashlib.sha256(json.dumps([(str(p.relative_to(root)), sha(p))
    for p in sorted(source_files)]).encode()).hexdigest()
result_path = receipts / 'build-receipts.json'
results = json.loads(result_path.read_text()) if result_path.exists() else []

def save_receipts():
    result_path.write_text(json.dumps(results, indent=2) + '\n')

def run(label, command):
    completed = [r for r in results if r['label'] == label and r['exit_code'] == 0
                 and r.get('source_fingerprint') == source_fingerprint]
    if completed:
        print(f'{label}: already retained for this source', flush=True)
        return None
    start = time.monotonic()
    log_path = receipts / (label + '-' + source_fingerprint[:12] + '-' + str(len(results)) + '.log')
    with log_path.open('w') as log:
        proc = subprocess.run(command, cwd=root, env=env, stdout=log, stderr=subprocess.STDOUT)
    record = {'label': label, 'command': command, 'exit_code': proc.returncode,
              'source_fingerprint': source_fingerprint,
              'elapsed_seconds': round(time.monotonic()-start, 3), 'log_path': str(log_path.relative_to(root)), 'log_sha256': sha(log_path)}
    results.append(record)
    save_receipts()
    print(f'{label}: {proc.returncode}', flush=True)
    if proc.returncode:
        raise SystemExit(f'Build failed: {log_path}')
    return record

all_features = tomllib.loads((root / 'embassy-cw32/Cargo.toml').read_text())['features']
classic = sorted(f for f in all_features if f.startswith('cw32') and not f.startswith(('cw32l010','cw32l011','cw32l012')))
parts = sorted(f for f in tomllib.loads((root / 'examples/classic-adc-scan/Cargo.toml').read_text())['features'] if f.startswith('cw32'))
regressions = ['cw32l010f8p6', 'cw32l011k8t6', 'cw32l012c8t6']
representatives = ['cw32a030c8t7', 'cw32f002f3p7', 'cw32f003e4p7', 'cw32f020c6u7', 'cw32f030c8t7',
                   'cw32l031c8t6', 'cw32l052c8t6', 'cw32l083mct6', 'cw32r031c8u6', 'cw32w031r8u6']
profiles = classic + regressions
if args.profiles:
    assert set(args.profiles) <= set(profiles), 'unknown profile'
    profiles = args.profiles
base = ['cargo', 'build', '--manifest-path', str(Path(__file__).resolve().parents[1] / 'firmware/Cargo.toml'), '--offline', '--locked', '--release', '-p', 'embassy-cw32', '--target',
        'thumbv6m-none-eabi', '--no-default-features', '--features']
for chip in profiles:
    run('library-' + chip, base + [chip + ',rt'])
    if chip in representatives:
        run('library-defmt-' + chip, base + [chip + ',rt,defmt'])
    if chip in parts:
        label = 'firmware-' + chip
        record = run(label, ['cargo', 'build', '--offline', '--locked', '--release', '--bins', '--target',
                            'thumbv6m-none-eabi', '--manifest-path', 'examples/classic-adc-scan/Cargo.toml',
                            '--no-default-features', '--features', chip])
        if record is not None:
            outputs = []
            for binary in ('borrowed_ordered', 'owned_ordered'):
                source = target / 'thumbv6m-none-eabi/release' / binary
                saved = receipts / 'elf' / (chip + '-' + binary + '.elf')
                saved.parent.mkdir(exist_ok=True)
                shutil.copyfile(source, saved)
                inspection = subprocess.check_output(['readelf', '-h', str(saved)], text=True)
                assert 'ARM' in inspection and 'EXEC' in inspection
                saved.with_suffix('.readelf.txt').write_text(inspection)
                outputs.append({'binary':binary, 'path':str(saved.relative_to(root)),
                                'sha256':sha(saved), 'bytes':saved.stat().st_size})
            routes = sorted(target.glob('thumbv6m-none-eabi/release/build/cw32-classic-adc-scan-examples-*/out/analog-input-routes.txt'),
                            key=lambda p:p.stat().st_mtime_ns)
            assert routes
            record['analog_input_routes'] = routes[-1].read_text().strip()
            record['firmware'] = outputs
            save_receipts()
    # Reproducible private package caches are removed only after all linked
    # firmware for this profile has been retained with hashes and ARM headers.
    if shutil.disk_usage(target).free < 1_500_000_000:
        with (receipts / 'cache-cleanup.log').open('a') as log:
            cleanup = subprocess.run(['cargo', 'clean', '--manifest-path', str(Path(__file__).resolve().parents[1] / 'firmware/Cargo.toml'), '--release', '--target', 'thumbv6m-none-eabi',
                                      '--target-dir', str(target), '-p', 'embassy-cw32', '-p', 'cw32-metapac'],
                                     cwd=root, env=env, stdout=log, stderr=subprocess.STDOUT)
        assert cleanup.returncode == 0

current = [r for r in results if r.get('source_fingerprint') == source_fingerprint]
(receipts / 'summary.json').write_text(json.dumps({
    'source_fingerprint':source_fingerprint, 'ordinary_build_commands':len(current),
    'successful_commands':sum(r['exit_code']==0 for r in current),
    'retained_firmware_elfs':sum(len(r.get('firmware',[])) for r in current),
    'classic_library_profiles':len(classic), 'classic_exact_packages':len(parts),
    'requested_profiles':profiles, 'hal_tests_or_firmware_execution':False,
}, indent=2)+'\n')
print('Normal ARM builds complete; no firmware executed.', flush=True)
