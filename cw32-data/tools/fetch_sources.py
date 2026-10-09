#!/usr/bin/env python3
"""Acquire SHA-pinned SVDs from official SDKs, including nested MDK packs.
All raw inputs stay ignored under sources/vendor/; no redistribution license assumed.
"""
import hashlib, io, json, urllib.request, zipfile
from pathlib import Path
from source_provenance import compatibility_views, validate_lock
ROOT = Path(__file__).resolve().parents[2]

def check(data, expected):
    actual = hashlib.sha256(data).hexdigest()
    if actual != expected:
        raise SystemExit(f"SHA-256 mismatch: expected {expected}, got {actual}")

def find_svd(archive, basename, expected_sha, depth=0):
    if depth > 4:
        return None
    with zipfile.ZipFile(io.BytesIO(archive)) as z:
        for name in z.namelist():
            if name.replace("\\", "/").split("/")[-1].lower() == basename.lower():
                data = z.read(name)
                if hashlib.sha256(data).hexdigest() == expected_sha:
                    return data
        for name in z.namelist():
            if name.lower().endswith((".pack", ".zip")):
                data = find_svd(z.read(name), basename, expected_sha, depth+1)
                if data is not None:
                    return data
    return None

out = ROOT / "sources/vendor"
out.mkdir(parents=True, exist_ok=True)
# The legacy vendor-sources.json is a generated view, never a second pin authority.
lock = json.loads((ROOT / "sources/evidence-sources.json").read_text())
validate_lock(ROOT, lock)
vendor_sources = json.loads(compatibility_views(lock)["build/provenance/vendor-sources.json"])
for item in vendor_sources:
    target = out / item["svd_basename"]
    if target.exists():
        check(target.read_bytes(), item["svd_sha256"])
        print(f'Verified cached {item["family"]}')
        continue
    archive = out / item["url"].split("/")[-1]
    if not archive.exists():
        with urllib.request.urlopen(item["url"], timeout=120) as source:
            data = source.read()
        check(data, item["archive_sha256"])
        archive.write_bytes(data)
    data = archive.read_bytes()
    check(data, item["archive_sha256"])
    svd = find_svd(data, item["svd_basename"], item["svd_sha256"])
    if svd is None:
        raise SystemExit(f'Pinned SVD not found for {item["family"]}')
    check(svd, item["svd_sha256"])
    target.write_bytes(svd)
    print(f'Verified and extracted {item["family"]}')
