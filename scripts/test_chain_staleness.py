#!/usr/bin/env python3
"""
護衛鏈過期契約審計護衛 — 11 case pytest, 對齊 R172/R180/R195 三階段
M2 補時間維度量測 closure 軸

R172 5 case + R180 3 case (本體健康 = 量化口徑常數) + R195 3 case
(內部函式 hidden gap = 路徑/排除/正則邊界守護), 共 11 case 守 9 個
hidden gap。 1 個 Python pytest 模組, 走 R132/R137 同樣模式 (純量化
腳本 + pytest, 不破 K42 chain 20 Rust 護衛飽和契約)。

R172 5 case:
  1. test_量測_回傳_17_個_test_檔 — 對齊 src-tauri/src 17 個 test-bearing 檔
     (R172 grep 16 + T-OGRE15 telemetry.rs)
  2. test_守衛鏈_計數_健康檢查 — chain_count_min 20 守住
  3. test_過期判定_stale_超過_threshold — mock now_unix 提前 STALE_DAYS*2
  4. test_過期判定_fresh_未超過_threshold — mock now_unix = last_commit
  5. test_整體_fail_closed_stale_觸發 — 任何 stale 觸發 overall_pass=False

R180 3 case (本體健康 = 量化口徑常數):
  6. test_本體_STALE_DAYS_守住_90天 — R172 過期閾值契約
  7. test_本體_CHAIN_COUNT_MIN_守住_20 — R97 紅線 chain 下限
  8. test_本體_TEST_MARKER_RE_守住_純_marker_pattern — #\[test\] 識別契約

R195 3 case (內部函式 hidden gap 守護):
  9. test_本體_SRC_TAURI_路徑_對齊_src_tauri_src — 路徑不漂移
 10. test_iter_test_files_排除_target_子樹 — 防 build artifact 污染
 11. test_iter_test_files_排除_無_test_marker_的_rs_檔 — 防 #\[cfg(test)\] 誤判
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

def test_量測_回傳_17_個_test_檔():
    """src-tauri/src 含 #\[test\] 檔應 = 17 (R172 grep 16 + T-OGRE15
    telemetry.rs 護衛 mod, otel-genai-runtime-emit-2026-q3 Phase 3)"""
    results = cs.measure(src_root=REPO_SRC)
    assert len(results) == 17, f"預期 17 個 test 檔, 實際 {len(results)}: {[r.relpath for r in results]}"


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


# ---------- 9-11. R195 內部函式 hidden gap 守護: 路徑 / 排除 / 正則邊界 ----------
# 對齊 R187 test_k0_measure.py 9 case 模式 (R188 從 6→9 case 內部函式
# hidden gap 守護軸向): R172 5 case + R180 3 case = 8 case 守住量化口徑
# 常數, R195 加 3 case 守住內部函式 hidden gap (路徑 / 排除邏輯 / 正則邊界)。
# 防有人改 SRC_TAURI 路徑 / 移除 target/ 排除 / 改寬 _TEST_MARKER_RE 導致
# K42 量化值失真 (R172 量測結果悄悄漂移, chain_staleness_drift_check.py
# 守的「5 維度對齊」就成了 meta-bug 層假象)。

# ---------- fixtures ----------

@pytest.fixture
def tmp_src_with_layers(tmp_path):
    """factory: 造 src root 含多層結構, 回傳 src_root Path
    結構:
      src_root/
        real.rs              (有 #[test] marker, 應被列)
        no_marker.rs         (無 #[test], 只有 #[cfg(test)], 應被排除)
        target/
          build_artifact.rs  (有 #[test] marker, 應被排除 — target/ 子樹)
    """
    src_root = tmp_path / "src"
    src_root.mkdir()
    # real.rs: 有 #[test] marker
    (src_root / "real.rs").write_text(
        "#[test]\nfn real_test() {}\n",
        encoding="utf-8",
    )
    # no_marker.rs: 只有 #[cfg(test)] 模組, 沒有 #[test] fn
    (src_root / "no_marker.rs").write_text(
        "#[cfg(test)]\nmod tests {}\n",
        encoding="utf-8",
    )
    # target/ 子樹內的 build_artifact.rs: 有 #[test] marker 但應被排除
    target = src_root / "target"
    target.mkdir()
    (target / "build_artifact.rs").write_text(
        "#[test]\nfn build_test() {}\n",
        encoding="utf-8",
    )
    return src_root


# ---------- 9. SRC_TAURI 路徑契約 ----------

def test_本體_SRC_TAURI_路徑_對齊_src_tauri_src():
    """SRC_TAURI 對齊 src-tauri/src (R172 既有路徑契約)

    守: 若有人改 SRC_TAURI 路徑 (e.g. 指到 src/ 漏了 -tauri/ 子目錄,
    或指到 src-tauri/tests/ 替代 src-tauri/src) → chain_staleness.py
    跑出來的護衛鏈量化值跟實際 K42 chain 20 護衛脫鉤, drift_check
    變 meta-bug 假象。
    """
    # SRC_TAURI 應 = <REPO_ROOT>/src-tauri/src
    expected = cs.REPO_ROOT / "src-tauri" / "src"
    assert cs.SRC_TAURI == expected, (
        f"SRC_TAURI 應 = {expected}, 實際 {cs.SRC_TAURI}. "
        f"改路徑前須先在 MISSION 補頁說明架構變更理由。"
    )
    # 防有人改 type (e.g. 從 Path 改成 str)
    assert isinstance(cs.SRC_TAURI, Path), (
        f"SRC_TAURI 應保持 Path 實例 (給 Path.rglob 用), 實際 {type(cs.SRC_TAURI)}"
    )
    # 路徑必須存在 (R172 既有事實, src-tauri/src 必含 Rust 源碼)
    assert cs.SRC_TAURI.exists(), f"SRC_TAURI 路徑必須存在: {cs.SRC_TAURI}"


# ---------- 10. _iter_test_files 排除 target/ 子樹 ----------

def test_iter_test_files_排除_target_子樹(tmp_src_with_layers):
    """_iter_test_files 必須排除 target/ 子樹內的 .rs 檔

    守: 若有人刪 `_iter_test_files` 內 `if "target" in rs.parts: continue`
    → target/ build artifact 會被當護衛計入 chain 量化值, K42 量化
    值被 build 產物污染失真 (R172 設計事實: target/ 是 cargo build
    產物, 跟護衛鏈無關)。

    對齊 R172 _iter_test_files 既有排除邏輯 (R170 設計事實)。
    """
    src_root = tmp_src_with_layers
    files = cs._iter_test_files(src_root)
    rels = sorted(f.name for f in files)
    # 只應回 real.rs (有 #[test] + 不在 target/ 子樹)
    # no_marker.rs 沒 #[test] marker → 被排除 (case 11 守)
    # build_artifact.rs 在 target/ → 被排除 (本 case 守)
    assert rels == ["real.rs"], (
        f"_iter_test_files 應只回 ['real.rs'], 實際 {rels}. "
        f"target/ 子樹內的 #[test] 檔應被排除 (R170/R172 設計事實)。"
    )


# ---------- 11. _iter_test_files 排除無 #[test] marker 的 .rs 檔 ----------

def test_iter_test_files_排除_無_test_marker_的_rs_檔(tmp_path):
    r"""_iter_test_files 必須排除只含 #[cfg(test)] 但無 #[test] fn 的 .rs 檔

    守: 若有人改 _TEST_MARKER_RE 為 `^\s*#\[(test|cfg\(test\))\]\s*$`
    (改寬含 #[cfg(test)]) → 純測試模組宣告但無實際 #[test] fn 的 .rs 檔
    會被當護衛計入, 護衛鏈 chain 計數虛胖失真 (R172 既有設計事實:
    純 #[cfg(test)] mod 宣告不算護衛檔, 必須有 #[test] fn 才是)。

    對齊 case 8 _TEST_MARKER_RE 守住純 marker pattern, 本 case 守
    排除行為邊界 (有 #[cfg(test)] 但無 #[test] fn 不算護衛)。
    """
    src_root = tmp_path / "src"
    src_root.mkdir()
    # cfg_only.rs: 只有 #[cfg(test)] 模組, 沒有 #[test] fn
    (src_root / "cfg_only.rs").write_text(
        "#[cfg(test)]\nmod tests { fn helper() {} }\n",
        encoding="utf-8",
    )
    # real.rs: 有 #[test] fn
    (src_root / "real.rs").write_text(
        "#[test]\nfn real_test() {}\n",
        encoding="utf-8",
    )
    # empty.rs: 純空檔
    (src_root / "empty.rs").write_text("// empty\n", encoding="utf-8")

    files = cs._iter_test_files(src_root)
    rels = sorted(f.name for f in files)
    # cfg_only.rs 沒 #[test] marker → 被排除 (本 case 守)
    # empty.rs 沒 #[test] marker → 被排除
    # real.rs 有 #[test] marker → 保留
    assert rels == ["real.rs"], (
        f"_iter_test_files 應只回 ['real.rs'], 實際 {rels}. "
        f"#[cfg(test)] 模組宣告但無 #[test] fn 的 .rs 檔應被排除 "
        f"(R172 設計事實: 純 cfg 標記不算護衛檔)。"
    )
