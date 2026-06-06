# Proposal: Cross-Provider Timeline View (M0 spec 提案)

## Goal

開 LobsterPulse **第 6 視圖** — Cross-Provider Timeline：

> **24h 橫向時間軸，13 provider 同框顯示「過去 24 小時每個 provider 的狀態分布」**

把 5 views 從「**當下 snapshot**」進化成「**歷史連續 strip**」, 補上 MISSION 北極星
「0 切換成本」缺的最後一塊 — **時間維度**。

### 為什麼是 wow

1. **現有 5 views 全部缺時間維度** — 膠囊 (300×46) 只給當下, 展開面板是 list,
   Bot 總覽是 9+4 卡片, 事件診斷是 filter tabs, 設定是 config。**沒有任何 view 講
   「過去」**。
2. **下班回來不記得 agent 跑過啥** — 開發者最常見痛點：早上開機、晚上收工、想
   「我今天/昨天到底讓幾個 agent 跑、燒多少 token、哪個最會卡住」 — 現有 5 views
   全都答不出。
3. **13 provider 跨比較 = 0 切換** — 9 OpenAB bot + 4 本機 CLI 同框 24h strip,
   1 眼看出「cicx 下午 2 點爆量、claude 整天穩定、grokx 早上沒動」。這是
   MISSION 北極星「單一膠囊統一監控」**時間軸化**。
4. **競品沒做** — Token Telemetry 走 port 3000 web dashboard + cost/reasoning,
   tokenusage 純 token 計量。**桌面常駐 + 時間軸 + 多 provider 同框** = 沒人做
   (對齊 CLAUDE.md 競品備忘 3 條邊界)。
5. **補進既有資料流** — SessionManager 已經有 provider 集合 + state 累加, 加 1
   個 ring buffer (e.g. 24h × 13 provider × 1 分鐘解析度 = 18,720 cell) 即可, 不
   需新外部資料源。**真正用既有資料出 wow**, 不做 spec theater。

## Background

### 5 rounds 連 M0 closure 死循環 (R101-R116)

- R101-R106 連 6 輪 M0 spec closure / 護衛
- R108/R109 接力 2 輪 M1 突破 (K0 Quota 6→9/13)
- R114 dual-emit value guard (M0) + R114 R114-2/3 接力 (K0 推進 + refactor)
- R115 lobster-rules-engine spec closure (M0)
- R116 R115 接力 + R111 MISSION spec 對齊

**KPI 結果**: K0 Quota 9/13 持平 2 輪, K0-A1 5/13, K0-A2 2/13。

5 輪「無改善」表象下, 真正原因:
- 4 個 missing K0 Quota (irisx_bot/grokx/lpbot/mimo) = **非本機 scope** (需 OpenAB
  端 snapshot 寫入鏈路, Claude 端不可 ship)
- K0-A1 5→13, K0-A2 2→13 = 需事件流過, **非本機 scope**
- R107+ prometheus-counter-rename 5-week timeline = **owner 級 follow-up**
- 護衛 chain 17 條飽和 = **不擴張契約**

### Wow 提案的 source

R100 策略顧問 (2026-06-04) #3 行動 closure (R105) 結論:

> 「單一膠囊＋多 runtime 狀態, 而不是只算 token」

Timeline 是這個結論的**時間延伸** — 把「多 runtime 狀態」從「當下」拉到「24h
連續」, 仍是「單一視圖」哲學 (新第 6 視圖, 不取代膠囊、不破 web dashboard 邊界)。

### Past 視圖演進的時間軸

| Round | 視圖 | 類型 |
|---|---|---|
| R0 | 膠囊 | 當下 |
| R10 | 展開面板 | 當下 list |
| R40 | Bot 總覽 | 當下 9+4 卡片 |
| R60 | 事件診斷 | 當下 14 filter tabs |
| R70 | 設定 | config |
| **R117** | **Timeline** | **歷史 24h strip** |

## Scope

### In Scope (本 change M0)

