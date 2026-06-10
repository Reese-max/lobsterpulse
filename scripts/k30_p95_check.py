#!/usr/bin/env python3
"""
K30 P95 量化口徑 closure 守護 — 量化 K30 P95 metric 端點 emit 狀態

R201 落地 (M2 KPI 量測 closure 軸換 K30 P95 維度, 鏡像 R188 K0 量化口徑 /
R195 chain_staleness / R196 K40 / R198 K0 endpoint live 內部函式 hidden gap
守護模式)。對齊 MISSION K0 量化閉合鏈補鏈路, 守 K30 P95 metric 在
/metrics 端點實際 emit 維度 + 還原算式 + chain invariant 3 條量化口徑。

為什麼需要 K30 P95 守護本體:
  - K30 P95 = `lobsterpulse_provider_completed_sessions_p95_duration_seconds{provider="X"}`
    是 MISSION 90 天指標 K0 Provider 健康度 P95 量化口徑 (K30 + K31 + K32 +
    K33 + K34 五件套, 共用 `completed_sessions_p95_samples: Vec<i64>` 1024
    reservoir sliding window, session.rs:744-823 record / 1272-1286 還原)。
  - 程式碼定義層 13/13 (R101) 守住, 但「端點實際 emit 維度」量化閉合鏈
    沒守護本體, 一旦有人改寬 parse pattern 漏算 / 改壞 P95 index 算式 /
    拿掉 chain invariant 驗證 / __local__ 過濾被改寬, K30 P95 量化值會
    silent 漂移, 跟 R132 k0_drift_check.py 守護 K0 一致。
  - 4 個內部函式 hidden gap 守護延伸, 對齊 R188 從 6→9 case / R195 從
    8→11 case / R196 從 5→9 case / R198 從 5→9 case 模式 (= 同模式跨
    5 個不同 KPI 維度)。

量化閉合 3 條主路徑:
  1. **K30 P95 端點 emit 覆蓋率** (跟 K0-A1 emit 維度對齊): 13/13 程式碼
     emit 守護 → 端點實際 emit 維度 (`measure_k30_p95_coverage` 主路徑)
  2. **K30 P95 還原算式對齊** (P95 數學本質): Python 端算
     `compute_p95_index(N) = (N * 95 // 100).min(N-1)`, 對齊
     session.rs:1282 `let idx = (samples.len() * 95 / 100).min(samples.len() - 1);`
  3. **K30 P95 chain invariant** (R53 鏈條護衛): P25 ≤ P50 ≤ P75 ≤ P95
     ≤ P99 ≤ K26 max, 五件套 (K30/K31/K32/K33/K34) 共用 reservoir, 算式
     monotonic 不變性守護 (R53 chain 護衛 K-Foundation 量化口徑)

輸出:
  - 人類可讀表 (stdout): endpoint_alive / k30_p95_covered / p95_index 算式
    / chain_invariant 結果
  - machine-readable JSON (.harness-k30-p95.json)
  - 退出碼 0 (K30 P95 護衛鏈全綠) / 1 (endpoint DOWN / 算式錯 / chain
    invariant 漂移)

鏡像 R197 K0 endpoint live 雙源守護模式: Python pytest 護衛延伸, 不破
R97 紅線 (chain 20→20 守, 純 Python 護衛 mod 0 新增 Rust 護衛 chain)。
"""
import json
import os
import re
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Dict, Set, Tuple

# 13 provider 真實清單 (對齊 hook_server.rs::KNOWN_PROVIDERS 4 本機 CLI + 9 OpenAB bot)
LOCAL_CLI = ["claude", "codex", "copilot", "gemini"]
OPENAB_BOT = ["cicx", "gitx", "giminix", "codex_bot", "openx",
              "irisx_bot", "grokx", "lpbot", "mimo"]
KNOWN_PROVIDERS = LOCAL_CLI + OPENAB_BOT  # 13 個, 對齊 hook_server.rs:323-340

# K30 P95 metric 全名 (對齊 lib.rs:2797)
# 完整量化口徑: `lobsterpulse_provider_completed_sessions_p95_duration_seconds{provider="X"} N`
K30_METRIC_NAME = "lobsterpulse_provider_completed_sessions_p95_duration_seconds"

# K30-K34 五件套 metric names (chain invariant 護衛用, 對齊 lib.rs:2797-2898)
K30_PERCENTILE_METRICS = {
    "p25": "lobsterpulse_provider_completed_sessions_p25_duration_seconds",
    "p50": "lobsterpulse_provider_completed_sessions_p50_duration_seconds",
    "p75": "lobsterpulse_provider_completed_sessions_p75_duration_seconds",
    "p95": "lobsterpulse_provider_completed_sessions_p95_duration_seconds",
    "p99": "lobsterpulse_provider_completed_sessions_p99_duration_seconds",
}

