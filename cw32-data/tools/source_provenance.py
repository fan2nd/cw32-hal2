#!/usr/bin/env python3
"""Validate canonical source pins and generate deterministic provenance views.

Offline by default. --write updates only derived views, never external source
pins, authored YAML or evidence claims. --sources additionally verifies bytes.
"""
from __future__ import annotations
import argparse
import bz2
import fnmatch
import hashlib
import io
import json
import lzma
from pathlib import Path
import re
import struct
import sys
import tarfile
import tomllib
from urllib.parse import urlsplit
import zipfile
import zlib
import yaml

# Support both script execution and importlib-based source tests.
sys.path.insert(0, str(Path(__file__).resolve().parent))
from source_scope import discovery_only, scope_report, validate_scope

ROOT = Path(__file__).resolve().parents[2]
LOCK = 'sources/evidence-sources.json'
APPROVED_SDK_ROOT = 'sources/approved-sdk-members'
APPROVED_SDK_DOCS = {f'{APPROVED_SDK_ROOT}/{name}' for name in
                     ['README.md', 'LICENSE-APACHE-2.0.txt', 'SHA256SUMS']}
# File-level Apache notices were inspected for only these 11 pinned members.
# Paths, sizes and hashes still come from the single source lock, not a new catalog.
APPROVED_SDK_MEMBER_IDS = {
    'member:cw32f003/Libraries/inc/cw32f003.h',
    'member:cw32f020/Libraries/inc/cw32f020.h',
    'member:cw32f030/Libraries/inc/cw32f030.h',
    'member:cw32l010/CW32L010_StandardPeripheralLib_V1.0.9/Libraries/inc/cw32l010.h',
    'member:cw32l011/Libraries/inc/cw32l011.h',
    'member:cw32l012/Libraries/inc/cw32l012.h',
    'member:cw32l031/Libraries/inc/cw32l031.h',
    'member:cw32l052/CW32L052_StandardPeripheralLib_V1.4/Libraries/inc/cw32l052.h',
    'member:cw32l083/Libraries/inc/cw32l083.h',
    'member:cw32r031/Libraries/inc/cw32r031.h',
    'member:cw32w031/Libraries/inc/cw32w031.h',
}
VIEWS = {'build/provenance/source-lock.json', 'build/provenance/vendor-sources.json',
         'build/provenance/reference-index.json', 'build/provenance/SOURCE-CATALOG.md'}
REPORT_PATTERNS = ('*coverage*.json', '*verification*.json', '*report*.json',
                   '*manifest*.json', '*migration*.json', '*refactor*.json',
                   '*equivalence*.json', '*layout.json', '*review.json', '*rebase.json')
SHA = re.compile(r'^[0-9a-f]{64}$')
# A shared budget covers the entire distribution, including every nested layer.
MAX_ARCHIVE_DEPTH = 8
MAX_ARCHIVE_ENTRIES = 10000
MAX_DECOMPRESSED_BYTES = 256 * 1024 * 1024
MAX_MEMBER_BYTES = 64 * 1024 * 1024

class ProvenanceError(ValueError):
    pass

def require(condition, message):
    if not condition:
        raise ProvenanceError(message)

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def encode(value):
    return json.dumps(value, indent=2, ensure_ascii=False) + '\n'

def pointer_part(value):
    return str(value).replace('~', '~0').replace('/', '~1')

def walk(value, pointer=''):
    yield pointer, value
    if isinstance(value, dict):
        for key, item in value.items():
            yield from walk(item, pointer + '/' + pointer_part(key))
    elif isinstance(value, list):
        for i, item in enumerate(value):
            yield from walk(item, pointer + '/' + str(i))

def safe_path(root, relative):
    require(isinstance(relative, str) and relative and '\\' not in relative,
            f'Invalid relative path: {relative}')
    path = Path(relative)
    require(not path.is_absolute() and '..' not in path.parts, f'Unsafe path: {relative}')
    target = root / path
    require(not any(x.is_symlink() for x in [target, *target.parents] if x != root.parent), f'Symlinked path: {relative}')
    require(not target.is_symlink(), f'Symlink is not evidence: {relative}')
    return target

def source_nodes(lock):
    nodes = []
    for i, row in enumerate(lock['artifacts']):
        base = f'/artifacts/{i}'
        nodes.append({'id': row['id'], 'path': row['path'], 'sha256': row['sha256'],
                      'bytes': row['bytes'], 'kind': row['kind'], 'lock_pointer': base,
                      'original_id': row['id'], 'url': row['url'],
                      'provenance': row['provenance'], 'license': row['license'],
                      **({'role': row['role']} if 'role' in row else {})})
        for j, member in enumerate(row.get('members', [])):
            nodes.append({'id': 'member:' + member['path'], 'path': member['path'],
                          'sha256': member['sha256'], 'bytes': member['bytes'], 'kind': 'sdk-member',
                          'lock_pointer': base + f'/members/{j}', 'original_id': row['id'],
                          'archive_member_chain': member['members']})
        if 'text' in row:
            text = row['text']
            nodes.append({'id': 'text:' + text['path'], 'path': text['path'],
                          'sha256': text['sha256'], 'bytes': text['bytes'], 'kind': 'pdf-text',
                          'lock_pointer': base + '/text', 'original_id': row['id'],
                          'transform_ref': LOCK + '#/text_extractor'})
    return nodes

