#!/usr/bin/env python3
"""Replay the finite, pinned inverse of the L010/L011 RCC module extraction.

This acceptance proof needs neither Git history nor a compiler. It does not
normalize Rust expressions, ignore unknown tokens, or erase field accesses.
Every source byte is checked, all introduced scaffolding must match exactly,
and every original line is reconstructed exactly once before SHA-256 checking.
Run with --self-test to require rejection of the dangerous control-flow edits.
"""
from pathlib import Path
import argparse
import hashlib
import json
import re

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / 'docs/l010-rcc-module-relocation.json'
# Pins both transport scaffolding and every finite formatter inverse hunk.
# Changing this acceptance boundary requires an explicit review, not regeneration
# from the candidate source to make a failed comparison disappear.
MANIFEST_SHA256 = 'f5f9cd669c93f86f2f0f8f457dd8d1458386dc5a73e70049cad0a2cf3571c033'
ORIGINAL_SHA256 = '1f40106a0ab9f60fddcb90b3e2b8a8ced96e824a3b2fa95bf6ef1d639f8bf99d'
FIELD_MAP = {
    'expected_clock': 'expected.clock',
    'expected_sources': 'expected.sources',
    'expected_hsi': 'expected.hsi',
    'expected_hsi_stable': 'expected.hsi_stable',
    'expected_hse': 'expected.hse',
    'expected_hse_stable': 'expected.hse_stable',
    'expected_wait': 'expected.wait',
    'lsi_target': 'targets.lsi',
    'lse_target': 'targets.lse',
}
PATH_MAP = {
    'super::lse::': 'crate::rcc::lse::',
    'super::peripheral::': 'crate::rcc::peripheral::',
}
MODULES = {
    'mod.rs', 'io.rs', 'inspection.rs', 'lsi_sysclk.rs', 'lse_sysclk.rs',
    'configure/mod.rs', 'configure/admission.rs', 'configure/flash_bridge.rs',
    'configure/calibrate_hsi.rs', 'configure/establish_hse.rs', 'configure/finish.rs',
}


def sha(text):
    return hashlib.sha256(text.encode()).hexdigest()


def load_manifest():
    raw = MANIFEST.read_bytes()
    assert hashlib.sha256(raw).hexdigest() == MANIFEST_SHA256, 'Acceptance manifest changed'
    manifest = json.loads(raw)
    assert manifest['schema'] == 1
    assert manifest['original_sha256'] == ORIGINAL_SHA256
    assert manifest['original_lines'] == 3622
    assert manifest['field_map'] == FIELD_MAP
    assert manifest['path_map'] == PATH_MAP
    assert {d['path'] for d in manifest['destinations']} == MODULES
    return manifest


def approve_line(before, after, segment):
    """Only enumerate exact identifier, ancestry, and sibling visibility edits."""
    candidate = before
    if segment['kind'] != 'root':
        for old, new in PATH_MAP.items():
            candidate = candidate.replace(old, new)
    if segment['kind'] == 'phase':
        for old, new in FIELD_MAP.items():
            candidate = re.sub(r'\b' + old + r'\b', new, candidate)
    for name in segment['public']:
        candidate = re.sub(r'^(\s*)(fn|struct) ' + name + r'\b',
                           r'\1pub(super) \2 ' + name, candidate)
    if segment['fields'] and re.match(r'^    [a-z_]+:', candidate):
        candidate = candidate.replace('    ', '    pub(super) ', 1)
    assert candidate == after, ('Unapproved source edit', before, after)


def reconstruct(manifest, actual, *, check_output_hashes=True):
    assert set(actual) == MODULES, 'Extra or missing source module'
    original = {}
    for destination in manifest['destinations']:
        path = destination['path']
        formatted = actual[path]
        if check_output_hashes:
            assert sha(formatted) == destination['sha256'], f'Changed source: {path}'
        raw = formatted
        previous_start = len(formatted) + 1
        for edit in reversed(destination['formatter_inverse']):
            start, end = edit['formatted_start'], edit['formatted_end']
            assert 0 <= start <= end < previous_start, f'Overlapping formatter edits: {path}'
            assert raw[start:end] == edit['formatted'], f'Unknown formatter hunk: {path}'
            raw = raw[:start] + edit['raw'] + raw[end:]
            previous_start = start
        if check_output_hashes:
            assert sha(raw) == destination['raw_sha256'], f'Raw inverse mismatch: {path}'
        offset = 0
        for segment in destination['segments']:
            if 'scaffold' in segment:
                text = segment['scaffold']
                assert raw[offset:offset + len(text)] == text, f'Unknown scaffolding: {path}'
                offset += len(text)
                continue
            text = raw[offset:offset + segment['raw_length']]
            offset += segment['raw_length']
            a, b = segment['old_lines']
            lines = text.splitlines(keepends=True)
            assert len(lines) == b - a + 1, f'Line partition changed: {path}'
            rewrites = {r['old_line']: r for r in segment['line_rewrites']}
            assert len(rewrites) == len(segment['line_rewrites'])
            for number, line in zip(range(a, b + 1), lines):
                if number in rewrites:
                    rewrite = rewrites.pop(number)
                    assert line == rewrite['after'], f'Changed transformed line: {number}'
                    approve_line(rewrite['before'], rewrite['after'], segment)
                    line = rewrite['before']
                assert number not in original, f'Duplicated original line: {number}'
                original[number] = line
            assert not rewrites, 'Out-of-range rewrite'
        assert offset == len(raw), f'Unaccounted source: {path}'
    assert sorted(original) == list(range(1, 3623)), 'Omitted or fabricated original line'
    baseline = ''.join(original[n] for n in sorted(original))
    assert sha(baseline) == ORIGINAL_SHA256, 'Baseline byte reconstruction failed'
    return baseline


