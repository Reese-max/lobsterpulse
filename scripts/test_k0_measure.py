#!/usr/bin/env python3
"""
K0 量測護衛 — 6 case pytest, 對齊 R132/R137/R144/R172/R176 模式
(M0+M2 雙 hidden gap closure)

R185 落地。k0_measure.py 是 K0 KPI 量測的 **生產者** (寫 .harness-k0.json)，
k0_drift_check.py 是 **消費者** (R132 已有 5 case pytest 護衛)。生產者 0
pytest = 隱藏 gap：regex 改壞 / 別名改壞 / FRESH_HOURS 改壞 / STALE_MARKER
改壞 → K0 量化值悄悄漂移 → 消費者 pytest 守住 0 漂移但量的是錯的值
(meta-bug 層 hidden gap, R180 R13 防護漏洞軸延伸)。本檔補 9 case 守 6 個
hidden gap:

  1. test_KNOWN_PROVIDERS_13_個_結構性_4_本機_9_OpenAB — 4 missing bot
     結構性確認 (對齊 R131 接力 + MISSION K0 列)
  2. test_FRESH_HOURS_24_對齊_MISSION_K0_設定 — 量測常數對齊 MISSION
     KPI 設定 (FRESH_HOURS 改壞 → K0-B 量化值悄悄變)
  3. test_STALE_MARKER_正則_8位數日期_對齊_R110 — STALE_MARKER 改寬/改窄
     → 將 stale 檔當 fresh 計 (R110 修過一次, 沒 test 守就可能復發)
  4. test_parse_provider_sessions_空字串_全_0 — edge case: 端點 DOWN
     / 沒 emit 任何樣本, 對齊 K0-A2 「沒事件流過」情境
  5. test_parse_provider_emit_解析_多_metric_family — 對齊 K0-A1 emit
     維度 (R101 補的是程式碼定義 13/13, 端點實際 emit 受 OpenAB bot
     進程是否運作影響)
  6. test_scan_quota_snapshots_openx_雙_base_name_別名 — R114 修的
     關鍵 hidden gap: openx 加 usage-bot.json (legacy), 沒 test 守
     → 改壞 K0 Quota coverage 立刻 -1, 從 9/13 → 8/13 (M0 級 KPI 倒退)

M0 bug 範例: 若有人手滑刪 R114 openx alias (`base_names.append("usage-bot")`)
→ scan_quota_snapshots 永遠把 openx 計成 missing → K0 Quota coverage 9/13
→ 8/13 → k0_drift_check.py 觸發 K0-Q 倒退 1 維度 fail-closed。沒本 test
守 → 寫壞到 ship → CI 才發現 (M0 後果)。

鏡像 R176 k41_chore_treadmill.py pytest 模式, 1 個 Python pytest 模組,
不破 K42 chain 20 條飽和契約 (Python script 不算 Rust 護衛, 對齊
R132/R137/R144/R172 既 ~5 case 模式)。
"""
import os
import sys
import time
from pathlib import Path
from unittest import mock

import pytest

SCRIPT_DIR = Path(__file__).resolve().parent
SCRIPT = SCRIPT_DIR / "k0_measure.py"
sys.path.insert(0, str(SCRIPT_DIR))

import k0_measure as k0  # noqa: E402


# ---------- 1. KNOWN_PROVIDERS 結構性驗證 ----------

def test_KNOWN_PROVIDERS_13_個_結構性_4_本機_9_OpenAB():
    """對齊 R131 結構性確認: 4 本機 CLI + 9 OpenAB bot = 13

    4 missing bot (irisx_bot/grokx/lpbot/mimo) 永久非本機 scope
    (R131 量化確認, R182 mission-k0 提案重申), 本機守護 13 個
    KNOWN_PROVIDERS 集合不漂移。
    """
    assert len(k0.KNOWN_PROVIDERS) == 13
    assert len(k0.LOCAL_CLI) == 4
    assert len(k0.OPENAB_BOT) == 9
    assert set(k0.LOCAL_CLI) == {"claude", "codex", "copilot", "gemini"}
    # R78 補齊 9 個 OpenAB bot (含 R78 拆出的 grokx T-BOT11 / 新增 lpbot T-BOT12)
    assert set(k0.OPENAB_BOT) == {
        "cicx", "gitx", "giminix", "codex_bot", "openx",
        "irisx_bot", "grokx", "lpbot", "mimo",
    }
    # 無重複
    assert len(set(k0.KNOWN_PROVIDERS)) == 13
    # LOCAL_CLI + OPENAB_BOT = KNOWN_PROVIDERS (單一 source of truth)
    assert k0.KNOWN_PROVIDERS == k0.LOCAL_CLI + k0.OPENAB_BOT


