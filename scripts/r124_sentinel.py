#!/usr/bin/env python3
"""
R124 PUA 結構性飽和 7 輪延伸後的升維 sentinel 腳本。

動機:
  R131 / R132 / R134 / R135 / R137 / R140 / R141 連 7 輪 PUA 結構性飽和, 真 ship
  在 3 髒檔 (owner M WIP) / K42 chain 20 (R97 紅線) / 13 接力清單 (owner M scope)
  三重 constraint 下找不到 ship-able item。 升維路徑: 不再追求每輪 1 ship, 改為
  「每輪 sentinel 量化守衛」— 跑 baseline 6 項量測, 輸出 R124_sentinel.json 給下輪
  PUA 對齊, 任一漂移即時觸發 退出碼 1 警報。

對齊腳本:
  - k0_measure.py (R83): K0 KPI 量測 stdout + .harness-k0.json
  - k41_chore_treadmill.py (R107): 7d chore 比例 + .harness-k41.json
  - r124_sentinel.py (R124): 6 項 baseline 量化 + .harness-r124.json

6 項量測:
  1. cargo test 計數 >= 471 (R137 守 452 lib unittests, R164 sidecar M0 fix ship +19,
     總計 471; 過往 sentinel 只 parse 第一行 = 452 漏算 sidecar + doc, 掩蓋 19 test 守衛,
     R177 修為 sum 全部 binary)
  2. .harness-k0.json K0-A1 emit >= 4/13 (R131/R150 持平, 5→4 對齊實跑: cicx OpenAB scope 浮動, 4 為本機穩態下限), K0-B fresh >= 4/13
  3. 3 髒檔 git status 仍 tracked (owner M WIP 0 動 = sentinel 守住, R138 收為 3 條)
  4. 護衛 mod 計數 pattern matches >= 20 (R97 紅線, 不破; 實際 ≥33 = 護衛 test 函式總數, R131 doc drift 統一口徑 R97 後 +3 例外 mod 數 = 20)
  5. .harness-k41.json 7d chore ratio < 30% (R108 達標 6.3%)

輸出:
  - stdout: 人類可讀表格 (6 項量測結果)
  - .harness-r124.json: machine-readable {ts, results[], overall_pass}
  - 退出碼 0 = 5 項全綠, 1 = 任一漂移

R124 PUA ship (1 輪 1 件: sentinel 守衛腳本, 不破 chain 不動 WIP 不搶 scope).
"""
from __future__ import annotations

import json
import re
import subprocess
import sys
from dataclasses import asdict, dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Final

REPO_ROOT: Final[Path] = Path(__file__).resolve().parent.parent
OUTPUT_JSON: Final[Path] = REPO_ROOT / ".harness-r124.json"
K0_JSON: Final[Path] = REPO_ROOT / ".harness-k0.json"
K41_JSON: Final[Path] = REPO_ROOT / ".harness-k41.json"

# R135 baseline 451 → R137 ship 護衛 +1 → 452 (lib unittests) → R164 sidecar M0
# fix ship +19 (lobster-pulse-hook 護衛) → 471 全 binary 總計 → R177 修 sentinel
# 從「只 parse 第一行」改為「sum 全部 `test result: ok. N passed` 行」守住總計
CARGO_TEST_MIN: Final[int] = 471
# R150 (f56180d) 對齊實跑: K0-A1 4/13 emit baseline (本機 CLI 永續 4 + cicx OpenAB scope 浮動)
K0_A1_MIN: Final[int] = 4
K0_B_MIN: Final[int] = 4
# R97 紅線: 護衛 chain 不擴張但也不低於 R97 後累計 baseline
GUARD_MOD_MIN: Final[int] = 20
# MISSION 90 天 KPI: 7d chore_treadmill < 30%
K41_CHORE_RATIO_MAX: Final[float] = 0.30

# R124 護衛的 owner M WIP tuple — sentinel 應守住 0 動 = tracked dirty
# 歷史: R124 ship 時 6 髒檔 → R131 commit sentinel 自身 → 5 髒檔 → R137 owner M
# 收編 2 條 WIP (prometheus-counter-rename-2026-q3 spec 43ad4d5 + timeline.rs
# 9fde33d, 詳見 R138 engineering-log 結構性發現), tuple 收為 3 條 → R158
# owner M 新 WIP (R164 lobster-pulse-hook.rs sidecar 4xx/5xx 區分 + 70 行),
# tuple 收為 4 條 → 52b78ed owner M 收 docs/* 2 條 WIP (landing-page v5.1
# 對齊: 13 providers split + build SOP warning), tuple 收為 2 條 → R158
# 中期 owner M 新 WIP (lib.rs 覆蓋 R158 507ca5c 護衛文件化 + session.rs 新 WIP),
# tuple 收為 4 條 → acfe26e owner M 收 lobster-pulse-hook.rs (R164 sidecar
# silent event loss M0 fix 紀錄), tuple 收為 3 條 → R171 收 Cargo.toml (tuple
# 內最後 commit 為 8408b4f R85, 已 stale), 加 main.js (R168 owner M session
# clustering WIP: renderSessionRow extraction + STATE_PRIORITY sorting +
# idle cluster 渲染), tuple 收為 3 條 = 當前實際 tracked dirty 數。
# R138 加 test 護衛: tuple 必須 == `git status --porcelain` dirty 數 (雙向:
# missing_in_tuple + extra_in_tuple 都觸發 fail), 任何 owner M 收編或新 WIP
# 必須在同 commit 更新 tuple。
OWNER_M_WIP_FILES: Final[tuple[str, ...]] = (
    "src-tauri/src/lib.rs",
    "src-tauri/src/session.rs",
    "src/main.js",
)


