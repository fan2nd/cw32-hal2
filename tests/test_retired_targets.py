#!/usr/bin/env python3
"""Current support and immutable historical snapshots have disjoint roles."""
import json
from pathlib import Path
import tempfile
import tomllib
import unittest
from current_targets import ROOT, active_names, historical_chip_path, is_retired, retired_targets


class RetirementTests(unittest.TestCase):
    def test_exact_scope_and_unknown_not_silently_filtered(self):
        self.assertEqual(retired_targets(), {'CW32W031', 'CW32W031R8U6'})
        self.assertEqual(active_names({'CW32W031', 'CW32W031R8U6', 'CW32L031', 'UNKNOWN', 'CW32W031OTHER'}),
                         {'CW32L031', 'UNKNOWN', 'CW32W031OTHER'})

    def test_active_output_and_features_have_exact_survivor_roster(self):
        import yaml
        historical = set()
        for path in (ROOT / 'cw32-data/inputs').glob('*.yaml'):
            profile = yaml.safe_load(path.read_text())
            if profile.get('quarantine'):
                continue
            historical.update(chip['name'] for chip in profile['chips'])
            if profile.get('parts_catalog'):
                parts = yaml.safe_load((ROOT / profile['parts_catalog']).read_text())['parts']
                historical.update(p['name'] for p in parts if p['family'] == profile['line'])
        self.assertEqual(len(historical), 54)
        expected = historical - retired_targets()
        self.assertEqual(len(expected), 52)
        self.assertEqual({p.stem for p in (ROOT / 'cw32-data/data/chips').glob('*.json')}, expected)
        for crate in ['cw32-metapac', 'embassy-cw32']:
            features = tomllib.loads((ROOT / crate / 'Cargo.toml').read_text())['features']
            self.assertEqual({n.upper() for n in features if n.startswith('cw32')}, expected)
        for name in retired_targets():
            self.assertFalse((ROOT / 'cw32-metapac/src/chips' / name.lower()).exists())

    def test_historical_snapshots_are_pinned_not_live_fallbacks(self):
        for name in retired_targets():
            live = ROOT / 'cw32-data/data/chips' / (name + '.json')
            snapshot = historical_chip_path(live)
            self.assertEqual(json.loads(snapshot.read_text())['name'], name)
            self.assertEqual(snapshot.parent, ROOT / 'docs/retired-w031')
        path = ROOT / 'cw32-data/data/chips/UNKNOWN.json'
        self.assertEqual(historical_chip_path(path), path)
        self.assertFalse(path.exists())

    def test_scope_drift_missing_snapshot_and_live_reappearance_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            authority = root / 'cw32-data-serde/src/retired-targets.txt'
            authority.parent.mkdir(parents=True)
            for text in ['CW32W031\n', 'CW32W031\nCW32W031R8U6\nCW32L031\n', 'CW32W031\nCW32W031\n']:
                authority.write_text(text)
                with self.assertRaises(AssertionError): retired_targets(root)
            authority.write_text('CW32W031\nCW32W031R8U6\n')
            live = root / 'cw32-data/data/chips/CW32W031.json'
            with self.assertRaises(FileNotFoundError): historical_chip_path(live, root)
            snapshot = root / 'docs/retired-w031/CW32W031.json'
            snapshot.parent.mkdir(parents=True)
            snapshot.write_text('{}')
            with self.assertRaises(AssertionError): historical_chip_path(live, root)
            live.parent.mkdir(parents=True)
            live.write_text('{}')
            with self.assertRaises(AssertionError): historical_chip_path(live, root)


if __name__ == '__main__':
    unittest.main()
