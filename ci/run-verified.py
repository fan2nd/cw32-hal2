#!/usr/bin/env python3
"""Run a validation command with before/after source hashes and a persisted log.

Usage: python3 ci/run-verified.py --output docs/verification-logs/stage6/hal -- ./ci/check-hal.sh
The result fails if any packaged input changes during the command. Reports under
verification-logs are excluded to allow independent validation logs to coexist.
"""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'cw32-data/tools'))
from source_provenance import APPROVED_SDK_DOCS, approved_sdk_members

EXCLUDED_ROOTS = {'.cargo', '.rustup', 'sources', 'build', '.git'}
EXCLUDED_COMPONENTS = {'target', '__pycache__', '.git', 'verification-logs'}
SOURCE_MANIFESTS = {'sources/catalog.json', 'sources/evidence-sources.json', 'sources/layout-history.json',
                    'sources/README.md', 'sources/SOURCES.md', 'sources/REFERENCE-PACKAGE.md'} | APPROVED_SDK_DOCS
SOURCE_MANIFESTS |= set(approved_sdk_members(json.loads((ROOT / 'sources/evidence-sources.json').read_text())))


def snapshot(scope):
    result = []
    for path in sorted(ROOT.rglob('*')):
        rel = path.relative_to(ROOT)
        if not path.is_file() or path.is_symlink():
            continue
        if (rel.parts[0] in EXCLUDED_ROOTS and str(rel) not in SOURCE_MANIFESTS) or any(p in EXCLUDED_COMPONENTS for p in rel.parts):
            continue
        if path.suffix in {'.pyc', '.zip'}:
            continue
        if scope == 'hal-matrix' and str(rel) not in SOURCE_MANIFESTS and not (rel.parts[0] in {'embassy-cw32', 'cw32-metapac', 'cw32-data', 'firmware', 'examples', 'ci'} or str(rel) in {'Cargo.toml', 'Cargo.lock', 'ci/check-hal.sh', 'ci/run-verified.py', 'ci/summarize-hal-matrix.py', 'tests/test_module_layout.py'}):
            continue
        result.append({'path': str(rel), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest(),
                       'executable': bool(path.stat().st_mode & 0o111)})
    return result


def save(path, value):
    raw = (json.dumps(value, indent=2, sort_keys=True) + '\n').encode()
    path.write_bytes(raw)
    return hashlib.sha256(raw).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--scope', choices=['all', 'hal-matrix'], default='all')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('command', nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ['--'] else args.command
    if not command:
        parser.error('a command is required')
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    if not output.is_relative_to(ROOT / 'docs/verification-logs'):
        parser.error('output must be below docs/verification-logs so reports are outside the input snapshot')
    before = snapshot(args.scope)
    before_hash = save(output / 'source-before.json', before)
    started = datetime.datetime.now(datetime.timezone.utc).isoformat()
    tick = time.monotonic()
    env = os.environ.copy()
    env['CARGO_INCREMENTAL'] = '0'
    with (output / 'run.log').open('wb') as log:
        process = subprocess.Popen(command, cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT)
        print(f'PID {process.pid}; log {output / "run.log"}', flush=True)
        result = process.wait()
    after = snapshot(args.scope)
    after_hash = save(output / 'source-after.json', after)
    summary = {'input_scope': args.scope, 'command': command, 'start_utc': started,
               'end_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
               'duration_seconds': round(time.monotonic() - tick, 3), 'exit_code': result,
               'source_files': len(before), 'source_unchanged': before == after,
               'source_before_sha256': before_hash, 'source_after_sha256': after_hash,
               'log_sha256': hashlib.sha256((output / 'run.log').read_bytes()).hexdigest(),
               'status': 'passed' if result == 0 and before == after else 'failed'}
    save(output / 'summary.json', summary)
    print(json.dumps(summary, indent=2), flush=True)
    raise SystemExit(0 if summary['status'] == 'passed' else 1)


if __name__ == '__main__':
    main()
