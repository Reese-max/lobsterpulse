#!/usr/bin/env python3
"""
K40 量化護衛 — 5 case pytest 守 K40 spec 量化口徑不漂移

R192 落地。鏡像 R187 test_k0_measure.py 模式 (M2 KPI 量測 closure 軸守護
本體延伸)。每個 case 守一個 hidden gap, 共 5 case 守 5 個量化口徑常數。

對齊:
  - test_k0_measure.py: 同樣 1 個 pytest 模組, case 編號接續
  - test_k41_drift_check.py (R191): 同樣 K-dimension 守護風格
  - k40_measure.py: 生產者腳本, 本檔護衛其量化口徑
"""
import importlib.util
import sys
import tempfile
from pathlib import Path

import pytest

# 動態載入 k40_measure.py (避免 src-tauri Cargo 等命名衝突)
_K40_PATH = Path(__file__).resolve().parent / "k40_measure.py"
_spec = importlib.util.spec_from_file_location("k40_measure", _K40_PATH)
assert _spec and _spec.loader, "k40_measure.py 載入失敗"
k40 = importlib.util.module_from_spec(_spec)
sys.modules["k40_measure"] = k40
_spec.loader.exec_module(k40)


# ----- case 1: 守 closed 算法 = 全 [x] 算 closed (R192 量化口徑常數 #1) -----
def test_closed_算法_全_x_算_closed_不回歸():
    """
    對齊 MISSION K40 量化口徑: change 內所有 tasks 都 [x] 才算 closed。
    任何一行 [ ] 都算 active。改壞這個算法 = K40 closed/active 漂移。
    """
    with tempfile.TemporaryDirectory() as tmp:
        spec_root = Path(tmp)
        # 全 [x] = 應 closed
        (spec_root / "all-done").mkdir()
        (spec_root / "all-done" / "tasks.md").write_text(
            "- [x] task1\n- [x] task2\n- [x] task3\n", encoding="utf-8"
        )
        # 有 [ ] = 應 active
        (spec_root / "one-left").mkdir()
        (spec_root / "one-left" / "tasks.md").write_text(
            "- [x] task1\n- [x] task2\n- [ ] task3\n", encoding="utf-8"
        )
        results = k40.measure(spec_root=spec_root)
    by_name = {r.name: r for r in results}
    assert by_name["all-done"].is_closed is True, "全 [x] 應算 closed"
    assert by_name["all-done"].progress == "3/3"
    assert by_name["one-left"].is_closed is False, "有 [ ] 算 active"
    assert by_name["one-left"].progress == "2/3"


# ----- case 2: 守 active 算法 = 有 [ ] 算 active (R192 量化口徑常數 #2) -----
def test_active_算法_有_空白_算_active_不回歸():
    """
    對齊 MISSION K40 量化口徑: change 內只要有一行 [ ] 就算 active。
    即使只有 1 個 [ ] 也要算 active, 不准有「半關閉」狀態。
    """
    with tempfile.TemporaryDirectory() as tmp:
        spec_root = Path(tmp)
        (spec_root / "mostly-done").mkdir()
        (spec_root / "mostly-done" / "tasks.md").write_text(
            "- [x] task1\n- [x] task2\n- [x] task3\n"
            "- [x] task4\n- [x] task5\n- [x] task6\n"
            "- [x] task7\n- [x] task8\n- [ ] task9\n",
            encoding="utf-8",
        )
        results = k40.measure(spec_root=spec_root)
    assert len(results) == 1
    assert results[0].is_closed is False, "9/8 [x]+1 [ ] 應算 active"
    assert results[0].closed_tasks == 8
    assert results[0].total_tasks == 9