def approved_sdk_members(lock):
    """Resolve the inspected headers to their exact original archive paths."""
    result, found = {}, set()
    for artifact in lock['artifacts']:
        for member in artifact.get('members', []):
            source_id = 'member:' + member['path']
            if source_id in APPROVED_SDK_MEMBER_IDS:
                require(artifact['kind'] == 'sdk' and len(member['members']) == 1,
                        f'Approved SDK member origin changed: {source_id}')
                relative = f"{APPROVED_SDK_ROOT}/sdk-members/{Path(artifact['path']).stem}/{member['members'][0]}"
                require(source_id not in found and relative not in result,
                        f'Duplicate approved SDK member: {source_id}')
                result[relative] = member
                found.add(source_id)
    require(found == APPROVED_SDK_MEMBER_IDS, 'Approved SDK member missing from source lock')
    return result

def validate_approved_sdk_members(root, files, lock):
    """Allow the unchanged licensed subset only at its delivery paths, with notices."""
    included = {str(rel): path for rel, path in files}
    selected = {name for name in included if name.startswith(APPROVED_SDK_ROOT + '/')}
    if not selected:
        return set()
    require(len(included) == len(files), 'Duplicate distribution delivery path')
    members = approved_sdk_members(lock)
    require(selected == set(members) | APPROVED_SDK_DOCS,
            'Approved SDK subset must contain exactly 11 headers, README, license and SHA256SUMS')
    checksums = {}
    for line in included[f'{APPROVED_SDK_ROOT}/SHA256SUMS'].read_text().splitlines():
        sha, separator, relative = line.partition('  ')
        name = f'{APPROVED_SDK_ROOT}/{relative}'
        require(separator and SHA.fullmatch(sha) and name not in checksums,
                'Invalid approved SDK SHA256SUMS')
        checksums[name] = sha
    require(set(checksums) == selected - {f'{APPROVED_SDK_ROOT}/SHA256SUMS'},
            'Approved SDK SHA256SUMS file list mismatch')
    for name in selected:
        require(included[name] == safe_path(root, name), f'Approved SDK delivery path mismatch: {name}')
        if name in checksums:
            require(digest(included[name]) == checksums[name], f'Approved SDK checksum mismatch: {name}')
    require(digest(included[f'{APPROVED_SDK_ROOT}/LICENSE-APACHE-2.0.txt']) ==
            'cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30',
            'Approved SDK requires the unmodified official Apache-2.0 license')
    for name, member in members.items():
        data = included[name].read_bytes()
        require(len(data) == member['bytes'] and hashlib.sha256(data).hexdigest() == member['sha256'],
                f'Approved SDK source-lock mismatch: {name}')
        require(b'SPDX-License-Identifier: Apache-2.0' in data[:2048] and
                b'Copyright (c) 2009-2018 ARM Limited. All rights reserved.' in data[:2048] and
                b'Licensed under the Apache License, Version 2.0' in data[:2048],
                f'Approved SDK Apache notice missing: {name}')
    return set(members)

def enrich_source_refs(value, lock):
    """Add stable IDs to exact, unambiguous pinned-source records in place.

    Existing IDs and compatibility fields are retained. Project-evidence hashes
    and ambiguous member identities do not gain invented vendor references.
    Reproduction tools use this before comparing or writing authored YAML.
    """
    by_hash = {}
    for node in source_nodes(lock):
        by_hash.setdefault(node['sha256'], []).append(node['id'])
    for _, row in walk(value):
        if not isinstance(row, dict) or 'source_ref' in row:
            continue
        sha = row.get('sha256') or row.get('archive_sha256')
        candidates = by_hash.get(sha, []) if isinstance(sha, str) else []
        if len(candidates) == 1:
            row['source_ref'] = candidates[0]
    return value

def input_paths(root):
    # Curated YAML, vendor catalog facts and authored evidence are authorities.
    # Generated chip/PAC JSON, audit reports and provenance views are not inputs.
    for pattern in ['cw32-data/**/*.yaml', 'sources/catalog.json', 'docs/*.json',
                    'tests/*sources*.json']:
        for path in sorted(root.glob(pattern)):
            rel = path.relative_to(root).as_posix()
            if rel in VIEWS or rel == LOCK or rel.startswith('cw32-data/data/'):
                continue
            if rel.startswith('docs/') and (any(fnmatch.fnmatch(path.name, pattern) for pattern in REPORT_PATTERNS)
                    or path.name in {'generated-data-parity.json', 'schema-reimplementation.json'}):
                continue
            yield rel

