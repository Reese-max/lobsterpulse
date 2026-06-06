#!/usr/bin/env python3
"""
K0 量化值漂移偵測 — 比對 .harness-k0.json 跟 MISSION R131 baseline 寫死常數

R132 落地。對齊 MISSION.md K0 量化閉合:
  k0_measure.py 印出當前 K0 量化值 (.harness-k0.json) 但 0 baseline 對齊 —
  量化值倒退沒人知, 跑了跟沒跑一樣 (hidden gap)。

本腳本補 K0 量測閉合:
  1. 讀 .harness-k0.json (k0_measure.py 當前快照)
  2. 跟 BASELINE 寫死常數比對 (R131 MISSION column 量化值)
  3. 持平/進步 → exit 0 PASS
  4. 倒退 → exit 1 FAIL (CI/手動跑都會 fail-closed)
  5. 產出 drift report table (stdout)

設計取捨:
  - BASELINE 寫死常數 (非讀 MISSION.md): MISSION 格式會變, regex 解析易碎;
    寫死常數版 baseline 改時需同步改 (透明, 對齊 R-CPT-4 「不開新 OTel 維度」護衛)。
  - 1 個 Python script, 0 Rust 護衛, 不破 K42 chain 19 條飽和契約。
  - 對齊 R131 接力清單「結構性量化解 K0」精神 (量化值與 code 真實一致)。
"""
import argparse
import json
import sys
from pathlib import Path
from typing import Dict, NamedTuple

# Windows cp950 預設會把 UTF-8 中文替換成 U+FFFD, pytest 子進程讀回時
# assertion 對不上. 強制 UTF-8 stdout/stderr 修掉 (對齊 k0_measure.py
# 跨平台可讀性).
if sys.platform == "win32":
    for _stream in (sys.stdout, sys.stderr):
        if hasattr(_stream, "reconfigure"):
            _stream.reconfigure(encoding="utf-8", errors="replace")


# R131 MISSION column K0 量化值 (寫死, baseline 改時同步改本常數)
# 對齊 engineering-log.md R131 entry 「KPI 進展表」前值欄:
#   K0-A1 emit 覆蓋 5/13, K0-A2 sample 覆蓋 1/13,
#   K0 Quota K0-B fresh 4/13, K0 Quota K0-Q 覆蓋 9/13
BASELINE: Dict[str, int] = {
    "k0a1_emit_covered": 5,
    "k0a2_sample_covered": 1,
    "k0b_fresh": 4,
    "k0q_coverage": 9,
}

K0_TOTAL = 13  # 對齊 hook_server.rs::KNOWN_PROVIDERS 4+9


class DriftResult(NamedTuple):
    """單一 KPI 維度漂移結果"""
    key: str
    baseline: int
    current: int
    delta: int
    status: str  # "PASS" | "REGRESS"


def load_current(json_path: Path) -> Dict[str, int]:
    """
    讀 .harness-k0.json 抽 4 個 KPI 維度的 covered 數字。
    檔不在 / JSON 壞 / 缺 key → raise FileNotFoundError 或 KeyError
    (讓 caller 決定 fail-closed 退出碼)。
    """
    data = json.loads(json_path.read_text(encoding="utf-8"))
    return {
        "k0a1_emit_covered": int(data["k0a1_health_emit"]["covered"]),
        "k0a2_sample_covered": int(data["k0a2_health_sample"]["covered"]),
        "k0b_fresh": int(data["k0b_quota_freshness"]["fresh"]),
        "k0q_coverage": int(data["k0q_quota_coverage"]["covered"]),
    }


def compute_drift(current: Dict[str, int]) -> list[DriftResult]:
    """比對 4 維度, 產出 DriftResult list。持平/進步=PASS, 倒退=REGRESS"""
    out = []
    for key, base in BASELINE.items():
        cur = current.get(key, 0)
        delta = cur - base
        status = "REGRESS" if delta < 0 else "PASS"
        out.append(DriftResult(key, base, cur, delta, status))
    return out


def render_report(results: list[DriftResult]) -> str:
    """人類可讀: KPI | baseline | current | delta | status (ASCII, 避 cp950)"""
    rows = []
    rows.append(f"{'KPI':<22} {'baseline':>8} {'current':>8} {'delta':>6}  status")
    rows.append("-" * 60)
    for r in results:
        delta_s = f"{r.delta:+d}" if r.delta != 0 else "  0"
        rows.append(
            f"{r.key:<22} {r.baseline:>5}/{K0_TOTAL:<2} {r.current:>5}/{K0_TOTAL:<2} "
            f"{delta_s:>6}  [{r.status}]"
        )
    return "\n".join(rows)


def main() -> int:
    p = argparse.ArgumentParser(
        description="K0 量化漂移偵測 (對齊 MISSION R131 baseline)"
    )
    p.add_argument(
        "--json",
        type=Path,
        default=Path(__file__).resolve().parent.parent / ".harness-k0.json",
        help="k0_measure.py 產出的 .harness-k0.json 路徑",
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
        print(f"[FAIL] .harness-k0.json 找不到: {args.json}", file=sys.stderr)
        print("       提示: 先跑 python scripts/k0_measure.py 產出", file=sys.stderr)
        return 2
    except (json.JSONDecodeError, KeyError) as e:
        print(f"[FAIL] .harness-k0.json 解析失敗: {e}", file=sys.stderr)
        return 2

    results = compute_drift(current)
    regressed = [r for r in results if r.status == "REGRESS"]
    progressed = [r for r in results if r.delta > 0]

    # --strict 模式: 任何一維有進步也算 FAIL (預警 KPI 量化口徑變動)
    fail = bool(regressed) or (args.strict and bool(progressed))

    print("=" * 60)
    print("K0 量化漂移偵測 — 對齊 MISSION R131 baseline")
    print(f"  source : {args.json}")
    print(f"  mode   : {'strict (進步也算 FAIL)' if args.strict else 'default (倒退才算 FAIL)'}")
    print("=" * 60)
    print(render_report(results))
    print("=" * 60)

    if fail:
        if regressed:
            print(f"[FAIL] {len(regressed)} 維度倒退, K0 量化值比 R131 baseline 差:")
            for r in regressed:
                print(f"       - {r.key}: {r.baseline} → {r.current} (Δ {r.delta})")
        if args.strict and progressed:
            print(f"[FAIL] {len(progressed)} 維度進步, --strict 模式拒絕 (KPI 量化口徑可能變):")
            for r in progressed:
                print(f"       - {r.key}: {r.baseline} → {r.current} (Δ +{r.delta})")
        return 1
    else:
        msg = "全部持平" if not progressed else f"{len(progressed)} 維度進步, 其餘持平"
        print(f"[PASS] K0 量化值 {msg}, 對齊 R131 baseline")
        return 0


if __name__ == "__main__":
    sys.exit(main())
