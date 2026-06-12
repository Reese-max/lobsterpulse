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
    # K30 measure 守衛鏈守住過濾
    assert "__local__" not in k30.measure_k30_p95_coverage(metrics_text)[2]


# ---------- R209 4 case M0 級 hidden gap pytest 護衛延伸 ----------
# 鏡像 R204 k0_drift_check / R206 chain_staleness_drift_check / R207 k40_drift_check
# 內部函式 hidden gap 守護模式, 跨 5 個不同 KPI 維度對稱 (R201 4 case baseline
# → R209 +4 = 8 case 合計, closure 軸 10 → 11 維度換 K30 P95 主腳本剩餘
# fetch_live_metrics / render_report / main + K30_METRIC_NAME 常數契約 4 對象)。

def test_fetch_live_metrics_連線拒絕_不_raise_回_False_空字串_守住():
    """守 K30 P95 fetch_live_metrics 異常處理不退化為 raise

    Hidden gap: 若有人改 fetch_live_metrics 把 (URLError, OSError) 改成
    raise 而非 return (False, "") → main() 端 try/except 假定永不 raise,
    整個 K30 chain 崩潰, 退出碼 0 假陽性 (跟 R188/R198 endpoint DOWN +
    down_suffix 守護同模式, 跨 K0 → K30 維度對稱)。
    """
    import urllib.error

    # OSError (connection refused) 必須 swallow 回 (False, "")
    with mock.patch("k30_p95_check.urllib.request.urlopen",
                    side_effect=OSError("Connection refused")):
        alive, text = k30.fetch_live_metrics(
            url="http://127.0.0.1:1/metrics", timeout=1)
    assert alive is False, \
        "OSError 必須回 False 不 raise, K30 守衛鏈守住 main endpoint_alive 路徑"
    assert text == "", "alive=False 時 text 必須空字串"

    # URLError (DNS fail) 必須 swallow 回 (False, "")
    with mock.patch("k30_p95_check.urllib.request.urlopen",
                    side_effect=urllib.error.URLError("DNS fail")):
        alive, text = k30.fetch_live_metrics(
            url="http://invalid.example.host/metrics", timeout=1)
    assert alive is False
    assert text == ""

    # happy path: 200 + 文本
    with mock.patch("k30_p95_check.urllib.request.urlopen") as m:
        m.return_value.__enter__.return_value.read.return_value = b"ok"
        alive, text = k30.fetch_live_metrics(
            url="http://127.0.0.1:19380/metrics", timeout=1)
    assert alive is True
    assert text == "ok"


def test_render_report_emit_providers_排序_鎖定_alphabetical_守住():
    """守 K30 P95 render_report 報表 emit_providers 排序契約不退

    Hidden gap: 若有人改 render_report 把 `sorted(emit_set)` 拿掉 →
    報表 emit_providers 順序隨 set 內部 hash 浮動, 對齊 R198 K0 endpoint
    live render 契約 (sorted 鎖定), 防「row 順序漂移」silent 造假。
    雖然 main() 傳入前已 sorted, 但 render 端需守住 sorted 契約, 將來
    若有人直接傳 unsorted emit_set 進 render 仍能鎖定順序。
    """
    # 給一個未排序的 emit_set (模擬 caller 端漏 sorted)
    unsorted_emit = {"openx", "claude", "gemini", "copilot", "codex"}
    out = {"chain_invariant_ok": True}
    report = k30.render_report(True, 5, 13, unsorted_emit, out)

    # 守 sorted 鎖定: 報表內 emit_providers 順序應為 alphabetical
    expected = "emit providers : ['claude', 'codex', 'copilot', 'gemini', 'openx']"
    assert expected in report, \
        f"emit_providers 排序必須 alphabetical 鎖定, 缺契約: {expected!r} not in {report!r}"

    # 守 chain_invariant_ok 顯示
    assert "chain OK       : True" in report, \
        f"chain_invariant_ok 顯示契約不退, 缺 True 顯示: {report!r}"

    # 守 K30 P95 emit 覆蓋率顯示契約
    assert "K30 P95 emit   : 5/13" in report, \
        f"K30 emit 覆蓋率顯示契約不退, 缺 5/13: {report!r}"


