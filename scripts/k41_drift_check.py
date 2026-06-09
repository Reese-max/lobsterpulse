#!/usr/bin/env python3
"""
K41 量化口徑漂移偵測 — 比對 k41_chore_treadmill.py 量化口徑常數 vs 寫死 BASELINE

R191 落地。對齊 MISSION.md K41 量化閉合:
  k41_chore_treadmill.py 印出當前 K41 7d 量化值 (.harness-k41.json) 但
  0 量化口徑守護 — 有人改 GOVERNANCE_PREFIXES tuple (漏算 refactor/archive
  算進 chore / 漏算 chore(scope) 雙類型 R176 fix) 或 WINDOW_DAYS / THRESHOLD
  常數, K41 量化值跟 MISSION baseline 對不上, 跑了跟沒跑一樣 (hidden gap)。

本腳本補 K41 量測閉合 (鏡像 k0_drift_check.py R132 模式):
  1. AST 解析 k41_chore_treadmill.py 拿 4 個量化口徑常數:
     - GOVERNANCE_PREFIXES: tuple[str, ...]  (chore/refactor/archive/sensor)
     - WINDOW_DAYS: int                       (7d 視窗)
     - THRESHOLD: float                       (0.30 警戒線)
     - _classify_prefix: 函式本體內處理 chore(scope) + chore(spec)+docs 雙類型
  2. 跟 BASELINE 寫死常數比對 (R188 7d 11.8% 對齊時的量化口徑)
  3. 持平 → exit 0 PASS
  4. 任一常數被改 → exit 1 FAIL (drift 偵測)
  5. 產出 drift report table (stdout)

設計取捨:
  - 用 AST 解析 source 拿常數 (非 import 跑): 避免 k41_chore_treadmill.py
    跑 git log 副作用, 也避免被 import 邊界耦合
  - BASELINE 寫死常數 (非讀 MISSION.md): MISSION 格式會變, regex 解析易碎;
    寫死常數版 baseline 改時需同步改 (透明)
  - 1 個 Python script, 0 Rust 護衛, 不破 K42 chain 20 條飽和契約
  - 對齊 R132 R-CPT M3 spec closure 軸 (K41 量化口徑守護補鏈路)
"""
from __future__ import annotations

import argparse
import ast
import sys
from pathlib import Path
from typing import Any, NamedTuple


# Windows cp950 預設會把 UTF-8 中文替換成 U+FFFD, pytest 子進程讀回時
# assertion 對不上. 強制 UTF-8 stdout/stderr 修掉 (對齊 k0_drift_check.py
# 跨平台可讀性).
if sys.platform == "win32":
    for _stream in (sys.stdout, sys.stderr):
        if hasattr(_stream, "reconfigure"):
            _stream.reconfigure(encoding="utf-8", errors="replace")


# R188 7d 11.8% 對齊時的 K41 量化口徑常數 (寫死 baseline)
# 對齊 k41_chore_treadmill.py 當前 source 內 4 個量化口徑常數
# 改壞任一就 FAIL (drift 偵測)
BASELINE: dict[str, Any] = {
    "governance_prefixes": ("chore", "refactor", "archive", "sensor"),
    "window_days": 7,
    "threshold": 0.30,
    "classify_chore_scope": True,  # R176 修的 chore(scope) 分類不漂移
    "classify_chore_plus_docs": True,  # R176 修的雙類型 + 分類不漂移
}


class DriftResult(NamedTuple):
    """單一量化口徑常數漂移結果"""
    key: str
    baseline: Any
    current: Any
    status: str  # "PASS" | "REGRESS"


def _extract_constant(tree: ast.Module, name: str) -> Any:
    """從 AST module-level 抽指定常數的值 (scalar / tuple / list)
    找不到 / 不是 Assign target → raise ValueError (讓 caller 決定 fail-closed)
    """
    for node in tree.body:
        if isinstance(node, ast.Assign):
            for target in node.targets:
                if isinstance(target, ast.Name) and target.id == name:
                    return ast.literal_eval(node.value)
    raise ValueError(f"常數 {name} 找不到或不是 literal")


def _check_classify_chore_scope(source: str) -> bool:
    """檢查 source 內有 `if "(" in head` 處理 (R176 修的 chore(scope))

    防有人把這段刪掉讓 chore(scope) 不再被分類為 chore
    """
    return 'if "(" in head' in source


