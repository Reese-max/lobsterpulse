#!/usr/bin/env python3
"""
護衛鏈過期契約審計護衛 — 5 case pytest, 對齊 R172 M2 補時間維度量測

R172 落地。1 個 Python pytest 模組, 走 R132/R137 同樣模式
(純量化腳本 + 5 case pytest, 不破 K42 chain 20 Rust 護衛飽和契約)。

5 case:
  1. test_量測_回傳_16_個_test_檔 — 對齊 src-tauri/src 16 個 test-bearing 檔
  2. test_守衛鏈_計數_健康檢查 — chain_count_min 20 守住
  3. test_過期判定_stale_超過_threshold — mock now_unix 提前 STALE_DAYS*2
  4. test_過期判定_fresh_未超過_threshold — mock now_unix = last_commit
  5. test_整體_fail_closed_stale_觸發 — 任何 stale 觸發 overall_pass=False
"""
import json
import sys
from pathlib import Path

import pytest

SCRIPT = Path(__file__).resolve().parent / "chain_staleness.py"
sys.path.insert(0, str(SCRIPT.parent))

import chain_staleness as cs  # noqa: E402

REPO_SRC = cs.SRC_TAURI


# ---------- 1. 量測主路徑 ----------

def test_量測_回傳_16_個_test_檔():
    """src-tauri/src 含 #\[test\] 檔應 = 16 (跟 R172 grep 對齊)"""
    results = cs.measure(src_root=REPO_SRC)
    assert len(results) == 16, f"預期 16 個 test 檔, 實際 {len(results)}: {[r.relpath for r in results]}"


# ---------- 2. 計數健康檢查 ----------

def test_守衛鏈_計數_健康檢查_守住_20():
    """所有 test 檔的 test_fn 總和應 >= 20 (R97 紅線 chain 計數下限)"""
    results = cs.measure(src_root=REPO_SRC)
    total = sum(r.test_fn_count for r in results)
    assert total >= cs.CHAIN_COUNT_MIN, f"chain 計數 {total} < {cs.CHAIN_COUNT_MIN}"


# ---------- 3. 過期判定: 提前到 stale ----------

def test_過期判定_stale_超過_threshold():
    """mock now_unix 提前到「每個檔案最後 commit + STALE_DAYS*2」 → 全 stale"""
    results = cs.measure(src_root=REPO_SRC)
    fresh_results = [r for r in results if r.last_commit_unix > 0]
    assert fresh_results, "應至少有 1 個檔有真實 commit time"
    earliest = min(r.last_commit_unix for r in fresh_results)
    far_future = earliest + cs.STALE_DAYS * 2 * 86400

    stale_results = cs.measure(src_root=REPO_SRC, now_unix=far_future)
    stale_count = sum(1 for r in stale_results if r.is_stale)
    assert stale_count == len(fresh_results), (
        f"提前到 +{cs.STALE_DAYS*2}d 應全 stale, 實際 {stale_count}/{len(fresh_results)}"
    )


# ---------- 4. 過期判定: 同時刻 = fresh ----------

def test_過期判定_fresh_未超過_threshold():
    """mock now_unix = 最後 commit unix time → 該檔 days=0, is_stale=False"""
    results = cs.measure(src_root=REPO_SRC)
    fresh_results = [r for r in results if r.last_commit_unix > 0]
    assert fresh_results, "應至少有 1 個檔有真實 commit time"
    target = fresh_results[0]

    remeasured = cs.measure(src_root=REPO_SRC, now_unix=target.last_commit_unix)
    matched = next(r for r in remeasured if r.relpath == target.relpath)
    assert matched.days_since_last_commit == 0
    assert matched.is_stale is False


# ---------- 5. 整體 fail-closed: stale 觸發 FAIL ----------

