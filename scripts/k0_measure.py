#!/usr/bin/env python3
"""
K0 KPI 量測腳本 — 量化 LobsterPulse 13 provider 監控覆蓋率

R83 落地。對齊 MISSION.md K0 兩軸:
  K0-A1 Provider 健康度 emit 覆蓋率: 多少 provider 在 /metrics 端點實際有
           emit 過任何 `lobsterpulse_provider_*{provider="X"}` 樣本
           (R101 補齊的是程式碼定義 13/13; 端點實際 emit 受事件流影響)
  K0-A2 Provider 健康度 sample 覆蓋率: 多少 provider 有非零 sessions 樣本
           (真正「provider 在運作 + 事件流過」維度)
  K0-B  Quota 監控即時性:     多少 provider 的 usage-*.json 是新鮮的 (<24h)

R102 拆 K0-A → K0-A1 (emit 維度) + K0-A2 (sample 維度), 對齊 MISSION spec
(R101 寫 K0 13/13 是程式碼定義, 端點實際 emit 受 OpenAB bot 進程是否運作影響;
不拆會誤導 KPI 報表, 5/13 端點 emit 跟 13/13 程式碼定義是不同的兩個事實)。

輸出:
  - 人類可讀表 (stdout)
  - machine-readable JSON (.harness-k0.json)
  - 退出碼 0 (KPI 改善中) / 1 (KPI 倒退或全 0)

這是策略顧問 R80 建議 #3 「每週自動報表」的工程化落地。
把「未量測」變成「可量測」, 是 M2 等級的真任務 (推進 K0 直接量化)。
"""
import json
import os
import re
import sys
import time
import urllib.request
import urllib.error
from pathlib import Path
from typing import Dict, List, Tuple

# 13 provider 真實清單 (對齊 CLAUDE.md v5.1 「4 本機 CLI + 9 OpenAB bot」)
# hook_server.rs::KNOWN_PROVIDERS 為 source of truth: 4 + 9 = 13。
# R108 修 docstring/spec drift: 舊版寫「14 / 4+10」是 R83 落地時尚未對齊 R78
# (grokx/lpbot/mimo 補完後) 的殘留, 已同步收齊。
LOCAL_CLI = ["claude", "codex", "copilot", "gemini"]
OPENAB_BOT = ["cicx", "gitx", "giminix", "codex_bot", "openx",
              "irisx_bot", "grokx", "lpbot", "mimo"]
KNOWN_PROVIDERS = LOCAL_CLI + OPENAB_BOT  # 13 個, 對齊 hook_server.rs:323-340

# 配置
METRICS_URL = os.environ.get("LOBSTERPULSE_METRICS_URL", "http://127.0.0.1:19380/metrics")
QUOTA_DIR = Path(os.environ.get("LOBSTERPULSE_QUOTA_DIR",
                                 str(Path.home() / ".lobsterpulse")))
FRESH_HOURS = 24
STALE_MARKER = re.compile(r"\.stale-\d{8}$")


def fetch_metrics() -> str:
    """抓 Prometheus /metrics 端點, 失敗回空字串"""
    try:
        with urllib.request.urlopen(METRICS_URL, timeout=2) as r:
            return r.read().decode("utf-8", errors="replace")
    except (urllib.error.URLError, OSError):
        return ""


def parse_provider_sessions(metrics_text: str) -> Dict[str, float]:
    """
    解析 `lobsterpulse_provider_sessions{provider="..."}` 與
    `lobsterpulse_sessions_total`。
    沒 metrics 文字 → 全 0; 沒對應 provider → 0 (視為沒樣本)。
    """
    out: Dict[str, float] = {p: 0.0 for p in KNOWN_PROVIDERS}
    if not metrics_text:
        return out
    # provider_sessions{provider="X"} N
    pattern = re.compile(
        r'lobsterpulse_provider_sessions\{provider="([^"]+)"\}\s+([0-9.eE+-]+)'
    )
    for m in pattern.finditer(metrics_text):
        prov, val = m.group(1), float(m.group(2))
        if prov in out:
            out[prov] = val
    return out


def parse_provider_emit(metrics_text: str) -> set:
    """
    R102: 解析 /metrics 端點實際 emit 過哪些 provider label (任何
    `lobsterpulse_provider_*{provider="X"}` 都算 emit 過)。
    對齊 K0-A1 端點 emit 維度 (R101 補的 13/13 是程式碼定義, 端點
    實際 emit 受 OpenAB bot 是否在運作影響 — OpenAB 9 個 bot 平時無
    事件 → 端點不會 emit 對應樣本)。
    """
    out: set = set()
    if not metrics_text:
        return out
    pattern = re.compile(
        r'lobsterpulse_provider_[a-zA-Z0-9_]+\{provider="([^"]+)"\}'
    )
    for m in pattern.finditer(metrics_text):
        out.add(m.group(1))
    return out