def validate_lock(root, lock, sources=None, include_discovery=False):
    require(lock['schema_version'] == 1, 'Unsupported source lock schema')
    nodes = source_nodes(lock)
    require(len({x['id'] for x in nodes}) == len(nodes), 'Duplicate source ID')
    require(len({x['path'] for x in nodes}) == len(nodes), 'Duplicate source output path')
    try:
        validate_scope(lock)
    except ValueError as error:
        raise ProvenanceError(str(error)) from error
    ids = {x['id'] for x in nodes}
    for node in nodes:
        safe_path(root, node['path'])
        require(bool(SHA.fullmatch(node['sha256'])), f'Invalid SHA256: {node["id"]}')
        require(isinstance(node['bytes'], int) and node['bytes'] > 0, f'Invalid size: {node["id"]}')
        if sources and (include_discovery or not discovery_only(node)):
            p = safe_path(sources, node['path'])
            require(p.is_file(), f'Missing acquired source: {p}')
            require(p.stat().st_size == node['bytes'] and digest(p) == node['sha256'],
                    f'Acquired source mismatch: {p}')
    for artifact in lock['artifacts']:
        url = urlsplit(artifact['url'])
        require(url.scheme == 'https' and url.hostname in {'www.whxy.com', 'cache.nxp.com'},
                f'Unverified source host: {artifact["url"]}')
        pr = artifact['provenance']
        require(pr['filename'] == Path(artifact['path']).name, 'Filename/path mismatch')
        require(pr['chip_scope'], f'No chip scope: {artifact["id"]}')
        require('acquired_date' in pr, 'Acquisition date must be explicit, including null')
        if pr['acquired_date']:
            require(pr.get('acquisition_evidence'), 'Acquisition date lacks download evidence')
            require((root / pr['acquisition_evidence'].split('#')[0]).is_file(), 'Acquisition record missing')
        if pr.get('superseded_by'):
            require(pr['superseded_by'] in ids, 'Unresolved superseding source ID')
        if artifact['kind'] == 'pdf':
            require(pr['printed_revision'] and pr['revision_history_date'], 'PDF printed identity missing')
            require(pr.get('revision_evidence', {}).get('pdf_page_1_based') == 1,
                    'PDF revision needs exact cover attribution')
        require(artifact['license']['bundle_raw'] is False and artifact['license']['bundle_derived_text'] is False,
                'Redistribution policy changed: separately review before granting raw/derived inclusion')
        for spec in [artifact, *artifact.get('members', []), *([artifact['text']] if 'text' in artifact else [])]:
            for ref in spec.get('evidence', []):
                require(safe_path(root, ref.split('#')[0]).is_file(), f'Missing evidence reference: {ref}')
    audit_ids = set()
    for audit in lock.get('project_audit_inputs', []):
        require(audit['id'] not in audit_ids and audit['id'] not in ids, 'Duplicate project audit ID')
        audit_ids.add(audit['id'])
        path = safe_path(root, audit['path'])
        require(path.is_file() and path.stat().st_size == audit['bytes']
                and digest(path) == audit['sha256'], f'Immutable project audit mismatch: {audit["path"]}')
        require(audit['reproducible_from_current_source'] is False and audit['legal_clearance'] is False,
                'Project audit exception requires explicit historical/non-legal scope')
    for name, pin in lock['upstream'].items():
        require(re.fullmatch('[0-9a-f]{40}', pin['rev']), f'Unpinned upstream {name}')
    for lock_path in ('Cargo.lock', 'firmware/Cargo.lock'):
        cargo = tomllib.loads((root / lock_path).read_text())
        for pkg in cargo['package']:
            source = pkg.get('source', '')
            if source.startswith('git+'):
                matches = [pin for pin in lock['upstream'].values()
                           if source.removeprefix('git+').split('?')[0].removesuffix('.git') == pin['url'].removesuffix('.git')]
                require(matches and source.split('#')[-1] == matches[0]['rev'],
                        f'{lock_path} Git revision not covered by canonical source lock: {source}')
    return nodes

def compatibility_views(lock):
    vendor = []
    for a in lock['artifacts']:
        if a['kind'] != 'sdk':
            continue
        svd = next(m for m in a['members'] if m['path'] == a['provenance']['generator_svd_path'])
        header = next(m for m in a['members'] if m['path'] == a['provenance']['generator_header_path'])
        vendor.append({'family': a['family'], 'url': a['url'], 'archive_sha256': a['sha256'],
                       'svd_sha256': svd['sha256'], 'svd_basename': Path(svd['path']).name,
                       'selection': 'Exact member chain: ' + ' -> '.join(svd['members']),
                       'header_sha256': header['sha256']})
    f030 = next(x for x in lock['artifacts'] if x.get('family') == 'CW32F030')
    svd = next(m for m in f030['members'] if m['path'] == f030['provenance']['generator_svd_path'])
    header = next(m for m in f030['members'] if m['path'] == f030['provenance']['generator_header_path'])
    source = {'schema_version': 1, 'generated_from': LOCK, 'upstream': lock['upstream'],
              'vendor': {'url': f030['url'], 'archive_sha256': f030['sha256'],
                         'svd_member': ' -> '.join(svd['members']), 'svd_sha256': svd['sha256'],
                         'cmsis_header_sha256': header['sha256'],
                         'raw_redistribution': 'excluded: whole SDK/SVD redistribution permission not verified'}}
    return {'build/provenance/vendor-sources.json': encode(vendor), 'build/provenance/source-lock.json': encode(source)}

