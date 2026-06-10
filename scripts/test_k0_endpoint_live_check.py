"""
K0 endpoint live 雙源量測 closure 守護 — pytest test suite

R197 落地 (M2 KPI 量測 closure 軸換 endpoint live 軸, 鏡像 R194 K42 過期契約
漂移偵測模式: 雙源比對守 hidden gap)。5 case pytest 守 5 個 K0 endpoint
live 雙源 hidden gap:

  - case 1 endpoint_alive happy path: 端點 UP + 5 label emit (含 __local__) →
    live_emit_count=4 (4 KNOWN_PROVIDERS)
  - case 2 endpoint_alive boundary: 端點 DOWN (URLError mock) →
    endpoint_alive=False + live_emit_count=0
  - case 3 JSON 缺失 boundary: .harness-k0.json 不存在 →
    json_emit_count=None + drift 雙源比對跳過 (空 list 守住)
  - case 4 雙源一致 happy path: live emit = {4 KNOWN + __local__} +
    JSON providers = {4 KNOWN} → drift = {live_new:['__local__'],
    json_stale:[]} (mirror production: JSON 不寫 __local__ 因為只算
    KNOWN_PROVIDERS)
  - case 5 雙源漂移 hidden gap: live emit = {claude, codex} + JSON
    providers = {claude, codex, copilot, gemini} → drift =
    {live_new:[], json_stale:['copilot', 'gemini']} (JSON 寫死但端點
    已停 = stale JSON 隱藏 bug, R194 K42 過期契約漂移模式的 K0 維度
    橫展, M0 級最關鍵守護)

對齊既 R196 內部函式 hidden gap 4 case 模式 (R196 加 4 case 守 K40
內部函式 hidden gap) + R194 K42 漂移偵測 5 case 模式。防有人改寬
parse_live_providers pattern 漏算 __local__ / 改嚴漏算 KNOWN_PROVIDERS
/ 把 drift 偵測拿掉 / 改壞 endpoint_alive URLError fallback / JSON 缺失
crash。換軸 R197 sub-axis 換 (內部函式 hidden gap → endpoint live 雙源
漂移), mirror R194 K42 chain_staleness_drift_check 模式到 K0 維度。
"""
import json
import os
import sys
import tempfile
from pathlib import Path
from unittest import mock

import pytest

# 把 scripts/ 加進 sys.path 才能 import k0_endpoint_live_check
SCRIPTS_DIR = Path(__file__).resolve().parent
if str(SCRIPTS_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPTS_DIR))

import k0_endpoint_live_check as k0  # noqa: E402


# ---------- fixtures ----------

def _metrics_text(labels: list[str]) -> str:
    """組 Prometheus /metrics 假文字: 對齊 k0_measure.parse_provider_emit
    pattern `lobsterpulse_provider_*{provider="X"}`。
    """
    lines = [
        "# HELP lobsterpulse_provider_sessions Sessions per provider",
        "# TYPE lobsterpulse_provider_sessions gauge",
    ]
    for label in labels:
        lines.append(f'lobsterpulse_provider_sessions{{provider="{label}"}} 0')
        lines.append(f'lobsterpulse_provider_active{{provider="{label}"}} 0')
    return "\n".join(lines) + "\n"


# ---------- case 1: endpoint_alive happy path + live_emit_count hidden gap ----------

def test_endpoint_UP_5_label_emit_KNOWN_PROVIDERS_4_個_live_emit_守住():
    """守 endpoint live 維度 1+2 hidden gap:
      - endpoint_alive True (HTTP 200 守住, URLError fallback 不誤觸)
      - live_emit_count = 4 (5 label 中 4 個是 KNOWN_PROVIDERS,
        1 個 __local__ 被過濾掉 — __local__ 不在 13 provider scope)
      - live_provider_labels 含 5 label 原始清單 (不過濾)
    防有人改 parse_live_providers pattern 漏算 __local__ 或把
    live_emit_count 計算從「對齊 KNOWN_PROVIDERS」改成「全 emit label 數」
    失真 (5 變 5 = 失去 __local__ 過濾語意, 5 變 4 = 漏算某個 emit)。
    """
    metrics_text = _metrics_text(["__local__", "claude", "codex", "copilot", "gemini"])
    with mock.patch.object(k0, "fetch_live_metrics",
                           return_value=(True, metrics_text)):
        result = k0.measure_endpoint_live()

    assert result["endpoint_alive"] is True, (
        f"endpoint_alive 應 True, 實際 {result['endpoint_alive']}. "
        f"URLError fallback 不應誤觸 happy path。"
    )
    assert result["live_emit_count"] == 4, (
        f"5 label 中 4 個是 KNOWN_PROVIDERS (claude/codex/copilot/gemini), "
        f"__local__ 被過濾, 應 live_emit_count=4, 實際 {result['live_emit_count']}. "
        f"改寬 pattern 會把 __local__ 算成 emit, 改嚴會漏算某個 KNOWN_PROVIDERS。"
    )
    assert result["live_provider_labels"] == [
        "__local__", "claude", "codex", "copilot", "gemini"
    ], (
        f"live_provider_labels 應保留 5 個原始 label 排序, 實際 {result['live_provider_labels']}. "
        f"拿掉 __local__ 過濾只會影響 live_emit_count, 不影響原始清單。"
    )


