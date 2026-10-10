#!/usr/bin/env python3
"""Recover hash-pinned external official evidence without changing existing files.

Only the Python standard library and Poppler's pdftotext are required. See
../../docs/evidence-acquisition.md for cache, offline and source-audit usage.
"""
from __future__ import annotations

import argparse
from contextlib import contextmanager
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import stat
import subprocess
import sys
import tempfile
from urllib.parse import urlsplit
import urllib.request
import zipfile

# Support both script execution and importlib-based source tests.
sys.path.insert(0, str(Path(__file__).resolve().parent))
from source_scope import scope_artifacts, scope_report, validate_scope

ROOT = Path(__file__).resolve().parents[2]
MANIFEST = ROOT / "sources/evidence-sources.json"
CHUNK = 1024 * 1024
MAX_MEMBER_BYTES = 64 * CHUNK


class EvidenceError(RuntimeError):
    """Acquisition stopped rather than accepting changed or unsafe evidence."""


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(CHUNK), b""):
            digest.update(block)
    return digest.hexdigest()


def relative_path(value):
    # ZIP paths use POSIX separators; backslashes and drive prefixes are unsafe
    # on at least one supported host, even if harmless on the current host.
    if not isinstance(value, str) or not value or "\\" in value or ":" in value or "\0" in value:
        raise EvidenceError(f"Unsafe relative path: {value!r}")
    parts = value.split("/")
    if value.startswith("/") or any(p in ("", ".", "..") for p in parts):
        raise EvidenceError(f"Unsafe relative path: {value!r}")
    return PurePosixPath(value)


def destination(root, value):
    relative = relative_path(value)
    path = root
    for part in relative.parts:
        path = path / part
        if path.is_symlink():
            raise EvidenceError(f"Refusing symlink in evidence path: {path}")
    return path


def verify(path, spec):
    if not path.is_file() or path.is_symlink():
        raise EvidenceError(f"Expected regular file: {path}")
    actual = sha256(path)
    if actual != spec["sha256"]:
        raise EvidenceError(f"SHA-256 mismatch for {path}: expected {spec['sha256']}, got {actual}. "
                            "Existing files were not overwritten; review the source revision or choose an empty root.")
    if path.stat().st_size != spec["bytes"]:
        raise EvidenceError(f"Size mismatch for {path}")


@contextmanager
def staging(target):
    target.parent.mkdir(parents=True, exist_ok=True)
    descriptor, name = tempfile.mkstemp(prefix=".cw32-evidence-", dir=target.parent)
    os.close(descriptor)
    path = Path(name)
    try:
        yield path
    finally:
        path.unlink(missing_ok=True)


def publish(temp, target, spec):
    verify(temp, spec)
    try:
        # Same-directory atomic no-clobber publication, including concurrent
        # invocations. Never rename over a preexisting destination.
        os.link(temp, target)
    except FileExistsError:
        verify(target, spec)


def check_url(url):
    parsed = urlsplit(url)
    if (parsed.scheme != "https" or parsed.hostname not in {"www.whxy.com", "cache.nxp.com"}
            or parsed.username or parsed.password or parsed.port not in (None, 443)):
        raise EvidenceError(f"Refusing non-official HTTPS URL: {url}")


class OfficialRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        check_url(newurl)
        return super().redirect_request(req, fp, code, msg, headers, newurl)


def transfer(source, target, limit):
    total = 0
    with target.open("wb") as output:
        for block in iter(lambda: source.read(CHUNK), b""):
            total += len(block)
            if total > limit:
                raise EvidenceError(f"Download/extraction exceeds pinned size ({limit} bytes)")
            output.write(block)


def obtain(root, spec, cache, offline, opener):
    target = destination(root, spec["path"])
    if target.exists():
        verify(target, spec)
        return target, "existing"
    cached = destination(cache, spec["path"]) if cache else None
    with staging(target) as temp:
        if cached is not None and cached.exists():
            verify(cached, spec)
            with cached.open("rb") as source:
                transfer(source, temp, spec["bytes"])
            origin = "cache"
        elif offline:
            raise EvidenceError(f"Missing offline evidence: {spec['path']}; supply --cache-root or allow the pinned download")
        else:
            check_url(spec["url"])
            with opener.open(spec["url"], timeout=120) as response:
                check_url(response.geturl())
                transfer(response, temp, spec["bytes"])
            origin = "download"
        publish(temp, target, spec)
    return target, origin


