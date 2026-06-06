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
> **R109 補**：本機 CLI 段 4/4 滿覆蓋（copilot live quota 落地）；
> 缺 4 個 OpenAB bot（`irisx_bot`/`grokx`/`lpbot`/`mimo` 完全 missing — 仍非本機 scope）。
> **R114 補**：k0_measure.py openx legacy alias 修，`usage-bot.json` 終於被認到（修後
> K0-Q 8/13 → 9/13，+1 從 openx alias 修：openx 從永遠 missing 變可計入 stale bucket，
> 對齊 MISSION 13/13 目標口徑「snapshot 存在」即算 data path 接上）。
>
> **R111 補**：k0_measure 端點復活 (main app 跑起來 `/metrics` 200 OK) → K0-A1 emit
> 從 R108 0/13 進步到 **5/13** (claude/codex/copilot/gemini/cicx 5 label 端點實際 emit，
> __local__ 是 internal label 不算)，K0-A2 sample **2/13** (claude=11 + cicx=1 真有
> sessions 累加)。端點 DOWN (R108) ≠ emit 邏輯壞：純粹是 main app 沒跑沒在 emit，現
> R111 端點活著就復活。剩 11 個 provider 需事件流過 (cicx=1, claude=11, 其他 0) —
> **非本機 scope**，需 OpenAB 端跑起來才有 K0-A1/A2 13/13 真正達成。K0 Quota K0-Q
> 9/13 持平 R114 (4 fresh + 5 stale, 4 missing 仍 irisx_bot/grokx/lpbot/mimo 寫
> snapshot，非本機 scope)。R115 lobster-rules-engine spec closure (R116 接力) 進
> closed 集，K40 6/6 → 7/7。

| KPI | R81 baseline（前值） | R108 量測現況 | R109 補 | R111 補 (端點復活) | 驗收差距 |
|---|---:|---:|---:|---:|---:|
| K0-A1 emit 覆蓋 | 0/13 | 0/13 (endpoint DOWN, 未跑 build) | 0/13 (endpoint 仍 DOWN) | **5/13** (endpoint UP, 5 provider labels 端點實際 emit: claude/codex/copilot/gemini/cicx) | 缺 8 (5 emit 但 0 sessions, 距 13/13 sample 級距仍差 8) |
| K0-A2 sample 覆蓋 | 0/13 | 0/13 (endpoint DOWN) | 0/13 (endpoint 仍 DOWN) | **2/13** (claude=11 + cicx=1 真有 sessions) | 缺 11 (非本機 scope, 需 OpenAB 端跑起來) |
| K0 程式碼定義層 (R101) | 0/13 | 13/13 (R101 達標) | 13/13 (守住) | 13/13 (守住) | 達標 |
| K0 Quota 監控即時性 | 6 OpenAB snapshot；本機無 | **K0-B fresh 4/13 + K0-Q 8/13** | **K0-B fresh 4/13 + K0-Q 9/13** (R114 修 openx alias: openx 從 missing 變 stale, +1) | **K0-B fresh 4/13 + K0-Q 9/13** 持平 R114 (4 missing: irisx_bot/grokx/lpbot/mimo 非本機 scope) | 缺 4 (irisx_bot/grokx/lpbot/mimo 完全 missing) |
| K40 規格覆蓋率 | 1/1 (openab-bot-sync 12/12) | 5/5 active change 全 closed (43/43 tasks) | 5/5 持續 closed | **7/7** (R116 R115 lobster-rules-engine closure 接力) | 達標 |
| K41 chore_treadmill 24h | 55% | **R108 k41_chore_treadmill.py 7d: 13/206 = 6.3%** | 達標延續 | 達標延續 | 達標 (<30%) |
| K42 護衛 chain | 17 條 | 17 條 (R113.1 owner M dual-emit value guard 提案中) | 17 條 (R114 落地 dual-emit value guard 進既有 `render_prometheus_tests` mod, chain 17→17 不擴張守住) | 17 條 持平 (R115 護衛 test 三條加進既 `auto_rules::tests` mod, 走既有 mod 17→17) | 達標 (守住) |

**R108+R109+R114+R111 量化結論**：
- K0 Quota 距 13/13 目標缺 4 (R108 4 個, R109 補無變, R114 修 openx alias +1 但仍缺 4 個, R111 持平)
  — 缺 OpenAB `irisx_bot`/`grokx`/`lpbot`/`mimo` 寫 snapshot，**非本機 scope**
- K0-A1 emit 0/13 (R108/R109) → 5/13 (R111 端點復活) — 端點 DOWN (R108) ≠ emit 邏輯壞, main app 跑就 5 label 端點 emit
- K0-A2 sample 0/13 (R108/R109) → 2/13 (R111 claude=11 + cicx=1) — 距 13/13 仍缺 11, **非本機 scope** (需 OpenAB 端跑起來)
- 本機 CLI 段 K0 Quota 100% 滿覆蓋（claude R85 / codex R86 / gemini R108 / copilot R109 — 4/4）
- 5 個文件/治理級 KPI 全綠 — supervisor 報的「drift」是 **文件 vs 量測分叉**，非 KPI 倒退
- 下個 M1 候選：R115+ 接力 K0 Quota 4 missing 補鏈路（OpenAB scope）+ R116 接力 R115 護衛 chain 17→17 守住 (走既有 mod, 不擴張) + R112 Capsule Brief 樣式已落地, JS 配套等 owner M 收 R117+

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
**KPI-impact**: K-Foundation +1（90 天量化退場標準從無到有）
**驗收週期**: 2026-09-04
