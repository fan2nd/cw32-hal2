#!/usr/bin/env python3
"""Offline security/recovery tests for external evidence acquisition."""
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import stat
import tempfile
import unittest
import zipfile
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
module_spec = importlib.util.spec_from_file_location("acquire_evidence", ROOT / "cw32-data/tools/acquire_evidence.py")
a = importlib.util.module_from_spec(module_spec)
module_spec.loader.exec_module(a)


def spec(data, path="vendor.zip"):
    return {"path": path, "sha256": hashlib.sha256(data).hexdigest(), "bytes": len(data),
            "url": "https://www.whxy.com/uploads/files/test/vendor.zip"}


def zip_bytes(entries):
    output = io.BytesIO()
    with zipfile.ZipFile(output, "w") as archive:
        for name, data in entries:
            archive.writestr(name, data)
    return output.getvalue()


class NeverNetwork:
    def open(self, *args, **kwargs):
        raise AssertionError("offline/existing reads must not access the network")


class AcquisitionTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="cw32-evidence-unit-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / "output"
        self.root.mkdir()
        self.cache = Path(self.temp.name) / "cache"
        self.cache.mkdir()

    def test_complete_manifest_is_well_formed(self):
        manifest = a.load_manifest(a.MANIFEST)
        self.assertEqual(len(manifest["artifacts"]), 45)
        self.assertEqual(sum(row["kind"] == "sdk" for row in manifest["artifacts"]), 12)
        l011 = next(row for row in manifest["artifacts"] if row["path"] == "CW32L011_UserManual_CN_V1.1.pdf")
        self.assertEqual(l011["sha256"], "b245887e9037caf267f739e25bd3c354c2579d23315fc06389e055f371159f4f")
        self.assertIn("20260602", l011["url"])
        nxp = next(row for row in manifest["artifacts"] if row["path"] == "NXP_UM10204_Rev7.pdf")
        self.assertEqual(nxp["sha256"], "dc91f00f65584e06ef36e26c93bf9d91a95fb3c8a1830a9223e53caf678b36af")
        a.check_url(nxp["url"])

    def test_hal_source_audits_are_covered_by_the_lock(self):
        manifest = a.load_manifest(a.MANIFEST)
        pinned = {}
        for row in manifest["artifacts"]:
            for item in [row, *row.get("members", []), *([row["text"]] if "text" in row else [])]:
                pinned[item["path"]] = item["sha256"]
        def check(value):
            if isinstance(value, list):
                for item in value:
                    check(item)
            if not isinstance(value, dict):
                return
            for key in ("path", "file", "artifact", "filename", "source"):
                path = value.get(key)
                if isinstance(path, str) and "sha256" in value:
                    if path.startswith("/workspace/shared/cw32-sources/"):
                        path = path.removeprefix("/workspace/shared/cw32-sources/")
                    if path in pinned or Path(path).suffix in (".pdf", ".html") or "/Libraries/" in path:
                        self.assertIn(path, pinned)
                        self.assertEqual(pinned[path], value["sha256"], path)
                        if "text_sha256" in value:
                            self.assertEqual(pinned[str(Path(path).with_suffix(".txt"))], value["text_sha256"])
            for item in value.values():
                check(item)
        for name in ("watchdog-evidence", "crc-evidence", "window-watchdog-evidence", "gpio-async-evidence",
                     "l011-manual-follow-up-evidence", "remaining-uart-evidence", "l031-r031-w031-uart-evidence",
                     "final-serial-af-evidence", "remaining-serial-af-evidence", "l031-shared-serial-evidence",
                     "gpio-isr-access-corrections", "timer-adc-isr-access-corrections"):
            check(json.loads((ROOT / "docs" / (name + ".json")).read_text()))

    def test_shared_serial_archive_member_inputs_are_covered(self):
        manifest = a.load_manifest(a.MANIFEST)
        for family in json.loads((ROOT / "docs/l031-shared-serial-evidence.json").read_text())["families"]:
            archive = next(row for row in manifest["artifacts"] if row.get("family") == family["family"])
            for role in ("spi_implementation", "i2c_implementation"):
                source = family["sources"][role]
                members = [row for row in archive["members"] if row["path"].endswith("/" + source["member"])]
                self.assertEqual(len(members), 1, (family["family"], role))
                self.assertEqual(members[0]["sha256"], source["sha256"])

    def test_path_traversal_absolute_backslash_and_drives_rejected(self):
        for path in ("../evil", "/tmp/evil", "good/../../evil", "a\\b", "C:/evil", "a//b", "a/./b", "a/", ""):
            with self.subTest(path=path), self.assertRaises(a.EvidenceError):
                a.destination(self.root, path)

    def test_destination_symlink_and_symlink_parent_rejected(self):
        (self.root / "link").symlink_to(self.cache, target_is_directory=True)
        for path in ("link", "link/file"):
            with self.assertRaises(a.EvidenceError):
                a.destination(self.root, path)

    def test_offline_verified_cache_recovery_and_idempotence(self):
        data = b"pinned bytes"
        source = self.cache / "vendor.zip"
        source.write_bytes(data)
        target, origin = a.obtain(self.root, spec(data), self.cache, True, NeverNetwork())
        self.assertEqual(origin, "cache")
        self.assertEqual(target.read_bytes(), data)
        self.assertNotEqual(target.stat().st_ino, source.stat().st_ino, "cache is copied, avoiding linked human edits")
        before = target.stat().st_mtime_ns
        _, origin = a.obtain(self.root, spec(data), self.cache, True, NeverNetwork())
        self.assertEqual(origin, "existing")
        self.assertEqual(target.stat().st_mtime_ns, before)

    def test_changed_existing_file_is_preserved(self):
        path = self.root / "vendor.zip"
        path.write_bytes(b"human edits")
        with self.assertRaises(a.EvidenceError):
            a.obtain(self.root, spec(b"official bytes"), self.cache, False, NeverNetwork())
        self.assertEqual(path.read_bytes(), b"human edits")

    def test_changed_cache_fails_closed_no_output(self):
        (self.cache / "vendor.zip").write_bytes(b"changed source")
        with self.assertRaises(a.EvidenceError):
            a.obtain(self.root, spec(b"official bytes"), self.cache, False, NeverNetwork())
        self.assertEqual(list(self.root.iterdir()), [])

    def test_missing_offline_does_not_download(self):
        with self.assertRaisesRegex(a.EvidenceError, "Missing offline evidence"):
            a.obtain(self.root, spec(b"source"), self.cache, True, NeverNetwork())
        self.assertEqual(list(self.root.iterdir()), [])

    def test_changed_download_never_publishes(self):
        class Response(io.BytesIO):
            def geturl(self):
                return "https://www.whxy.com/vendor.zip"
        class Opener:
            def open(self, *args, **kwargs):
                return Response(b"bad")
        with self.assertRaises(a.EvidenceError):
            a.obtain(self.root, spec(b"abc"), None, False, Opener())
        self.assertEqual(list(self.root.iterdir()), [])

    def test_oversized_transfer_rejected(self):
        with self.assertRaises(a.EvidenceError):
            a.transfer(io.BytesIO(b"abcd"), self.root / "temp", 3)

    def test_only_official_https_allowed(self):
        for url in ("http://www.whxy.com/a", "https://evil.example/a", "https://www.whxy.com.evil/a", "https://user@www.whxy.com/a", "file:///tmp/a", "https://www.whxy.com:444/a", "https://cache.nxp.com.evil/a", "https://user@cache.nxp.com/a", "http://cache.nxp.com/a"):
            with self.subTest(url=url), self.assertRaises(a.EvidenceError):
                a.check_url(url)

    def test_redirect_is_checked_before_request(self):
        with self.assertRaises(a.EvidenceError):
            a.OfficialRedirect().redirect_request(None, None, 302, "Found", {}, "https://evil.example/other")

    def test_exact_nested_pack_member(self):
        data = b"CMSIS header"
        archive = self.root / "sdk.zip"
        archive.write_bytes(zip_bytes([("inner.pack", zip_bytes([("Device/chip.h", data)]))]))
        row = {**spec(data, "sdk/Libraries/inc/chip.h"), "members": ["inner.pack", "Device/chip.h"]}
        a.extract(self.root, archive, [row])
        self.assertEqual((self.root / row["path"]).read_bytes(), data)
        (self.root / row["path"]).write_bytes(b"edited")
        with self.assertRaises(a.EvidenceError):
            a.extract(self.root, archive, [row])
        self.assertEqual((self.root / row["path"]).read_bytes(), b"edited")

    def test_zip_slip_duplicate_symlink_and_special_members_rejected(self):
        for entry in ("../escape", "/absolute", "C:/drive", "a\\b"):
            with zipfile.ZipFile(io.BytesIO(zip_bytes([(entry, b"bad")]))) as archive:
                with self.subTest(entry=entry), self.assertRaises(a.EvidenceError):
                    a.archive_members(archive)
        for kind in (stat.S_IFLNK, stat.S_IFIFO):
            entry = zipfile.ZipInfo("special")
            entry.create_system = 3
            entry.external_attr = (kind | 0o777) << 16
            with zipfile.ZipFile(io.BytesIO(zip_bytes([(entry, b"target")]))) as archive:
                with self.assertRaises(a.EvidenceError):
                    a.archive_members(archive)
        with self.assertWarns(UserWarning):
            data = zip_bytes([("same", b"a"), ("same", b"b")])
        with zipfile.ZipFile(io.BytesIO(data)) as archive:
            with self.assertRaises(a.EvidenceError):
                a.archive_members(archive)

    def test_manifest_cannot_redirect_output_outside_root(self):
        manifest = json.loads(a.MANIFEST.read_text())
        manifest["artifacts"][0]["members"][0]["path"] = "../escape"
        path = self.root / "bad.json"
        path.write_text(json.dumps(manifest))
        with self.assertRaises(a.EvidenceError):
            a.load_manifest(path)

    def test_metadata_idempotence_and_human_edit_preservation(self):
        name = "official-sdk-urls.json"
        value = {"CW32F030": "https://www.whxy.com/official.zip"}
        a.metadata(self.root, name, value, False)
        path = self.root / name
        path.write_text(json.dumps(value, separators=(",", ":")))
        before = path.read_bytes()
        a.metadata(self.root, name, value, False)
        self.assertEqual(path.read_bytes(), before)
        path.write_text('{"changed": true}')
        with self.assertRaises(a.EvidenceError):
            a.metadata(self.root, name, value, False)
        self.assertEqual(path.read_text(), '{"changed": true}')

    def test_pdf_text_hash_mismatch_preserves_preexisting_text(self):
        pdf = self.root / "manual.pdf"
        pdf.write_bytes(b"pdf")
        target = self.root / "manual.txt"
        target.write_bytes(b"human edit")
        with patch.object(a.subprocess, "run", side_effect=AssertionError("must not regenerate")):
            with self.assertRaises(a.EvidenceError):
                a.make_text(self.root, pdf, spec(b"official", "manual.txt"), {"program": "pdftotext"})
        self.assertEqual(target.read_bytes(), b"human edit")


if __name__ == "__main__":
    unittest.main()