- 開新 `openspec/changes/cross-provider-timeline/` change 資料夾
- 寫 4 個 change-level spec 檔: `proposal.md` / `design.md` / `tasks.md` /
  `.openspec.yaml`
- 寫 1 個 capability spec: `specs/cross-provider-timeline/spec.md`
- 定義 Timeline 資料模型 (24h × 13 provider × 1-min cell = 18,720 cell ring buffer)
- 定義 Timeline 視圖 UX (1 strip + 1 區域 zoom + 1 quick-jump 24h/7d/all-time toggle)
- 定義 Timeline 對齊既有 K0 護衛 (不破 K42 chain 17 條飽和契約)
- 對齊 MISSION 北極星 3 條 (單一膠囊 / 真實任務狀態 / 0 切換)

### Out of Scope (本 change M0 明確不做)

- **不寫 code** — M0 spec-only, 1 輪 1 件紀律守住
- **不動 SessionManager** — ring buffer 新增是 R118+ M1 範疇
- **不動 main.js** — Timeline JS 渲染是 R118+ M1
- **不動 owner M 11 髒檔** (R13 防護) — docs/index.html, docs/styles.css,
  src/styles.css, src-tauri/Cargo.toml + 5 untracked + 2 bash crash dump
- **不做 all-time archive** — 24h 短期 + 7d 縮圖; 永久 archive 留 R119+
- **不做 cost heatmap** — 對齊 CLAUDE.md 競品邊界「不做 cost anomaly detection」,
  Timeline 顯示 state distribution, 不顯示 cost
- **不做 cross-session correlation** (e.g.「cicx 死後 5 分鐘 claude 也死」) —
  留 R119+ M1
- **不做 Timeline 編輯 / 標註** (e.g.「我覺得這段 cicx 不該這麼久」) — 留 R120+
- **不做 Timeline export** (PNG / CSV / share link) — 留 R120+
- **不做 Timeline alert 規則** (e.g.「連續 10 分鐘所有 provider 都 Stale 觸發
  notification」) — 跟 R115 lobster-rules-engine 接力避免雙 spec 衝突
- **不接 OTel SDK** — Timeline emit 用既有 K0-A1/A2 metric 即可, 不開新 OTel 維度
- **不動 6 counter rename 廣播時程** — R107+ owner follow-up, 本 change 不碰

## Capabilities

- `cross-provider-timeline` — LobsterPulse 第 6 視圖, 24h × 13 provider
  ring-buffer time-series 視覺化, 0 切換看出歷史 activity distribution, 對齊
  既有 5 views (膠囊 / 展開面板 / Bot 總覽 / 事件診斷 / 設定), 不取代, 補時間
  維度。

## 對齊 MISSION 量化值

| MISSION KPI | 本 change 影響 |
|---|---|
| K0-A1 端點 emit 5/13 | 不動 (本 change spec-only) |
| K0-A2 sample 2/13 | 不動 |
| K0 Quota 9/13 | 不動 (Timeline 用既有 snapshot, 不開新 data path) |
| K40 規格覆蓋率 7/7 | **+1 → 8/8** (R117 cross-provider-timeline closure) |
| K41 chore_treadmill <30% | 守住 (M0 spec-only, 0 chore commit) |
| K42 護衛 chain 17 條 | **守住 17, 不擴張** (M0 不加 test) |
| **新 K-Metric 提案** | **K43 Timeline 視圖使用率** (e.g. 7d 內開啟次數 / 24h 內 hover 互動次數) — R118+ M1 補 |

## 對齊 CLAUDE.md 競品備忘 3 條邊界

| 邊界 | 守住方式 |
|---|---|
| 不做 cloud dashboard | Timeline 是本機 Tauri 視圖, 不開 port, 不接 server |
| 不做純 token 計量 | Timeline 顯示 state distribution (Working/Idle/Waiting/Stale), 不顯示 token/cost 數字 (留 R120+ optional toggle) |
| 不做純 log reader | Timeline 用 SessionManager 即時累加, 不是讀 log file; emit task-completed/waiting 同步觸發 hover detail |
