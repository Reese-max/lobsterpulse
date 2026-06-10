#!/usr/bin/env python3
"""
K0 target baseline 守衛 pytest — 守 4 個 M0 級 hidden gap + 1 個 R13 防護

R197 落地。對應 k0_target_baseline_check.py 5 維度守護, 鏡像 R132
k0_drift_check.py 5 case + R187 k0_measure 9 case 模式, Python script
不算 Rust 護衛, chain 20→20 守住, R97 紅線不破。

5 case 對應 4 個 M0 級 hidden gap + 1 個 R13 防護:

  1. test_持平_對齊_R182_Path_A_5_維度全_PASS — 守 baseline 量化口徑不漂
  2. test_KNOWN_PROVIDERS_結構_改壞_觸發_REGRESS — 守 M0 級 hidden gap 1
     (LOCAL_CLI 數量被改寬 / OPENAB_BOT 數量被改少 → KNOWN_PROVIDERS 總數
     悄悄失真, K0 量化值失真, R131 結構性確認失效)
  3. test_4_missing_bot_從_OPENAB_BOT_移除_觸發_REGRESS — 守 M0 級 hidden gap 2
     (把 irisx_bot/grokx/lpbot/mimo 從 OPENAB_BOT 拿掉 → 結構性確認失效,
     R182 決議「永久非本機 scope」不攻自破)
  4. test_LOCAL_CLI_被_誤改_觸發_REGRESS — 守 M0 級 hidden gap 3
     (把 LOCAL_CLI 改壞 e.g. 拿掉 gemini → 本機 4/13 降為 3/13, K0 量化
     倒退, 結構性決議受損)
  5. test_MISSION_缺_R182_補欄_標記_觸發_REGRESS — 守 M0 級 hidden gap 4
     (有人把 MISSION.md R182 補欄拿掉, 4 missing 永久非 scope 標記消失,
     K0 量化口徑悄悄漂回 13/13 不可達目標, R182 決議不攻自破) +
     test_腳本源缺_回退碼_2 — 守 R13 防護 (k0_measure.py 找不到 drift check crash)
"""
import os
import re
import sys
from pathlib import Path

import pytest

SCRIPT_DIR = Path(__file__).resolve().parent
SCRIPT = SCRIPT_DIR / "k0_target_baseline_check.py"
REPO_ROOT = SCRIPT_DIR.parent
K0_MEASURE = REPO_ROOT / "scripts" / "k0_measure.py"
MISSION_MD = REPO_ROOT / "MISSION.md"
sys.path.insert(0, str(SCRIPT_DIR))

import k0_target_baseline_check as k0t  # noqa: E402


# ---------- 1. 持平 baseline ----------

def test_持平_對齊_R182_Path_A_5_維度全_PASS():
    """守 R182 結構性降級決議 5 維度量化口徑不漂
    (4+5+4 permanent skip, k0_measure KNOWN_PROVIDERS 結構對齊, MISSION R182 補欄在位)
    """
    code, results = k0t.run_check()
    statuses = [r.status for r in results]
    assert code == 0, f"R182 Path A 守護 FAIL: code={code} statuses={statuses}"
    assert statuses == ["PASS"] * 5, f"5 維度全 PASS 預期, 實際: {statuses}"


# ---------- 2. KNOWN_PROVIDERS 結構改壞 (M0 hidden gap 1) ----------

def test_KNOWN_PROVIDERS_結構_改壞_觸發_REGRESS(tmp_path, monkeypatch):
    """守 M0 級 hidden gap 1: LOCAL_CLI 數量被改寬 / OPENAB_BOT 被改少
    → KNOWN_PROVIDERS 總數失真, K0 量化值悄悄漂
    """
    # 模擬 k0_measure.py 結構被改壞: LOCAL_CLI 從 4 變 3 (拿掉 gemini)
    bad_k0 = '''LOCAL_CLI = ["claude", "codex", "copilot"]
OPENAB_BOT = ["cicx", "gitx", "giminix", "codex_bot", "openx",
              "irisx_bot", "grokx", "lpbot", "mimo"]
'''
    bad_k0_path = tmp_path / "k0_measure.py"
    bad_k0_path.write_text(bad_k0, encoding="utf-8")
    # 模擬 MISSION.md 完整
    mission_src = k0t.read_file(k0t.MISSION_MD)

    # 把 K0_MEASURE path 改到 tmp_path
    monkeypatch.setattr(k0t, "K0_MEASURE", bad_k0_path)
    r = k0t.check_known_providers(bad_k0)
    assert r.status == "REGRESS", f"LOCAL_CLI 改壞應觸發 REGRESS, 實際: {r}"
    assert "3 LOCAL_CLI" in r.actual


