# LobsterPulse 龍蝦監控 — Mission

> **本檔是策略決策錨點**。所有 PR / 規格變更 / commit 都要回頭對齊這頁。
> 補頁時機：R81（策略顧問連 3 次判 DRIFTING 之後強制補）。

---

## 北極星（North Star）

> **單一膠囊，統一監控所有 AI coding agent 的真實任務狀態。**
> 不管它是本機 CLI、OpenAB bot、還是未來新形態的 agent runtime。

用人話講：當開發者桌面上同時跑著 5 個 AI agent，
LobsterPulse 必須能用 1 個膠囊 + 1 個 view 讓他 0 切換成本地知道：
- 哪個 agent 還在做事 / 哪個在等他回 / 哪個已死
- 每個 agent 本次 session 燒多少 token / 配額
- 出事時 1 鍵撈到事件診斷

---

## 非目標（Non-Goals）

明確拒做，避免 scope 失控：

1. **不做 IDE 整合**（VS Code / JetBrains extension） — 搶 IDE 市場是別人的仗
2. **不做雲端 dashboard / SaaS 訂閱** — 本機桌面工具，server-side 不在 scope
3. **不做 agent 排程 / 路由 / 編排** — 我們是監控，不是 orchestrator
4. **不做付費 / 商業化** — 開源 hobby 專案
5. **不做 Linux/macOS 平台專屬優化**（僅 Windows 主力，跨平台以不擋路為原則）
6. **不做 OpenAB 之外的 bot 框架**（Discord / Slack bot 不在 scope；本機 CLI hook 仍支援）
7. **不取代 CLI 本身** — 不搶 agent 邏輯，只讀事件

---

## 90 天成功指標（KPI）

> 90 天後（~2026-09-04）回頭驗收這 3 個數字。

| KPI | 前值 (R81) | 90 天目標 | 量測方式 |
|---|---:|---:|---|
| **K0-A1 Provider 健康度 emit 覆蓋率（端點實際 emit）** | 0/13 provider 在 /metrics 端點實際 emit 過 `lobsterpulse_provider_*{provider="X"}` 樣本 | 13/13 端點 emit (R102 拆維度, 受 OpenAB bot 是否在運作影響) | 掃 /metrics 文本中出現的 `provider="..."` label 集合 |
| **K0-A2 Provider 健康度 sample 覆蓋率（非零 sessions）** | 0/13 provider 有非零 `provider_sessions` 樣本 | 13/13 真正「在運作 + 事件流過」 | `lobsterpulse_provider_sessions{provider="X"}` 值 > 0 |
| **K0 Provider 健康度（程式碼定義層, R101 補齊）** | 0/13 provider 有對應 metric family 程式碼 emit 路徑 | 13/13 程式碼 emit 定義 (R101 已達標) | grep `lobsterpulse_provider_*.{provider=X}` 對每個 X 都有定義 |
| **K0 Quota 監控即時性** | 6 個 OpenAB bot 有 snapshot；本機 CLI 無 quota 監控 | 13/13 provider 都有 | `usage-*.json` 或等價 metric 是否被讀到 |
| **K40 規格覆蓋率** | 1/1 active change (openab-bot-sync) 12/12 落地 | 100% 落地、0 漂移 | `spectra validate --changes <name>` 通過 + tasks.md 12/12 |
| **K41 chore_treadmill 紅線** | 24h 55% 觸發 | <30% 持續 7 日 | `git log --since='7d' --pretty=format:'%s' \| grep -c '^chore' / total < 0.30` |
| **K42 護欄 chain 飽和** | 17 條 saturated | 守住 17，不過度擴張 | guard test 全綠 + 新增需有架構變更理由 |

### R108 量測快照（補：避免 R81 前值凍結誤導）

> 補頁動機：R108 supervisor 報 `consecutive_drifts: 3` + top_risk = 「K0 Quota 數字」，
> 追源頭發現 MISSION/CLAUDE 量化值停在 R81、跟現實分叉。R81 baseline 是策略錨點
> 不可抹，**新加 R108 量測 column** 保留 R81 作為「歷史基準」+ 補當前現況。
>
> **R108 ~ R144 補敘述詳細歷史見 [`docs/kpi-history.md`](kpi-history.md)**（R109 補 copilot / R111 補端點復活 / R114 補 openx alias / R119 補 chain 19 / R122 補 timeline 護衛 / R127 補 .gitignore 護衛 / R128 補 T-CPT10 ship / R130 補 spec closure / R131 補 4 missing 結構性確認 / R132 補 R-CPT 整體 closure + chain 20 + baseline 451 / R144 補 K40 doc drift 修 (R135 樂觀 closure 寫入修: 8 closed + 1 active 9/16 otel-genai) + 接力 1 doc drift closure)。MISSION 主表只留 R81 baseline + 最新一欄，中間補段全部歸檔本檔避免 MISSION 欄位爆炸。

