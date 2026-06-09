#!/usr/bin/env python3
r"""
K40 量化漂移偵測護衛 — 5 case pytest, 對齊 R192 量化真實值 baseline 守護

R193 落地。鏡像 R132 test_k0_drift_check.py 模式 (M2 KPI 量測 closure 軸
守護本體延伸)。1 個 Python pytest 模組, 走既無既有護衛維度 (Python
script 不算 Rust 護衛, 不破 K42 chain 20 條飽和契約)。

5 case 守 5 個 K40 量化口徑 hidden gap:
  1. 持平 → exit 0 PASS (對齊 R192 baseline)
  2. 進步 (closed 增加) → exit 0 PASS (K40 量化值變好)
  3. 倒退 (closed 減少) → exit 1 FAIL
  4. 缺欄位 → exit 2 (解析失敗)
  5. JSON 損壞 → exit 2 (解析失敗)
"""
import json
import subprocess
import sys
from pathlib import Path

import pytest

SCRIPT = Path(__file__).resolve().parent / "k40_drift_check.py"
K40_JSON = Path(__file__).resolve().parent.parent / ".harness-k40.json"
PYTHON = sys.executable


# ---------- fixtures ----------

@pytest.fixture
def tmp_k40_json(tmp_path):
    """factory: 寫假 .harness-k40.json 進 tmp, 回傳 path"""
    def _setup(payload: dict | None = None,
               raw: str | None = None,
               missing: list[str] | None = None) -> Path:
        target = tmp_path / ".harness-k40.json"
        if raw is not None:
            target.write_text(raw, encoding="utf-8")
        else:
            data = {
                "k40_changes_total": 10,
                "k40_changes_closed": 8,
                "k40_changes_active": 2,
                "active_names": [
                    "mission-k0-restructure-2026-q3",
                    "otel-genai-runtime-emit-2026-q3",
                ],
            }
            if missing:
                for k in missing:
                    data.pop(k, None)
            if payload:
                data.update(payload)
            target.write_text(
                json.dumps(data, indent=2, ensure_ascii=False),
                encoding="utf-8",
            )
        return target
    return _setup


def run_drift(json_path: Path, *args: str) -> subprocess.CompletedProcess:
    """跑 k40_drift_check.py 子進程, 回傳 CompletedProcess"""
    return subprocess.run(
        [PYTHON, str(SCRIPT), "--json", str(json_path), *args],
        capture_output=True, text=True, encoding="utf-8", errors="replace",
    )


# ---------- 5 case 護衛 ----------

def test_持平_對齊_R192_量化真實值_4_維度全_PASS():
    """k40_measure.py 量化真實值全對齊 BASELINE → exit 0 PASS

    R193 baseline 對齊: total=10 + closed=8 + active=2 + active_names
    [mission-k0, otel-genai] 全守。
    """
    if not K40_JSON.exists():
        pytest.skip(f".harness-k40.json 找不到: {K40_JSON} (先跑 k40_measure.py)")
    r = run_drift(K40_JSON)
    assert r.returncode == 0, f"預期 PASS, 實際 exit={r.returncode}\n{r.stdout}{r.stderr}"
    assert "對齊 R192 baseline" in r.stdout or "守住" in r.stdout
    assert "FAIL" not in r.stdout.split("=" * 110)[-1]  # 最後一段不該有 FAIL


def test_進步_closed_增加_active_縮減_也_PASS(tmp_k40_json):
    """K40 量化值變好 (closed 10/10 = 8→10, active 0/2 = 2→0) → exit 0 PASS

    進步是好事, 預設模式不 fail (對齊 k0_drift_check.py R132 預設行為)。
    """
    p = tmp_k40_json(payload={
        "k40_changes_total": 10,
        "k40_changes_closed": 10,  # 從 8 進步到 10
        "k40_changes_active": 0,   # 從 2 進步到 0
        "active_names": [],         # 全 closed
    })
    r = run_drift(p)
    assert r.returncode == 0, f"預期 PASS (進步), 實際 exit={r.returncode}\n{r.stdout}{r.stderr}"
    assert "進步" in r.stdout or "守住" in r.stdout


def test_倒退_closed_減少_觸發_REGRESS(tmp_k40_json):
    """改 closed 從 8 → 6 (active 從 2 → 4) → exit 1 FAIL, 訊息含「1 維度倒退」+ 指出 k40_changes_closed

    M0 級 hidden gap 守護: 防有人改 k40_measure.py 算法 (e.g. 漏算 archive/ 排除)
    或刪 tasks.md 導致 K40 量化值倒退。
    """
    p = tmp_k40_json(payload={
        "k40_changes_total": 10,
        "k40_changes_closed": 6,  # 從 8 倒退到 6
        "k40_changes_active": 4,  # 從 2 倒退到 4
        "active_names": [
            "extra-1",
            "extra-2",
            "mission-k0-restructure-2026-q3",
            "otel-genai-runtime-emit-2026-q3",
        ],
    })
    r = run_drift(p)
    assert r.returncode == 1, f"預期 FAIL, 實際 exit={r.returncode}\n{r.stdout}{r.stderr}"
    # 至少有 k40_changes_closed 倒退, total 也可能差 (active_names set 不同也算)
    assert "倒退" in r.stdout
    assert "k40_changes_closed" in r.stdout or "k40_changes_active" in r.stdout \
        or "active_names" in r.stdout


def test_缺欄位_k40_changes_total_缺失_回退碼_2(tmp_k40_json):
    """缺 k40_changes_total 欄位 → exit 2 (KeyError 解析失敗)

    防有人改 .harness-k40.json schema 漏欄位而 k40_drift_check.py 靜默放行。
    """
    p = tmp_k40_json(missing=["k40_changes_total"])
    r = run_drift(p)
    assert r.returncode == 2, f"預期 exit=2, 實際 exit={r.returncode}\n{r.stdout}{r.stderr}"
    assert "解析失敗" in r.stderr or "找不到" in r.stderr


def test_JSON_損壞_回退碼_2(tmp_k40_json):
    """JSON 壞 (raw 不合法) → exit 2 (JSONDecodeError 解析失敗)

    防 .harness-k40.json 被破壞 (e.g. 寫入中斷 / 手編輯) 而漂移偵測靜默放行。
    """
    p = tmp_k40_json(raw='{"k40_changes_total": 10, "broken": tru')  # 截斷
    r = run_drift(p)
    assert r.returncode == 2, f"預期 exit=2, 實際 exit={r.returncode}\n{r.stdout}{r.stderr}"
    assert "解析失敗" in r.stderr or "Expecting" in r.stderr or "delimiter" in r.stderr
