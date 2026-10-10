#!/usr/bin/env python3
"""Negative and reproducibility checks for the provenance authority/views."""
import copy
import bz2
import gzip
import hashlib
import importlib.util
import io
import json
import lzma
from pathlib import Path
import shutil
import struct
import tarfile
import tempfile
import unittest
from unittest import mock
import zipfile
import zlib
import yaml

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('provenance', ROOT / 'cw32-data/tools/source_provenance.py')
p = importlib.util.module_from_spec(spec)
spec.loader.exec_module(p)

def archive_bytes(kind, members):
    stream = io.BytesIO()
    if kind == 'zip':
        with zipfile.ZipFile(stream, 'w', zipfile.ZIP_DEFLATED) as archive:
            for name, body in members:
                archive.writestr(name, body)
    else:
        with tarfile.open(fileobj=stream, mode='w') as archive:
            for name, body in members:
                member = tarfile.TarInfo(name)
                member.size = len(body)
                archive.addfile(member, io.BytesIO(body))
    return stream.getvalue()


class ProvenanceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.lock = json.loads((ROOT / p.LOCK).read_text())

    def test_current_views_are_reproducible(self):
        first = p.generate(ROOT)
        self.assertEqual(first, p.generate(ROOT))
        for rel, value in first.items():
            self.assertEqual((ROOT / rel).read_text(), value, rel)

    def test_missing_evidence_reference_fails(self):
        lock = copy.deepcopy(self.lock)
        lock['artifacts'][0]['evidence'].append('docs/does-not-exist.json')
        with self.assertRaisesRegex(p.ProvenanceError, 'Missing evidence reference'):
            p.validate_lock(ROOT, lock)

    def test_duplicate_output_fails(self):
        lock = copy.deepcopy(self.lock)
        lock['artifacts'].append(copy.deepcopy(lock['artifacts'][0]))
        with self.assertRaisesRegex(p.ProvenanceError, 'Duplicate source'):
            p.validate_lock(ROOT, lock)

    def test_missing_superseding_source_fails(self):
        lock = copy.deepcopy(self.lock)
        lock['artifacts'][0]['provenance']['superseded_by'] = 'vendor:missing.pdf'
        with self.assertRaisesRegex(p.ProvenanceError, 'Unresolved superseding'):
            p.validate_lock(ROOT, lock)

    def test_acquisition_date_requires_receipt(self):
        lock = copy.deepcopy(self.lock)
        lock['artifacts'][0]['provenance']['acquisition_evidence'] = None
        lock['artifacts'][0]['provenance']['acquired_date'] = '2026-10-08'
        with self.assertRaisesRegex(p.ProvenanceError, 'lacks download evidence'):
            p.validate_lock(ROOT, lock)

    def test_immutable_project_audit_hash_is_enforced(self):
        lock = copy.deepcopy(self.lock)
        lock['project_audit_inputs'][0]['sha256'] = '0' * 64
        with self.assertRaisesRegex(p.ProvenanceError, 'Immutable project audit mismatch'):
            p.validate_lock(ROOT, lock)

    def test_firmware_git_pin_is_covered(self):
        lock = copy.deepcopy(self.lock)
        lock['upstream']['embassy']['rev'] = '0' * 40
        with self.assertRaisesRegex(p.ProvenanceError, 'firmware/Cargo.lock Git revision'):
            p.validate_lock(ROOT, lock)

    def test_unpinned_upstream_rejected(self):
        lock = copy.deepcopy(self.lock)
        lock['upstream']['chiptool']['rev'] = 'main'
        with self.assertRaisesRegex(p.ProvenanceError, 'Unpinned upstream'):
            p.validate_lock(ROOT, lock)

    def test_dependency_revision_drift_rejected(self):
        lock = copy.deepcopy(self.lock)
        lock['upstream']['chiptool']['rev'] = '0' * 40
        with self.assertRaisesRegex(p.ProvenanceError, 'Cargo.lock Git revision'):
            p.validate_lock(ROOT, lock)

    def test_filename_is_not_printed_revision(self):
        a = next(a for a in self.lock['artifacts'] if a['path'] == 'CW32F020_DataSheet_CN_V1.3.pdf')
        self.assertEqual(a['provenance']['filename_version'], '1.3')
        self.assertEqual(a['provenance']['printed_revision'], '1.2')
        self.assertEqual(a['provenance']['revision_history_date'], '2023-02-14')
        current = next(x for x in self.lock['artifacts'] if x['id'] == a['provenance']['superseded_by'])
        self.assertNotEqual(a['sha256'], current['sha256'])
        self.assertEqual(current['provenance']['printed_revision'], '1.3')

    def test_pll_receipt_uses_selected_own_sources_and_survives_packaging(self):
        receipt_path = 'sources/x030-f020-hsi-pll-source-receipt.json'
        receipt = json.loads((ROOT / receipt_path).read_text())
        authority = {a['id']: a for a in self.lock['artifacts']}
        for source in receipt['source_identities']:
            original = authority[source['id']]
            self.assertEqual(source['sha256'], original['sha256'])
            self.assertEqual(source['path'], original['path'])
            self.assertEqual(source['printed_revision'], original['provenance']['printed_revision'])
            self.assertEqual(source['chip_scope'], original['provenance']['chip_scope'])
            self.assertEqual(original['provenance']['status'], 'selected')
            self.assertIn(receipt_path, original['evidence'])
        f020 = receipt['profiles']['CW32F020']
        self.assertEqual(f020['datasheet_source'], 'vendor:current-datasheets/CW32F020_DataSheet_CN_V1.3.pdf')
        self.assertEqual(f020['pll_datasheet_output_hz'], [8000000, 48000000])
        package_spec = importlib.util.spec_from_file_location('package_source', ROOT / 'ci/package-source.py')
        package = importlib.util.module_from_spec(package_spec)
        package_spec.loader.exec_module(package)
        required = package.required_evidence(ROOT)
        self.assertIn(receipt_path, required)
        self.assertTrue(package.include_file(Path(receipt_path), required))
        self.assertFalse(package.include_file(Path('sources/unreviewed-receipt.json'), required))

    def test_cover_history_and_acquisition_are_distinct(self):
        a = next(a for a in self.lock['artifacts'] if a['path'] == 'CW32L011_UserManual_CN_V1.1.pdf')
        self.assertEqual(a['provenance']['cover_date'], '2026-06')
        self.assertEqual(a['provenance']['revision_history_date'], '2025-09-19')
        self.assertIsNone(a['provenance']['acquired_date'])
        self.assertNotEqual(a['sha256'], a['provenance']['previous_snapshot']['sha256'])

    def test_wrong_external_reference_fails(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / 'docs').mkdir()
            (root / 'docs/broken.json').write_text(json.dumps({'file': 'manual.pdf', 'sha256': '0' * 64}))
            with self.assertRaisesRegex(p.ProvenanceError, 'Referenced external file hash absent'):
                p.reference_index(root, self.lock, p.source_nodes(self.lock))

    def test_explicit_unresolved_reference_fails(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / 'docs').mkdir()
            (root / 'docs/broken.json').write_text(json.dumps({'source_ref': 'vendor:missing.pdf'}))
            with self.assertRaisesRegex(p.ProvenanceError, 'Unknown explicit source_ref'):
                p.reference_index(root, self.lock, p.source_nodes(self.lock))

    def yaml_reference_index(self, source):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            for relative in [p.LOCK, 'Cargo.lock', 'firmware/Cargo.lock', 'requirements-dev.txt',
                             'docs/toolchain.json', 'docs/upstream-file-provenance.json']:
                target = root / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(ROOT / relative, target)
            path = root / 'cw32-data/inputs/fixture.yaml'
            path.parent.mkdir(parents=True)
            path.write_text(yaml.safe_dump({'source': source}))
            # No generated provenance, coverage or PAC files exist in this fixture.
            return p.reference_index(root, self.lock, p.source_nodes(self.lock))

    def test_authored_yaml_ids_index_without_generated_views(self):
        artifact = self.lock['artifacts'][0]
        index = self.yaml_reference_index({'source_ref': artifact['id'],
                                          'sha256': artifact['sha256'], 'url': artifact['url']})
        document = next(row for row in index['evidence_documents']
                        if row['path'] == 'cw32-data/inputs/fixture.yaml')
        self.assertIn({'pointer': '/source/source_ref', 'source_ids': [artifact['id']],
                       'citation_context_pointer': '/source'}, document['references'])

    def test_authored_yaml_reference_rejects_mismatched_hash(self):
        artifact = self.lock['artifacts'][0]
        with self.assertRaisesRegex(p.ProvenanceError, 'Source reference/hash mismatch'):
            self.yaml_reference_index({'source_ref': artifact['id'], 'sha256': '0' * 64})

    def test_authored_yaml_reference_rejects_mismatched_url(self):
        artifact = self.lock['artifacts'][0]
        with self.assertRaisesRegex(p.ProvenanceError, 'Source reference/URL mismatch'):
            self.yaml_reference_index({'source_ref': artifact['id'], 'url': 'https://example.com/other'})

    def test_source_ref_enrichment_requires_unique_vendor_identity(self):
        artifact = self.lock['artifacts'][0]
        data = {'source': {'sha256': artifact['sha256']},
                'project_evidence': {'sha256': '0' * 64}}
        p.enrich_source_refs(data, self.lock)
        self.assertEqual(data['source']['source_ref'], artifact['id'])
        self.assertNotIn('source_ref', data['project_evidence'])
        self.assertEqual(p.enrich_source_refs(copy.deepcopy(data), self.lock), data)
        ambiguous = copy.deepcopy(self.lock)
        duplicate = copy.deepcopy(artifact)
        duplicate['id'] = 'vendor:second-pinned-origin'
        duplicate['path'] = 'second-pinned-origin.zip'
        ambiguous['artifacts'].append(duplicate)
        value = {'sha256': artifact['sha256']}
        p.enrich_source_refs(value, ambiguous)
        self.assertNotIn('source_ref', value)

    def test_renamed_vendor_file_cannot_be_packaged(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / 'docs').mkdir()
            (root / 'docs/upstream-file-provenance.json').write_text('{"files": []}')
            path = root / 'innocent.bin'
            path.write_bytes(b'raw vendor fixture')
            lock = copy.deepcopy(self.lock)
            lock['artifacts'][0]['sha256'] = hashlib.sha256(path.read_bytes()).hexdigest()
            with self.assertRaisesRegex(p.ProvenanceError, 'Unlicensed external input'):
                p.validate_distribution(root, [('innocent.bin', path)], lock)

    def approved_sdk_fixture(self, root):
        (root / 'docs').mkdir()
        (root / 'docs/upstream-file-provenance.json').write_text('{"files": []}')
        shutil.copytree(ROOT / p.APPROVED_SDK_ROOT, root / p.APPROVED_SDK_ROOT)
        return [(name, root / name) for name in sorted(set(p.approved_sdk_members(self.lock)) | p.APPROVED_SDK_DOCS)]

    def refresh_sdk_checksums(self, root, files):
        # Updating a delivery checksum must not bypass the source/notice checks.
        (root / p.APPROVED_SDK_ROOT / 'SHA256SUMS').write_text(''.join(
            f'{p.digest(path)}  {path.relative_to(root / p.APPROVED_SDK_ROOT)}\n'
            for _, path in files if path.name != 'SHA256SUMS'))

    def test_exact_licensed_sdk_subset_is_allowed(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            p.validate_distribution(root, self.approved_sdk_fixture(root), self.lock)

    def test_incomplete_or_extra_licensed_subset_is_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            files = self.approved_sdk_fixture(root)
            for name, _ in files:
                with self.subTest(missing=name), self.assertRaisesRegex(p.ProvenanceError, 'exactly 11 headers'):
                    p.validate_distribution(root, [(n, path) for n, path in files if n != name], self.lock)
            extra = root / p.APPROVED_SDK_ROOT / 'extra.h'
            extra.write_text('unreviewed input')
            with self.assertRaisesRegex(p.ProvenanceError, 'exactly 11 headers'):
                p.validate_distribution(root, files + [(str(extra.relative_to(root)), extra)], self.lock)

    def test_approved_header_tampering_rejected_after_checksum_refresh(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            files = self.approved_sdk_fixture(root)
            name = next(iter(p.approved_sdk_members(self.lock)))
            path = root / name
            path.write_bytes(path.read_bytes() + b'changed')
            self.refresh_sdk_checksums(root, files)
            with self.assertRaisesRegex(p.ProvenanceError, 'source-lock mismatch'):
                p.validate_distribution(root, files, self.lock)

    def test_customized_apache_license_is_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            files = self.approved_sdk_fixture(root)
            (root / p.APPROVED_SDK_ROOT / 'LICENSE-APACHE-2.0.txt').write_text('custom license')
            self.refresh_sdk_checksums(root, files)
            with self.assertRaisesRegex(p.ProvenanceError, 'unmodified official Apache'):
                p.validate_distribution(root, files, self.lock)

    def test_missing_apache_notice_is_rejected_even_with_repin(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            files = self.approved_sdk_fixture(root)
            lock = copy.deepcopy(self.lock)
            name, member = next(iter(p.approved_sdk_members(lock).items()))
            path = root / name
            path.write_bytes(path.read_bytes().replace(b'SPDX-License-Identifier: Apache-2.0', b'removed file license notice'))
            member.update(bytes=path.stat().st_size, sha256=p.digest(path))
            self.refresh_sdk_checksums(root, files)
            with self.assertRaisesRegex(p.ProvenanceError, 'Apache notice missing'):
                p.validate_distribution(root, files, lock)

    def test_renamed_or_nested_approved_header_stays_prohibited(self):
        name = next(iter(p.approved_sdk_members(self.lock)))
        data = (ROOT / name).read_bytes()
        for payload in [data, archive_bytes('zip', [('renamed.h', data)])]:
            with self.subTest(nested=payload != data), self.assertRaisesRegex(p.ProvenanceError, 'Unlicensed external input'):
                self.validate_archive_fixtures([payload])

    def test_duplicate_approved_delivery_path_is_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            files = self.approved_sdk_fixture(root)
            with self.assertRaisesRegex(p.ProvenanceError, 'Duplicate distribution delivery path'):
                p.validate_distribution(root, files + [files[-1]], self.lock)

    def test_unresolved_upstream_file_blocks_packaging(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / 'docs').mkdir()
            (root / 'docs/upstream-file-provenance.json').write_text(json.dumps({'files': [
                {'project_path': 'unknown.rs', 'distribution_state': 'unresolved',
                 'observed_project_sha256': '0' * 64}]}))
            with self.assertRaisesRegex(p.ProvenanceError, 'Unresolved upstream redistribution'):
                p.validate_distribution(root, [('unknown.rs', root / 'unknown.rs')], self.lock)

    def test_renamed_historical_upstream_body_blocks_packaging(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / 'docs').mkdir()
            path = root / 'renamed.rs'
            path.write_bytes(b'historical upstream fixture')
            (root / 'docs/upstream-file-provenance.json').write_text(json.dumps({'files': [
                {'project_path': 'excluded.rs', 'distribution_state': 'excluded_historical_unresolved',
                 'observed_project_sha256': p.digest(path)}]}))
            with self.assertRaisesRegex(p.ProvenanceError, 'Unlicensed external input'):
                p.validate_distribution(root, [('renamed.rs', path)], self.lock)

    def test_replacement_requires_review_and_exact_hash(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / 'docs').mkdir()
            path = root / 'replacement.rs'
            path.write_bytes(b'independently authored fixture')
            row = {'project_path': 'replacement.rs', 'distribution_state': 'reviewed_independent_replacement',
                   'observed_project_sha256': '0' * 64, 'replacement_review': {}}
            manifest = root / 'docs/upstream-file-provenance.json'
            manifest.write_text(json.dumps({'files': [row]}))
            with self.assertRaisesRegex(p.ProvenanceError, 'Independent replacement review missing'):
                p.validate_distribution(root, [('replacement.rs', path)], self.lock)
            (root / 'docs/review.json').write_text('{}')
            row['replacement_review'] = {'independent_review_passed': True,
                'review_reference': 'docs/review.json', 'verification_reference': 'docs/review.json',
                'replacement_sha256': '1' * 64}
            manifest.write_text(json.dumps({'files': [row]}))
            with self.assertRaisesRegex(p.ProvenanceError, 'Reviewed replacement hash changed'):
                p.validate_distribution(root, [('replacement.rs', path)], self.lock)
            row['replacement_review']['replacement_sha256'] = p.digest(path)
            manifest.write_text(json.dumps({'files': [row]}))
            p.validate_distribution(root, [('replacement.rs', path)], self.lock)

    def validate_archive_fixtures(self, payloads, prohibited=b'known predecessor fixture', review=None):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / 'docs').mkdir()
            if review is None:
                review = {'files': [{'project_path': 'excluded.rs',
                    'distribution_state': 'excluded_historical_unresolved',
                    'observed_project_sha256': hashlib.sha256(prohibited).hexdigest()}]}
            (root / 'docs/upstream-file-provenance.json').write_text(json.dumps(review))
            files = []
            for i, payload in enumerate(payloads):
                # Every input and inner member deliberately has an unrelated name.
                path = root / f'ordinary-{i}.data'
                path.write_bytes(payload)
                files.append((path.name, path))
            p.validate_distribution(root, iter(files), self.lock)
            self.assertFalse((root / 'escaped.rs').exists())

    def test_renamed_nested_predecessor_archives_rejected(self):
        body = b'known predecessor fixture'
        for kind in ['zip', 'tar']:
            for wrap in [lambda x: x, gzip.compress, bz2.compress, lzma.compress]:
                with self.subTest(kind=kind, wrapper=wrap):
                    inner = wrap(archive_bytes(kind, [('../../escaped.rs', body)]))
                    outer = archive_bytes('zip', [('notes.dat', inner)])
                    with self.assertRaisesRegex(p.ProvenanceError, 'Unlicensed external input.*notes.dat'):
                        self.validate_archive_fixtures([outer])

    def test_direct_compressed_snapshot_identity_rejected(self):
        snapshot = gzip.compress(archive_bytes('tar', [('previous.rs', b'snapshot body')]))
        review = {'files': [], 'historical_archives': [{
            'archive_path': 'excluded.tar.gz',
            'archive_sha256': hashlib.sha256(snapshot).hexdigest(),
            'archive_bytes': len(snapshot),
            'distribution_state': 'excluded_historical_unresolved',
            'members': []}]}
        with self.assertRaisesRegex(p.ProvenanceError, 'Unlicensed external input'):
            self.validate_archive_fixtures([snapshot], review=review)

    def test_archived_member_identity_rejected_after_repacking(self):
        body = b'archived predecessor different from live ancestry record'
        review = {'files': [], 'historical_archives': [{
            'archive_path': 'excluded.tar.gz', 'archive_sha256': '0' * 64,
            'archive_bytes': 123, 'distribution_state': 'excluded_historical_unresolved',
            'members': [{'member_path': 'previous.rs', 'sha256': hashlib.sha256(body).hexdigest(),
                         'bytes': len(body), 'distribution_state': 'excluded_historical_unresolved'}]}]}
        repacked = archive_bytes('zip', [('renamed', lzma.compress(body))])
        with self.assertRaisesRegex(p.ProvenanceError, 'Unlicensed external input'):
            self.validate_archive_fixtures([repacked], review=review)

    def test_compressed_raw_and_nested_vendor_bytes_rejected(self):
        body = b'known predecessor fixture'
        for compress in [gzip.compress, bz2.compress, lzma.compress]:
            with self.subTest(compression=compress):
                with self.assertRaisesRegex(p.ProvenanceError, 'Unlicensed external input'):
                    self.validate_archive_fixtures([compress(body)])
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / 'docs').mkdir()
            (root / 'docs/upstream-file-provenance.json').write_text('{"files": []}')
            path = root / 'renamed.data'
            path.write_bytes(archive_bytes('zip', [('innocent', gzip.compress(body))]))
            lock = copy.deepcopy(self.lock)
            lock['artifacts'][0]['sha256'] = hashlib.sha256(body).hexdigest()
            with self.assertRaisesRegex(p.ProvenanceError, 'Unlicensed external input.*innocent'):
                p.validate_distribution(root, [('renamed.data', path)], lock)

    def test_permitted_source_and_own_archives_accepted(self):
        body = b'fn independently_authored() {}\n'
        payloads = [body]
        for kind in ['zip', 'tar']:
            archive = archive_bytes(kind, [('source.rs', body)])
            payloads.extend([archive, gzip.compress(archive), bz2.compress(archive), lzma.compress(archive)])
        payloads.append(gzip.compress(archive_bytes('zip', [('nested', bz2.compress(payloads[2]))])))
        self.validate_archive_fixtures(payloads)

    def test_concatenated_compression_streams_inspected(self):
        body = b'known predecessor fixture'
        for compress in [gzip.compress, bz2.compress, lzma.compress]:
            with self.subTest(compression=compress):
                with self.assertRaisesRegex(p.ProvenanceError, 'Unlicensed external input'):
                    self.validate_archive_fixtures([compress(b'permitted') + compress(body)])
                with self.assertRaisesRegex(p.ProvenanceError, 'Unlicensed external input'):
                    self.validate_archive_fixtures([compress(body[:10]) + compress(body[10:])])

    def test_archive_recursion_limit_fails_closed(self):
        body = b'own source'
        with mock.patch.object(p, 'MAX_ARCHIVE_DEPTH', 2):
            self.validate_archive_fixtures([gzip.compress(gzip.compress(body))])
            with self.assertRaisesRegex(p.ProvenanceError, 'depth budget exceeded'):
                self.validate_archive_fixtures([gzip.compress(gzip.compress(gzip.compress(body)))])

    def test_archive_entry_limit_and_shared_budget_fail_closed(self):
        with mock.patch.object(p, 'MAX_ARCHIVE_ENTRIES', 1):
            for kind in ['zip', 'tar']:
                with self.subTest(kind=kind):
                    with self.assertRaisesRegex(p.ProvenanceError, 'entry budget exceeded'):
                        self.validate_archive_fixtures([archive_bytes(kind, [('a', b'a'), ('b', b'b')])])
            one = archive_bytes('zip', [('source', b'own')])
            with self.assertRaisesRegex(p.ProvenanceError, 'entry budget exceeded'):
                self.validate_archive_fixtures([one, one])

    def test_archive_expansion_limits_fail_closed(self):
        for name in ['MAX_MEMBER_BYTES', 'MAX_DECOMPRESSED_BYTES']:
            with self.subTest(limit=name), mock.patch.object(p, name, 100):
                with self.assertRaisesRegex(p.ProvenanceError, 'byte budget exceeded'):
                    self.validate_archive_fixtures([gzip.compress(b'a' * 101)])
        with mock.patch.object(p, 'MAX_DECOMPRESSED_BYTES', 100):
            with self.assertRaisesRegex(p.ProvenanceError, 'byte budget exceeded'):
                self.validate_archive_fixtures([gzip.compress(b'a' * 60), gzip.compress(b'b' * 60)])

    def test_tar_metadata_headers_consume_entry_budget(self):
        stream = io.BytesIO()
        with tarfile.open(fileobj=stream, mode='w') as archive:
            member = tarfile.TarInfo('source')
            member.pax_headers = {'comment': 'project-authored fixture'}
            archive.addfile(member, io.BytesIO(b''))
        self.validate_archive_fixtures([stream.getvalue()])
        with mock.patch.object(p, 'MAX_ARCHIVE_ENTRIES', 1):
            with self.assertRaisesRegex(p.ProvenanceError, 'entry budget exceeded'):
                self.validate_archive_fixtures([stream.getvalue()])

    def test_tar_unknown_typeflag_body_is_inspected(self):
        body = b'known predecessor fixture'
        stream = io.BytesIO()
        with tarfile.open(fileobj=stream, mode='w') as archive:
            member = tarfile.TarInfo('source')
            member.type = b'Z'
            member.size = len(body)
            archive.addfile(member, io.BytesIO(body))
        with self.assertRaisesRegex(p.ProvenanceError, 'Unlicensed external input'):
            self.validate_archive_fixtures([stream.getvalue()])

    def test_concatenated_and_unreferenced_zip_records_fail_closed(self):
        first = archive_bytes('zip', [('hidden', b'known predecessor fixture')])
        second = archive_bytes('zip', [('visible', b'own source')])
        # A normal concatenation and one whose directory offsets were adjusted.
        adjusted = bytearray(second)
        central = adjusted.index(b'PK\x01\x02')
        end = adjusted.rindex(b'PK\x05\x06')
        struct.pack_into('<L', adjusted, central + 42, len(first))
        struct.pack_into('<L', adjusted, end + 16, len(first) + central)
        for payload in [first + second, first + adjusted]:
            with self.assertRaisesRegex(p.ProvenanceError, 'ZIP'):
                self.validate_archive_fixtures([payload])

    def test_zip_data_descriptors_and_small_zip64_local_headers_accepted(self):
        class NonSeekable(io.BytesIO):
            def seek(self, *args):
                raise OSError('stream does not seek')

        for force_zip64 in [False, True]:
            stream = NonSeekable()
            with zipfile.ZipFile(stream, 'w', zipfile.ZIP_DEFLATED) as archive:
                with archive.open('own.rs', 'w', force_zip64=force_zip64) as member:
                    member.write(b'project-authored source')
            self.validate_archive_fixtures([stream.getvalue()])

    def test_forged_zip_size_cannot_hide_expanded_content(self):
        for compression in [zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED, zipfile.ZIP_BZIP2]:
            with self.subTest(compression=compression):
                stream = io.BytesIO()
                with zipfile.ZipFile(stream, 'w', compression) as archive:
                    archive.writestr('source', b'A' * 1000)
                payload = bytearray(stream.getvalue())
                central = payload.index(b'PK\x01\x02')
                struct.pack_into('<L', payload, 14, zlib.crc32(b'A'))
                struct.pack_into('<L', payload, 22, 1)
                struct.pack_into('<L', payload, central + 16, zlib.crc32(b'A'))
                struct.pack_into('<L', payload, central + 24, 1)
                with mock.patch.object(p, 'MAX_DECOMPRESSED_BYTES', 100):
                    with self.assertRaisesRegex(p.ProvenanceError, 'byte budget exceeded'):
                        self.validate_archive_fixtures([payload])

    def test_permitted_zip_compression_methods_accepted(self):
        for compression in [zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED, zipfile.ZIP_BZIP2]:
            stream = io.BytesIO()
            with zipfile.ZipFile(stream, 'w', compression) as archive:
                archive.writestr('source', b'own source')
            self.validate_archive_fixtures([stream.getvalue()])

    def test_tar_hidden_metadata_consumes_shared_byte_budget(self):
        stream = io.BytesIO()
        with tarfile.open(fileobj=stream, mode='w') as archive:
            for i in range(3):
                member = tarfile.TarInfo(f'own-{i}')
                member.pax_headers = {'comment': 'x' * 60}
                archive.addfile(member, io.BytesIO(b''))
        with mock.patch.object(p, 'MAX_DECOMPRESSED_BYTES', 100):
            with self.assertRaisesRegex(p.ProvenanceError, 'byte budget exceeded'):
                self.validate_archive_fixtures([stream.getvalue()])

    def test_sparse_tar_is_rejected_before_parsing(self):
        for old_gnu in [True, False]:
            stream = io.BytesIO()
            with tarfile.open(fileobj=stream, mode='w') as archive:
                member = tarfile.TarInfo('own')
                if old_gnu:
                    member.type = tarfile.GNUTYPE_SPARSE
                else:
                    member.pax_headers = {'GNU.sparse.map': '0,1'}
                archive.addfile(member, io.BytesIO(b''))
            with self.assertRaisesRegex(p.ProvenanceError, 'Unsupported sparse TAR'):
                self.validate_archive_fixtures([stream.getvalue()])

    def test_malformed_recognized_archives_fail_closed(self):
        own = archive_bytes('tar', [('source', b'own source')])
        for payload in [b'PK\x03\x04broken', b'\x1f\x8bbroken', b'BZhbroken',
                        b'\xfd7zXZ\x00broken', own + b'uninspectable trailing data']:
            with self.subTest(prefix=payload[:8]):
                with self.assertRaises(p.ProvenanceError):
                    self.validate_archive_fixtures([payload])

    def test_missing_acquired_source_is_not_success(self):
        with tempfile.TemporaryDirectory() as d:
            with self.assertRaisesRegex(p.ProvenanceError, 'Missing acquired source'):
                p.validate_lock(ROOT, self.lock, Path(d))

    def test_write_views_cannot_change_source_pins(self):
        before = hashlib.sha256((ROOT / p.LOCK).read_bytes()).hexdigest()
        p.generate(ROOT)
        self.assertEqual(before, hashlib.sha256((ROOT / p.LOCK).read_bytes()).hexdigest())

if __name__ == '__main__':
    unittest.main(verbosity=2)