| KPI | R81 baseline（前值） | R108 量測現況 | R109 補 | R111 補 (端點復活) | R119 補 (chain 19 + R-CPT 接力) | R130 補 (R-CPT M1 8/8 closure) | R132 補 (R-CPT 整體 closure + chain 20 + baseline 451) | R144 補 (K40 doc drift 修 + 接力 1 closure) | 驗收差距 |
|---|---:|---:|---:|---:|---:|---:|
| K0-A1 emit 覆蓋 | 0/13 | 0/13 (endpoint DOWN, 未跑 build) | 0/13 (endpoint 仍 DOWN) | **5/13** (endpoint UP, 5 provider labels 端點實際 emit: claude/codex/copilot/gemini/cicx) | 缺 8 (5 emit 但 0 sessions, 距 13/13 sample 級距仍差 8) | **5/13 持平 R119** (R128 T-CPT10 純 frontend, 對齊 R-CPT-4 不開新 OTel 維度護衛) | 缺 8 (非本機 scope) | **4/13** (R150 spec drift 修: cicx 屬 OpenAB scope 浮動, 4/13 為本機穩態下限) | 缺 8 (cicx 屬 OpenAB scope, 非本機可達穩態; 其餘 4 missing 仍非本機 scope) |
| K0-A2 sample 覆蓋 | 0/13 | 0/13 (endpoint DOWN) | 0/13 (endpoint 仍 DOWN) | **2/13** (claude=11 + cicx=1 真有 sessions) | 缺 11 (非本機 scope, 需 OpenAB 端跑起來) | **1/13 持平 R119** (claude=3 sessions 累加, endpoint sessions 隨時間浮動) | 缺 12 (非本機 scope) | 1/13 持平 R132 | 缺 12 (非本機 scope) |
| K0 程式碼定義層 (R101) | 0/13 | 13/13 (R101 達標) | 13/13 (守住) | 13/13 (守住) | 達標 |
| K0 Quota 監控即時性 | 6 OpenAB snapshot；本機無 | **K0-B fresh 4/13 + K0-Q 8/13** | **K0-B fresh 4/13 + K0-Q 9/13** (R114 修 openx alias: openx 從 missing 變 stale, +1) | **K0-B fresh 4/13 + K0-Q 9/13** 持平 R114 (4 missing: irisx_bot/grokx/lpbot/mimo 非本機 scope) | **K0-B fresh 4/13 + K0-Q 9/13 持平 R119** (R128 不開新 snapshot, 對齊 R-CPT-4 不開新 data path 護衛) | 缺 4 (irisx_bot/grokx/lpbot/mimo 完全 missing, 非本機 scope) | K0-B 4/13 + K0-Q 9/13 持平 R132 | 缺 4 (非本機 scope) |
| K40 規格覆蓋率 | 1/1 (openab-bot-sync 12/12) | 5/5 active change 全 closed (43/43 tasks) | 5/5 持續 closed | **7/7** (R116 R115 lobster-rules-engine closure 接力) | **7/7 持續 + R-CPT M1 進度條 8/8 closure** (R128 ship T-CPT10, R-CPT change 整體待 R131+ 收 closure 接力) | **7/7** 持平 R119 (R130 spec closure, R131 4 missing 結構性確認) | **8/9 closed + 1 active 9/16** (R135 收 R-CPT 整體 15/15 + prometheus-counter-rename 6/6 入庫 8 change N/N closed; otel-genai-runtime-emit-2026-q3 [9/16] 仍 active, 缺 T-OGRE10~16 7 tasks) | **8/9 closed + 1 active 9/16 持平 R132** (R144 修 R135 樂觀 closure 寫入: 9/9 錯記 → 8/9 + 1 active; otel-genai owner M scope) | **達標 (2026-07-05 M1 落地 T-OGRE10~16 7 tasks, otel-genai 16/16 closed; K40 量測口徑 9/10 closed + 1 active = mission-k0 Path B owner M 未選)** |
| K41 chore_treadmill 24h | 55% | **R108 k41_chore_treadmill.py 7d: 13/206 = 6.3%** | 達標延續 | 達標延續 | 達標 (<30%) | 達標 | 達標 (<30%) |
| K42 護衛 chain | 17 條 | 17 條 (R113.1 owner M dual-emit value guard 提案中) | 17 條 (R114 落地 dual-emit value guard 進既有 `render_prometheus_tests` mod, chain 17→17 不擴張守住) | 17 條 持平 (R115 護衛 test 三條加進既 `auto_rules::tests` mod, 走既有 mod 17→17) | **19 條** (R122 ship `timeline::tests` mod 走 R97 飽和契約例外 +1, R127 ship `.gitignore` 護衛 +1, R97 後 +2 例外架構理由明確; baseline 446/446 全綠) | **20 條** (R131 ship plugin registry 護衛 +1 走既 `auto_rules::tests` mod, R97 後 +3 例外架構理由明確; baseline 450/450 全綠) | **20 條 持平 R131** (R135 .gitignore 補網 __pycache__/ 護衛 test +1 走既護衛, chain 不擴張; baseline 451/451 全綠) | **20 條 持平 R132** (R144 不開新護衛, doc-level 修, chain 20→20 守住) | 達標 (R97 後 +3 例外守住) |

