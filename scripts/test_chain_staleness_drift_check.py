#!/usr/bin/env python3
r"""
K42 護衛鏈過期契約漂移偵測護衛 — 5 case pytest, 對齊 R172 量化真實值 baseline 守護

R194 落地。鏡像 R132 test_k0_drift_check.py + R193 test_k40_drift_check.py 模式
(M2 KPI 量測 closure 軸守護本體延伸, K0→K40→K42 維度接力)。1 個 Python pytest
模組, 走既無既有護衛維度 (Python script 不算 Rust 護衛, 不破 K42 chain 20
條飽和契約)。

5 case 守 5 個 K42 量化口徑 hidden gap:
  1. 持平 → exit 0 PASS (對齊 R172 baseline)
  2. 進步 (file_count / total_test_fn 增加, stale 仍 0) → exit 0 PASS
  3. 倒退 (file_count / total_test_fn 縮減 / stale 增加 / chain_count_min 倒退 /
     overall_pass True→False) → exit 1 FAIL
  4. 缺欄位 → exit 2 (解析失敗)
  5. JSON 損壞 → exit 2 (解析失敗)
"""
import json
import subprocess
import sys
from pathlib import Path

import pytest

SCRIPT = Path(__file__).resolve().parent / "chain_staleness_drift_check.py"
CHAIN_STALE_JSON = (
    Path(__file__).resolve().parent.parent / ".harness-chain-staleness.json"
)
PYTHON = sys.executable


# ---------- fixtures ----------

@pytest.fixture
def tmp_chain_json(tmp_path):
    """factory: 寫假 .harness-chain-staleness.json 進 tmp, 回傳 path"""
    def _setup(payload: dict | None = None,
               raw: str | None = None,
               missing: list[str] | None = None) -> Path:
        target = tmp_path / ".harness-chain-staleness.json"
        if raw is not None:
            target.write_text(raw, encoding="utf-8")
        else:
            data = {
                "file_count": 16,
                "total_test_fn": 471,
                "stale_count": 0,
                "chain_count_min": 20,
                "overall_pass": True,
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
    """跑 chain_staleness_drift_check.py 子進程, 回傳 CompletedProcess"""
    return subprocess.run(
        [PYTHON, str(SCRIPT), "--json", str(json_path), *args],
        capture_output=True, text=True, encoding="utf-8", errors="replace",
    )


# ---------- 5 case 護衛 ----------

def test_持平_對齊_R172_量化真實值_5_維度全_PASS():
    """chain_staleness.py 量化真實值全對齊 BASELINE → exit 0 PASS

    R194 baseline 對齊: file_count=16 + total_test_fn=471 + stale_count=0 +
    chain_count_min=20 + overall_pass=True 全守。
    """
    if not CHAIN_STALE_JSON.exists():
        pytest.skip(
            f".harness-chain-staleness.json 找不到: {CHAIN_STALE_JSON} "
            f"(先跑 chain_staleness.py)"
        )
    r = run_drift(CHAIN_STALE_JSON)
    assert r.returncode == 0, (
        f"預期 PASS, 實際 exit={r.returncode}\n{r.stdout}{r.stderr}"
    )
    assert "對齊 R172 baseline" in r.stdout
    assert "FAIL" not in r.stdout.split("=" * 72)[-1]  # 最後一段不該有 FAIL


def test_進步_護衛增加_stale_仍_0_也_PASS(tmp_chain_json):
    """K42 量化值變好 (file_count 16→18, total_test_fn 471→500) → exit 0 PASS

    進步是好事, 預設模式不 fail (對齊 k0_drift_check.py R132 預設行為)。
    stale_count 仍 0 守住健康底線, overall_pass=True 健康度不退。
    """
    p = tmp_chain_json(payload={
        "file_count": 18,        # 從 16 進步到 18
        "total_test_fn": 500,    # 從 471 進步到 500
        "stale_count": 0,        # 仍 0
        "chain_count_min": 20,   # 持平
        "overall_pass": True,
    })
    r = run_drift(p)
    assert r.returncode == 0, (
        f"預期 PASS (進步), 實際 exit={r.returncode}\n{r.stdout}{r.stderr}"
    )
    assert "進步" in r.stdout or "守住" in r.stdout


def test_倒退_護衛縮減_或_stale_增加_觸發_REGRESS(tmp_chain_json):
    """改 file_count 16→14 (護衛縮減) + stale 0→1 (新過期) → exit 1 FAIL

    M0 級 hidden gap 守護: 防有人改 chain_staleness.py 算法 (e.g. 漏算某個
    .rs 檔 / 改 STALE_DAYS 閾值 / 改 chain_count_min) 導致 K42 量化值倒退。
    """
    p = tmp_chain_json(payload={
        "file_count": 14,        # 從 16 倒退到 14
        "total_test_fn": 400,    # 從 471 倒退到 400
        "stale_count": 1,        # 從 0 倒退到 1 (新過期)
        "chain_count_min": 18,   # 從 20 倒退到 18 (chain 護衛掉)
        "overall_pass": False,   # 從 True 倒退到 False
    })
    r = run_drift(p)
    assert r.returncode == 1, (
        f"預期 FAIL, 實際 exit={r.returncode}\n{r.stdout}{r.stderr}"
    )
    assert "倒退" in r.stdout
    # 至少有 file_count / total_test_fn / stale_count / chain_count_min /
    # overall_pass 任一被指到
    keys = ("file_count", "total_test_fn", "stale_count", "chain_count_min", "overall_pass")
    assert any(k in r.stdout for k in keys), (
        f"預期 stdout 含 keys, 實際:\n{r.stdout}"
    )


def test_缺欄位_chain_count_min_缺失_回退碼_2(tmp_chain_json):
    """缺 chain_count_min 欄位 → exit 2 (KeyError 解析失敗)

    防有人改 .harness-chain-staleness.json schema 漏欄位而
    chain_staleness_drift_check.py 靜默放行。
    """
    p = tmp_chain_json(missing=["chain_count_min"])
    r = run_drift(p)
    assert r.returncode == 2, (
        f"預期 exit=2, 實際 exit={r.returncode}\n{r.stdout}{r.stderr}"
    )
    assert "解析失敗" in r.stderr or "缺欄位" in r.stderr or "找不到" in r.stderr


def test_JSON_損壞_回退碼_2(tmp_chain_json):
    """JSON 壞 (raw 不合法) → exit 2 (JSONDecodeError 解析失敗)

    防 .harness-chain-staleness.json 被破壞 (e.g. 寫入中斷 / 手編輯) 而
    漂移偵測靜默放行。
    """
    p = tmp_chain_json(raw='{"file_count": 16, "broken": tru')  # 截斷
    r = run_drift(p)
    assert r.returncode == 2, (
        f"預期 exit=2, 實際 exit={r.returncode}\n{r.stdout}{r.stderr}"
    )
    assert "解析失敗" in r.stderr or "Expecting" in r.stderr or "delimiter" in r.stderr