def test_整體_fail_closed_stale_觸發():
    """任一檔 is_stale=True → overall_pass=False"""
    fake = cs.GuardFileStale(
        relpath="src-tauri/src/fake.rs",
        test_fn_count=5,
        last_commit_unix=1_000_000_000,  # 2001-09-09
        last_commit_iso="2001-09-09T01:46:40Z",
        days_since_last_commit=9000,
        is_stale=True,
    )
    fresh = cs.GuardFileStale(
        relpath="src-tauri/src/lib.rs",
        test_fn_count=151,
        last_commit_unix=1_700_000_000,
        last_commit_iso="2023-11-14T22:13:20Z",
        days_since_last_commit=10,
        is_stale=False,
    )
    assert cs.overall_pass([fake, fresh]) is False
    assert cs.overall_pass([fresh]) is True
    # chain 計數 < CHAIN_COUNT_MIN 也 fail
    tiny = cs.GuardFileStale(
        relpath="src-tauri/src/tiny.rs",
        test_fn_count=cs.CHAIN_COUNT_MIN - 1,
        last_commit_unix=1_700_000_000,
        last_commit_iso="2023-11-14T22:13:20Z",
        days_since_last_commit=10,
        is_stale=False,
    )
    assert cs.overall_pass([tiny]) is False


# ---------- 6-8. R179 護衛本體健康測試: 守住 3 個硬編碼契約 ----------
# 對齊 R176/R177 R13 軸模式: 護衛腳本本身不被亂改, baseline 對齊。
# chain_staleness.py 有 3 個關鍵硬編碼 (STALE_DAYS / CHAIN_COUNT_MIN / _TEST_MARKER_RE),
# 若被偷改, 量測結果會 silent pass 過期契約, 但 R97 紅線守不住。
# 加 3 個本體健康測試守住這 3 個契約, 對齊 R176 r124_sentinel tuple + R177 cargo test 計數 模式。

def test_本體_STALE_DAYS_守住_90天():
    """R172 設定 STALE_DAYS=90, 不讓人偷改成過大 (放水) 或過小 (誤殺) 閾值。
    對齊 MISSION R133+ 接力護衛 過期契約審計 90 天契約。
    """
    assert cs.STALE_DAYS == 90, (
        f"STALE_DAYS 應守住 90 (R172 契約), 實際 {cs.STALE_DAYS}. "
        f"若要改閾值, 需先在 MISSION 補頁或新開 spec change 提案。"
    )


def test_本體_CHAIN_COUNT_MIN_守住_20():
    """CHAIN_COUNT_MIN 對齊 R97 紅線 K42 chain 20 條飽和契約下限。
    若被偷降, overall_pass 會 silent pass chain 計數 < 20, 違反 R97。
    """
    assert cs.CHAIN_COUNT_MIN == 20, (
        f"CHAIN_COUNT_MIN 應守住 20 (R97 紅線 K42 chain 下限), 實際 {cs.CHAIN_COUNT_MIN}. "
        f"chain 計數下限歸 r124_sentinel 守, 這裡只防 silent pass 放水。"
    )


def test_本體_TEST_MARKER_RE_守住_純_marker_pattern():
    """_TEST_MARKER_RE 是 chain_staleness 識別 #[test] marker 的唯一契約。
    若 regex 被改寬 (含 #[bench] / #[ignore] 等), 會把非 test fn 算進 chain 計數。
    若被改嚴, 會漏算真實 test fn, 護衛 silently pass 失真。
    pattern 對齊 R170/R172: 純粹 `#[test]` marker (含前置空白), 不含 `#[cfg(test)]`。
    """
    import re as _re
    expected = _re.compile(r"^\s*#\[test\]\s*$", _re.MULTILINE)
    assert cs._TEST_MARKER_RE.pattern == expected.pattern, (
        f"_TEST_MARKER_RE pattern 漂移: 預期 {expected.pattern!r}, "
        f"實際 {cs._TEST_MARKER_RE.pattern!r}."
    )
    assert cs._TEST_MARKER_RE.flags == expected.flags, (
        f"_TEST_MARKER_RE flags 漂移: 預期 {expected.flags}, 實際 {cs._TEST_MARKER_RE.flags}."
    )
