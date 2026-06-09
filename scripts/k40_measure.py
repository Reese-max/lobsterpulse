#!/usr/bin/env python3
r"""
K40 KPI 量測腳本 — 量化 LobsterPulse openspec/changes/ spec 完成度

R192 落地。對齊 MISSION.md K40 規格覆蓋率:
  K40 = 量化 spec change 的 closure 進度 (1/1 active 12/12 → 8/9 closed + 1 active 9/16)。
  本腳本是 K40 量化「生產者」(鏡像 k0_measure.py 模式), R191 K41 模式換
  K40 維度。R132 接力清單把 K40 量化底層列為 M1 候選, R192 接力補鏈路。

  量化口徑:
    - k40_changes_total    = openspec/changes/ 有 tasks.md 的 change 數 (排除 archive/)
    - k40_changes_closed   = 全 [x] 的 change 數 (無 [ ])
    - k40_changes_active   = 有 [ ] 的 change 數
    - k40_active_progress  = 每個 active change 的 [x]/total (例 9/16)

  守護口徑不漂移 (對齊 R187/R188/R189/R191 M2 KPI 量測 closure 軸):
    1. closed 算法 = 全 [x] 算 closed (一行 [ ] 都算 active)
    2. active 算法 = 有 [ ] 算 active
    3. 真實量化值 (寫 .harness-k40.json) 由消費者 (k40_drift_check.py 之後接力)
       對齊 MISSION 表; PUA 不 patch MISSION 量化值 (owner M scope Path A 才動)
    4. tasks.md 解析走 r`^\s*-\s*\[[ x]\]` 正則, 對齊既 grep --count 算法

不破 R97 紅線:
  - chain 計數仍由 r124_sentinel 守 (K42 chain 20)
  - 本腳本只補 K40 量化底層, 互補非取代
  - 純 read-only 量化, 0 Rust 護衛, chain 20 → 20 守住

對齊:
  - k0_measure.py (R83): 同樣 1 個 Python script + dataclass + measure/render/main 三段
  - k0_drift_check.py (R132): 寫死 BASELINE 常數, 後續消費者對齊
  - r124_sentinel.py (R124): 同樣 .harness-*.json 輸出格式
"""
from __future__ import annotations

import argparse
import json
import re
import sys
import time
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Final

REPO_ROOT: Final[Path] = Path(__file__).resolve().parent.parent
SPEC_DIR: Final[Path] = REPO_ROOT / "openspec" / "changes"
OUTPUT_JSON: Final[Path] = REPO_ROOT / ".harness-k40.json"

# Windows cp950 預設會把 UTF-8 中文替換成 U+FFFD, 強制 UTF-8 stdout/stderr
if sys.platform == "win32":
    for _stream in (sys.stdout, sys.stderr):
        if hasattr(_stream, "reconfigure"):
            _stream.reconfigure(encoding="utf-8", errors="replace")


@dataclass(frozen=True)
class ChangeProgress:
    """單一 spec change 的 closure 進度"""
    name: str
    total_tasks: int
    closed_tasks: int
    is_closed: bool  # True = 全 [x], False = 有 [ ]
    progress: str  # 顯示用 "9/16"


_TASK_RE: Final[re.Pattern[str]] = re.compile(r"^\s*-\s*\[([ x])\]", re.MULTILINE)


def _iter_change_dirs(spec_root: Path) -> list[Path]:
    """遞迴列 openspec/changes/ 下所有含 tasks.md 的 change 目錄
    排除 archive/ 子樹 (歷史封存, 不算 active K40 量化)
    """
    out: list[Path] = []
    if not spec_root.exists():
        return out
    for d in sorted(spec_root.iterdir()):
        if not d.is_dir():
            continue
        if "archive" in d.parts or d.name == "archive":
            continue
        if (d / "tasks.md").is_file():
            out.append(d)
    return out