def mutation_tests(manifest, actual):
    results = []

    def reject(name, path=None, before=None, after=None, operation=None):
        mutated = actual.copy()
        if operation:
            operation(mutated)
        else:
            assert before in mutated[path], (name, 'mutation target missing')
            mutated[path] = mutated[path].replace(before, after, 1)
        try:
            # Exercise the inverse and original-source oracle, not an opaque
            # candidate-file checksum rejection. The acceptance run still pins
            # both output hashes and the complete immutable manifest.
            reconstruct(manifest, mutated, check_output_hashes=False)
        except AssertionError:
            results.append(name)
        else:
            raise AssertionError(f'Dangerous mutation accepted: {name}')

    reject('swapped CR0/CR1 observations', 'configure/mod.rs',
           'let old_clock = r.cr0().read();\n    let old_sources = r.cr1().read();',
           'let old_sources = r.cr1().read();\n    let old_clock = r.cr0().read();')
    reject('moved L011 post_observation', 'configure/mod.rs',
           'entry.post_observation()?;\n        entry.admit()?;',
           'entry.admit()?;\n        entry.post_observation()?;')
    reject('eager ccs closure evaluation', 'configure/mod.rs',
           'let ccs_unchanged = |after_lse: bool| {',
           'let _eager = r.cr1().read();\n    let ccs_unchanged = |after_lse: bool| {')
    reject('lost retained inspect gate restore', 'configure/admission.rs',
           '.inspect_for_init(cs, config.timeout, || {',
           '.inspect_without_restore(cs, config.timeout, || {')
    reject('reversed Flash error priority', 'configure/flash_bridge.rs',
           'flash_write.and(current_wait)?', 'current_wait.and(flash_write)?')
    reject('wrong WAIT mirror', 'configure/flash_bridge.rs',
           'expected.wait.as_mut().unwrap().1 = target_1;',
           'expected.wait.as_mut().unwrap().1 = target_0;')
    reject('recalculated first LSI request', 'configure/calibrate_hsi.rs',
           'let first_lsi_request = admitted.first_lsi_request;',
           'let first_lsi_request = !r.lsi().read().stable();')
    reject('lost LSI tail return', 'configure/finish.rs',
           'return finish_lsi_sysclk(config, clocks, entry, trim, cs);',
           'let _ = finish_lsi_sysclk(config, clocks, entry, trim, cs);')
    reject('removed barrier', 'configure/flash_bridge.rs', 'barrier();', '')
    reject('cfg widening', 'configure/mod.rs',
           '#[cfg(all(rcc_cw32l011_v1, rcc_lsi_sysclk))]', '#[cfg(rcc_lsi_sysclk)]')
    reject('extra module', operation=lambda tree: tree.update({'extra.rs': ''}))
    reject('broadened visibility', 'configure/flash_bridge.rs',
           'pub(super) fn bridge', 'pub(crate) fn bridge')
    reject('duplicated item', operation=lambda tree: tree.update({'io.rs': tree['io.rs'] * 2}))
    reject('missing item', operation=lambda tree: tree.pop('io.rs'))
    return results


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--self-test', action='store_true')
    parser.add_argument('--write-original', type=Path)
    args = parser.parse_args()
    manifest = load_manifest()
    root = ROOT / manifest['root']
    assert not (ROOT / manifest['original_path']).exists(), 'Old module still present'
    actual = {str(p.relative_to(root)): p.read_text() for p in root.rglob('*.rs')}
    baseline = reconstruct(manifest, actual)
    if args.write_original:
        args.write_original.write_text(baseline)
    print('PASS: 11 complete modules invert to all 3,622 original lines, byte-for-byte')
    if args.self_test:
        results = mutation_tests(manifest, actual)
        print(f'PASS: {len(results)} control-flow/cfg/source mutation classes rejected')
        for result in results:
            print('  rejected:', result)


if __name__ == '__main__':
    main()