def _check_classify_chore_plus_docs(source: str) -> bool:
    """檢查 source 內有 `head.split("+", 1)[0]` 雙類型處理 (R176 修的 chore(spec)+docs)

    防有人把這段刪掉讓雙類型 commit 全部漏算
    """
    return 'head.split("+", 1)[0]' in source


def measure(script_path: Path) -> list[DriftResult]:
    """AST 解析 k41_chore_treadmill.py 拿量化口徑常數, 對齊 BASELINE"""
    src = script_path.read_text(encoding="utf-8", errors="replace")
    tree = ast.parse(src, filename=str(script_path))

    cur_prefixes = _extract_constant(tree, "GOVERNANCE_PREFIXES")
    cur_window = _extract_constant(tree, "WINDOW_DAYS")
    cur_threshold = _extract_constant(tree, "THRESHOLD")
    cur_scope = _check_classify_chore_scope(src)
    cur_plus = _check_classify_chore_plus_docs(src)

    results: list[DriftResult] = []
    results.append(DriftResult(
        "governance_prefixes",
        BASELINE["governance_prefixes"],
        tuple(cur_prefixes) if not isinstance(cur_prefixes, tuple) else cur_prefixes,
        "PASS" if tuple(cur_prefixes) == BASELINE["governance_prefixes"] else "REGRESS",
    ))
    results.append(DriftResult(
        "window_days",
        BASELINE["window_days"],
        int(cur_window),
        "PASS" if int(cur_window) == BASELINE["window_days"] else "REGRESS",
    ))
    # THRESHOLD 是 float 比較, 用 abs 差容忍微浮點
    threshold_ok = abs(float(cur_threshold) - BASELINE["threshold"]) < 1e-9
    results.append(DriftResult(
        "threshold",
        BASELINE["threshold"],
        float(cur_threshold),
        "PASS" if threshold_ok else "REGRESS",
    ))
    results.append(DriftResult(
        "classify_chore_scope",
        BASELINE["classify_chore_scope"],
        cur_scope,
        "PASS" if cur_scope == BASELINE["classify_chore_scope"] else "REGRESS",
    ))
    results.append(DriftResult(
        "classify_chore_plus_docs",
        BASELINE["classify_chore_plus_docs"],
        cur_plus,
        "PASS" if cur_plus == BASELINE["classify_chore_plus_docs"] else "REGRESS",
    ))
    return results


def render_report(results: list[DriftResult]) -> str:
    """人類可讀: KPI | baseline | current | status (ASCII, 避 cp950)"""
    rows = []
    rows.append(f"{'KPI':<28} {'baseline':<22} {'current':<22}  status")
    rows.append("-" * 88)
    for r in results:
        base_s = repr(r.baseline)[:22]
        cur_s = repr(r.current)[:22]
        rows.append(
            f"{r.key:<28} {base_s:<22} {cur_s:<22}  [{r.status}]"
        )
    return "\n".join(rows)


def main() -> int:
    p = argparse.ArgumentParser(
        description="K41 量化口徑漂移偵測 (對齊 MISSION R188 11.8% baseline)"
    )
    p.add_argument(
        "--script",
        type=Path,
        default=Path(__file__).resolve().parent / "k41_chore_treadmill.py",
        help="k41_chore_treadmill.py 路徑 (預設 scripts/k41_chore_treadmill.py)",
    )
    args = p.parse_args()

    if not args.script.exists():
        print(f"[FAIL] 腳本找不到: {args.script}", file=sys.stderr)
        return 2
    try:
        results = measure(args.script)
    except (SyntaxError, ValueError) as e:
        print(f"[FAIL] 解析 {args.script} 失敗: {e}", file=sys.stderr)
        return 2

    regressed = [r for r in results if r.status == "REGRESS"]
    fail = bool(regressed)

    print("=" * 88)
    print("K41 量化口徑漂移偵測 — 對齊 MISSION R188 11.8% baseline (鏡像 k0_drift_check.py R132 模式)")
    print(f"  source : {args.script}")
    print("=" * 88)
    print(render_report(results))
    print("=" * 88)

    if fail:
        print(f"[FAIL] {len(regressed)} 維度漂移, K41 量化口徑常數被改:")
        for r in regressed:
            print(f"       - {r.key}: {r.baseline!r} → {r.current!r}")
        return 1
    else:
        print(f"[PASS] K41 量化口徑 5 維度全對齊, BASELINE 守護守住")
        return 0


if __name__ == "__main__":
    sys.exit(main())
