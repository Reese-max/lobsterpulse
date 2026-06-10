#!/usr/bin/env python3
"""
K0 target baseline 守護 — 守 R182 Path A (降級) 結構性決議不退化

R197 落地。對應 openspec/changes/mission-k0-restructure-2026-q3/proposal.md
「MCAP-2: Path A (降級) 設計草案」+ Decision Asks 段的 Path A 決議。

Path A 降級口徑 (R182 結構性決議, owner M 選 A 不選 B):
  K0-A1 emit 覆蓋: 4/13 (本機穩態下限) + 5/13 (OpenAB scope 受 cicx 等浮動)
  K0-A2 sample 覆蓋: 1/13 (claude=3 sessions 累加) + 4/13 永久非 scope
  K0 Quota: K0-B fresh 4/13 + K0-Q 9/13 (缺 4 個 = irisx_bot/grokx/lpbot/mimo
           完全不寫 usage-*.json snapshot, OpenAB 端永遠不可達, 永久非本機 scope)

守護鏈 (5 維度):
  1. k0_measure.py KNOWN_PROVIDERS 結構 = 4 LOCAL_CLI + 9 OPENAB_BOT = 13
  2. 4 missing bot (irisx_bot/grokx/lpbot/mimo) 標記為永久非 scope
  3. MISSION.md 90 天目標反映 Path A 降級 (非 13/13 不可達)
  4. 5 active OpenAB (cicx/gitx/giminix/codex_bot/openx) 不被誤降
  5. 4 LOCAL_CLI (claude/codex/copilot/gemini) 不可被降為「永久非 scope」

鏡像 R132 k0_drift_check.py + R187 k0_measure.py 護衛模式: 1 個 Python
script + 1 個 pytest 護衛模組, BASELINE 寫死常數 + DriftResult NamedTuple
+ render_report table + 退出碼 0/1/2 fail-closed (R13 防護)。

不破 R97 紅線: 走 Python pytest 護衛維度, 不新增 Rust 護衛 mod, chain
20→20 守住。
"""
import json
import re
import sys
from pathlib import Path
from typing import NamedTuple, Tuple

# 4 missing bot (R131 結構性確認 0 spec drift, R182 升級為永久非本機 scope)
MISSING_BOT = frozenset(["irisx_bot", "grokx", "lpbot", "mimo"])
# 5 active OpenAB bot (cicx 等屬 OpenAB scope 浮動, 非永久 skip)
ACTIVE_OPENAB = frozenset(["cicx", "gitx", "giminix", "codex_bot", "openx"])
# 4 LOCAL_CLI (本機端永遠可達, 不可被誤降為永久非 scope)
LOCAL_CLI = frozenset(["claude", "codex", "copilot", "gemini"])

# 對齊 k0_measure.py KNOWN_PROVIDERS source of truth
EXPECTED_PROVIDER_COUNT = 13  # 4 LOCAL_CLI + 9 OPENAB_BOT

# Path A 90 天量化目標 (R182 結構性決議, 寫死 BASELINE 防悄悄漂回 13/13)
EXPECTED_K0A1_TARGET = "4/13"  # 本機穩態下限
EXPECTED_K0A2_TARGET = "1/13"  # claude=3 sessions 累加現況
EXPECTED_K0Q_TARGET = "9/13"   # 4 missing permanent skip
EXPECTED_K0B_TARGET = "4/13"   # 本機 fresh 4/13

REPO_ROOT = Path(__file__).resolve().parent.parent
K0_MEASURE = REPO_ROOT / "scripts" / "k0_measure.py"
MISSION_MD = REPO_ROOT / "MISSION.md"


class DriftResult(NamedTuple):
    name: str
    expected: str
    actual: str
    status: str  # PASS / REGRESS / MISSING


def read_file(path: Path) -> str:
    if not path.exists():
        return ""
    return path.read_text(encoding="utf-8", errors="replace")