# ---------- case 2: endpoint_alive boundary (M0 級 — 端點死掉不爆) ----------

def test_endpoint_DOWN_alive_False_live_emit_0_守住():
    """守 endpoint live 維度 1+2 boundary: 端點 DOWN (URLError mock) →
    endpoint_alive=False + live_emit_count=0 + live_provider_labels 空。
    這是 M0 級 hidden gap: 端點死掉 (main app 沒跑 / port 被佔 / 網路斷)
    → urllib 拋 URLError / OSError, fetch_live_metrics 必須回
    (False, "") 不爆, 後續 parse_live_providers 空字串守 0/13 不 crash。
    改壞 try/except → URL error 會 propagate, K0 endpoint live closure
    整個失效, 報表腳本 crash。
    """
    with mock.patch.object(k0, "fetch_live_metrics",
                           return_value=(False, "")):
        result = k0.measure_endpoint_live()

    assert result["endpoint_alive"] is False, (
        f"endpoint DOWN 應 endpoint_alive=False, 實際 {result['endpoint_alive']}. "
        f"URLError / OSError 必須被 try/except 守住回 (False, '')。"
    )
    assert result["live_emit_count"] == 0, (
        f"endpoint DOWN 應 live_emit_count=0, 實際 {result['live_emit_count']}."
    )
    assert result["live_provider_labels"] == [], (
        f"endpoint DOWN 應 live_provider_labels 空, 實際 {result['live_provider_labels']}."
    )

    # 雙源比對: JSON 存在時, 端點 DOWN 仍能跑雙源 (live 全空 vs json
    # 寫死 → drift.json_stale = JSON 寫的所有 provider), 不應 crash
    with tempfile.TemporaryDirectory() as tmp:
        json_path = Path(tmp) / "k0.json"
        json_path.write_text(json.dumps({
            "k0a1_health_emit": {
                "covered": 4, "total": 13,
                "providers": ["claude", "codex", "copilot", "gemini"]
            }
        }), encoding="utf-8")
        with mock.patch.object(k0, "fetch_live_metrics",
                               return_value=(False, "")):
            result2 = k0.measure_endpoint_live(json_path=json_path)
        assert result2["endpoint_alive"] is False
        assert result2["live_emit_count"] == 0
        assert result2["json_emit_count"] == 4, (
            f"JSON 寫了 4 個, 端點 DOWN 雙源比對應讀到 json_emit_count=4, "
            f"實際 {result2['json_emit_count']}."
        )
        # 真 stale JSON 隱藏 bug 應被偵測: JSON 寫了 4 個但當下 0 個 emit
        assert result2["drift"]["json_stale"] == ["claude", "codex", "copilot", "gemini"], (
            f"endpoint DOWN 應觸發 json_stale = JSON 寫的所有 provider, "
            f"實際 {result2['drift']['json_stale']}. 這是 M0 級 stale JSON "
            f"隱藏 bug 守護, 對齊 R194 K42 過期契約漂移偵測模式。"
        )


# ---------- case 3: JSON 缺失 boundary (M0 級 — .harness-k0.json 沒跑過不爆) ----------

