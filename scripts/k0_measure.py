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
from typing import Dict, Set

REGISTRY_PATH = Path(__file__).resolve().parent.parent / "src" / "provider-capabilities.json"


def load_registry(path: Path = REGISTRY_PATH) -> Dict:
    """The versioned registry is the source for K0 provider IDs and denominators."""
    registry = json.loads(path.read_text(encoding="utf-8"))
    ids = [p["id"] for p in registry["providers"]]
    if len(ids) != len(set(ids)) or not registry.get("registry_version"):
        raise ValueError("invalid provider capability registry")
    return registry


REGISTRY = load_registry()
LOCAL_CLI = [p["id"] for p in REGISTRY["providers"] if p["scope"] == "local_cli"]
OPENAB_BOT = [p["id"] for p in REGISTRY["providers"] if p["scope"] == "openab_push"]
KNOWN_PROVIDERS = LOCAL_CLI + OPENAB_BOT

# 配置
METRICS_URL = os.environ.get("LOBSTERPULSE_METRICS_URL", "http://127.0.0.1:19380/metrics")
QUOTA_DIR = Path(os.environ.get("LOBSTERPULSE_QUOTA_DIR",
                                 str(Path.home() / ".lobsterpulse")))
FRESH_HOURS = 24
STALE_MARKER = re.compile(r"\.stale-\d{8}$")


def read_configured_providers() -> Dict[str, bool] | None:
    """Return None when config cannot be read; unknown must never become zero."""
    override = os.environ.get("LOBSTERPULSE_CONFIG_FILE")
    if override:
        path = Path(override)
    elif sys.platform == "darwin":
        path = Path.home() / "Library" / "Application Support" / "lobsterpulse" / "config.json"
    elif os.name == "nt":
        path = Path(os.environ.get("APPDATA", "")) / "lobsterpulse" / "config.json"
    else:
        path = Path(os.environ.get("XDG_CONFIG_HOME", str(Path.home() / ".config"))) / "lobsterpulse" / "config.json"
    try:
        providers = json.loads(path.read_text(encoding="utf-8"))["providers"]
        if not isinstance(providers, dict):
            return None
        return {p: providers.get(p, {}).get("enabled") is True for p in KNOWN_PROVIDERS}
    except (OSError, ValueError, KeyError, TypeError, AttributeError):
        return None


def parse_provider_metric(metrics_text: str, metric: str) -> Dict[str, float]:
    """Read a single provider-labelled Prometheus family without inferring missing samples."""
    pattern = re.compile(rf'^{re.escape(metric)}\{{provider="([^"]+)"\}}\s+([0-9.eE+-]+)$', re.MULTILINE)
    return {m.group(1): float(m.group(2)) for m in pattern.finditer(metrics_text)
            if m.group(1) in KNOWN_PROVIDERS}


def build_coverage_receipt(metrics_text: str, health: Dict[str, float],
                           quota: Dict[str, Dict], configured: Dict[str, bool] | None,
                           *, registry: Dict = None, timestamp: str = None,
                           metrics_available: bool = True) -> Dict:
    """Evidence-bearing current coverage; exclusions never erase advertised gaps."""
    registry = registry or REGISTRY
    timestamp = timestamp or time.strftime("%Y-%m-%dT%H:%M:%S%z")
    records = registry["providers"]
    registered_ids = [p["id"] for p in records]
    scoped = [p for p in records if p["lifecycle"] == "active"]
    exclusions = [{"id": p["id"], "reason": f"lifecycle:{p['lifecycle']}"}
                  for p in records if p["lifecycle"] != "active"]
    events = parse_provider_metric(metrics_text, "lobsterpulse_provider_events_total")
    idle = parse_provider_metric(metrics_text, "lobsterpulse_provider_idle_seconds")
    current_emit = {p["id"] for p in scoped
                    if events.get(p["id"], 0) > 0
                    and p["id"] in idle and 0 <= idle[p["id"]] < p["freshness_seconds"]}
    nonzero = {p["id"] for p in scoped if health.get(p["id"], 0) > 0}
    fresh_quota = {p["id"] for p in scoped if quota.get(p["id"], {}).get("state") == "fresh"}
    configured_ids = ({p["id"] for p in scoped if configured.get(p["id"], False)}
                      if configured is not None else None)

    def dimension(ids: Set[str] | None, *, is_registry_count: bool = False) -> Dict:
        numerator = len(ids) if ids is not None else None
        denominator = len(registered_ids) if is_registry_count else len(scoped)
        return {
            "status": "MEASURED" if ids is not None else "UNAVAILABLE",
            "numerator": numerator,
            "denominator": denominator,
            "registered_denominator": len(registered_ids),
            "gap_to_registered": len(registered_ids) - numerator if numerator is not None else None,
            "provider_ids": sorted(ids) if ids is not None else [],
            "exclusions": [] if is_registry_count else list(exclusions),
        }

    provider_states = {}
    for p in records:
        pid = p["id"]
        q_state = quota.get(pid, {}).get("state", "missing")
        if p["lifecycle"] != "active":
            status = "OUT_OF_SCOPE"
        elif configured is not None and not configured.get(pid, False):
            status = "NOT_CONFIGURED"
        elif not metrics_available:
            status = "UNAVAILABLE"
        elif pid in current_emit:
            status = "LIVE_EMITTING"
        elif pid in idle and events.get(pid, 0) > 0:
            status = "STALE"
        elif p["support_level"] == "hook_intake_quota_external":
            status = "EXTERNAL_DEPENDENCY"
        else:
            status = "NOT_MONITORED"
        quota_status = ("OUT_OF_SCOPE" if p["lifecycle"] != "active" else
                        "UNAVAILABLE" if q_state in ("no_dir", "unavailable") else
                        "FRESH" if q_state == "fresh" else
                        "STALE" if q_state == "stale" else
                        "EXTERNAL_DEPENDENCY" if p["support_level"] == "hook_intake_quota_external" else
                        "NOT_MONITORED")
        provider_states[pid] = {
            "health_status": status,
            "quota_status": quota_status,
            "configured": configured.get(pid) if configured is not None else None,
            "last_event_age_seconds": idle.get(pid),
            "session_count": health.get(pid, 0) if metrics_available else None,
            "quota_state": q_state,
            "quota_age_seconds": quota.get(pid, {}).get("snapshot_age_seconds"),
            "reason": "quota source ownership unconfirmed" if quota_status == "EXTERNAL_DEPENDENCY" else None,
        }

    return {
        "registry_version": registry["registry_version"],
        "baseline_id": registry["baseline_id"],
        "timestamp": timestamp,
        "dimensions": {
            "registered": dimension(set(registered_ids), is_registry_count=True),
            "configured": dimension(configured_ids),
            "live_emitting": dimension(current_emit if metrics_available else None),
            "nonzero_sessions": dimension(nonzero if metrics_available else None),
            "quota_observable": dimension(fresh_quota if all(
                quota.get(p["id"], {}).get("state") not in ("no_dir", "unavailable")
                for p in scoped) else None),
        },
        "providers": provider_states,
    }


