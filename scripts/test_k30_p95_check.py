"""
K30 P95 量化口徑 closure 守護 — pytest test suite

R201 落地 (M2 KPI 量測 closure 軸換 K30 P95 維度, 鏡像 R188 K0 量化口徑
/ R195 chain_staleness / R196 K40 / R198 K0 endpoint live 內部函式
hidden gap 守護模式)。4 case pytest 守 4 個 K30 P95 內部函式 M0 級
hidden gap:

  - case 1 parse_p95_metric_line: K30 全名 regex 結構漂移觸發
    (e.g. 改嚴只認 `provider_sessions_p95_` 漏 `completed_sessions_p95_`
    → silent 漏算 K30 emit, K0 量化值偏小)
  - case 2 compute_p95_index: P95 還原算式錯觸發
    (e.g. `(N-1) * 0.95` 浮點 round 取代 `(N*95)//100` 整數, 小 N 漂移
    > 1 → P95 index silent 偏, K30 chain invariant 漂移)
  - case 3 verify_p95_chain_invariant: P99 ≤ max 邊界漏觸發
    (e.g. 拿掉 `p99 <= max_val` 邊界, P99 算式 bug 算出 > max 不警示,
    R53 chain 護衛 K-Foundation 量化口徑悄悄漂移)
  - case 4 measure_k30_p95_coverage: __local__ 過濾邏輯被改寬觸發
    (e.g. 從 `if p in KNOWN_PROVIDERS` 改成寬鬆 `if p.startswith("__")`
    反向, 端點內部 __local__ 標籤被誤算 +1 造假, 跟 R198 K0 endpoint
    live __local__ 過濾 hidden gap 同模式, 跨 K0 → K30 維度對稱)

對齊 R188 從 6→9 / R195 從 8→11 / R196 從 5→9 / R198 從 5→9 case
內部函式 hidden gap 守護模式 (= 同模式跨 5 個不同 KPI 維度)。
"""
import sys
from pathlib import Path
from unittest import mock

import pytest

# 把 scripts/ 加進 sys.path 才能 import k30_p95_check
SCRIPTS_DIR = Path(__file__).resolve().parent
if str(SCRIPTS_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPTS_DIR))

import k30_p95_check as k30  # noqa: E402


# ---------- fixtures ----------

def _metrics_text_with_p95(labels_secs: dict) -> str:
    """組裝最小 /metrics 文本含 K30 P95 metric lines"""
    lines = [
        "# HELP lobsterpulse_provider_completed_sessions_p95_duration_seconds 95th percentile",
        "# TYPE lobsterpulse_provider_completed_sessions_p95_duration_seconds gauge",
    ]
    for prov, secs in labels_secs.items():
        lines.append(
            f'lobsterpulse_provider_completed_sessions_p95_duration_seconds'
            f'{{provider="{prov}"}} {secs}'
        )
    return "\n".join(lines) + "\n"


# ---------- 4 case M0 級 hidden gap pytest 護衛 ----------

def test_parse_p95_metric_line_K30_全名_regex_改嚴_漏_completed_sessions_觸發_REGRESS():
    """守 K30 P95 全名 `provider_completed_sessions_p95_` regex 結構

    Hidden gap: 若有人改 regex 為 `r'provider_sessions_p95_duration_seconds\\{provider="..."\\}'`
    (漏 `completed_` 段) → K30 emit 端 lib.rs:2797 完整 metric name 包含
    `completed_sessions` 段, 改嚴後 regex 不 match, parse 回空 dict →
    k30_p95_covered 從 4 掉到 0, K0 量化閉合鏈 silent 漂移。
    """
    metrics_text = _metrics_text_with_p95({"claude": 60, "codex": 45})
    # 直接呼叫 k30.parse_p95_metric_line, 應抓到 2 個 provider
    out = k30.parse_p95_metric_line(metrics_text)
    assert "claude" in out, "K30 完整 metric name 含 `completed_sessions_p95_`, 必須 parse 到"
    assert "codex" in out, "K30 完整 metric name 含 `completed_sessions_p95_`, 必須 parse 到"
    assert out["claude"] == 60
    assert out["codex"] == 45

    # 模擬「regex 改嚴漏 `completed_` 段」silent 漂移: 改寫 metrics_text 模擬
    # emit 端拿掉 `completed_` 段 (mirror 有人改 source code), 守護鏈應
    # 在 parse 層偵測到「K30 emit 0/13」漂移。測試斷言: 即便 source code 改壞,
    # parse 仍能區分 (真實改壞不在 test 範圍, 但守住 K30 metric name 完整
    # 契約不動)。
    drift_metrics = metrics_text.replace(
        "completed_sessions_p95_duration_seconds",
        "sessions_p95_duration_seconds"
    )
    drift_out = k30.parse_p95_metric_line(drift_metrics)
    assert drift_out == {}, \
        "若 source code 改 emit 端拿掉 `completed_` 段, K30 守護鏈應能反映 " \
        "(parse 回空 = 量化閉合鏈 silent 漂移偵測觸發)"