# ---------- 2. FRESH_HOURS 量測常數對齊 MISSION K0 ----------

def test_FRESH_HOURS_24_對齊_MISSION_K0_設定():
    """FRESH_HOURS=24 對齊 MISSION.md K0-B Quota 即時性「<24h 算 fresh」

    守: 若有人改 FRESH_HOURS 沒同步 MISSION, 護衛 fail 並指出實際值,
    阻擋 K0-B 量測口徑漂移。
    """
    assert k0.FRESH_HOURS == 24, (
        f"FRESH_HOURS 應 = 24 (對齊 MISSION K0-B <24h 設定), 實際 {k0.FRESH_HOURS}\n"
        f"改動時須同步 MISSION.md K0-B 量化設定"
    )


# ---------- 3. STALE_MARKER 正則對齊 R110 ----------

def test_STALE_MARKER_正則_8位數日期_對齊_R110():
    """STALE_MARKER = \\.stale-\\d{8}$ 對齊 R110 修後的 8 位數日期

    守: 若有人改寬 (e.g. \\d{6}) 或改窄 (e.g. \\d{10}) → 將 stale
    檔當 fresh 計 (改寬) 或 fresh 檔被當 stale (改窄), K0-B 量化值
    立刻失真。R110 修過一次, 沒 test 守就可能復發。
    """
    pattern = k0.STALE_MARKER.pattern
    assert pattern == r"\.stale-\d{8}$", (
        f"STALE_MARKER 應 = \\.stale-\\d{{8}}$ (對齊 R110 8 位數日期), 實際 {pattern}"
    )
    # 行為驗證: 8 位數日期檔名匹配
    matched = k0.STALE_MARKER.search("usage-cicx.json.stale-20260605")
    assert matched is not None
    # 邊界: 7 位數不應匹配
    assert k0.STALE_MARKER.search("usage-cicx.json.stale-2026060") is None
    # 邊界: 9 位數不應匹配
    assert k0.STALE_MARKER.search("usage-cicx.json.stale-202606055") is None
    # 邊界: 沒前綴 .stale- 不應匹配
    assert k0.STALE_MARKER.search("usage-cicx.json") is None
    assert k0.STALE_MARKER.search("usage-cicx.json.fresh-20260605") is None


# ---------- 4. parse_provider_sessions edge case ----------

def test_parse_provider_sessions_空字串_全_0():
    """空 metrics 文字 → 全 13 provider 0.0

    對齊 K0-A2 「端點 DOWN / 沒事件流過」情境: 沒 emit 不是 fail,
    是量化值 0, 護衛守住「空字串不要當 error raise」行為。
    """
    out = k0.parse_provider_sessions("")
    assert len(out) == 13
    assert all(v == 0.0 for v in out.values())
    # 集合對齊 KNOWN_PROVIDERS
    assert set(out.keys()) == set(k0.KNOWN_PROVIDERS)


def test_parse_provider_sessions_標準_metric_解析():
    """標準 `lobsterpulse_provider_sessions{provider="X"} N` 解析

    守: regex 改壞 (e.g. 漏 {provider="X"}) → K0-A2 量化值立刻 0,
    k0_drift_check.py 觸發 K0-A2 倒退 fail-closed。
    """
    metrics_text = """
# HELP lobsterpulse_provider_sessions Active sessions
# TYPE lobsterpulse_provider_sessions gauge
lobsterpulse_provider_sessions{provider="claude"} 3
lobsterpulse_provider_sessions{provider="codex"} 1
lobsterpulse_provider_sessions{provider="cicx"} 0
lobsterpulse_provider_sessions{provider="unknown_bot"} 99
"""
    out = k0.parse_provider_sessions(metrics_text)
    assert out["claude"] == 3.0
    assert out["codex"] == 1.0
    assert out["cicx"] == 0.0
    # 未知 provider 不污染 13 個 KNOWN_PROVIDERS
    assert "unknown_bot" not in out
    assert out["copilot"] == 0.0  # 沒出現 → 0


# ---------- 5. parse_provider_emit edge case ----------

def test_parse_provider_emit_解析_多_metric_family():
    """K0-A1 emit 維度: 解析任何 lobsterpulse_provider_*{provider="X"}

    對齊 K0-A1 「/metrics 端點實際 emit 過 provider 樣本」維度
    (R101 補的是程式碼定義 13/13, 端點實際 emit 受 OpenAB bot 是否
    在運作影響, 端點 emit 過就算覆蓋)。
    """
    metrics_text = """
# TYPE lobsterpulse_provider_sessions gauge
lobsterpulse_provider_sessions{provider="claude"} 3
# TYPE lobsterpulse_provider_active gauge
lobsterpulse_provider_active{provider="claude"} 1
# TYPE lobsterpulse_provider_tokens_input_total counter
lobsterpulse_provider_tokens_input_total{provider="cicx"} 1234
"""
    out = k0.parse_provider_emit(metrics_text)
    # 3 個不同 provider label 都算 emit 過
    assert "claude" in out
    assert "cicx" in out
    # 3 個 metric family 同 provider label 只算 1 次 (set 語意)
    assert out == {"claude", "cicx"}