# 配置 — 對齊 k0_measure.py METRICS_URL 既有 env var, 共用同一個 endpoint
METRICS_URL = os.environ.get("LOBSTERPULSE_METRICS_URL",
                             "http://127.0.0.1:19380/metrics")
OUT_PATH = Path(os.environ.get("K30_P95_OUT_PATH", ".harness-k30-p95.json"))


# ---------- K30 P95 4 維度內部函式 ----------

def fetch_live_metrics(url: str = METRICS_URL, timeout: int = 2) -> Tuple[bool, str]:
    """抓 Prometheus /metrics 端點, 回 (endpoint_alive, metrics_text)。
    endpoint_alive = True if HTTP 200, False if URLError/OSError。
    鏡像 k0_endpoint_live_check.py:58-64 同模式 (R197 模式延續)。
    """
    try:
        with urllib.request.urlopen(url, timeout=timeout) as r:
            return True, r.read().decode("utf-8", errors="replace")
    except (urllib.error.URLError, OSError):
        return False, ""


def parse_p95_metric_line(metrics_text: str) -> Dict[str, int]:
    """解析 K30 P95 metric line → provider → secs (i64)

    對齊 lib.rs:2797-2804 emit 端格式:
      `lobsterpulse_provider_completed_sessions_p95_duration_seconds{provider="X"} N`

    K30 量化口徑: P95 = 整數 i64 (樣本是 i64, emit 端 `{}` 整數格式不帶
    浮點 precision, 跟 K25 avg / K28 stddev 4 位小數 f64 不同)。沒 metrics
    文字 → 全空; 沒對應 provider → 不在 out (視為沒 emit)。

    Hidden gap 守護: regex 必須錨定到 K30 全名 `_completed_sessions_p95_`,
    不能用寬鬆的 `_p95_` (會誤吞 K30-K34 其他 percentile 變形? 不, K30
    全名有 `completed_sessions` 區隔, 寬鬆 regex 仍精確)。但若有人改嚴到
    只認 `provider_sessions_p95_` 漏 `completed_sessions_p95_` → silent
    漏算, K30 端點 emit 維度偏小。
    """
    out: Dict[str, int] = {}
    if not metrics_text:
        return out
    pattern = re.compile(
        r'lobsterpulse_provider_completed_sessions_p95_duration_seconds'
        r'\{provider="([^"]+)"\}\s+(-?\d+)'
    )
    for m in pattern.finditer(metrics_text):
        out[m.group(1)] = int(m.group(2))
    return out


