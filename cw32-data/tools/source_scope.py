"""Separate exactly two historical discovery pages from hardware evidence."""
DISCOVERY_URLS = {
    'a030-official-manuals.html': 'https://www.whxy.com/tongyonggaoxingnengMCU/CW32A030C8T7.html?act=doc&cid=21',
    'a030-official-sdk.html': 'https://www.whxy.com/tongyonggaoxingnengMCU/CW32A030C8T7.html?act=doc&cid=22',
}


def discovery_only(spec):
    """Scope is an explicit classification, never inferred from an extension."""
    if 'role' not in spec:
        return False
    path = spec.get('path')
    if (spec['role'] != 'discovery-only' or path not in DISCOVERY_URLS
            or spec.get('kind') != 'html' or spec.get('id') != 'vendor:' + path
            or spec.get('url') != DISCOVERY_URLS[path]
            or 'members' in spec or 'text' in spec):
        raise ValueError('Only the two named A030 HTML records may be discovery-only')
    return True


def validate_scope(lock):
    discovery = [row['path'] for row in lock['artifacts'] if discovery_only(row)]
    if sorted(discovery) != sorted(DISCOVERY_URLS):
        raise ValueError('Exactly the two A030 discovery-only records are required')
    for row in lock['artifacts']:
        for child in [*row.get('members', []), *([row['text']] if 'text' in row else [])]:
            if 'role' in child:
                raise ValueError('SDK members and PDF text cannot be discovery-only')


def scope_artifacts(lock, include_discovery=False):
    validate_scope(lock)
    return [row for row in lock['artifacts'] if include_discovery or not discovery_only(row)]


def scope_report(lock, include_discovery=False):
    required = scope_artifacts(lock, include_discovery)
    return {
        'scope': 'all-records' if include_discovery else 'hardware',
        'catalogued_originals': len(lock['artifacts']),
        'required_originals': len(required),
        'required_source_files': sum(1 + len(row.get('members', [])) + ('text' in row) for row in required),
        'generated_metadata_files': len(lock['generated_metadata']),
        'omitted_discovery': [{'id': row['id'], 'path': row['path']} for row in lock['artifacts']
                              if discovery_only(row) and not include_discovery],
    }
