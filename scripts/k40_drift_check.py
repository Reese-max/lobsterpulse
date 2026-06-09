#!/usr/bin/env python3
r"""
K40 量化漂移偵測 — 比對 .harness-k40.json 跟 R192 量化真實值寫死 baseline

R193 落地。對齊 MISSION.md K40 規格覆蓋率量化閉合:
  k40_measure.py (R192 生產者) 印出當前 K40 量化值 (.harness-k40.json) 但
  0 baseline 對齊 — 量化值倒退 / 變動沒人知, 跑了跟沒跑一樣 (hidden gap)。
  本腳本補 K40 量測閉合 consumer 側 (鏡像 k0_drift_check.py R132 模式):

  1. 讀 .harness-k40.json (k40_measure.py 當前快照)
  2. 跟 BASELINE 寫死常數比對 (R192 量化真實值: 10 total + 8 closed + 2 active)
  3. 持平/進步 → exit 0 PASS
  4. 倒退 → exit 1 FAIL (CI/手動跑都會 fail-closed)
  5. 缺欄位 / JSON 壞 → exit 2 (解析失敗)
  6. 產出 drift report table (stdout)

守護口徑 (對齊 R192 K40 量化口徑):
  - k40_changes_total:   openspec/changes/ 排除 archive/ 的有 tasks.md change 數
  - k40_changes_closed:  全 [x] 的 change 數 (R192 守護算法 = 0/0 算 closed)
  - k40_changes_active:  有 [ ] 的 change 數 (R192 守護算法)
  - active_names:        量化真實 active 清單 (對齊 mission-k0 + otel-genai)

不破 R97 紅線:
  - chain 計數仍由 r124_sentinel 守 (K42 chain 20)
  - 本腳本只補 K40 量化底層 consumer 側, 互補非取代
  - 純 read-only 量化偵測, 0 Rust 護衛, chain 20 → 20 守住
  - 對齊 k0_drift_check.py (R132) 模式, 不開新 OTel 維度護衛

設計取捨:
  - BASELINE 寫死常數 (非讀 MISSION.md): MISSION 格式會變, regex 解析易碎;
    寫死常數版 baseline 改時需同步改 (透明, 對齊 R132 k0_drift_check 取捨)
  - active_names 用集合比對 (順序無關, 內容相等即 PASS)
  - 5 case pytest 護衛 (持平/進步/倒退/缺欄位/JSON 損壞), 對齊 R132 5 case 模式
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any, Dict, NamedTuple

# Windows cp950 預設會把 UTF-8 中文替換成 U+FFFD, pytest 子進程讀回時
# assertion 對不上. 強制 UTF-8 stdout/stderr 修掉 (對齊 k0_drift_check.py
# 跨平台可讀性).
if sys.platform == "win32":
    for _stream in (sys.stdout, sys.stderr):
        if hasattr(_stream, "reconfigure"):
            _stream.reconfigure(encoding="utf-8", errors="replace")


# R192 K40 量化真實值 (寫死 baseline)
# 對齊 engineering-log.md R192 entry 「KPI 進展表」前值欄:
#   k40_changes_total   = 10 (含 archive/ 排除)
#   k40_changes_closed  = 8  (全 [x] 8 change: contract-matrix-guard/cross-provider-timeline/
#                             lobster-rules-engine/openab-bot-sync/otel-provider-metrics-contract/
#                             prometheus-counter-convention/prometheus-counter-rename-2026-q3/
#                             r114-k0-coverage-and-dual-emit-guard)
#   k40_changes_active  = 2  (mission-k0-restructure-2026-q3 3/15 + otel-genai-runtime-emit-2026-q3 9/16)
#   active_names        = sorted([mission-k0, otel-genai])
BASELINE: Dict[str, Any] = {
    "k40_changes_total": 10,
    "k40_changes_closed": 8,
    "k40_changes_active": 2,
    "active_names": ("mission-k0-restructure-2026-q3", "otel-genai-runtime-emit-2026-q3"),
}


class DriftResult(NamedTuple):
    """單一 K40 維度漂移結果"""
    key: str
    baseline: Any
    current: Any
    delta: int  # for scalar; 0 for set
    status: str  # "PASS" | "REGRESS" | "ADVANCE" (active_names set 縮 = advance)


def load_current(json_path: Path) -> Dict[str, Any]:
    """讀 .harness-k40.json 抽 4 個 K40 量化維度。
    檔不在 / JSON 壞 / 缺 key → raise FileNotFoundError 或 KeyError
    (讓 caller 決定 fail-closed 退出碼)。
    """
    data = json.loads(json_path.read_text(encoding="utf-8"))
    return {
        "k40_changes_total": int(data["k40_changes_total"]),
        "k40_changes_closed": int(data["k40_changes_closed"]),
        "k40_changes_active": int(data["k40_changes_active"]),
        "active_names": tuple(sorted(data.get("active_names", []))),
    }


def compute_drift(current: Dict[str, Any]) -> list[DriftResult]:
    """比對 4 維度, 產出 DriftResult list。

    active_names 集合語意:
      - current ⊆ baseline (主動 closure 推進, active 縮減): PASS
      - current ⊃ baseline (新 active change 出現): REGRESS
      - current = baseline: PASS
      - current ≠ baseline 且非子集 (既有 active 換成另一個): REGRESS
    """
    out: list[DriftResult] = []

    # total: 變動 = 漂移 (增減都算 — PUA 不主動加 change)
    cur_total = current["k40_changes_total"]
    out.append(DriftResult(
        "k40_changes_total",
        BASELINE["k40_changes_total"],
        cur_total,
        cur_total - BASELINE["k40_changes_total"],
        "PASS" if cur_total == BASELINE["k40_changes_total"] else "REGRESS",
    ))

    # closed: 持平/進步 = PASS, 倒退 = REGRESS
    cur_closed = current["k40_changes_closed"]
    out.append(DriftResult(
        "k40_changes_closed",
        BASELINE["k40_changes_closed"],
        cur_closed,
        cur_closed - BASELINE["k40_changes_closed"],
        "PASS" if cur_closed >= BASELINE["k40_changes_closed"] else "REGRESS",
    ))

    # active: 持平/縮減 = PASS (active 變少 = 進步), 增加 = REGRESS
    cur_active = current["k40_changes_active"]
    out.append(DriftResult(
        "k40_changes_active",
        BASELINE["k40_changes_active"],
        cur_active,
        cur_active - BASELINE["k40_changes_active"],
        "PASS" if cur_active <= BASELINE["k40_changes_active"] else "REGRESS",
    ))

    # active_names: 集合子集 (含相等) = PASS (主動 closure 推進 OK),
    # 集合擴大 / 完全換掉 = REGRESS (新 active change 出現或既有換走)
    cur_names = set(current["active_names"])
    base_names = set(BASELINE["active_names"])
    if cur_names <= base_names:
        names_status = "PASS"
    else:
        names_status = "REGRESS"
    out.append(DriftResult(
        "active_names",
        sorted(base_names),
        sorted(cur_names),
        len(cur_names) - len(base_names),
        names_status,
    ))

    return out


def render_report(results: list[DriftResult]) -> str:
    """人類可讀: KPI | baseline | current | delta | status (ASCII, 避 cp950)"""
    rows = []
    rows.append(f"{'KPI':<22} {'baseline':<40} {'current':<40}  status")
    rows.append("-" * 110)
    for r in results:
        base_s = repr(r.baseline)[:40]
        cur_s = repr(r.current)[:40]
        if r.delta == 0 and r.key != "active_names":
            delta_s = "  0"
        elif r.key == "active_names":
            delta_s = "  -"
        else:
            delta_s = f"{r.delta:+d}"
        rows.append(
            f"{r.key:<22} {base_s:<40} {cur_s:<40}  [{r.status}]"
        )
    return "\n".join(rows)


def main() -> int:
    p = argparse.ArgumentParser(
        description="K40 量化漂移偵測 (對齊 R192 量化真實值 baseline)"
    )
    p.add_argument(
        "--json",
        type=Path,
        default=Path(__file__).resolve().parent.parent / ".harness-k40.json",
        help="k40_measure.py 產出的 .harness-k40.json 路徑",
    )
    p.add_argument(
        "--strict",
        action="store_true",
        help="嚴格模式: 進步也算 FAIL (預設 closed 進步 / active 縮減都 PASS)",
    )
    args = p.parse_args()

    try:
        current = load_current(args.json)
    except FileNotFoundError:
        print(f"[FAIL] .harness-k40.json 找不到: {args.json}", file=sys.stderr)
        print("       提示: 先跑 python scripts/k40_measure.py 產出", file=sys.stderr)
        return 2
    except (json.JSONDecodeError, KeyError) as e:
        print(f"[FAIL] .harness-k40.json 解析失敗: {e}", file=sys.stderr)
        return 2

    results = compute_drift(current)
    regressed = [r for r in results if r.status == "REGRESS"]

    # --strict 模式: closed 增加 (advance) 或 active 縮減 (advance) 也算 FAIL
    # (預警 KPI 量化口徑可能變動, PUA 不主動進步閉合)
    advanced: list[DriftResult] = []
    if args.strict:
        for r in results:
            if r.key == "k40_changes_closed" and r.delta > 0:
                advanced.append(r)
            elif r.key == "k40_changes_active" and r.delta < 0:
                advanced.append(r)

    fail = bool(regressed) or (args.strict and bool(advanced))

    print("=" * 110)
    print("K40 量化漂移偵測 — 對齊 R192 量化真實值 baseline (鏡像 k0_drift_check.py R132 模式)")
    print(f"  source : {args.json}")
    print(f"  mode   : {'strict (進步也算 FAIL)' if args.strict else 'default (倒退才算 FAIL)'}")
    print("=" * 110)
    print(render_report(results))
    print("=" * 110)

    if fail:
        if regressed:
            print(f"[FAIL] {len(regressed)} 維度倒退, K40 量化值比 R192 baseline 差:")
            for r in regressed:
                print(f"       - {r.key}: {r.baseline!r} → {r.current!r} (Δ {r.delta:+d})")
        if args.strict and advanced:
            print(f"[FAIL] {len(advanced)} 維度進步, --strict 模式拒絕 (KPI 量化口徑可能變):")
            for r in advanced:
                print(f"       - {r.key}: {r.baseline!r} → {r.current!r} (Δ {r.delta:+d})")
        return 1
    else:
        held = [r for r in results if r.delta == 0 or r.key == "active_names"]
        moved = [r for r in results if r.delta != 0 and r.key != "active_names"]
        if not moved:
            print("[PASS] K40 量化值 4 維度全對齊 R192 baseline, 守住")
        else:
            adv = [r for r in moved if r.delta > 0 or r.key == "k40_changes_active"]
            print(f"[PASS] K40 量化值 {len(moved)} 維度有變動 (進步/持平混合), 對齊 R192 baseline")
        return 0


if __name__ == "__main__":
    sys.exit(main())