**R108~R132 量化結論**：
- K0 Quota 距 13/13 目標缺 4 (R108 4 個 → R114 修 openx alias +1 → 仍缺 4 個) — 缺 OpenAB `irisx_bot`/`grokx`/`lpbot`/`mimo` 寫 snapshot，**非本機 scope**
- K0-A1 emit 0/13 (R108/R109) → 5/13 (R111 端點復活) → **4/13 (R150 實跑對齊)** — 端點 DOWN (R108) ≠ emit 邏輯壞, main app 跑就 4~5 label 端點 emit (cicx 屬 OpenAB scope 隨 bot 上下線浮動, 4/13 為本機穩態下限)
- K0-A2 sample 0/13 (R108/R109) → 2/13 (R111 claude=11 + cicx=1) → **1/13 (R132 claude=3 sessions 累加)** — 距 13/13 仍缺 12, **非本機 scope**
- 本機 CLI 段 K0 Quota 100% 滿覆蓋（claude R85 / codex R86 / gemini R108 / copilot R109 — 4/4）
- 5 個文件/治理級 KPI 全綠 — supervisor 報的「drift」是 **文件 vs 量測分叉**，非 KPI 倒退
- K42 護衛鏈 17 → **20** (R122 ship `timeline::tests` mod + R127 ship `.gitignore` 護衛 + R131 ship plugin registry 護衛, R97 後 +3 例外架構理由明確, R135 .gitignore 補網 __pycache__/ +1 test chain 不擴張, baseline 451/451 全綠守住)
- R-CPT change 整體 15/15 closure (R135 收 M0+M1+M2/M3 spec closure, K40 spec coverage 從 7/7 升至 **8/9 closed + 1 active 9/16** = 8 N/N closed + otel-genai-runtime-emit-2026-q3 [9/16] active, 缺 T-OGRE10~16 7 tasks owner M scope)
- 4 missing bot 結構性確認 0 spec drift (R131 量化) — 本機端 13/13 程式碼層全部對齊 KNOWN_PROVIDERS + 4 同步點 + parse_provider 護衛 + read path
- 中間補敘述 (R109/R111/R114/R119/R122/R127/R128/R130/R131) 全部歸檔 [`docs/kpi-history.md`](kpi-history.md), 恢復 MISSION 決策可讀性
- 下個 M1 候選：R133+ 接力 K0 Quota 4 missing 補鏈路 (OpenAB scope) + R133+ 接力 K0-A1 emit 4/13 → 5/13 護衛 (本機 4 已達穩態, 5/13 需 cicx OpenAB 端) + R117 capsule-brief JS 配套等 owner M 收 + R133+ 接力護衛 過期契約審計 (護衛對應 spec 最後更新時間)

### R182 補 (Path A 結構性降級決議: 4+5+4 永久非 scope) — R197 closure

