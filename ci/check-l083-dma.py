#!/usr/bin/env python3
"""Build qualified software-copy firmware and HAL libraries; never execute them."""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[1]

def firmware_info(path, part):
    """Reject an empty successful link; inspect the actual ELF32 ARM image."""
    memory = json.loads((ROOT / 'cw32-data/data/chips' / (part.upper() + '.json')).read_text())['memory'][0]
    ram = next(region for region in memory if region['name'] == 'RAM')
    raw = path.read_bytes()
    header = struct.unpack_from('<16sHHIIIIIHHHHHH', raw)
    assert header[0][:6] == b'\x7fELF\x01\x01', 'expected little-endian ELF32'
    assert header[1:3] == (2, 40), 'expected ARM executable'
    entry, section_offset, section_size, section_count, names_index = (
        header[4], header[6], header[11], header[12], header[13])
    assert entry & 1 and section_size == 40, 'missing Thumb reset entry/sections'
    def section(index):
        assert index < section_count
        return struct.unpack_from('<IIIIIIIIII', raw, section_offset + index * section_size)
    names = section(names_index)
    strings = raw[names[4]:names[4] + names[5]]
    sections = {strings[row[0]:].split(b'\0', 1)[0].decode(): row
                for row in map(section, range(section_count))}
    vector, text = sections['.vector_table'], sections['.text']
    assert vector[2] & 2 and vector[3] == 0 and vector[5] >= 8, 'missing allocated vectors'
    assert text[2] & 6 == 6 and text[5] > 0, 'missing allocated executable text'
    assert text[3] <= entry & ~1 < text[3] + text[5], 'reset entry outside text'
    stack, reset = struct.unpack_from('<II', raw, vector[4])
    assert reset == entry, 'vector reset and ELF entry differ'
    assert ram['address'] < stack <= ram['address'] + ram['size'] and stack % 8 == 0, 'invalid SRAM stack vector'
    for name, row in sections.items():
        if row[2] & 2 and row[5]:
            assert any(region['address'] <= row[3] and row[3] + row[5] <= region['address'] + region['size']
                       for region in memory), 'allocated section outside selected memory: ' + name
    return {'part': part, 'entry': entry, 'initial_stack': stack,
            'sram_start': ram['address'], 'sram_end': ram['address'] + ram['size'],
            'vector_address': vector[3], 'vector_bytes': vector[5],
            'text_address': text[3], 'text_bytes': text[5]}

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--artifacts', type=Path, required=True,
                        help='external directory for exact linked ELF files and command receipts')
    args = parser.parse_args()
    artifacts = args.artifacts.resolve()
    if artifacts.is_relative_to(ROOT):
        parser.error('retain build products outside the distributed source tree')
    artifacts.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, CARGO_INCREMENTAL='0', CARGO_PROFILE_DEV_DEBUG='0',
               CARGO_PROFILE_RELEASE_DEBUG='0', CARGO_BUILD_JOBS='1')
    target = Path(env.get('CARGO_TARGET_DIR', ROOT / 'target')).resolve()
    features = tomllib.loads((ROOT / 'embassy-cw32/Cargo.toml').read_text())['features']
    profiles = sorted(f for f in features if f.startswith(('cw32a030', 'cw32f030', 'cw32l083')))
    parts = sorted(f for f in tomllib.loads((ROOT / 'examples/l083-dma/Cargo.toml').read_text())['features'] if f.startswith('cw32l083'))
    receipts = []
    def build(name, command, elf=None, cwd=ROOT, part=None):
        print(name, flush=True)
        log = artifacts / (name + '.log')
        with log.open('wb') as output:
            result = subprocess.run(command, cwd=cwd, env=env, stdout=output, stderr=subprocess.STDOUT)
        row = {'name': name, 'command': command, 'cwd': str(cwd), 'exit_code': result.returncode,
               'log': log.name, 'log_sha256': hashlib.sha256(log.read_bytes()).hexdigest()}
        if result.returncode == 0 and elf:
            destination = artifacts / (name + '.elf')
            shutil.copy2(target / 'thumbv6m-none-eabi/release' / elf, destination)
            row.update(elf=destination.name, bytes=destination.stat().st_size,
                       elf_sha256=hashlib.sha256(destination.read_bytes()).hexdigest())
            try:
                row['firmware'] = firmware_info(destination, part)
            except (AssertionError, KeyError, ValueError, struct.error) as error:
                row['artifact_error'] = str(error)
        receipts.append(row)
        (artifacts / 'receipts.json').write_text(json.dumps(receipts, indent=2) + '\n')
        if result.returncode or 'artifact_error' in row:
            raise SystemExit(result.returncode or row['artifact_error'])
    for profile in profiles:
        build('hal-' + profile, ['cargo', 'build', '--manifest-path', 'firmware/Cargo.toml',
              '--offline', '--locked', '--release', '--target', 'thumbv6m-none-eabi',
              '-p', 'embassy-cw32', '--no-default-features', '--features', profile + ',rt,defmt'])
    for part in parts:
        build('owned-copy-' + part, ['cargo', 'build', '--manifest-path', 'Cargo.toml',
              '--offline', '--locked', '--release', '--target', 'thumbv6m-none-eabi',
              '--no-default-features', '--features', part], 'cw32-l083-dma-example', cwd=ROOT / 'examples/l083-dma', part=part)
    build('owned-copy-cw32f030c8t7', ['cargo', 'build', '--manifest-path', 'Cargo.toml',
          '--offline', '--locked', '--release', '--target', 'thumbv6m-none-eabi', '--bin', 'dma_owned_copy'],
          'dma_owned_copy', cwd=ROOT / 'examples/cw32f030', part='cw32f030c8t7')
    print(f'Passed {len(profiles)} ARM library builds and {len(parts) + 1} firmware links; no execution.')

if __name__ == '__main__':
    main()