def _parse_tasks(tasks_path: Path) -> tuple[int, int]:
    r"""解析 tasks.md 算 total / closed。
    一行 `^\s*-\s*\[ \]` 算 active task,
    一行 `^\s*-\s*\[x\]` 算 closed task。
    不存在的 [ ] 視為 0 (空 tasks.md = 0/0, 邊界由守護 case 5 cover)。
    """
    try:
        text = tasks_path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return (0, 0)
    closed = 0
    active = 0
    for m in _TASK_RE.finditer(text):
        if m.group(1) == "x":
            closed += 1
        else:
            active += 1
    return (closed + active, closed)


def measure(spec_root: Path = SPEC_DIR) -> list[ChangeProgress]:
    """量測每個 change 的 closure 進度"""
    out: list[ChangeProgress] = []
    for d in _iter_change_dirs(spec_root):
        total, closed = _parse_tasks(d / "tasks.md")
        out.append(ChangeProgress(
            name=d.name,
            total_tasks=total,
            closed_tasks=closed,
            # 0/0 視為 closed (無 active task = 0/0 算完成, 防 0/0 變 active
            # 干擾 K40 closed/active 量化)。守護 case 5 cover 邊界。
            is_closed=(closed == total),
            progress=f"{closed}/{total}" if total > 0 else "0/0",
        ))
    return out


def render_table(results: list[ChangeProgress]) -> str:
    """stdout 表格: change | progress | closed/total | state"""
    header = f"{'change':<48} {'progress':>10}  {'state':<8}"
    rows = [header, "-" * len(header)]
    closed_count = 0
    active_count = 0
    for r in sorted(results, key=lambda x: (x.is_closed, x.name)):
        state = "closed" if r.is_closed else "active"
        if r.is_closed:
            closed_count += 1
        else:
            active_count += 1
        rows.append(f"{r.name:<48} {r.progress:>10}  {state:<8}")
    rows.append("-" * len(header))
    rows.append(
        f"{'TOTAL':<48} {closed_count:>5}/{len(results):<5}  "
        f"closed={closed_count} active={active_count}"
    )
    return "\n".join(rows)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="K40 spec 量化 (鏡像 k0_measure 模式)")
    parser.add_argument("--json", type=Path, default=OUTPUT_JSON,
                        help=f"輸出 JSON 路徑 (預設 {OUTPUT_JSON.name})")
    parser.add_argument("--spec-root", type=Path, default=SPEC_DIR,
                        help=f"掃的 spec root (預設 {SPEC_DIR})")
    args = parser.parse_args(argv)

    results = measure(spec_root=args.spec_root)
    closed = sum(1 for r in results if r.is_closed)
    active = len(results) - closed
    table = render_table(results)

    print(f"LobsterPulse K40 spec 量測 — {time.strftime('%Y-%m-%d %H:%M:%S')}")
    print(f"  spec root : {args.spec_root}")
    print(f"  files     : {len(results)} (含 archive/ 排除)")
    print("=" * 70)
    print(table)
    print("=" * 70)
    print(f"K40 規格覆蓋率: {closed}/{len(results)} closed + "
          f"{active} active  (MISSION 對齊: 8/9 closed + 1 active 9/16)")
    # 提示量化口徑差異: MISSION 表只列 1 active (otel-genai 9/16), 但實際
    # mission-k0-restructure-2026-q3 也是 active (3/15, T-MKR4 [ ] + Phase 2/3
    # 全部 [ ])。這是 K40 spec drift 證據, PUA 量化真實值不 patch MISSION。
    active_names = sorted(r.name for r in results if not r.is_closed)
    if active_names:
        print(f"[K40 active changes (量化真實): {active_names}]")

    payload = {
        "timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "spec_root": str(args.spec_root),
        "k40_changes_total": len(results),
        "k40_changes_closed": closed,
        "k40_changes_active": active,
        "active_names": active_names,
        "mission_alignment_note": (
            "MISSION K40 表寫 8/9 closed + 1 active 9/16 (otel-genai). "
            "量化真實值: closed={closed} active={active}. "
            "PUA 守護量化算法不漂移, 不 patch MISSION (owner M scope Path A 才動)."
        ).format(closed=closed, active=active),
        "changes": [asdict(r) for r in results],
    }
    args.json.write_text(json.dumps(payload, indent=2, ensure_ascii=False),
                         encoding="utf-8")
    print(f"\nJSON 寫入: {args.json}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