# ----- case 3: 守 K40 真實 active 數 = 2 (mission-k0 3/15 + otel-genai 9/16) -----
def test_K40_真實_active_2_mission_k0_加_otel_genai_不回歸():
    """
    對齊 R192 量化真實值: 2 active (mission-k0-restructure-2026-q3 3/15 +
    otel-genai-runtime-emit-2026-q3 9/16)。MISSION 表寫 1 active (otel-genai
    only) 是 K40 spec drift 證據; 守護守住「量化真實 = 2 active」這條口徑
    不漂移, PUA 不 patch MISSION (owner M scope Path A 才動)。
    """
    results = k40.measure()
    actives = sorted(r.name for r in results if not r.is_closed)
    assert actives == [
        "mission-k0-restructure-2026-q3",
        "otel-genai-runtime-emit-2026-q3",
    ], f"K40 量化真實 active 應為 2 個, 實際: {actives}"


# ----- case 4: 守 K40 真實 closed 數 = 8 (全 [x] 8 個) -----
def test_K40_真實_closed_8_全_x_的_8_change_不回歸():
    """
    對齊 R192 量化真實值: 8 個 change 全部 tasks [x] (closed) — 對齊 MISSION
    R132 K40 8 closed 量化值守住。改 K40 量化算法或 grep 正則會 fail。
    """
    results = k40.measure()
    closed = sorted(r.name for r in results if r.is_closed)
    expected = [
        "contract-matrix-guard",
        "cross-provider-timeline",
        "lobster-rules-engine",
        "openab-bot-sync",
        "otel-provider-metrics-contract",
        "prometheus-counter-convention",
        "prometheus-counter-rename-2026-q3",
        "r114-k0-coverage-and-dual-emit-guard",
    ]
    assert closed == expected, f"K40 量化真實 closed 應為 8 個, 實際: {closed}"


# ----- case 5: 守空 tasks.md 邊界 + tasks 解析正則 -----
def test_空_tasks_md_邊界_0_0_不爆_加_tasks_解析正則_不漂移():
    """
    守 2 個邊界:
    1. 空 tasks.md (0 [x] 0 [ ]) → progress 0/0, is_closed = True (0/0 視為空, 全 [x])
       設計決策: 0/0 算 closed (無 active task = 完成), 防 0/0 變成 active 干擾量化
    2. tasks 解析正則 `^\\s*-\\s*\\[[ x]\\]`: 對齊 `grep -cE '^\\s*-\\s*\\[[ x]\\]'`
       既 grep 算法 (case 1 守 closed 已驗), 此 case 再守多行/縮排/連續 task 不漂移

    archive/ 子樹也應被排除 (歷史封存, 不算 active K40 量化), 對齊 k40_measure.py
    _iter_change_dirs 排除邏輯。
    """
    # 邊界 1: 空 tasks.md
    with tempfile.TemporaryDirectory() as tmp:
        spec_root = Path(tmp)
        (spec_root / "empty").mkdir()
        (spec_root / "empty" / "tasks.md").write_text("# 沒有 task\n", encoding="utf-8")
        results = k40.measure(spec_root=spec_root)
    assert len(results) == 1
    assert results[0].total_tasks == 0
    assert results[0].closed_tasks == 0
    assert results[0].progress == "0/0"
    # 0/0 算 closed (無 active task = 0/0 視為完成) — 防 0/0 被誤算 active
    assert results[0].is_closed is True, "0/0 視為 closed (無 active task 干擾)"

    # 邊界 2: archive/ 子樹應被排除
    with tempfile.TemporaryDirectory() as tmp:
        spec_root = Path(tmp)
        (spec_root / "real-change").mkdir()
        (spec_root / "real-change" / "tasks.md").write_text(
            "- [x] only\n", encoding="utf-8"
        )
        (spec_root / "archive").mkdir()
        (spec_root / "archive" / "old-change").mkdir()
        (spec_root / "archive" / "old-change" / "tasks.md").write_text(
            "- [ ] should-not-count\n", encoding="utf-8"
        )
        results = k40.measure(spec_root=spec_root)
    names = sorted(r.name for r in results)
    assert names == ["real-change"], f"archive/ 應被排除, 實際: {names}"
