#!/usr/bin/env python3
"""Ordinary ARM builds and genuine external-input firmware links. No HAL execution."""
import hashlib
import json
import os
import shutil
import subprocess
import time
import tomllib
from pathlib import Path

root = Path(__file__).resolve().parents[1]
receipts = root / 'build/verification/classic-timer-input'
receipts.mkdir(parents=True, exist_ok=True)
target = Path(os.environ.get('CARGO_TARGET_DIR', str(root / 'target/classic-timer-input')))
env = os.environ | {'CARGO_TARGET_DIR': str(target), 'CARGO_INCREMENTAL': '0',
                    'CARGO_PROFILE_DEV_DEBUG': '0', 'CARGO_PROFILE_RELEASE_DEBUG': '0'}
buffered_only = os.environ.get('CW32_MATRIX_BUFFERED_ONLY') == '1'
results = ([x for x in json.loads((receipts / 'builds.json').read_text())
            if not x['label'].startswith('firmware-timer-input-')] if buffered_only else [])
def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()
def run(label, command):
    start = time.monotonic()
    with (receipts / f'{label}.log').open('w') as log:
        proc = subprocess.run(command, cwd=root, env=env, stdout=log, stderr=subprocess.STDOUT)
    result = {'label': label, 'command': command, 'exit_code': proc.returncode,
              'elapsed_seconds': round(time.monotonic() - start, 3), 'log_sha256': sha(receipts / f'{label}.log')}
    results.append(result)
    (receipts / 'builds.json').write_text(json.dumps(results, indent=2) + '\n')
    print(f'{label}: {proc.returncode}', flush=True)
    if proc.returncode:
        raise SystemExit(f'Build failed: {receipts / (label + ".log")}')
    return result

def save_generated(label, reserved=None):
    files = sorted(target.glob('thumbv6m-none-eabi/release/build/embassy-cw32-*/out/_generated.rs'), key=lambda p: p.stat().st_mtime_ns)
    assert files
    content = files[-1].read_text()
    if reserved:
        for capability in ('impl_gtim', 'impl_classic_input', 'impl_timer_pin', 'impl_capture_pin'):
            assert f'{capability}!({reserved},' not in content, (label, reserved, capability)
    (receipts / f'{label}-instances.rs').write_text(content)
    return sha(files[-1])

def clean_finished():
    # Preserve logs and firmware first. Only this private target's completed CW32
    # compiler packages are cleaned; dependency/toolchain caches remain shared.
    with (receipts / 'cache-cleanup.log').open('a') as log:
        result = subprocess.run(['cargo', 'clean', '--manifest-path', str(Path(__file__).resolve().parents[1] / 'firmware/Cargo.toml'), '--release', '--target', 'thumbv6m-none-eabi', '--target-dir', str(target), '-p', 'embassy-cw32', '-p', 'cw32-metapac'], cwd=root, env=env, stdout=log, stderr=subprocess.STDOUT)
    assert result.returncode == 0

features = tomllib.loads((root / 'embassy-cw32/Cargo.toml').read_text())['features']
classic = sorted(f for f in features if f.startswith('cw32') and not f.startswith(('cw32l010','cw32l011','cw32l012')))
representatives = ['cw32a030c8t7','cw32f002f3p7','cw32f003e4p7','cw32f020c6u7','cw32f030c8t7',
                   'cw32l031c8t6','cw32l052c8t6','cw32l083mct6','cw32r031c8u6','cw32w031r8u6']
base = ['cargo','build','--manifest-path', str(Path(__file__).resolve().parents[1] / 'firmware/Cargo.toml'),'--offline','--locked','--release','-p','embassy-cw32','--target','thumbv6m-none-eabi','--no-default-features','--features']
for chip in ([] if buffered_only else classic + ['cw32l010f8p6','cw32l011k8t6','cw32l012c8t6']):
    run(f'lib-{chip}', base + [f'{chip},rt'])
    save_generated(f'lib-{chip}')
    if chip in representatives or chip.startswith(('cw32l010','cw32l011','cw32l012')):
        run(f'lib-defmt-{chip}', base + [f'{chip},rt,defmt'])
    clean_finished()
for chip in ([] if buffered_only else representatives):
    reserved = 'GTIM' if chip.startswith(('cw32f002','cw32f003')) else 'GTIM1'
    driver = 'time-driver-gtim' if reserved == 'GTIM' else 'time-driver-gtim1'
    label = f'reserved-{chip}'
    result = run(label, base + [f'{chip},rt,defmt,{driver}'])
    result['reserved_instance'] = reserved
    result['generated_sha256'] = save_generated(label, reserved)
    clean_finished()

firmware=[]
def link(chip, manifest, extra=''):
    label = f'firmware-{Path(manifest).parent.name}-{chip}' + ('-reserved' if extra else '')
    result = run(label, ['cargo','build','--offline','--locked','--release','--bins','--target','thumbv6m-none-eabi',
                         '--manifest-path',manifest,'--no-default-features','--features',chip+extra])
    outputs=[]
    binaries = ('polling_capture','quadrature') if 'classic-timer-input' in manifest else ('polling_capture','quadrature_encoder')
    for binary in binaries:
        source=target / 'thumbv6m-none-eabi/release' / binary
        saved=receipts / 'elf' / (label + '-' + binary + '.elf')
        saved.parent.mkdir(exist_ok=True)
        shutil.copy2(source,saved)
        info=subprocess.check_output(['readelf','-h',str(saved)],text=True)
        assert 'ARM' in info and 'EXEC' in info
        (saved.with_suffix('.readelf.txt')).write_text(info)
        outputs.append({'binary':binary,'path':str(saved.relative_to(root)),'sha256':sha(saved),'bytes':saved.stat().st_size})
    candidates=sorted(target.glob('thumbv6m-none-eabi/release/build/cw32-classic-timer-input-examples-*/out/external-input-routes.txt'),key=lambda p:p.stat().st_mtime_ns)
    if 'classic-timer-input' in manifest:
        assert candidates
        result['external_input_routes']=candidates[-1].read_text().strip()
    result['firmware']=outputs
    firmware.extend(outputs)
    (receipts / 'builds.json').write_text(json.dumps(results,indent=2)+'\n')
    clean_finished()

parts=tomllib.loads((root/'examples/classic-timer-input/Cargo.toml').read_text())['features']
for chip in ([] if buffered_only else sorted(p for p in parts if p.startswith('cw32'))):
    link(chip,'examples/classic-timer-input/Cargo.toml')
for chip in ([] if buffered_only else representatives):
    if not chip.startswith(('cw32f002','cw32f003')):
        link(chip,'examples/classic-timer-input/Cargo.toml',',time-driver')
for chip in ('cw32l010f8p6','cw32l011k8t6','cw32l012c8t6'):
    link(chip,'examples/timer-input/Cargo.toml')
(receipts / 'builds.json').write_text(json.dumps(results,indent=2)+'\n')
(receipts / 'summary.json').write_text(json.dumps({'ordinary_builds':len(results),'firmware_elfs':sum(len(x.get('firmware',[])) for x in results),
     'classic_library_profiles':len(classic),'classic_exact_packages':sum(p.startswith('cw32') for p in parts),
     'failed':sum(x['exit_code']!=0 for x in results),'hal_tests_or_hardware_execution':False},indent=2)+'\n')
print('All ordinary builds and external-input firmware links completed.',flush=True)