def test_parse_provider_emit_空字串_空_set():
    """空 metrics 文字 → 空 set

    對齊 K0-A1 「端點 DOWN / 沒 emit 任何樣本」情境。
    """
    assert k0.parse_provider_emit("") == set()


# ---------- 6. scan_quota_snapshots openx alias 護衛 ----------

def test_scan_quota_snapshots_openx_雙_base_name_別名(tmp_path, monkeypatch):
    """R114 修: openx 加 usage-bot.json (legacy) 雙 base name 別名

    守: 若有人手滑刪 `base_names.append("usage-bot")` → openx 永遠
    計成 missing → K0 Quota coverage 9/13 → 8/13 倒退 1 維度,
    k0_drift_check.py 觸發 K0-Q 倒退 fail-closed (M0 級 KPI 倒退)。

    對齊 R114 護衛 (K42 chain 17→17 不擴張守住), 走既「Python
    pytest 護衛」維度不破飽和契約。
    """
    # 把 QUOTA_DIR env 指向 tmp (避免動到 ~/.lobsterpulse 真實目錄)
    monkeypatch.setattr(k0, "QUOTA_DIR", tmp_path)

    # 模擬 OpenAB legacy: 寫 usage-bot.json (BackendType::Other 走這名)
    legacy = tmp_path / "usage-bot.json"
    legacy.write_text("{}", encoding="utf-8")
    # mtime 設為 1 小時前 (fresh)
    fresh_mtime = time.time() - 3600
    os.utime(legacy, (fresh_mtime, fresh_mtime))

    out = k0.scan_quota_snapshots()

    # 關鍵: openx 必須被認成 fresh (不是 missing)
    assert "openx" in out, f"openx 不應 missing, 實際 out keys: {list(out.keys())}"
    assert out["openx"]["state"] == "fresh", (
        f"openx 應 = fresh (走 usage-bot.json legacy 別名), "
        f"實際 state={out['openx']['state']}, path={out['openx']['path']}"
    )
    # 本機 4 個沒 usage-local.json → missing (沒寫)
    assert out["claude"]["state"] == "missing"
    assert out["codex"]["state"] == "missing"
    # 其他 8 個 OpenAB bot 沒對應檔 → missing
    assert out["cicx"]["state"] == "missing"
    assert out["gitx"]["state"] == "missing"


def test_scan_quota_snapshots_STALE_MARKER_分流(tmp_path, monkeypatch):
    """STALE_MARKER 檔名分流: usage-{bot}.json vs usage-{bot}.json.stale-YYYYMMDD

    守: 若主檔 (fresh) 跟 stale-日期檔同時存在, 只認 fresh 為 fresh,
    過期的才進 stale bucket。R110 修過一次 (移除 hardcoded `.stale-`
    死碼), 沒 test 守就可能復發。
    """
    monkeypatch.setattr(k0, "QUOTA_DIR", tmp_path)

    # 同 bot 寫 2 個檔: 1 個 fresh (主檔) + 1 個 stale (8 位數日期後綴)
    fresh_file = tmp_path / "usage-cicx.json"
    fresh_file.write_text("{}", encoding="utf-8")
    fresh_mtime = time.time() - 3600
    os.utime(fresh_file, (fresh_mtime, fresh_mtime))

    stale_file = tmp_path / "usage-cicx.json.stale-20260501"
    stale_file.write_text("{}", encoding="utf-8")
    # stale 檔 mtime 設 30 天前, 對齊 STALE_MARKER 行為
    stale_mtime = time.time() - 30 * 24 * 3600
    os.utime(stale_file, (stale_mtime, stale_mtime))

    out = k0.scan_quota_snapshots()

    # 有 fresh 主檔 → state=fresh (不該被 stale 檔污染)
    assert out["cicx"]["state"] == "fresh", (
        f"cicx 應 = fresh (有主檔), 實際 state={out['cicx']['state']}, "
        f"path={out['cicx']['path']}"
    )


# ---------- 7. 本機 4 CLI 共用 usage-local.json 邏輯守護 ----------

