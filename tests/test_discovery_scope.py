#!/usr/bin/env python3
"""Offline scope, receipt and non-mutating discovery observation checks."""
import contextlib
import copy
import hashlib
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import Mock, patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'cw32-data/tools'))
import acquire_evidence as a
import refresh_discovery as r
import source_provenance as p
from source_scope import DISCOVERY_URLS, discovery_only, scope_artifacts, scope_report, validate_scope


class Response(io.BytesIO):
    def __init__(self, data, url):
        super().__init__(data)
        self.url = url

    def geturl(self):
        return self.url


class DiscoveryScopeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.lock = a.load_manifest(a.MANIFEST)
        cls.pages = [row for row in cls.lock['artifacts'] if discovery_only(row)]

    def fixture(self, directory):
        lock = copy.deepcopy(self.lock)
        for row in lock['artifacts']:
            for spec in [row, *row.get('members', []), *([row['text']] if 'text' in row else [])]:
                data = ('fixture:' + spec['path']).encode()
                spec.update(sha256=hashlib.sha256(data).hexdigest(), bytes=len(data))
                if not discovery_only(row):
                    target = directory / spec['path']
                    target.parent.mkdir(parents=True, exist_ok=True)
                    target.write_bytes(data)
        for name, value in lock['generated_metadata'].items():
            (directory / name).write_text(json.dumps(value))
        return lock

    def test_exactly_two_discovery_records_and43_hardware_originals(self):
        self.assertEqual({row['path'] for row in self.pages}, set(DISCOVERY_URLS))
        self.assertEqual(len(scope_artifacts(self.lock)), 43)
        self.assertEqual(len(scope_artifacts(self.lock, True)), 45)
        self.assertEqual(sum(1 + len(row.get('members', [])) + ('text' in row)
                             for row in scope_artifacts(self.lock)) + len(self.lock['generated_metadata']), 553)
        self.assertFalse(any(discovery_only(row) for row in scope_artifacts(self.lock)))

    def test_other_artifacts_members_text_and_scope_identity_cannot_be_reclassified(self):
        for row in self.lock['artifacts']:
            for spec in [row, *row.get('members', []), *([row['text']] if 'text' in row else [])]:
                if spec['path'] in DISCOVERY_URLS:
                    continue
                with self.subTest(path=spec['path']), self.assertRaises(ValueError):
                    discovery_only({**spec, 'role': 'discovery-only'})
        for key, value in [('kind', 'pdf'), ('id', 'vendor:other'), ('url', 'https://www.whxy.com/other'), ('role', 'optional')]:
            with self.subTest(key=key), self.assertRaises(ValueError):
                discovery_only({**self.pages[0], key: value})
        lock = copy.deepcopy(self.lock)
        next(row for row in lock['artifacts'] if discovery_only(row)).pop('role')
        with self.assertRaisesRegex(ValueError, 'Exactly the two'):
            validate_scope(lock)

    def test_discovery_cannot_hide_nested_hardware_inputs(self):
        for child_key in ('members', 'text'):
            lock = copy.deepcopy(self.lock)
            row = next(row for row in lock['artifacts'] if discovery_only(row))
            child = {'path': 'hidden-hardware.h', 'sha256': '0' * 64, 'bytes': 1, 'members': ['Libraries/inc/hidden.h']}
            row[child_key] = [child] if child_key == 'members' else child
            with self.subTest(child=child_key), self.assertRaises(ValueError):
                validate_scope(lock)
            with self.assertRaises(p.ProvenanceError):
                p.validate_lock(ROOT, lock)
            with tempfile.TemporaryDirectory() as d:
                manifest = Path(d) / 'manifest.json'
                manifest.write_text(json.dumps(lock))
                with self.assertRaises(ValueError):
                    a.load_manifest(manifest)
        for child_key in ('members', 'text'):
            lock = copy.deepcopy(self.lock)
            parent = next(row for row in lock['artifacts'] if child_key in row)
            child = parent['members'][0] if child_key == 'members' else parent['text']
            child['role'] = 'discovery-only'
            with self.assertRaisesRegex(ValueError, 'members and PDF text'):
                validate_scope(lock)

    def test_refresh_collision_and_symlink_destination_never_overwrite(self):
        row = self.pages[0]
        data = b'observed candidate'
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            canonical = root / row['path']
            canonical.write_bytes(b'reviewed canonical bytes')
            candidate = root / (Path(row['path']).stem + '.' + hashlib.sha256(data).hexdigest() + '.html')
            candidate.write_bytes(b'previous observation')
            opener = Mock()
            opener.open.return_value = Response(data, row['url'])
            result = r.observe(row, opener=opener, output=root)
            self.assertEqual(result['status'], 'unavailable')
            self.assertEqual(canonical.read_bytes(), b'reviewed canonical bytes')
            self.assertEqual(candidate.read_bytes(), b'previous observation')
            (root / 'build').mkdir()
            (root / 'build/discovery-observations').symlink_to(root)
            with patch.object(r, 'ROOT', root), patch.object(r, 'OUTPUT_ROOT', root / 'build/discovery-observations'), self.assertRaises(a.EvidenceError):
                r.main([])

    def test_hardware_acquisition_receipt_excludes_missing_discovery_explicitly(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            lock = self.fixture(root)
            with patch.object(a, 'load_manifest', return_value=lock):
                output = io.StringIO()
                with contextlib.redirect_stdout(output):
                    a.main(['--source-root', d, '--verify'])
                report = json.loads(output.getvalue().splitlines()[-1])
                self.assertEqual(report['existing'], 43)
                self.assertEqual(report['outputs'], 553)
                self.assertEqual(report['required_originals'], 43)
                self.assertEqual(report['catalogued_originals'], 45)
                self.assertEqual(len(report['omitted_discovery']), 2)
                self.assertTrue(report['complete_hardware'])
                self.assertFalse(report['complete_manifest'])
                with self.assertRaises(a.EvidenceError), contextlib.redirect_stdout(io.StringIO()):
                    a.main(['--source-root', d, '--verify', '--include-discovery'])
                with self.assertRaisesRegex(a.EvidenceError, 'Discovery snapshots require'):
                    a.main(['--source-root', d, '--only', self.pages[0]['path']])
                for row in lock['artifacts']:
                    if discovery_only(row):
                        (root / row['path']).write_bytes(('fixture:' + row['path']).encode())
                output = io.StringIO()
                with contextlib.redirect_stdout(output):
                    a.main(['--source-root', d, '--verify', '--include-discovery'])
                report = json.loads(output.getvalue().splitlines()[-1])
                self.assertEqual(report['existing'], 45)
                self.assertEqual(report['outputs'], 555)
                self.assertEqual(report['omitted_discovery'], [])
                self.assertTrue(report['complete_manifest'])

    def test_provenance_scope_accepts_missing_or_changed_html_but_strict_replay_rejects(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            lock = self.fixture(root)
            p.validate_lock(ROOT, lock, root)
            with self.assertRaisesRegex(p.ProvenanceError, 'Missing acquired source'):
                p.validate_lock(ROOT, lock, root, include_discovery=True)
            for row in lock['artifacts']:
                if discovery_only(row):
                    (root / row['path']).write_bytes(b'unknown live page')
            p.validate_lock(ROOT, lock, root)
            with self.assertRaisesRegex(p.ProvenanceError, 'Acquired source mismatch'):
                p.validate_lock(ROOT, lock, root, include_discovery=True)

    def test_missing_and_changed_immutable_inputs_still_fail_in_both_consumers(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            lock = self.fixture(root)
            sdk = next(row for row in lock['artifacts'] if row['kind'] == 'sdk')
            pdf = next(row for row in lock['artifacts'] if row['kind'] == 'pdf' and 'text' in row)
            for spec in [sdk, sdk['members'][0], pdf, pdf['text']]:
                path = root / spec['path']
                data = path.read_bytes()
                for corruption in [None, b'!' + data[1:]]:
                    if corruption is None:
                        path.unlink()
                    else:
                        path.write_bytes(corruption)
                    with self.subTest(path=spec['path'], corruption=corruption), self.assertRaises(p.ProvenanceError):
                        p.validate_lock(ROOT, lock, root)
                    with patch.object(a, 'load_manifest', return_value=lock), contextlib.redirect_stdout(io.StringIO()), self.assertRaises(a.EvidenceError):
                        a.main(['--source-root', d, '--verify'])
                    path.write_bytes(data)

    def test_local_observation_reports_missing_changed_and_unchanged_without_writes(self):
        row = copy.deepcopy(self.pages[0])
        data = b'reviewed'
        row.update(sha256=hashlib.sha256(data).hexdigest(), bytes=len(data))
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            self.assertEqual(r.observe(row, local_root=root)['status'], 'missing')
            path = root / row['path']
            for content, expected in [(data, 'unchanged'), (b'changed!', 'changed')]:
                path.write_bytes(content)
                self.assertEqual(r.observe(row, local_root=root)['status'], expected)
                self.assertEqual(path.read_bytes(), content)
            self.assertEqual(list(root.iterdir()), [path])

    def test_network_observation_is_bounded_timed_and_never_accepts_changed_bytes(self):
        row = self.pages[0]
        for data, expected in [(b'unknown response', 'changed'), (b'x' * (r.MAX_BYTES + 1), 'unavailable')]:
            with tempfile.TemporaryDirectory() as d:
                opener = Mock()
                opener.open.return_value = Response(data, row['url'])
                result = r.observe(row, opener=opener, output=Path(d))
                self.assertEqual(result['status'], expected)
                opener.open.assert_called_once_with(row['url'], timeout=30)
                if expected == 'changed':
                    self.assertEqual(Path(result['candidate']).read_bytes(), data)
                else:
                    self.assertEqual(list(Path(d).iterdir()), [])
        opener = Mock()
        opener.open.side_effect = OSError('offline')
        self.assertEqual(r.observe(row, opener=opener)['status'], 'unavailable')

    def test_refresh_report_disclaims_acceptance_and_missing_is_nonzero(self):
        with tempfile.TemporaryDirectory() as d:
            output = io.StringIO()
            with contextlib.redirect_stdout(output):
                code = r.main(['--check-local', d])
            report = json.loads(output.getvalue())
            self.assertEqual(code, 2)
            self.assertEqual({x['status'] for x in report['observations']}, {'missing'})
            self.assertFalse(report['source_acceptance'])
            self.assertFalse(report['hardware_verification'])
            self.assertEqual(list(Path(d).iterdir()), [])

    def test_current_and_historical_html_remain_distribution_denied(self):
        # The guard's deny catalog is independent of hardware materialization scope.
        for original in self.pages:
            for historical in [False, True]:
                lock = copy.deepcopy(self.lock)
                row = next(item for item in lock['artifacts'] if item['id'] == original['id'])
                data = b'raw reviewed HTML fixture'
                digest = hashlib.sha256(data).hexdigest()
                if historical:
                    row['pin_history'].append({'sha256': digest, 'bytes': len(data)})
                else:
                    row.update(sha256=digest, bytes=len(data))
                with tempfile.TemporaryDirectory() as d:
                    root = Path(d)
                    (root / 'docs').mkdir()
                    (root / 'docs/upstream-file-provenance.json').write_text('{"files": []}')
                    path = root / 'renamed.bin'
                    path.write_bytes(data)
                    with self.assertRaisesRegex(p.ProvenanceError, 'Unlicensed external input'):
                        p.validate_distribution(root, [('renamed.bin', path)], lock)


if __name__ == '__main__':
    unittest.main(verbosity=2)
