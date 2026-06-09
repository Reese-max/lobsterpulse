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