> **觸發**: MISSION 自身定義的強制升級條件過期 ~10 週沒人 fire, R182 觸發訊號鏈
> 3 重鎖定 (AI Supervisor 方向 UNKNOWN 0/10 + 策略顧問 #1 行動 closure 路徑 +
> MISSION 自身 2-週 lag 觸發條件過期 ~10 週)。R197 選 **Path A 降級** 1 輪 closure。

> **「0 結構性差距」的口徑聲明**：下表所有「0 結構性差距」僅對**已宣告可達 scope**
> 成立（本機 4 + OpenAB 5 = 9），不是對外宣稱的 13 provider。4 個永久非本機 scope 的
> bot（irisx_bot/grokx/lpbot/mimo）已移出分母；若把 13 當分母，實際觀測覆蓋率仍以
> 表列分子（如 2/13 emit）為準——「0 gap」不代表 13 provider 都在被監控。

**K0 結構性降級口徑 (R182 決議, R197 落地)**:

| 子指標 | 本機可達 | OpenAB scope (受 cicx 等浮動) | 永久非本機 scope (永久 skip) | 結構性差距 |
|---|---:|---:|---:|---:|
| K0-A1 emit 覆蓋 | 2/13 (claude/codex live emit；copilot/gemini 目前無 runner snapshot, R212 修正不造假) | 5/13 (cicx/gitx/giminix/codex_bot/openx 受 OpenAB 端 bot 上下線浮動) | **4/13** 永久非本機 scope (irisx_bot/grokx/lpbot/mimo 完全不寫 snapshot, OpenAB 端永遠不可達) | **0 結構性差距** (2/13 本機當前真實值; OpenAB 受 cicx 端浮動; 4/13 永久 skip 移出 K0 量化) |
| K0-A2 sample 覆蓋 | 1/13 (claude=3 sessions 累加, R132 對齊) | 4/13 (5 active OpenAB 中 4 個 = cicx/gitx/giminix/codex_bot 屬受 bot 是否在運作浮動, openx 屬 legacy alias) | **4/13** 永久非本機 scope (同 K0-A1, 不會有事件流過) | **0 結構性差距** (1/13 本機可控 100% 達標, 8/13 OpenAB 端 5 active 受 cicx 等浮動 + 4 永久 skip) |
| K0 程式碼定義層 (R101) | 13/13 (R101 達標, 跟 K0-A1/A2 量化口徑解耦) | — | — | 達標 |
| K0 Quota 監控即時性 | K0-B fresh 2/13 (usage-local.json 目前實際 runners: claude/codex；copilot/gemini 缺 runner 不算 fresh) | K0-Q 5/13 (5 active OpenAB stale snapshot 有 data path, 但非 fresh) | **K0-Q 4/13** 永久非本機 scope (永久 skip, 移出 K0 量化) | **0 結構性差距** (4 missing = OpenAB 端不寫 `usage-*.json` 永久 skip, 移出 K0 量化; OpenAB 端 scope 由 OpenAB 端 owner 自追, 不計入 LobsterPulse K0) |

**決議文字 (R182 → R197 寫入 MISSION)**:
- K0 目標從 13/13 全 scope 改為 **本機當前 2/13 + OpenAB 5/13 + 4 missing 永久非本機 scope 雙軌制**；本機數值以 `usage-local.json` 實際 `runners[].name` 為準, 不再由檔案存在推論 4/13
- 4 missing (irisx_bot/grokx/lpbot/mimo) 明確標註為 **永久非本機 scope**, 移出 K0 量化口徑
- 5 active OpenAB (cicx/gitx/giminix/codex_bot/openx) 仍受 OpenAB 端 bot 上下線浮動影響, 屬 OpenAB 端 owner 自追 scope
- 4 本機 CLI (claude/codex/copilot/gemini) 程式碼定義仍在, 不可被誤降為「永久非 scope」；但 K0-B fresh 只計實際存在的 runner snapshot
- R182 接力順位 #1 (K0 Quota 4 missing 補鏈路) → **永久 skip** (R182 決議移出 K0 量化)
- R182 接力順位 #2 (K0-A1 emit 4/13 → 5/13 護衛) → **永久 skip** (5/13 受 OpenAB cicx 端浮動, 不再列為 K0 量化)
- R182 接力順位 #5 (R175-R180 transparent 透明化軸延伸) → **unblock** (結構性失靈真因 = 結構性死結, 死結已解)

