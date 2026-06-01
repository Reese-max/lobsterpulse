#!/bin/bash
# Official release build — binary + installer bundles.
#
# Produces:
#   src-tauri/target/release/lobster-pulse                     (binary)
#   src-tauri/target/release/bundle/deb/*.deb
#   src-tauri/target/release/bundle/rpm/*.rpm
#   src-tauri/target/release/bundle/appimage/*.AppImage
#
# IMPORTANT: always use `cargo tauri build` for releases, NOT `cargo build --release`.
# Plain cargo build skips frontend embedding — the webview will fall back to
# devUrl (localhost:1420) and show "Could not connect to localhost".

set -e
cd "$(dirname "$0")"

echo "→ Killing any running instance..."
pkill -9 -x lobster-pulse 2>/dev/null || true
sleep 1

echo "→ Building release (cargo tauri build)..."
cargo tauri build

echo
echo "✓ Done. Outputs:"
ls -lh src-tauri/target/release/lobster-pulse 2>/dev/null
ls -lh src-tauri/target/release/bundle/deb/*.deb 2>/dev/null || true
ls -lh src-tauri/target/release/bundle/rpm/*.rpm 2>/dev/null || true
ls -lh src-tauri/target/release/bundle/appimage/*.AppImage 2>/dev/null || true
