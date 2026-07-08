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

def test_持平_對齊_R212_truthful_runner_baseline(write_k0_json):
    """持平 (2/1/2/7) → exit 0, 訊息含「全部持平」

    R212 baseline 對齊: usage-local.json 只含 claude/codex runner, 不再把
    copilot/gemini 誤算 fresh。
    """
    p = write_k0_json(emit=2, sample=1, fresh=2, coverage=7)
    r = run_drift(p)
    assert r.returncode == 0, f"預期 PASS, 實際 exit={r.returncode}\n{r.stdout}{r.stderr}"
    assert "全部持平" in r.stdout


def test_倒退_K0_A1_從_2_掉到_1_觸發_REGRESS(write_k0_json):
    """K0-A1 倒退 (1 < 2) → exit 1, 訊息含「1 維度倒退」+ 指出 K0-A1

    R212 baseline 對齊: 倒退偵測仍守住 (1 < R212 baseline 2)。
    """
    p = write_k0_json(emit=1, sample=1, fresh=2, coverage=7)
    r = run_drift(p)
    assert r.returncode == 1, f"預期 FAIL, 實際 exit={r.returncode}\n{r.stdout}{r.stderr}"
    assert "1 維度倒退" in r.stdout
    assert "k0a1_emit_covered" in r.stdout


def test_進步_K0_A1_從_2_升到_3_預設_PASS_strict_FAIL(write_k0_json):
    """K0-A1 進步 (3 > 2) → 預設 exit 0 PASS, --strict exit 1 FAIL

    R212 baseline 對齊: 進步偵測門檻 2→3。
    """
    p = write_k0_json(emit=3, sample=1, fresh=2, coverage=7)
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


# === R204 內部函式 hidden gap 守護延伸 4 case ===
# 對齊 R188 k0_measure / R195 chain_staleness / R196 K40 / R198 K0 endpoint live
# / R201 K30 P95 / R202 K41 drift / R203 k0_target_baseline_check 內部函式既模式
# = 跨 8 個不同 KPI 維度對稱 (R203 7 維度飽和 → R204 第 8 維度 transferability
# validation, k0_drift_check.py 3 個內部函式 load_current / compute_drift /
# render_report 各 M0 級 hidden gap 邊界守護)
import importlib.util as _ilu

_K0_DC_SPEC = _ilu.spec_from_file_location(
    "k0_drift_check_module", SCRIPT
)
_k0_dc = _ilu.module_from_spec(_K0_DC_SPEC)
_K0_DC_SPEC.loader.exec_module(_k0_dc)


def test_load_current_缺_k0a1_health_emit_nested_KeyError(tmp_path):
    """load_current() 缺巢狀 key k0a1_health_emit → KeyError (fail-fast)

    M0 級 hidden gap 守護: 防 k0_measure.py 改 schema (e.g. k0a1_health_emit →
    k0a1_emit / k0a1_health) 而 k0_drift_check.py load_current 預設靜默處理,
    current.get 預設 0 觸發假 REGRESS 卻沒人知。
    """
    bad = tmp_path / ".harness-k0.json"
    payload = {
        "timestamp": "2026-06-11T00:00:00+0800",
        "metrics_endpoint_alive": True,
        "providers_total": 13,
        # k0a1_health_emit 整個 missing
        "k0a2_health_sample": {"covered": 1, "total": 13, "pct": 7.7},
        "k0b_quota_freshness": {"fresh": 2, "total": 13, "pct": 15.4},
        "k0q_quota_coverage": {"covered": 7, "total": 13, "pct": 53.8},
    }
    bad.write_text(json.dumps(payload), encoding="utf-8")
    with pytest.raises(KeyError) as exc_info:
        _k0_dc.load_current(bad)
    assert "k0a1_health_emit" in str(exc_info.value)


