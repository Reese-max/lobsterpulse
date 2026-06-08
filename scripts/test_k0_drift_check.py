#!/usr/bin/env python3
"""
K0 漂移偵測護衛 — 5 case pytest, 對齊 R132 量化閉合護衛鏈

R132 落地。1 個 Python pytest 模組, 走既無既有護衛維度 (Python
script 不算 Rust 護衛, 不破 K42 chain 19 條飽和契約)。
"""
import json
import subprocess
import sys
from pathlib import Path

import pytest

SCRIPT = Path(__file__).resolve().parent / "k0_drift_check.py"
PYTHON = sys.executable


# ---------- fixtures ----------

@pytest.fixture
def write_k0_json(tmp_path):
    """factory: 寫 1 個 .harness-k0.json 進 tmp, 回傳 path"""
    def _write(emit: int, sample: int, fresh: int, coverage: int) -> Path:
        payload = {
            "timestamp": "2026-06-06T16:31:40+0800",
            "metrics_endpoint_alive": True,
            "providers_total": 13,
            "k0a1_health_emit": {"covered": emit, "total": 13, "pct": round(emit/13*100, 1)},
            "k0a2_health_sample": {"covered": sample, "total": 13, "pct": round(sample/13*100, 1)},
            "k0b_quota_freshness": {"fresh": fresh, "total": 13, "pct": round(fresh/13*100, 1)},
            "k0q_quota_coverage": {"covered": coverage, "total": 13, "pct": round(coverage/13*100, 1)},
        }
        p = tmp_path / ".harness-k0.json"
        p.write_text(json.dumps(payload), encoding="utf-8")
        return p
    return _write


def run_drift(json_path: Path, *args: str) -> subprocess.CompletedProcess:
    """跑 k0_drift_check.py 子進程, 回傳 CompletedProcess"""
    return subprocess.run(
        [PYTHON, str(SCRIPT), "--json", str(json_path), *args],
        capture_output=True, text=True, encoding="utf-8", errors="replace",
    )


# ---------- 5 case 護衛 ----------

def test_持平_對齊_R150_baseline(write_k0_json):
    """持平 (4/1/4/9) → exit 0, 訊息含「全部持平」

    R150 baseline 對齊: K0-A1 5→4 (cicx OpenAB scope 浮動, 4 為本機穩態下限)。
    """
    p = write_k0_json(emit=4, sample=1, fresh=4, coverage=9)
    r = run_drift(p)
    assert r.returncode == 0, f"預期 PASS, 實際 exit={r.returncode}\n{r.stdout}{r.stderr}"
    assert "全部持平" in r.stdout


def test_倒退_K0_A1_從_4_掉到_3_觸發_REGRESS(write_k0_json):
    """K0-A1 倒退 (3 < 4) → exit 1, 訊息含「1 維度倒退」+ 指出 K0-A1

    R150 baseline 對齊: 倒退偵測仍守住 (3 < R150 baseline 4)。
    """
    p = write_k0_json(emit=3, sample=1, fresh=4, coverage=9)
    r = run_drift(p)
    assert r.returncode == 1, f"預期 FAIL, 實際 exit={r.returncode}\n{r.stdout}{r.stderr}"
    assert "1 維度倒退" in r.stdout
    assert "k0a1_emit_covered" in r.stdout


def test_進步_K0_A1_從_4_升到_5_預設_PASS_strict_FAIL(write_k0_json):
    """K0-A1 進步 (5 > 4) → 預設 exit 0 PASS, --strict exit 1 FAIL

    R150 baseline 對齊: 進步偵測門檻 4→5。
    """
    p = write_k0_json(emit=5, sample=1, fresh=4, coverage=9)
    # 預設模式
    r1 = run_drift(p)
    assert r1.returncode == 0
    assert "1 維度進步" in r1.stdout
    # --strict 模式
    r2 = run_drift(p, "--strict")
    assert r2.returncode == 1
    assert "1 維度進步" in r2.stdout


def test_缺_json_檔_回退碼_2(tmp_path):
    """JSON 檔不存在 → exit 2, stderr 含「找不到」"""
    missing = tmp_path / "nonexistent.json"
    r = run_drift(missing)
    assert r.returncode == 2
    assert "找不到" in r.stderr


def test_壞_JSON_回退碼_2(tmp_path):
    """JSON parse 失敗 → exit 2, stderr 含「解析失敗」"""
    bad = tmp_path / ".harness-k0.json"
    bad.write_text("{not valid json", encoding="utf-8")
    r = run_drift(bad)
    assert r.returncode == 2
    assert "解析失敗" in r.stderr
