#!/usr/bin/env python3
r"""
K42 護衛鏈過期契約審計漂移偵測 — 比對 .harness-chain-staleness.json 跟
R172 量化真實值寫死 baseline

R194 落地。對齊 MISSION.md K42 護衛鏈 + R-CPT M3 接力清單
「R133+ 接力護衛 過期契約審計 (護衛對應 spec 最後更新時間)」:
  chain_staleness.py (R172 生產者) 印出當前 K42 量化值
  (.harness-chain-staleness.json) 但 0 baseline 對齊 — 量化值倒退 / 變動
  沒人知, 跑了跟沒跑一樣 (hidden gap)。
  本腳本補 K42 量測閉合 consumer 側 (鏡像 k40_drift_check.py R193 模式):

  1. 讀 .harness-chain-staleness.json (chain_staleness.py 當前快照)
  2. 跟 BASELINE 寫死常數比對 (R172 量化真實值: 16 test files + 0 stale +
     20 chain_count_min + 471 total_test_fn + overall_pass=True)
  3. 持平/進步 → exit 0 PASS
  4. 倒退 → exit 1 FAIL (CI/手動跑都會 fail-closed)
  5. 缺欄位 / JSON 壞 → exit 2 (解析失敗)
  6. 產出 drift report table (stdout)

守護口徑 (對齊 R172 K42 量化口徑):
  - file_count:        含 #[test] 的 .rs 檔數 (排除 target/)
  - total_test_fn:     全部護衛檔的 #[test] 函式總和
  - stale_count:       超 STALE_DAYS=90 沒更新的護衛檔數
  - chain_count_min:   K42 chain 健康下限 (對齊 r124_sentinel.py R124 chain 20)
  - overall_pass:      護衛鏈健康整體 PASS/FAIL (False = chain 掉或任何 stale)

不破 R97 紅線:
  - chain 計數仍由 r124_sentinel 守 (K42 chain 20)
  - 本腳本只補 K42 過期契約漂移偵測 consumer 側, 互補非取代
  - 純 read-only 量化偵測, 0 Rust 護衛, chain 20 → 20 守住
  - 對齊 k0_drift_check.py (R132) + k40_drift_check.py (R193) 模式
  - 5 case pytest 護衛 (持平/進步/倒退/缺欄位/JSON 損壞)

設計取捨:
  - BASELINE 寫死常數 (非讀 MISSION.md): MISSION 格式會變, regex 解析易碎;
    寫死常數版 baseline 改時需同步改 (透明, 對齊 R132 設計取捨)
  - overall_pass 當成 bool 維度 (R172 chain_staleness 寫的) — 跟數值維度同
    套 DriftResult, True 持平/進步 PASS, False 倒退 REGRESS
  - 5 case pytest 護衛 (持平/進步/倒退/缺欄位/JSON 損壞), 對齊 R132/R193 5 case 模式
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


# R172 chain_staleness 量化真實值 (寫死 baseline)
# 對齊 .harness-chain-staleness.json 當前快照 (2026-06-09 量測):
#   file_count        = 16  (含 #[test] 的 .rs 檔, 排除 target/)
#   total_test_fn     = 471 (全部護衛檔的 #[test] 函式總和)
#   stale_count       = 0   (超 STALE_DAYS=90 沒更新的護衛檔數)
#   chain_count_min   = 20  (K42 chain 健康下限, 對齊 r124_sentinel.py)
#   overall_pass      = True (chain 沒掉 + 無 stale = 健康)
BASELINE: Dict[str, Any] = {
    "file_count": 16,
    "total_test_fn": 471,
    "stale_count": 0,
    "chain_count_min": 20,
    "overall_pass": True,
}


class DriftResult(NamedTuple):
    """單一 K42 維度漂移結果"""
    key: str
    baseline: Any
    current: Any
    delta: str  # "持平" | "+N" | "-N" | "True→False" | "False→True"
    status: str  # "PASS" | "REGRESS"


def load_current(json_path: Path) -> Dict[str, Any]:
    """
    讀 .harness-chain-staleness.json 抽 5 個 K42 維度。
    檔不在 / JSON 壞 / 缺 key → raise FileNotFoundError / KeyError / ValueError
    (讓 caller 決定 fail-closed 退出碼)。
    """
    data = json.loads(json_path.read_text(encoding="utf-8"))
    return {
        "file_count": int(data["file_count"]),
        "total_test_fn": int(data["total_test_fn"]),
        "stale_count": int(data["stale_count"]),
        "chain_count_min": int(data["chain_count_min"]),
        "overall_pass": bool(data["overall_pass"]),
    }


def _compute_delta(key: str, base: Any, cur: Any) -> str:
    """產出 delta 字串, 數值維度算 +/-N, bool 維度算 True↔False 標記"""
    if isinstance(base, bool) and isinstance(cur, bool):
        if base == cur:
            return "持平"
        return "True→False" if base else "False→True"
    if isinstance(base, (int, float)) and isinstance(cur, (int, float)):
        d = cur - base
        return "持平" if d == 0 else f"{d:+d}"
    return "持平" if base == cur else f"{base!r}→{cur!r}"


def compute_drift(current: Dict[str, Any]) -> list[DriftResult]:
    """
    比對 5 維度, 產出 DriftResult list。

    守護口徑:
      - file_count 倒退 → REGRESS (護衛檔掉了)
      - total_test_fn 倒退 → REGRESS (護衛函式少了)
      - stale_count 增加 → REGRESS (新過期護衛)
      - chain_count_min 倒退 → REGRESS (chain 護衛數掉)
      - overall_pass True→False → REGRESS (chain 健康度退步)
      - overall_pass False→True → PASS (chain 健康度回升, 不算進步擋)
    """
    out = []
    for key, base in BASELINE.items():
        cur = current.get(key)
        if cur is None:
            # 缺欄位時 raise 給 main() 處理, 不在這裡 fail-closed
            raise KeyError(key)
        delta = _compute_delta(key, base, cur)
        # 判定 REGRESS 規則
        if isinstance(base, bool) and isinstance(cur, bool):
            status = "REGRESS" if (base and not cur) else "PASS"
        elif isinstance(base, int) and isinstance(cur, int):
            # stale_count 特殊: 增加 = REGRESS (新過期)
            if key == "stale_count":
                status = "REGRESS" if cur > base else "PASS"
            else:
                status = "REGRESS" if cur < base else "PASS"
        else:
            status = "PASS"
        out.append(DriftResult(key, base, cur, delta, status))
    return out


def render_report(results: list[DriftResult]) -> str:
    """人類可讀: KPI | baseline | current | delta | status (ASCII, 避 cp950)"""
    rows = []
    rows.append(f"{'KPI':<20} {'baseline':>10} {'current':>10} {'delta':>14}  status")
    rows.append("-" * 72)
    for r in results:
        if isinstance(r.baseline, bool):
            base_s = str(r.baseline)
            cur_s = str(r.current)
        elif isinstance(r.baseline, int):
            base_s = f"{r.baseline}"
            cur_s = f"{r.current}"
        else:
            base_s = repr(r.baseline)
            cur_s = repr(r.current)
        rows.append(
            f"{r.key:<20} {base_s:>10} {cur_s:>10} {r.delta:>14}  [{r.status}]"
        )
    return "\n".join(rows)


def main() -> int:
    p = argparse.ArgumentParser(
        description="K42 護衛鏈過期契約漂移偵測 (對齊 R172 baseline)"
    )
    p.add_argument(
        "--json",
        type=Path,
        default=Path(__file__).resolve().parent.parent / ".harness-chain-staleness.json",
        help="chain_staleness.py 產出的 .harness-chain-staleness.json 路徑",
    )
    p.add_argument(
        "--strict",
        action="store_true",
        help="嚴格模式: 進步也算 FAIL (預設持平/進步都 PASS)",
    )
    args = p.parse_args()

    try:
        current = load_current(args.json)
    except FileNotFoundError:
        print(f"[FAIL] .harness-chain-staleness.json 找不到: {args.json}", file=sys.stderr)
        print("       提示: 先跑 python scripts/chain_staleness.py 產出", file=sys.stderr)
        return 2
    except (json.JSONDecodeError, KeyError, ValueError) as e:
        print(f"[FAIL] .harness-chain-staleness.json 解析失敗: {e}", file=sys.stderr)
        return 2

    try:
        results = compute_drift(current)
    except KeyError as e:
        print(f"[FAIL] 缺欄位: {e}", file=sys.stderr)
        return 2

    regressed = [r for r in results if r.status == "REGRESS"]
    progressed = [
        r for r in results
        if (isinstance(r.baseline, (int, float)) and isinstance(r.current, (int, float)) and r.current > r.baseline)
        or (isinstance(r.baseline, bool) and isinstance(r.current, bool) and (not r.baseline and r.current))
    ]

    # --strict 模式: 任何一維有進步也算 FAIL (預警 KPI 量化口徑變動)
    fail = bool(regressed) or (args.strict and bool(progressed))

    print("=" * 72)
    print("K42 護衛鏈過期契約漂移偵測 — 對齊 R172 chain_staleness baseline")
    print(f"  source : {args.json}")
    print(f"  mode   : {'strict (進步也算 FAIL)' if args.strict else 'default (倒退才算 FAIL)'}")
    print("=" * 72)
    print(render_report(results))
    print("=" * 72)

    if fail:
        if regressed:
            print(f"[FAIL] {len(regressed)} 維度倒退, K42 量化值比 R172 baseline 差:")
            for r in regressed:
                print(f"       - {r.key}: {r.baseline} → {r.current} (Δ {r.delta})")
        if args.strict and progressed:
            print(f"[FAIL] {len(progressed)} 維度進步, --strict 模式拒絕 (KPI 量化口徑可能變):")
            for r in progressed:
                print(f"       - {r.key}: {r.baseline} → {r.current} (Δ {r.delta})")
        return 1
    else:
        msg = "全部持平" if not progressed else f"{len(progressed)} 維度進步, 其餘持平"
        print(f"[PASS] K42 護衛鏈 {msg}, 對齊 R172 baseline")
        return 0


if __name__ == "__main__":
    sys.exit(main())