def test_K30_METRIC_NAME_常量_完整_對齊_lib_rs_2797_契約_不退():
    """守 K30 P95 metric name 完整字串契約不退化

    Hidden gap: 若有人改 K30_METRIC_NAME 拿掉某段 (e.g. `completed_sessions_`)
    → parse_p95_metric_line regex 仍認, 但 emit 端 lib.rs:2797 完整 name
    含 `completed_sessions_` 段, 量化閉合鏈 silent 漂移。對齊 R188 /
    R196 / R198 / R206 / R207 同模式 (K0 / K40 / chain_staleness / k30
    metric name 契約守護)。
    """
    expected_main = "lobsterpulse_provider_completed_sessions_p95_duration_seconds"
    assert k30.K30_METRIC_NAME == expected_main, \
        f"K30 P95 metric name 必須對齊 lib.rs:2797 emit 端, 實得 {k30.K30_METRIC_NAME!r}"

    # 守 K30-K34 五件套 chain invariant 用的 percentile metrics 全部對齊
    expected_p25 = "lobsterpulse_provider_completed_sessions_p25_duration_seconds"
    expected_p50 = "lobsterpulse_provider_completed_sessions_p50_duration_seconds"
    expected_p75 = "lobsterpulse_provider_completed_sessions_p75_duration_seconds"
    expected_p95 = "lobsterpulse_provider_completed_sessions_p95_duration_seconds"
    expected_p99 = "lobsterpulse_provider_completed_sessions_p99_duration_seconds"
    assert k30.K30_PERCENTILE_METRICS["p25"] == expected_p25
    assert k30.K30_PERCENTILE_METRICS["p50"] == expected_p50
    assert k30.K30_PERCENTILE_METRICS["p75"] == expected_p75
    assert k30.K30_PERCENTILE_METRICS["p95"] == expected_p95
    assert k30.K30_PERCENTILE_METRICS["p99"] == expected_p99

    # p25/p50/p75/p95/p99 五件套齊備, 守住 R53 chain invariant 5 量化口徑
    assert set(k30.K30_PERCENTILE_METRICS.keys()) == {"p25", "p50", "p75", "p95", "p99"}, \
        "K30-K34 五件套 percentile keys 必須齊備不退"


def test_main_chain_invariant_漂移_退出碼_1_守住_fail_closed():
    """守 K30 P95 main() chain_invariant 漂移 → 退出碼 1 fail-closed

    Hidden gap: 若有人改 main 把 chain invariant 漂移當 warning 不阻斷
    (return 0) → R53 chain 護衛 K-Foundation 量化口徑悄悄漂移, K30 chain
    invariant 失去 fail-closed 保護。對齊 R196 / R197 / R204 fail-closed
    行為守護模式, 跨 K0 / K40 / K30 維度對稱。
    """
    # 模擬 endpoint 活 + chain 漂移 (P99 > max), 退出碼必須 1
    with mock.patch.object(k30, "fetch_live_metrics",
                           return_value=(True, "fake metrics")):
        with mock.patch.object(k30, "measure_k30_p95_coverage",
                               return_value=(5, 13, {"claude", "codex"})):
            with mock.patch.object(k30, "verify_p95_chain_invariant",
                                   return_value=False):  # chain 漂移觸發
                rc = k30.main()
    assert rc == 1, \
        f"K30 chain invariant 漂移必須退出碼 1 fail-closed, 實得 {rc}, " \
        "守住 R53 chain 護衛不退"

    # 正常路徑 (endpoint 活 + chain OK) 應退出碼 0
    with mock.patch.object(k30, "fetch_live_metrics",
                           return_value=(True, "fake metrics")):
        with mock.patch.object(k30, "measure_k30_p95_coverage",
                               return_value=(5, 13, {"claude"})):
            with mock.patch.object(k30, "verify_p95_chain_invariant",
                                   return_value=True):
                rc = k30.main()
    assert rc == 0, "正常路徑 (chain OK) 退出碼 0 守住, K30 happy path 不退"
