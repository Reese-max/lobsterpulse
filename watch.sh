#!/bin/bash
# Dev mode with frontend hot reload via devUrl
# - Starts static file server for src/
# - Launches Tauri pointing at localhost:1420
# - Any frontend change: just reload the window (Ctrl+R or right-click → Reload)
# - Rust changes: Ctrl+C and run again
#
# Frontend changes do NOT require rebuilding the binary.

set -e
cd "$(dirname "$0")"

echo "→ Killing any running instance..."
pkill -9 -x lobster-pulse 2>/dev/null || true
pkill -f "serve.*1420" 2>/dev/null || true
sleep 1

echo "→ Starting cargo tauri dev (will spawn file server on :1420)..."
cargo tauri dev
