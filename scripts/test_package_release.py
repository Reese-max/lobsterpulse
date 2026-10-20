import hashlib
import json
import subprocess
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path

from scripts.package_release import package_release, validate_version


class ReleasePackageTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / "src-tauri").mkdir()
        for name in ("package.json", "src-tauri/tauri.conf.json"):
            (self.root / name).write_text(json.dumps({"version": "0.5.4"}), encoding="utf-8")
        (self.root / "src-tauri/Cargo.toml").write_text(
            '[package]\nname = "lobster-pulse"\nversion = "0.5.4"\n', encoding="utf-8"
        )
        (self.root / "src-tauri/Cargo.lock").write_text(
            'version = 3\n[[package]]\nname = "lobster-pulse"\nversion = "0.5.4"\n',
            encoding="utf-8",
        )
        (self.root / "CHANGELOG.md").write_text(
            "# Changelog\n\n## v0.5.4 · 2026-04-17 — fixture\n- notes\n", encoding="utf-8"
        )
        self.build = self.root / "build with spaces"
        self.build.mkdir()
        self.output = self.root / "output"

    def binaries(self, suffix):
        for name in ("lobster-pulse", "lobster-pulse-hook"):
            binary = self.build / (name + suffix)
            binary.write_bytes((name + " fixture").encode())
            binary.chmod(0o755)
        (self.build / "unrelated-secret.txt").write_text("must not ship", encoding="utf-8")

    def test_canonical_version_accepts_exact_tag(self):
        self.assertEqual(validate_version(self.root, "v0.5.4"), "0.5.4")

    def test_wrong_or_missing_tag_is_rejected(self):
        for tag in ("main", "", "0.5.4", "v0.5.5", "v0.5.4/../../escape"):
            with self.subTest(tag=tag), self.assertRaisesRegex(ValueError, "tag"):
                validate_version(self.root, tag)

    def test_every_mismatched_version_surface_is_rejected(self):
        for name in ("package.json", "src-tauri/Cargo.toml", "src-tauri/Cargo.lock"):
            file = self.root / name
            original = file.read_text(encoding="utf-8")
            file.write_text(original.replace("0.5.4", "0.2.2"), encoding="utf-8")
            with self.subTest(file=name), self.assertRaisesRegex(ValueError, "version"):
                validate_version(self.root, "v0.5.4")
            file.write_text(original, encoding="utf-8")

    def test_tag_without_changelog_section_is_rejected(self):
        changelog = self.root / "CHANGELOG.md"
        for text in ("# Changelog\n", "### v0.5.4 · nested\n", "## v0.5.40 · other\n", "## v0.5.4-rc1\n",
                     "## v0.5.4　· awk does not split on U+3000\n"):
            changelog.write_text(text, encoding="utf-8")
            with self.subTest(text=text), self.assertRaisesRegex(ValueError, "CHANGELOG"):
                validate_version(self.root, "v0.5.4")

    def test_bracketed_changelog_heading_is_accepted(self):
        (self.root / "CHANGELOG.md").write_text("## [v0.5.4] - 2026-04-17\n", encoding="utf-8")
        self.assertEqual(validate_version(self.root, "v0.5.4"), "0.5.4")

    def test_portable_archives_have_only_sibling_binaries_and_valid_checksum(self):
        for platform in ("windows", "linux", "macos-arm64", "macos-x64"):
            suffix = ".exe" if platform == "windows" else ""
            self.binaries(suffix)
            with self.subTest(platform=platform):
                archive, checksum = package_release(
                    self.root, "v0.5.4", platform, self.build, self.output
                )
                self.assertEqual(archive.name, f"lobster-pulse-v0.5.4-{platform}.zip")
                self.assertEqual(checksum.name, archive.name + ".sha256")
                with zipfile.ZipFile(archive) as bundle:
                    self.assertIsNone(bundle.testzip())
                    self.assertEqual(bundle.namelist(), [
                        "lobster-pulse" + suffix, "lobster-pulse-hook" + suffix
                    ])
                    for name in bundle.namelist():
                        self.assertEqual(bundle.read(name), (self.build / name).read_bytes())
                self.assertEqual(
                    checksum.read_bytes(),
                    (hashlib.sha256(archive.read_bytes()).hexdigest() + "  " + archive.name + "\n").encode(),
                )

    def test_missing_either_binary_fails_before_creating_output(self):
        for missing in ("lobster-pulse.exe", "lobster-pulse-hook.exe"):
            self.binaries(".exe")
            (self.build / missing).unlink()
            with self.subTest(missing=missing), self.assertRaises(FileNotFoundError):
                package_release(self.root, "v0.5.4", "windows", self.build, self.output)
            self.assertFalse(self.output.exists())

    def test_empty_binary_fails_before_creating_output(self):
        for empty in ("lobster-pulse.exe", "lobster-pulse-hook.exe"):
            self.binaries(".exe")
            (self.build / empty).write_bytes(b"")
            with self.subTest(empty=empty), self.assertRaisesRegex(ValueError, "empty"):
                package_release(self.root, "v0.5.4", "windows", self.build, self.output)
            self.assertFalse(self.output.exists())

    def test_invalid_tag_does_not_create_archive(self):
        self.binaries(".exe")
        with self.assertRaises(ValueError):
            package_release(self.root, "v99.0.0", "windows", self.build, self.output)
        self.assertFalse(self.output.exists())

    def test_repackaging_replaces_stale_archive_entries(self):
        self.binaries(".exe")
        archive, _ = package_release(self.root, "v0.5.4", "windows", self.build, self.output)
        with zipfile.ZipFile(archive, "a") as bundle:
            bundle.writestr("stale.exe", b"old")
        package_release(self.root, "v0.5.4", "windows", self.build, self.output)
        with zipfile.ZipFile(archive) as bundle:
            self.assertNotIn("stale.exe", bundle.namelist())

    def test_cli_checks_real_repository_and_rejects_branch_names(self):
        root = Path(__file__).resolve().parents[1]
        version = json.loads((root / "src-tauri/tauri.conf.json").read_text(encoding="utf-8"))["version"]
        for tag, expected in (("v" + version, 0), ("main", 1)):
            result = subprocess.run(
                [sys.executable, str(root / "scripts/package_release.py"), tag],
                cwd=self.root, capture_output=True, text=True,
            )
            with self.subTest(tag=tag):
                self.assertEqual(result.returncode, expected, result.stdout + result.stderr)
                self.assertIn("version" if expected else tag, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