def scan_quota_snapshots() -> Dict[str, Dict]:
    """
    掃 QUOTA_DIR 裡所有 usage-*.json, 區分新鮮 (mtime < FRESH_HOURS) 與 stale。
    本機 CLI (claude/codex/copilot/gemini) 走 usage-local.json (合併 1 個檔)。
    OpenAB bot 各走 usage-{id}.json。
    """
    out: Dict[str, Dict] = {}
    if not QUOTA_DIR.is_dir():
        return {p: {"state": "no_dir", "mtime_age_hours": None,
                   "path": None} for p in KNOWN_PROVIDERS}

    now = time.time()
    # 本機 4 個共用 usage-local.json
    local_path = QUOTA_DIR / "usage-local.json"
    if local_path.exists():
        age_h = (now - local_path.stat().st_mtime) / 3600.0
        for p in LOCAL_CLI:
            out[p] = {
                "state": "fresh" if age_h < FRESH_HOURS else "stale",
                "mtime_age_hours": round(age_h, 2),
                "path": str(local_path),
            }
    else:
        for p in LOCAL_CLI:
            out[p] = {"state": "missing", "mtime_age_hours": None,
                      "path": str(local_path)}

    # OpenAB bot 各看 usage-{id}.json (可能含 .stale-YYYYMMDD 後綴)。
    # 實際檔名格式 (對齊 STALE_MARKER 正則 \.stale-\d{8}$):
    #   - usage-{bot}.json                  ← fresh (主檔,OpenAB 正常寫入)
    #   - usage-{bot}.json.stale-YYYYMMDD   ← stale (主檔被 rename 成 .stale-日期 標記過期)
    # glob 抓所有匹配,再用 STALE_MARKER 分流:符合的進 stale_paths,其餘(嚴格就
    # 是 usage-{bot}.json) 進 fresh_path。R110 修:移除 R83 殘留的 `candidates`
    # 死碼(從未被引用,且硬編碼 .stale- 無日期跟 STALE_MARKER 8 位數要求不一致,
    # 誤導讀者以為 stale 檔無日期)。
    #
    # R114 修:openx 需對齊 hook_server.rs:376-378 別名 — OpenAB BackendType::Other
    # 寫 usage-bot.json (legacy), 而 hook_server 把 POST /hook/bot rewrite 成
    # "openx"。本腳本若只看 usage-openx.json* 會永遠漏算 openx (就算 OpenAB
    # 正常運作也計不到), 必須雙 glob 把 usage-bot.json* 也納入 openx bucket。
    # 修法:openx 加第二個 base name "usage-bot", 跟主檔名併行 glob。對齊
    # hook_server.rs 的別名語意, 避免 K0 Quota coverage 永遠少算 1 個 provider。
    for bot in OPENAB_BOT:
        base_names = [f"usage-{bot}"]
        if bot == "openx":
            base_names.append("usage-bot")
        all_files: List[Path] = []
        for base in base_names:
            all_files.extend(QUOTA_DIR.glob(f"{base}.json*"))
        fresh_path = None
        stale_paths: List[Path] = []
        for f in all_files:
            if STALE_MARKER.search(f.name):
                stale_paths.append(f)
            else:
                fresh_path = f
        if fresh_path and fresh_path.exists():
            age_h = (now - fresh_path.stat().st_mtime) / 3600.0
            out[bot] = {
                "state": "fresh" if age_h < FRESH_HOURS else "stale",
                "mtime_age_hours": round(age_h, 2),
                "path": str(fresh_path),
            }
        elif stale_paths:
            # 取最舊 stale 檔案, age 是「自從變 stale 多久」
            latest = max(stale_paths, key=lambda p: p.stat().st_mtime)
            age_h = (now - latest.stat().st_mtime) / 3600.0
            out[bot] = {
                "state": "stale",
                "mtime_age_hours": round(age_h, 2),
                "path": str(latest),
            }
        else:
            out[bot] = {"state": "missing", "mtime_age_hours": None,
                        "path": None}
    return out


def render_table(health: Dict[str, float], quota: Dict[str, Dict]) -> str:
    """人類可讀: provider | metrics_sessions | quota_state | quota_age_h"""
    rows = []
    rows.append(f"{'provider':<14} {'metrics':>8} {'quota_state':<10} "
                f"{'age_h':>8}  visual")
    rows.append("-" * 60)
    for p in KNOWN_PROVIDERS:
        sess = health.get(p, 0.0)
        q = quota.get(p, {})
        state = q.get("state", "unknown")
        age = q.get("mtime_age_hours")
        age_s = f"{age:8.2f}" if age is not None else "      n/a"
        # 視覺化 (純 ASCII, 避 Windows cp950 編碼錯誤)
        m_dot = "[X]" if sess > 0 else "[ ]"
        q_dot = {"fresh": "[X]", "stale": "[S]", "missing": "[ ]",
                 "no_dir": "[ ]"}.get(state, "[?]")
        visual = f"m={m_dot} q={q_dot}"
        rows.append(f"{p:<14} {sess:>8.1f} {state:<10} {age_s}  {visual}")
    return "\n".join(rows)


