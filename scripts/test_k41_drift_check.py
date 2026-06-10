#!/usr/bin/env python3
"""
K41 量化口徑漂移偵測護衛 — 5 case pytest, 對齊 R191 量化口徑閉合護衛鏈

R191 落地。1 個 Python pytest 模組, 走既無既有護衛維度 (Python
script 不算 Rust 護衛, 不破 K42 chain 20 條飽和契約)。
"""
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

SCRIPT = Path(__file__).resolve().parent / "k41_drift_check.py"
K41_SCRIPT = Path(__file__).resolve().parent / "k41_chore_treadmill.py"
PYTHON = sys.executable


# ---------- fixtures ----------

@pytest.fixture
def tmp_k41_script(tmp_path):
    """factory: 複製 k41_chore_treadmill.py 進 tmp, 回傳 path + 改壞 closure"""
    if not K41_SCRIPT.exists():
        pytest.skip(f"k41_chore_treadmill.py 找不到: {K41_SCRIPT}")
    def _setup(modifier=None) -> Path:
        target = tmp_path / "k41_chore_treadmill.py"
        shutil.copy(K41_SCRIPT, target)
        if modifier:
            text = target.read_text(encoding="utf-8")
            text = modifier(text)
            target.write_text(text, encoding="utf-8")
        return target
    return _setup


def run_drift(script_path: Path, *args: str) -> subprocess.CompletedProcess:
    """跑 k41_drift_check.py 子進程, 回傳 CompletedProcess"""
    return subprocess.run(
        [PYTHON, str(SCRIPT), "--script", str(script_path), *args],
        capture_output=True, text=True, encoding="utf-8", errors="replace",
    )


# ---------- 5 case 護衛 ----------

def test_持平_對齊_R188_量化口徑_5_維度全_PASS():
    """k41_chore_treadmill.py 量化口徑常數全對齊 BASELINE → exit 0 PASS

    R191 baseline 對齊: GOVERNANCE_PREFIXES/WINDOW_DAYS/THRESHOLD/R176 雙分類修 全守。
    """
    r = run_drift(K41_SCRIPT)
    assert r.returncode == 0, f"預期 PASS, 實際 exit={r.returncode}\n{r.stdout}{r.stderr}"
    assert "全對齊" in r.stdout
    assert "5 維度" in r.stdout or "5 維度漂移" in r.stdout  # 後者是 FAIL 訊息


def test_GOVERNANCE_PREFIXES_改壞_觸發_REGRESS(tmp_k41_script):
    """改 GOVERNANCE_PREFIXES tuple 漏算 refactor → exit 1 FAIL, 訊息含「1 維度漂移」+ 指出 governance_prefixes

    M0 級 hidden gap 守護: 防有人改 tuple 漏算 refactor/archive/sensor, K41 量化值悄悄錯。
    """
    def _modifier(text: str) -> str:
        return text.replace(
            'GOVERNANCE_PREFIXES = ("chore", "refactor", "archive", "sensor")',
            'GOVERNANCE_PREFIXES = ("chore", "refactor", "sensor")',  # 漏 archive
        )
    p = tmp_k41_script(_modifier)
    r = run_drift(p)
    assert r.returncode == 1, f"預期 FAIL, 實際 exit={r.returncode}\n{r.stdout}{r.stderr}"
    assert "1 維度漂移" in r.stdout
    assert "governance_prefixes" in r.stdout


def test_THRESHOLD_改壞_觸發_REGRESS(tmp_k41_script):
    """改 THRESHOLD 從 0.30 到 0.50 → exit 1 FAIL, 訊息含「1 維度漂移」+ 指出 threshold

    M0 級 hidden gap 守護: 防有人放寬警戒線, K41 達標造假。
    """
    def _modifier(text: str) -> str:
        return text.replace("THRESHOLD = 0.30", "THRESHOLD = 0.50")
    p = tmp_k41_script(_modifier)
    r = run_drift(p)
    assert r.returncode == 1, f"預期 FAIL, 實際 exit={r.returncode}\n{r.stdout}{r.stderr}"
    assert "1 維度漂移" in r.stdout
    assert "threshold" in r.stdout


