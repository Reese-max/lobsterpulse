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


# ----- case 3: 守 K40 真實 active 數 = 1 (mission-k0 Path B, R197 決議已取代) -----
def test_K40_真實_active_1_mission_k0_不回歸():
    """
    對齊 R192 量化真實值 2 active → 2026-07-05 M1 落地 otel-genai Phase 2/3
    (T-OGRE10~16 全 7 task ship) 後量化真實 = 1 active (mission-k0 Path B
    T-MKRB1~6 + Phase 4 placeholder, R197 Path A 決議已取代不開工)。守護守住
    「量化真實 = 1 active」這條口徑不漂移 (新 active change 出現 / otel-genai
    被解勾都會 fail)。
    """
    results = k40.measure()
    actives = sorted(r.name for r in results if not r.is_closed)
    assert actives == [
        "mission-k0-restructure-2026-q3",
    ], f"K40 量化真實 active 應為 1 個, 實際: {actives}"


# ----- case 4: 守 K40 真實 closed 數 = 9 (全 [x] 9 個) -----
def test_K40_真實_closed_9_全_x_的_9_change_不回歸():
    """
    對齊量化真實值: 9 個 change 全部 tasks [x] (closed) — R192 8 個 +
    2026-07-05 M1 落地 otel-genai-runtime-emit-2026-q3 16/16。
    改 K40 量化算法或 grep 正則或解勾既 closed change 會 fail。
    """
    results = k40.measure()
    closed = sorted(r.name for r in results if r.is_closed)
    expected = [
        "contract-matrix-guard",
        "cross-provider-timeline",
        "lobster-rules-engine",
        "openab-bot-sync",
        "otel-genai-runtime-emit-2026-q3",
        "otel-provider-metrics-contract",
        "prometheus-counter-convention",
        "prometheus-counter-rename-2026-q3",
        "r114-k0-coverage-and-dual-emit-guard",
    ]
    assert closed == expected, f"K40 量化真實 closed 應為 9 個, 實際: {closed}"


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


# ---------- 6-8. R196 內部函式 hidden gap 守護: 正則 / 排除 / 解析 fallback ----------
# 對齊 R195 test_chain_staleness.py 9-11 case 模式 (R195 從 8→11 case 內部
# 函式 hidden gap 守護軸向): R192 5 case 守住量化口徑常數, R196 加 3 case 守
# 內部函式 hidden gap (_TASK_RE 正則 / _iter_change_dirs archive/ 排除雙重
# 判斷 / _parse_tasks OSError fallback)。防有人改寬 _TASK_RE pattern 導致
# K40 量化值失真, 或刪 _iter_change_dirs 內 archive/ 排除讓歷史封存干擾
# active 計數, 或改壞 _parse_tasks 讓不可讀檔案 silent pass 成 (0,0) 干擾
# K40 closed/active 量化口徑 (k40_drift_check.py 守的「5 維度對齊」就成
# meta-bug 假象, 鏡像 R195 chain_staleness_drift_check 同樣 meta-bug 風險)。


# ---------- 6. _TASK_RE 正則邊界 ----------

