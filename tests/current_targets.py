"""Exact current-target subtraction; historical evidence is not modified."""
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def retired_targets(root=ROOT):
    names = (root / 'cw32-data-serde/src/retired-targets.txt').read_text().splitlines()
    assert names == ['CW32W031', 'CW32W031R8U6'], 'unexpected retirement scope'
    return frozenset(names)


def is_retired(name, root=ROOT):
    return name.upper() in retired_targets(root)


def active_names(names, root=ROOT):
    """Preserve every non-retired name, including unknown names for later checks."""
    retired = retired_targets(root)
    return {name for name in names if name.upper() not in retired}


def historical_chip_path(path, root=ROOT):
    """Read ONLY the two immutable retirement snapshots for historical audits.

    This is not a generated-data fallback: live retired output must be absent,
    and every other path is returned unchanged (missing inputs still fail).
    """
    import hashlib
    path = Path(path)
    expected = {
        'CW32W031': '1aaaab9824417c136b6509b6cba16f68f616ba09bb626d92340746ad44cfb075',
        'CW32W031R8U6': 'f8d4d0daf484dd615656e4ef1cffa69c17ff486ae43b8bb0a5ec3a137ca3980a',
    }
    assert set(expected) == retired_targets(root)
    if path.name not in {name + '.json' for name in expected}:
        return path
    assert path.parent.resolve() == (root / 'cw32-data/data/chips').resolve(), 'not a normal chip-data path'
    assert not path.exists(), 'retired target unexpectedly present in generated data'
    snapshot = root / 'docs/retired-w031' / path.name
    assert hashlib.sha256(snapshot.read_bytes()).hexdigest() == expected[path.stem], 'historical snapshot drift'
    return snapshot