**護衛**: `scripts/k0_target_baseline_check.py` + pytest 5 case 守住 R182 結構性決議不退
(5 維度: KNOWN_PROVIDERS 結構 / 4 missing 永久非 scope / 5 active OpenAB 不退 / 4 LOCAL_CLI 不可
永久 skip / MISSION R182 補欄 + 4 missing + 永久非 scope 標記不退)。

**未選 Path B 原因**: 1 sprint 工作量 (~500 行 Rust + 9 handler + 護衛), owner M capacity
未確認, 結構性風險 > 結構性收益 (Path A 對已宣告可達 scope 9/13 已能 0 結構性差距達標——非對外宣稱的 13 provider 全數, Path B 的 4 missing
unblock 收益不抵 sprint 級投入)。

### M1 補 (2026-07-05): otel-genai Phase 2/3 落地 closure

- `otel-genai-runtime-emit-2026-q3` T-OGRE10~16 全 7 task owner M M1 接力落地
  (OTel 0.31 SDK + `telemetry.rs` + `start_otlp_exporter` command + SessionManager
  4 事件點 emit span + 13 條 provider mapping + `telemetry::tests` 護衛 mod +
  .gitignore 護衛), change 16/16 closed。
- K40 量測口徑: 9/10 closed + 1 active (唯一 active = mission-k0 Path B 6 task +
  Phase 4 placeholder, R197 Path A 決議已取代, owner M 未選不開工)。
- K42 護衛 chain 20 → **21** (`telemetry::tests` 走 R97 後 +4 例外, 架構理由 =
  跨 session.rs ↔ hook_server.rs ↔ telemetry.rs 3 mod 邊界)。cargo test --lib
  baseline 452 → **470** (460 既有 + 10 新增) 全綠。

任一指標連 2 週落後 → 觸發策略重審（不是「再補一輪」）。

---

## Provider 納入 / 淘汰標準

### 納入（新加 provider 必須全中）

1. **單一 contract** — 必須用同一個 `HookEvent` schema 接入，禁止每個 bot 自帶一套
2. **可量測** — 必須能 emit 成功率 / 延遲 / token / quota 至少 3 項 metric
3. **有維護者** — 至少有 1 個 commit 維護者 or 自動 sync SOP（owner 簽認）
4. **使用情境真實** — 不是為了湊數；至少 1 個實際工作流在用
5. **spec 先行** — `openspec/changes/<name>/proposal.md` 寫完才能寫 code

### 淘汰（任一條件持續 30 天成立即砍）

1. **無事件** — 30 天內 0 個 hook event 進來
2. **維護者缺席** — 60 天無 commit、PR 沒人接
3. **metric 漂移** — P95 延遲連 7 日 > 基準值 2x 且無降級計畫
4. **quota 監控失效** — 連 14 天 `usage-*.json` 沒更新或讀不到
5. **spec drift** — 規格已標記 drift 30 天未修

淘汰流程：開 `openspec/changes/deprecate-<name>/` → 30 天 deprecation window → 砍。

---

## 方向決策規則（防 DRIFTING）

每次考慮「加一個新東西」時，跑這 3 個測試：

1. **對齊單一 contract 嗎？** — 不對齊 = 拒（標準化是策略顧問 #2 行動）
2. **能推進上面 5 個 KPI 嗎？** — 不能 = 拒（純治理批次不算）
3. **有量化退場標準嗎？** — 沒有 = 拒（避免 5 年後累積 50 個 bot 無人砍）

如果 3 個都通過 → 可以做。否則 → 留 `openspec/changes/` 提案區，等 90 天後再議。

---

## 與 CLAUDE.md / engineering-log 的關係

- `CLAUDE.md`：專案事實（端點 / plugin / view / 設計決策）— **HOW**
- `engineering-log.md`：每輪實驗紀錄 — **WHEN/WHAT**
- `MISSION.md`（本檔）：策略決策錨點 — **WHY**

三者缺一不可。R82+ owner 決策時，先讀本檔再讀 CLAUDE.md。

---

**補頁者**: R81
**歷史補頁歸檔**: [`docs/kpi-history.md`](kpi-history.md)（R132 拆出去，恢復 MISSION 決策可讀性）
**KPI-impact**: K-Foundation +1（90 天量化退場標準從無到有）
**驗收週期**: 2026-09-04