def test_本體_TASK_RE_守住_純_marker_pattern_不漂移():
    """_TASK_RE 是 k40_measure 識別 tasks.md task 行的唯一契約。
    守 pattern 跟 flags 兩個軸:
      - pattern 漂移風險: 改寬 (e.g. 加 `*` 變成 `[-* x]`) 會把 list bullet
        誤算 task, 量化 closed/active 數字悄悄多算; 改嚴 (e.g. 漏掉
        `\\s*` 前置空白) 會把縮排 task 漏算。
      - flags 漂移風險: 拿掉 MULTILINE flag 會只 match 第一行, 整份
        tasks.md 變 1/0 / 0/0, K40 量化口徑整個失真。
    對齊既 grep 算法 `grep -cE '^\\s*-\\s*\\[[ x]\\]'`, pattern 必須嚴格
    守住 `^\\s*-\\s*\\[[ x]\\]` + MULTILINE。
    """
    import re as _re
    expected = _re.compile(r"^\s*-\s*\[([ x])\]", _re.MULTILINE)
    assert k40._TASK_RE.pattern == expected.pattern, (
        f"_TASK_RE pattern 漂移: 預期 {expected.pattern!r}, "
        f"實際 {k40._TASK_RE.pattern!r}. 改寬會誤算 list bullet, "
        f"改嚴會漏算縮排 task。"
    )
    assert k40._TASK_RE.flags == expected.flags, (
        f"_TASK_RE flags 漂移: 預期 MULTILINE, 實際 {k40._TASK_RE.flags}. "
        f"拿掉 MULTILINE 只 match 第一行, 整份 tasks.md 量化值失真。"
    )

    # 行為驗證: tab 縮排 + 4 space 縮排 + 多行混合都能正確計數
    with tempfile.TemporaryDirectory() as tmp:
        spec_root = Path(tmp)
        (spec_root / "indented").mkdir()
        (spec_root / "indented" / "tasks.md").write_text(
            "header line\n"
            "\t- [x] tab indented\n"
            "    - [x] 4-space indented\n"
            "- [x] no indent\n"
            "  - [ ] 2-space active\n"
            "  - [x] 2-space closed\n",
            encoding="utf-8",
        )
        results = k40.measure(spec_root=spec_root)
    assert len(results) == 1
    r = results[0]
    assert r.total_tasks == 5, f"tab/space 縮排 5 個 task 應全算, 實際 total={r.total_tasks}"
    assert r.closed_tasks == 4, f"4 closed (含 tab+space+no-indent+2-space), 實際 closed={r.closed_tasks}"
    assert r.is_closed is False, "5 個有 1 個 [ ] 應算 active"


# ---------- 7. _iter_change_dirs archive/ 排除雙重判斷 + 空 spec_root 邊界 ----------

def test_iter_change_dirs_排除_archive_子樹_雙重判斷_不漂移():
    """_iter_change_dirs 排除 archive/ 走雙重判斷:
      - 頂層 `d.name == "archive"` (直接子目錄)
      - 深層 `"archive" in d.parts` (子樹內任一層)
    守: 若有人刪 `d.name == "archive"` 只留 `in d.parts` → 頂層 archive/
    仍會被排除 (parts 守得到), 但若有人反過來只留 `d.name == "archive"`
    → 深層 archive/sub/old-change 會 silent 漏網, 干擾 K40 active 量化。
    對齊 R192 _iter_change_dirs 既有排除邏輯 (R187 k0_measure 模式)。
    """
    with tempfile.TemporaryDirectory() as tmp:
        spec_root = Path(tmp)
        # 頂層 archive/ 應排除
        (spec_root / "archive").mkdir()
        (spec_root / "archive" / "top-level-archive").mkdir()
        (spec_root / "archive" / "top-level-archive" / "tasks.md").write_text(
            "- [ ] top-level-archive-task\n", encoding="utf-8"
        )
        # 深層 archive/sub/ 應排除
        (spec_root / "nested").mkdir()
        (spec_root / "nested" / "archive").mkdir()
        (spec_root / "nested" / "archive" / "deep-archive").mkdir()
        (spec_root / "nested" / "archive" / "deep-archive" / "tasks.md").write_text(
            "- [ ] deep-archive-task\n", encoding="utf-8"
        )
        # 真實 change 應保留
        (spec_root / "real-change").mkdir()
        (spec_root / "real-change" / "tasks.md").write_text(
            "- [x] real-task\n", encoding="utf-8"
        )
        # 邊界: 含 archive 字眼但不是 archive/ 目錄 (e.g. archived-notes/) 應保留
        (spec_root / "archived-notes").mkdir()
        (spec_root / "archived-notes" / "tasks.md").write_text(
            "- [x] notes-task\n", encoding="utf-8"
        )
        # 邊界: 目錄裡沒 tasks.md 應跳過
        (spec_root / "no-tasks-dir").mkdir()
        (spec_root / "no-tasks-dir" / "README.md").write_text("not tasks\n", encoding="utf-8")

        results = k40.measure(spec_root=spec_root)

    names = sorted(r.name for r in results)
    assert names == ["archived-notes", "real-change"], (
        f"archive/ 雙重排除 + archived-notes/ 應保留, 實際: {names}. "
        f"頂層 archive/ 跟深層 nested/archive/ 都應排除, "
        f"archived-notes/ (含 archive 字眼但非 archive/ 目錄) 應保留。"
    )


