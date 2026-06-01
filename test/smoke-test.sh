#!/usr/bin/env bash
# ============================================================
#  smoke-test.sh — LobsterPulse（監控）自治迴圈的編譯驗證 gate
#  engineer-loop 每輪有改動時跑此檔；非 0 退出 → 自動 rollback 該輪改動。
#  以 180s timeout 執行，故用最快的 `cargo check`（編譯層驗證，README 認可的驗證方式）。
#  用法：smoke-test.sh [quick|full]
# ============================================================
set -uo pipefail
cd "$(dirname "$0")/.." || { echo "[smoke] FAIL: cannot cd to project root"; exit 1; }
MODE="${1:-quick}"

echo "[smoke] mode=$MODE — cargo check (src-tauri)..."
if ! ( cd src-tauri && cargo check --quiet ) 2>&1; then
    echo "[smoke] FAIL: cargo check 編譯失敗"
    exit 1
fi

echo "[smoke] PASS (cargo check 綠)"
exit 0
