#!/usr/bin/env python3
"""
K0 endpoint live 雙源量測 closure 守護 — 量化 /metrics 端點當下真實 emit 狀態

R197 落地 (M2 KPI 量測 closure 軸換 endpoint live 軸, 鏡像 R194 K42 過期契約
漂移偵測模式: 雙源比對守 hidden gap)。對齊 MISSION K0-A1 emit 維度但
不依賴 .harness-k0.json 新鮮度 — 純 endpoint live 當下量測, 即時反映
端點真實 emit 狀態 (不像 k0_measure.py 是 cumulative baseline 快照)。

換軸動機 (R197 prompt): 連 2 輪 (R195+R196) 都 M2 內部函式 hidden gap
closure 軸, 換 endpoint live 雙源 closure 軸 (sub-axis 換: 內部函式
hidden gap → endpoint live 雙源, mirror R194 K42 過期契約漂移模式)。
R124 sentinel 仍讀 .harness-k0.json, R197 不改 sentinel 本體 (R184
owner M 簽收 scope 守住), 純新增 K0 endpoint live 雙源守護本體。

為什麼需要 endpoint live 雙源 (k0_measure 不夠的原因):
  - k0_measure.py 是 cumulative baseline 快照: 跑一次寫 .harness-k0.json,
    之後 sentinel 讀的都是歷史值; 端點當下 DOWN / 新 provider 加入 / 端點
    重啟後丟失 emit, sentinel 都看不到 (要等下次有人跑 k0_measure 才更新)。
  - R197 K0 endpoint live: 純當下 curl /metrics, 不依賴 JSON 新鮮度,
    任何時候跑都反映端點真實狀態; 雙源 (live + JSON) 比對守「JSON 寫了
    但當下沒 emit」= stale JSON 隱藏 bug。

輸出:
  - 人類可讀表 (stdout): endpoint_alive / live_emit_count / json_emit_count / drift
  - machine-readable JSON (.harness-k0-live.json) — 給未來 sentinel 雙源接軌預留
  - 退出碼 0 (endpoint live 健康) / 1 (endpoint DOWN 或雙源漂移)

這是 R194 K42 過期契約漂移偵測模式的橫向 K0 維度 closure 補鏈路,
不破 K42 chain 20 條飽和契約 (Python 護衛不算 Rust 護衛, 走 R97 後
「1 輪 1 件」紀律, 不動既有護衛 chain 結構)。
"""
import json
import os
import re
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Dict, List, Optional, Set, Tuple

# 13 provider 真實清單 (對齊 hook_server.rs::KNOWN_PROVIDERS 4 本機 CLI + 9 OpenAB bot)
LOCAL_CLI = ["claude", "codex", "copilot", "gemini"]
OPENAB_BOT = ["cicx", "gitx", "giminix", "codex_bot", "openx",
              "irisx_bot", "grokx", "lpbot", "mimo"]
KNOWN_PROVIDERS = LOCAL_CLI + OPENAB_BOT  # 13 個, 對齊 hook_server.rs:323-340

# 配置 — 對齊 k0_measure.py METRICS_URL 既有 env var, 共用同一個 endpoint
LIVE_URL = os.environ.get("LOBSTERPULSE_METRICS_URL",
                          "http://127.0.0.1:19380/metrics")
JSON_PATH = Path(os.environ.get("K0_LIVE_JSON_PATH", ".harness-k0.json"))
OUT_PATH = Path(os.environ.get("K0_LIVE_OUT_PATH", ".harness-k0-live.json"))


# ---------- 端點 live 5 維度量測 ----------

def fetch_live_metrics(url: str = LIVE_URL, timeout: int = 2) -> Tuple[bool, str]:
    """抓 Prometheus /metrics 端點, 回 (endpoint_alive, metrics_text)。
    endpoint_alive = True if HTTP 200, False if URLError/OSError。
    """
    try:
        with urllib.request.urlopen(url, timeout=timeout) as r:
            return True, r.read().decode("utf-8", errors="replace")
    except (urllib.error.URLError, OSError):
        return False, ""


def parse_live_providers(metrics_text: str) -> Set[str]:
    """R197 解析 /metrics 端點 emit 過哪些 provider label。
    跟 k0_measure.parse_provider_emit 用同樣 pattern (re 對齊):
      `lobsterpulse_provider_*{provider="X"}`
    """
    if not metrics_text:
        return set()
    pattern = re.compile(
        r'lobsterpulse_provider_[a-zA-Z0-9_]+\{provider="([^"]+)"\}'
    )
    return {m.group(1) for m in pattern.finditer(metrics_text)}


def read_json_emit_providers(json_path: Path = JSON_PATH) -> Optional[Set[str]]:
    """讀 .harness-k0.json 取 k0a1_health_emit.providers 集合。
    JSON 不存在或欄位缺失 → 回 None (雙源比對時視為「無歷史 baseline」)。
    """
    if not json_path.exists():
        return None
    try:
        data = json.loads(json_path.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, OSError):
        return None
    k0a1 = data.get("k0a1_health_emit")
    if not isinstance(k0a1, dict):
        return None
    providers = k0a1.get("providers")
    if not isinstance(providers, list):
        return None
    return set(providers)


