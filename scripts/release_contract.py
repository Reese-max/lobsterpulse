"""Check the app version and prepare a two-binary portable release archive."""

import argparse
import hashlib
import json
import pathlib
import sys
import tomllib
import zipfile


ROOT = pathlib.Path(__file__).resolve().parents[1]


def app_version(root: pathlib.Path = ROOT) -> str:
    cargo = tomllib.loads((root / "src-tauri/Cargo.toml").read_text(encoding="utf-8"))
    version = cargo["package"]["version"]
    lock = tomllib.loads((root / "src-tauri/Cargo.lock").read_text(encoding="utf-8"))
    lock_version = next((p["version"] for p in lock["package"] if p["name"] == cargo["package"]["name"]), None)
    tauri = json.loads((root / "src-tauri/tauri.conf.json").read_text(encoding="utf-8"))
    package = json.loads((root / "package.json").read_text(encoding="utf-8"))
    if lock_version != version or tauri["version"] != version or package["version"] != version:
        raise ValueError(
            f"app version mismatch: Cargo={version}, Cargo.lock={lock_version}, Tauri={tauri['version']}, "
            f"package.json={package['version']}"
        )
    docs = (root / "docs/index.html").read_text(encoding="utf-8")
    readme = (root / "README.md").read_text(encoding="utf-8")
    if f"原始碼版本：<strong>v{version}</strong>" not in docs:
        raise ValueError("docs landing page does not label the current source version")
    if f"原始碼中的 app 版本是 `{version}`" not in readme:
        raise ValueError("README does not label the current source version")
    return version


def check_tag(tag: str, root: pathlib.Path = ROOT) -> str:
    version = app_version(root)
    if tag != f"v{version}":
        raise ValueError(f"tag {tag!r} does not match app version v{version}")
    changelog = (root / "CHANGELOG.md").read_text(encoding="utf-8")
    if f"## {tag} " not in changelog and f"## [{tag}]" not in changelog:
        raise ValueError(f"CHANGELOG.md has no {tag} section")
    return version


def package_release(tag: str, platform: str, build_dir: pathlib.Path,
                    output_dir: pathlib.Path, root: pathlib.Path = ROOT) -> tuple[pathlib.Path, pathlib.Path]:
    check_tag(tag, root)
    if platform not in {"windows", "linux", "macos-arm64", "macos-x64"}:
        raise ValueError(f"unsupported platform: {platform}")
    suffix = ".exe" if platform == "windows" else ""
    names = [f"lobster-pulse{suffix}", f"lobster-pulse-hook{suffix}"]
    for name in names:
        path = build_dir / name
        if not path.is_file() or path.stat().st_size == 0:
            raise ValueError(f"missing or empty release binary: {path}")
    output_dir.mkdir(parents=True, exist_ok=True)
    archive = output_dir / f"lobster-pulse-{tag}-{platform}.zip"
    with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as out:
        for name in names:
            out.write(build_dir / name, arcname=name)
    with zipfile.ZipFile(archive) as check:
        if sorted(check.namelist()) != sorted(names) or check.testzip() is not None:
            raise ValueError("release archive verification failed")
    checksum = output_dir / f"{archive.name}.sha256"
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    checksum.write_text(f"{digest}  {archive.name}\n", encoding="ascii")
    return archive, checksum


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", help="release tag, required when packaging")
    parser.add_argument("--platform", choices=["windows", "linux", "macos-arm64", "macos-x64"])
    parser.add_argument("--build-dir", type=pathlib.Path)
    parser.add_argument("--output-dir", type=pathlib.Path, default=ROOT)
    args = parser.parse_args()
    try:
        if args.platform or args.build_dir:
            if not (args.tag and args.platform and args.build_dir):
                parser.error("packaging requires --tag, --platform and --build-dir")
            archive, checksum = package_release(args.tag, args.platform, args.build_dir, args.output_dir)
            print(f"verified {archive.name} and {checksum.name}")
        elif args.tag:
            print(f"verified app version {check_tag(args.tag)} and {args.tag}")
        else:
            print(f"verified app version {app_version()} across Cargo, Cargo.lock, Tauri, package.json, README and docs")
    except ValueError as exc:
        print(f"release contract failed: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
