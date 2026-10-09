#!/usr/bin/env python3
"""Exercise ./d's output transaction; tool stubs inject failures at CLI boundaries.

These synthetic fixtures do not assert hardware facts. The real data generator's
collision guard is covered by generation_boundaries.rs, and report path selection
is checked against the real reporting tools below.
"""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
VIEWS = ('source-lock.json', 'vendor-sources.json', 'reference-index.json', 'SOURCE-CATALOG.md')
OUTPUTS = ('cw32-data/data', 'cw32-metapac', 'build/reports/coverage.json',
           *(f'build/provenance/{name}' for name in VIEWS))


def load_module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class GenerationTransactionTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='cw32-output-transaction-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / 'workspace with spaces'
        self.root.mkdir()
        shutil.copy2(ROOT / 'd', self.root / 'd')
        self.bin = Path(self.temp.name) / 'bin'
        self.bin.mkdir()
        self.write('cw32-data/registers/authored.yaml', 'new reviewed definition\n')
        self.write('build/reports/unrelated.json', 'unrelated report\n')
        self.write('build/provenance/unrelated.txt', 'unrelated authored note\n')
        self.script(self.bin / 'cargo', '''
import os
from pathlib import Path
import sys
args = sys.argv
out = Path(args[args.index('--out-dir') + 1])
if 'cw32-data-gen' in args:
    assert Path(args[args.index('--root') + 1]) == Path.cwd()
    value = Path('cw32-data/registers/authored.yaml').read_text()
    destination = out / 'registers/result.json'
    if destination.exists() and destination.read_text() != value:
        raise RuntimeError('conflicting profiles attempted to share register version')
    step = 'data'
else:
    data = Path(args[args.index('--data-dir') + 1])
    value = (data / 'registers/result.json').read_text()
    destination = out / 'src/lib.rs'
    step = 'pac'
destination.parent.mkdir(parents=True, exist_ok=True)
destination.write_text(value)
if os.environ.get('FAIL_STEP') == step:
    raise SystemExit(71)
''')
        self.script(self.bin / 'rustfmt', '''
import os
raise SystemExit(72 if os.environ.get('FAIL_STEP') == 'format' else 0)
''')
        self.script(self.root / 'cw32-data/tools/coverage.py', '''
import os
from pathlib import Path
import sys
args = sys.argv
data = Path(args[args.index('--data-dir') + 1])
pac = Path(args[args.index('--pac-dir') + 1])
value = (data / 'registers/result.json').read_text()
assert value == (pac / 'src/lib.rs').read_text()
Path(args[args.index('--output') + 1]).write_text(value)
if os.environ.get('FAIL_STEP') == 'coverage':
    raise SystemExit(73)
''')
        self.script(self.root / 'cw32-data/tools/source_provenance.py', f'''
import os
from pathlib import Path
import sys
args = sys.argv
assert Path(args[args.index('--root') + 1]) == Path.cwd()
out = Path(args[args.index('--out-dir') + 1])
out.mkdir()
for name in {VIEWS!r}:
    (out / name).write_text(Path('cw32-data/registers/authored.yaml').read_text())
if os.environ.get('FAIL_STEP') == 'provenance':
    raise SystemExit(74)
''')
        # Fault injection stays in this test process's Python path, with no
        # production switches or alternate commit implementation.
        (self.bin / 'sitecustomize.py').write_text('''
import os
import sys
if sys.argv[0] == '-':
    original = os.rename
    count = 0
    failures = {int(n) for n in os.environ.get('FAIL_RENAMES', '').split(',') if n}
    def rename(source, destination):
        global count
        count += 1
        if count in failures:
            raise OSError('injected rename failure')
        return original(source, destination)
    os.rename = rename
''')

    def write(self, rel, value):
        path = self.root / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(value)

    def script(self, path, body):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(f'#!{sys.executable}\n' + body)
        path.chmod(0o755)

    def old_outputs(self):
        self.write('cw32-data/data/old.json', 'previous data\n')
        self.write('cw32-metapac/src/old.rs', 'previous pac\n')
        for rel in OUTPUTS[2:]:
            self.write(rel, f'previous {rel}\n')

    def snapshot(self):
        return {str(p.relative_to(self.root)): p.read_bytes()
                for p in self.root.rglob('*') if p.is_file()
                and not any(part.startswith('.generation.') or part == '__pycache__'
                            for part in p.relative_to(self.root).parts)}

    def run_generation(self, command='gen-all', **env):
        environ = dict(os.environ, PATH=str(self.bin) + os.pathsep + os.environ.get('PATH', ''),
                       PYTHONPATH=str(self.bin), **env)
        return subprocess.run(['bash', str(self.root / 'd'), command], cwd=self.temp.name,
                              env=environ, capture_output=True, text=True)

    def assert_no_staging(self):
        self.assertEqual(list((self.root / 'build').glob('.generation.*')), [])

    def test_each_generation_step_failure_preserves_all_old_bytes(self):
        self.old_outputs()
        before = self.snapshot()
        for step in ('data', 'pac', 'format', 'coverage', 'provenance'):
            with self.subTest(step=step):
                result = self.run_generation(FAIL_STEP=step)
                self.assertNotEqual(result.returncode, 0, result.stdout)
                self.assertEqual(self.snapshot(), before)
                self.assert_no_staging()

    def test_success_replaces_only_known_outputs_after_authored_change(self):
        self.old_outputs()
        for value in ('first reviewed definition\n', 'changed reviewed definition\n'):
            self.write('cw32-data/registers/authored.yaml', value)
            result = self.run_generation()
            self.assertEqual(result.returncode, 0, result.stderr)
            for rel in ('cw32-data/data/registers/result.json', 'cw32-metapac/src/lib.rs', *OUTPUTS[2:]):
                self.assertEqual((self.root / rel).read_text(), value)
            self.assertFalse((self.root / 'cw32-data/data/old.json').exists())
            self.assertFalse((self.root / 'cw32-metapac/src/old.rs').exists())
            self.assertEqual((self.root / 'cw32-data/registers/authored.yaml').read_text(), value)
            self.assertEqual((self.root / 'build/reports/unrelated.json').read_text(), 'unrelated report\n')
            self.assertEqual((self.root / 'build/provenance/unrelated.txt').read_text(), 'unrelated authored note\n')
            self.assert_no_staging()

    def test_fresh_source_first_generation(self):
        result = self.run_generation()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(all((self.root / rel).exists() for rel in OUTPUTS))
        self.assert_no_staging()

    def test_each_commit_rename_failure_restores_all_old_bytes(self):
        self.old_outputs()
        before = self.snapshot()
        for index in range(1, 2 * len(OUTPUTS) + 1):
            with self.subTest(rename=index):
                result = self.run_generation(FAIL_RENAMES=str(index))
                self.assertNotEqual(result.returncode, 0)
                self.assertIn('injected rename failure', result.stderr)
                self.assertEqual(self.snapshot(), before)
                self.assert_no_staging()

    def test_each_fresh_commit_failure_restores_absent_outputs(self):
        before = self.snapshot()
        for index in range(1, len(OUTPUTS) + 1):
            with self.subTest(rename=index):
                result = self.run_generation(FAIL_RENAMES=str(index))
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(self.snapshot(), before)
                self.assertTrue(all(not (self.root / rel).exists() for rel in OUTPUTS))
                self.assert_no_staging()

    def test_rollback_failure_retains_recoverable_backups(self):
        self.old_outputs()
        result = self.run_generation(FAIL_RENAMES='4,5')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('backups retained at', result.stderr)
        stages = list((self.root / 'build').glob('.generation.*'))
        self.assertEqual(len(stages), 1)
        self.assertTrue((stages[0] / 'KEEP').exists())
        self.assertEqual((stages[0] / 'previous/0/old.json').read_text(), 'previous data\n')
        self.assertEqual((stages[0] / 'previous/1/src/old.rs').read_text(), 'previous pac\n')

    def test_data_only_failure_and_success(self):
        self.old_outputs()
        before = self.snapshot()
        result = self.run_generation('gen', FAIL_RENAMES='2')
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.snapshot(), before)
        result = self.run_generation('gen')
        self.assertEqual(result.returncode, 0, result.stderr)
        after = self.snapshot()
        self.assertEqual({p: b for p, b in before.items() if not p.startswith('cw32-data/data/')},
                         {p: b for p, b in after.items() if not p.startswith('cw32-data/data/')})
        self.assert_no_staging()

    def test_symlinked_and_unexpected_destinations_are_preserved(self):
        self.old_outputs()
        external = Path(self.temp.name) / 'authored'
        external.mkdir()
        (external / 'source.txt').write_text('do not delete\n')
        for rel in ('cw32-data/data', 'cw32-metapac', 'build/provenance', 'build/reports'):
            with self.subTest(path=rel):
                destination = self.root / rel
                backup = destination.with_name(destination.name + '.test-backup')
                destination.rename(backup)
                destination.symlink_to(external, target_is_directory=True)
                before = self.snapshot()
                result = self.run_generation()
                self.assertNotEqual(result.returncode, 0)
                self.assertTrue(destination.is_symlink())
                self.assertEqual(self.snapshot(), before)
                self.assertEqual((external / 'source.txt').read_text(), 'do not delete\n')
                destination.unlink()
                backup.rename(destination)
                self.assert_no_staging()
        shutil.rmtree(self.root / 'cw32-data/data')
        self.write('cw32-data/data', 'unexpected authored file\n')
        before = self.snapshot()
        result = self.run_generation()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('Unexpected generated-output type', result.stderr)
        self.assertEqual(self.snapshot(), before)


