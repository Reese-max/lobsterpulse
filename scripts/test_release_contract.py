"""Regression checks for release version and portable archive requirements."""

import hashlib
import json
import pathlib
import shutil
import tempfile
import unittest
import zipfile

from release_contract import ROOT, app_version, check_tag, package_release


class ReleaseContractTests(unittest.TestCase):
    def test_version_surfaces_and_tag(self):
        version = app_version()
        self.assertEqual(check_tag(f"v{version}"), version)
        with self.assertRaisesRegex(ValueError, "does not match app version"):
            check_tag("v0.0.0")

    def test_mismatched_manifest_fails(self):
        with tempfile.TemporaryDirectory() as temp:
            root = pathlib.Path(temp)
            for name in ("src-tauri/Cargo.toml", "src-tauri/Cargo.lock", "src-tauri/tauri.conf.json",
                         "package.json", "README.md", "docs/index.html"):
                destination = root / name
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(ROOT / name, destination)
            package = json.loads((root / "package.json").read_text(encoding="utf-8"))
            package["version"] = "9.9.9"
            (root / "package.json").write_text(json.dumps(package), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "app version mismatch"):
                app_version(root)

    def test_windows_archive_requires_both_binaries_and_valid_checksum(self):
        with tempfile.TemporaryDirectory() as temp:
            folder = pathlib.Path(temp)
            build_dir = folder / "build"
            build_dir.mkdir()
            (build_dir / "lobster-pulse.exe").write_bytes(b"main fixture")
            with self.assertRaisesRegex(ValueError, "missing or empty"):
                package_release(f"v{app_version()}", "windows", build_dir, folder)
            (build_dir / "lobster-pulse-hook.exe").write_bytes(b"sidecar fixture")
            archive, checksum = package_release(f"v{app_version()}", "windows", build_dir, folder)
            with zipfile.ZipFile(archive) as zipped:
                self.assertEqual(sorted(zipped.namelist()), ["lobster-pulse-hook.exe", "lobster-pulse.exe"])
                self.assertEqual(zipped.read("lobster-pulse-hook.exe"), b"sidecar fixture")
            self.assertEqual(
                checksum.read_bytes(),
                f"{hashlib.sha256(archive.read_bytes()).hexdigest()}  {archive.name}\n".encode("ascii"),
            )


if __name__ == "__main__":
    unittest.main()
