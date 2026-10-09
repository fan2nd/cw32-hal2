"""Keep hardware route contracts independent of additive provenance references.

The provenance validator separately checks every source_ref against the canonical
source lock. These projections retain every other field, including source hashes,
so earlier hardware fingerprints remain meaningful after references are added.
"""
from functools import lru_cache
import importlib.util
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def hardware_facts(value):
    if isinstance(value, dict):
        return {key: hardware_facts(item) for key, item in value.items() if key != 'source_ref'}
    if isinstance(value, list):
        return [hardware_facts(item) for item in value]
    return value


@lru_cache(maxsize=1)
def _provenance():
    spec = importlib.util.spec_from_file_location('route_source_provenance', ROOT / 'cw32-data/tools/source_provenance.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    lock = json.loads((ROOT / 'sources/evidence-sources.json').read_text())
    return module, lock


def with_source_refs(value):
    module, lock = _provenance()
    return module.enrich_source_refs(value, lock)