def check_known_providers(k0_src: str) -> DriftResult:
    """維度 1: k0_measure.py KNOWN_PROVIDERS 結構 = 4 LOCAL_CLI + 9 OPENAB_BOT = 13

    對齊 hook_server.rs:323-340 source of truth, 守住 R131 結構性確認不漂。
    """
    if not k0_src:
        return DriftResult("KNOWN_PROVIDERS_結構", f"{EXPECTED_PROVIDER_COUNT}",
                           "(missing)", "MISSING")
    # 抓 LOCAL_CLI 列表
    m_local = re.search(r'LOCAL_CLI\s*=\s*\[(.*?)\]', k0_src, re.DOTALL)
    # 抓 OPENAB_BOT 列表
    m_openab = re.search(r'OPENAB_BOT\s*=\s*\[(.*?)\]', k0_src, re.DOTALL)
    if not m_local or not m_openab:
        return DriftResult("KNOWN_PROVIDERS_結構", f"{EXPECTED_PROVIDER_COUNT}",
                           "(unparseable)", "REGRESS")
    local_count = len(re.findall(r'"([^"]+)"', m_local.group(1)))
    openab_count = len(re.findall(r'"([^"]+)"', m_openab.group(1)))
    total = local_count + openab_count
    if total == EXPECTED_PROVIDER_COUNT and local_count == 4 and openab_count == 9:
        return DriftResult("KNOWN_PROVIDERS_結構",
                           f"4 LOCAL_CLI + 9 OPENAB_BOT = {EXPECTED_PROVIDER_COUNT}",
                           f"{local_count} LOCAL_CLI + {openab_count} OPENAB_BOT = {total}",
                           "PASS")
    return DriftResult("KNOWN_PROVIDERS_結構",
                       f"4 LOCAL_CLI + 9 OPENAB_BOT = {EXPECTED_PROVIDER_COUNT}",
                       f"{local_count} LOCAL_CLI + {openab_count} OPENAB_BOT = {total}",
                       "REGRESS")


def check_missing_bot_in_openab(k0_src: str) -> DriftResult:
    """維度 2: 4 missing bot 必須在 OPENAB_BOT 列表內 (永久非本機 scope)"""
    if not k0_src:
        return DriftResult("4_missing_bot_永久非_scope", "in OPENAB_BOT",
                           "(missing)", "MISSING")
    m = re.search(r'OPENAB_BOT\s*=\s*\[(.*?)\]', k0_src, re.DOTALL)
    if not m:
        return DriftResult("4_missing_bot_永久非_scope", "in OPENAB_BOT",
                           "(unparseable)", "REGRESS")
    listed = set(re.findall(r'"([^"]+)"', m.group(1)))
    missing_in_list = MISSING_BOT - listed
    if not missing_in_list:
        return DriftResult("4_missing_bot_永久非_scope",
                           f"{sorted(MISSING_BOT)} in OPENAB_BOT",
                           f"{sorted(MISSING_BOT & listed)} in OPENAB_BOT",
                           "PASS")
    return DriftResult("4_missing_bot_永久非_scope",
                       f"all {len(MISSING_BOT)} in OPENAB_BOT",
                       f"missing from OPENAB_BOT: {sorted(missing_in_list)}",
                       "REGRESS")


def check_active_openab_5(k0_src: str) -> DriftResult:
    """維度 3: 5 active OpenAB bot 不可被誤降 (Path A 不刪 5 個活的)"""
    if not k0_src:
        return DriftResult("5_active_OpenAB_不退化", "all 5 in OPENAB_BOT",
                           "(missing)", "MISSING")
    m = re.search(r'OPENAB_BOT\s*=\s*\[(.*?)\]', k0_src, re.DOTALL)
    if not m:
        return DriftResult("5_active_OpenAB_不退化", "all 5 in OPENAB_BOT",
                           "(unparseable)", "REGRESS")
    listed = set(re.findall(r'"([^"]+)"', m.group(1)))
    missing_in_list = ACTIVE_OPENAB - listed
    if not missing_in_list:
        return DriftResult("5_active_OpenAB_不退化",
                           f"all 5 in OPENAB_BOT",
                           f"{sorted(ACTIVE_OPENAB & listed)} in OPENAB_BOT",
                           "PASS")
    return DriftResult("5_active_OpenAB_不退化",
                       f"all {len(ACTIVE_OPENAB)} in OPENAB_BOT",
                       f"missing from OPENAB_BOT: {sorted(missing_in_list)}",
                       "REGRESS")