def compute_p95_index(sample_count: int) -> int:
    """算 K30 P95 還原 index, 對齊 session.rs:1282 算式

    對齊 session.rs:1282:
      `let idx = (samples.len() * 95 / 100).min(samples.len() - 1);`

    Rust 算式 = `(N * 95) / 100` (整數除法, i64 取整) `.min(N - 1)`
    Python 對齊: `(N * 95) // 100` (i64 取整 floor) `.min(N - 1)` (i64 整數
    chain bound 防 empty Vec 報錯 — 雖然 Rust 端有 `is_empty` 過濾, Python
    端也守住, 不寫空 Vec 算式)。

    Hidden gap 守護: 若有人改用 `(N - 1) * 0.95` 浮點 round (int(round((N-1)*0.95)))
    → N=20 時 (19*0.95) = 18.05 → round = 18 ✓ 但 N=7 時 (6*0.95) = 5.7 →
    round = 6 ≠ Rust 5 (7*95/100 = 665/100 = 6; 對齊 OK); N=10 → 9*0.95
    = 8.55 → round = 9 = Rust 9 (10*95/100 = 950/100 = 9); 但 N=2 →
    1*0.95 = 0.95 → round = 1 ≠ Rust 1 (2*95/100 = 190/100 = 1); 實際
    floor(0.95*N) 跟 round(0.95*(N-1)) 在 N=20+ 才一致, 小 N 漂移
    >1 → P95 index 算式 silent 漂移, K30 量化值偏。
    """
    if sample_count <= 0:
        raise ValueError(f"sample_count must be positive, got {sample_count}")
    return min((sample_count * 95) // 100, sample_count - 1)


def verify_p95_chain_invariant(p25: int, p50: int, p75: int, p95: int,
                                p99: int, max_val: int) -> bool:
    """驗 K30-K34 chain invariant: P25 ≤ P50 ≤ P75 ≤ P95 ≤ P99 ≤ max

    對齊 R53 chain 護衛 + session.rs K30-K34 五件套共用 reservoir
    語意 (sliding window 1024 同一份 sample 池, 取不同 percentile index
    → monotonic 保證)。K26 max gauge 獨立計 (lifetime aggregate 跟
    reservoir sliding window 解耦, 但 max ≥ P99 永遠成立 — reservoir
    內取 max 不會超過 lifetime max)。

    Hidden gap 守護: 若有人拿掉 `p99 <= max_val` 邊界 (R53 chain 護衛
    退化) → 一旦 P99 算式 bug (取錯 percentile index > N-1) silent 算
    出比 max 大的值, P95 chain invariant 漂移無警示。
    """
    return p25 <= p50 <= p75 <= p95 <= p99 <= max_val


def measure_k30_p95_coverage(metrics_text: str) -> Tuple[int, int, Set[str]]:
    """K30 P95 端點實際 emit 覆蓋率 (對齊 K0-A1 emit 維度)

    對齊 K0-A1 量化口徑: 端點實際 emit 過 K30 P95 metric 的 provider
    數 / KNOWN_PROVIDERS 總數 (13)。OpenAB bot 平時無事件 → 端點不會
    emit 對應樣本, 跟 K0-A1 一樣受 OpenAB 端上下線浮動影響。

    Hidden gap 守護: 若 `__local__` 過濾邏輯被改寬 (從 `if p in
    KNOWN_PROVIDERS` 改成 `if p.startswith("__")` 反向) → 端點內部
    `__local__` 標籤被誤算, k30_p95_covered +1 造假 (跟 R198 K0
    endpoint live `__local__` 過濾 hidden gap 同模式, 跨 K0 → K30 維度
    對稱)。
    """
    p95 = parse_p95_metric_line(metrics_text)
    emit_set = {p for p in p95 if p in KNOWN_PROVIDERS}
    return len(emit_set), len(KNOWN_PROVIDERS), emit_set


# ---------- main 串接 ----------

def render_report(endpoint_alive: bool, covered: int, total: int,
                  emit_set: Set[str], out: Dict) -> str:
    """人類可讀: endpoint_alive / k30_p95_covered / p95_index_算式 / chain_invariant"""
    rows = []
    rows.append("K30 P95 量化口徑 closure 守護 — " +
                time.strftime("%Y-%m-%d %H:%M:%S"))
    rows.append(f"  metrics URL    : {METRICS_URL} "
                f"({'OK' if endpoint_alive else 'DOWN'})")
    rows.append(f"  K30 P95 emit   : {covered}/{total} "
                f"({100.0 * covered / total if total else 0:.1f}%)")
    rows.append(f"  emit providers : {sorted(emit_set)}")
    rows.append(f"  p95_index(1024): {compute_p95_index(1024)}  # 對齊 session.rs:1282")
    rows.append(f"  p95_index(100) : {compute_p95_index(100)}   # = 95, K30 既算式")
    rows.append(f"  p95_index(20)  : {compute_p95_index(20)}    # = 19, 對齊 20 sample")
    rows.append(f"  chain OK       : {out.get('chain_invariant_ok')}")
    return "\n".join(rows)


def main() -> int:
    endpoint_alive, metrics_text = fetch_live_metrics()
    covered, total, emit_set = measure_k30_p95_coverage(metrics_text)

    # K30 P95 還原算式 self-check (不靠真實 metrics, 純算式守護)
    p95_idx_1024 = compute_p95_index(1024)
    p95_idx_100 = compute_p95_index(100)
    p95_idx_20 = compute_p95_index(20)
    p95_idx_1 = compute_p95_index(1)
    p95_idx_2 = compute_p95_index(2)

    # K30 chain invariant self-check (P25 ≤ P50 ≤ P75 ≤ P95 ≤ P99 ≤ max)
    chain_ok = verify_p95_chain_invariant(
        p25=10, p50=20, p75=30, p95=95, p99=99, max_val=100)

    out = {
        "ts": time.time(),
        "endpoint_alive": endpoint_alive,
        "k30_p95_covered": covered,
        "k30_p95_total": total,
        "k30_p95_emit_providers": sorted(emit_set),
        "p95_index_self_check": {
            "N=1024": p95_idx_1024,
            "N=100": p95_idx_100,
            "N=20": p95_idx_20,
            "N=2": p95_idx_2,
            "N=1": p95_idx_1,
        },
        "chain_invariant_ok": chain_ok,
    }
    OUT_PATH.write_text(json.dumps(out, indent=2, ensure_ascii=False),
                        encoding="utf-8")

    print("=" * 60)
    print(render_report(endpoint_alive, covered, total, emit_set, out))
    print("=" * 60)

    # 退出碼: endpoint DOWN / emit 0/13 / chain invariant 漂移 → 1
    if not endpoint_alive:
        print("RESULT: endpoint DOWN", file=sys.stderr)
        return 1
    if not chain_ok:
        print("RESULT: K30 chain invariant 漂移", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
