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


# === R207 內部函式 hidden gap 守護延伸 4 case ===
# 對齊 R188 k0_measure / R195 chain_staleness / R196 K40 producer (k40_measure)
# / R198 K0 endpoint live / R201 K30 P95 / R202 K41 drift / R203 k0_target_baseline_check
# / R204 k0_drift_check (transferability validation 第 1 對象)
# / R206 chain_staleness_drift_check (transferability validation 第 2 對象)
# 內部函式既模式 = 跨 9 個不同 KPI 維度對稱 (R206 9 維度飽和 →
# R207 第 10 維度 transferability validation 第 3 對象, k40_drift_check.py
# 3 個內部函式 load_current / compute_drift / render_report 各 M0 級
# hidden gap 邊界守護)。
#
# R207 transferability validation 特色: k40_drift_check.py 設計選擇跟
# k0_drift_check.py / chain_staleness_drift_check.py 不一樣 —
# compute_drift 用 direct dict access (KeyError fail-fast), 不是 .get(key, 0)
# 預設 0。hidden gap 守護套到不同設計選擇仍守住 = 真正 transferability:
#   - k0 / chain_staleness: 悄悄退步但 REGRESS 一定觸發 (.get 預設 0)
#   - k40: 立刻 KeyError crash (direct access, fail-closed)
# 兩策略都守住「不靜默放行」M0 級 hidden gap, 證明 pattern 可跨設計 transfer。
import importlib.util as _ilu

_K40_DC_SPEC = _ilu.spec_from_file_location(
    "k40_drift_check_module", SCRIPT
)
_k40_dc = _ilu.module_from_spec(_K40_DC_SPEC)
_K40_DC_SPEC.loader.exec_module(_k40_dc)


def test_load_current_缺_k40_changes_total_nested_KeyError(tmp_path):
    """load_current() 缺 k40_changes_total → KeyError (fail-fast, 不靜默 default)

    M0 級 hidden gap 守護: 防 k40_measure.py 改 schema (e.g. k40_changes_total
    → k40_total / 整個 key 改名) 而 k40_drift_check.load_current 因 .get 預設
    處理而悄悄回 0 → 觸發假 REGRESS 卻沒人知 (cascade 影響 compute_drift 4 維度)。
    """
    bad = tmp_path / ".harness-k40.json"
    payload = {
        # k40_changes_total 整個 missing
        "k40_changes_closed": 8,
        "k40_changes_active": 2,
        "active_names": [
            "mission-k0-restructure-2026-q3",
            "otel-genai-runtime-emit-2026-q3",
        ],
    }
    bad.write_text(json.dumps(payload), encoding="utf-8")
    with pytest.raises(KeyError) as exc_info:
        _k40_dc.load_current(bad)
    assert "k40_changes_total" in str(exc_info.value)


def test_load_current_k40_changes_total_是字串_自動轉_int(tmp_path):
    """load_current() 數字欄位是字串 "10" → int("10") = 10 (type coercion 守護)

    守住 load_current() 內 int(data["k40_changes_total"]) 的 type coercion
    邏輯 (對齊 k0_drift_check R204 / chain_staleness 內 _compute_delta 同模式)。
    防有人改 k40_measure.py 量化輸出從 int 改 str (e.g. json 序列化改 ensure_ascii
    或 schema 標記 type 變動) 而 k40_drift_check.load_current 因 type error crash
    或悄悄回 0。
    """
    p = tmp_path / ".harness-k40.json"
    payload = {
        "k40_changes_total": "10",  # string 不是 int
        "k40_changes_closed": "8",  # string
        "k40_changes_active": "2",  # string
        "active_names": [
            "mission-k0-restructure-2026-q3",
            "otel-genai-runtime-emit-2026-q3",
        ],
    }
    p.write_text(json.dumps(payload), encoding="utf-8")
    cur = _k40_dc.load_current(p)
    assert cur["k40_changes_total"] == 10
    assert cur["k40_changes_closed"] == 8
    assert cur["k40_changes_active"] == 2
    assert cur["active_names"] == (
        "mission-k0-restructure-2026-q3",
        "otel-genai-runtime-emit-2026-q3",
    )
    assert isinstance(cur["k40_changes_total"], int), \
        f"type coercion 該回 int, 實際={type(cur['k40_changes_total'])}"
    assert isinstance(cur["k40_changes_closed"], int)
    assert isinstance(cur["k40_changes_active"], int)