def test_iter_change_dirs_spec_root_不存在_回空_list_不爆():
    """spec_root 不存在時 (新 clone 還沒開任何 change) → 回空 list,
    measure() 回空 list, 量化值 = 0/0。防有人把 `if not spec_root.exists()`
    拿掉 → Path.iterdir() 會 FileNotFoundError, 量化腳本 crash, K40
    量化口徑整個失效。
    """
    with tempfile.TemporaryDirectory() as tmp:
        nonexistent = Path(tmp) / "no-such-spec-root"
        # 雙重驗證: _iter_change_dirs 直接呼叫 + measure() 走完整路徑
        assert k40._iter_change_dirs(nonexistent) == [], (
            f"不存在的 spec_root 應回空 list, 實際 {_iter_change_dirs(nonexistent)}"
        )
        results = k40.measure(spec_root=nonexistent)
        assert results == [], f"measure(不存在的 spec_root) 應回空 list, 實際 {results}"


# ---------- 8. _parse_tasks OSError fallback + 編碼 errors="replace" 邊界 ----------

def test_parse_tasks_不可讀檔案_OSError_fallback_0_0_不漂移():
    """_parse_tasks 對 OSError (檔案不存在 / 權限拒絕) 走 (0, 0) fallback。
    守: 若有人把 try/except OSError 拿掉 → Path.read_text 對不存在檔案
    會 FileNotFoundError, 量化腳本 crash; 對權限拒絕會 PermissionError,
    K40 量化口徑整個失效。fallback (0, 0) 設計事實: 不可讀 tasks.md 視
    同 0 task, 由 measure() 端 `is_closed=(closed == total)` 把 0/0
    算 closed (case 5 邊界 1 已 cover), 守 K40 量化口徑不漂移。
    """
    with tempfile.TemporaryDirectory() as tmp:
        # 邊界 1: tasks.md 不存在 → FileNotFoundError 走 (0,0) fallback
        nonexistent = Path(tmp) / "no-tasks.md"
        total, closed = k40._parse_tasks(nonexistent)
        assert (total, closed) == (0, 0), (
            f"不存在的 tasks.md 應 fallback (0, 0), 實際 ({total}, {closed})"
        )

    # 邊界 2: tasks.md 是目錄 (Path.read_text 會 IsADirectoryError, OSError 子類)
    with tempfile.TemporaryDirectory() as tmp:
        dir_path = Path(tmp) / "is-a-dir.md"
        dir_path.mkdir()
        total, closed = k40._parse_tasks(dir_path)
        assert (total, closed) == (0, 0), (
            f"tasks.md 是目錄應 fallback (0, 0), 實際 ({total}, {closed})"
        )

    # 邊界 3: 編碼 errors="replace" 守住 — 寫入含損壞 UTF-8 byte 的 tasks.md,
    # _parse_tasks 不爆, 仍能正確計數 ASCII 範圍的 task 行
    with tempfile.TemporaryDirectory() as tmp:
        spec_root = Path(tmp)
        (spec_root / "real").mkdir()
        # \xff 是無效 UTF-8 lead byte, errors="replace" 會替換成 U+FFFD
        # 但 ASCII 範圍的 - [x] / - [ ] task 行仍能被 _TASK_RE 認得
        (spec_root / "real" / "tasks.md").write_bytes(
            b"\xff\xfe bad utf-8\n- [x] good task 1\n- [x] good task 2\n- [ ] active task\n"
        )
        total, closed = k40._parse_tasks(spec_root / "real" / "tasks.md")
        assert total == 3, f"壞 UTF-8 邊界應仍計到 3 task, 實際 total={total}"
        assert closed == 2, f"壞 UTF-8 邊界應仍計到 2 closed, 實際 closed={closed}"