def test_scan_quota_snapshots_本機_4_CLI_共用_usage_local_json(tmp_path, monkeypatch):
    """R108 量測事實守護: 4 本機 CLI 共用 1 個 usage-local.json (R85 設計)

    守: 若有人改成本機 4 CLI 各讀 `usage-{cli}.json` 或新增 split 邏輯 →
    4 個本機 CLI 全部 missing (OpenAB hook 端只寫 usage-local.json) →
    K0-B fresh 從 4/13 立刻 0/13 (M0 級 KPI 倒退, R108 量化守住 4/13 為
    本機穩態下限)。R132 接力清源 + R176 補 K41 護衛時同步確認此口徑。
    """
    monkeypatch.setattr(k0, "QUOTA_DIR", tmp_path)

    # 模擬 R108 量測事實: 只有 1 個 usage-local.json, 4 本機 CLI 共用
    local = tmp_path / "usage-local.json"
    local.write_text("{}", encoding="utf-8")
    fresh_mtime = time.time() - 3600  # 1 小時前, fresh
    os.utime(local, (fresh_mtime, fresh_mtime))

    out = k0.scan_quota_snapshots()

    # 4 本機 CLI 全部 fresh, path 都指 usage-local.json
    for p in k0.LOCAL_CLI:
        assert out[p]["state"] == "fresh", (
            f"{p} 應 = fresh (走 usage-local.json 共用), "
            f"實際 state={out[p]['state']}, path={out[p]['path']}"
        )
        assert out[p]["path"] == str(local), (
            f"{p} path 應 = usage-local.json, 實際 {out[p]['path']}"
        )
    # OpenAB 9 個全 missing (空目錄)
    for p in k0.OPENAB_BOT:
        assert out[p]["state"] == "missing", (
            f"{p} 應 = missing (空目錄), 實際 {out[p]['state']}"
        )


# ---------- 8. QUOTA_DIR 不存在時全 13 provider no_dir 守護 ----------

def test_scan_quota_snapshots_QUOTA_DIR_不存在_全_13_no_dir(tmp_path, monkeypatch):
    """QUOTA_DIR 不存在時 (e.g. 全新裝機 + 還沒建目錄) 13 provider 全 no_dir

    守: 若有人改 QUOTA_DIR 處理邏輯 (e.g. raise FileNotFoundError, 或
    部分回 missing) → K0 Quota coverage 計算會爆 / 報表 crash /
    k0_drift_check 觸發假 fail。對齊 k0_measure.py:107-109 既有 `no_dir`
    設計 (回傳 13 個全 no_dir 字典)。
    """
    # 指向不存在的子目錄
    nonexistent = tmp_path / "does_not_exist"
    assert not nonexistent.exists()
    monkeypatch.setattr(k0, "QUOTA_DIR", nonexistent)

    out = k0.scan_quota_snapshots()

    # 全 13 個都應 = no_dir (path=None, mtime_age_hours=None)
    assert len(out) == 13
    for p in k0.KNOWN_PROVIDERS:
        assert out[p]["state"] == "no_dir", (
            f"{p} 在 QUOTA_DIR 不存在時應 = no_dir, 實際 {out[p]['state']}"
        )
        assert out[p]["path"] is None
        assert out[p]["mtime_age_hours"] is None


# ---------- 9. OpenAB 4 missing bot 結構性 missing 守護 ----------

def test_scan_quota_snapshots_OpenAB_4_missing_bot_結構性_missing(tmp_path, monkeypatch):
    """R131 結構性確認: irisx_bot/grokx/lpbot/mimo 永久非本機 scope

    守: 若有人改 missing 邏輯 (e.g. 給這 4 個加 default snapshot 路徑,
    或把「missing」改成「auto_fresh」之類的假數據) → K0 Quota coverage
    9/13 → 13/13 假象反而掩蓋 OpenAB bot 是否真在運作的事實。

    對齊 R131 量化: 4 missing bot 是「永久非本機 scope」, 本機守護量化
    口徑不漂移 (即使是 missing, 計算法必須一致, 不能為了 KPI 漂亮造假)。
    """
    monkeypatch.setattr(k0, "QUOTA_DIR", tmp_path)
    # 不寫任何檔案 → 全 13 個應 = missing
    out = k0.scan_quota_snapshots()

    # 9 個 OpenAB bot 全 missing (含 4 個結構性 missing)
    for p in k0.OPENAB_BOT:
        assert out[p]["state"] == "missing", (
            f"{p} 應 = missing (空 QUOTA_DIR), 實際 state={out[p]['state']}"
        )
        assert out[p]["path"] is None

    # 特別守護 4 個結構性 missing bot (R131 量化確認)
    for p in ("irisx_bot", "grokx", "lpbot", "mimo"):
        assert out[p]["state"] == "missing", (
            f"{p} 結構性 missing bot (R131), 量化口徑必須守住"
        )

    # 4 本機 CLI 也 missing (沒 usage-local.json)
    for p in k0.LOCAL_CLI:
        assert out[p]["state"] == "missing"