def archive_members(archive):
    """Reject ambiguous/unsafe entries without ever using extract/extractall."""
    result = {}
    for item in archive.infolist():
        name = item.filename[:-1] if item.is_dir() else item.filename
        relative_path(name)
        mode = item.external_attr >> 16
        kind = stat.S_IFMT(mode)
        if kind not in (0, stat.S_IFREG, stat.S_IFDIR):
            raise EvidenceError(f"ZIP contains a link or special file: {item.filename}")
        if item.filename in result:
            raise EvidenceError(f"ZIP contains duplicate member: {item.filename}")
        if item.flag_bits & 1:
            raise EvidenceError(f"Encrypted ZIP member: {item.filename}")
        result[item.filename] = item
    return result


def member_bytes(archive, members):
    """Read an explicitly pinned chain, bounding every nested archive member."""
    if not members or len(members) > 5:
        raise EvidenceError("Invalid ZIP/PACK member chain")
    data = archive
    for name in members:
        relative_path(name)
        with zipfile.ZipFile(data) as container:
            entries = archive_members(container)
            if name not in entries or entries[name].is_dir():
                raise EvidenceError(f"Pinned archive member is missing: {name}")
            info = entries[name]
            if info.file_size > MAX_MEMBER_BYTES:
                raise EvidenceError(f"Oversized nested archive member: {name}")
            with container.open(info) as source:
                content = source.read(MAX_MEMBER_BYTES + 1)
            if len(content) > MAX_MEMBER_BYTES:
                raise EvidenceError(f"Oversized archive content: {name}")
            data = io.BytesIO(content)
    return data.getvalue()


def extract(root, archive, specs):
    # Validate even an archive whose extracted files already exist.
    with zipfile.ZipFile(archive) as container:
        archive_members(container)
    for spec in specs:
        target = destination(root, spec["path"])
        if target.exists():
            verify(target, spec)
            continue
        data = member_bytes(archive, spec["members"])
        with staging(target) as temp:
            temp.write_bytes(data)
            publish(temp, target, spec)


def make_text(root, pdf, spec, extractor):
    target = destination(root, spec["path"])
    if target.exists():
        verify(target, spec)
        return
    if shutil.which(extractor["program"]) is None:
        raise EvidenceError("pdftotext is required; install Poppler (verified version "
                            + extractor["verified_version"] + ") and rerun")
    with staging(target) as temp:
        result = subprocess.run([extractor["program"], *extractor["arguments"], str(pdf), str(temp)],
                                capture_output=True, text=True, timeout=120)
        if result.returncode:
            raise EvidenceError(f"pdftotext failed for {pdf.name}: {result.stderr.strip()}")
        try:
            publish(temp, target, spec)
        except EvidenceError as error:
            version = subprocess.run([extractor["program"], "-v"], capture_output=True, text=True)
            raise EvidenceError(f"{error}\nText must reproduce Poppler {extractor['verified_version']} output. "
                                f"Installed: {(version.stderr or version.stdout).splitlines()[0]}") from error


def normalized_metadata(name, value):
    if name == "official-datasheet-manifest.json":
        # The historical acquisition used absolute paths. The consumer uses
        # only each basename; preserve such an existing file without rewriting.
        return [{**row, "path": Path(row["path"]).name} for row in value]
    return value


def metadata(root, name, value, verify_only):
    target = destination(root, name)
    if target.exists():
        actual = json.loads(target.read_text())
        if normalized_metadata(name, actual) != normalized_metadata(name, value):
            raise EvidenceError(f"Generated source metadata changed: {target}; refusing to overwrite it")
        return
    if verify_only:
        raise EvidenceError(f"Missing generated source metadata: {target}")
    data = (json.dumps(value, indent=2, ensure_ascii=False) + "\n").encode()
    spec = {"sha256": hashlib.sha256(data).hexdigest(), "bytes": len(data)}
    with staging(target) as temp:
        temp.write_bytes(data)
        publish(temp, target, spec)


