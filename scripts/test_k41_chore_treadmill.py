#!/usr/bin/env python3
"""
K41 護衛 — 5 case pytest, 對齊 R132/R137/R144/R172 模式 (M0+M2 雙 hidden gap closure)

R176 落地。1 個 Python pytest 模組, 走既無既有護衛維度
(Python script 不算 Rust 護衛, 不破 K42 chain 20 條飽和契約)。

5 case 守 2 個 hidden gap:
  1. test_GOVERNANCE_PREFIXES_4_前綴_對齊_R107 — 結構性常數驗證
  2. test_常量對齊_MISSION_K41_7d_30pct — WINDOW_DAYS=7 + THRESHOLD=0.30
  3. test_零_commit_空_list_回傳_0_0 — 邊界: 視窗內無 commit
  4. test_chore_含_scope_分類_正確 — M0 觸發: 修 conventional commit scope bug
  5. test_feat_fix_docs_含_scope_不誤分類 — 反向: 非治理批不應誤觸發

M0 bug: k41_chore_treadmill.py 原 measure() 用 `subject.split(":", 1)[0] in GOVERNANCE_PREFIXES`
分類, 沒處理 conventional commit "chore(scope):" 格式, 導致實際 commit 樣本中
"chore(gitignore):" / "chore(spec):" / "chore(lib):" 全部被誤分類為非治理批,
K41 量化值 undercount。例: R127 chore(gitignore) 漏算。
"""
import subprocess
import sys
from pathlib import Path

import pytest

SCRIPT = Path(__file__).resolve().parent / "k41_chore_treadmill.py"
sys.path.insert(0, str(SCRIPT.parent))

import k41_chore_treadmill as k41  # noqa: E402


def _make_completed(stdout: bytes) -> "subprocess.CompletedProcess":
    """造 1 個假的 CompletedProcess, stdout 傳 bytes (對齊原腳本 decode 邏輯)"""
    return subprocess.CompletedProcess(args=[], returncode=0, stdout=stdout, stderr=b"")


# ---------- 1. GOVERNANCE_PREFIXES 結構性驗證 ----------

def test_GOVERNANCE_PREFIXES_4_前綴_對齊_R107():
    """對齊 R107 定義: chore / refactor / archive / sensor 共 4 個治理批前綴

    守: 若有人不小心把 test / docs / perf 加進 GOVERNANCE_PREFIXES,
    護衛 fail 並指出實際前綴清單, 阻擋 chore_treadmill 失真。
    """
    expected = {"chore", "refactor", "archive", "sensor"}
    actual = set(k41.GOVERNANCE_PREFIXES)
    assert actual == expected, (
        f"GOVERNANCE_PREFIXES 應 = {expected}, 實際 {actual}\n"
        f"差異: 多了 {actual - expected}, 少了 {expected - actual}"
    )
    assert len(k41.GOVERNANCE_PREFIXES) == 4


# ---------- 2. 量測常數對齊 MISSION K41 ----------

def test_常量對齊_MISSION_K41_7d_30pct():
    """WINDOW_DAYS=7 + THRESHOLD=0.30 對齊 MISSION.md 90 天 KPI 設定

    守: 若有人改 WINDOW_DAYS 或 THRESHOLD 沒同步 MISSION, 護衛 fail
    並指出實際值, 阻擋 K41 量測口徑漂移。
    """
    assert k41.WINDOW_DAYS == 7, f"WINDOW_DAYS 應 = 7, 實際 {k41.WINDOW_DAYS}"
    assert k41.THRESHOLD == 0.30, f"THRESHOLD 應 = 0.30, 實際 {k41.THRESHOLD}"


# ---------- 3. 邊界: 視窗內 0 commit ----------

def test_零_commit_空_list_回傳_0_0_空_chore_list(monkeypatch):
    """mock git log → 0 commit → measure() 回 (0, 0, [])

    守: 空視窗不應爆 (ZeroDivisionError) 也不應回 None, 對齊 main()
    「0 commits in last 7d (baseline reset)」輸出路徑。0/0 留給 main()
    自己 short-circuit, 護衛不強制除法安全 (純函式語意)。
    """
    monkeypatch.setattr(
        "subprocess.run",
        lambda *a, **kw: _make_completed(b""),
    )
    chore_n, total, chore_list = k41.measure()
    assert chore_n == 0
    assert total == 0
    assert chore_list == []


# ---------- 4. M0 觸發: chore 含 scope 也要正確分類 ----------

def test_chore_含_scope_分類_正確(monkeypatch):
    """mock git log → 全部 chore(scope): 格式 → 應全部分類為治理批

    M0 bug 觸發: 原 measure() 用 `subject.split(":", 1)[0] in GOVERNANCE_PREFIXES`,
    "chore(gitignore): R127 ..." 抽出 prefix = "chore(gitignore)" 不在 4 前綴
    tuple 中, 導致誤分類。R127/135/137/115 等實際 commit 都踩到這 bug。
    修法: _classify_prefix() helper 處理 "type(scope)" 拆 scope。
    """
    fake_log = (
        b"chore(gitignore): R127 daemon\n"
        b"chore(lib): R121 closure\n"
        b"chore(spec): R111 handoff\n"
        b"chore: rotate engineering-log (1022->500)\n"  # 無 scope 也應分類
    )
    monkeypatch.setattr(
        "subprocess.run",
        lambda *a, **kw: _make_completed(fake_log),
    )
    chore_n, total, chore_list = k41.measure()
    assert total == 4
    assert chore_n == 4, (
        f"chore(scope): 應全部分類為 chore, 實際 chore_n={chore_n}, "
        f"未分類清單 = {[s for s in fake_log.decode().splitlines() if s not in chore_list]}"
    )


# ---------- 5. 反向: feat/fix/docs 含 scope 不應誤分類 ----------

def test_feat_fix_docs_含_scope_不誤分類(monkeypatch):
    """mock git log → feat/fix/docs(scope): 格式 → 應 0 個分類為 chore

    守: 若 _classify_prefix() 過度寬鬆把 feat/fix/docs 誤抓, 護衛 fail
    並指出誤分類清單, 阻擋 chore_treadmill 假警報。
    """
    fake_log = (
        b"feat(scripts): chain_staleness.py\n"
        b"fix(scripts): r124_sentinel tuple\n"
        b"docs(engineering-log): R175 entry\n"
        b"feat(scripts): commit_subject_lint.py\n"
        b"fix(sidecar): lobster-pulse-hook HTTP status\n"
    )
    monkeypatch.setattr(
        "subprocess.run",
        lambda *a, **kw: _make_completed(fake_log),
    )
    chore_n, total, chore_list = k41.measure()
    assert total == 5
    assert chore_n == 0, (
        f"feat/fix/docs 不應被分類為 chore, 誤分類 = {chore_list}"
    )