def reference_index(root, lock, nodes):
    hashes = {}
    for node in nodes:
        hashes.setdefault(node['sha256'], []).append(node['id'])
    by_url = {a['url']: a for a in lock['artifacts']}
    known_ids = {n['id'] for n in nodes}
    references = []
    unresolved = []
    documents = []
    for rel in sorted(set(input_paths(root))):
        content = (root / rel).read_text()
        data = yaml.safe_load(content) if Path(rel).suffix == '.yaml' else json.loads(content)
        refs = []
        citations = []
        for pointer, value in walk(data):
            if isinstance(value, str) and value in hashes:
                refs.append({'pointer': pointer, 'source_ids': hashes[value],
                             'citation_context_pointer': pointer.rsplit('/', 1)[0]})
            if isinstance(value, str):
                for url in re.findall(r'https://(?:www\.whxy\.com/uploads/files/|cache\.nxp\.com/docs/)[^\s"<>]+', value):
                    url = url.rstrip(').,;')
                    if url in by_url:
                        refs.append({'pointer': pointer, 'source_ids': [by_url[url]['id']],
                                     'citation_context_pointer': pointer.rsplit('/', 1)[0],
                                     'match': 'official URL in evidence text'})
                    else:
                        unresolved.append({'document': rel, 'pointer': pointer, 'url': url,
                                           'reason': 'Official artifact URL absent from canonical lock'})
            if isinstance(value, dict) and isinstance(value.get('url'), str):
                url = value['url']
                sha = value.get('sha256') or value.get('archive_sha256')
                if (('www.whxy.com/uploads/files/' in url or 'cache.nxp.com/docs/' in url) and sha):
                    if url not in by_url or sha != by_url[url]['sha256']:
                        # Member hashes may deliberately carry their archive URL.
                        valid_member = any(n['sha256'] == sha and by_url.get(url, {}).get('id') == n['original_id'] for n in nodes)
                        if not valid_member:
                            unresolved.append({'document': rel, 'pointer': pointer, 'url': url,
                                               'sha256': sha, 'reason': 'Source URL/hash pair absent from canonical lock'})
            if isinstance(value, dict) and value.get('sha256') and value['sha256'] not in hashes:
                path = (value.get('path') or value.get('file') or value.get('filename')
                        or value.get('artifact') or value.get('document_filename') or pointer.split('/')[-1])
                if re.search(r'\.(pdf|svd|h|c|pdsc|zip|txt)(?:$|\b)', str(path)):
                    unresolved.append({'document': rel, 'pointer': pointer, 'sha256': value['sha256'],
                                       'reason': 'Referenced external file hash absent from canonical lock'})
            if re.search(r'(pages?|sections?|tables?|reviewed_sections)$', pointer.split('/')[-1], re.I):
                citations.append({'pointer': pointer, 'interpretation': 'See original evidence; page bases and source scope are preserved there'})
            if isinstance(value, dict) and isinstance(value.get('source_ref'), str):
                if value['source_ref'] in known_ids:
                    refs.append({'pointer': pointer + '/source_ref', 'source_ids': [value['source_ref']],
                                 'citation_context_pointer': pointer})
                    node = next(n for n in nodes if n['id'] == value['source_ref'])
                    original = next(a for a in lock['artifacts'] if a['id'] == node['original_id'])
                    if value.get('sha256'):
                        require(value['sha256'] == node['sha256'],
                                f'Source reference/hash mismatch: {rel}#{pointer}')
                    if value.get('archive_sha256'):
                        require(value['archive_sha256'] == original['sha256'],
                                f'Source reference/archive hash mismatch: {rel}#{pointer}')
                    if value.get('url'):
                        require(value['url'] == original['url'],
                                f'Source reference/URL mismatch: {rel}#{pointer}')
                    for page in value.get('pdf_pages_1_based', []):
                        require(isinstance(page, int) and 1 <= page <= original['provenance']['pdf_page_count'],
                                f'Invalid PDF page reference: {rel}#{pointer}')
                    require(len(value.get('printed_pages', [])) == len(value.get('pdf_pages_1_based', [])),
                            f'Printed/PDF page attribution mismatch: {rel}#{pointer}')
                else:
                    unresolved.append({'document': rel, 'pointer': pointer, 'source_ref': value['source_ref'],
                                       'reason': 'Unknown explicit source_ref'})
        if refs:
            documents.append({'path': rel, 'sha256': digest(root / rel), 'references': refs, 'citation_pointers': citations})
            for ref in refs:
                references.append({'document': rel, **ref})
    require(not unresolved, 'Unresolved external references:\n' + encode(unresolved))
    # Generated metadata is a compatibility copy inside the lock; validate rather than silently fix it.
    for name, data in lock.get('generated_metadata', {}).items():
        for pointer, value in walk(data):
            if isinstance(value, dict) and value.get('url') in by_url and value.get('sha256'):
                require(value['sha256'] == by_url[value['url']]['sha256'], f'Stale generated metadata: {name}#{pointer}')
    corrections = []
    for rel in sorted(set(input_paths(root))):
        if any(word in rel for word in ['correction', 'conflict', 'divergence']) or rel.startswith('cw32-data/inputs/'):
            corrections.append({'path': rel, 'sha256': digest(root / rel)})
    generator_files = []
    for folder in ['cw32-data-gen', 'cw32-metapac-gen', 'cw32-data-serde', 'cw32-data-macros']:
        for path in sorted((root / folder).rglob('*')):
            if path.is_file() and not {'target', '__pycache__'} & set(path.parts) and path.suffix in {'.rs', '.toml'}:
                generator_files.append({'path': path.relative_to(root).as_posix(), 'sha256': digest(path)})
    transforms = []
    for pattern in ['cw32-data/tools/*.py', 'cw32-data/*/build*.py']:
        transforms.extend({'path': p.relative_to(root).as_posix(), 'sha256': digest(p)} for p in sorted(root.glob(pattern)))
    rust_locks = [{'path': path, 'sha256': digest(root / path),
                   'packages': tomllib.loads((root / path).read_text())['package']}
                  for path in ('Cargo.lock', 'firmware/Cargo.lock')]
    return {'schema_version': 1, 'generated_from': LOCK, 'authority_sha256': digest(root / LOCK),
            'scope': 'Source identities and exact JSON-pointer references; cited page/section/range claims remain in the referenced evidence documents. Navigation is not a new hardware-fact authority.',
            'sources': nodes, 'evidence_documents': documents, 'unresolved_references': unresolved,
            'historical_snapshots': [{'selected_source_id': a['id'], **a['provenance']['previous_snapshot']}
                                     for a in lock['artifacts'] if 'previous_snapshot' in a['provenance']],
            'known_provenance_gaps': [
                'First acquisition dates are unknown unless explicitly backed by logged download evidence.',
                'Original acquisition URL for the superseded September 2025 L011 manual snapshot is unknown.',
                'The local CW32 generator snapshot has no Git commit; complete file hashes are provided.'],
            'upstream': lock['upstream'], 'verification_environment': lock['verification_environment'],
            'project_audit_inputs': lock.get('project_audit_inputs', []),
            'rust_toolchain': json.loads((root / 'docs/toolchain.json').read_text()),
            'rust_dependency_locks': rust_locks,
            'python_dependency_lock': {'path': 'requirements-dev.txt', 'sha256': digest(root / 'requirements-dev.txt')},
            'upstream_file_ancestry': {'path': 'docs/upstream-file-provenance.json',
                                      'sha256': digest(root / 'docs/upstream-file-provenance.json')},
            'generator_snapshot': {'git_commit': None, 'status': 'Uncommitted local snapshot; hashes are the reproducible identity',
                                   'files': generator_files},
            'transforms': transforms, 'curated_corrections_and_conflicts': corrections,
            'hardware_validation': False}

