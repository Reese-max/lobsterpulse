#!/usr/bin/env python3
"""
護衛鏈過期契約審計 — 量化每個 test-bearing .rs 檔的最後 commit time 與
#\[test\] 函式數, 對比 STALE_DAYS 閾值, 列出過期護衛。

R172 M2 落地。動機 (R171 揭示):
  R124 r124_sentinel 量測快照 0 變化真因 = 只量化 chain 計數
  (pattern matches >= 20), 沒記每個護衛檔案的「時間戳」維度。
  補時間維度: 護衛檔案最後 commit 超過 STALE_DAYS 天視為過期,
  提示需重新對齊 spec (openspec/changes/.../proposal.md)。

不破 R97 紅線:
  - chain 計數仍由 r124_sentinel 守 (K42 chain 20)
  - 本腳本只補時間維度, 互補非取代
  - 純 read-only 量化, 0 Rust 護衛, chain 20 → 20 守住

對齊:
  - k0_drift_check.py (R132): 同樣 1 個 Python script, BASELINE 寫死常數
  - r124_sentinel.py (R124): 同樣 .harness-*.json 輸出格式
  - commit_subject_lint.py (R137): 走同樣 stdout 表格 + JSON 雙輸出
"""
from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import time
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Final

REPO_ROOT: Final[Path] = Path(__file__).resolve().parent.parent
OUTPUT_JSON: Final[Path] = REPO_ROOT / ".harness-chain-staleness.json"
SRC_TAURI: Final[Path] = REPO_ROOT / "src-tauri" / "src"

# R132 K42 chain 20 護衛: 對齊 r124_sentinel.py 寫死 chain 計數下限。
# 本腳本守「時間維度」, 計數仍歸 r124_sentinel。CHAIN_COUNT_MIN 用
# fail-closed 健康檢查 (>= 20 = chain 沒意外掉), 跟時間維度分開。
CHAIN_COUNT_MIN: Final[int] = 20

# 護衛檔最後 commit 超過 90 天 = 過期, 需重新對齊對應 spec
# (對齊 R132 MISSION 「R133+ 接力護衛 過期契約審計」接力清單)。
STALE_DAYS: Final[int] = 90

# Windows cp950 預設會把 UTF-8 中文替換成 U+FFFD, 強制 UTF-8 stdout/stderr
if sys.platform == "win32":
    for _stream in (sys.stdout, sys.stderr):
        if hasattr(_stream, "reconfigure"):
            _stream.reconfigure(encoding="utf-8", errors="replace")


@dataclass(frozen=True)
class GuardFileStale:
    """單一護衛檔的時間維度量化結果"""
    relpath: str
    test_fn_count: int
    last_commit_unix: int
    last_commit_iso: str
    days_since_last_commit: int
    is_stale: bool


_TEST_MARKER_RE: Final[re.Pattern[str]] = re.compile(r"^\s*#\[test\]\s*$", re.MULTILINE)


def _iter_test_files(src_root: Path) -> list[Path]:
    """遞迴找所有含 #\[test\] 的 .rs 檔, 排除 target/ build artifact"""
    out: list[Path] = []
    if not src_root.exists():
        return out
    for rs in sorted(src_root.rglob("*.rs")):
        if "target" in rs.parts:
            continue
        try:
            text = rs.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        if _TEST_MARKER_RE.search(text):
            out.append(rs)
    return out


def _git_last_commit_unix(relpath: str) -> int:
    """`git log -1 --format=%ct -- <relpath>` → unix time, 0 = no commit"""
    proc = subprocess.run(
        ["git", "log", "-1", "--format=%ct", "--", relpath],
        cwd=str(REPO_ROOT),
        capture_output=True, text=True, encoding="utf-8", errors="replace",
    )
    if proc.returncode != 0 or not proc.stdout.strip():
        return 0
    try:
        return int(proc.stdout.strip())
    except ValueError:
        return 0


def _count_test_fns(rs_path: Path) -> int:
    """數檔內 #\[test\] marker 出現次數 (含 #\[test\] 後接 fn, 不含 #\[cfg(test)\])"""
    try:
        text = rs_path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return 0
    return len(_TEST_MARKER_RE.findall(text))


def _iso_from_unix(unix_ts: int) -> str:
    if unix_ts <= 0:
        return "1970-01-01T00:00:00Z"
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(unix_ts))