def test_compute_drift_current_缺_k40_changes_closed_觸發_KeyError_fail_closed():
    """compute_drift() current 缺 k40_changes_closed → KeyError (direct access fail-fast)

    守住 M0 級 hidden gap: 防止 k40_measure.py schema 改時 k40_drift_check
    假 PASS 或悄悄退步。

    R207 transferability 對照: k40_drift_check 設計用 direct dict access
    (`current["k40_changes_closed"]`), 跟 k0_drift_check / chain_staleness_drift_check
    用 .get(key, 0) 預設 0 的策略不同。兩策略都守住 hidden gap (不靜默放行):
      - k0 / chain_staleness: 缺 key 預設 0, delta=負數 → REGRESS (status 訊號觸發)
      - k40: 缺 key 直接 KeyError, main() catch 不到 KeyError (只 catch FileNotFoundError
        / JSONDecodeError) 會 crash, 但這也是 fail-closed (Python traceback 比
        假 REGRESS 訊號更明確, 工程師一看就懂 schema 壞了)

    對齊 R188 k0_measure / R195 chain_staleness / R196 K40 / R198 K0 endpoint live
    / R201 K30 P95 / R202 K41 drift / R203 k0_target_baseline_check / R204 k0_drift_check
    / R206 chain_staleness_drift_check 內部函式 hidden gap 守護模式: 套到不同
    設計選擇 (direct access vs .get) 仍守住不靜默放行 = 真正 transferability。
    """
    current = {
        "k40_changes_total": 10,
        # k40_changes_closed 缺 (模擬 schema 漂移 / k40_measure.py 量化少算 1 維)
        "k40_changes_active": 2,
        "active_names": [
            "mission-k0-restructure-2026-q3",
            "otel-genai-runtime-emit-2026-q3",
        ],
    }
    with pytest.raises(KeyError) as exc_info:
        _k40_dc.compute_drift(current)
    assert "k40_changes_closed" in str(exc_info.value), \
        f"KeyError 訊息該含 k40_changes_closed, 實際={exc_info.value}"


def test_render_report_delta_為_0_顯示_兩空格_不帶_sign():
    """render_report() delta=0 → "  0" (3 字元寬, 無 +sign), 進步/倒退帶 sign

    守住 M0 級 hidden gap: 報表格式簽一致 (對齊 k40_drift_check 設計取捨 —
    持平用 2 空格 + 0, 進步用 +N, 倒退用 -N, 防 f-string 格式被人改成
    f"{r.delta:+d}" 一律帶 sign 讓持平顯示 +0 破壞 R193 量化報表可讀性)。

    對齊 R188 6→9 / R195 8→11 / R196 9→13 / R198 5→9 / R201 4→8 / R202 5→9
    / R203 6→10 / R204 5→9 / R206 5→9 內部函式 hidden gap 邊界守護模式,
    R207 5→9 鏡像 R204 5→9 結構 (k40 跟 k0 都是 5 baseline case + 4 內部守護 = 9)。
    """
    r0 = _k40_dc.DriftResult("k40_changes_total", 10, 10, 0, "PASS")  # 持平
    r1 = _k40_dc.DriftResult("k40_changes_closed", 8, 9, 1, "PASS")  # 進步
    r2 = _k40_dc.DriftResult("k40_changes_active", 2, 4, 2, "REGRESS")  # 倒退
    report = _k40_dc.render_report([r0, r1, r2])
    assert "  0" in report, f"delta=0 該顯示 '  0' (2 空格 + 0), 實際報表:\n{report}"
    assert "+1" in report, f"delta=+1 該有 +sign, 實際報表:\n{report}"
    assert "+2" in report, f"delta=+2 該有 +sign, 實際報表:\n{report}"
    assert " +0" not in report, f"持平不該有 +sign, 實際報表:\n{report}"