def catalog(lock, index):
    out = ['# CW32 data source catalog', '', '<!-- Generated by cw32-data/tools/source_provenance.py; do not edit. -->', '',
           'The canonical URL/hash/member authority is [sources/evidence-sources.json](../../sources/evidence-sources.json).',
           'The [reference index](reference-index.json) links every indexed claim back to its source and exact evidence-document pointer (JSON Pointer over parsed YAML/JSON).',
           'A selected source is the project pin, not a claim that it is the newest vendor release.',
           'Unknown acquisition dates are intentional. A URL date is never used as an acquisition or publication date.', '',
           '## Upstream and tools', '']
    for name, pin in lock['upstream'].items():
        out.append(f'- {name}: [{pin["rev"]}]({pin["url"]}/tree/{pin["rev"]})')
    env = lock['verification_environment']
    out += ['', f'Python {env["python"]}; PDF text: Poppler {lock["text_extractor"]["verified_version"]}, `pdftotext -layout -enc UTF-8`.',
            'Rust versions are in [toolchain.json](../../docs/toolchain.json); exact dependency versions, Git commits and registry checksums are in [Cargo.lock](../../Cargo.lock), [firmware/Cargo.lock](../../firmware/Cargo.lock) and the reference index.',
            'The local CW32 generators have no recorded Git commit. Their version is 0.1.0 and the index records each source-file SHA-256.',
            'Python dependency versions are pinned in [requirements-dev.txt](../../requirements-dev.txt).',
            'Upstream license reviews are recorded per pinned component. In particular, stm32-data has no verified repository-wide license; its stm32-metapac-gen package declares MIT OR Apache-2.0.', '',
            '## Source inventory', '', '| Source | Printed revision | Publication / cover | Revision-history date | Acquisition | Chip scope |',
            '|---|---|---|---|---|---|']
    for a in lock['artifacts']:
        p = a['provenance']
        out.append(f'| [{a["path"]}]({a["url"]}) | {p["printed_revision"] or "unknown"} | {p["publication_date"] or "unknown"} | {p["revision_history_date"] or "unknown"} | {p["acquired_date"] or "unknown"} | {", ".join(p["chip_scope"])} |')
    out += ['', '## Version and license cautions', '',
            '- The historical F020 filename says V1.3 but its cover and footers print Rev 1.2. Keep its old hash separate from the current Rev 1.3 PDF under `current-datasheets/`.',
            '- The L011 manual has September 2025 and June 2026 snapshots with the same printed Rev 1.1. Only the selected June snapshot is current input; the historical hash and unknown original URL are retained in its provenance record.',
            '- Cover months and revision-history dates can differ. Both are recorded; neither is replaced with the URL directory date.',
            '- No vendor archive, SVD, PDF, HTML or extracted text is approved for bundling. Only the 11 unchanged CMSIS headers in `sources/approved-sdk-members/` are included with their inspected Apache-2.0 notices and official license. This file-level exception does not license the whole SDK; whole-archive redistribution remains unverified.',
            '- Exact inspected notices and their locations are recorded per artifact. Fetch the pinned official source and verify its hash locally.', '',
            '## Evidence, transforms and corrections', '',
            'The reference index records source original/member/text relationships, deterministic extraction settings, source-consuming evidence documents and their hashes, and local generator/transformation hashes.',
            'Claim details stay in their original evidence manifests: follow `citation_context_pointer` for page, section, table, operating range and interpretation. Do not infer a timing limit from a register-map alias.',
            'Reviewed corrections and unresolved conflicts remain linked individually in `curated_corrections_and_conflicts`. They are project interpretations, not vendor-issued errata.',
            'See [the review/refresh process](../../docs/source-provenance.md) before accepting changed sources or candidate registers.', '']
    return '\n'.join(out)