def test_load_current_covered_是字串_自動轉_int_2():
    """load_current() covered 欄位是字串 "2" → int("2") = 2 (type coercion 守護)

    守住 load_current() 內 int(data[...]["covered"]) 的 type coercion 邏輯
    (對齊 chain_staleness 內 _compute_delta 同模式)。防有人改 k0_measure.py
    量化輸出從 int 改 str (e.g. json 序列化用 ensure_ascii=False 漏 type 標記)
    而 k0_drift_check.load_current 因 type error crash 或悄悄回 0。
    """
    import tempfile
    with tempfile.NamedTemporaryFile(
        mode="w", suffix=".json", delete=False, encoding="utf-8"
    ) as f:
        payload = {
            "k0a1_health_emit": {"covered": "2", "total": 13, "pct": 15.4},
            "k0a2_health_sample": {"covered": "1", "total": 13, "pct": 7.7},
            "k0b_quota_freshness": {"fresh": "2", "total": 13, "pct": 15.4},
            "k0q_quota_coverage": {"covered": "7", "total": 13, "pct": 53.8},
        }
        f.write(json.dumps(payload))
        f.flush()
        cur = _k0_dc.load_current(Path(f.name))
    assert cur == {
        "k0a1_emit_covered": 2,
        "k0a2_sample_covered": 1,
        "k0b_fresh": 2,
        "k0q_coverage": 7,
    }


def test_compute_drift_current_缺_key_預設_0_觸發_REGRESS():
    """compute_drift() current 缺 k0a1_emit_covered → 預設 0, delta=-2 → REGRESS

    守住 M0 級 hidden gap: 防止 k0_measure.py schema 改時 k0_drift_check
    假 PASS (current.get(key, 0) 預設 0 不 raise 而是悄悄退步)。

    對齊 R188 k0_measure / R195 chain_staleness / R196 K40 / R198 K0 endpoint
    live / R201 K30 P95 / R202 K41 drift / R203 k0_target_baseline_check
    內部函式 hidden gap 守護模式: 當前值缺漏 → fail-closed 而非 silent 放行。
    """
    current = {
        # k0a1_emit_covered 缺 (模擬 schema 漂移 / k0_measure.py 量化少算 1 維)
        "k0a2_sample_covered": 1,
        "k0b_fresh": 2,
        "k0q_coverage": 7,
    }
    results = _k0_dc.compute_drift(current)
    k0a1 = next(r for r in results if r.key == "k0a1_emit_covered")
    assert k0a1.current == 0, f"缺 key 應預設 0, 實際 current={k0a1.current}"
    assert k0a1.delta == -2, f"BASELINE=2 缺 key 預設 0 → delta=-2, 實際={k0a1.delta}"
    assert k0a1.status == "REGRESS", f"delta<0 必觸發 REGRESS, 實際={k0a1.status}"


def test_render_report_delta_為_0_顯示_兩空格_不帶_sign():
    """render_report() delta=0 → "  0" (3 字元寬, 無 +sign), 進步/倒退帶 sign

    守住 M0 級 hidden gap: 報表格式簽一致 (對齊 R132 設計取捨 — 持平用 2 空格
    + 0, 進步用 +N, 倒退用 -N, 防 f-string 格式被人改成 f"{r.delta:+d}"
    一律帶 sign 讓持平顯示 +0 破壞 R132 量化報表可讀性)。

    對齊 R188 6→9 / R195 8→11 內部函式 hidden gap 邊界守護模式。
    """
    r0 = _k0_dc.DriftResult("k0a1_emit_covered", 2, 2, 0, "PASS")  # 持平
    r1 = _k0_dc.DriftResult("k0a2_sample_covered", 1, 2, 1, "PASS")  # 進步
    r2 = _k0_dc.DriftResult("k0b_fresh", 2, 1, -1, "REGRESS")  # 倒退
    report = _k0_dc.render_report([r0, r1, r2])
    assert "  0" in report, f"delta=0 該顯示 '  0' (兩個空格 + 0), 實際報表:\n{report}"
    assert "+1" in report, f"delta=+1 該有 +sign, 實際報表:\n{report}"
    assert "-1" in report, f"delta=-1 該有 -sign, 實際報表:\n{report}"
    assert " +0" not in report, f"持平不該有 +sign, 實際報表:\n{report}"