# ---------- R211 內部函式 hidden gap 守護延伸 4 case ----------
# 鏡像 R188 k0_measure / R195 chain_staleness / R196 K40 producer (本檔
# R196 已守 _TASK_RE / _iter_change_dirs / _parse_tasks) / R198 K0 endpoint
# live / R201 K30 P95 / R202 K41 drift / R203 k0_target_baseline_check /
# R204 k0_drift_check / R206 chain_staleness_drift_check / R207 k40_drift_check
# 內部函式既模式 = 跨 11 個不同 KPI 維度對稱 (R207 第 10 維度 → R211 第
# 11 維度外推驗證, 補 K40 producer 端 R196 沒守的剩餘內部函式 hidden gap:
# render_table 排序穩定性 / ChangeProgress immutability / main argv+JSON schema)。
#
# R211 不重複 R196 既守 (case 6 _TASK_RE / case 7 _iter_change_dirs archive 排除 +
# spec_root 邊界 / case 8 _parse_tasks OSError + encoding), 補 4 個新軸:
#   9. render_table sort 穩定性 + closed/active 計數對 (sort by (is_closed, name)
#      同狀態群按 name 字母排, closed 群在前, active 群在後)
#  10. render_table 空 results list 邊界 (新 clone 還沒任何 change → 表格不爆)
#  11. ChangeProgress frozen=True 守護 (dataclass immutability, 防有人拿掉
#      frozen 讓 K40 量化值被 silent mutation 污染)
#  12. main() argv + .harness-k40.json schema 守護 (json 輸出欄位
#      k40_changes_total/closed/active + active_names + changes 對齊 contract)


# ---------- 9. render_table sort 穩定性 + closed/active 計數對 ----------

def test_render_table_sort_穩定性_closed_群在前_active_群在後_同群按_name_字母():
    """render_table() sort by (is_closed, name) + closed_count + active_count 守口徑

    M0 級 hidden gap 守護:
      - sort 穩定性: sort by (is_closed, name) 讓 closed 群 (False=0 → closed 排前面)
        在 active 群 (True=1) 前面, 同群內按 name 字母排
      - closed_count + active_count == len(results) (不重算 / 不漏算)
      - TOTAL 列 closed=N / active=M 對得上每 row 對應狀態
    防有人改 render_table 排序鍵 (e.g. 改成 sort by name 直接, 失去 closed 群優先
    可讀性) 或計數算法 (e.g. closed_count 用 `sum(1 for r in results if r.is_closed)`
    但 active_count 寫死 `len(results) - closed_count` 算出負數 if closed > total bug)。

    對齊 R204 k0_drift_check.render_report / R207 k40_drift_check.render_report
    既 hidden gap 守護模式 (delta 格式簽一致 + 計數對得上), R211 補 producer
    端 render_table 同模式。
    """
    with tempfile.TemporaryDirectory() as tmp:
        spec_root = Path(tmp)
        # 故意交錯命名, 確認 sort 穩定性 (同狀態按 name 字母)
        specs = {
            # closed 群 (全 [x]) — 字母故意打亂放
            "zebra-closed": "- [x] a\n",
            "kiwi-closed": "- [x] b\n",
            "mango-closed": "- [x] c\n",
            # active 群 (有 [ ])
            "alpha-active": "- [ ] d\n",
            "banana-active": "- [ ] e\n",
        }
        for name, content in specs.items():
            d = spec_root / name
            d.mkdir()
            (d / "tasks.md").write_text(content, encoding="utf-8")

        results = k40.measure(spec_root=spec_root)
        table = k40.render_table(results)

    lines = table.splitlines()
    # 1. sort 穩定性: closed 群 (kiwi < mango < zebra) 必須在 active 群 (alpha < banana) 前面
    closed_lines = [
        i for i, ln in enumerate(lines)
        if any(name in ln for name in ("kiwi-closed", "mango-closed", "zebra-closed"))
    ]
    active_lines = [
        i for i, ln in enumerate(lines)
        if any(name in ln for name in ("alpha-active", "banana-active"))
    ]
    assert len(closed_lines) == 3, f"應有 3 個 closed row, 實際 {len(closed_lines)}"
    assert len(active_lines) == 2, f"應有 2 個 active row, 實際 {len(active_lines)}"
    assert max(closed_lines) < min(active_lines), (
        f"closed 群必須在 active 群前面 (sort by is_closed 先 False/True), "
        f"closed_lines={closed_lines}, active_lines={active_lines}"
    )
    # 同 closed 群內按 name 字母排: kiwi < mango < zebra
    kiwi_idx, mango_idx, zebra_idx = closed_lines
    assert kiwi_idx < mango_idx < zebra_idx, (
        f"同 closed 群內按 name 字母排, 實際順序: "
        f"kiwi={kiwi_idx}, mango={mango_idx}, zebra={zebra_idx}"
    )
    # 同 active 群內按 name 字母排: alpha < banana
    alpha_idx, banana_idx = active_lines
    assert alpha_idx < banana_idx

    # 2. closed_count + active_count == len(results) (計數對得上)
    # 5 個 change: 3 closed + 2 active
    assert "closed=3" in table, f"TOTAL 列必須顯示 closed=3, 實際:\n{table}"
    assert "active=2" in table, f"TOTAL 列必須顯示 active=2, 實際:\n{table}"

    # 3. header + 邊界線守住
    assert "change" in table  # header
    assert "progress" in table
    assert "state" in table


