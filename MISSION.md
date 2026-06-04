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
| **K0 Provider 健康度覆蓋率** | 0/13 provider 有 P95 延遲 + 成功率指標 | 13/13 | Prometheus exporter 對應 metric 是否存在且有非零樣本 |
| **K0 Quota 監控即時性** | 6 個 OpenAB bot 有 snapshot；本機 CLI 無 quota 監控 | 13/13 provider 都有 | `usage-*.json` 或等價 metric 是否被讀到 |
| **K40 規格覆蓋率** | 1/1 active change (openab-bot-sync) 12/12 落地 | 100% 落地、0 漂移 | `spectra validate --changes <name>` 通過 + tasks.md 12/12 |
| **K41 chore_treadmill 紅線** | 24h 55% 觸發 | <30% 持續 7 日 | `git log --since='7d' --pretty=format:'%s' \| grep -c '^chore' / total < 0.30` |
| **K42 護欄 chain 飽和** | 17 條 saturated | 守住 17，不過度擴張 | guard test 全綠 + 新增需有架構變更理由 |

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