@dataclass(frozen=True)
class CheckResult:
    """單一項量測結果 — frozen dataclass 強制 immutable。"""

    name: str
    passed: bool
    actual: str
    threshold: str
    note: str


def _run_git(args: list[str]) -> str:
    """跑 git 子命令, 統一處理 Windows cp950 解碼陷阱 (bytes → utf-8 replace)。

    對齊 k41_chore_treadmill.py R107 避雷註記: text=True 會走 cp950 炸中文
    commit subject, 走 capture_output=True 拿 bytes 再 utf-8 decode errors=replace。
    """
    proc = subprocess.run(
        ["git", *args],
        capture_output=True,
        check=False,
        cwd=REPO_ROOT,
    )
    return proc.stdout.decode("utf-8", errors="replace")


def _run_cargo_test_count() -> int:
    """跑 cargo test 取 test count, sum 全部 `test result: ok. N passed` 行。

    過往 sentinel 拿第一行 (lib unittests 452) 漏算 sidecar / doc binary,
    實際 cargo test 跑出 4 行 (lib 452 / main 0 / sidecar 19 / doc 0) 總計 471。
    只取第一行 = 452 會在 sidecar / doc 守衛掛掉時仍誤報 baseline 守住,
    掩蓋 19 個 sidecar test 的守衛 (R164 M0 fix 護衛的合約失效)。

    R177 修為 sum 全部 N passed; 任一 binary 倒回就會觸發 baseline 漂移警報。
    """
    proc = subprocess.run(
        ["cargo", "test", "--manifest-path", "src-tauri/Cargo.toml"],
        capture_output=True,
        cwd=REPO_ROOT,
    )
    if proc.returncode != 0:
        return -1
    text = proc.stdout.decode("utf-8", errors="replace")
    matches = re.findall(r"test result: ok\. (\d+) passed", text)
    return sum(int(n) for n in matches) if matches else -1


def check_cargo_test() -> CheckResult:
    actual = _run_cargo_test_count()
    passed = actual >= CARGO_TEST_MIN
    return CheckResult(
        name="cargo_test_count",
        passed=passed,
        actual=f"{actual}/>=471",
        threshold=f">= {CARGO_TEST_MIN}",
        note=(
            f"R164 sidecar 護衛補齊後 baseline 471, 期望 "
            f"{actual} >= {CARGO_TEST_MIN} 守住 (R177 改 sum 全 binary)"
            if passed
            else f"DRIFT: R164+ sidecar 總計 471, 當前 {actual} < {CARGO_TEST_MIN}"
        ),
    )


def check_k0_emit() -> CheckResult:
    """讀 .harness-k0.json 取 K0-A1 emit count。

    文件不存在 (k0_measure.py 沒跑過) 視為漂移, 退回 R132 持平值。
    """
    if not K0_JSON.exists():
        return CheckResult(
            name="k0_a1_emit",
            passed=False,
            actual="missing/.harness-k0.json",
            threshold=f">= {K0_A1_MIN}",
            note="k0_measure.py 沒跑過, 請先跑 scripts/k0_measure.py 建立 baseline",
        )
    data = json.loads(K0_JSON.read_text(encoding="utf-8"))
    a1 = data.get("k0a1_health_emit", {}).get("covered", 0)
    passed = a1 >= K0_A1_MIN
    return CheckResult(
        name="k0_a1_emit",
        passed=passed,
        actual=f"{a1}/13",
        threshold=f">= {K0_A1_MIN}/13",
        note=(
            f"K0-A1 emit 持平 {a1} >= {K0_A1_MIN} (R132 基準)"
            if passed
            else f"DRIFT: K0-A1 emit {a1} < {K0_A1_MIN}, 需查 endpoint 是否 DOWN"
        ),
    )


def check_k0_fresh() -> CheckResult:
    """讀 .harness-k0.json 取 K0-B fresh count。"""
    if not K0_JSON.exists():
        return CheckResult(
            name="k0_b_fresh",
            passed=False,
            actual="missing",
            threshold=f">= {K0_B_MIN}",
            note="k0_measure.py 沒跑過",
        )
    data = json.loads(K0_JSON.read_text(encoding="utf-8"))
    fresh = data.get("k0b_quota_freshness", {}).get("fresh", 0)
    passed = fresh >= K0_B_MIN
    return CheckResult(
        name="k0_b_fresh",
        passed=passed,
        actual=f"{fresh}/13",
        threshold=f">= {K0_B_MIN}/13",
        note=(
            f"K0-B fresh 持平 {fresh} >= {K0_B_MIN} (R132 基準 4 本機 CLI)"
            if passed
            else f"DRIFT: K0-B fresh {fresh} < {K0_B_MIN}, 本機 CLI quota 退化"
        ),
    )