def test_JSON_缺失_雙源比對_跳過_drift_空_守住():
    """守 endpoint live 維度 3 boundary: .harness-k0.json 不存在 (k0_measure.py
    從沒跑過) → json_emit_count=None + drift 雙源比對跳過 (空 list 守住)。
    防有人把 read_json_emit_providers 的 `if not json_path.exists(): return None`
    拿掉 → FileNotFoundError, K0 endpoint live closure 失效;
    或把 `data.get("k0a1_health_emit")` 拿掉 → KeyError, 雙源比對 crash。
    """
    with tempfile.TemporaryDirectory() as tmp:
        nonexistent_json = Path(tmp) / "no-such-k0.json"
        metrics_text = _metrics_text(["claude", "codex"])
        with mock.patch.object(k0, "fetch_live_metrics",
                               return_value=(True, metrics_text)):
            result = k0.measure_endpoint_live(json_path=nonexistent_json)

    assert result["json_emit_count"] is None, (
        f"JSON 缺失應 json_emit_count=None, 實際 {result['json_emit_count']}. "
        f"read_json_emit_providers 必須守住 FileNotFoundError 不爆。"
    )
    assert result["json_emit_providers"] is None, (
        f"JSON 缺失應 json_emit_providers=None, 實際 {result['json_emit_providers']}."
    )
    assert result["drift"] == {"live_new": [], "json_stale": []}, (
        f"JSON 缺失應 drift 雙源比對跳過 (空 list 守住), 實際 {result['drift']}. "
        f"拿掉 `if json_providers is None` 守衛 → drift 會用 None 集合算漂移, crash。"
    )

    # 邊界: JSON 存在但欄位缺失 (e.g. k0_measure 寫壞 / 半完成)
    with tempfile.TemporaryDirectory() as tmp:
        bad_json = Path(tmp) / "bad.json"
        bad_json.write_text(json.dumps({"timestamp": "2026-06-10"}),
                            encoding="utf-8")
        metrics_text = _metrics_text(["claude"])
        with mock.patch.object(k0, "fetch_live_metrics",
                               return_value=(True, metrics_text)):
            result2 = k0.measure_endpoint_live(json_path=bad_json)
        assert result2["json_emit_count"] is None, (
            f"JSON 欄位缺失應 json_emit_count=None, 實際 {result2['json_emit_count']}. "
            f"對齊 read_json_emit_providers 既有 k0a1 not isinstance dict 守衛。"
        )


# ---------- case 4: 雙源一致 happy path (mirror production 真實狀態) ----------

def test_雙源一致_drift_OK_live_new_含__local__守住():
    """守 endpoint live 維度 5 happy path: 雙源一致 (production 真實狀態
    鏡像 — live 端點 emit 5 label 含 __local__, JSON 寫 4 KNOWN_PROVIDERS
    對齊 K0-A1 emit 維度過濾 __local__)。結果 drift.live_new = ['__local__']
    (端點 emit 但 JSON 不寫 — 預期, 因為 JSON 對齊 K0-A1 只算
    KNOWN_PROVIDERS), drift.json_stale = [] (JSON 寫的 4 個都還在
    emit 中, 沒 stale)。
    防有人改寬 JSON 寫入邏輯把 __local__ 也算進去, 或拿掉 live_new
    偵測 (把空集合跟空集合 drift 算「OK」變成「真無漂移」假 PASS)。
    """
    with tempfile.TemporaryDirectory() as tmp:
        json_path = Path(tmp) / "k0.json"
        json_path.write_text(json.dumps({
            "k0a1_health_emit": {
                "covered": 4, "total": 13,
                "providers": ["claude", "codex", "copilot", "gemini"]
            }
        }), encoding="utf-8")
        metrics_text = _metrics_text(["__local__", "claude", "codex", "copilot", "gemini"])
        with mock.patch.object(k0, "fetch_live_metrics",
                               return_value=(True, metrics_text)):
            result = k0.measure_endpoint_live(json_path=json_path)

    assert result["endpoint_alive"] is True
    assert result["live_emit_count"] == 4, (
        f"live 5 label 中 4 KNOWN_PROVIDERS, 實際 {result['live_emit_count']}."
    )
    assert result["json_emit_count"] == 4, (
        f"JSON 寫 4 個, 實際 {result['json_emit_count']}."
    )
    # 雙源一致 happy path: drift.live_new 含 __local__ (預期), json_stale 空
    assert result["drift"]["live_new"] == ["__local__"], (
        f"JSON 不寫 __local__ 是預期 (對齊 K0-A1 KNOWN_PROVIDERS scope), "
        f"應 live_new=['__local__'], 實際 {result['drift']['live_new']}. "
        f"拿掉 live_new 偵測 → __local__ 漂移 silent 失真。"
    )
    assert result["drift"]["json_stale"] == [], (
        f"JSON 寫的 4 個都還在 emit, 應 json_stale=[], 實際 {result['drift']['json_stale']}."
    )