# ---------- 3. 4 missing bot 從 OPENAB_BOT 移除 (M0 hidden gap 2) ----------

def test_4_missing_bot_從_OPENAB_BOT_移除_觸發_REGRESS(tmp_path):
    """守 M0 級 hidden gap 2: 把 irisx_bot/grokx/lpbot/mimo 從 OPENAB_BOT 拿掉
    → R131 結構性確認失效, R182 「永久非本機 scope」決議不攻自破
    """
    bad_k0 = '''LOCAL_CLI = ["claude", "codex", "copilot", "gemini"]
OPENAB_BOT = ["cicx", "gitx", "giminix", "codex_bot", "openx"]
'''
    bad_k0_path = tmp_path / "k0_measure.py"
    bad_k0_path.write_text(bad_k0, encoding="utf-8")
    r = k0t.check_missing_bot_in_openab(bad_k0_path.read_text(encoding="utf-8"))
    assert r.status == "REGRESS", f"4 missing 移除應觸發 REGRESS, 實際: {r}"
    assert "irisx_bot" in r.actual
    assert "grokx" in r.actual


# ---------- 4. LOCAL_CLI 被誤改 (M0 hidden gap 3) ----------

def test_LOCAL_CLI_被_誤改_觸發_REGRESS():
    """守 M0 級 hidden gap 3: LOCAL_CLI 改壞 (e.g. 拿掉 gemini)
    → 本機 4/13 降為 3/13, K0 量化倒退, R182 決議受損
    """
    bad_k0 = '''LOCAL_CLI = ["claude", "codex", "copilot"]
OPENAB_BOT = ["cicx", "gitx", "giminix", "codex_bot", "openx",
              "irisx_bot", "grokx", "lpbot", "mimo"]
'''
    r = k0t.check_local_cli_4(bad_k0)
    assert r.status == "REGRESS", f"LOCAL_CLI 改壞應觸發 REGRESS, 實際: {r}"


# ---------- 5. MISSION 缺 R182 標記 (M0 hidden gap 4) + R13 腳本源缺防護 ----------

def test_MISSION_缺_R182_補欄_標記_觸發_REGRESS():
    """守 M0 級 hidden gap 4: 有人把 MISSION.md R182 補欄拿掉
    → 4 missing 永久非 scope 標記消失, K0 量化口徑悄悄漂回 13/13 不可達
    """
    bad_mission = "# LobsterPulse\n\n## 90 天成功指標\n\n| KPI | 目標 |\n|---|---|\n| K0-A1 | 13/13 |\n"
    r = k0t.check_mission_k0_target(bad_mission)
    assert r.status == "REGRESS", f"MISSION 缺 R182 標記應觸發 REGRESS, 實際: {r}"


def test_腳本源缺_回退碼_2(tmp_path, monkeypatch):
    """守 R13 防護: k0_measure.py 找不到 → drift check crash, R13 防護失守
    MISSION.md 缺 → 同樣 fail-closed
    """
    fake_k0 = tmp_path / "k0_measure.py"
    fake_mission = tmp_path / "MISSION.md"
    monkeypatch.setattr(k0t, "K0_MEASURE", fake_k0)
    monkeypatch.setattr(k0t, "MISSION_MD", fake_mission)
    code, results = k0t.run_check()
    assert code == 2, f"源檔缺應回退碼 2, 實際: {code}"
    assert any(r.status == "MISSING" for r in results), \
        f"源檔缺應至少 1 維度 MISSING, 實際: {[r.status for r in results]}"