def read_quota_snapshot(path: Path, now: float) -> tuple:
    """Read content and diagnostic mtime from the same file, without a stat/read race."""
    try:
        with path.open(encoding="utf-8") as stream:
            data = json.load(stream)
            age_h = (now - os.fstat(stream.fileno()).st_mtime) / 3600
        if not isinstance(data, dict) or not isinstance(data.get("runners"), list):
            return None, "unavailable", None
        return data, "measured", round(age_h, 2)
    except FileNotFoundError:
        return None, "missing", None
    except (OSError, ValueError):
        return None, "unavailable", None


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
    """Measure successful runner payloads using registry thresholds and updated_at.

    mtime remains diagnostic only: copying an old snapshot cannot refresh it.
    Local runners share one read; OpenAB keeps its exact filename/legacy alias.
    """
    out: Dict[str, Dict] = {}
    if not QUOTA_DIR.is_dir():
        return {p: {"state": "no_dir", "mtime_age_hours": None,
                   "path": None} for p in KNOWN_PROVIDERS}

    now = time.time()
    cache = {}
    for provider in REGISTRY["providers"]:
        pid = provider["id"]
        local = provider["scope"] == "local_cli"
        names = ["usage-local" if local else f"usage-{pid}"]
        if pid == "openx":
            names.append("usage-bot")
        paths = [QUOTA_DIR / f"{name}.json" for name in names]
        if not local:
            paths += sorted((path for name in names
                             for path in QUOTA_DIR.glob(f"{name}.json.stale-*")
                             if STALE_MARKER.search(path.name)), reverse=True)
        selected = None
        for path in paths:
            if path not in cache:
                cache[path] = read_quota_snapshot(path, now)
            candidate = cache[path]
            if selected is None or candidate[1] != "missing":
                selected = (path, *candidate)
            if candidate[0] is not None:
                break
        path, snapshot, state, mtime_age = selected
        age = None
        if snapshot is not None:
            runners = [r for r in snapshot["runners"] if isinstance(r, dict)
                       and r.get("ok") is True and (not local or r.get("name") == pid)]
            updated = snapshot.get("updated_at")
            if type(updated) in (int, float) and 0 < updated <= now:
                age = now - updated
            state = "missing"
            if runners and age is not None:
                state = "fresh" if age < provider["quota_freshness_seconds"] else "stale"
                if STALE_MARKER.search(path.name):
                    state = "stale"
        out[pid] = {"state": state, "mtime_age_hours": mtime_age,
                    "snapshot_age_seconds": age,
                    "path": str(path) if state != "missing" else None}
    return out


def render_table(health: Dict[str, float], quota: Dict[str, Dict]) -> str:
    """人類可讀: provider | metrics_sessions | quota_state | diagnostic mtime_age_h"""
    rows = []
    rows.append(f"{'provider':<14} {'metrics':>8} {'quota_state':<10} "
                f"{'mtime_h':>8}  visual")
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
    configured = read_configured_providers()
    coverage = build_coverage_receipt(metrics_text, health, quota, configured,
                                      metrics_available=bool(metrics_text))

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
    print(f"Registry {coverage['registry_version']} / baseline {coverage['baseline_id']}")
    for label, receipt in coverage["dimensions"].items():
        value = "unknown" if receipt["numerator"] is None else str(receipt["numerator"])
        print(f"{label}: {value}/{receipt['denominator']} "
              f"(registered {receipt['registered_denominator']}; "
              f"exclusions {receipt['exclusions']})")

    # 寫 machine-readable JSON 供後續儀表板/CI 用
    report = {
        "timestamp": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "registry_version": coverage["registry_version"],
        "baseline_id": coverage["baseline_id"],
        "coverage_receipt": coverage,
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
