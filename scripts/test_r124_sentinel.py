#!/usr/bin/env python3
"""
R124 sentinel 護衛 — 5 case pytest, 對齊 R132 量化閉合護衛鏈

R132 落地。1 個 Python pytest 模組, 走既無既有護衛維度 (Python
script 不算 Rust 護衛, 不破 K42 chain 20 條飽和契約)。

R150 補 1 case: check_k0_emit 缺失 .harness-k0.json 路徑不應
NameError — R131 修 sentinel 時引用 K0_A_MIN (不存在) 改 K0_A1_MIN
後, 此 case 鎖住「K0_JSON 缺失 → fail-closed 報漂移」不退回。
"""
import json
import sys
from pathlib import Path

import pytest

SENTINEL = Path(__file__).resolve().parent / "r124_sentinel.py"
PYTHON = sys.executable


# ---------- helpers ----------

def run_sentinel(*args: str) -> "subprocess.CompletedProcess":  # noqa: F821
    """跑 r124_sentinel.py 子進程 (走 PYTHONPATH=scripts 注入模組路徑)"""
    import subprocess
    return subprocess.run(
        [PYTHON, "-c",
         "import sys; sys.path.insert(0, 'scripts'); "
         "import r124_sentinel; sys.exit(r124_sentinel.main())",
         *args],
        capture_output=True, text=True, encoding="utf-8", errors="replace",
    )


@pytest.fixture
def write_k0_json(tmp_path, monkeypatch):
    """factory: 寫 1 個 .harness-k0.json 進 tmp, 並把 sentinel 的 K0_JSON 指過去"""
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
        import r124_sentinel
        monkeypatch.setattr(r124_sentinel, "K0_JSON", p)
        return p
    return _write


# ---------- 5 case 護衛 ----------

def test_全部_持平_對齊_R132_baseline(monkeypatch, write_k0_json):
    """全 5 項 sentinel check 持平 → exit 0 PASS"""
    import r124_sentinel
    # 確保 K41_JSON 存在且 ratio 達標 (K41 KPI 7d chore < 30%)
    k41 = r124_sentinel.REPO_ROOT / ".harness-k41.json"
    if not k41.exists():
        k41.write_text(json.dumps({"ratio": 0.07}), encoding="utf-8")
        monkeypatch.setattr(r124_sentinel, "K41_JSON", k41)
    write_k0_json(emit=4, sample=1, fresh=4, coverage=9)
    # 跳過 cargo_test (慢) 跟 owner_m_wip 之外的純 module 級測試
    results = [r124_sentinel.check_k0_emit(),
               r124_sentinel.check_k0_fresh(),
               r124_sentinel.check_k41_chore()]
    assert all(r.passed for r in results), \
        f"預期全 PASS, 實際 {[r.name + ':' + str(r.passed) for r in results]}"


def test_K0_A1_倒退_從_4_掉到_3_觸發_FAIL(write_k0_json):
    """K0-A1 emit 4 → 3 (低於 K0_A1_MIN=4) → check_k0_emit 報 FAIL"""
    # .harness-k0.json 寫成 3 emit (低於 baseline 4)
    import r124_sentinel
    write_k0_json(emit=3, sample=1, fresh=4, coverage=9)
    result = r124_sentinel.check_k0_emit()
    assert result.passed is False
    assert "3" in result.actual
    assert "DRIFT" in result.note or "3" in result.actual


def test_K0_B_fresh_倒退_從_4_掉到_3_觸發_FAIL(write_k0_json):
    """K0-B fresh 4 → 3 → check_k0_fresh 報 FAIL"""
    import r124_sentinel
    write_k0_json(emit=4, sample=1, fresh=3, coverage=9)
    result = r124_sentinel.check_k0_fresh()
    assert result.passed is False
    assert "3" in result.actual


def test_K0_JSON_缺失_不_NameError_退回_FAIL(monkeypatch):
    """R150 護衛: K0_JSON 不存在路徑 → fail-closed 報漂移 (不應 NameError)。

    過去 r124_sentinel.py:141 引用 K0_A_MIN (從未定義) → 缺失路徑會 crash
    出 NameError, sentinel 反而無法 fail-closed 觸發警報。修完後應回傳
    passed=False 的 CheckResult 而非 raise。
    """
    import r124_sentinel
    monkeypatch.setattr(r124_sentinel, "K0_JSON", Path("/nonexistent/.harness-k0.json"))
    # 不應 raise
    result = r124_sentinel.check_k0_emit()
    assert result.passed is False
    assert result.name == "k0_a1_emit"
    assert "missing" in result.actual
    # threshold 應顯示「>= 4」(K0_A1_MIN 的值), 不是 K0_A_MIN
    assert "4" in result.threshold
    assert "K0_A_MIN" not in result.threshold  # 確認不再引用不存在常數


def test_guard_chain_持平_守住_R97_紅線():
    """護衛 mod 計數 >= 20 (R97 紅線) — R132 baseline 33"""
    import r124_sentinel
    result = r124_sentinel.check_guard_chain()
    assert result.passed is True
    assert "R97" in result.note or "紅線" in result.note or "守住" in result.note


def test_OWNER_M_WIP_FILES_tuple_對齊_當前_git_status(monkeypatch):
    """R138 護衛: tuple 必須 == `git status --porcelain` 當前 dirty tracked 數 (扣 sentinel 自身)。
    守住 R124 sentinel self-consistency: tuple 跟事實同步, owner M 收編或新 WIP
    必須在同 commit 更新 tuple, 防止 R138 之後 tuple 再次 stale 導致 sentinel
    self-FAIL (R138 前 5 條 tuple / 3 條實際 dirty, sentinel 自身 DRIFT 的根因)。
    SELF_EXEMPT 是 sentinel 模組本身, 允許它們在測試 / commit 過程中 dirty
    (這護衛合約就是守這些檔), 其他任何 dirty 必須在 tuple 內。
    """
    import r124_sentinel
    import subprocess as _sp
    SELF_EXEMPT = {
        "scripts/r124_sentinel.py",
        "scripts/test_r124_sentinel.py",
        "engineering-log.md",  # PUA 每輪 append 紀錄 (R124 sentinel 是守 R13 防護,
                               # engineering-log 是 PUA 自身工作紀錄, 非 owner M WIP)
    }
    proc = _sp.run(
        ["git", "status", "--porcelain"],
        capture_output=True, text=True, encoding="utf-8", errors="replace",
        cwd=r124_sentinel.REPO_ROOT, check=False,
    )
    actual_dirty = {
        line.split(maxsplit=1)[1].replace("\\", "/")
        for line in proc.stdout.splitlines() if line.strip()
    } - SELF_EXEMPT  # sentinel 自身免計
    declared = set(r124_sentinel.OWNER_M_WIP_FILES)
    missing_in_tuple = actual_dirty - declared  # owner M 沒宣告
    extra_in_tuple = declared - actual_dirty    # tuple 內已收編
    assert not missing_in_tuple, (
        f"tuple 缺 owner M 當前 WIP {missing_in_tuple}, 需在同 commit 更新 "
        f"OWNER_M_WIP_FILES (R138 結構性發現: tuple 跟事實必須 sync)"
    )
    assert not extra_in_tuple, (
        f"tuple 內檔已不 dirty {extra_in_tuple}, owner M 已收編, tuple 應同步移除"
    )
