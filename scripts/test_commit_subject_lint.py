#!/usr/bin/env python3
"""
commit_subject_lint.py 5 case pytest — R167 補測試, 補 R137 留的 test gap

對齊 R132 量化閉合護衛鏈慣例: 純 stdlib pytest, 5 case 鎖純函式 +
CLI smoke。 不擴 K42 chain 20 條飽和契約 (Python script 不算 Rust
護衛, 走 R124 sentinel test 同樣路徑, 既無既有護衛維度)。
"""
import subprocess
import sys
from pathlib import Path

import pytest

SCRIPT = Path(__file__).resolve().parent / "commit_subject_lint.py"
PYTHON = sys.executable


# ---------- helpers ----------

@pytest.fixture(scope="module")
def csl():
    """載入 commit_subject_lint 模組一次, 給純函式測試用"""
    sys.path.insert(0, str(SCRIPT.parent))
    import commit_subject_lint  # noqa: E402
    return commit_subject_lint


def run_cli(*args: str) -> subprocess.CompletedProcess:
    """跑 commit_subject_lint.py CLI 子進程, 捕 stdout/stderr"""
    return subprocess.run(
        [PYTHON, str(SCRIPT), *args],
        capture_output=True, text=True, encoding="utf-8", errors="replace",
    )


# ---------- 5 case 護衛 ----------

def test_parse_type_scope_三_形式(csl):
    """parse_type_scope 必須正確解析 3 種 form: with-scope / no-scope / no-colon"""
    # with-scope
    t, s, r, full = csl.parse_type_scope("chore(scripts): add X")
    assert (t, s, r) == ("chore", "scripts", "add X")
    # no-scope
    t, s, r, full = csl.parse_type_scope("chore: rotate log")
    assert (t, s, r) == ("chore", None, "rotate log")
    # no colon (非 conventional)
    t, s, r, full = csl.parse_type_scope("WIP on foo")
    assert (t, s, r, full) == (None, None, None, "WIP on foo")
    # 未知 type (e.g. "Merge") → 一律 None
    t, s, r, full = csl.parse_type_scope("Merge: branch X")
    assert t is None
    assert full == "Merge: branch X"


def test_lint_長_subject_被_抓出(csl):
    """lint: subject > max_len 進 long_subjects, 不進 no_scope"""
    subjects = [
        ("aaaaaaa", "x" * 80),  # long, no type prefix → 不算 no_scope (type=None)
        ("bbbbbbb", "chore: short"),  # in range, no scope
        ("ccccccc", "feat(scope): also short"),  # in range, with scope
    ]
    result = csl.lint(subjects, max_len=72)
    assert len(result["long_subjects"]) == 1
    assert result["long_subjects"][0]["sha"] == "aaaaaaa"
    assert result["long_subjects"][0]["length"] == 80
    assert len(result["no_scope"]) == 1
    assert result["no_scope"][0]["sha"] == "bbbbbbb"
    assert result["no_scope"][0]["type"] == "chore"
    assert result["total"] == 3


def test_lint_空_輸入_回_零(csl):
    """lint: 空 subjects → 0 long / 0 no_scope / 0 total"""
    result = csl.lint([], max_len=72)
    assert result == {"long_subjects": [], "no_scope": [], "total": 0}


def test_format_report_含_兩_段(csl):
    """format_report: 印出 total + long 段 + no_scope 段 + 標題"""
    result = {
        "long_subjects": [{"sha": "abc1234", "subject": "x" * 90, "length": 90}],
        "no_scope": [{"sha": "def5678", "subject": "chore: rotate log", "type": "chore"}],
        "total": 5,
    }
    report = csl.format_report(result, max_len=72)
    # 必含三段標題
    assert "total commits scanned: 5" in report
    assert "long subjects (> 72 chars): 1" in report
    assert "known-type without scope: 1" in report
    assert "Long subjects" in report
    assert "abc1234" in report
    assert "No-scope subjects" in report
    assert "def5678" in report
    assert "chore" in report


def test_main_exit_0_且_JSON_含_keys(csl):
    """main(): 跑 --limit 3 --json → exit 0 + stdout 是合法 JSON + 含三 key

    R167 護衛: 純 audit 工具退出碼永遠 0 (不入 chain 護衛, 留 owner M
    簽收時決定 fail-closed)。 子進程跑避免污染 csl singleton 狀態。
    """
    proc = run_cli("--limit", "3", "--json")
    assert proc.returncode == 0, (
        f"audit 工具應永遠 exit 0, 實際 {proc.returncode}; "
        f"stderr={proc.stderr!r}"
    )
    import json
    payload = json.loads(proc.stdout)
    assert "long_subjects" in payload
    assert "no_scope" in payload
    assert "total" in payload
    assert payload["total"] == 3