# ---------- case 5: 雙源漂移 hidden gap (M0 級 — stale JSON 隱藏 bug 守護) ----------

def test_雙源漂移_json_stale_觸發_端點已停_守住():
    """守 endpoint live 維度 5 hidden gap: 雙源漂移 — JSON 寫死 4 KNOWN
    但端點當下只 emit 2 個 (e.g. main app 重啟後 copilot/gemini 進程
    還沒起來, 但 .harness-k0.json 是上次跑的歷史值)。結果
    drift.json_stale = ['copilot', 'gemini'] (JSON 寫了但端點已停
    = stale JSON 隱藏 bug, 對齊 R194 K42 過期契約漂移偵測模式),
    drift.live_new = [] (端點沒新 emit provider)。
    這是 R197 K0 維度最關鍵守護: 改寬 drift 邏輯把 live_new + json_stale
    任一拿掉, 或把雙向 set diff 改成單向 (- only), stale JSON 隱藏
    bug 就 silent 失真, k0_measure 跟 endpoint live 報表會說「K0-A1
    持平 4/13」但實際端點只剩 2/13, K0 量化值失真。
    """
    with tempfile.TemporaryDirectory() as tmp:
        json_path = Path(tmp) / "k0.json"
        json_path.write_text(json.dumps({
            "k0a1_health_emit": {
                "covered": 4, "total": 13,
                "providers": ["claude", "codex", "copilot", "gemini"]
            }
        }), encoding="utf-8")
        # 端點當下只 emit 2 個 (copilot/gemini 進程還沒起來)
        metrics_text = _metrics_text(["claude", "codex"])
        with mock.patch.object(k0, "fetch_live_metrics",
                               return_value=(True, metrics_text)):
            result = k0.measure_endpoint_live(json_path=json_path)

    assert result["endpoint_alive"] is True
    assert result["live_emit_count"] == 2, (
        f"端點當下只 emit 2 個, 實際 {result['live_emit_count']}."
    )
    assert result["json_emit_count"] == 4, (
        f"JSON 寫 4 個, 實際 {result['json_emit_count']}."
    )
    # 雙向漂移: json_stale 觸發 (JSON 寫死但端點已停), live_new 空
    assert result["drift"]["json_stale"] == ["copilot", "gemini"], (
        f"JSON 寫了 4 個但端點當下只 emit 2 個, 應 json_stale=['copilot','gemini'], "
        f"實際 {result['drift']['json_stale']}. 這是 M0 級 stale JSON 隱藏 bug 守護, "
        f"對齊 R194 K42 過期契約漂移偵測模式, mirror R194 chain_staleness_drift_check "
        f"5 case 模式到 K0 維度。"
    )
    assert result["drift"]["live_new"] == [], (
        f"端點沒新 emit provider, 應 live_new=[], 實際 {result['drift']['live_new']}."
    )


# ---------- R198 內部函式 hidden gap 守護延伸 4 case ----------
#
# 鏡像 R188 (k0_measure 6→9) / R195 (chain_staleness 8→11) / R196
# (K40 12→16) 「內部函式軸 hidden gap 守護延伸」模式, R198 換對齊
# K0 endpoint live 內部函式軸, 5→9 守住 4 個內部函式的 M0 級 hidden gap:
#
#   - case 6 fetch_live_metrics 層: timeout 參數被改大 (2s → 30s)
#     隱藏慢端點問題, 量化口徑 silent 漂移
#   - case 7 parse_live_providers 層: regex pattern 改嚴, 漏算
#     `lobsterpulse_provider_` metric 前綴以外的合法 emit 變形
#   - case 8 read_json_emit_providers 層: k0a1_health_emit.covered 結構
#     改壞 (漏 providers 欄位), 雙源比對 silent 退化
#   - case 9 measure_endpoint_live 層: __local__ 不該被算進
#     live_emit_count 13 scope, 防有人改寬 KNOWN_PROVIDERS 含 __local__
#     偷偷 +1, 量化值造假
#
# R198 不破 R97 紅線 (純 Python pytest 護衛, chain 20→20 守), 不搶 owner M
# scope (前端 UI 接到 emit 留 owner M), 換對齊 K0 endpoint live 內部函式軸
# 守住既有 closure 不退化。