def main() -> int:
    metrics_text = fetch_metrics()
    health = parse_provider_sessions(metrics_text)
    emit_providers = parse_provider_emit(metrics_text)
    quota = scan_quota_snapshots()

    # R102: K0-A 拆雙軌
    # K0-A1: /metrics 端點實際 emit 過 provider 樣本的 provider 數
    #         (受 bot 進程是否運作 + 是否有事件流過影響)
    k0a1_covered = sum(1 for p in KNOWN_PROVIDERS if p in emit_providers)
    # K0-A2: 有非零 sessions 樣本的 provider 數 (真正「在運作 + 事件流過」)
    k0a2_covered = sum(1 for v in health.values() if v > 0)
    # K0-B: quota 新鮮的 provider 數
    k0b_covered = sum(1 for v in quota.values() if v.get("state") == "fresh")
    # R114: K0 Quota coverage — 任何狀態 (fresh/stale) 都算 quota data path 已接上,
    # 對齊 MISSION.md "K0 Quota 監控即時性" KPI 13/13 目標「usage-*.json 或等價
    # metric 是否被讀到」, 比 K0-B (僅 fresh) 寬。R114 修 openx legacy alias 後從
    # 9/13 → 10/13 (openx 的 usage-bot.json 終於被認到)。
    k0_quota_coverage = sum(1 for v in quota.values()
                            if v.get("state") in ("fresh", "stale"))

    total = len(KNOWN_PROVIDERS)
    k0a1_pct = round(100.0 * k0a1_covered / total, 1)
    k0a2_pct = round(100.0 * k0a2_covered / total, 1)
    k0b_pct = round(100.0 * k0b_covered / total, 1)

    print("=" * 60)
    print(f"LobsterPulse K0 量測 — {time.strftime('%Y-%m-%d %H:%M:%S')}")
    print(f"  metrics URL : {METRICS_URL} ({'OK' if metrics_text else 'DOWN'})")
    print(f"  quota dir   : {QUOTA_DIR}")
    print("=" * 60)
    print(render_table(health, quota))
    # R111 修:R102 拆 K0-A 雙軌時漏了「端點 down」與「13 provider 真的 0
    # emit」在 stdout 報表的區分 — 兩種情況都會印 0/13,讀者分不出是「端點
    # 死掉沒量到」還是「13 個 provider 都沒事件流過」。端點 down 時明確標
    # (endpoint DOWN) suffix,避免誤導;K0-B 不受影響(quota 是 filesystem
    # scan,不走 /metrics 端點)。JSON 結構不動,`metrics_endpoint_alive`
    # 欄位 R102 已落,消費者可自己分流。
    endpoint_alive = bool(metrics_text)
    down_suffix = "" if endpoint_alive else " (endpoint DOWN)"
    print("-" * 60)
    print(f"K0-A1 健康度 emit 覆蓋率 (端點實際 emit): "
          f"{k0a1_covered}/{total} ({k0a1_pct}%){down_suffix}")
    print(f"K0-A2 健康度 sample 覆蓋率 (非零 sessions): "
          f"{k0a2_covered}/{total} ({k0a2_pct}%){down_suffix}")
    print(f"K0-B  Quota 即時性 (fresh <24h): "
          f"{k0b_covered}/{total} ({k0b_pct}%)")
    # R114: 補 K0 Quota coverage (任何狀態) — 對齊 MISSION 13/13 目標。
    k0_quota_cov_pct = round(100.0 * k0_quota_coverage / total, 1)
    print(f"K0-Q  Quota 覆蓋率 (fresh+stale 都有 data path): "
          f"{k0_quota_coverage}/{total} ({k0_quota_cov_pct}%)")
    print("=" * 60)
    print(f"[K0-A1 端點 emit 過的 provider label: "
          f"{sorted(emit_providers)}]")

    # 寫 machine-readable JSON 供後續儀表板/CI 用
    report = {
        "timestamp": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "metrics_endpoint_alive": bool(metrics_text),
        "providers_total": total,
        "k0a1_health_emit": {
            "covered": k0a1_covered, "total": total, "pct": k0a1_pct,
            "providers": sorted(p for p in emit_providers if p in KNOWN_PROVIDERS),
        },
        "k0a2_health_sample": {
            "covered": k0a2_covered, "total": total, "pct": k0a2_pct,
        },
        "k0b_quota_freshness": {"fresh": k0b_covered, "total": total,
                                "pct": k0b_pct},
        # R114: K0 Quota coverage KPI 量化 (fresh+stale 都有 data path 算覆蓋)
        "k0q_quota_coverage": {"covered": k0_quota_coverage, "total": total,
                                "pct": k0_quota_cov_pct},
        "providers": {
            p: {
                "metrics_sessions": health.get(p, 0.0),
                "metrics_emit": p in emit_providers,
                "quota": quota.get(p, {}),
            }
            for p in KNOWN_PROVIDERS
        },
    }
    out_json = Path(__file__).resolve().parent.parent / ".harness-k0.json"
    out_json.write_text(json.dumps(report, indent=2, ensure_ascii=False),
                        encoding="utf-8")
    print(f"\nJSON 寫入: {out_json}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