def load_manifest(path):
    manifest = json.loads(path.read_text())
    if manifest["schema_version"] != 1:
        raise EvidenceError("Unsupported evidence manifest schema")
    paths = set()
    for row in manifest["artifacts"]:
        check_url(row["url"])
        if row["kind"] not in {"sdk", "pdf", "html"}:
            raise EvidenceError(f"Unknown evidence kind: {row['kind']}")
        for spec in [row, *row.get("members", []), *([row["text"]] if "text" in row else [])]:
            relative_path(spec["path"])
            if spec["path"] in paths:
                raise EvidenceError(f"Duplicate output path: {spec['path']}")
            paths.add(spec["path"])
            if not re.fullmatch(r"[0-9a-f]{64}", spec["sha256"]) or not isinstance(spec["bytes"], int) or spec["bytes"] < 0:
                raise EvidenceError(f"Invalid hash/size: {spec['path']}")
            if spec is not row:
                for member in spec.get("members", []):
                    relative_path(member)
    for name in manifest["generated_metadata"]:
        relative_path(name)
        if name in paths:
            raise EvidenceError(f"Duplicate metadata path: {name}")
    validate_scope(manifest)
    return manifest


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-root", type=Path, default=Path(os.environ.get("CW32_SOURCES", ROOT / "sources/vendor")))
    parser.add_argument("--cache-root", type=Path, help="Optional existing source root; only SHA-verified original artifacts are copied")
    parser.add_argument("--offline", action="store_true", help="Never download; require verified existing/cache originals")
    parser.add_argument("--verify", action="store_true", help="Verify every output already exists; do not write or download")
    parser.add_argument("--originals-only", action="store_true", help="Acquire or verify original PDF/SDK/HTML files only; do not extract SDK members or generate text/reports")
    parser.add_argument("--only", action="append", metavar="PATH", help="Acquire one original artifact and its derivatives (repeatable); omit for complete evidence")
    parser.add_argument("--include-discovery", action="store_true", help="Strict archival replay: also require the two exact historical HTML snapshots")
    args = parser.parse_args(argv)
    manifest = load_manifest(MANIFEST)
    root = args.source_root.expanduser().absolute()
    cache = args.cache_root.expanduser().absolute() if args.cache_root else None
    for base in [root, *([cache] if cache else [])]:
        if base.is_symlink() or any(p.is_symlink() for p in base.parents):
            raise EvidenceError(f"Refusing symlinked source/cache root: {base}")
    artifacts = scope_artifacts(manifest, args.include_discovery)
    selected = set(args.only or [])
    known = {row["path"] for row in manifest["artifacts"]}
    if selected - known:
        raise EvidenceError("Unknown --only artifact: " + ", ".join(sorted(selected - known)))
    if selected - {row["path"] for row in artifacts}:
        raise EvidenceError("Discovery snapshots require --include-discovery for strict replay; use refresh_discovery.py for current observations")
    opener = urllib.request.build_opener(OfficialRedirect())
    counts = {"existing": 0, "cache": 0, "download": 0, "outputs": 0}
    for row in artifacts:
        if selected and row["path"] not in selected:
            continue
        specs = [row] if args.originals_only else [row, *row.get("members", []), *([row["text"]] if "text" in row else [])]
        if args.verify:
            for spec in specs:
                verify(destination(root, spec["path"]), spec)
            counts["existing"] += 1
            mode = "verified"
        else:
            original, mode = obtain(root, row, cache, args.offline, opener)
            counts[mode] += 1
            if not args.originals_only and row.get("members"):
                extract(root, original, row["members"])
            if not args.originals_only and "text" in row:
                make_text(root, original, row["text"], manifest["text_extractor"])
        counts["outputs"] += len(specs)
        print(f"{mode}: {row['path']}", flush=True)
    if not selected and not args.originals_only:
        for name, value in manifest["generated_metadata"].items():
            metadata(root, name, value, args.verify)
            counts["outputs"] += 1
    print(json.dumps({"source_root": str(root), "complete_manifest": args.include_discovery and not bool(selected) and not args.originals_only, "complete_hardware": not bool(selected) and not args.originals_only, "originals_only": args.originals_only, **scope_report(manifest, args.include_discovery), **counts}, sort_keys=True))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (EvidenceError, OSError, ValueError, KeyError, zipfile.BadZipFile, subprocess.SubprocessError) as error:
        print(f"Evidence acquisition failed: {error}", file=sys.stderr)
        raise SystemExit(1)
