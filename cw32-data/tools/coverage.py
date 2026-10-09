#!/usr/bin/env python3
"""Reconcile catalog, input eligibility and generated outputs without inflating coverage."""
import argparse
import json
from pathlib import Path
import yaml

ROOT = Path(__file__).resolve().parents[2]


def generate(root: Path = ROOT, data_dir: Path | None = None, pac_dir: Path | None = None) -> dict:
    data = root / 'cw32-data'
    data_dir = data_dir if data_dir is not None else data / 'data'
    pac_dir = pac_dir if pac_dir is not None else root / 'cw32-metapac'
    catalog = json.loads((root / 'sources/catalog.json').read_text())
    inputs = [yaml.safe_load(p.read_text()) for p in sorted((data / 'inputs').glob('*.yaml'))]
    families = {item['line']: item for item in inputs}
    for source in inputs:
        if source.get('parts_catalog'):
            extra = yaml.safe_load((root / source['parts_catalog']).read_text())['parts']
            source['chips'] = source['chips'] + [
                p for p in extra if p['family'] == source['line']
                and p['name'] not in {c['name'] for c in source['chips']}
            ]
    parts = []
    for part in catalog['parts']:
        name = part['part']
        family = name[:8]
        source = families.get(family)
        features = []
        normalized_peripherals = 0
        if source:
            for chip in source['chips']:
                if name.startswith(chip['name']):
                    chip_path = data_dir / 'chips' / (chip['name'] + '.json')
                    pac_path = pac_dir / 'src/chips' / chip['name'].lower() / 'pac.rs'
                    generated = chip_path.is_file() and pac_path.is_file() and not source.get('quarantine')
                    if chip_path.is_file():
                        chip_data = json.loads(chip_path.read_text())
                        normalized_peripherals = max(normalized_peripherals,
                            sum(len(c['peripherals']) for c in chip_data['cores']))
                    features.append({
                        'feature': chip['name'].lower(),
                        'normalized_metadata_present': chip_path.is_file(),
                        'pac_present': generated,
                        'exact_orderable_part': chip['name'] == name,
                        'memory_described': bool(chip.get('memory')),
                    })
        parts.append({
            'part': name,
            'family': family,
            'catalog_source': part['source_url'],
            'datasheet': part.get('datasheet_url'),
            'source_manifest_present': source is not None,
            'quarantine': source.get('quarantine') if source else 'No dedicated reviewed input manifest',
            'matching_features': features,
            'normalized_peripheral_instances': normalized_peripherals,
            'hardware_validated': False,
            'hal_status': 'active development; driver verification tracked separately',
        })
    return {
        'schema_version': 1,
        'catalog_snapshot': catalog.get('retrieved_utc'),
        'scope': 'Current official catalog snapshot, not an exhaustive list of historical or future orderable parts',
        'definitions': {
            'pac_present': 'File generation only; compile evidence recorded separately',
            'matching_features': 'Family/die feature may cover register map without exact memory or package pinout',
            'quarantine': 'Do not count quarantined input as implemented support',
        },
        'summary': {
            'catalog_entries': len(parts),
            'catalog_families': len(set(p['family'] for p in parts)),
            'source_manifests': len(inputs),
            'eligible_inputs': sum(not i.get('quarantine') for i in inputs),
            'quarantined_inputs': sum(bool(i.get('quarantine')) for i in inputs),
            'catalog_entries_with_generated_family_pac': sum(any(f['pac_present'] for f in p['matching_features']) for p in parts),
            'catalog_entries_with_exact_part_pac': sum(any(f['pac_present'] and f['exact_orderable_part'] for f in p['matching_features']) for p in parts),
            'hardware_validated_parts': 0,
        },
        'parts': parts,
    }

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    parser.add_argument('--data-dir', type=Path, help='Generated data to inspect (authored inputs still come from this workspace)')
    parser.add_argument('--pac-dir', type=Path, help='Generated PAC to inspect')
    parser.add_argument('--output', type=Path, default=ROOT / 'build/reports/coverage.json')
    args = parser.parse_args()
    path = args.output
    content = json.dumps(generate(data_dir=args.data_dir, pac_dir=args.pac_dir), ensure_ascii=False, indent=2) + '\n'
    if args.check:
        assert path.read_text() == content, 'coverage report stale; run coverage.py'
    else:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content)
    print(json.dumps(json.loads(content)['summary'], indent=2))