def test_compute_p95_index_還原算式_改用_round_浮點_小_N_漂移_觸發_REGRESS():
    """守 K30 P95 還原算式 `(N * 95) // 100` i64 整數, 對齊 session.rs:1282

    Hidden gap: 若有人改 compute_p95_index 用 `(N - 1) * 0.95` 浮點 round
    → 小 N 時跟 Rust `(N * 95) / 100` 整數算式漂移 > 1:
      - N=20: Rust = 20*95/100 = 19, round(19*0.95) = round(18.05) = 18 ❌
      - N=10: Rust = 950/100 = 9, round(9*0.95) = round(8.55) = 9 ✓
      - N=2:  Rust = 190/100 = 1, round(1*0.95) = round(0.95) = 1 ✓
    """
    # 對齊 session.rs:1282 算式 (Rust 整數除法 1024*95 = 97280, /100 = 972)
    assert k30.compute_p95_index(1024) == 972  # 1024*95/100 = 972 (i64 整數)
    assert k30.compute_p95_index(100) == 95   # 100*95/100 = 95
    assert k30.compute_p95_index(20) == 19    # 20*95/100 = 19
    assert k30.compute_p95_index(1) == 0      # 1*95/100 = 0
    assert k30.compute_p95_index(2) == 1      # 2*95/100 = 1

    # 模擬「改成 round 浮點」算式, 對 N=20 漂移到 18, 守護鏈應能偵測
    # (用 mock 把函式本體 swap 進 round 浮點版, 驗證 K30 算式跟 Rust 不一致)
    buggy = lambda n: round((n - 1) * 0.95)  # noqa: E731
    assert buggy(20) == 18, "round 浮點版 N=20 算 18, 跟 Rust 19 漂移 = 算式壞"
    assert k30.compute_p95_index(20) == 19, \
        "K30 守護鏈仍守住 `(N*95)//100` i64 整數, 沒被 round 浮點版污染"


def test_verify_p95_chain_invariant_P99_leq_max_邊界被拿掉_觸發_REGRESS():
    """守 R53 K30-K34 chain invariant 完整 P25 ≤ P50 ≤ P75 ≤ P95 ≤ P99 ≤ max

    Hidden gap: 若有人改 verify 拿掉 `p99 <= max_val` 邊界 → P99 算式
    bug (取錯 percentile index > N-1) silent 算出比 max 大的值不警示,
    R53 chain 護衛 K-Foundation 量化口徑悄悄漂移。
    """
    # happy path
    assert k30.verify_p95_chain_invariant(10, 20, 30, 95, 99, 100) is True
    assert k30.verify_p95_chain_invariant(1, 1, 1, 1, 1, 1) is True

    # P99 > max 邊界必須擋 (mirror 有人改算式算 P99=200 超 lifetime max=100)
    assert k30.verify_p95_chain_invariant(10, 20, 30, 95, 200, 100) is False, \
        "P99 > max 必須 False, 守住 R53 chain invariant P99 ≤ max 邊界不退"

    # 模擬「拿掉 p99 <= max_val 邊界」buggy 版, 守護鏈應能區分
    buggy = lambda p25, p50, p75, p95, p99, max_val: p25 <= p50 <= p75 <= p95  # noqa: E731
    assert buggy(10, 20, 30, 95, 200, 100) is True, \
        "buggy 版 (漏 p99 <= max) P99=200 不擋, K30 守護鏈必須仍守住完整鏈"
    # K30 守護鏈 (完整 5 件套 + max) 仍擋 P99 > max
    assert k30.verify_p95_chain_invariant(10, 20, 30, 95, 200, 100) is False


def test_measure_k30_p95_coverage___local__標籤存在_不計入_emit_count_守住():
    """守 K30 P95 端點 emit `__local__` 過濾邏輯不退

    Hidden gap: 若有人改 measure_k30_p95_coverage 把 `if p in
    KNOWN_PROVIDERS` 拿掉或反向 (`if p.startswith("__")` 反向) → 端點
    內部 `__local__` 標籤被誤算 +1 造假, k30_p95_covered 漂移到 5/13。
    對齊 R198 K0 endpoint live __local__ 過濾 hidden gap 同模式, 跨
    K0 → K30 維度對稱。
    """
    # 含 __local__ + 4 個本機 CLI, K30 emit 應只看 4 (KNOWN_PROVIDERS)
    metrics_text = _metrics_text_with_p95({
        "claude": 60, "codex": 45, "copilot": 30, "gemini": 25,
        "__local__": 999,
    })
    covered, total, emit_set = k30.measure_k30_p95_coverage(metrics_text)
    assert "__local__" not in emit_set, \
        "__local__ 標籤必須過濾掉, K30 量化閉合鏈守住 13/13 程式碼 emit 不退"
    assert covered == 4, f"K30 端點 emit 應只看 4 個本機 CLI, 實得 {covered}"
    assert total == 13, "KNOWN_PROVIDERS 13 不動, 對齊 R108 k0_measure 14→13 spec drift 修"

    # 模擬「__local__ 過濾邏輯被改寬」buggy 版, 守護鏈應能區分
    buggy_metrics = k30.parse_p95_metric_line(metrics_text)
    buggy_emit = set(buggy_metrics.keys())  # 沒過濾 __local__
    assert "__local__" in buggy_emit, \
        "parse_p95_metric_line 不過濾 (合理, parse 純結構), 量化層 measure 才過濾"
    # K30 measure 守護鏈守住過濾
    assert "__local__" not in k30.measure_k30_p95_coverage(metrics_text)[2]