class ArchiveBudget:
    """Bound inspection without extracting archive paths to the filesystem."""
    def __init__(self):
        self.entries = 0
        self.decompressed_bytes = 0

    def entry(self, label):
        self.entries += 1
        require(self.entries <= MAX_ARCHIVE_ENTRIES,
                f'Archive validation entry budget exceeded: {label}')

    def allowance(self):
        return min(MAX_MEMBER_BYTES, MAX_DECOMPRESSED_BYTES - self.decompressed_bytes)

    def check_size(self, size, label):
        require(0 <= size <= self.allowance(),
                f'Archive validation byte budget exceeded: {label}')

    def produced(self, data, label):
        self.check_size(len(data), label)
        self.decompressed_bytes += len(data)
        return data

    def read_member(self, stream, label):
        return self.produced(stream.read(self.allowance() + 1), label)


def zip_entry_count(data, label):
    """Preflight the central directory before ZipFile allocates its member list.

    ZIP64 end records are refused: supported budgets are below ZIP64's size and
    entry thresholds. ZIP64 local headers for small members remain supported.
    """
    end = data.rfind(b'PK\x05\x06', max(0, len(data) - 65557))
    require(end >= 0 and end + 22 <= len(data), f'Uninspectable ZIP archive: {label}')
    _, disk, directory_disk, disk_count, count, size, offset, comment = struct.unpack_from('<4s4H2LH', data, end)
    require(end + 22 + comment == len(data) and disk == directory_disk == 0
            and disk_count == count and count != 65535 and size != 0xffffffff
            and offset != 0xffffffff and data[max(0, end - 20):end - 16] != b'PK\x06\x07',
            f'Unsupported or malformed ZIP directory: {label}')
    start = end - size
    require(0 <= offset == start, f'Unsupported ZIP prefix or directory bounds: {label}')
    actual = 0
    cursor = start
    while cursor < end:
        require(cursor + 46 <= end and data[cursor:cursor + 4] == b'PK\x01\x02',
                f'Invalid ZIP directory entry: {label}')
        name, extra, note = struct.unpack_from('<3H', data, cursor + 28)
        cursor += 46 + name + extra + note
        actual += 1
        require(actual <= MAX_ARCHIVE_ENTRIES, f'Archive validation entry budget exceeded: {label}')
    require(cursor == end and actual == count, f'Invalid ZIP directory count: {label}')
    return actual


def validate_zip_regions(data, archive, label):
    """Refuse unreferenced local records, prefixes, gaps and trailing payloads."""
    members = sorted(archive.infolist(), key=lambda member: member.header_offset)
    cursor = 0
    bounds = {}
    for i, member in enumerate(members):
        require(member.header_offset == cursor and data[cursor:cursor + 4] == b'PK\x03\x04'
                and cursor + 30 <= len(data), f'Uninspectable ZIP local records: {label}')
        flags = struct.unpack_from('<H', data, cursor + 6)[0]
        method = struct.unpack_from('<H', data, cursor + 8)[0]
        name, extra = struct.unpack_from('<2H', data, cursor + 26)
        start = cursor + 30 + name + extra
        end = start + member.compress_size
        limit = members[i + 1].header_offset if i + 1 < len(members) else archive.start_dir
        require(flags == member.flag_bits and method == member.compress_type and end <= limit,
                f'Invalid ZIP local bounds or compression: {label}')
        local_name = data[cursor + 30:cursor + 30 + name].decode('utf-8' if flags & 0x800 else 'cp437')
        require(local_name == member.orig_filename and not flags & 1,
                f'Uninspectable ZIP filename or encryption: {label}')
        descriptor = data[end:limit]
        if flags & 8:
            if len(descriptor) in {16, 24} and descriptor.startswith(b'PK\x07\x08'):
                descriptor = descriptor[4:]
            require(len(descriptor) in {12, 20}, f'Invalid ZIP data descriptor: {label}')
            values = struct.unpack('<LLL' if len(descriptor) == 12 else '<LQQ', descriptor)
            require(values == (member.CRC, member.compress_size, member.file_size),
                    f'Invalid ZIP descriptor values: {label}')
        else:
            require(not descriptor, f'Uninspectable ZIP gap: {label}')
        bounds[member] = (start, end)
        cursor = limit
    require(cursor == archive.start_dir, f'Uninspectable ZIP prefix: {label}')
    return bounds


def inspect_zip_member(data, member, label, budget):
    # ZipExtFile truncates to declared file_size, and its bzip2 decoder does not
    # bound output allocation. Inspect the complete compressed span ourselves.
    if member.compress_type == zipfile.ZIP_STORED:
        body = budget.produced(data, label)
    else:
        decoder = (zlib.decompressobj(-zlib.MAX_WBITS)
                   if member.compress_type == zipfile.ZIP_DEFLATED else bz2.BZ2Decompressor())
        body = budget.produced(decoder.decompress(data, budget.allowance() + 1), label)
        require(decoder.eof and not decoder.unused_data,
                f'Uninspectable or truncated ZIP compressed member: {label}')
    require(len(body) == member.file_size and zlib.crc32(body) == member.CRC,
            f'ZIP expanded size or CRC mismatch: {label}')
    return body