# ---------- 10. render_table 空 results list 邊界 ----------

def test_render_table_空_results_list_不爆_只_render_header_加_TOTAL_0_0():
    """render_table([]) → header + 邊界線 + TOTAL 0/0, 不 IndexError / 不 KeyError

    M0 級 hidden gap 守護: 防有人改 render_table 把 header 跟 TOTAL 列硬連
    (e.g. 用 zip(closed_count, active_count) 但 closed_count/active_count 從
    results 算 = 0/0 時 for 迴圈不執行 → 印出空 header), 或 sorted([],
    key=...) 對空 list 爆 TypeError。新 clone 還沒任何 change 時 results=[]
    必須 graceful render (header + 邊界線 + TOTAL 0/0)。
    """
    table = k40.render_table([])
    lines = table.splitlines()
    # header + 邊界線 + TOTAL 列 = 至少 3 行
    assert len(lines) >= 3, f"空 results 至少要有 header + 邊界線 + TOTAL, 實際 {len(lines)} 行"
    # header 行
    assert "change" in lines[0]
    assert "progress" in lines[0]
    # TOTAL 列 closed=0 active=0
    total_line = lines[-1]
    assert "closed=0" in total_line, f"TOTAL 列必須顯示 closed=0, 實際: {total_line}"
    assert "active=0" in total_line, f"TOTAL 列必須顯示 active=0, 實際: {total_line}"
    # 0/0 (no results, total = 0/0)
    assert "0/0" in total_line


# ---------- 11. ChangeProgress frozen=True 守護 ----------

def test_ChangeProgress_frozen_dataclass_守住_immutability_不漂移():
    """ChangeProgress 是 frozen=True dataclass, 不可 mutation。

    M0 級 hidden gap 守護: 防有人拿掉 frozen=True (e.g. `@dataclass` 不寫
    frozen) 讓 ChangeProgress 變 mutable, 之後 measure() / render_table() pipeline
    中有人不小心改 r.is_closed 或 r.progress 會 silent 污染 K40 量化結果
    (consumer k40_drift_check.load_current 拿到的 payload 已經被改過, 觸發
    假 REGRESS 警報)。frozen=True 是 K40 量化值的唯一來源, 一旦 mutation
    入口被打開, K40 量化口徑整個失守。

    對齊 R188 k0_measure / R195 chain_staleness 等 producer 端 immutability
    守護軸, R211 補 K40 producer 端 ChangeProgress 守護。
    """
    # 1. frozen=True 守住 (dataclass __dataclass_params__.frozen == True)
    import dataclasses as _dc
    params = _dc.fields(k40.ChangeProgress)
    assert k40.ChangeProgress.__dataclass_params__.frozen is True, (
        "ChangeProgress 必須 frozen=True, 防 measure/render_table pipeline 中 "
        "silent mutation 污染 K40 量化值"
    )
    # 2. 4 個欄位 (name / total_tasks / closed_tasks / is_closed / progress) 全守住
    field_names = {f.name for f in params}
    assert field_names == {"name", "total_tasks", "closed_tasks", "is_closed", "progress"}, (
        f"ChangeProgress 欄位集合漂移, 預期 {{name, total_tasks, closed_tasks, "
        f"is_closed, progress}}, 實際 {field_names}"
    )

    # 3. 實例 mutation 必須 raise FrozenInstanceError (frozen 行為)
    cp = k40.ChangeProgress(
        name="test", total_tasks=2, closed_tasks=2,
        is_closed=True, progress="2/2",
    )
    import dataclasses as _dc
    with pytest.raises(_dc.FrozenInstanceError):
        cp.is_closed = False  # type: ignore[misc]
    with pytest.raises(_dc.FrozenInstanceError):
        cp.progress = "1/2"  # type: ignore[misc]