def test_WINDOW_DAYS_改壞_觸發_REGRESS(tmp_k41_script):
    """改 WINDOW_DAYS 從 7 到 30 → exit 1 FAIL, 訊息含「1 維度漂移」+ 指出 window_days

    M0 級 hidden gap 守護: 防有人放寬時間視窗, K41 量化口徑漂移。
    """
    def _modifier(text: str) -> str:
        return text.replace("WINDOW_DAYS = 7", "WINDOW_DAYS = 30")
    p = tmp_k41_script(_modifier)
    r = run_drift(p)
    assert r.returncode == 1, f"預期 FAIL, 實際 exit={r.returncode}\n{r.stdout}{r.stderr}"
    assert "1 維度漂移" in r.stdout
    assert "window_days" in r.stdout


def test_腳本不存在_回退碼_2(tmp_path):
    """k41_chore_treadmill.py 找不到 → exit 2, stderr 含「找不到」"""
    missing = tmp_path / "nonexistent_k41.py"
    r = run_drift(missing)
    assert r.returncode == 2
    assert "找不到" in r.stderr


# ---------- R202 內部函式 hidden gap 守護延伸 4 case ----------
# 對齊 R188/R195/R196/R198/R201 模式: pytest 護衛延伸 4 個 M0 級 hidden gap,
# 守 k41_drift_check.py 內部函式 (R191 5 case 只守「量化口徑常數被改壞」的外顯
# 行為, 沒守 4 個內部輔助函式自身的失守邊界):
#   1. _check_classify_chore_scope 改壞 (R176 fix 失守)
#   2. _check_classify_chore_plus_docs 改壞 (R176 fix 失守)
#   3. 雙分類修同時壞 (2 維度同時 REGRESS)
#   4. _extract_constant 找不到常數 (AST literal_eval 失敗 → exit 2)
# 換本質不同角度: R188 守 k0_measure 內部 / R195 守 chain_staleness 內部 /
# R196 守 K40 內部 / R198 守 K0 endpoint live 內部 / R201 守 K30 P95 內部 →
# R202 守 k41_drift_check 內部 = 第 6 個不同 KPI 維度對稱


def test__check_classify_chore_scope_改壞_R176_fix_失守_觸發_REGRESS(tmp_k41_script):
    """刪 k41_chore_treadmill.py 的 `if "(" in head` 行 (R176 M0 fix) →
    _check_classify_chore_scope 內部輔助函式回 False → 該維度 REGRESS, exit 1

    M0 級 hidden gap 守護: 防有人重構 k41_chore_treadmill.py 把 R176 修的
    chore(scope) 分類邏輯刪掉, k41_drift_check 內部 _check_classify_chore_scope
    函式沒獨立單元測試 → R176 fix 失守無人察覺, K41 量化值悄悄 undercount。
    """
    def _modifier(text: str) -> str:
        # 刪掉 R176 fix 的核心標記行, _check_classify_chore_scope 內部字串比對會 miss
        return text.replace('if "(" in head:', 'if False:')
    p = tmp_k41_script(_modifier)
    r = run_drift(p)
    assert r.returncode == 1, (
        f"預期 FAIL (R176 fix 失守 REGRESS), 實際 exit={r.returncode}\n"
        f"{r.stdout}{r.stderr}"
    )
    assert "1 維度漂移" in r.stdout
    assert "classify_chore_scope" in r.stdout