def measure(src_root: Path = SRC_TAURI, now_unix: int | None = None) -> list[GuardFileStale]:
    """
    量測所有護衛檔的時間維度:
      1. 找所有含 #\[test\] 的 .rs 檔
      2. 對每個拿 git log 最後 commit time
      3. 對每個數 #\[test\] 函式數
      4. 算 days_since_last_commit 對比 STALE_DAYS

    now_unix: 測試用注入點, 預設 time.time()
    """
    if now_unix is None:
        now_unix = int(time.time())
    results: list[GuardFileStale] = []
    for rs in _iter_test_files(src_root):
        rel = str(rs.relative_to(REPO_ROOT)).replace("\\", "/")
        commit_unix = _git_last_commit_unix(rel)
        test_count = _count_test_fns(rs)
        days = (now_unix - commit_unix) // 86400 if commit_unix > 0 else 10**9
        results.append(GuardFileStale(
            relpath=rel,
            test_fn_count=test_count,
            last_commit_unix=commit_unix,
            last_commit_iso=_iso_from_unix(commit_unix),
            days_since_last_commit=int(days),
            is_stale=days > STALE_DAYS,
        ))
    return results


def render_table(results: list[GuardFileStale]) -> str:
    """stdout 表格: relpath | test_fn | last_commit | days | stale"""
    header = f"{'relpath':<48} {'tests':>6} {'last_commit':<22} {'days':>5}  stale"
    rows = [header, "-" * len(header)]
    total_tests = 0
    stale_count = 0
    for r in sorted(results, key=lambda x: -x.days_since_last_commit):
        rows.append(
            f"{r.relpath:<48} {r.test_fn_count:>6} {r.last_commit_iso:<22} "
            f"{r.days_since_last_commit:>5}  {'YES' if r.is_stale else 'no'}"
        )
        total_tests += r.test_fn_count
        if r.is_stale:
            stale_count += 1
    rows.append("-" * len(header))
    rows.append(
        f"{'TOTAL':<48} {total_tests:>6}    "
        f"stale={stale_count}/{len(results)}    threshold={STALE_DAYS}d"
    )
    return "\n".join(rows)


def overall_pass(results: list[GuardFileStale], chain_count_min: int = CHAIN_COUNT_MIN) -> bool:
    """
    過期審計 fail-closed 條件:
      1. 護衛鏈計數 < chain_count_min (R97 紅線健康檢查, 跟 r124_sentinel 對齊)
      2. 任何護衛檔 is_stale=True (超 STALE_DAYS 沒更新)
    """
    total = sum(r.test_fn_count for r in results)
    if total < chain_count_min:
        return False
    return not any(r.is_stale for r in results)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="護衛鏈過期契約審計 (時間維度)")
    parser.add_argument("--json", type=Path, default=OUTPUT_JSON,
                        help=f"輸出 JSON 路徑 (預設 {OUTPUT_JSON.name})")
    parser.add_argument("--src-root", type=Path, default=SRC_TAURI,
                        help="遞迴掃的 src root (預設 src-tauri/src)")
    parser.add_argument("--stale-days", type=int, default=STALE_DAYS,
                        help=f"過期閾值天數 (預設 {STALE_DAYS})")
    args = parser.parse_args(argv)

    results = measure(src_root=args.src_root)
    passed = overall_pass(results, chain_count_min=CHAIN_COUNT_MIN)
    table = render_table(results)

    print("護衛鏈過期契約審計 (R172 補時間維度, 不取代 r124_sentinel 計數護衛)")
    print(f"  threshold: {args.stale_days} days  |  chain_count_min: {CHAIN_COUNT_MIN}  "
          f"|  files: {len(results)}")
    print(table)

    payload = {
        "timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "stale_days_threshold": args.stale_days,
        "chain_count_min": CHAIN_COUNT_MIN,
        "file_count": len(results),
        "total_test_fn": sum(r.test_fn_count for r in results),
        "stale_count": sum(1 for r in results if r.is_stale),
        "overall_pass": passed,
        "files": [asdict(r) for r in results],
    }
    args.json.write_text(json.dumps(payload, indent=2, ensure_ascii=False), encoding="utf-8")
    print(f"\nJSON: {args.json}")
    print(f"Verdict: {'PASS' if passed else 'FAIL'}")
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