def inspect_distribution_bytes(data, label, hashes, budget, depth=0):
    """Match known exact prohibited content; this is not a copyright scanner."""
    require(hashlib.sha256(data).hexdigest() not in hashes,
            f'Unlicensed external input in release: {label}')
    codec = next((name for magic, name in [(b'\x1f\x8b', 'gzip'), (b'BZh', 'bzip2'),
                                         (b'\xfd7zXZ\x00', 'xz')] if data.startswith(magic)), None)
    is_zip = data.startswith((b'PK\x03\x04', b'PK\x05\x06', b'PK\x07\x08')) or zipfile.is_zipfile(io.BytesIO(data))
    header = data[:512]
    is_tar = len(header) == 512 and (header == bytes(512) or header[257:263] in {b'ustar\x00', b'ustar '})
    if not is_tar and len(header) == 512:
        try:
            tarfile.TarInfo.frombuf(header, 'utf-8', 'surrogateescape')
            is_tar = True
        except tarfile.HeaderError:
            pass
    if not (codec or is_zip or is_tar):
        return
    require(depth < MAX_ARCHIVE_DEPTH, f'Archive validation depth budget exceeded: {label}')
    try:
        if codec:
            # Inspect individual concatenated streams and their combined output:
            # neither a new stream nor splitting an archive may hide known bytes.
            chunks = []
            remaining = data
            while remaining:
                budget.entry(label)
                if codec == 'gzip':
                    decoder = zlib.decompressobj(16 + zlib.MAX_WBITS)
                elif codec == 'bzip2':
                    decoder = bz2.BZ2Decompressor()
                else:
                    decoder = lzma.LZMADecompressor(memlimit=MAX_MEMBER_BYTES)
                decoded = budget.produced(decoder.decompress(remaining, budget.allowance() + 1), label)
                require(decoder.eof, f'Truncated or over-budget {codec} stream: {label}')
                chunks.append(decoded)
                require(sum(map(len, chunks)) <= MAX_MEMBER_BYTES,
                        f'Archive validation byte budget exceeded: {label}')
                inspect_distribution_bytes(decoded, label + f'!{codec}[{len(chunks)}]', hashes, budget, depth + 1)
                remaining = decoder.unused_data
                if codec in {'gzip', 'xz'} and not remaining.strip(b'\x00'):
                    remaining = b''
            if len(chunks) > 1:
                inspect_distribution_bytes(b''.join(chunks), label + f'!{codec}[combined]', hashes, budget, depth + 1)
        elif is_zip:
            count = zip_entry_count(data, label)
            require(count <= MAX_ARCHIVE_ENTRIES - budget.entries,
                    f'Archive validation entry budget exceeded: {label}')
            with zipfile.ZipFile(io.BytesIO(data)) as archive:
                bounds = validate_zip_regions(data, archive, label)
                for member in archive.infolist():
                    member_label = label + '!' + member.filename
                    budget.entry(member_label)
                    budget.check_size(member.file_size, member_label)
                    # ZIP LZMA does not expose a decoder memory limit in zipfile.
                    require(member.compress_type in {zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED, zipfile.ZIP_BZIP2},
                            f'Unsupported ZIP compression: {member_label}')
                    start, end = bounds[member]
                    body = inspect_zip_member(data[start:end], member, member_label, budget)
                    inspect_distribution_bytes(body, member_label, hashes, budget, depth + 1)
        else:
            # Charge the entire TAR layer before tarfile parses hidden metadata.
            # Member reads are charged again as their separately inspected layer.
            budget.produced(data, label)

            class BoundedTarInfo(tarfile.TarInfo):
                def _proc_member(self, archive):
                    # Includes metadata headers otherwise hidden by tarfile.
                    budget.entry(label + '!' + self.name)
                    budget.check_size(self.size, label + '!' + self.name)
                    require(self.type != tarfile.GNUTYPE_SPARSE,
                            f'Unsupported sparse TAR member: {label}!{self.name}')
                    if self.type in {tarfile.XHDTYPE, tarfile.XGLTYPE, tarfile.SOLARIS_XHDTYPE}:
                        start = archive.fileobj.tell()
                        require(b'GNU.sparse.' not in data[start:start + self.size],
                                f'Unsupported sparse TAR metadata: {label}!{self.name}')
                    return super()._proc_member(archive)

            with tarfile.open(fileobj=io.BytesIO(data), mode='r:', tarinfo=BoundedTarInfo) as archive:
                for member in archive:
                    # tarfile (like common TAR readers) treats unknown typeflags
                    # as regular files. Their bodies need the same inspection.
                    if member.isfile() or member.type not in tarfile.SUPPORTED_TYPES:
                        member_label = label + '!' + member.name
                        budget.check_size(member.size, member_label)
                        with archive.extractfile(member) as stream:
                            body = budget.read_member(stream, member_label)
                        inspect_distribution_bytes(body, member_label, hashes, budget, depth + 1)
                    else:
                        require(member.size == 0, f'Unexpected non-file TAR payload: {label}!{member.name}')
                require(not data[archive.offset:].strip(b'\x00'),
                        f'Uninspectable trailing TAR data: {label}')
    except (OSError, EOFError, ValueError, RuntimeError, RecursionError, tarfile.TarError,
            zipfile.BadZipFile, zlib.error, lzma.LZMAError) as error:
        if isinstance(error, ProvenanceError):
            raise
        raise ProvenanceError(f'Cannot inspect archive {label}: {error}') from error