class ReportPathTests(unittest.TestCase):
    def test_coverage_uses_staged_outputs_with_original_authored_root(self):
        coverage = load_module('coverage_paths', ROOT / 'cw32-data/tools/coverage.py')
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / 'sources').mkdir()
            (root / 'cw32-data/inputs').mkdir(parents=True)
            (root / 'sources/catalog.json').write_text(json.dumps({'parts': [
                {'part': 'CW32TEST01', 'source_url': 'https://example.invalid'}]}))
            (root / 'cw32-data/inputs/test.yaml').write_text(json.dumps({
                'line': 'CW32TEST', 'chips': [{'name': 'CW32TEST01'}]}))
            stage = root / 'build/staged'
            (stage / 'data/chips').mkdir(parents=True)
            (stage / 'data/chips/CW32TEST01.json').write_text('{"cores": [{"peripherals": [1, 2]}]}')
            (stage / 'pac/src/chips/cw32test01').mkdir(parents=True)
            (stage / 'pac/src/chips/cw32test01/pac.rs').write_text('')
            expected = coverage.generate(root, stage / 'data', stage / 'pac')
            self.assertEqual(expected['summary']['catalog_entries_with_exact_part_pac'], 1)
            self.assertEqual(expected['parts'][0]['normalized_peripheral_instances'], 2)
            self.assertEqual(coverage.generate(root)['summary']['catalog_entries_with_exact_part_pac'], 0)
            shutil.copytree(stage / 'data', root / 'cw32-data/data')
            shutil.copytree(stage / 'pac', root / 'cw32-metapac')
            self.assertEqual(expected, coverage.generate(root))

    def test_provenance_staging_preserves_source_identity_and_default_bytes(self):
        provenance = load_module('provenance_paths', ROOT / 'cw32-data/tools/source_provenance.py')
        expected = provenance.generate(ROOT)
        before = {rel: (ROOT / rel).read_bytes() for rel in provenance.input_paths(ROOT)}
        with tempfile.TemporaryDirectory() as temp:
            out = Path(temp) / 'views'
            self.assertEqual(provenance.main(['--root', str(ROOT), '--out-dir', str(out), '--write']), 0)
            self.assertEqual({str(out / Path(rel).name): value for rel, value in expected.items()},
                             {str(path): path.read_text() for path in out.iterdir()})
            self.assertEqual(provenance.main(['--root', str(ROOT), '--out-dir', str(out)]), 0)
        self.assertEqual(before, {rel: (ROOT / rel).read_bytes() for rel in provenance.input_paths(ROOT)})


if __name__ == '__main__':
    unittest.main(verbosity=2)
