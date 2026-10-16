"""Validate a release tag; optionally package only the app and sibling sidecar."""

import argparse
import hashlib
import json
import tomllib
import zipfile
from pathlib import Path


def validate_version(root, tag):
    version = json.loads((root / "src-tauri/tauri.conf.json").read_text(encoding="utf-8"))["version"]
    versions = {
        "package.json": json.loads((root / "package.json").read_text(encoding="utf-8"))["version"],
        "Cargo.toml": tomllib.loads((root / "src-tauri/Cargo.toml").read_text(encoding="utf-8"))["package"]["version"],
        "Cargo.lock": next(
            (p["version"] for p in tomllib.loads(
                (root / "src-tauri/Cargo.lock").read_text(encoding="utf-8")
            )["package"] if p["name"] == "lobster-pulse"), None
        ),
    }
    if any(value != version for value in versions.values()):
        raise ValueError(f"App versions must match tauri.conf.json version {version}: {versions}")
    if tag != f"v{version}":
        raise ValueError(f"Release tag must be v{version}, matching the app version; got {tag!r}")
    return version


def package_release(root, tag, platform, build_dir, output_dir):
    validate_version(root, tag)
    suffix = ".exe" if platform == "windows" else ""
    binaries = [build_dir / (name + suffix) for name in ("lobster-pulse", "lobster-pulse-hook")]
    for binary in binaries:
        if not binary.is_file():
            raise FileNotFoundError(f"Missing required binary: {binary}")
    output_dir.mkdir(parents=True, exist_ok=True)
    archive = output_dir / f"lobster-pulse-{tag}-{platform}.zip"
    with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as bundle:
        for binary in binaries:
            bundle.write(binary, arcname=binary.name)
    with archive.open("rb") as source:
        digest = hashlib.file_digest(source, "sha256").hexdigest()
    checksum = archive.with_suffix(".zip.sha256")
    checksum.write_text(f"{digest}  {archive.name}\n", encoding="utf-8", newline="\n")
    return archive, checksum


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("tag", help="existing tag matching the canonical Tauri app version")
    parser.add_argument("--platform", choices=("windows", "linux", "macos-arm64", "macos-x64"))
    parser.add_argument("--build-dir", type=Path, default=Path("src-tauri/target/release"))
    parser.add_argument("--output-dir", type=Path, default=Path("."))
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    try:
        validate_version(root, args.tag)
        if args.platform:
            for artifact in package_release(root, args.tag, args.platform, args.build_dir, args.output_dir):
                print(artifact)
        else:
            print(f"Validated {args.tag}: all app versions match tauri.conf.json")
    except (OSError, ValueError, KeyError) as error:
        parser.exit(1, f"Release validation failed: {error}\n")


if __name__ == "__main__":
    main()
