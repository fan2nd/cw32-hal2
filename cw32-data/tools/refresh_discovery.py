#!/usr/bin/env python3
"""Observe current A030 discovery pages without accepting or repinning them.

Default: bounded HTTPS refresh into an ignored build/discovery-observations run.
--check-local: report retained snapshot status without network or writes.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import tempfile
import urllib.request

from acquire_evidence import ROOT, MANIFEST, OfficialRedirect, check_url, destination, load_manifest
from source_scope import discovery_only

MAX_BYTES = 64 * 1024
TIMEOUT_SECONDS = 30
OUTPUT_ROOT = ROOT / 'build/discovery-observations'


def identify(row, data):
    sha = hashlib.sha256(data).hexdigest()
    return {'sha256': sha, 'bytes': len(data),
            'status': 'unchanged' if sha == row['sha256'] and len(data) == row['bytes'] else 'changed'}


def observe(row, opener=None, local_root=None, output=None):
    result = {'id': row['id'], 'path': row['path'], 'url': row['url'],
              'reviewed_identity': {'sha256': row['sha256'], 'bytes': row['bytes']}}
    try:
        if local_root is not None:
            if local_root.is_symlink() or any(parent.is_symlink() for parent in local_root.parents):
                raise ValueError('Refusing symlinked discovery source root')
            path = destination(local_root, row['path'])
            if not path.exists():
                return {**result, 'status': 'missing'}
            with path.open('rb') as source:
                data = source.read(MAX_BYTES + 1)
        else:
            check_url(row['url'])
            with opener.open(row['url'], timeout=TIMEOUT_SECONDS) as response:
                check_url(response.geturl())
                result['response_url'] = response.geturl()
                data = response.read(MAX_BYTES + 1)
        if len(data) > MAX_BYTES:
            return {**result, 'status': 'unavailable', 'reason': f'Response exceeds {MAX_BYTES}-byte observation limit'}
        result.update(identify(row, data))
        if output is not None:
            # A unique run directory and exclusive file creation cannot clobber
            # reviewed sources or previous observations, even for equal hashes.
            candidate = output / (Path(row['path']).stem + '.' + result['sha256'] + '.html')
            with candidate.open('xb') as stream:
                stream.write(data)
            result['candidate'] = str(candidate)
    except (OSError, ValueError, RuntimeError) as error:
        result.update(status='unavailable', reason=str(error))
    return result


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check-local', type=Path, metavar='SOURCE_ROOT',
                        help='Report local discovery snapshot status; no network and no writes')
    args = parser.parse_args(argv)
    lock = load_manifest(MANIFEST)
    rows = [row for row in lock['artifacts'] if discovery_only(row)]
    output = None
    if args.check_local is None:
        # This location is always excluded by the source packager and gitignore.
        destination(ROOT, 'build/discovery-observations')
        OUTPUT_ROOT.mkdir(parents=True, exist_ok=True)
        output = Path(tempfile.mkdtemp(prefix='observation-', dir=OUTPUT_ROOT))
    opener = urllib.request.build_opener(OfficialRedirect())
    observations = [observe(row, opener, args.check_local, output) for row in rows]
    report = {'scope': 'discovery-only', 'observed_at_utc': datetime.now(timezone.utc).isoformat(),
              'mode': 'local-check' if args.check_local else 'network-refresh',
              'source_acceptance': False, 'hardware_verification': False,
              'observations': observations,
              'historical_claims': 'Unchanged. An observation never replaces a reviewed source or proves global SDK absence.'}
    if output is not None:
        with (output / 'receipt.json').open('x') as stream:
            stream.write(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))
    return 0 if all(row['status'] == 'unchanged' for row in observations) else 2


if __name__ == '__main__':
    raise SystemExit(main())