# ---------- case 6: fetch_live_metrics 內部函式 — timeout 參數 boundary 守護 ----------

def test_fetch_live_metrics_timeout_參數_改大_2_到_30_觸發_REGRESS():
    """守 fetch_live_metrics 內部函式的 timeout 預設值 boundary。
    R197 落地時 timeout=2 是 fast-fail 設計 (silent 慢端點 = silent K0
    量化失真); 若有人把它改成 30, 端點掛了會 30s 才回 endpoint_alive=False,
    期間整個 k0_endpoint_live_check.py 卡住, harness K0 量化無法在
    合理時間出表 (對齊 k0_measure.py scrape 節奏)。
    守法: 透過 mock urllib.request.urlopen, 驗證傳入的 timeout 參數
    仍是 2 (不是 30 / 0 / None / 任意值), 守住 K0 endpoint live
    量化閉合時間上限。
    """
    captured = {}

    def fake_urlopen(req, timeout=None):
        captured["timeout"] = timeout
        # 用 case 1 的 happy path 文字
        return _FakeHTTPResponse(_metrics_text(["claude", "__local__"]))

    with mock.patch.object(k0.urllib.request, "urlopen", side_effect=fake_urlopen):
        alive, text = k0.fetch_live_metrics()

    assert alive is True
    assert captured.get("timeout") == 2, (
        f"fetch_live_metrics timeout 預設值被改, 應 = 2 (fast-fail 量化閉合), "
        f"實際 {captured.get('timeout')}. 這是 M0 級量化口徑時間上限守護, "
        f"改大會讓 K0 endpoint live 在端點掛掉時 silent 卡 30s, K0 量化值 "
        f"失真。對齊 R132 k0_drift_check BASELINE 寫死模式, mirror R196 "
        f"K40 內部函式 hidden gap 守護延伸 case 6 模式。"
    )


class _FakeHTTPResponse:
    """mock urllib HTTP response, 對齊 case 1 happy path 結構。"""
    def __init__(self, text: str):
        self._text = text.encode("utf-8")

    def read(self) -> bytes:
        return self._text

    def __enter__(self):
        return self

    def __exit__(self, *args):
        return False


# ---------- case 7: parse_live_providers 內部函式 — regex pattern 改嚴 boundary 守護 ----------

def test_parse_live_providers_改嚴_pattern_只認_lobsterpulse_provider_守住():
    """守 parse_live_providers 內部函式的 regex pattern 改嚴 boundary。
    R197 pattern `lobsterpulse_provider_*{provider="X"}` (raw escape) 是寬鬆抓所有
    metric 變形 (sessions / active / tokens / etc); 若有人改成
    嚴格 `lobsterpulse_provider_sessions{provider="X"}$` (只認
    sessions 一種), 任何 `lobsterpulse_provider_active` /
    `lobsterpulse_provider_tokens` 變形都會 silent 漏算, K0-A1
    live_emit_count 偏小。
    守法: 餵含 `lobsterpulse_provider_active` 而不含 `provider_sessions`
    的 metrics_text, 確認 parse 仍抓得到, 守住寬鬆 pattern 不退嚴。
    """
    # 故意只給 active 變形 (沒 sessions), 嚴格 pattern 會回空 set
    metrics_text = "\n".join([
        "# TYPE lobsterpulse_provider_active gauge",
        'lobsterpulse_provider_active{provider="claude"} 0',
        'lobsterpulse_provider_active{provider="__local__"} 0',
    ]) + "\n"

    labels = k0.parse_live_providers(metrics_text)

    assert "claude" in labels, (
        f"parse_live_providers 改嚴 pattern 漏算 active 變形, "
        f"應含 'claude', 實際 {sorted(labels)}. 這是 M0 級量化口徑 "
        f"pattern 改嚴 silent 漂移守護, 對齊 R196 K40 內部函式 case 7 模式。"
    )
    assert "__local__" in labels, (
        f"parse_live_providers 改嚴 pattern 漏算 __local__ 變形, "
        f"應含 '__local__', 實際 {sorted(labels)}."
    )
    # KNOWN_PROVIDERS 不在的 label 也該被 parse 抓到 (留給 measure_endpoint_live 過濾)
    assert len(labels) == 2, (
        f"parse_live_providers 應 parse 全部合法 provider label, 應 = 2, "
        f"實際 {len(labels)}: {sorted(labels)}."
    )