def test__check_classify_chore_plus_docs_改壞_R176_fix_失守_觸發_REGRESS(tmp_k41_script):
    """刪 k41_chore_treadmill.py 的 `head.split("+", 1)[0]` 行 (R176 M0 fix) →
    _check_classify_chore_plus_docs 內部輔助函式回 False → 該維度 REGRESS, exit 1

    M0 級 hidden gap 守護: 防有人重構把 R176 修的「chore(spec)+docs(...)」雙類型
    處理邏輯刪掉, k41_drift_check 內部 _check_classify_chore_plus_docs 函式沒
    獨立單元測試 → R115 等罕見雙類型 commit 全部漏算, K41 量化值悄悄錯。
    """
    def _modifier(text: str) -> str:
        return text.replace('head = head.split("+", 1)[0]', 'pass  # R176 fix 刪除')
    p = tmp_k41_script(_modifier)
    r = run_drift(p)
    assert r.returncode == 1, (
        f"預期 FAIL (R176 fix 失守 REGRESS), 實際 exit={r.returncode}\n"
        f"{r.stdout}{r.stderr}"
    )
    assert "1 維度漂移" in r.stdout
    assert "classify_chore_plus_docs" in r.stdout


def test_雙分類修同時壞_觸發_2_維度漂移_訊息列舉_兩個_key(tmp_k41_script):
    """R176 雙分類修同時被刪 → 2 維度同時 REGRESS, exit 1, 訊息含「2 維度漂移」+ 同時列舉兩個 key

    M0 級 hidden gap 守護: 防有人一次性 refactor 把 R176 兩個分類修都拿掉,
    k41_drift_check 內部兩個 _check_classify_chore_* 函式都要被守護, 確認
    measure() 串接兩個內部函式時 2 維度漂移都會被抓到, 報錯訊息明確列舉
    兩個 key (不能只報第一個就吞第二個)。
    """
    def _modifier(text: str) -> str:
        text = text.replace('if "(" in head:', 'if False:')
        text = text.replace('head = head.split("+", 1)[0]', 'pass  # R176 fix 刪除')
        return text
    p = tmp_k41_script(_modifier)
    r = run_drift(p)
    assert r.returncode == 1, (
        f"預期 FAIL (雙維度 REGRESS), 實際 exit={r.returncode}\n"
        f"{r.stdout}{r.stderr}"
    )
    assert "2 維度漂移" in r.stdout
    assert "classify_chore_scope" in r.stdout
    assert "classify_chore_plus_docs" in r.stdout


def test__extract_constant_AST_literal_eval_失敗_常數被替換_回退碼_2(tmp_k41_script):
    """把 GOVERNANCE_PREFIXES 從 tuple literal 改成函式呼叫 (BinOp) →
    _extract_constant 內部 ast.literal_eval 失敗 → main 捕 ValueError → exit 2

    M0 級 hidden gap 守護: 防有人把 k41_chore_treadmill.py 的量化口徑常數從
    literal 改成 compute() / list(...) / BinOp 動態算式, k41_drift_check 內部
    _extract_constant 函式會 raise ValueError, 確認 main 正確 catch 並回退碼 2
    (不是悄悄回 0 PASS, 也不是誤報 1 REGRESS; 既有 test_腳本不存在 守的是
    file-level 找不到, 這個守的是 file 在但常數型別被改)。
    """
    def _modifier(text: str) -> str:
        # 改成 BinOp (tuple concatenation), ast.literal_eval 不支援
        return text.replace(
            'GOVERNANCE_PREFIXES = ("chore", "refactor", "archive", "sensor")',
            'GOVERNANCE_PREFIXES = ("chore",) + ("refactor", "archive", "sensor")',
        )
    p = tmp_k41_script(_modifier)
    r = run_drift(p)
    assert r.returncode == 2, (
        f"預期 FAIL (AST literal_eval 失敗 → exit 2), 實際 exit={r.returncode}\n"
        f"{r.stdout}{r.stderr}"
    )
    # main 捕 ValueError → print "[FAIL] 解析 {path} 失敗: malformed node ..."
    # (ast.literal_eval 對 BinOp raise 的訊息是 "malformed node or string on line N")
    assert "失敗" in r.stderr
    assert "malformed" in r.stderr or "literal" in r.stderr.lower()