def validate_distribution(root, files, lock=None):
    """Refuse known prohibited hashes, including renamed/nested archive members.

    Exact-content/provenance guard only; it does not certify license clearance or
    identify arbitrary copied content. Recognized containers with unsupported
    features, malformed content or exhausted inspection budgets fail closed.
    """
    files = list(files)
    lock = lock or json.loads((root / LOCK).read_text())
    review_path = root / 'docs/upstream-file-provenance.json'
    require(review_path.is_file(), 'Missing upstream redistribution review')
    review = json.loads(review_path.read_text())
    allowed_states = {'unresolved', 'excluded_historical_unresolved',
                      'explicit_upstream_package_declaration', 'reviewed_independent_replacement'}
    for row in review['files']:
        require(row['distribution_state'] in allowed_states, f'Unknown distribution review state: {row["project_path"]}')
        if row['distribution_state'] == 'reviewed_independent_replacement':
            proof = row.get('replacement_review', {})
            require(proof.get('independent_review_passed') is True,
                    f'Independent replacement review missing: {row["project_path"]}')
            for key in ['review_reference', 'verification_reference']:
                require(isinstance(proof.get(key), str) and safe_path(root, proof[key].split('#')[0]).is_file(),
                        f'Replacement proof missing {key}: {row["project_path"]}')
            current = safe_path(root, row['project_path'])
            require(current.is_file() and digest(current) == proof.get('replacement_sha256'),
                    f'Reviewed replacement hash changed: {row["project_path"]}')
    unresolved_paths = {x['project_path'] for x in review['files']
                        if x['distribution_state'] in {'unresolved', 'excluded_historical_unresolved'}}
    archived_hashes = set()
    for archive in review.get('historical_archives', []):
        require(archive['distribution_state'] == 'excluded_historical_unresolved',
                f'Unknown historical archive review state: {archive["archive_path"]}')
        safe_path(root, archive['archive_path'])
        require(bool(SHA.fullmatch(archive['archive_sha256'])) and archive['archive_bytes'] > 0,
                f'Invalid historical archive identity: {archive["archive_path"]}')
        unresolved_paths.add(archive['archive_path'])
        archived_hashes.add(archive['archive_sha256'])
        for member in archive['members']:
            require(member['distribution_state'] == 'excluded_historical_unresolved'
                    and bool(SHA.fullmatch(member['sha256'])) and member['bytes'] > 0,
                    f'Invalid historical archive member identity: {archive["archive_path"]}!{member["member_path"]}')
            archived_hashes.add(member['sha256'])
    for rel, path in files:
        require(str(rel) not in unresolved_paths, f'Unresolved upstream redistribution permission: {rel}; see docs/upstream-file-provenance.json')
    approved = validate_approved_sdk_members(root, files, lock)
    hashes = {n['sha256'] for n in source_nodes(lock)} | archived_hashes
    hashes.update(x['observed_project_sha256'] for x in review['files']
                  if x['distribution_state'] in {'unresolved', 'excluded_historical_unresolved', 'reviewed_independent_replacement'})
    hashes.update(h['sha256'] for a in lock['artifacts'] for h in a.get('pin_history', []))
    hashes.update(a['provenance']['previous_snapshot']['sha256'] for a in lock['artifacts']
                  if 'previous_snapshot' in a['provenance'])
    budget = ArchiveBudget()
    for rel, path in files:
        if str(rel) in approved:
            continue  # Exact bytes and license checked above; renamed/nested copies stay prohibited.
        with path.open('rb') as stream:
            data = stream.read(MAX_MEMBER_BYTES + 1)
        require(len(data) <= MAX_MEMBER_BYTES, f'Archive validation input byte budget exceeded: {rel}')
        inspect_distribution_bytes(data, str(rel), hashes, budget)


def generate(root, sources=None, include_discovery=False):
    lock = json.loads((root / LOCK).read_text())
    nodes = validate_lock(root, lock, sources, include_discovery)
    index = reference_index(root, lock, nodes)
    result = compatibility_views(lock)
    result['build/provenance/reference-index.json'] = encode(index)
    result['build/provenance/SOURCE-CATALOG.md'] = catalog(lock, index)
    return result

def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--root', type=Path, default=ROOT)
    p.add_argument('--sources', type=Path, help='Acquired-source directory; verifies all hardware-required original/member/text bytes')
    p.add_argument('--include-discovery', action='store_true', help='Also require exact historical HTML bytes for an all-record archival replay')
    p.add_argument('--write', action='store_true', help='Regenerate only the four derived views; never repin sources')
    p.add_argument('--out-dir', type=Path, help='Directory for the four derived views (authored inputs still come from --root)')
    args = p.parse_args(argv)
    result = generate(args.root, args.sources, args.include_discovery)
    for rel, content in result.items():
        path = args.out_dir / Path(rel).name if args.out_dir is not None else args.root / rel
        if args.write:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content)
        else:
            require(path.is_file() and path.read_text() == content, f'Stale/missing generated view: {rel}; review and run --write')
    if args.sources:
        lock = json.loads((args.root / LOCK).read_text())
        print(json.dumps({**scope_report(lock, args.include_discovery), 'complete_manifest': args.include_discovery, 'complete_hardware': True}, sort_keys=True))
    print(f'PASS: canonical source lock, {len(result)} views, evidence references and dependency pins' + ('; acquired bytes verified in reported scope' if args.sources else ' (offline metadata check)'))
    return 0

if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except (OSError, KeyError, ValueError, StopIteration) as error:
        print(f'Provenance validation failed: {error}', file=sys.stderr)
        raise SystemExit(1)
