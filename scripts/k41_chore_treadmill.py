#!/usr/bin/env python3
"""
K41 KPI 量測腳本 — 量化 7 日 chore_treadmill 比例

對齊 MISSION.md 90 天 KPI:
  K41 chore_treadmill 紅線: 24h 55% 觸發 → <30% 持續 7 日

「7 日平均」解讀為「最近 7 日內 commit 中 chore / refactor / archive / sensor
的佔比」單一時間點量測。連續多點 <30% 為達標條件, 由 owner/排程每週跑累積
判斷, 本腳本只負責「把未量測變可量測」(對齊 k0_measure.py R83 同樣定位)。

為什麼把 refactor/archive/sensor 也算進 chore: 三者皆為「治理批」, 無 KPI
直接推進貢獻, 累積過高 = 治理 treadmill 失控 (策略顧問 R80 警告)。

輸出:
  - 人類可讀表 (stdout)
  - machine-readable JSON (.harness-k41.json)
  - 退出碼 0 (達標 <30%) / 1 (漂移 ≥30%)

R107 落地。對齊 k0_measure.py 風格 (純 stdlib, 不引依賴)。
"""
import json
import subprocess
import sys
from datetime import datetime, timedelta, timezone
from pathlib import Path

# 對齊 git 規範: chore / refactor / archive / sensor 為治理批, 視為
# chore_treadmill 分子。 feat / fix / docs / test / perf / ci / style 為
# 任務型 commit, 視為分母 (前值: MISSION.md R81 補頁定義)。
GOVERNANCE_PREFIXES = ("chore", "refactor", "archive", "sensor")

WINDOW_DAYS = 7
THRESHOLD = 0.30  # MISSION 90 天 KPI 上限


def git_log_subjects(window_days: int) -> list[str]:
    """回傳最近 window_days 內的 commit subject 清單 (新→舊)。

    Windows cp950 解碼陷阱: 走 bytes → utf-8 decode (errors=replace) 避雷,
    不走 text=True (subprocess 預設 cp950, commit 含中文會炸)。
    對齊 k0_measure.py 同樣在 Windows 環境跑的風格 (它走 urllib 而非 git,
    沒踩到這雷; 本腳本是 scripts/ 第一個吃 git 輸出的, 留下避雷註記)。
    """
    since = (datetime.now(timezone.utc) - timedelta(days=window_days)).isoformat()
    raw = subprocess.run(
        ["git", "log", f"--since={since}", "--pretty=format:%s"],
        capture_output=True,
        check=True,
        cwd=Path(__file__).resolve().parent.parent,
    ).stdout
    return [s for s in raw.decode("utf-8", errors="replace").splitlines() if s]


def measure(window_days: int = WINDOW_DAYS) -> tuple[int, int, list[str]]:
    subjects = git_log_subjects(window_days)
    chore = [s for s in subjects if s.split(":", 1)[0] in GOVERNANCE_PREFIXES]
    return len(chore), len(subjects), chore


def main() -> int:
    chore_n, total, chore_list = measure()
    if total == 0:
        print(f"K41 chore_treadmill: 0 commits in last {WINDOW_DAYS}d (baseline reset)")
        return 0
    ratio = chore_n / total
    status = "OK" if ratio < THRESHOLD else "VIOLATED"
    print(
        f"K41 chore_treadmill ({WINDOW_DAYS}d): {chore_n}/{total} = "
        f"{ratio:.1%} (threshold <{THRESHOLD:.0%}) [{status}]"
    )
    if chore_list:
        for s in chore_list:
            print(f"  - {s}")
    payload = {
        "window_days": WINDOW_DAYS,
        "threshold": THRESHOLD,
        "chore_count": chore_n,
        "total_count": total,
        "ratio": ratio,
        "status": status,
        "chore_subjects": chore_list,
        "ts": datetime.now(timezone.utc).isoformat(),
    }
    out_path = Path(__file__).resolve().parent.parent / ".harness-k41.json"
    out_path.write_text(json.dumps(payload, indent=2), encoding="utf-8")
    print(f"JSON 寫入: {out_path}")
    return 0 if status == "OK" else 1


if __name__ == "__main__":
    sys.exit(main())