def measure_endpoint_live(url: str = LIVE_URL,
                          json_path: Path = JSON_PATH) -> Dict:
    """K0 endpoint live 5 維度量測 closure。

    回 dict:
      - endpoint_alive: bool (HTTP 200 = True)
      - live_emit_count: int (端點當下 emit 過的 KNOWN_PROVIDERS provider 數,
        過濾掉 __local__ 等非 13 provider label)
      - live_provider_labels: List[str] (端點 emit 過的所有 label 原始清單,
        含 __local__)
      - json_emit_count: Optional[int] (.harness-k0.json k0a1_health_emit.covered
        歷史 baseline, JSON 不存在 → None)
      - json_emit_providers: Optional[List[str]] (JSON 寫的 provider 清單)
      - drift: Dict (雙源比對漂移偵測):
        - live_new: JSON 沒寫但當下 emit (新增 provider 端點 emit, JSON 沒更新)
        - json_stale: JSON 寫了但當下沒 emit (JSON 寫死 baseline, 端點已停)
    """
    endpoint_alive, metrics_text = fetch_live_metrics(url)
    live_labels = parse_live_providers(metrics_text)
    # live_emit_count 對齊 K0-A1: 只算 KNOWN_PROVIDERS 中的 (13 provider scope)
    live_emit_count = sum(1 for p in KNOWN_PROVIDERS if p in live_labels)

    json_providers = read_json_emit_providers(json_path)
    if json_providers is None:
        json_emit_count: Optional[int] = None
        json_emit_providers: Optional[List[str]] = None
        drift: Dict[str, List[str]] = {"live_new": [], "json_stale": []}
    else:
        json_emit_count = len(json_providers)
        json_emit_providers = sorted(json_providers)
        # 雙源漂移: live - json = 端點新 emit 但 JSON 沒更新;
        #           json - live = JSON 寫死但端點已停 (stale JSON 隱藏 bug)
        drift = {
            "live_new": sorted(live_labels - json_providers),
            "json_stale": sorted(json_providers - live_labels),
        }

    return {
        "endpoint_alive": endpoint_alive,
        "live_emit_count": live_emit_count,
        "live_provider_labels": sorted(live_labels),
        "json_emit_count": json_emit_count,
        "json_emit_providers": json_emit_providers,
        "drift": drift,
    }


# ---------- 報表渲染 ----------

def render_live_report(result: Dict) -> str:
    """人類可讀 endpoint live 報表 (5 維度)。"""
    lines = [
        "=" * 60,
        f"LobsterPulse K0 endpoint live — {time.strftime('%Y-%m-%d %H:%M:%S')}",
        f"  live URL    : {LIVE_URL}",
        f"  json path   : {JSON_PATH} "
        f"({'OK' if result['json_emit_count'] is not None else 'MISSING'})",
        "=" * 60,
    ]
    # 維度 1+2: endpoint_alive + live_emit_count
    alive_str = "UP" if result["endpoint_alive"] else "DOWN"
    lines.append(
        f"endpoint_alive : {alive_str}"
    )
    lines.append(
        f"live_emit      : {result['live_emit_count']}/{len(KNOWN_PROVIDERS)} "
        f"(端點當下 emit 過的 KNOWN_PROVIDERS label 數)"
    )
    lines.append(
        f"live_labels    : {result['live_provider_labels']} "
        f"(原始清單, 含 __local__ 等非 13 provider)"
    )
    # 維度 3+4: json_emit_count + json_emit_providers
    if result["json_emit_count"] is None:
        lines.append("json_emit      : MISSING (.harness-k0.json 不存在, 雙源比對跳過)")
    else:
        lines.append(
            f"json_emit      : {result['json_emit_count']}/{len(KNOWN_PROVIDERS)} "
            f"(歷史 baseline)"
        )
        lines.append(
            f"json_providers : {result['json_emit_providers']}"
        )
    # 維度 5: 雙源漂移
    drift = result["drift"]
    has_drift = bool(drift["live_new"] or drift["json_stale"])
    lines.append("-" * 60)
    if result["json_emit_count"] is None:
        lines.append("drift          : N/A (no JSON baseline)")
    elif has_drift:
        lines.append(
            f"drift          : LIVE_JSON_MISMATCH "
            f"(live_new={drift['live_new']}, json_stale={drift['json_stale']})"
        )
    else:
        lines.append("drift          : OK (雙源一致)")
    lines.append("=" * 60)
    return "\n".join(lines)


# ---------- main ----------

def main() -> int:
    result = measure_endpoint_live()
    print(render_live_report(result))

    # 寫 machine-readable JSON 給未來 sentinel 雙源接軌預留
    report = {
        "timestamp": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "endpoint_url": LIVE_URL,
        "endpoint_alive": result["endpoint_alive"],
        "live_emit_count": result["live_emit_count"],
        "live_provider_labels": result["live_provider_labels"],
        "json_emit_count": result["json_emit_count"],
        "json_emit_providers": result["json_emit_providers"],
        "drift": result["drift"],
    }
    OUT_PATH.write_text(
        json.dumps(report, indent=2, ensure_ascii=False),
        encoding="utf-8",
    )
    print(f"\nJSON 寫入: {OUT_PATH}")

    # 退出碼: endpoint DOWN → 1; 雙源漂移 → 1; 都健康 → 0
    has_drift = bool(result["drift"]["live_new"] or result["drift"]["json_stale"])
    if not result["endpoint_alive"]:
        return 1
    if has_drift:
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
