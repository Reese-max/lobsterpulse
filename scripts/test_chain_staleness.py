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