# ---------- case 8: read_json_emit_providers 內部函式 — JSON 結構改壞 boundary 守護 ----------

def test_read_json_emit_providers_JSON_改寫成_covered_無_providers_回_None_守住():
    """守 read_json_emit_providers 內部函式的 JSON 結構讀取 boundary。
    R197 讀 `.harness-k0.json` 的 `k0a1_health_emit.providers` list;
    若有人改 k0_measure.py 把 `providers` 改成 `covered` (或加新欄位
    `k0a1_health_emit.covered` 而 `providers` 漏寫), JSON 讀取層
    應該 silent 回 None (而不是 KeyError crash 或回錯的 set)。
    守法: 餵缺 `providers` 欄位的 JSON, 確認回 None, 守住雙源比對
    fail-closed (None → 雙源比對跳過, 跟 R197 case 3 一致)。
    """
    with tempfile.TemporaryDirectory() as tmp:
        json_path = Path(tmp) / "k0.json"
        # k0a1_health_emit 有但 providers 欄位缺失 (改壞)
        json_path.write_text(json.dumps({
            "k0a1_health_emit": {
                "covered": 4, "total": 13
                # 故意不寫 "providers" 欄位
            }
        }), encoding="utf-8")

        result = k0.read_json_emit_providers(json_path=json_path)

    assert result is None, (
        f"read_json_emit_providers 對 JSON 缺 providers 欄位應回 None "
        f"(fail-closed 雙源比對跳過), 實際 {result}. 這是 M0 級 JSON 結構 "
        f"改壞 silent 漂移守護, 對齊 R196 K40 內部函式 case 8 模式。"
    )


# ---------- case 9: measure_endpoint_live 內部函式 — __local__ 不計入 live_emit_count 守護 ----------

def test_measure_endpoint_live___local__標籤存在_不計入_live_emit_count_守住():
    """守 measure_endpoint_live 內部函式的 __local__ 過濾邏輯。
    R197 設計 `live_emit_count` 只算 KNOWN_PROVIDERS 13 個,
    `__local__` 是內部累計 label, 不該被算進 13 scope。
    若有人改寬 sum 邏輯 (e.g. `sum(1 for p in live_labels)` 直接
    算全部 label), 端點 emit 5 label (含 __local__) 會被算成
    5/13, K0-A1 量化值偷偷 +1 造假 (結構性 0 差距 closure 失守)。
    守法: 餵 5 label (4 KNOWN + 1 __local__), 確認 live_emit_count=4
    (不是 5), live_provider_labels 含 __local__ (原始清單要留,
    給後續診斷用)。
    """
    metrics_text = _metrics_text(["claude", "codex", "copilot", "gemini", "__local__"])
    with mock.patch.object(k0, "fetch_live_metrics",
                           return_value=(True, metrics_text)):
        result = k0.measure_endpoint_live()

    # live_emit_count 只算 KNOWN_PROVIDERS 13 scope
    assert result["live_emit_count"] == 4, (
        f"__local__ 不該被算進 live_emit_count 13 scope, 應 = 4 (4 KNOWN), "
        f"實際 {result['live_emit_count']}. 這是 M0 級量化口徑 __local__ "
        f"過濾守護, 對齊 R197 量測口徑 13 scope 不變。"
    )
    # live_provider_labels 原始清單要留 __local__ (給後續診斷)
    assert "__local__" in result["live_provider_labels"], (
        f"live_provider_labels 原始清單應保留 __local__ (給診斷), "
        f"實際 {result['live_provider_labels']}."
    )
    # 雙向驗證: 原始清單大小應該是 5 (4 KNOWN + 1 __local__)
    assert len(result["live_provider_labels"]) == 5, (
        f"live_provider_labels 應含全部 emit 過的 label (4 KNOWN + 1 __local__), "
        f"應 = 5, 實際 {len(result['live_provider_labels'])}: "
        f"{result['live_provider_labels']}."
    )