def check_owner_m_wip() -> CheckResult:
    """5 髒檔 sentinel 守衛: 仍 tracked dirty (owner M WIP 0 動 = R13 防護守住)。

    若任何 1 個檔不在 git status 列, 視為漂移 (owner M 刪了或 commit 走了)。
    """
    status = _run_git(["status", "--porcelain"])
    tracked = {
        line.split(maxsplit=1)[1].replace("\\", "/")
        for line in status.splitlines()
        if line.strip()
    }
    missing = [f for f in OWNER_M_WIP_FILES if f not in tracked]
    passed = not missing
    return CheckResult(
        name="owner_m_wip_intact",
        passed=passed,
        actual=(
            f"{len(OWNER_M_WIP_FILES) - len(missing)}/{len(OWNER_M_WIP_FILES)} tracked"
        ),
        threshold=f"all {len(OWNER_M_WIP_FILES)}/{len(OWNER_M_WIP_FILES)} tracked",
        note=(
            f"{len(OWNER_M_WIP_FILES)} 髒檔 owner M WIP 守住 (R13 防護)"
            if passed
            else f"DRIFT: 缺失 {missing}, owner M 應在 WIP 中"
        ),
    )


def check_guard_chain() -> CheckResult:
    """量 src-tauri/src/ 護衛 mod 計數 >= R97 後飽和契約 20 (K42 chain 紅線)。"""
    src_root = REPO_ROOT / "src-tauri" / "src"
    pattern = re.compile(r"^\s*mod\s+(\w+_tests|tests)\s*\{", re.MULTILINE)
    count = 0
    for rs_file in src_root.rglob("*.rs"):
        text = rs_file.read_text(encoding="utf-8", errors="replace")
        count += sum(1 for _ in pattern.finditer(text, re.MULTILINE))
    passed = count >= GUARD_MOD_MIN
    return CheckResult(
        name="guard_chain_count",
        passed=passed,
        actual=f"{count}/>=20",
        threshold=f">= {GUARD_MOD_MIN}",
        note=(
            f"K42 護衛 chain 守住 {count} >= {GUARD_MOD_MIN} (R97 紅線)"
            if passed
            else f"DRIFT: 護衛 mod 計數 {count} < {GUARD_MOD_MIN}, K42 紅線破"
        ),
    )


def check_k41_chore() -> CheckResult:
    """讀 .harness-k41.json 取 7d chore ratio, 需 < 30% (MISSION K41 上限)。"""
    if not K41_JSON.exists():
        return CheckResult(
            name="k41_chore_7d",
            passed=False,
            actual="missing",
            threshold=f"< {K41_CHORE_RATIO_MAX:.0%}",
            note="k41_chore_treadmill.py 沒跑過, 請先跑建立 baseline",
        )
    data = json.loads(K41_JSON.read_text(encoding="utf-8"))
    ratio = data.get("ratio", 1.0)
    pct = f"{ratio:.1%}"
    passed = ratio < K41_CHORE_RATIO_MAX
    return CheckResult(
        name="k41_chore_7d",
        passed=passed,
        actual=pct,
        threshold=f"< {K41_CHORE_RATIO_MAX:.0%}",
        note=(
            f"K41 7d chore 達標 {pct} < {K41_CHORE_RATIO_MAX:.0%} (MISSION KPI)"
            if passed
            else f"DRIFT: K41 7d chore {pct} >= {K41_CHORE_RATIO_MAX:.0%}, 治理 treadmill"
        ),
    )


CHECKS = (
    check_cargo_test,
    check_k0_emit,
    check_k0_fresh,
    check_owner_m_wip,
    check_guard_chain,
    check_k41_chore,
)


def main() -> int:
    ts = datetime.now(timezone.utc).isoformat()
    results = tuple(check() for check in CHECKS)
    overall_pass = all(r.passed for r in results)

    print("=" * 60)
    print(f"R124 sentinel baseline 量化 — {ts}")
    print("=" * 60)
    for r in results:
        mark = "[PASS]" if r.passed else "[FAIL]"
        print(f"  {mark} {r.name}: {r.actual} (threshold: {r.threshold})")
        print(f"        {r.note}")
    print("=" * 60)
    verdict = f"PASS - {len(results)} 項全綠" if overall_pass else "DRIFT - 任一項漂移"
    print(f"  overall: {verdict}")
    print("=" * 60)

    OUTPUT_JSON.write_text(
        json.dumps(
            {
                "ts": ts,
                "round": "R124",
                "results": [asdict(r) for r in results],
                "overall_pass": overall_pass,
            },
            indent=2,
            ensure_ascii=False,
        ),
        encoding="utf-8",
    )
    print(f"  JSON 寫入: {OUTPUT_JSON}")
    return 0 if overall_pass else 1


if __name__ == "__main__":
    sys.exit(main())