def check_local_cli_4(k0_src: str) -> DriftResult:
    """維度 4: 4 LOCAL_CLI 永遠可達, 不可被降為永久非 scope"""
    if not k0_src:
        return DriftResult("4_LOCAL_CLI_不可_永久_skip", "all 4 in LOCAL_CLI",
                           "(missing)", "MISSING")
    m = re.search(r'LOCAL_CLI\s*=\s*\[(.*?)\]', k0_src, re.DOTALL)
    if not m:
        return DriftResult("4_LOCAL_CLI_不可_永久_skip", "all 4 in LOCAL_CLI",
                           "(unparseable)", "REGRESS")
    listed = set(re.findall(r'"([^"]+)"', m.group(1)))
    if listed == LOCAL_CLI:
        return DriftResult("4_LOCAL_CLI_不可_永久_skip",
                           f"{sorted(LOCAL_CLI)}",
                           f"{sorted(listed)}", "PASS")
    return DriftResult("4_LOCAL_CLI_不可_永久_skip",
                       f"{sorted(LOCAL_CLI)}",
                       f"{sorted(listed)}", "REGRESS")


def check_mission_k0_target(mission_src: str) -> DriftResult:
    """維度 5: MISSION.md 90 天目標反映 Path A 降級 (4/13+5/13+4 missing 永久非 scope)

    對應 R182 結構性決議, 守住「不悄悄改回 13/13 不可達目標」+ 「4 missing 永久非 scope」標記不退。
    """
    if not mission_src:
        return DriftResult("MISSION_90_day_target_降級", "Path A 4+5+4 skip",
                           "(missing)", "MISSING")
    # 找 R182 補欄 (Path A 決議 entry)
    r182_marker = "R182 補" in mission_src
    # 找 4 missing 永久非 scope 標記
    missing_marker = "4 missing" in mission_src or "4 個 missing" in mission_src
    # 找永久非 scope 關鍵字
    permanent_skip = "永久非" in mission_src or "永久 skip" in mission_src
    if r182_marker and missing_marker and permanent_skip:
        return DriftResult("MISSION_90_day_target_降級",
                           "R182 補欄 + 4 missing + 永久非 scope",
                           f"R182={r182_marker} missing={missing_marker} perm={permanent_skip}",
                           "PASS")
    return DriftResult("MISSION_90_day_target_降級",
                       "R182 補欄 + 4 missing + 永久非 scope",
                       f"R182={r182_marker} missing={missing_marker} perm={permanent_skip}",
                       "REGRESS")


def run_check() -> Tuple[int, list]:
    """跑 5 維度 K0 target baseline 守護, 回 (exit_code, results)"""
    k0_src = read_file(K0_MEASURE)
    mission_src = read_file(MISSION_MD)
    results = [
        check_known_providers(k0_src),
        check_missing_bot_in_openab(k0_src),
        check_active_openab_5(k0_src),
        check_local_cli_4(k0_src),
        check_mission_k0_target(mission_src),
    ]
    if any(r.status == "MISSING" for r in results):
        return 2, results  # R13 fail-closed: 源檔缺
    if any(r.status == "REGRESS" for r in results):
        return 1, results
    return 0, results


def render_report(results: list) -> str:
    """人類可讀報告 (對齊 k0_drift_check.py 格式)"""
    lines = []
    lines.append("=" * 78)
    lines.append("K0 Target Baseline Check (R182 Path A 結構性降級決議守護)")
    lines.append("=" * 78)
    lines.append(f"{'#':<3} {'維度':<35} {'狀態':<10}")
    lines.append("-" * 78)
    for i, r in enumerate(results, 1):
        lines.append(f"{i:<3} {r.name:<35} {r.status:<10}")
        lines.append(f"    期望: {r.expected}")
        lines.append(f"    實際: {r.actual}")
    lines.append("=" * 78)
    pass_n = sum(1 for r in results if r.status == "PASS")
    regress_n = sum(1 for r in results if r.status == "REGRESS")
    missing_n = sum(1 for r in results if r.status == "MISSING")
    lines.append(f"PASS={pass_n} REGRESS={regress_n} MISSING={missing_n} / 總 5 維度")
    return "\n".join(lines)


def main() -> int:
    code, results = run_check()
    print(render_report(results))
    if code != 0:
        print(f"\n[FAIL] 退出碼 {code} (1=REGRESS 漂回 13/13 不可達 / 2=源檔缺)")
    else:
        print("\n[OK] R182 Path A 結構性降級決議守住")
    return code


if __name__ == "__main__":
    sys.exit(main())