# ---------- 12. main() argv + .harness-k40.json schema 守護 ----------

def test_main_argv_解析_加_json_schema_對齊_k40_drift_check_contract(capsys, tmp_path):
    """main() 接受 --json / --spec-root argv + 寫 .harness-k40.json schema 對齊 contract

    M0 級 hidden gap 守護:
      - argv 解析: --json / --spec-root 兩個 flag 都接受, 不爆 argparse
      - .harness-k40.json schema 對齊 k40_drift_check.load_current consumer
        contract (欄位: k40_changes_total / k40_changes_closed / k40_changes_active
        / active_names / changes + spec_root + timestamp + mission_alignment_note)
      - 防有人改 main() payload schema (e.g. 漏寫 k40_changes_active 或
        active_names 改名) 而 k40_drift_check.load_current KeyError fail-fast
        整個 K40 量化 pipeline 失效

    對齊 R204 / R206 / R207 既模式: producer payload schema 必須對齊 consumer
    contract, 兩端契約漂移 = M0 級 hidden gap。
    """
    import json as _json
    spec_root = tmp_path / "fake-spec"
    spec_root.mkdir()
    (spec_root / "alpha").mkdir()
    (spec_root / "alpha" / "tasks.md").write_text(
        "- [x] task1\n- [x] task2\n", encoding="utf-8"
    )
    (spec_root / "beta").mkdir()
    (spec_root / "beta" / "tasks.md").write_text(
        "- [x] task1\n- [ ] task2\n", encoding="utf-8"
    )

    json_path = tmp_path / "out.json"
    exit_code = k40.main([
        "--json", str(json_path),
        "--spec-root", str(spec_root),
    ])
    assert exit_code == 0, f"main() 該 exit 0, 實際 {exit_code}"

    # JSON 寫入 + schema 守護
    assert json_path.exists(), f"main() 該寫 {json_path}"
    payload = _json.loads(json_path.read_text(encoding="utf-8"))

    # 1. 5 個 consumer contract 必要欄位 (對齊 k40_drift_check.load_current)
    required = {
        "k40_changes_total", "k40_changes_closed",
        "k40_changes_active", "active_names", "changes",
    }
    assert required.issubset(payload.keys()), (
        f"main() payload 漏 consumer contract 欄位, 預期 {required}, "
        f"實際 {set(payload.keys())}"
    )
    # 2. 量化值對得上 (alpha 2/2 closed + beta 1/2 active = 1 closed + 1 active)
    assert payload["k40_changes_total"] == 2, f"total 應為 2, 實際 {payload['k40_changes_total']}"
    assert payload["k40_changes_closed"] == 1, f"closed 應為 1, 實際 {payload['k40_changes_closed']}"
    assert payload["k40_changes_active"] == 1, f"active 應為 1, 實際 {payload['k40_changes_active']}"
    # 3. active_names list 對得上
    assert payload["active_names"] == ["beta"], (
        f"active_names 應為 ['beta'], 實際 {payload['active_names']}"
    )
    # 4. changes list 內每個 ChangeProgress asdict 欄位對得上
    assert len(payload["changes"]) == 2
    change_names = sorted(c["name"] for c in payload["changes"])
    assert change_names == ["alpha", "beta"]
